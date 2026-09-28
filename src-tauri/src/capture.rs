//! Capture a window's rendered page as a PNG, for the Now Playing card: the
//! real player exactly as it looks right now (skin, ticker, time, visualizer).

/// PNG bytes of the given window's page, via WebView2's own CapturePreview.
#[cfg(target_os = "windows")]
#[tauri::command]
pub async fn capture_window_png(
    label: String,
    app: tauri::AppHandle,
) -> Result<tauri::ipc::Response, String> {
    use tauri::Manager;
    use webview2_com::CapturePreviewCompletedHandler;
    use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG;
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::Com::StructuredStorage::{
        CreateStreamOnHGlobal, GetHGlobalFromStream,
    };
    use windows::Win32::System::Com::{STATFLAG_NONAME, STATSTG};
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    let window = app
        .get_webview_window(&label)
        .ok_or_else(|| format!("no window '{label}'"))?;
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<Vec<u8>, String>>();

    window
        .with_webview(move |webview| unsafe {
            let mut tx = Some(tx);
            let started = (|| -> windows::core::Result<()> {
                let core = webview.controller().CoreWebView2()?;
                let stream = CreateStreamOnHGlobal(HGLOBAL::default(), true)?;
                let written = stream.clone();
                let reply = tx.take();
                // WebView2 fills the stream asynchronously, then calls this back
                // on the UI thread; copy the PNG out and hand it to the command.
                let handler = CapturePreviewCompletedHandler::create(Box::new(move |result| {
                    let bytes = result.map_err(|e| e.to_string()).and_then(|()| {
                        let mut stat = STATSTG::default();
                        written
                            .Stat(&mut stat, STATFLAG_NONAME)
                            .map_err(|e| e.to_string())?;
                        let hglobal = GetHGlobalFromStream(&written).map_err(|e| e.to_string())?;
                        // The allocation can be larger than what was written.
                        let len = (stat.cbSize as usize).min(GlobalSize(hglobal));
                        let ptr = GlobalLock(hglobal) as *const u8;
                        if ptr.is_null() || len == 0 {
                            return Err("the capture came back empty".to_string());
                        }
                        let bytes = std::slice::from_raw_parts(ptr, len).to_vec();
                        let _ = GlobalUnlock(hglobal);
                        Ok(bytes)
                    });
                    if let Some(reply) = reply {
                        let _ = reply.send(bytes);
                    }
                    Ok(())
                }));
                core.CapturePreview(COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG, &stream, &handler)
            })();
            if let (Err(e), Some(tx)) = (started, tx.take()) {
                let _ = tx.send(Err(e.to_string()));
            }
        })
        .map_err(|e| e.to_string())?;

    let bytes = rx
        .await
        .map_err(|_| "the capture didn't complete".to_string())??;
    Ok(tauri::ipc::Response::new(bytes))
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub async fn capture_window_png(_label: String) -> Result<tauri::ipc::Response, String> {
    Err("capturing a window isn't supported on this platform yet".into())
}
