//! What turns a file's or a station's bytes into sound (0.7.8). AAC goes to
//! rusty_aac, which decodes HE-AAC whole: its high band (SBR) and its stereo
//! (PS). symphonia's AAC decoder plays only the core, so an AAC+ station came
//! out at 22 kHz, dull, and some even in mono. Everything else, and AAC that
//! rusty_aac won't take, stays with symphonia.

use std::io::Read;
use std::path::PathBuf;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_AAC};
use symphonia::core::formats::{FormatReader, SeekMode, SeekTo, Track};
use symphonia::core::units::Time;

/// What the next piece of sound was.
pub(crate) enum Next {
    /// Interleaved samples at this rate and channel count are in `out`.
    Audio { rate: u32, channels: usize },
    /// A piece that wouldn't decode: carry on with the next.
    Skip,
    /// Nothing more.
    End,
}

pub(crate) enum Source {
    /// symphonia reads and decodes.
    Symphonia {
        format: Box<dyn FormatReader>,
        decoder: Box<dyn symphonia::core::codecs::Decoder>,
        track_id: u32,
        buf: Option<SampleBuffer<f32>>,
    },
    /// AAC in an MP4 / M4A: symphonia reads, rusty_aac decodes.
    Mp4Aac {
        format: Box<dyn FormatReader>,
        track_id: u32,
        /// The container's AudioSpecificConfig, to start again after a seek.
        config: Vec<u8>,
        decoder: rusty_aac::AacDecoder,
    },
    /// AAC in ADTS frames (a station, an .aac file): rusty_aac takes them
    /// whole, headers and all.
    Adts { frames: AdtsFrames, decoder: rusty_aac::AacDecoder },
}

impl Source {
    /// ADTS AAC from `reader` (`file`: where it is, to seek in it).
    pub(crate) fn adts(reader: Box<dyn Read + Send>, file: Option<PathBuf>) -> Source {
        Source::Adts { frames: AdtsFrames::new(reader, file), decoder: rusty_aac::AacDecoder::new() }
    }

    /// The probed `track` of `format`: rusty_aac for AAC with its config,
    /// symphonia for the rest.
    pub(crate) fn probed(format: Box<dyn FormatReader>, track: &Track) -> Result<Source, String> {
        let params = &track.codec_params;
        if params.codec == CODEC_TYPE_AAC
            && let Some(config) = params.extra_data.as_deref()
            && let Ok(decoder) = rusty_aac::AacDecoder::with_config_bytes(config)
        {
            return Ok(Source::Mp4Aac { format, track_id: track.id, config: config.to_vec(), decoder });
        }
        let decoder = symphonia::default::get_codecs()
            .make(params, &DecoderOptions::default())
            .map_err(|e| format!("no decoder: {e}"))?;
        Ok(Source::Symphonia { format, decoder, track_id: track.id, buf: None })
    }

    /// ADTS: whether a frame comes at all (a station that says AAC but
    /// sends something else, a .aac file that isn't). Nothing is used up.
    pub(crate) fn finds_sound(&mut self) -> bool {
        match self {
            Source::Adts { frames, .. } => frames.next_header().is_some(),
            _ => true,
        }
    }

    /// Who decodes, for the log.
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Source::Symphonia { .. } => "symphonia",
            Source::Mp4Aac { .. } => "AAC in MP4, rusty_aac",
            Source::Adts { .. } => "AAC (ADTS), rusty_aac",
        }
    }

    /// Decode the next piece into `out` (cleared first).
    pub(crate) fn next(&mut self, out: &mut Vec<f32>) -> Next {
        out.clear();
        match self {
            Source::Symphonia { format, decoder, track_id, buf } => loop {
                let Ok(packet) = format.next_packet() else { return Next::End };
                if packet.track_id() != *track_id {
                    continue;
                }
                return match decoder.decode(&packet) {
                    Ok(decoded) => {
                        let spec = *decoded.spec();
                        let b = buf.get_or_insert_with(|| SampleBuffer::new(decoded.capacity() as u64, spec));
                        b.copy_interleaved_ref(decoded);
                        out.extend_from_slice(b.samples());
                        Next::Audio { rate: spec.rate, channels: spec.channels.count() }
                    }
                    Err(symphonia::core::errors::Error::DecodeError(_)) => Next::Skip,
                    Err(_) => Next::End,
                };
            },
            Source::Mp4Aac { format, track_id, decoder, .. } => loop {
                let Ok(packet) = format.next_packet() else { return Next::End };
                if packet.track_id() != *track_id {
                    continue;
                }
                return aac(decoder.decode(&packet.data, None), out);
            },
            Source::Adts { frames, decoder } => match frames.next_frame() {
                Some(frame) => aac(decoder.decode(&frame, None), out),
                None => Next::End,
            },
        }
    }

    /// To `ms` from the start (a station can't: nothing happens).
    pub(crate) fn seek(&mut self, ms: u32) {
        let to = |track_id: u32| SeekTo::Time { time: Time::from(ms as f64 / 1000.0), track_id: Some(track_id) };
        match self {
            Source::Symphonia { format, decoder, track_id, .. } => {
                let _ = format.seek(SeekMode::Accurate, to(*track_id));
                decoder.reset();
            }
            Source::Mp4Aac { format, track_id, config, decoder } => {
                let _ = format.seek(SeekMode::Accurate, to(*track_id));
                if let Ok(fresh) = rusty_aac::AacDecoder::with_config_bytes(config) {
                    *decoder = fresh;
                }
            }
            Source::Adts { frames, decoder } => {
                let Some(path) = frames.file.clone() else { return };
                let Ok(file) = std::fs::File::open(&path) else { return };
                let mut fresh = AdtsFrames::new(Box::new(file), Some(path));
                fresh.skip_to(ms);
                *frames = fresh;
                *decoder = rusty_aac::AacDecoder::new();
            }
        }
    }
}

fn aac(decoded: Result<rusty_aac::DecodedAudio, rusty_aac::Error>, out: &mut Vec<f32>) -> Next {
    match decoded {
        Ok(f) if f.channels > 0 && f.sample_rate > 0 && !f.samples.is_empty() => {
            out.extend_from_slice(&f.samples);
            Next::Audio { rate: f.sample_rate, channels: f.channels as usize }
        }
        _ => Next::Skip,
    }
}

/// How far to look for the next frame before giving up: frames are 8 KB at
/// most, and what comes between them (a stream joined midway) is less.
const SEARCH: usize = 64 * 1024;

/// The core sample rates an ADTS header can name, by its index.
const ADTS_RATES: [u32; 13] = [96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350];

/// ADTS frames out of a byte stream, each whole (header and all); bytes
/// between frames (a stream joined midway) are passed over.
pub(crate) struct AdtsFrames {
    reader: Box<dyn Read + Send>,
    buf: Vec<u8>,
    pos: usize,
    /// The file it's read from, if it is one (to seek).
    file: Option<PathBuf>,
}

impl AdtsFrames {
    pub(crate) fn new(reader: Box<dyn Read + Send>, file: Option<PathBuf>) -> AdtsFrames {
        AdtsFrames { reader, buf: Vec::new(), pos: 0, file }
    }

    /// At least `want` bytes from `pos` on, or false at the end.
    fn fill(&mut self, want: usize) -> bool {
        let mut chunk = [0u8; 16 * 1024];
        while self.buf.len() - self.pos < want {
            match self.reader.read(&mut chunk) {
                Ok(0) | Err(_) => return false,
                Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
            }
        }
        true
    }

    /// The length of the frame at `pos`, if a frame header is there.
    fn frame_here(&self) -> Option<usize> {
        let b = &self.buf[self.pos..];
        if b[0] != 0xFF || b[1] & 0xF6 != 0xF0 {
            return None;
        }
        let len = (((b[3] & 0x03) as usize) << 11) | ((b[4] as usize) << 3) | ((b[5] as usize) >> 5);
        (len >= 7).then_some(len)
    }

    fn next_header(&mut self) -> Option<usize> {
        let mut passed = 0;
        loop {
            if !self.fill(7) {
                return None;
            }
            if let Some(len) = self.frame_here() {
                return Some(len);
            }
            self.pos += 1;
            passed += 1;
            if passed > SEARCH {
                return None;
            }
            if self.pos > 256 * 1024 {
                self.buf.drain(..self.pos);
                self.pos = 0;
            }
        }
    }

    pub(crate) fn next_frame(&mut self) -> Option<Vec<u8>> {
        let len = self.next_header()?;
        if !self.fill(len) {
            return None;
        }
        let frame = self.buf[self.pos..self.pos + len].to_vec();
        self.pos += len;
        if self.pos > 256 * 1024 {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
        Some(frame)
    }

    /// Past the frames before `ms` (their headers say how long each lasts).
    fn skip_to(&mut self, ms: u32) {
        let mut at_ms = 0.0f64;
        while at_ms < ms as f64 {
            let Some(len) = self.next_header() else { return };
            let b = &self.buf[self.pos..];
            let rate = ADTS_RATES.get(((b[2] >> 2) & 0x0F) as usize).copied().unwrap_or(44100);
            let blocks = (b[6] & 0x03) as f64 + 1.0;
            if !self.fill(len) {
                return;
            }
            at_ms += blocks * 1024.0 * 1000.0 / rate as f64;
            self.pos += len;
        }
        self.buf.drain(..self.pos);
        self.pos = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tone made into AAC-LC ADTS by rusty_aac's own encoder.
    fn tone_adts(seconds: f32) -> Vec<u8> {
        let rate = 44100;
        let pcm: Vec<f32> = (0..(rate as f32 * seconds) as usize)
            .flat_map(|i| {
                let s = (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 0.3;
                [s, s]
            })
            .collect();
        let mut encoder = rusty_aac::AacEncoder::new(rusty_aac::AacEncoderConfig { bitrate_bps: 128_000, ..Default::default() });
        encoder.push_pcm(&pcm, 2, rate).unwrap();
        encoder.finish();
        let mut adts = Vec::new();
        while let Ok(packet) = encoder.next_packet() {
            adts.extend(rusty_aac::write_adts_header(&rusty_aac::AdtsHeader {
                object_type: 2,
                sample_rate: rate,
                channels: 2,
                frame_length: 7 + packet.data.len(),
                header_len: 7,
            }));
            adts.extend(packet.data);
        }
        adts
    }

    /// Bytes handed over a few at a time, as a station's arrive.
    struct Trickle(Vec<u8>, usize);
    impl Read for Trickle {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            let n = out.len().min(self.1).min(self.0.len());
            out[..n].copy_from_slice(&self.0[..n]);
            self.0.drain(..n);
            Ok(n)
        }
    }

    #[test]
    fn adts_aac_plays_through_rusty_aac_however_the_bytes_come() {
        let mut bytes = vec![0x12, 0xFF, 0x00]; // joined midway: junk first
        bytes.extend(tone_adts(1.0));
        let mut source = Source::adts(Box::new(Trickle(bytes, 333)), None);
        let (mut frames, mut rate, mut channels) = (0, 0, 0);
        let mut out = Vec::new();
        loop {
            match source.next(&mut out) {
                Next::Audio { rate: r, channels: c } => {
                    (rate, channels) = (r, c);
                    frames += out.len() / c;
                }
                Next::Skip => {}
                Next::End => break,
            }
        }
        assert_eq!((rate, channels), (44100, 2));
        // a second of sound (the encoder's start-up adds a frame or two)
        assert!((43_000..48_000).contains(&frames), "{frames} frames");
    }

    /// Something that isn't ADTS (MP3 frames, a station that says AAC but
    /// isn't) is told at once instead of searched forever; real AAC is found.
    #[test]
    fn what_is_not_aac_is_told_at_once() {
        let mp3ish: Vec<u8> = std::iter::repeat_n([0xFF, 0xFB, 0x90, 0x64], 50_000).flatten().collect();
        assert!(!Source::adts(Box::new(std::io::Cursor::new(mp3ish)), None).finds_sound());
        let mut junk_then_aac = vec![0u8; 1000];
        junk_then_aac.extend(tone_adts(0.5));
        let mut source = Source::adts(Box::new(std::io::Cursor::new(junk_then_aac)), None);
        assert!(source.finds_sound());
        // and nothing was used up: it plays from its first frame
        let mut out = Vec::new();
        assert!(matches!(source.next(&mut out), Next::Audio { .. } | Next::Skip));
    }

    #[test]
    fn a_file_seeks_by_its_frames() {
        let path = std::env::temp_dir().join(format!("spotiamp-seek-{}.aac", std::process::id()));
        std::fs::write(&path, tone_adts(3.0)).unwrap();
        let mut source = Source::adts(Box::new(std::fs::File::open(&path).unwrap()), Some(path.clone()));
        source.seek(2000);
        let mut out = Vec::new();
        let mut frames = 0;
        loop {
            match source.next(&mut out) {
                Next::Audio { channels, .. } => frames += out.len() / channels,
                Next::Skip => {}
                Next::End => break,
            }
        }
        // what's left after 2 s of 3 (and the encoder's extra frames)
        assert!((43_000..49_000).contains(&frames), "{frames} frames left");
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod real {
    use super::*;

    /// Real stations (STATIONS: "name|url" lines in a file) through the
    /// station path and rusty_aac: the rate and channels that come out.
    /// Network; run by hand.
    #[test]
    #[ignore]
    fn real_aac_stations() {
        let list = std::fs::read_to_string(std::env::var("STATIONS").unwrap()).unwrap();
        for line in list.lines().filter(|l| l.contains('|')) {
            let (name, url) = line.split_once('|').unwrap();
            let url = url.to_string();
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let r = (|| -> Result<String, String> {
                    let generation = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
                    let tuned = crate::radio::open(&url, generation)?;
                    if !tuned.aac {
                        return Err("not AAC".into());
                    }
                    let mut source = Source::adts(Box::new(tuned.audio), None);
                    let (mut out, mut frames, mut spec) = (Vec::new(), 0usize, (0, 0));
                    while frames < 48_000 * 3 {
                        match source.next(&mut out) {
                            Next::Audio { rate, channels } => {
                                spec = (rate, channels);
                                frames += out.len() / channels;
                            }
                            Next::Skip => {}
                            Next::End => break,
                        }
                    }
                    Ok(format!("{} Hz, {} ch, {} frames", spec.0, spec.1, frames))
                })();
                let _ = tx.send(r);
            });
            let r = rx.recv_timeout(std::time::Duration::from_secs(40)).unwrap_or(Err("timed out".into()));
            println!("{} {name}: {}", if r.is_ok() { "OK  " } else { "FAIL" }, r.unwrap_or_else(|e| e));
        }
    }
}
