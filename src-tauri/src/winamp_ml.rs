//! Winamp 5's Media Library as its controls drew it (gen_ml: the skinned
//! tree, list, header, buttons and labels). The page lays out each control
//! and hands over what it draws in order: solid fills, pictures (the skin's
//! genex.bmp parts) and text. Windows writes the text in the skin's font the
//! way gen_ml had it written (the playlist font, -12 by default,
//! DRAFT_QUALITY, through DrawText), so the letters come out as Winamp's did
//! on this machine. Read for its behaviour; none of Winamp's code is used.

use serde::{Deserialize, Serialize};

/// A font: the skin's (the playlist font) unless the page asks otherwise.
#[derive(Deserialize, Clone)]
pub struct MlFont {
    pub face: String,
    /// character height in pixels (CreateFont's -height)
    pub size: i32,
    #[serde(default)]
    pub bold: bool,
}

/// One thing drawn, in order.
#[derive(Deserialize)]
#[serde(tag = "k")]
pub enum MlOp {
    /// a solid rectangle
    #[serde(rename = "f")]
    Fill { x: i32, y: i32, w: i32, h: i32, c: [u8; 3] },
    /// an opaque picture: w * h RGBA pixels, base64
    #[serde(rename = "i")]
    Img { x: i32, y: i32, w: i32, h: i32, d: String },
    /// text in a rectangle with DrawText's flags (DT_*), on what's there
    #[serde(rename = "t")]
    Text {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        t: String,
        c: [u8; 3],
        #[serde(default)]
        f: u32,
        #[serde(default)]
        font: usize,
        /// drawn only inside this rectangle [x, y, w, h]
        #[serde(default)]
        clip: Option<[i32; 4]>,
    },
}

#[derive(Deserialize)]
pub struct MlPaint {
    pub w: i32,
    pub h: i32,
    pub fonts: Vec<MlFont>,
    pub ops: Vec<MlOp>,
}

/// The painted area: [w, h] as little-endian i32 then RGBA, top row first;
/// what no op touched is see-through.
pub struct MlPainted {
    pub w: i32,
    pub h: i32,
    pub rgba: Vec<u8>,
}

impl MlPainted {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + self.rgba.len());
        out.extend_from_slice(&self.w.to_le_bytes());
        out.extend_from_slice(&self.h.to_le_bytes());
        out.extend_from_slice(&self.rgba);
        out
    }
}

/// Texts to measure, each with its font's index.
#[derive(Deserialize)]
pub struct MlMeasure {
    pub fonts: Vec<MlFont>,
    pub texts: Vec<(String, usize)>,
}

/// Each font's [height, ascent, average width, external leading] and each
/// text's [width, height] as DrawText measures one line.
#[derive(Serialize, Default)]
pub struct MlMeasured {
    pub metrics: Vec<[i32; 4]>,
    pub sizes: Vec<[i32; 2]>,
}

/// A colour no op draws by chance: what's left of it after painting is
/// what no op covered.
const KEY: [u8; 3] = [255, 0, 254];

#[cfg(windows)]
mod gdi {
    use super::MlFont;
    use windows::Win32::Graphics::Gdi::{
        CLIP_DEFAULT_PRECIS, CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH, DRAFT_QUALITY, FF_DONTCARE, FW_BOLD,
        FW_NORMAL, HFONT, OUT_DEFAULT_PRECIS,
    };
    use windows::core::PCWSTR;

    /// gen_ml's skin font (stockobjects.cpp): CreateFont(-height, ...,
    /// DRAFT_QUALITY, the playlist font's name)
    pub fn font(f: &MlFont) -> HFONT {
        let face: Vec<u16> = f.face.encode_utf16().chain(std::iter::once(0)).collect();
        let size = f.size.clamp(1, 200);
        unsafe {
            CreateFontW(
                -size,
                0,
                0,
                0,
                if f.bold { FW_BOLD.0 as i32 } else { FW_NORMAL.0 as i32 },
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                DRAFT_QUALITY,
                (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
                PCWSTR(face.as_ptr()),
            )
        }
    }
}

#[cfg(windows)]
pub fn paint(p: &MlPaint) -> Option<MlPainted> {
    use base64::Engine;
    use std::ffi::c_void;
    use windows::Win32::Foundation::{COLORREF, RECT};
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, CreateSolidBrush, DIB_RGB_COLORS,
        DRAW_TEXT_FORMAT, DT_NOPREFIX, DeleteDC, DeleteObject, DrawTextW, FillRect, GdiFlush, HGDIOBJ,
        IntersectClipRect, RestoreDC, SaveDC, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
    };

    if p.w <= 0 || p.h <= 0 || p.w > 8192 || p.h > 8192 || p.fonts.is_empty() || p.fonts.len() > 8 {
        return None;
    }
    let rgb = |c: [u8; 3]| COLORREF(c[0] as u32 | (c[1] as u32) << 8 | (c[2] as u32) << 16);
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
        let fonts: Vec<_> = p.fonts.iter().map(gdi::font).collect();
        let old_font = SelectObject(dc, HGDIOBJ(fonts[0].0));
        let fill = |x: i32, y: i32, w: i32, h: i32, c: [u8; 3]| {
            let brush = CreateSolidBrush(rgb(c));
            let r = RECT { left: x, top: y, right: x + w, bottom: y + h };
            FillRect(dc, &r, brush);
            let _ = DeleteObject(HGDIOBJ(brush.0));
        };
        fill(0, 0, p.w, p.h, KEY);
        SetBkMode(dc, TRANSPARENT);
        let px = bits as *mut u8;
        for op in &p.ops {
            match op {
                MlOp::Fill { x, y, w, h, c } => fill(*x, *y, *w, *h, *c),
                MlOp::Img { x, y, w, h, d } => {
                    let Ok(data) = base64::engine::general_purpose::STANDARD.decode(d) else {
                        continue;
                    };
                    if *w <= 0 || *h <= 0 || data.len() < (*w * *h * 4) as usize {
                        continue;
                    }
                    let _ = GdiFlush();
                    for row in 0..*h {
                        let ty = y + row;
                        if ty < 0 || ty >= p.h {
                            continue;
                        }
                        for col in 0..*w {
                            let tx = x + col;
                            if tx < 0 || tx >= p.w {
                                continue;
                            }
                            let s = ((row * w + col) * 4) as usize;
                            if data[s + 3] == 0 {
                                continue;
                            }
                            let t = ((ty * p.w + tx) * 4) as usize;
                            *px.add(t) = data[s + 2];
                            *px.add(t + 1) = data[s + 1];
                            *px.add(t + 2) = data[s];
                            *px.add(t + 3) = 0;
                        }
                    }
                }
                MlOp::Text { x, y, w, h, t, c, f, font, clip } => {
                    if t.is_empty() {
                        continue;
                    }
                    let saved = clip.map(|[cx, cy, cw, ch]| {
                        let s = SaveDC(dc);
                        IntersectClipRect(dc, cx, cy, cx + cw, cy + ch);
                        s
                    });
                    SelectObject(dc, HGDIOBJ(fonts[(*font).min(fonts.len() - 1)].0));
                    SetTextColor(dc, rgb(*c));
                    let mut text: Vec<u16> = t.encode_utf16().take(4096).collect();
                    let mut r = RECT { left: *x, top: *y, right: x + w, bottom: y + h };
                    DrawTextW(dc, &mut text, &mut r, DRAW_TEXT_FORMAT(*f) | DT_NOPREFIX);
                    if let Some(s) = saved {
                        let _ = RestoreDC(dc, s);
                    }
                }
            }
        }
        let _ = GdiFlush();
        let src = std::slice::from_raw_parts(bits as *const u8, (p.w * p.h * 4) as usize);
        let mut rgba = Vec::with_capacity(src.len());
        for px in src.chunks_exact(4) {
            let (r, g, b) = (px[2], px[1], px[0]);
            let a = if [r, g, b] == KEY { 0 } else { 255 };
            rgba.extend_from_slice(&[r, g, b, a]);
        }
        SelectObject(dc, old_font);
        SelectObject(dc, old_bitmap);
        for f in fonts {
            let _ = DeleteObject(HGDIOBJ(f.0));
        }
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(dc);
        Some(MlPainted { w: p.w, h: p.h, rgba })
    }
}

#[cfg(not(windows))]
pub fn paint(_p: &MlPaint) -> Option<MlPainted> {
    None
}

#[cfg(windows)]
pub fn measure(m: &MlMeasure) -> MlMeasured {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DT_CALCRECT, DT_NOPREFIX, DT_SINGLELINE, DeleteDC, DeleteObject, DrawTextW,
        GetTextMetricsW, HGDIOBJ, SelectObject, TEXTMETRICW,
    };

    if m.fonts.is_empty() || m.fonts.len() > 8 || m.texts.len() > 20000 {
        return MlMeasured::default();
    }
    unsafe {
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return MlMeasured::default();
        }
        let fonts: Vec<_> = m.fonts.iter().map(gdi::font).collect();
        let old_font = SelectObject(dc, HGDIOBJ(fonts[0].0));
        let mut out = MlMeasured::default();
        for f in &fonts {
            SelectObject(dc, HGDIOBJ(f.0));
            let mut tm = TEXTMETRICW::default();
            let _ = GetTextMetricsW(dc, &mut tm);
            out.metrics.push([tm.tmHeight, tm.tmAscent, tm.tmAveCharWidth, tm.tmExternalLeading]);
        }
        for (t, font) in &m.texts {
            SelectObject(dc, HGDIOBJ(fonts[(*font).min(fonts.len() - 1)].0));
            let mut text: Vec<u16> = t.encode_utf16().take(4096).collect();
            let mut r = RECT::default();
            if text.is_empty() {
                out.sizes.push([0, 0]);
                continue;
            }
            DrawTextW(dc, &mut text, &mut r, DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX);
            out.sizes.push([r.right - r.left, r.bottom - r.top]);
        }
        SelectObject(dc, old_font);
        for f in fonts {
            let _ = DeleteObject(HGDIOBJ(f.0));
        }
        let _ = DeleteDC(dc);
        out
    }
}

#[cfg(not(windows))]
pub fn measure(_m: &MlMeasure) -> MlMeasured {
    MlMeasured::default()
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    fn arial() -> MlFont {
        MlFont { face: "Arial".into(), size: 12, bold: false }
    }

    #[test]
    fn what_no_op_covers_stays_see_through() {
        let p = MlPaint {
            w: 20,
            h: 10,
            fonts: vec![arial()],
            ops: vec![MlOp::Fill { x: 0, y: 0, w: 10, h: 10, c: [1, 2, 3] }],
        };
        let out = paint(&p).expect("painted");
        assert_eq!(&out.rgba[0..4], &[1, 2, 3, 255]);
        let right = ((5 * 20 + 15) * 4) as usize;
        assert_eq!(out.rgba[right + 3], 0);
    }

    #[test]
    fn text_lands_on_its_background() {
        let p = MlPaint {
            w: 60,
            h: 16,
            fonts: vec![arial()],
            ops: vec![
                MlOp::Fill { x: 0, y: 0, w: 60, h: 16, c: [0, 0, 0] },
                MlOp::Text { x: 0, y: 0, w: 60, h: 16, t: "Audio".into(), c: [0, 255, 0], f: 0x24, font: 0, clip: None },
            ],
        };
        let out = paint(&p).expect("painted");
        assert!(out.rgba.chunks_exact(4).any(|px| px[1] > 128 && px[3] == 255));
        assert!(out.rgba.chunks_exact(4).all(|px| px[3] == 255));
    }

    #[test]
    fn arial_12_measures_as_winamp_laid_it_out() {
        let m = measure(&MlMeasure { fonts: vec![arial()], texts: vec![("Library".into(), 0)] });
        // the playlist font's height (a row of the playlist editor)
        assert_eq!(m.metrics[0][0], 15);
        assert!(m.sizes[0][0] > 20 && m.sizes[0][0] < 60);
    }
}
