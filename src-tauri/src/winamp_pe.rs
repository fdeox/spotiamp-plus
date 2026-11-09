//! Winamp 5's playlist editor list, painted the way Winamp painted it
//! (draw_pe.cpp: draw_pl2 and draw_pe_song). Windows draws every row with the
//! playlist font through the same calls, so the rows come out as Winamp's did
//! on this machine: the font, its smoothing, the "..." of a long name, the
//! length to the right. Read for its behaviour; none of its code is used.
//!
//! The list is painted on its own: the window's x 12 to width - 20 and y 20
//! to height - 38, where Winamp drew it. A modern skin's playlist is that
//! window held 10 px left of, 20 px above, 5 px right of and 38 px below its
//! place (gen_ff's region offsets); a classic skin's is the whole window.

use serde::Deserialize;

/// A row as the playlist gives it.
#[derive(Deserialize)]
pub struct PeRow {
    /// as shown: "<number>. <title>"
    pub name: String,
    /// seconds; -1 when not known (no length drawn)
    #[serde(default = "minus_one")]
    pub time: i32,
    /// 1 selected, 2 the playing one (both: 3)
    #[serde(default)]
    pub sel: u8,
    /// a text colour of its own (Spotiamp+'s greyed rows), else the list's
    #[serde(default)]
    pub fg: Option<[u8; 3]>,
}

fn minus_one() -> i32 {
    -1
}

/// What to paint: the list's size, its rows from the first one shown on (more
/// than fit is fine) and Winamp's playlist font and colours.
#[derive(Deserialize)]
pub struct PePaint {
    /// the playlist font (Winamp's default: Arial)
    pub face: String,
    /// and its size (config_pe_fontsize, 12 by default)
    pub size: i32,
    /// the list's width (the window's width - 32) and height (- 58)
    pub w: i32,
    pub h: i32,
    pub rows: Vec<PeRow>,
    /// normal text, the playing row's text, background, selected background
    pub colors: [[u8; 3]; 4],
}

/// The painted list, opaque RGBA top row first, with the row height (the
/// font's) and how many rows fit (Winamp's num_songs).
pub struct PePainted {
    pub w: i32,
    pub h: i32,
    pub row_h: i32,
    pub rows: i32,
    pub rgba: Vec<u8>,
}

impl PePainted {
    /// [w, h, row height, rows as little-endian i32] then the pixels
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(16 + self.rgba.len());
        for v in [self.w, self.h, self.row_h, self.rows] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&self.rgba);
        out
    }
}

/// " m:ss ", as Winamp formats a row's length.
pub fn length_text(time: i32) -> String {
    format!(" {}:{:02} ", time / 60, time % 60)
}

#[cfg(windows)]
pub fn paint(p: &PePaint) -> Option<PePainted> {
    use std::ffi::c_void;
    use windows::Win32::Foundation::{COLORREF, RECT};
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CLIP_DEFAULT_PRECIS, CreateCompatibleDC, CreateDIBSection, CreateFontW,
        CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, DRAFT_QUALITY, DRAW_TEXT_FORMAT, DT_CALCRECT,
        DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject, DrawTextW,
        FF_DONTCARE, FW_NORMAL, GdiFlush, GetStockObject, GetTextMetricsW, HBRUSH, HDC, HGDIOBJ, NULL_PEN,
        OUT_DEFAULT_PRECIS, Rectangle, SelectObject, SetBkColor, SetTextColor, SetViewportOrgEx, TEXTMETRICW,
    };
    use windows::core::PCWSTR;

    if p.w <= 0 || p.h <= 0 || p.w > 8192 || p.h > 8192 || p.size <= 0 || p.size > 200 {
        return None;
    }
    let rgb = |c: [u8; 3]| COLORREF(c[0] as u32 | (c[1] as u32) << 8 | (c[2] as u32) << 16);
    // the window Winamp drew in
    let width = p.w + 32;
    let height = p.h + 58;
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: p.w,
            biHeight: -p.h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    unsafe {
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return None;
        }
        let mut bits: *mut c_void = std::ptr::null_mut();
        let Ok(bitmap) = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
            let _ = DeleteDC(dc);
            return None;
        };
        let old_bitmap = SelectObject(dc, HGDIOBJ(bitmap.0));
        let face: Vec<u16> = p.face.encode_utf16().chain(std::iter::once(0)).collect();
        // (draw_reinit_plfont: DRAFT_QUALITY, so Windows' own smoothing)
        let font = CreateFontW(
            -p.size,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            DRAFT_QUALITY,
            (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
            PCWSTR(face.as_ptr()),
        );
        let old_font = SelectObject(dc, HGDIOBJ(font.0));
        let mut tm = TEXTMETRICW::default();
        let _ = GetTextMetricsW(dc, &mut tm);
        let row_h = tm.tmHeight.max(1);
        // the window's own coordinates from here on
        let _ = SetViewportOrgEx(dc, -12, -20, None);
        let normbrush = CreateSolidBrush(rgb(p.colors[2]));
        let selbrush = CreateSolidBrush(rgb(p.colors[3]));
        let null_pen = GetStockObject(NULL_PEN);
        let num_songs = (height - 38 - 20 - 2) / row_h;
        let shown = (p.rows.len() as i32).min(num_songs.max(0));
        let song = |dc: HDC, pos: i32, name: &str, time: i32, sel: u8, own: Option<[u8; 3]>| {
            let fg = own.unwrap_or(if sel & 2 != 0 { p.colors[1] } else { p.colors[0] });
            let bg = if sel & 1 != 0 { p.colors[3] } else { p.colors[2] };
            let brush: HBRUSH = if sel & 1 != 0 { selbrush } else { normbrush };
            let (left, right, ypos) = (12, width - 20, 22 + pos * row_h);
            let mut endp = right;
            let mut r = RECT { left, top: ypos, right: endp, bottom: ypos + row_h };
            SetTextColor(dc, rgb(fg));
            SetBkColor(dc, rgb(bg));
            if time >= 0 {
                let mut text: Vec<u16> = length_text(time).encode_utf16().collect();
                let mut r2 = r;
                DrawTextW(dc, &mut text, &mut r2, DT_VCENTER | DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX);
                let tw = r2.right - r2.left;
                r2.top = r.top;
                r2.bottom = r.bottom;
                if tw < endp - left {
                    r2.left = endp - tw;
                    r2.right = endp;
                    r.right -= tw;
                    endp -= tw;
                    DrawTextW(dc, &mut text, &mut r2, DT_VCENTER | DT_SINGLELINE | DT_RIGHT | DT_NOPREFIX);
                }
            }
            let mut text: Vec<u16> = name.encode_utf16().collect();
            let flags: DRAW_TEXT_FORMAT = DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX;
            DrawTextW(dc, &mut text, &mut r, flags);
            DrawTextW(dc, &mut text, &mut r, flags | DT_CALCRECT);
            let old_brush = SelectObject(dc, HGDIOBJ(brush.0));
            let old_pen = SelectObject(dc, null_pen);
            let _ = Rectangle(dc, r.right, r.top, 1 + endp, 1 + ypos + row_h);
            if pos == 0 || pos == num_songs - 1 {
                SelectObject(dc, HGDIOBJ(normbrush.0));
                if pos == 0 {
                    let _ = Rectangle(dc, left, 20, 1 + right, 1 + 23);
                }
                if pos == num_songs - 1 {
                    let _ = Rectangle(dc, left, ypos + row_h, 1 + right, height - 38 + 1);
                }
            }
            SelectObject(dc, old_pen);
            SelectObject(dc, old_brush);
        };
        for (pos, row) in p.rows.iter().take(shown as usize).enumerate() {
            song(dc, pos as i32, &row.name, row.time, row.sel, row.fg);
        }
        for pos in shown..num_songs {
            song(dc, pos, " ", -1, 0, None);
        }
        let _ = GdiFlush();
        let src = std::slice::from_raw_parts(bits as *const u8, (p.w * p.h * 4) as usize);
        let mut rgba = Vec::with_capacity(src.len());
        for px in src.chunks_exact(4) {
            rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
        }
        SelectObject(dc, old_font);
        SelectObject(dc, old_bitmap);
        let _ = DeleteObject(HGDIOBJ(font.0));
        let _ = DeleteObject(HGDIOBJ(normbrush.0));
        let _ = DeleteObject(HGDIOBJ(selbrush.0));
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(dc);
        Some(PePainted { w: p.w, h: p.h, row_h, rows: num_songs.max(0), rgba })
    }
}

#[cfg(not(windows))]
pub fn paint(_p: &PePaint) -> Option<PePainted> {
    None
}

/// A line of text the way Winamp 5's main window wrote the song's title
/// when it wasn't the skin's bitmap font (draw_main.cpp's
/// _draw_songname_winfont): DrawText, top left, in the playlist font at
/// -size, its colour on the skin's background.
#[derive(Deserialize)]
pub struct LinePaint {
    pub face: String,
    pub size: i32,
    pub text: String,
    pub fg: [u8; 3],
    pub bg: [u8; 3],
}

/// The line, as wide as its text and as high as the font (opaque RGBA).
pub struct LinePainted {
    pub w: i32,
    pub h: i32,
    pub rgba: Vec<u8>,
}

impl LinePainted {
    /// [w, h, 0, 0 as little-endian i32] then the pixels
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(16 + self.rgba.len());
        for v in [self.w, self.h, 0, 0] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&self.rgba);
        out
    }
}

#[cfg(windows)]
pub fn paint_line(p: &LinePaint) -> Option<LinePainted> {
    use std::ffi::c_void;
    use windows::Win32::Foundation::{COLORREF, RECT};
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CLIP_DEFAULT_PRECIS, CreateCompatibleDC, CreateDIBSection, CreateFontW,
        CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, DRAFT_QUALITY, DT_CALCRECT, DT_LEFT,
        DT_NOPREFIX, DT_SINGLELINE, DeleteDC, DeleteObject, DrawTextW, FF_DONTCARE, FW_NORMAL, FillRect, GdiFlush,
        GetTextMetricsW, HGDIOBJ, OUT_DEFAULT_PRECIS, SelectObject, SetBkColor, SetTextColor, TEXTMETRICW,
    };
    use windows::core::PCWSTR;

    if p.size <= 0 || p.size > 200 || p.text.chars().count() > 4096 {
        return None;
    }
    let rgb = |c: [u8; 3]| COLORREF(c[0] as u32 | (c[1] as u32) << 8 | (c[2] as u32) << 16);
    unsafe {
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return None;
        }
        let face: Vec<u16> = p.face.encode_utf16().chain(std::iter::once(0)).collect();
        let font = CreateFontW(
            -p.size,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            DRAFT_QUALITY,
            (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
            PCWSTR(face.as_ptr()),
        );
        let old_font = SelectObject(dc, HGDIOBJ(font.0));
        let mut tm = TEXTMETRICW::default();
        let _ = GetTextMetricsW(dc, &mut tm);
        let h = tm.tmHeight.max(1);
        let mut text: Vec<u16> = p.text.encode_utf16().collect();
        let mut r = RECT::default();
        if !text.is_empty() {
            DrawTextW(dc, &mut text, &mut r, DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX);
        }
        let w = (r.right - r.left).clamp(1, 32767);
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        let Ok(bitmap) = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
            SelectObject(dc, old_font);
            let _ = DeleteObject(HGDIOBJ(font.0));
            let _ = DeleteDC(dc);
            return None;
        };
        let old_bitmap = SelectObject(dc, HGDIOBJ(bitmap.0));
        let brush = CreateSolidBrush(rgb(p.bg));
        let all = RECT { left: 0, top: 0, right: w, bottom: h };
        FillRect(dc, &all, brush);
        SetTextColor(dc, rgb(p.fg));
        SetBkColor(dc, rgb(p.bg));
        let mut at = all;
        if !text.is_empty() {
            DrawTextW(dc, &mut text, &mut at, DT_LEFT | DT_SINGLELINE | DT_NOPREFIX);
        }
        let _ = GdiFlush();
        let src = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize);
        let mut rgba = Vec::with_capacity(src.len());
        for px in src.chunks_exact(4) {
            rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
        }
        SelectObject(dc, old_bitmap);
        SelectObject(dc, old_font);
        let _ = DeleteObject(HGDIOBJ(brush.0));
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteObject(HGDIOBJ(font.0));
        let _ = DeleteDC(dc);
        Some(LinePainted { w, h, rgba })
    }
}

#[cfg(not(windows))]
pub fn paint_line(_p: &LinePaint) -> Option<LinePainted> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_as_winamp_wrote_them() {
        assert_eq!(length_text(211), " 3:31 ");
        assert_eq!(length_text(4500), " 75:00 ");
        assert_eq!(length_text(5), " 0:05 ");
    }

    #[cfg(windows)]
    #[test]
    fn a_line_is_as_wide_as_its_text() {
        let line = |text: &str| {
            paint_line(&LinePaint { face: "Arial".into(), size: 10, text: text.into(), fg: [0, 255, 0], bg: [0, 0, 0] }).expect("painted")
        };
        let short = line("1. A");
        let long = line("1. A longer title (3:31)");
        assert!(long.w > short.w);
        assert_eq!(short.h, long.h);
        assert_eq!(short.rgba.len(), (short.w * short.h * 4) as usize);
    }

    #[cfg(windows)]
    #[test]
    fn rows_fit_the_list_as_in_winamp() {
        let p = PePaint {
            face: "Arial".into(),
            size: 12,
            w: 407,
            h: 99,
            rows: vec![PeRow { name: "1. A".into(), time: 211, sel: 2, fg: None }],
            colors: [[255, 255, 255], [0, 255, 0], [0, 0, 0], [0, 0, 198]],
        };
        let painted = paint(&p).expect("painted");
        assert_eq!((painted.w, painted.h), (407, 99));
        assert!(painted.row_h > 0);
        assert_eq!(painted.rows, (99 - 2) / painted.row_h);
        assert_eq!(painted.rgba.len(), 407 * 99 * 4);
    }
}
