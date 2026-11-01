//! Files symphonia has no decoder for, played by Windows itself (0.8.0):
//! Media Foundation's source reader decodes them to float samples, and the
//! shell's property system reads their tags. That's WMA above all (Windows
//! Media Audio, from many an old CD rip), and any file symphonia turns away
//! that Windows can still play.

use std::path::Path;
use std::sync::OnceLock;

use windows::core::{GUID, HSTRING};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFSample, IMFSourceReader, MFAudioFormat_Float, MFCreateMediaType,
    MFCreateSourceReaderFromURL, MFMediaType_Audio, MFStartup, MFSTARTUP_LITE, MF_MT_AUDIO_NUM_CHANNELS,
    MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_PD_DURATION,
    MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED, MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READER_ALL_STREAMS,
    MF_SOURCE_READER_FIRST_AUDIO_STREAM, MF_SOURCE_READER_MEDIASOURCE, MF_VERSION,
};
use windows::Win32::Storage::EnhancedStorage::{
    PKEY_Audio_EncodingBitrate, PKEY_Audio_SampleRate, PKEY_Media_Duration, PKEY_Music_AlbumArtist,
    PKEY_Music_AlbumTitle, PKEY_Music_Artist, PKEY_Title,
};
use windows::Win32::System::Com::StructuredStorage::{
    PropVariantClear, PropVariantToStringAlloc, PropVariantToUInt32, PropVariantToUInt64, PROPVARIANT,
};
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_MULTITHREADED};
use windows::Win32::System::Variant::VT_I8;
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreFromParsingName, GPS_DEFAULT};

use crate::decoding::Next;

const AUDIO: u32 = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;

/// COM on this thread (both kinds work for what's used here; a thread that
/// already has the other kind keeps it) and Media Foundation once per run.
fn ready() -> Result<(), String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    static STARTED: OnceLock<Result<(), String>> = OnceLock::new();
    STARTED
        .get_or_init(|| unsafe { MFStartup(MF_VERSION, MFSTARTUP_LITE) }.map_err(|e| format!("Media Foundation: {e}")))
        .clone()
}

/// A file Media Foundation decodes, as float samples.
pub(crate) struct MfSource {
    reader: IMFSourceReader,
    rate: u32,
    channels: usize,
}

impl MfSource {
    /// Open `path` and its length in ms (0 if it doesn't say).
    pub(crate) fn open(path: &Path) -> Result<(MfSource, u32), String> {
        ready()?;
        let url = HSTRING::from(path.as_os_str());
        let reader = unsafe { MFCreateSourceReaderFromURL(&url, None::<&IMFAttributes>) }
            .map_err(|e| format!("unsupported: Windows can't open it ({})", e.message()))?;
        unsafe {
            let _ = reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false);
            reader.SetStreamSelection(AUDIO, true).map_err(|_| "no audio track".to_string())?;
            let float = MFCreateMediaType().map_err(|e| e.message())?;
            float.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).map_err(|e| e.message())?;
            float.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_Float).map_err(|e| e.message())?;
            reader
                .SetCurrentMediaType(AUDIO, None, &float)
                .map_err(|e| format!("unsupported: Windows has no decoder for it ({})", e.message()))?;
        }
        let mut source = MfSource { reader, rate: 0, channels: 0 };
        source.read_format()?;
        let duration_ms = unsafe {
            source
                .reader
                .GetPresentationAttribute(MF_SOURCE_READER_MEDIASOURCE.0 as u32, &MF_PD_DURATION)
                .ok()
                .and_then(|mut pv| {
                    let v = PropVariantToUInt64(&pv).ok();
                    let _ = PropVariantClear(&mut pv);
                    v
                })
                .map_or(0, |hns| (hns / 10_000) as u32)
        };
        Ok((source, duration_ms))
    }

    /// The rate and channels the decoder hands out (asked again whenever
    /// the reader says they changed).
    fn read_format(&mut self) -> Result<(), String> {
        let current = unsafe { self.reader.GetCurrentMediaType(AUDIO) }.map_err(|e| e.message())?;
        let rate = unsafe { current.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND) }.unwrap_or(0);
        let channels = unsafe { current.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS) }.unwrap_or(0) as usize;
        if rate == 0 || channels == 0 {
            return Err("unsupported: no sound format Spotiamp+ can play".into());
        }
        (self.rate, self.channels) = (rate, channels);
        Ok(())
    }

    /// The next piece of sound into `out` (already cleared).
    pub(crate) fn next(&mut self, out: &mut Vec<f32>) -> Next {
        // the reader can hand out empty ticks; a run of them that never ends
        // is treated as the end rather than a hang
        for _ in 0..1000 {
            let mut flags = 0u32;
            let mut sample: Option<IMFSample> = None;
            let read = unsafe { self.reader.ReadSample(AUDIO, 0, None, Some(&mut flags), None, Some(&mut sample)) };
            if read.is_err() || flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                return Next::End;
            }
            if flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0 && self.read_format().is_err() {
                return Next::End;
            }
            let Some(sample) = sample else { continue };
            let Ok(buffer) = (unsafe { sample.ConvertToContiguousBuffer() }) else { return Next::Skip };
            let mut data = std::ptr::null_mut();
            let mut len = 0u32;
            if unsafe { buffer.Lock(&mut data, None, Some(&mut len)) }.is_err() {
                return Next::Skip;
            }
            let floats = unsafe { std::slice::from_raw_parts(data as *const f32, len as usize / 4) };
            out.extend_from_slice(floats);
            let _ = unsafe { buffer.Unlock() };
            if out.is_empty() {
                continue;
            }
            return Next::Audio { rate: self.rate, channels: self.channels };
        }
        Next::End
    }

    /// To `ms` from the start.
    pub(crate) fn seek(&mut self, ms: u32) {
        let mut at = PROPVARIANT::default();
        unsafe {
            let inner = &mut *at.Anonymous.Anonymous;
            inner.vt = VT_I8;
            inner.Anonymous.hVal = ms as i64 * 10_000; // in 100 ns
            // GUID_NULL: the position is in 100 ns units
            let _ = self.reader.SetCurrentPosition(&GUID::zeroed(), &at);
        }
    }
}

/// What the shell knows about a file: its tags, length, rate and bitrate.
#[derive(Default)]
pub(crate) struct ShellTags {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u32,
    pub sample_rate: u32,
    pub kbps: u32,
}

pub(crate) fn read_tags(path: &Path) -> Option<ShellTags> {
    ready().ok()?;
    let url = HSTRING::from(path.as_os_str());
    let store: IPropertyStore = unsafe { SHGetPropertyStoreFromParsingName(&url, None, GPS_DEFAULT) }.ok()?;
    let value = |key: &PROPERTYKEY| unsafe { store.GetValue(key).ok() };
    let text = |key: &PROPERTYKEY| -> String {
        let Some(mut pv) = value(key) else { return String::new() };
        let s = unsafe { PropVariantToStringAlloc(&pv) }.ok().map(|p| {
            let s = unsafe { p.to_string() }.unwrap_or_default();
            unsafe { CoTaskMemFree(Some(p.0 as _)) };
            s
        });
        let _ = unsafe { PropVariantClear(&mut pv) };
        s.unwrap_or_default().trim().to_string()
    };
    let number = |key: &PROPERTYKEY| -> u64 {
        let Some(mut pv) = value(key) else { return 0 };
        let n = unsafe { PropVariantToUInt64(&pv).or_else(|_| PropVariantToUInt32(&pv).map(u64::from)) }.unwrap_or(0);
        let _ = unsafe { PropVariantClear(&mut pv) };
        n
    };
    let mut artist = text(&PKEY_Music_Artist);
    if artist.is_empty() {
        artist = text(&PKEY_Music_AlbumArtist);
    }
    Some(ShellTags {
        title: text(&PKEY_Title),
        artist,
        album: text(&PKEY_Music_AlbumTitle),
        duration_ms: (number(&PKEY_Media_Duration) / 10_000) as u32,
        sample_rate: number(&PKEY_Audio_SampleRate) as u32,
        kbps: (number(&PKEY_Audio_EncodingBitrate) / 1000) as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::core::Interface;
    use windows::Win32::Media::MediaFoundation::{
        IMFMediaType, MFAudioFormat_PCM, MFAudioFormat_WMAudioV8, MFCreateMemoryBuffer, MFCreateSample,
        MFCreateSinkWriterFromURL, MFTranscodeGetAudioOutputAvailableTypes, MFT_ENUM_FLAG_ALL,
        MF_MT_AUDIO_AVG_BYTES_PER_SECOND, MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_BLOCK_ALIGNMENT,
    };

    /// `pieces` tenths of a second of a 440 Hz tone, 44.1 kHz stereo, as a
    /// WMA file written by Windows' own encoder; None where Windows has none
    /// (some servers).
    fn write_wma(path: &Path, pieces: usize) -> Option<()> {
        ready().ok()?;
        unsafe {
            let types =
                MFTranscodeGetAudioOutputAvailableTypes(&MFAudioFormat_WMAudioV8, MFT_ENUM_FLAG_ALL.0 as u32, None).ok()?;
            let target = (0..types.GetElementCount().ok()?).find_map(|i| {
                let t: IMFMediaType = types.GetElement(i).ok()?.cast().ok()?;
                let fits = t.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND).ok()? == 44_100
                    && t.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).ok()? == 2;
                fits.then_some(t)
            })?;
            let writer =
                MFCreateSinkWriterFromURL(&HSTRING::from(path.as_os_str()), None, None::<&IMFAttributes>).ok()?;
            let stream = writer.AddStream(&target).ok()?;
            let pcm = MFCreateMediaType().ok()?;
            pcm.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).ok()?;
            pcm.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM).ok()?;
            pcm.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2).ok()?;
            pcm.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, 44_100).ok()?;
            pcm.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16).ok()?;
            pcm.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, 4).ok()?;
            pcm.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 44_100 * 4).ok()?;
            writer.SetInputMediaType(stream, &pcm, None::<&IMFAttributes>).ok()?;
            writer.BeginWriting().ok()?;
            const FRAMES: usize = 4410; // 0.1 s a piece
            for piece in 0..pieces {
                let buffer = MFCreateMemoryBuffer((FRAMES * 4) as u32).ok()?;
                let mut data = std::ptr::null_mut();
                buffer.Lock(&mut data, None, None).ok()?;
                let samples = std::slice::from_raw_parts_mut(data as *mut i16, FRAMES * 2);
                for f in 0..FRAMES {
                    let t = (piece * FRAMES + f) as f32 / 44_100.0;
                    let v = ((t * 440.0 * std::f32::consts::TAU).sin() * 8000.0) as i16;
                    (samples[2 * f], samples[2 * f + 1]) = (v, v);
                }
                buffer.Unlock().ok()?;
                buffer.SetCurrentLength((FRAMES * 4) as u32).ok()?;
                let sample = MFCreateSample().ok()?;
                sample.AddBuffer(&buffer).ok()?;
                sample.SetSampleTime(piece as i64 * 1_000_000).ok()?;
                sample.SetSampleDuration(1_000_000).ok()?;
                writer.WriteSample(stream, &sample).ok()?;
            }
            writer.Finalize().ok()?;
        }
        Some(())
    }

    /// Seconds of sound left in `source`, checking its rate and channels.
    fn seconds_left(source: &mut MfSource) -> f64 {
        let (mut out, mut frames) = (Vec::new(), 0usize);
        loop {
            out.clear();
            match source.next(&mut out) {
                Next::Audio { rate, channels } => {
                    assert_eq!((rate, channels), (44_100, 2));
                    frames += out.len() / channels;
                }
                Next::Skip => {}
                Next::End => return frames as f64 / 44_100.0,
            }
        }
    }

    #[test]
    fn a_wma_plays_seeks_and_has_a_length() {
        let dir = std::env::temp_dir().join(format!("spotiamp-wma-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tone.wma");
        if write_wma(&path, 20).is_none() {
            eprintln!("no WMA encoder on this Windows: skipped");
            return;
        }
        let (mut source, ms) = MfSource::open(&path).expect("Windows opens its own WMA");
        assert!((1900..=2200).contains(&ms), "length {ms} ms");
        let all = seconds_left(&mut source);
        assert!((1.9..2.3).contains(&all), "{all} s of sound");
        source.seek(1500);
        let rest = seconds_left(&mut source);
        assert!((0.3..0.8).contains(&rest), "{rest} s after a seek to 1.5 s");
        let tags = read_tags(&path).expect("the shell reads it");
        assert!((1900..=2200).contains(&tags.duration_ms), "shell length {} ms", tags.duration_ms);
        drop(source);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A 12 s WMA file for the app's own test (.smoke/t-080.sh), written
    /// where SPOTIAMP_WMA_OUT says.
    #[test]
    #[ignore]
    fn write_a_wma_for_the_smoke_test() {
        let out = std::env::var("SPOTIAMP_WMA_OUT").expect("SPOTIAMP_WMA_OUT");
        write_wma(Path::new(&out), 120).expect("Windows' WMA encoder");
    }
}
