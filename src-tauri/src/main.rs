// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Spotiamp+ opens several small frameless windows; by default WebView2 gives
    // each its own renderer/GPU/utility processes, which balloons memory (a
    // handful of ~100 MB Chromium processes for a Winamp-style app). Collapse
    // them into one renderer and cap the JS heap — the windows are lightweight,
    // so the trade-off is invisible but the memory drop is large.
    unsafe {
        std::env::set_var(
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
            "--renderer-process-limit=1 --js-flags=--max-old-space-size=96 \
             --disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,Translate",
        );
    }

    env_logger::init();
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("A crypto provider");
    spotiamp_lib::run();
}
