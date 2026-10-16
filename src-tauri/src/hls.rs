//! Radio stations streamed in pieces (HLS, an .m3u8 list of short files;
//! 0.7.8). The list is read again as it grows, each new piece is fetched,
//! and the sound in it goes on to the decoder like any station's: AAC or MP3
//! from MPEG-TS pieces (.ts) or plain ones (.aac/.mp3, an ID3 tag in front).
//! Encrypted stations and MP4 pieces aren't played (yet).

use std::sync::mpsc::{Sender, SyncSender};
use std::time::Duration;

use oauth2::url::Url;

/// Fetches a whole small file: where it came from in the end, and its bytes.
pub(crate) type FetchAll = dyn Fn(&str) -> Result<(String, Vec<u8>), String>;

/// The sound inside the pieces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Sound {
    Aac,
    Mp3,
}

#[derive(Debug, Default)]
struct MediaList {
    /// Seconds a piece lasts at most.
    target: f64,
    /// The number of the first piece listed.
    sequence: u64,
    segments: Vec<String>,
    /// No more pieces will come (a recording rather than a live station).
    ended: bool,
}

fn absolute(base: &str, link: &str) -> Option<String> {
    Url::parse(base).ok()?.join(link).ok().map(|u| u.to_string())
}

/// From a list of qualities, the one to play: the best up to 320 kbps (or
/// the smallest, when all are above), audio only where there's a choice.
fn best_variant(text: &str, base: &str) -> Option<String> {
    let mut variants: Vec<(u64, bool, String)> = Vec::new();
    let mut lines = text.lines().map(str::trim);
    while let Some(line) = lines.next() {
        let Some(attrs) = line.strip_prefix("#EXT-X-STREAM-INF:") else { continue };
        let bandwidth = attrs
            .split(',')
            .find_map(|a| a.trim().strip_prefix("BANDWIDTH="))
            .and_then(|b| b.parse().ok())
            .unwrap_or(0);
        // a video codec in its list: not audio only
        let audio_only = !attrs.contains("avc1") && !attrs.contains("hvc1") && !attrs.contains("RESOLUTION=");
        if let Some(url) = lines.by_ref().find(|l| !l.is_empty() && !l.starts_with('#')).and_then(|uri| absolute(base, uri)) {
            variants.push((bandwidth, audio_only, url));
        }
    }
    let audio: Vec<_> = if variants.iter().any(|v| v.1) { variants.into_iter().filter(|v| v.1).collect() } else { variants };
    audio
        .iter()
        .filter(|v| v.0 <= 320_000)
        .max_by_key(|v| v.0)
        .or_else(|| audio.iter().min_by_key(|v| v.0))
        .map(|v| v.2.clone())
}

fn media_list(text: &str, base: &str) -> MediaList {
    let mut list = MediaList { target: 6.0, ..Default::default() };
    for line in text.lines().map(str::trim) {
        if let Some(v) = line.strip_prefix("#EXT-X-TARGETDURATION:") {
            list.target = v.parse().unwrap_or(6.0);
        } else if let Some(v) = line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:") {
            list.sequence = v.parse().unwrap_or(0);
        } else if line.starts_with("#EXT-X-ENDLIST") {
            list.ended = true;
        } else if let Some(url) = (!line.is_empty() && !line.starts_with('#')).then(|| absolute(base, line)).flatten() {
            list.segments.push(url);
        }
    }
    list
}

/// MPEG-TS: where the sound is, carried over from piece to piece.
#[derive(Debug, Default)]
pub(crate) struct Ts {
    pmt: Option<u16>,
    audio: Option<(u16, Sound)>,
}

/// A table's section (PAT, PMT) from a packet's payload: after its pointer
/// field, up to its end without the CRC.
fn section(payload: &[u8]) -> Option<&[u8]> {
    let p = payload.get(1 + *payload.first()? as usize..)?;
    let len = ((*p.get(1)? as usize & 0x0F) << 8) | *p.get(2)? as usize;
    p.get(..(3 + len).checked_sub(4)?)
}

/// The sound out of MPEG-TS packets.
fn ts_audio(data: &[u8], ts: &mut Ts, out: &mut Vec<u8>) {
    for packet in data.chunks_exact(188) {
        if packet[0] != 0x47 {
            continue;
        }
        let start = packet[1] & 0x40 != 0;
        let pid = ((packet[1] as u16 & 0x1F) << 8) | packet[2] as u16;
        let payload = match (packet[3] >> 4) & 3 {
            1 => &packet[4..],
            3 => match packet.get(5 + packet[4] as usize..) {
                Some(p) => p,
                None => continue,
            },
            _ => continue,
        };
        if pid == 0 && start {
            // the program list: where its table (PMT) is
            if let Some(s) = section(payload) {
                ts.pmt = s.get(8..).and_then(|programs| {
                    programs
                        .chunks_exact(4)
                        .find(|p| (p[0], p[1]) != (0, 0))
                        .map(|p| ((p[2] as u16 & 0x1F) << 8) | p[3] as u16)
                });
            }
        } else if Some(pid) == ts.pmt && start {
            // the program's table: which stream is its sound
            if let Some(s) = section(payload) {
                let info = s.get(10..12).map(|b| ((b[0] as usize & 0x0F) << 8) | b[1] as usize).unwrap_or(0);
                let mut i = 12 + info;
                while let Some(e) = s.get(i..i + 5) {
                    let kind = match e[0] {
                        0x0F => Some(Sound::Aac),
                        0x03 | 0x04 => Some(Sound::Mp3),
                        _ => None,
                    };
                    if let Some(kind) = kind {
                        ts.audio = Some((((e[1] as u16 & 0x1F) << 8) | e[2] as u16, kind));
                        break;
                    }
                    i += 5 + (((e[3] as usize & 0x0F) << 8) | e[4] as usize);
                }
            }
        } else if Some(pid) == ts.audio.map(|a| a.0) {
            let mut es = payload;
            // a new PES packet: its header off
            if start && es.starts_with(&[0, 0, 1]) {
                es = es.get(9 + *es.get(8).unwrap_or(&0) as usize..).unwrap_or(&[]);
            }
            out.extend_from_slice(es);
        }
    }
}

/// ID3 tags off the front of a plain piece.
fn without_id3(mut data: &[u8]) -> &[u8] {
    while data.len() >= 10 && &data[..3] == b"ID3" {
        let size = data[6..10].iter().fold(0usize, |n, b| (n << 7) | (*b as usize & 0x7F));
        let footer = if data[5] & 0x10 != 0 { 10 } else { 0 };
        data = data.get(10 + size + footer..).unwrap_or(&[]);
    }
    data
}

/// The sound in a piece, and what kind it is (when it can be told).
pub(crate) fn piece_audio(data: &[u8], ts: &mut Ts) -> (Vec<u8>, Option<Sound>) {
    if data.first() == Some(&0x47) && data.get(188).is_none_or(|b| *b == 0x47) {
        let mut out = Vec::with_capacity(data.len());
        ts_audio(data, ts, &mut out);
        return (out, ts.audio.map(|a| a.1));
    }
    let plain = without_id3(data);
    let kind = match plain {
        [0xFF, b, ..] if b & 0xF6 == 0xF0 => Some(Sound::Aac),
        [0xFF, b, ..] if b & 0xE0 == 0xE0 => Some(Sound::Mp3),
        _ => None,
    };
    (plain.to_vec(), kind)
}

/// Play a station's .m3u8 (already fetched: `text`, from `url`): its pieces
/// go to `audio`, after the decoder's been told the kind (`head_tx`). Runs
/// until the station ends or the player lets go.
pub(crate) fn play(
    url: String,
    text: String,
    kbps: u32,
    fetch_all: &FetchAll,
    head_tx: Sender<Result<super::radio::Head, String>>,
    audio: SyncSender<Vec<u8>>,
) {
    let mut told = false;
    let result = run(url, text, kbps, fetch_all, &head_tx, &audio, &mut told);
    match result {
        Err(e) if !told => {
            let _ = head_tx.send(Err(e));
        }
        Err(e) => log::info!("radio: the station's pieces stopped ({e})"),
        Ok(()) => log::info!("radio: the station's pieces ended"),
    }
}

fn run(
    url: String,
    text: String,
    kbps: u32,
    fetch_all: &FetchAll,
    head_tx: &Sender<Result<super::radio::Head, String>>,
    audio: &SyncSender<Vec<u8>>,
    told: &mut bool,
) -> Result<(), String> {
    let (mut list_url, mut text) = (url, text);
    let mut kbps = kbps;
    if text.contains("#EXT-X-STREAM-INF") {
        let variant = best_variant(&text, &list_url).ok_or("there's no stream in this station's list")?;
        kbps = text
            .lines()
            .find(|l| l.contains("#EXT-X-STREAM-INF"))
            .and_then(|l| l.split(',').find_map(|a| a.trim().strip_prefix("BANDWIDTH=")))
            .and_then(|b| b.parse::<u32>().ok())
            .map_or(kbps, |b| b / 1000);
        let (u, body) = fetch_all(&variant)?;
        list_url = u;
        text = String::from_utf8_lossy(&body).into_owned();
    }
    if text.contains("#EXT-X-KEY") && !text.contains("METHOD=NONE") {
        return Err("this station is encrypted, which Spotiamp+ can't play".into());
    }
    if text.contains("#EXT-X-MAP") {
        return Err("this station streams MP4 pieces, which Spotiamp+ can't play yet".into());
    }
    let mut ts = Ts::default();
    let mut next: Option<u64> = None;
    loop {
        let list = media_list(&text, &list_url);
        if list.segments.is_empty() {
            return Err("the station's list has no pieces".into());
        }
        let last = list.sequence + list.segments.len() as u64;
        // live: from three pieces before the newest (some seconds in hand);
        // fallen out of the list (a long stall): back to there
        let from = match next {
            Some(n) if n >= list.sequence && n <= last => n,
            _ if list.ended => list.sequence,
            _ => last.saturating_sub(3).max(list.sequence),
        };
        for (k, piece) in list.segments.iter().enumerate() {
            let seq = list.sequence + k as u64;
            if seq < from {
                continue;
            }
            let (_, bytes) = fetch_all(piece)?;
            let (sound, kind) = piece_audio(&bytes, &mut ts);
            if !*told {
                let kind = kind.ok_or("this station's sound is neither AAC nor MP3")?;
                let head = super::radio::Head::for_pieces(kind == Sound::Aac, kbps);
                log::info!("radio: pieces ({:?}, {kbps} kbps)", kind);
                head_tx.send(Ok(head)).map_err(|_| "stopped".to_string())?;
                *told = true;
            }
            if audio.send(sound).is_err() {
                return Ok(()); // the player let go
            }
            next = Some(seq + 1);
        }
        if list.ended {
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs_f64((list.target / 2.0).clamp(1.0, 10.0)));
        let (u, body) = fetch_all(&list_url)?;
        list_url = u;
        text = String::from_utf8_lossy(&body).into_owned();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_of_qualities_gives_the_best_up_to_320k() {
        let master = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=64000,CODECS=\"mp4a.40.5\"\nlow/list.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=128255,CODECS=\"mp4a.40.2\"\nmid/list.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=900000,CODECS=\"avc1.4d401f,mp4a.40.2\",RESOLUTION=640x360\nvideo.m3u8\n";
        assert_eq!(best_variant(master, "https://r.example/s/playlist.m3u8").unwrap(), "https://r.example/s/mid/list.m3u8");
    }

    #[test]
    fn a_live_list_reads_its_pieces() {
        let text = "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:6\n#EXT-X-MEDIA-SEQUENCE:5742991\n#EXTINF:6.016,\na_5742991.aac\n#EXTINF:6.016,\nhttps://cdn.example/a_5742992.aac?x=1\n";
        let list = media_list(text, "https://r.example/live/master_128.m3u8");
        assert_eq!((list.sequence, list.target, list.ended), (5742991, 6.0, false));
        assert_eq!(list.segments, ["https://r.example/live/a_5742991.aac", "https://cdn.example/a_5742992.aac?x=1"]);
    }

    #[test]
    fn plain_pieces_lose_their_id3_and_say_what_they_are() {
        let mut piece = b"ID3\x04\x00\x00\x00\x00\x00\x05PRIV!".to_vec();
        piece.extend([0xFF, 0xF1, 0x50, 0x80, 0x01, 0x1F, 0xFC]);
        let (sound, kind) = piece_audio(&piece, &mut Ts::default());
        assert_eq!((sound[..2].to_vec(), kind), (vec![0xFF, 0xF1], Some(Sound::Aac)));
        let (_, kind) = piece_audio(&[0xFF, 0xFB, 0x90, 0x00], &mut Ts::default());
        assert_eq!(kind, Some(Sound::Mp3));
    }

    /// A small MPEG-TS stream: its program list, its table naming an AAC
    /// stream on PID 0x101, and that stream's packets, whose sound comes out
    /// without the PES header.
    #[test]
    fn mpeg_ts_pieces_give_their_sound() {
        let packet = |pid: u16, start: bool, payload: &[u8]| {
            let mut p = vec![0x47, (if start { 0x40 } else { 0 }) | (pid >> 8) as u8, pid as u8, 0x10];
            p.extend_from_slice(payload);
            p.resize(188, 0xFF);
            p
        };
        // PAT: pointer 0, table 0, length 13, ts id, version, sections, program 1 -> PMT PID 0x100, CRC
        let pat = [0, 0x00, 0xB0, 13, 0, 1, 0xC1, 0, 0, 0, 1, 0xE1, 0x00, 0, 0, 0, 0];
        // PMT: pointer 0, table 2, length 18, program 1, ..., PCR PID, info 0, one stream: AAC (0x0F) on 0x101, CRC
        let pmt = [0, 0x02, 0xB0, 18, 0, 1, 0xC1, 0, 0, 0xE1, 0x01, 0xF0, 0x00, 0x0F, 0xE1, 0x01, 0xF0, 0x00, 0, 0, 0, 0];
        let mut pes = vec![0, 0, 1, 0xC0, 0, 0, 0x80, 0x80, 5, 1, 2, 3, 4, 5];
        pes.extend([0xFF, 0xF1, 0xAA, 0xBB]);
        let mut data = packet(0, true, &pat);
        data.extend(packet(0x100, true, &pmt));
        // the rest of the payload is part of the sound too (a real stream
        // would put its stuffing in an adaptation field instead)
        let mut first = packet(0x101, true, &pes);
        first.truncate(4 + pes.len());
        first.resize(188, 0xEE);
        data.extend(first);
        data.extend(packet(0x101, false, &[0xCC, 0xDD]));
        let mut ts = Ts::default();
        let (sound, kind) = piece_audio(&data, &mut ts);
        assert_eq!(kind, Some(Sound::Aac));
        assert_eq!(&sound[..4], &[0xFF, 0xF1, 0xAA, 0xBB]);
        assert!(sound.windows(2).any(|w| w == [0xCC, 0xDD]));
    }
}

#[cfg(test)]
mod real {
    /// Real HLS stations (STATIONS: "name|url" lines in a file), each through the
    /// open and the decoder, for how many really play. Network; run by hand.
    #[test]
    #[ignore]
    fn real_stations() {
        let list = std::fs::read_to_string(std::env::var("STATIONS").unwrap()).unwrap();
        let (mut ok, mut all) = (0, 0);
        for line in list.lines().filter(|l| l.contains('|')) {
            let (name, url) = line.split_once('|').unwrap();
            all += 1;
            let url = url.to_string();
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let r = (|| -> Result<String, String> {
                    let generation = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
                    let (mss, hint, _) = crate::radio::open(&url, generation)?;
                    let mut p = symphonia::default::get_probe().format(&hint, mss, &Default::default(), &Default::default()).map_err(|e| format!("probe: {e}"))?;
                    let track = p.format.tracks()[0].clone();
                    let mut dec = symphonia::default::get_codecs().make(&track.codec_params, &Default::default()).map_err(|e| format!("decoder: {e}"))?;
                    let mut frames = 0;
                    for _ in 0..40 {
                        let packet = p.format.next_packet().map_err(|e| format!("packet: {e}"))?;
                        frames += dec.decode(&packet).map(|b| b.frames()).unwrap_or(0);
                    }
                    Ok(format!("{frames} frames {:?} Hz", track.codec_params.sample_rate))
                })();
                let _ = tx.send(r);
            });
            let r = rx.recv_timeout(std::time::Duration::from_secs(30)).unwrap_or(Err("timed out".into()));
            if r.is_ok() {
                ok += 1;
            }
            println!("{} {name}: {}", if r.is_ok() { "OK  " } else { "FAIL" }, r.unwrap_or_else(|e| e));
        }
        println!("==== {ok} of {all} play");
    }
}
