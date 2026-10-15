//! Internet radio (0.7.8, Ctrl+L): a SHOUTcast / Icecast stream played through
//! the local-file engine, so the EQ, the visualizers, volume and balance work
//! on it as on a file. Classic SHOUTcast servers answer "ICY 200 OK" instead of
//! HTTP, which an HTTP client turns down, so plain http:// streams are read
//! over a socket of our own; https:// ones go through reqwest. The song names a
//! station sends between the audio (ICY metadata) are taken out of the sound
//! and passed on. A .pls / .m3u link is followed to the stream inside it.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use oauth2::reqwest;
use oauth2::url::Url;
use symphonia::core::io::{MediaSourceStream, ReadOnlySource};
use symphonia::core::probe::Hint;

/// How long a station may go quiet before it counts as gone.
const STALL: Duration = Duration::from_secs(20);
const CONNECT: Duration = Duration::from_secs(10);

/// What a station says about itself, and the song it's on.
#[derive(Debug, Default, Clone)]
pub struct StreamMeta {
    pub station: String,
    pub title: String,
    pub kbps: u32,
    /// Set when station or title changed and nobody has said so yet.
    pub changed: bool,
}

/// The response's head: what matters of its headers.
#[derive(Debug, Default)]
struct Head {
    content_type: String,
    metaint: usize,
    station: String,
    kbps: u32,
}

impl Head {
    fn from_headers<'a>(headers: impl Iterator<Item = (String, &'a str)>) -> Head {
        let mut head = Head::default();
        for (name, value) in headers {
            let value = value.trim();
            match name.as_str() {
                "content-type" => head.content_type = value.to_ascii_lowercase(),
                "icy-metaint" => head.metaint = value.parse().unwrap_or(0),
                "icy-name" => head.station = value.to_string(),
                "icy-br" => head.kbps = value.split(',').next().and_then(|v| v.trim().parse().ok()).unwrap_or(0),
                _ => {}
            }
        }
        head
    }

    fn is_playlist(&self, url: &str) -> bool {
        let path = url.split(['?', '#']).next().unwrap_or("").to_ascii_lowercase();
        ["mpegurl", "scpls", "pls+xml"].iter().any(|t| self.content_type.contains(t))
            || path.ends_with(".pls")
            || path.ends_with(".m3u")
            || path.ends_with(".m3u8")
    }

    /// The format, for the decoder's probe.
    fn hint(&self) -> Hint {
        let mut hint = Hint::new();
        let ct = &self.content_type;
        if ct.contains("mpeg") || ct.contains("mp3") {
            hint.with_extension("mp3");
        } else if ct.contains("aac") {
            hint.with_extension("aac");
        } else if ct.contains("ogg") {
            hint.with_extension("ogg");
        } else if ct.contains("flac") {
            hint.with_extension("flac");
        }
        hint
    }
}

/// The audio coming in, chunk by chunk.
enum Body {
    Socket(TcpStream, Vec<u8>),
    Https(reqwest::Response),
}

impl Body {
    fn next_chunk(&mut self) -> Option<Vec<u8>> {
        match self {
            Body::Socket(socket, early) => {
                if !early.is_empty() {
                    return Some(std::mem::take(early));
                }
                let mut buf = vec![0u8; 16 * 1024];
                match socket.read(&mut buf) {
                    Ok(0) | Err(_) => None,
                    Ok(n) => {
                        buf.truncate(n);
                        Some(buf)
                    }
                }
            }
            Body::Https(response) => tauri::async_runtime::block_on(response.chunk())
                .ok()
                .flatten()
                .map(|b| b.to_vec()),
        }
    }

    /// The start of a playlist file's text (they're small).
    fn text(&mut self) -> String {
        let mut bytes = Vec::new();
        while bytes.len() < 64 * 1024 {
            match self.next_chunk() {
                Some(chunk) => bytes.extend(chunk),
                None => break,
            }
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

enum Opened {
    Redirect(String),
    Stream(Head, Body),
}

/// A plain http:// request over a socket of our own (HTTP/1.0: no chunked
/// answers), taking "ICY 200 OK" as well as HTTP's own.
fn open_http(url: &str) -> Result<Opened, String> {
    let u = Url::parse(url).map_err(|_| "that isn't a link".to_string())?;
    let host = u.host_str().ok_or("that link has no server in it")?.to_string();
    let port = u.port_or_known_default().unwrap_or(80);
    let addr = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|_| format!("couldn't find {host}"))?
        .next()
        .ok_or_else(|| format!("couldn't find {host}"))?;
    let mut socket = TcpStream::connect_timeout(&addr, CONNECT).map_err(|_| format!("{host} didn't answer"))?;
    socket.set_read_timeout(Some(STALL)).ok();
    let path = &u[oauth2::url::Position::BeforePath..];
    let host_header = if u.port().is_some() { format!("{host}:{port}") } else { host.clone() };
    // in one go: some servers take the first packet for the whole request
    let request = format!(
        "GET {path} HTTP/1.0\r\nHost: {host_header}\r\nUser-Agent: Spotiamp+/{}\r\nAccept: */*\r\nIcy-MetaData: 1\r\nConnection: close\r\n\r\n",
        env!("CARGO_PKG_VERSION")
    );
    socket.write_all(request.as_bytes()).map_err(|_| format!("{host} didn't answer"))?;

    // the head, up to the empty line; what comes after it is audio already
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    let end = loop {
        if let Some(i) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        if raw.len() > 32 * 1024 {
            return Err("the station's answer made no sense".into());
        }
        let n = socket.read(&mut buf).map_err(|_| format!("{host} didn't answer"))?;
        if n == 0 {
            return Err(format!("{host} closed the connection"));
        }
        raw.extend_from_slice(&buf[..n]);
    };
    let early = raw[end + 4..].to_vec();
    let text = String::from_utf8_lossy(&raw[..end]).into_owned();
    let mut lines = text.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let status: u16 = status_line.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let headers: Vec<(String, &str)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v))
        .collect();
    if (300..400).contains(&status) {
        let to = headers.iter().find(|(k, _)| k == "location").map(|(_, v)| v.trim()).ok_or("a redirect to nowhere")?;
        return Ok(Opened::Redirect(u.join(to).map(|u| u.to_string()).unwrap_or_else(|_| to.to_string())));
    }
    if status != 200 {
        return Err(format!("the station answered {}", status_line.trim()));
    }
    Ok(Opened::Stream(Head::from_headers(headers.into_iter()), Body::Socket(socket, early)))
}

fn open_https(url: &str) -> Result<Opened, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT)
        .user_agent(concat!("Spotiamp+/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?;
    let response = tauri::async_runtime::block_on(client.get(url).header("Icy-MetaData", "1").send())
        .map_err(|_| "the station didn't answer".to_string())?;
    if !response.status().is_success() {
        return Err(format!("the station answered {}", response.status()));
    }
    let head = Head::from_headers(
        response
            .headers()
            .iter()
            .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.as_str().to_ascii_lowercase(), v))),
    );
    Ok(Opened::Stream(head, Body::Https(response)))
}

/// The first stream in a .pls or .m3u, or why there's none.
fn first_stream(text: &str, base: &str) -> Result<String, String> {
    if text.contains("#EXT-X-") {
        return Err("this station streams in pieces (HLS), which Spotiamp+ can't play yet".into());
    }
    let pls = text.to_ascii_lowercase().contains("[playlist]");
    let base = Url::parse(base).map_err(|_| "that isn't a link".to_string())?;
    text.lines()
        .map(str::trim)
        .filter_map(|l| {
            if pls {
                // File1=http://…
                let (key, value) = l.split_once('=')?;
                key.trim().to_ascii_lowercase().starts_with("file").then_some(value.trim())
            } else {
                (!l.starts_with('#') && !l.is_empty()).then_some(l)
            }
        })
        .filter_map(|l| base.join(l).ok().map(|u| u.to_string()))
        .find(|u| u.starts_with("http://") || u.starts_with("https://"))
        .ok_or_else(|| "there's no station in that list".into())
}

/// Connect (following redirects and playlist files), then pass the audio on
/// until the reader goes away or the station stops.
fn fetch(url: String, head_tx: mpsc::Sender<Result<Head, String>>, audio: SyncSender<Vec<u8>>) {
    let mut url = url;
    for _ in 0..5 {
        let opened = if url.to_ascii_lowercase().starts_with("https://") { open_https(&url) } else { open_http(&url) };
        match opened {
            Err(e) => {
                let _ = head_tx.send(Err(e));
                return;
            }
            Ok(Opened::Redirect(to)) => url = to,
            Ok(Opened::Stream(head, mut body)) => {
                if head.is_playlist(&url) {
                    match first_stream(&body.text(), &url) {
                        Ok(next) => url = next,
                        Err(e) => {
                            let _ = head_tx.send(Err(e));
                            return;
                        }
                    }
                    continue;
                }
                log::info!("radio: connected ({}, {} kbps)", head.content_type, head.kbps);
                if head_tx.send(Ok(head)).is_err() {
                    return;
                }
                while let Some(chunk) = body.next_chunk() {
                    if audio.send(chunk).is_err() {
                        return; // stopped
                    }
                }
                log::info!("radio: the station stopped sending");
                return;
            }
        }
    }
    let _ = head_tx.send(Err("too many redirects".into()));
}

/// The audio, the station's song names taken out of it.
struct StreamReader {
    chunks: Mutex<Receiver<Vec<u8>>>,
    audio: VecDeque<u8>,
    metaint: usize,
    until_meta: usize,
    meta_left: Option<usize>,
    meta_buf: Vec<u8>,
    meta: Arc<Mutex<StreamMeta>>,
    /// The player's; once it moves on (another song, stop), reading ends.
    generation: Arc<AtomicU64>,
    opened_at: u64,
}

impl StreamReader {
    fn feed(&mut self, mut data: &[u8]) {
        while !data.is_empty() {
            if self.metaint == 0 {
                self.audio.extend(data);
                return;
            }
            if let Some(left) = self.meta_left {
                let take = left.min(data.len());
                self.meta_buf.extend_from_slice(&data[..take]);
                data = &data[take..];
                if take == left {
                    self.meta_left = None;
                    self.until_meta = self.metaint;
                    if let Some(title) = stream_title(&self.meta_buf) {
                        let mut meta = self.meta.lock().unwrap_or_else(|e| e.into_inner());
                        if meta.title != title {
                            meta.title = title;
                            meta.changed = true;
                        }
                    }
                    self.meta_buf.clear();
                } else {
                    self.meta_left = Some(left - take);
                }
            } else if self.until_meta == 0 {
                let len = data[0] as usize * 16;
                data = &data[1..];
                if len == 0 {
                    self.until_meta = self.metaint;
                } else {
                    self.meta_left = Some(len);
                }
            } else {
                let take = self.until_meta.min(data.len());
                self.audio.extend(&data[..take]);
                data = &data[take..];
                self.until_meta -= take;
            }
        }
    }
}

impl Read for StreamReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let mut quiet = Duration::ZERO;
        while self.audio.is_empty() {
            if self.generation.load(Ordering::Relaxed) != self.opened_at {
                return Err(io::Error::new(io::ErrorKind::ConnectionAborted, "stopped"));
            }
            let next = self.chunks.lock().unwrap_or_else(|e| e.into_inner()).recv_timeout(Duration::from_millis(100));
            match next {
                Ok(chunk) => {
                    self.feed(&chunk);
                    quiet = Duration::ZERO;
                }
                Err(RecvTimeoutError::Timeout) => {
                    quiet += Duration::from_millis(100);
                    if quiet >= STALL {
                        return Err(io::Error::new(io::ErrorKind::TimedOut, "the station went quiet"));
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return Ok(0),
            }
        }
        let n = out.len().min(self.audio.len());
        for (o, b) in out.iter_mut().zip(self.audio.drain(..n)) {
            *o = b;
        }
        Ok(n)
    }
}

/// `StreamTitle='Artist - Song';…` → Artist - Song (UTF-8, or Latin-1 as
/// older stations send).
fn stream_title(block: &[u8]) -> Option<String> {
    let text = match std::str::from_utf8(block) {
        Ok(s) => s.to_string(),
        Err(_) => block.iter().map(|&b| b as char).collect(),
    };
    let start = text.find("StreamTitle='")? + "StreamTitle='".len();
    let rest = &text[start..];
    let end = rest.find("';").or_else(|| rest.rfind('\''))?;
    Some(rest[..end].trim().to_string())
}

/// Connect to a station: the audio ready for the decoder, a hint of its
/// format, and what it says about itself (filled in as songs change).
pub fn open(url: &str, generation: Arc<AtomicU64>) -> Result<(MediaSourceStream, Hint, Arc<Mutex<StreamMeta>>), String> {
    let opened_at = generation.load(Ordering::Relaxed);
    let (head_tx, head_rx) = mpsc::channel();
    let (audio_tx, audio_rx) = mpsc::sync_channel(64);
    let url = url.to_string();
    std::thread::Builder::new()
        .name("radio".into())
        .spawn(move || fetch(url, head_tx, audio_tx))
        .map_err(|e| e.to_string())?;
    // waiting here, the player can still be told to stop
    let head = loop {
        match head_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(head) => break head?,
            Err(RecvTimeoutError::Timeout) if generation.load(Ordering::Relaxed) == opened_at => continue,
            _ => return Err("stopped".into()),
        }
    };
    let hint = head.hint();
    let meta = Arc::new(Mutex::new(StreamMeta {
        station: head.station.clone(),
        title: String::new(),
        kbps: head.kbps,
        changed: true,
    }));
    let reader = StreamReader {
        chunks: Mutex::new(audio_rx),
        audio: VecDeque::new(),
        metaint: head.metaint,
        until_meta: head.metaint,
        meta_left: None,
        meta_buf: Vec::new(),
        meta: meta.clone(),
        generation,
        opened_at,
    };
    let mss = MediaSourceStream::new(Box::new(ReadOnlySource::new(reader)), Default::default());
    Ok((mss, hint, meta))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn reader(metaint: usize) -> (StreamReader, SyncSender<Vec<u8>>) {
        let (tx, rx) = mpsc::sync_channel(64);
        let r = StreamReader {
            chunks: Mutex::new(rx),
            audio: VecDeque::new(),
            metaint,
            until_meta: metaint,
            meta_left: None,
            meta_buf: Vec::new(),
            meta: Arc::new(Mutex::new(StreamMeta::default())),
            generation: Arc::new(AtomicU64::new(0)),
            opened_at: 0,
        };
        (r, tx)
    }

    /// `audio` with a song name after every `metaint` bytes
    fn with_meta(audio: &[u8], metaint: usize, title: &str) -> Vec<u8> {
        let mut block = format!("StreamTitle='{title}';").into_bytes();
        block.resize(block.len().div_ceil(16) * 16, 0);
        let mut out = Vec::new();
        for (i, piece) in audio.chunks(metaint).enumerate() {
            out.extend_from_slice(piece);
            if piece.len() == metaint {
                if i == 0 {
                    out.push((block.len() / 16) as u8);
                    out.extend_from_slice(&block);
                } else {
                    out.push(0);
                }
            }
        }
        out
    }

    #[test]
    fn song_names_come_out_of_the_audio() {
        let audio: Vec<u8> = (0..100u8).collect();
        let sent = with_meta(&audio, 16, "Şebnem Ferah - Sil Baştan");
        let (mut r, tx) = reader(16);
        // in awkward pieces, as the network hands them over
        for piece in sent.chunks(7) {
            tx.send(piece.to_vec()).unwrap();
        }
        drop(tx);
        let mut got = Vec::new();
        r.read_to_end(&mut got).unwrap();
        assert_eq!(got, audio);
        let meta = r.meta.lock().unwrap();
        assert_eq!(meta.title, "Şebnem Ferah - Sil Baştan");
        assert!(meta.changed);
    }

    #[test]
    fn a_stopped_player_stops_the_reading() {
        let (mut r, _tx) = reader(0);
        r.generation.store(1, Ordering::Relaxed);
        let e = r.read(&mut [0u8; 8]).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::ConnectionAborted);
    }

    #[test]
    fn playlists_lead_to_their_stream() {
        let pls = "[playlist]\nNumberOfEntries=2\nFile1=http://radio.example:8000/stream\nTitle1=One\nFile2=http://b.example/\n";
        assert_eq!(first_stream(pls, "http://x.example/a.pls").unwrap(), "http://radio.example:8000/stream");
        let m3u = "#EXTM3U\n#EXTINF:-1,Some FM\nhttps://s.example/live.mp3\n";
        assert_eq!(first_stream(m3u, "http://x.example/a.m3u").unwrap(), "https://s.example/live.mp3");
        assert_eq!(first_stream("live/aac\n", "http://x.example/list.m3u").unwrap(), "http://x.example/live/aac");
        assert!(first_stream("#EXTM3U\n#EXT-X-VERSION:3\nseg1.ts\n", "http://x.example/a.m3u8").is_err());
    }

    /// A classic SHOUTcast server: "ICY 200 OK", a name, song names in the audio.
    #[test]
    fn an_icy_station_plays() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let audio: Vec<u8> = (0..64u8).collect();
        let body = with_meta(&audio, 32, "Test Song");
        std::thread::spawn(move || {
            let (mut s, _) = server.accept().unwrap();
            let mut req = Vec::new();
            let mut buf = [0u8; 256];
            while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = s.read(&mut buf).unwrap();
                req.extend_from_slice(&buf[..n]);
            }
            assert!(String::from_utf8_lossy(&req).contains("Icy-MetaData: 1"));
            s.write_all(b"ICY 200 OK\r\nicy-name: Test FM\r\nicy-br: 128\r\nicy-metaint: 32\r\ncontent-type: audio/mpeg\r\n\r\n").unwrap();
            s.write_all(&body).unwrap();
        });
        let generation = Arc::new(AtomicU64::new(0));
        let (mut mss, _hint, meta) = open(&format!("http://127.0.0.1:{port}/;"), generation).unwrap();
        let mut got = Vec::new();
        mss.read_to_end(&mut got).unwrap();
        assert_eq!(got, audio);
        let meta = meta.lock().unwrap();
        assert_eq!((meta.station.as_str(), meta.title.as_str(), meta.kbps), ("Test FM", "Test Song", 128));
    }
}
