//! Modern skins' text drawn the way Winamp 5 drew it: Wasabi's Text widget
//! lays a text out (padding, alignment, tabs, shadow, a time's pieces) and
//! has Windows draw each piece with GDI into a black bitmap, which becomes
//! the text's colour with the ink as alpha (ClearType's per-channel ink when
//! the system smooths with ClearType). Doing the same calls here gives the
//! same pixels as Winamp on the same machine, where a browser draws its own.
//!
//! Plain functions, no Tauri: the app's commands (modern_window.rs) call
//! them, and so can a test tool built from this file.

// (a text drawn at a place, in a box, in a colour: Winamp's own signatures)
#![allow(clippy::too_many_arguments)]

use serde::Deserialize;

/// What Winamp's Text widget knew when it painted, from the skin and its
/// state. Sizes are the widget's, in pixels.
#[derive(Deserialize, Debug, Clone)]
pub struct Paint {
    /// the font's face name, as `face_of_skin_font` / `face_of_system_font` give it
    pub face: String,
    /// the skin's font size; Winamp draws it MulDiv(size, 72, 96) pixels high
    pub size: i32,
    #[serde(default)]
    pub bold: i32,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub underline: bool,
    #[serde(default = "yes")]
    pub antialias: bool,
    /// the text as printed (upper or lower case already applied)
    pub text: String,
    pub w: i32,
    pub h: i32,
    /// 0 left, 1 center, 2 right
    #[serde(default)]
    pub align: i32,
    /// 0 top, 1 center, 2 bottom
    #[serde(default = "one")]
    pub valign: i32,
    #[serde(default)]
    pub lpadding: i32,
    #[serde(default)]
    pub rpadding: i32,
    #[serde(default)]
    pub offsetx: i32,
    #[serde(default)]
    pub offsety: i32,
    pub color: [u8; 3],
    #[serde(default)]
    pub shadow: Option<Shadow>,
    #[serde(default)]
    pub wrapped: bool,
    /// a time display ("1:23"): laid out in its pieces
    #[serde(default)]
    pub time: Option<TimeLayout>,
    /// "freetype" for a skin older than 1.3 (Winamp's FreeType renderer),
    /// else Windows' GDI
    #[serde(default)]
    pub renderer: String,
    /// FreeType's font: "face:<face>" for a skin's own (load_font), or
    /// "sys:<id>" for Windows' fonts folder's
    #[serde(default)]
    pub ft_font: String,
    /// the name Winamp's FreeType renderer knew a skin's font by (its file's,
    /// without the extension): a few fonts were drawn a size bigger
    #[serde(default)]
    pub ft_name: String,
}

/// How Winamp measured and drew a text, GDI's or FreeType's way: what the
/// Text widget's layout (paint) works with.
trait Pen {
    /// the text's width, as Winamp's font reported it
    fn width(&self, s: &[u16]) -> i32;
    /// ...and height
    fn height(&self, s: &[u16]) -> i32;
    /// a text in a box (the font's textOut2), aligned 0 left, 1 centre, 2 right
    fn text_rect(&self, buf: &mut Buffer, x: i32, y: i32, w: i32, h: i32, s: &[u16], align: u32, color: [u8; 3]);
    /// ...wrapped in it
    fn text_wrapped(&self, buf: &mut Buffer, x: i32, y: i32, w: i32, h: i32, s: &[u16], align: u32, color: [u8; 3]);
    /// a text at a point (textOut)
    fn text_at(&self, buf: &mut Buffer, x: i32, y: i32, s: &[u16], color: [u8; 3]);
}

#[derive(Deserialize, Debug, Clone, Copy)]
pub struct Shadow {
    pub x: i32,
    pub y: i32,
    pub color: [u8; 3],
}

#[derive(Deserialize, Debug, Clone, Copy)]
pub struct TimeLayout {
    #[serde(default)]
    pub hours: bool,
    #[serde(default)]
    pub fixed: bool,
    /// the colon's width, -1 for its own
    #[serde(default = "minus_one")]
    pub colon: i32,
}

fn yes() -> bool {
    true
}
fn one() -> i32 {
    1
}
fn minus_one() -> i32 {
    -1
}

/// A painted text: its buffer (as wide as the widget, or as its text when
/// that's wider, where a ticker scrolls) in straight-alpha RGBA, top row
/// first, and how wide the text itself is.
pub struct Painted {
    pub w: i32,
    pub h: i32,
    pub text_w: i32,
    /// the width Winamp suggested for the text (its getAutoWidth)
    pub suggested: i32,
    pub rgba: Vec<u8>,
}

impl Painted {
    /// [w, h, text_w, suggested as little-endian i32] then the pixels:
    /// what the command hands the page.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(16 + self.rgba.len());
        out.extend_from_slice(&self.w.to_le_bytes());
        out.extend_from_slice(&self.h.to_le_bytes());
        out.extend_from_slice(&self.text_w.to_le_bytes());
        out.extend_from_slice(&self.suggested.to_le_bytes());
        out.extend_from_slice(&self.rgba);
        out
    }
}

/// The face Winamp names a font file by: the last full name (name ID 4) of
/// the Macintosh platform in its name table, read as Windows-1252; "" when
/// it has none, None when it has no name table (or isn't a font).
pub fn mac_full_name(data: &[u8]) -> Option<String> {
    let be16 = |at: usize| data.get(at..at + 2).map(|b| u16::from_be_bytes([b[0], b[1]]) as usize);
    let be32 = |at: usize| data.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let tables = be16(4)?;
    // (Winamp looked at the first 40 tables)
    for i in 0..tables.min(40) {
        let entry = 12 + i * 16;
        if data.get(entry..entry + 4)? != b"name" {
            continue;
        }
        let table = be32(entry + 8)?;
        let count = be16(table + 2)?;
        let strings = table + be16(table + 4)?;
        let mut name = String::new();
        for r in 0..count {
            let rec = table + 6 + r * 12;
            let (Some(platform), Some(name_id), Some(len), Some(off)) = (be16(rec), be16(rec + 6), be16(rec + 8), be16(rec + 10)) else {
                break;
            };
            if platform != 1 || name_id != 4 {
                continue;
            }
            let bytes = data.get(strings + off..strings + off + len.min(255)).unwrap_or(&[]);
            let bytes = bytes.split(|b| *b == 0).next().unwrap_or(&[]);
            name = decode_1252(bytes);
        }
        return Some(name);
    }
    None
}

fn decode_1252(bytes: &[u8]) -> String {
    // the 0x80-0x9F row, where 1252 differs from Latin-1
    const ROW: [char; 32] = [
        '\u{20AC}', '\u{81}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}', '\u{02C6}', '\u{2030}',
        '\u{0160}', '\u{2039}', '\u{0152}', '\u{8D}', '\u{017D}', '\u{8F}', '\u{90}', '\u{2018}', '\u{2019}', '\u{201C}',
        '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}', '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{9D}',
        '\u{017E}', '\u{0178}',
    ];
    bytes
        .iter()
        .map(|&b| if (0x80..0xA0).contains(&b) { ROW[(b - 0x80) as usize] } else { b as char })
        .collect()
}

/// Windows' fonts folder.
fn fonts_dir() -> std::path::PathBuf {
    let windir = std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into());
    std::path::Path::new(&windir).join("Fonts")
}

/// The face Winamp drew a font with when the skin has no font of that id:
/// Windows' fonts folder's "<id>.ttf" when there's one, else its default
/// font (Arial), each by the face in its file.
pub fn face_of_system_font(id: &str) -> String {
    let file = if id.to_lowercase().contains(".ttf") { id.to_string() } else { format!("{id}.ttf") };
    let path = fonts_dir().join(file);
    if let Ok(data) = std::fs::read(&path) {
        // (no name table: no face, and Windows picks)
        return mac_full_name(&data).unwrap_or_default();
    }
    std::fs::read(fonts_dir().join("Arial.ttf")).ok().and_then(|d| mac_full_name(&d)).unwrap_or_else(|| "Arial".into())
}

#[cfg(windows)]
mod gdi {
    use super::*;
    use std::ffi::c_void;
    use windows::Win32::Foundation::{COLORREF, HANDLE, RECT, SIZE};
    use windows::Win32::Graphics::Gdi::{
        AddFontMemResourceEx, ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CLEARTYPE_QUALITY,
        CLIP_DEFAULT_PRECIS, CreateCompatibleDC, CreateDIBSection, CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH,
        DIB_RGB_COLORS, DRAW_TEXT_FORMAT, DT_NOCLIP, DT_NOPREFIX, DT_WORDBREAK, DeleteDC, DeleteObject, DrawTextW,
        FF_DONTCARE, FW_BOLD, FW_NORMAL, GdiFlush, GetTextExtentPoint32W, HDC, HFONT, HGDIOBJ, NONANTIALIASED_QUALITY,
        OUT_TT_ONLY_PRECIS, SelectObject, SetBkColor, SetTextColor, TextOutW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        SPI_GETFONTSMOOTHINGTYPE, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    };
    use windows::core::PCWSTR;

    /// Fonts the skins brought, kept loaded while the app runs.
    static LOADED: std::sync::Mutex<Vec<isize>> = std::sync::Mutex::new(Vec::new());

    /// Load a skin's font file for this process; its face, as Winamp names it.
    pub fn load_font(data: &[u8]) -> Option<String> {
        let face = mac_full_name(data)?;
        let mut count = 0u32;
        let handle: HANDLE = unsafe {
            AddFontMemResourceEx(data.as_ptr() as *const c_void, data.len() as u32, None, std::ptr::addr_of_mut!(count))
        };
        if handle.is_invalid() || handle.0.is_null() {
            return None;
        }
        LOADED.lock().map(|mut l| l.push(handle.0 as isize)).ok();
        #[cfg(feature = "freetype")]
        super::ft::remember(&face, data);
        Some(face)
    }

    /// Windows' ClearType on: Winamp then kept each channel's ink.
    fn system_cleartype() -> bool {
        let mut kind = 0u32;
        unsafe {
            SystemParametersInfoW(
                SPI_GETFONTSMOOTHINGTYPE,
                0,
                Some(&mut kind as *mut u32 as *mut c_void),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
            .is_ok()
                && kind == 2 // FE_FONTSMOOTHINGCLEARTYPE
        }
    }

    /// Windows' MulDiv: rounded, halves away from zero.
    fn mul_div(a: i32, b: i32, c: i32) -> i32 {
        let q = (a as i64 * b as i64) as f64 / c as f64;
        ((q.abs() + 0.5).floor() * q.signum()) as i32
    }

    /// A font as Winamp made one (truetypefont_win32), selected into a DC.
    pub struct Font {
        dc: HDC,
        font: HFONT,
        old: HGDIOBJ,
        cleartype: bool,
    }

    impl Font {
        pub fn new(p: &Paint) -> Option<Font> {
            let face: Vec<u16> = p.face.encode_utf16().chain(std::iter::once(0)).collect();
            let height = mul_div(p.size, 72, 96);
            let quality = if !p.antialias { NONANTIALIASED_QUALITY } else { CLEARTYPE_QUALITY };
            let _ = ANTIALIASED_QUALITY; // (Winamp's choice before Windows XP)
            unsafe {
                let dc = CreateCompatibleDC(None);
                if dc.is_invalid() {
                    return None;
                }
                let font = CreateFontW(
                    -height,
                    0,
                    0,
                    0,
                    if p.bold != 0 { FW_BOLD.0 as i32 } else { FW_NORMAL.0 as i32 },
                    p.italic as u32,
                    p.underline as u32,
                    0,
                    DEFAULT_CHARSET,
                    OUT_TT_ONLY_PRECIS,
                    CLIP_DEFAULT_PRECIS,
                    quality,
                    (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
                    PCWSTR(face.as_ptr()),
                );
                if font.is_invalid() {
                    let _ = DeleteDC(dc);
                    return None;
                }
                let old = SelectObject(dc, HGDIOBJ(font.0));
                Some(Font { dc, font, old, cleartype: p.antialias && system_cleartype() })
            }
        }

        fn extent(&self, s: &[u16]) -> (i32, i32) {
            let mut size = SIZE::default();
            unsafe {
                let _ = GetTextExtentPoint32W(self.dc, s, &mut size);
            }
            (size.cx, size.cy)
        }

        /// Winamp's text width: the extent and its 0 + 2 padding
        pub fn width(&self, s: &[u16]) -> i32 {
            self.extent(s).0 + 2
        }

        /// ...and height: the extent and its 1 + 1 padding
        pub fn height(&self, s: &[u16]) -> i32 {
            self.extent(s).1 + 2
        }

        /// Windows draws white text on black in a w x h bitmap; Winamp keeps
        /// the ink: the colour times the ink, the ink's average as alpha
        /// (premultiplied, as it blits).
        fn draw(&self, w: i32, h: i32, how: impl FnOnce(HDC)) -> Option<Vec<u8>> {
            if w <= 0 || h <= 0 {
                return None;
            }
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
            unsafe {
                let mut bits: *mut c_void = std::ptr::null_mut();
                let bitmap = CreateDIBSection(Some(self.dc), &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
                let old = SelectObject(self.dc, HGDIOBJ(bitmap.0));
                SetBkColor(self.dc, COLORREF(0));
                SetTextColor(self.dc, COLORREF(0x00FF_FFFF));
                how(self.dc);
                let _ = GdiFlush();
                let px = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize).to_vec();
                SelectObject(self.dc, old);
                let _ = DeleteObject(HGDIOBJ(bitmap.0));
                Some(px)
            }
        }

        /// The ink of a drawn bitmap in the colour, premultiplied BGRA.
        fn ink(&self, px: &mut [u8], color: [u8; 3]) {
            for p in px.chunks_exact_mut(4) {
                let (b, g, r) = (p[0], p[1], p[2]);
                if b == 0 && g == 0 && r == 0 && p[3] == 0 {
                    continue;
                }
                let (mut ra, mut ga, mut ba, mut a) = (255u32, 255u32, 255u32, 255u32);
                if !(b == 255 && g == 255 && r == 255 && p[3] == 0) {
                    a = (b as u32 + g as u32 + r as u32) / 3;
                    if self.cleartype {
                        (ra, ga, ba) = (r as u32, g as u32, b as u32);
                    } else {
                        (ra, ga, ba) = (a, a, a);
                    }
                }
                p[3] = a as u8;
                p[2] = ((color[0] as u32 * ra + 128) / 255) as u8;
                p[1] = ((color[1] as u32 * ga + 128) / 255) as u8;
                p[0] = ((color[2] as u32 * ba + 128) / 255) as u8;
            }
        }

        /// Winamp's text in a box (textOut2 / textOutWrapped): drawn in a w x h
        /// bitmap with these DrawText flags, laid at (x, y + 1).
        pub fn text_box(&self, buf: &mut Buffer, x: i32, y: i32, w: i32, h: i32, s: &[u16], flags: u32, color: [u8; 3]) {
            let mut text = s.to_vec();
            let Some(mut px) = self.draw(w, h, |dc| unsafe {
                let mut r = RECT { left: 0, top: 0, right: w, bottom: h };
                DrawTextW(dc, &mut text, &mut r, DRAW_TEXT_FORMAT(flags) | DT_NOPREFIX);
            }) else {
                return;
            };
            self.ink(&mut px, color);
            buf.blit(&px, w, h, x, y + 1);
        }

        pub fn text_rect(&self, buf: &mut Buffer, x: i32, y: i32, w: i32, h: i32, s: &[u16], align: u32, color: [u8; 3]) {
            self.text_box(buf, x, y, w, h, s, align | DT_NOCLIP.0, color);
        }

        pub fn text_wrapped(&self, buf: &mut Buffer, x: i32, y: i32, w: i32, h: i32, s: &[u16], align: u32, color: [u8; 3]) {
            self.text_box(buf, x, y, w, h, s, align | DT_WORDBREAK.0, color);
        }

        /// Winamp's text at a point (textOut): drawn at (x, y) in a bitmap the
        /// buffer's size, laid at (0, 1).
        pub fn text_at(&self, buf: &mut Buffer, x: i32, y: i32, s: &[u16], color: [u8; 3]) {
            let (w, h) = (buf.w, buf.h);
            let Some(mut px) = self.draw(w, h, |dc| unsafe {
                let _ = TextOutW(dc, x, y, s);
            }) else {
                return;
            };
            self.ink(&mut px, color);
            buf.blit(&px, w, h, 0, 1);
        }
    }

    impl Drop for Font {
        fn drop(&mut self) {
            unsafe {
                SelectObject(self.dc, self.old);
                let _ = DeleteObject(HGDIOBJ(self.font.0));
                let _ = DeleteDC(self.dc);
            }
        }
    }

    impl Pen for Font {
        fn width(&self, s: &[u16]) -> i32 {
            Font::width(self, s)
        }
        fn height(&self, s: &[u16]) -> i32 {
            Font::height(self, s)
        }
        fn text_rect(&self, buf: &mut Buffer, x: i32, y: i32, w: i32, h: i32, s: &[u16], align: u32, color: [u8; 3]) {
            Font::text_rect(self, buf, x, y, w, h, s, align, color)
        }
        fn text_wrapped(&self, buf: &mut Buffer, x: i32, y: i32, w: i32, h: i32, s: &[u16], align: u32, color: [u8; 3]) {
            Font::text_wrapped(self, buf, x, y, w, h, s, align, color)
        }
        fn text_at(&self, buf: &mut Buffer, x: i32, y: i32, s: &[u16], color: [u8; 3]) {
            Font::text_at(self, buf, x, y, s, color)
        }
    }
}

#[cfg(windows)]
pub use gdi::load_font;

/// The text's buffer: premultiplied BGRA, cleared to nothing.
pub struct Buffer {
    pub w: i32,
    pub h: i32,
    px: Vec<u8>,
}

impl Buffer {
    fn new(w: i32, h: i32) -> Buffer {
        Buffer { w, h, px: vec![0; (w.max(0) * h.max(0) * 4) as usize] }
    }

    /// Premultiplied pixels laid over the buffer at (x, y).
    fn blit(&mut self, src: &[u8], sw: i32, sh: i32, x: i32, y: i32) {
        for sy in 0..sh {
            let dy = y + sy;
            if dy < 0 || dy >= self.h {
                continue;
            }
            for sx in 0..sw {
                let dx = x + sx;
                if dx < 0 || dx >= self.w {
                    continue;
                }
                let s = ((sy * sw + sx) * 4) as usize;
                let a = src[s + 3] as u32;
                if a == 0 && src[s] == 0 && src[s + 1] == 0 && src[s + 2] == 0 {
                    continue;
                }
                let d = ((dy * self.w + dx) * 4) as usize;
                for c in 0..4 {
                    let v = src[s + c] as u32 + (self.px[d + c] as u32 * (255 - a) + 127) / 255;
                    self.px[d + c] = v.min(255) as u8;
                }
            }
        }
    }

    /// Premultiplied pixels laid over the buffer the way Winamp's bitmaps
    /// blit (its blend table: d * (256 - a) >> 8, plus the source).
    #[allow(dead_code)]
    fn blit_over(&mut self, src: &[u8], sw: i32, sh: i32, x: i32, y: i32) {
        for sy in 0..sh {
            let dy = y + sy;
            if dy < 0 || dy >= self.h {
                continue;
            }
            for sx in 0..sw {
                let dx = x + sx;
                if dx < 0 || dx >= self.w {
                    continue;
                }
                let s = ((sy * sw + sx) * 4) as usize;
                let a = src[s + 3] as u32;
                if a == 0 && src[s] == 0 && src[s + 1] == 0 && src[s + 2] == 0 {
                    continue;
                }
                let d = ((dy * self.w + dx) * 4) as usize;
                for c in 0..4 {
                    let v = ((self.px[d + c] as u32 * (256 - a)) >> 8) + src[s + c] as u32;
                    self.px[d + c] = v.min(255) as u8;
                }
            }
        }
    }

    /// Straight-alpha RGBA for a canvas; a channel brighter than its alpha
    /// (ClearType's coloured edge over nothing) raises the alpha to it.
    fn into_rgba(self) -> Vec<u8> {
        let mut out = self.px;
        for p in out.chunks_exact_mut(4) {
            let (b, g, r, a) = (p[0] as u32, p[1] as u32, p[2] as u32, p[3] as u32);
            let aa = a.max(r).max(g).max(b);
            let st = |c: u32| (c * 255 + aa / 2).checked_div(aa).unwrap_or(0).min(255) as u8;
            (p[0], p[1], p[2], p[3]) = (st(r), st(g), st(b), aa as u8);
        }
        out
    }
}

/// Paint a text as Winamp's Text widget does (onBufferPaint), with the pen
/// Winamp drew that skin's text with.
#[cfg(windows)]
pub fn paint(p: &Paint) -> Option<Painted> {
    if p.renderer == "freetype" {
        #[cfg(feature = "freetype")]
        return ft::with_font(p, |font| lay_out(font, p));
        #[cfg(not(feature = "freetype"))]
        return None;
    }
    let font = gdi::Font::new(p)?;
    Some(lay_out(&font, p))
}

/// Wasabi's Text widget's layout (onBufferPaint) with a pen.
fn lay_out<P: Pen>(font: &P, p: &Paint) -> Painted {
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().collect() };
    let text = wide(&p.text);
    let colon_split = p.time.is_some() && p.text.contains(':');

    // the buffer: as wide as the widget, or as the text wants (a ticker's)
    let suggested = if p.text.is_empty() {
        32
    } else if colon_split {
        // (as Winamp measured it: the minutes one character short, the
        // seconds all after the first colon, no hours)
        let p_at = p.text.find(':').unwrap_or(0);
        let mut m: Vec<u16> = wide(&p.text[..p_at]);
        m.pop();
        let s = wide(&p.text[p_at + 1..]);
        let fixed = p.time.map(|t| t.fixed).unwrap_or(false);
        let fixw = font.width(&wide("0"));
        let wd = |s: &[u16]| if fixed { fixw * s.len() as i32 } else { font.width(s) };
        let colon = p.time.map(|t| t.colon).filter(|c| *c != -1).unwrap_or_else(|| wd(&wide(":")));
        wd(&s) + wd(&m) + colon + p.lpadding + p.rpadding
    } else {
        p.text
            .split('\n')
            .map(|line| line.split('\t').map(|piece| font.width(&wide(piece)) + 4).sum::<i32>())
            .max()
            .unwrap_or(0)
            + p.lpadding
            + p.rpadding
    };
    let (bw, bh) = (p.w.max(suggested).max(1), p.h.max(1));
    let mut buf = Buffer::new(bw, bh);
    let text_w = font.width(&text);
    if p.text.is_empty() {
        return Painted { w: bw, h: bh, text_w: 0, suggested, rgba: buf.into_rgba() };
    }
    let shadow = p.shadow.filter(|s| s.x != 0 || s.y != 0);
    let valign_y = |h: i32| match p.valign {
        1 => (bh - h) / 2,
        _ => 0, // top; Winamp's bottom stayed at the top too
    };

    if !colon_split {
        let mut align = p.align as u32;
        let wantpadding = if text_w <= bw - 2 - p.lpadding - p.rpadding { align != 1 } else {
            align = 0;
            true
        };
        let th = font.height(&text);
        let x = 2 + if wantpadding { p.lpadding } else { 0 } + p.offsetx;
        let y = valign_y(th) + p.offsety;
        for (i, piece) in p.text.split('\t').enumerate() {
            if i > 0 {
                align = 2;
            }
            let piece = wide(piece);
            let draw = |buf: &mut Buffer, x: i32, y: i32, color: [u8; 3]| {
                if p.wrapped {
                    font.text_wrapped(buf, x, y, bw - 2, bh, &piece, align, color);
                } else {
                    font.text_rect(buf, x, y, bw - 2, bh, &piece, align, color);
                }
            };
            if let Some(s) = shadow {
                draw(&mut buf, x + s.x, y + s.y, s.color);
            }
            draw(&mut buf, x, y, p.color);
        }
    } else {
        let t = p.time.unwrap();
        let (hours, mins, secs) = time_pieces(&p.text, t.hours);
        let th = font.height(&text);
        let fixw = font.width(&wide("0"));
        let wd = |s: &str| if t.fixed { fixw * s.encode_utf16().count() as i32 } else { font.width(&wide(s)) };
        let (ws, wm, wh, wc) = (wd(&secs), wd(&mins), wd(&hours), wd(":"));
        let colon = if t.colon == -1 { wc } else { t.colon };
        let (sx, sy) = p.shadow.map(|s| (s.x, s.y)).unwrap_or((0, 0));
        let shadow_color = p.shadow.map(|s| s.color).unwrap_or([0, 0, 0]);
        // a piece, a digit at a time on a digit's width when fixed
        let out = |buf: &mut Buffer, x: i32, y: i32, s: &str, color: [u8; 3]| {
            if !t.fixed {
                font.text_at(buf, x, y, &wide(s), color);
                return;
            }
            let mut x = x;
            for ch in s.encode_utf16() {
                let dw = fixw - font.width(&[ch]);
                font.text_at(buf, x + dw / 2, y, &[ch], color);
                x += fixw;
            }
        };
        let has_hours = t.hours && !hours.is_empty();
        let forwards = |buf: &mut Buffer, x: i32, y: i32, color: [u8; 3]| {
            if has_hours {
                out(buf, x, y, &hours, color);
                out(buf, x + wh, y, ":", color);
                out(buf, x + wh + colon, y, &mins, color);
                out(buf, x + wh + colon + wm, y, ":", color);
                out(buf, x + wh + colon + wm + colon, y, &secs, color);
            } else {
                out(buf, x, y, &mins, color);
                out(buf, x + wm, y, ":", color);
                out(buf, x + wm + colon, y, &secs, color);
            }
        };
        match p.align {
            2 => {
                let x = (bw - 2) - sx - p.rpadding + p.offsetx;
                let y = valign_y(th) + p.offsety;
                let backwards = |buf: &mut Buffer, x: i32, y: i32, color: [u8; 3]| {
                    out(buf, x - ws, y, &secs, color);
                    out(buf, x - ws - colon, y, ":", color);
                    out(buf, x - ws - colon - wm, y, &mins, color);
                    if has_hours {
                        out(buf, x - ws - colon - wm - colon, y, ":", color);
                        out(buf, x - ws - colon - wm - colon - wh, y, &hours, color);
                    }
                };
                if sx != 0 || sy != 0 {
                    backwards(&mut buf, x, y, shadow_color);
                }
                backwards(&mut buf, x + sx, y + sy, p.color);
            }
            0 => {
                let x = 2 - sx + p.lpadding + p.offsetx;
                let y = valign_y(th) + p.offsety;
                // (Winamp's left-aligned time always drew its shadow, offset
                // or not: it tested the offsets' array, never empty)
                forwards(&mut buf, x, y, shadow_color);
                forwards(&mut buf, x + sx, y + sy, p.color);
            }
            _ => {
                let x = if has_hours { (bw - ws - wm - wh - colon) / 3 } else { (bw - ws - wm - colon) / 2 } - sx + p.offsetx;
                let y = valign_y(th) + p.offsety;
                if sx != 0 || sy != 0 {
                    forwards(&mut buf, x, y, shadow_color);
                }
                forwards(&mut buf, x + sx, y + sy, p.color);
            }
        }
    }
    Painted { w: bw, h: bh, text_w, suggested, rgba: buf.into_rgba() }
}

/// A time's hours, minutes and seconds, as Winamp split "h:mm:ss" / "m:ss".
fn time_pieces(text: &str, with_hours: bool) -> (String, String, String) {
    let Some(p) = text.find(':') else {
        return (String::new(), text.to_string(), String::new());
    };
    let before = text[..p].to_string();
    let after = &text[p + 1..];
    if !with_hours {
        return (String::new(), before, after.to_string());
    }
    match after.find(':') {
        Some(p2) if p2 + 1 < after.len() => (before, after[..p2].to_string(), after[p2 + 1..].to_string()),
        Some(p2) => (String::new(), before, after[..p2].to_string()),
        None => (String::new(), before, after.to_string()),
    }
}

/// Winamp's FreeType renderer (freetypewac, FreeType of late 2013): its own
/// glyph drawing over FreeType's grey-antialiased glyphs, every font hinted
/// by FreeType's autohinter (that build had no TrueType bytecode hinting:
/// measured against Winamp's own text, the autohinter's glyphs match it).
#[cfg(feature = "freetype")]
// (FreeType's longs are 32 bits on Windows, 64 elsewhere)
#[allow(clippy::unnecessary_cast)]
mod ft {
    use super::*;
    use freetype_sys as f;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    struct Faces {
        lib: f::FT_Library,
        /// FreeType faces by font key, with the bytes they're read from
        faces: HashMap<String, (f::FT_Face, Arc<Vec<u8>>)>,
    }
    // (FreeType's objects, used only under the lock)
    unsafe impl Send for Faces {}

    static FACES: Mutex<Option<Faces>> = Mutex::new(None);
    /// the skins' font files, by the face load_font gave them
    static FILES: Mutex<Option<HashMap<String, Arc<Vec<u8>>>>> = Mutex::new(None);

    pub fn remember(face: &str, data: &[u8]) {
        if let Ok(mut files) = FILES.lock() {
            files.get_or_insert_with(HashMap::new).insert(face.to_string(), Arc::new(data.to_vec()));
        }
        // a skin's newer font of that face replaces the one read before
        if let Ok(mut faces) = FACES.lock()
            && let Some(faces) = faces.as_mut()
            && let Some((face, _)) = faces.faces.remove(&format!("face:{face}"))
        {
            unsafe { f::FT_Done_Face(face) };
        }
    }

    /// A font key's bytes, and the name Winamp's renderer knew it by.
    fn source(p: &Paint) -> Option<(Arc<Vec<u8>>, String)> {
        if let Some(face) = p.ft_font.strip_prefix("face:") {
            let files = FILES.lock().ok()?;
            return Some((files.as_ref()?.get(face)?.clone(), p.ft_name.clone()));
        }
        let id = p.ft_font.strip_prefix("sys:").unwrap_or("Arial");
        let file = if id.to_lowercase().contains(".ttf") { id.to_string() } else { format!("{id}.ttf") };
        if let Ok(data) = std::fs::read(fonts_dir().join(file)) {
            return Some((Arc::new(data), id.to_string()));
        }
        // Winamp's default font, under its file's name
        Some((Arc::new(std::fs::read(fonts_dir().join("Arial.ttf")).ok()?), "Arial".into()))
    }

    /// A few fonts Winamp's renderer drew bigger than asked, by name.
    fn tweak_size(name: &str, size: i32) -> i32 {
        match name.to_lowercase().as_str() {
            "nonstep" => size - 1,
            "04b_08__" | "04b_03b_" | "04b_09__" | "04b_21" | "ethnocentric" | "ethnocen" | "pixel" => size + 3,
            "blocky" => size + 6,
            "radiosta" => size + 2,
            _ => size,
        }
    }

    pub struct Font {
        face: f::FT_Face,
        bold: i32,
        italic: bool,
        antialias: bool,
    }

    /// Paint with a FreeType font for this text, under FreeType's lock.
    pub fn with_font<T>(p: &Paint, then: impl FnOnce(&Font) -> T) -> Option<T> {
        let mut guard = FACES.lock().ok()?;
        if guard.is_none() {
            let mut lib: f::FT_Library = std::ptr::null_mut();
            unsafe {
                if f::FT_Init_FreeType(&mut lib) != 0 {
                    return None;
                }
            }
            *guard = Some(Faces { lib, faces: HashMap::new() });
        }
        let faces = guard.as_mut()?;
        let key = p.ft_font.clone();
        let (data, name) = source(p)?;
        if !faces.faces.contains_key(&key) {
            let mut face: f::FT_Face = std::ptr::null_mut();
            let err = unsafe { f::FT_New_Memory_Face(faces.lib, data.as_ptr(), data.len() as f::FT_Long, 0, &mut face) };
            if err != 0 {
                return None;
            }
            unsafe {
                let _ = f::FT_Select_Charmap(face, f::FT_ENCODING_UNICODE);
            }
            faces.faces.insert(key.clone(), (face, data));
        }
        let face = faces.faces.get(&key)?.0;
        unsafe {
            let size = tweak_size(&name, p.size);
            let height = ((size as i64 * 64 * 72) as f64 / 96.0).round() as f::FT_F26Dot6;
            f::FT_Set_Char_Size(face, 0, height, 0, 0);
        }
        let font = Font { face, bold: p.bold, italic: p.italic, antialias: p.antialias };
        Some(then(&font))
    }

    /// Winamp's glyph loading (default, no embedded bitmaps), autohinted
    fn load_flags() -> i32 {
        f::FT_LOAD_DEFAULT | f::FT_LOAD_NO_BITMAP | f::FT_LOAD_FORCE_AUTOHINT
    }

    impl Font {
        fn slot(&self) -> f::FT_GlyphSlot {
            unsafe { (*self.face).glyph }
        }

        /// Winamp's slant for italic: 12 degrees as it reckoned them (its pi
        /// was 3.14159)
        #[allow(clippy::approx_constant)]
        fn slant(&self, glyph: f::FT_Glyph) {
            if !self.italic {
                return;
            }
            let pi2 = 3.14159f64 * 2.0;
            let shear = ((pi2 / (360 / 12) as f64).tan() * 65536.0) as f::FT_Fixed;
            let mut m = f::FT_Matrix { xx: 1 << 16, xy: shear, yx: 0, yy: 1 << 16 };
            let mut v = f::FT_Vector { x: 0, y: 0 };
            unsafe {
                f::FT_Glyph_Transform(glyph, &mut m, &mut v);
            }
        }

        /// how many times a glyph is laid, a pixel apart, for faux bold
        fn passes(&self) -> i32 {
            if self.bold == 0 {
                return 1;
            }
            let adv = unsafe { ((*self.slot()).advance.x >> 6) as f32 };
            ((adv * 0.1) as i32).max(2)
        }

        /// 'M''s top: the baseline's depth in the bitmap
        fn ascent(&self) -> i32 {
            self.cbox('M').map(|b| (b.yMax >> 6) as i32).unwrap_or(0)
        }

        fn cbox(&self, c: char) -> Option<f::FT_BBox> {
            unsafe {
                let gi = f::FT_Get_Char_Index(self.face, c as f::FT_ULong);
                f::FT_Load_Glyph(self.face, gi, load_flags() & !f::FT_LOAD_NO_BITMAP);
                let mut glyph: f::FT_Glyph = std::ptr::null_mut();
                if f::FT_Get_Glyph(self.slot(), &mut glyph) != 0 {
                    return None;
                }
                let mut b = f::FT_BBox { xMin: 0, yMin: 0, xMax: 0, yMax: 0 };
                f::FT_Glyph_Get_CBox(glyph, f::FT_GLYPH_BBOX_UNSCALED, &mut b);
                f::FT_Done_Glyph(glyph);
                Some(b)
            }
        }

        /// the advances (26.6) of these characters, with kerning and the
        /// faux bold's extra pixels, as Winamp measured
        fn advance(&self, s: &[u16]) -> i64 {
            let mut w: i64 = 0;
            let mut prev = 0;
            for &ch in s {
                unsafe {
                    let gi = f::FT_Get_Char_Index(self.face, ch as f::FT_ULong);
                    let mut delta = f::FT_Vector { x: 0, y: 0 };
                    if w > 0 && f::FT_HAS_KERNING(self.face) {
                        f::FT_Get_Kerning(self.face, prev, gi, f::FT_KERNING_DEFAULT, &mut delta);
                    }
                    if f::FT_Load_Glyph(self.face, gi, load_flags()) != 0 {
                        continue;
                    }
                    w += delta.x as i64 + (*self.slot()).advance.x as i64;
                    prev = gi;
                    w += ((self.passes() - 1) as i64) << 6;
                }
            }
            w
        }

        /// One glyph drawn into an ink plane (Winamp's drawChar): ink ORed
        /// in, faux bold laid again a pixel over at a time. Its advance.
        fn draw_char(&self, ink: &mut [u8], iw: i32, ih: i32, x0: i64, y0: i32, ch: u16, last: &mut u32) -> i64 {
            unsafe {
                let gi = f::FT_Get_Char_Index(self.face, ch as f::FT_ULong);
                let mut x0 = x0;
                let mut delta = f::FT_Vector { x: 0, y: 0 };
                if *last != 0 && f::FT_HAS_KERNING(self.face) {
                    f::FT_Get_Kerning(self.face, *last, gi, f::FT_KERNING_DEFAULT, &mut delta);
                    x0 += delta.x as i64;
                }
                if f::FT_Load_Glyph(self.face, gi, load_flags()) != 0 {
                    return 0;
                }
                let mut glyph: f::FT_Glyph = std::ptr::null_mut();
                if f::FT_Get_Glyph(self.slot(), &mut glyph) != 0 {
                    return 0;
                }
                self.slant(glyph);
                let mode = if self.antialias { f::FT_RENDER_MODE_NORMAL } else { f::FT_RENDER_MODE_MONO };
                if f::FT_Glyph_To_Bitmap(&mut glyph, mode, std::ptr::null_mut(), 1) != 0 {
                    f::FT_Done_Glyph(glyph);
                    return 0;
                }
                let bg = glyph as f::FT_BitmapGlyph;
                let (left, top) = ((*bg).left, (*bg).top);
                let bmp = &(*bg).bitmap;
                let (rows, bw, pitch) = (bmp.rows as i32, bmp.width as i32, bmp.pitch);
                let r = self.passes();
                let px = (x0 >> 6) as i32;
                let ys = 0.max(y0 - top);
                let ye = ih.min(y0 + rows - top);
                let xs = 0.max(px + left);
                let xe = iw.min(px + left + bw);
                let row0 = ys + top - y0;
                let col0 = xs - left - px;
                let len = ink.len() as i32;
                let gray = bmp.pixel_mode as u32 == f::FT_PIXEL_MODE_GRAY as u32;
                for z in 0..r {
                    for y in ys..ye {
                        let brow = (row0 + (y - ys)) * pitch;
                        for x in xs..xe {
                            let cov: u8 = if gray {
                                *bmp.buffer.offset((brow + col0 + (x - xs)) as isize)
                            } else {
                                let byte = *bmp.buffer.offset((brow + col0 + ((x - xs) >> 3)) as isize);
                                if byte & (1 << (7 - ((x - xs) & 7))) != 0 { 0xFF } else { 0 }
                            };
                            // (z passes run on along the row, as Winamp's did)
                            let i = y * iw + x + z;
                            if i < 0 || i >= len {
                                continue;
                            }
                            let i = i as usize;
                            if z == 0 || !gray {
                                ink[i] |= cov;
                            } else if x != iw - z {
                                ink[i] |= self.overlay(ink[i], cov, r, z);
                            }
                        }
                    }
                }
                f::FT_Done_Glyph(glyph);
                *last = gi;
                (*self.slot()).advance.x as i64 + delta.x as i64 + (((r - 1) as i64) << 6)
            }
        }

        /// faux bold's later layers: added, the last one softened at strength 2
        fn overlay(&self, c: u8, b: u8, r: i32, z: i32) -> u8 {
            let at = |i: u32, j: u32| (i * (j + 1)) >> 8;
            if self.bold == 2 && z == r - 1 && c < b {
                return (at(b as u32, 0xFF - 0xB0) + at(c as u32, 0xB0)) as u8;
            }
            (c as u32 + b as u32).min(255) as u8
        }

        /// A text drawn on a baseline from x (Winamp's drawText).
        fn draw_text(&self, ink: &mut [u8], iw: i32, ih: i32, x: i32, y: i32, s: &[u16]) {
            let mut pen = (x as i64) << 6;
            let mut last = 0u32;
            for &ch in s {
                pen += self.draw_char(ink, iw, ih, pen, y, ch, &mut last);
            }
        }

        /// The ink as the colour (Winamp's restoreCanvas: each channel and
        /// the alpha times the ink, >> 8) laid at (x, y).
        fn lay(&self, buf: &mut Buffer, ink: &[u8], iw: i32, ih: i32, x: i32, y: i32, color: [u8; 3]) {
            let mut px = vec![0u8; ink.len() * 4];
            for (i, &a) in ink.iter().enumerate() {
                if a == 0 {
                    continue;
                }
                let a = a as u32;
                px[i * 4] = ((color[2] as u32 * a) >> 8) as u8;
                px[i * 4 + 1] = ((color[1] as u32 * a) >> 8) as u8;
                px[i * 4 + 2] = ((color[0] as u32 * a) >> 8) as u8;
                px[i * 4 + 3] = ((255 * a) >> 8) as u8;
            }
            buf.blit_over(&px, iw, ih, x, y);
        }

        /// where a long line breaks (Winamp's find_break)
        fn find_break(&self, s: &[u16], width: i32) -> usize {
            let w = |n: usize| (self.advance(&s[..n]) >> 6) as i32;
            let space = |c: u16| matches!(c, 0x20 | 0x09 | 0x0A | 0x0B | 0x0C | 0x0D);
            if w(s.len()) <= width {
                return s.len();
            }
            let hard = s.iter().position(|&c| c == 0x0D || c == 0x0A).unwrap_or(s.len());
            if w(hard) <= width {
                return hard;
            }
            let mut soft = s.iter().position(|&c| space(c)).unwrap_or(s.len());
            if w(soft) <= width {
                let mut last;
                loop {
                    last = soft;
                    soft = last + 1 + s[(last + 1).min(s.len())..].iter().position(|&c| space(c)).unwrap_or(s.len().saturating_sub(last + 1));
                    if !(last < s.len() && w(soft.min(s.len())) <= width) {
                        break;
                    }
                }
                last
            } else {
                let mut i = 0;
                while i < s.len() && w(i) <= width {
                    i += 1;
                }
                i.saturating_sub(1)
            }
        }
    }

    impl Pen for Font {
        fn width(&self, s: &[u16]) -> i32 {
            if s.is_empty() {
                return 0;
            }
            // (the font's padding: -1 left, +1 right)
            (self.advance(s) >> 6) as i32 + 1 - 1
        }

        fn height(&self, s: &[u16]) -> i32 {
            if s.is_empty() {
                return 0;
            }
            let top = self.cbox('M').map(|b| b.yMax).unwrap_or(0);
            let bottom = self.cbox('g').map(|b| b.yMin).unwrap_or(0);
            let h = ((top - bottom) >> 6) as i32 + 2;
            let lines = s.iter().filter(|&&c| c == 0x0A).count() as i32;
            if lines > 0 { h * (lines + 1) } else { h }
        }

        fn text_rect(&self, buf: &mut Buffer, x: i32, y: i32, w: i32, h: i32, s: &[u16], align: u32, color: [u8; 3]) {
            if w <= 0 || h <= 0 {
                return;
            }
            let (iw, ih) = (w + 1, h);
            let mut ink = vec![0u8; (iw * ih) as usize];
            let width = self.width(s);
            let xstart = match align {
                2 => w - width,
                1 => (w - width) / 2,
                _ => 0,
            };
            self.draw_text(&mut ink, iw, ih, xstart, self.ascent(), s);
            self.lay(buf, &ink, iw, ih, x, y + 3, color);
        }

        fn text_wrapped(&self, buf: &mut Buffer, x: i32, y: i32, w: i32, h: i32, s: &[u16], _align: u32, color: [u8; 3]) {
            if w <= 0 || h <= 0 {
                return;
            }
            let (iw, ih) = (w + 1, h);
            let mut ink = vec![0u8; (iw * ih) as usize];
            let ascent = self.ascent();
            let descent = self.height(&"Mg".encode_utf16().collect::<Vec<_>>()) - ascent;
            let mut cur = 0usize;
            let mut yoff = ascent;
            while yoff < h {
                let next = cur + self.find_break(&s[cur..], w);
                self.draw_text(&mut ink, iw, ih, 0, yoff, &s[cur..next]);
                cur = next;
                while cur < s.len() && s[cur] == 0x20 {
                    cur += 1;
                }
                if cur < s.len() && (s[cur] == 0x0D || s[cur] == 0x0A) {
                    cur += 1;
                }
                if cur >= s.len() {
                    break;
                }
                yoff += ascent + descent;
            }
            self.lay(buf, &ink, iw, ih, x, y + 3, color);
        }

        fn text_at(&self, buf: &mut Buffer, x: i32, y: i32, s: &[u16], color: [u8; 3]) {
            let (w, h) = (self.width(s), self.height(s));
            if h <= 0 {
                return;
            }
            let (iw, ih) = (w + 1, h);
            let mut ink = vec![0u8; (iw * ih) as usize];
            self.draw_text(&mut ink, iw, ih, 0, self.ascent(), s);
            self.lay(buf, &ink, iw, ih, x, y + 3, color);
        }
    }
}

#[cfg(not(windows))]
pub fn paint(_p: &Paint) -> Option<Painted> {
    None
}

#[cfg(not(windows))]
pub fn load_font(_data: &[u8]) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_is_split_as_winamp_split_it() {
        assert_eq!(time_pieces("1:23", false), ("".into(), "1".into(), "23".into()));
        assert_eq!(time_pieces("1:02:03", true), ("1".into(), "02".into(), "03".into()));
        assert_eq!(time_pieces("1:23", true), ("".into(), "1".into(), "23".into()));
        assert_eq!(time_pieces("  : ", false), ("".into(), "  ".into(), " ".into()));
    }

    #[test]
    fn windows_1252_row() {
        assert_eq!(decode_1252(b"Caf\xe9 \x80"), "Café €");
    }
}
