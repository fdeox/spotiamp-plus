//! Controller ("free") mode bridge to the Windows media session (SMTC).
//!
//! librespot cannot serve non-Premium accounts at all — the server's
//! ProductInfo packet makes it call exit(1) the moment a session connects —
//! so this mode never opens a librespot session. Instead it reads what the
//! official Spotify app is playing and drives its transport through the same
//! system media session that the volume-overlay media keys use. No Premium
//! required, because Spotiamp+ isn't the one streaming.
//!
//! Every call is best-effort: if there is no session (Spotify closed, or a
//! non-Windows build) the commands report "nothing playing" or false rather
//! than erroring, and the UI shows an idle player.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
pub struct NowPlaying {
    /// False when no media session exists at all (e.g. Spotify isn't running).
    pub available: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub playing: bool,
}

#[cfg(target_os = "windows")]
mod imp {
    use super::NowPlaying;
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSession as MediaSession,
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
    };

    /// Prefer Spotify's session when several apps hold one (a browser playing
    /// a video would otherwise win "current"), falling back to whatever
    /// Windows considers current.
    fn session() -> Option<MediaSession> {
        let manager = Manager::RequestAsync().ok()?.get().ok()?;
        if let Ok(sessions) = manager.GetSessions() {
            for candidate in &sessions {
                if let Ok(id) = candidate.SourceAppUserModelId()
                    && id.to_string().to_lowercase().contains("spotify")
                {
                    return Some(candidate);
                }
            }
        }
        manager.GetCurrentSession().ok()
    }

    pub fn now_playing() -> NowPlaying {
        let Some(session) = session() else {
            return NowPlaying::default();
        };
        let mut out = NowPlaying {
            available: true,
            ..Default::default()
        };
        if let Ok(operation) = session.TryGetMediaPropertiesAsync()
            && let Ok(props) = operation.get()
        {
            out.title = props.Title().map(|s| s.to_string()).unwrap_or_default();
            out.artist = props.Artist().map(|s| s.to_string()).unwrap_or_default();
            out.album = props
                .AlbumTitle()
                .map(|s| s.to_string())
                .unwrap_or_default();
        }
        if let Ok(info) = session.GetPlaybackInfo() {
            out.playing = info
                .PlaybackStatus()
                .map(|status| status == PlaybackStatus::Playing)
                .unwrap_or(false);
        }
        if let Ok(timeline) = session.GetTimelineProperties() {
            // TimeSpan durations are in 100ns ticks.
            let to_ms = |t: windows::Foundation::TimeSpan| (t.Duration / 10_000).max(0) as u64;
            out.position_ms = timeline.Position().map(to_ms).unwrap_or(0);
            out.duration_ms = timeline.EndTime().map(to_ms).unwrap_or(0);

            // Spotify only refreshes the timeline every few seconds, so the
            // raw Position is stale most of the time — shown as-is it sticks,
            // then leaps. LastUpdatedTime says when Position was captured;
            // while playing, extrapolate to now for a smooth clock.
            if out.playing
                && let Ok(updated) = timeline.LastUpdatedTime()
            {
                // DateTime counts 100ns ticks since 1601-01-01 UTC; Unix time
                // starts 11644473600s later.
                if let Ok(since_epoch) =
                    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                {
                    let now_ticks =
                        (since_epoch.as_millis() as i64) * 10_000 + 11_644_473_600_000 * 10_000;
                    let elapsed_ms = (now_ticks - updated.UniversalTime) / 10_000;
                    if elapsed_ms > 0 {
                        out.position_ms = (out.position_ms + elapsed_ms as u64)
                            .min(out.duration_ms.max(out.position_ms));
                    }
                }
            }
        }
        out
    }

    pub fn play() -> bool {
        session()
            .and_then(|s| s.TryPlayAsync().ok()?.get().ok())
            .unwrap_or(false)
    }

    pub fn pause() -> bool {
        session()
            .and_then(|s| s.TryPauseAsync().ok()?.get().ok())
            .unwrap_or(false)
    }

    pub fn next() -> bool {
        session()
            .and_then(|s| s.TrySkipNextAsync().ok()?.get().ok())
            .unwrap_or(false)
    }

    pub fn previous() -> bool {
        session()
            .and_then(|s| s.TrySkipPreviousAsync().ok()?.get().ok())
            .unwrap_or(false)
    }

    pub fn seek(position_ms: u64) -> bool {
        session()
            .and_then(|s| {
                s.TryChangePlaybackPositionAsync((position_ms as i64) * 10_000)
                    .ok()?
                    .get()
                    .ok()
            })
            .unwrap_or(false)
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::NowPlaying;
    pub fn now_playing() -> NowPlaying {
        NowPlaying::default()
    }
    pub fn play() -> bool {
        false
    }
    pub fn pause() -> bool {
        false
    }
    pub fn next() -> bool {
        false
    }
    pub fn previous() -> bool {
        false
    }
    pub fn seek(_position_ms: u64) -> bool {
        false
    }
}

// The WinRT calls block on .get(), so every command hops onto the blocking
// pool instead of stalling an async-runtime worker.

#[tauri::command]
pub async fn smtc_now_playing() -> NowPlaying {
    tauri::async_runtime::spawn_blocking(imp::now_playing)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub async fn smtc_play() -> bool {
    tauri::async_runtime::spawn_blocking(imp::play)
        .await
        .unwrap_or(false)
}

#[tauri::command]
pub async fn smtc_pause() -> bool {
    tauri::async_runtime::spawn_blocking(imp::pause)
        .await
        .unwrap_or(false)
}

#[tauri::command]
pub async fn smtc_next() -> bool {
    tauri::async_runtime::spawn_blocking(imp::next)
        .await
        .unwrap_or(false)
}

#[tauri::command]
pub async fn smtc_previous() -> bool {
    tauri::async_runtime::spawn_blocking(imp::previous)
        .await
        .unwrap_or(false)
}

#[tauri::command]
pub async fn smtc_seek(position_ms: u64) -> bool {
    tauri::async_runtime::spawn_blocking(move || imp::seek(position_ms))
        .await
        .unwrap_or(false)
}
