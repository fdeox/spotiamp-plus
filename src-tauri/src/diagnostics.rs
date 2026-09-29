//! "Copy diagnostic info": one click puts what a bug report needs on the
//! clipboard (version, Windows build, mode, audio setup, the error, and the
//! recent warnings and errors from spotiamp.log), ready to paste into /bug on
//! Discord or a GitHub issue.
//!
//! It's meant to be pasted in public, so it's scrubbed first: the Spotify
//! username, the Windows user name, e-mail addresses and file paths (only the
//! file's extension is kept, that's what matters for playback bugs).

use crate::settings::Settings;

#[tauri::command]
pub fn copy_diagnostics(error: Option<String>) -> Result<(), String> {
    let text = build(error.as_deref());
    write_clipboard(&text)
}

fn build(error: Option<&str>) -> String {
    let settings = Settings::current().clone();
    let player = &settings.player;
    let mut out = String::new();
    out.push_str("Spotiamp+ diagnostic info\n");
    out.push_str(&format!("Version: {}\n", env!("CARGO_PKG_VERSION")));
    out.push_str(&format!("Windows: {}\n", windows_version()));
    out.push_str(&format!(
        "Mode: {}\n",
        if settings.controller_mode { "Free Mode (drives the Spotify app)" } else { "Premium (built-in playback)" }
    ));
    let devices = crate::spotify::list_output_devices();
    let device_state = match &player.audio_device {
        None => "system default".to_string(),
        Some(name) if devices.iter().any(|d| d == name) => format!("\"{name}\""),
        Some(name) => format!("\"{name}\" (not connected, using the default)"),
    };
    out.push_str(&format!(
        "Audio output: {device_state}, {} device(s) available\n",
        devices.len()
    ));
    out.push_str(&format!(
        "Skin: {}, UI scale: {}%, normalize: {}, taskbar extras: {}\n",
        if settings.skin.is_empty() { "classic" } else { &settings.skin },
        player.ui_scale_pct.unwrap_or(100),
        if player.normalize { "on" } else { "off" },
        if player.taskbar_extras { "on" } else { "off" },
    ));
    if let Some(error) = error.filter(|e| !e.trim().is_empty()) {
        out.push_str("\nError shown:\n");
        out.push_str(error.trim());
        out.push('\n');
    }

    let log = read_log();
    let spotify_user = log
        .lines()
        .rev()
        .find_map(|l| l.split("Authenticated as '").nth(1)?.split('\'').next().map(str::to_string));
    let recent: Vec<&str> = log
        .lines()
        .filter(|l| l.contains(" WARN ") || l.contains(" ERROR "))
        .collect();
    out.push_str("\nRecent warnings and errors:\n");
    if recent.is_empty() {
        out.push_str("(none)\n");
    }
    for line in recent.iter().skip(recent.len().saturating_sub(25)) {
        let line: String = line.chars().take(400).collect();
        out.push_str(&line);
        out.push('\n');
    }
    scrub(&out, spotify_user.as_deref())
}

/// The current log plus the previous one (it rotates at ~2 MB), oldest first.
fn read_log() -> String {
    let Some(dir) = crate::settings::get_config_dir() else {
        return String::new();
    };
    let read = |name: &str| {
        std::fs::read(dir.join(name))
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default()
    };
    let mut log = read("spotiamp.log.old");
    log.push_str(&read("spotiamp.log"));
    log
}

fn scrub(text: &str, spotify_user: Option<&str>) -> String {
    let mut text = text.to_string();
    // File paths first (they may contain the Windows user name too).
    text = scrub_paths(&text);
    if let Ok(profile) = std::env::var("USERPROFILE") {
        text = replace_ignore_case(&text, &profile, "%USERPROFILE%");
    }
    if let Ok(user) = std::env::var("USERNAME") {
        if user.len() >= 3 {
            text = replace_ignore_case(&text, &user, "<user>");
        }
    }
    if let Some(user) = spotify_user.filter(|u| u.len() >= 3) {
        text = replace_ignore_case(&text, user, "<spotify-user>");
    }
    // E-mail-looking words.
    text.split_inclusive(|c: char| c.is_whitespace() || c == '\'' || c == '"')
        .map(|word| {
            let core = word.trim_end_matches(|c: char| c.is_whitespace() || c == '\'' || c == '"');
            match core.split_once('@') {
                Some((name, domain)) if !name.is_empty() && domain.contains('.') => {
                    word.replacen(core, "<email>", 1)
                }
                _ => word.to_string(),
            }
        })
        .collect()
}

/// `C:\Music\Some Artist\song.flac` → `<file .flac>`. A path runs from its
/// drive letter to the next quote, bracket or line end.
fn scrub_paths(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let is_drive = i + 2 < chars.len()
            && chars[i].is_ascii_alphabetic()
            && chars[i + 1] == ':'
            && (chars[i + 2] == '\\' || chars[i + 2] == '/')
            && (i == 0 || !chars[i - 1].is_alphanumeric());
        if !is_drive {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let start = i;
        i += 3;
        // A Windows path can't hold another ':' after the drive, so that (as in
        // "C:\x.mp3: decode error") ends it too.
        while i < chars.len() && !matches!(chars[i], '"' | '\'' | '\n' | ')' | ']' | '>' | '`' | ':') {
            i += 1;
        }
        let path: String = chars[start..i].iter().collect();
        let path = path.trim_end();
        let tail = path.len();
        let name = path.rsplit(['\\', '/']).next().unwrap_or("");
        let ext = name
            .rsplit_once('.')
            .map(|(_, e)| e)
            .filter(|e| !e.is_empty() && e.len() <= 5 && e.chars().all(|c| c.is_ascii_alphanumeric()));
        match ext {
            Some(ext) => out.push_str(&format!("<file .{}>", ext.to_ascii_lowercase())),
            None => out.push_str("<path>"),
        }
        // keep whatever whitespace the trim dropped
        let consumed: String = chars[start..i].iter().collect();
        out.push_str(&consumed[tail..]);
    }
    out
}

fn replace_ignore_case(text: &str, needle: &str, with: &str) -> String {
    if needle.is_empty() {
        return text.to_string();
    }
    let lower = text.to_lowercase();
    let needle_lower = needle.to_lowercase();
    // Lowercasing can change byte lengths for some letters; fall back to an
    // exact-case replace then rather than risk cutting a character in half.
    if lower.len() != text.len() || needle_lower.len() != needle.len() {
        return text.replace(needle, with);
    }
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (idx, _) in lower.match_indices(&needle_lower) {
        out.push_str(&text[last..idx]);
        out.push_str(with);
        last = idx + needle.len();
    }
    out.push_str(&text[last..]);
    out
}

#[cfg(target_os = "windows")]
fn windows_version() -> String {
    use windows::core::{HSTRING, w};
    use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};

    let read = |name: &str| -> Option<String> {
        let mut buf = [0u16; 128];
        let mut size = (buf.len() * 2) as u32;
        unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion"),
                &HSTRING::from(name),
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr() as *mut _),
                Some(&mut size),
            )
            .ok()
            .ok()?;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..len]))
    };
    let build: u32 = read("CurrentBuildNumber").and_then(|b| b.parse().ok()).unwrap_or(0);
    // ProductName still says "Windows 10" on 11; the build number tells them apart.
    let name = if build >= 22000 { "Windows 11" } else { "Windows 10" };
    let release = read("DisplayVersion").unwrap_or_default();
    format!("{name} {release} (build {build})").replace("  ", " ")
}

#[cfg(not(target_os = "windows"))]
fn windows_version() -> String {
    std::env::consts::OS.to_string()
}

#[cfg(target_os = "windows")]
fn write_clipboard(text: &str) -> Result<(), String> {
    use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
    const CF_UNICODETEXT: u32 = 13;

    let wide: Vec<u16> = text.replace('\n', "\r\n").encode_utf16().chain(Some(0)).collect();
    unsafe {
        OpenClipboard(None).map_err(|e| format!("clipboard busy ({e})"))?;
        let result = (|| -> Result<(), String> {
            EmptyClipboard().map_err(|e| e.to_string())?;
            let mem: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2).map_err(|e| e.to_string())?;
            let ptr = GlobalLock(mem) as *mut u16;
            if ptr.is_null() {
                let _ = GlobalFree(Some(mem));
                return Err("couldn't lock clipboard memory".into());
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
            let _ = GlobalUnlock(mem);
            // On success the clipboard owns the memory.
            if let Err(e) = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(mem.0))) {
                let _ = GlobalFree(Some(mem));
                return Err(e.to_string());
            }
            Ok(())
        })();
        let _ = CloseClipboard();
        result
    }
}

#[cfg(not(target_os = "windows"))]
fn write_clipboard(_text: &str) -> Result<(), String> {
    Err("not supported on this platform".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_keep_only_the_extension() {
        assert_eq!(
            scrub_paths("open failed for 'D:\\Music\\My Band\\01 song.FLAC': no device"),
            "open failed for '<file .flac>': no device"
        );
        assert_eq!(scrub_paths("dir C:\\Users\\x\\AppData\n"), "dir <path>\n");
        assert_eq!(scrub_paths("ratio 3:2 and http://a"), "ratio 3:2 and http://a");
        assert_eq!(
            scrub_paths("C:\\Users\\x\\Music\\a b.mp3: decode error"),
            "<file .mp3>: decode error"
        );
    }

    #[test]
    fn emails_and_names_are_hidden() {
        let s = scrub("user bob@example.com said hi to Alice\n", Some("alice"));
        assert_eq!(s, "user <email> said hi to <spotify-user>\n");
    }
}
