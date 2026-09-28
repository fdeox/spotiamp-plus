// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Spotiamp+ opens several small frameless windows; by default WebView2 gives
    // each its own renderer/GPU/utility processes, which balloons memory (a
    // handful of ~100 MB Chromium processes for a Winamp-style app). Collapse
    // them into one renderer and cap the JS heap — the windows are lightweight,
    // so the trade-off is invisible but the memory drop is large.
    // One shared renderer is the big memory win. The JS-heap cap stays generous
    // (256 MB) — 96 MB was tight enough that the library/search could OOM-crash
    // the shared renderer into a broken "!" state. Anything already in the
    // variable (e.g. --remote-debugging-port while testing) is kept after ours
    // instead of being thrown away.
    const ARGS: &str = "--renderer-process-limit=1 --js-flags=--max-old-space-size=256 \
         --disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,Translate";
    let args = match std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS") {
        Ok(extra) if !extra.trim().is_empty() => format!("{ARGS} {extra}"),
        _ => ARGS.to_string(),
    };
    unsafe {
        std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", args);
    }

    spotiamp_lib::init_logging();
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("A crypto provider");
    spotiamp_lib::run();
}
