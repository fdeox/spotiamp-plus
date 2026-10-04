//! Loading classic Winamp 2.x skins (.wsz — a plain zip of BMP sprite sheets).
//! The sheets are extracted once into the config dir and served to every
//! window as data-URLs that override the `--skin-*` CSS variables.

use std::{collections::HashMap, io::Read, path::PathBuf};

use base64::Engine;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

use crate::settings::{Settings, get_config_dir};

/// Find the centre X of the title plate in a GEN.BMP titlebar, or None when the
/// titlebar is smooth and has no distinct plate.
///
/// A classic titlebar is a strip of horizontal "grip" bars with a flat plate
/// left clear in the middle for the title. Scanning down a column, a bar column
/// crosses those bright lines (high variance) while a plate column is near-flat
/// (low variance). The plate is the widest run of low-variance columns in the
/// central region — but only if it stays well short of the whole width, because
/// a smooth titlebar reads as low-variance everywhere and must NOT be mistaken
/// for a plate. Verified against six real skins (Winamp3/5, Nucleo all resolve
/// to the same centre; Bento and Sony CDX correctly report no plate).
fn locate_title_plate(sheet: &image::DynamicImage) -> Option<u32> {
    use image::GenericImageView;
    let (w, h) = sheet.dimensions();
    if w < 80 || h < 12 {
        return None;
    }
    let bright = |x: u32, y: u32| -> f32 {
        let p = sheet.get_pixel(x, y).0;
        (p[0] as f32 + p[1] as f32 + p[2] as f32) / 3.0
    };
    // Per-column brightness variance over the plate's clear band (rows 4..=10).
    let variance: Vec<f32> = (0..w)
        .map(|x| {
            let (mut s, mut s2) = (0.0f32, 0.0f32);
            for y in 4..=10 {
                let v = bright(x, y);
                s += v;
                s2 += v * v;
            }
            let n = 7.0;
            (s2 / n - (s / n).powi(2)).max(0.0)
        })
        .collect();

    // Search the centre, skipping the corner pieces on each side.
    let lo = 24usize;
    let hi = (w as usize).saturating_sub(28);
    if hi <= lo {
        return None;
    }
    // Adaptive threshold: a low percentile of the central variances, so it
    // scales to whatever brightness a given skin's bars have.
    let mut central: Vec<f32> = variance[lo..hi].to_vec();
    central.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let threshold = central[central.len() * 35 / 100] * 3.0 + 2.0;

    let (mut best, mut cur): (Option<(usize, usize)>, Option<(usize, usize)>) = (None, None);
    for x in lo..hi {
        if variance[x] <= threshold {
            cur = Some(match cur {
                Some((a, _)) => (a, x),
                None => (x, x),
            });
        } else if let Some(run) = cur.take() {
            if best.is_none_or(|b| run.1 - run.0 > b.1 - b.0) {
                best = Some(run);
            }
        }
    }
    if let Some(run) = cur {
        if best.is_none_or(|b| run.1 - run.0 > b.1 - b.0) {
            best = Some(run);
        }
    }

    let (a, b) = best?;
    // A run spanning most of the width means a smooth titlebar, not a plate.
    if (b - a + 1) as f32 > w as f32 * 0.45 {
        return None;
    }
    Some(((a + b) / 2) as u32)
}

/// Classic skins shipped inside the binary (embedded so they always ship with
/// the installer — external bundle resources don't survive NSIS reliably).
/// `(display name, .wsz bytes)`.
const BUNDLED_SKINS: &[(&str, &[u8])] = &[
    // our own: the base skin in the logo's colours, with SPOTIAMP+ titles
    ("Spotiamp+", include_bytes!("../../skins/Spotiamp+.wsz")),
    (
        "Bento Classified",
        include_bytes!("../../skins/Bento_Classified.wsz"),
    ),
    (
        "Winamp3 Classified",
        include_bytes!("../../skins/Winamp3_Classified_v5.5.wsz"),
    ),
    (
        "Winamp5 Classified",
        include_bytes!("../../skins/Winamp5_Classified_v5.5.wsz"),
    ),
    (
        "Nucleo NLog",
        include_bytes!("../../skins/Nucleo-NLog-2G1.wsz"),
    ),
    (
        "Sony CDX-MP3",
        include_bytes!("../../skins/Sony CDX-MP3.wsz"),
    ),
    (
        "Sony Esprit V2",
        include_bytes!("../../skins/Sony_Esprit_V2.wsz"),
    ),
];

/// Extra skin files that aren't 1:1 sprite sheets: GEN.BMP is cropped into the
/// generic-window titlebar tiles (library/visualizer), PLEDIT.TXT carries the
/// playlist colours.
const EXTRA_FILES: [&str; 3] = ["GEN.BMP", "GENEX.BMP", "PLEDIT.TXT"];

/// The sprite sheets we can re-skin, mapped to their CSS variable suffix.
/// (.CUR cursors keep the base skin for now.)
const SPRITES: [(&str, &str); 13] = [
    ("MAIN.BMP", "main"),
    ("CBUTTONS.BMP", "cbuttons"),
    ("MONOSTER.BMP", "monoster"),
    ("NUMBERS.BMP", "numbers"),
    ("PLAYPAUS.BMP", "playpaus"),
    ("PLEDIT.BMP", "pledit"),
    ("POSBAR.BMP", "posbar"),
    ("SHUFREP.BMP", "shufrep"),
    ("TEXT.BMP", "text"),
    ("TITLEBAR.BMP", "titlebar"),
    ("VOLUME.BMP", "volume"),
    ("BALANCE.BMP", "balance"),
    ("EQMAIN.BMP", "eqmain"),
];

fn custom_skin_dir() -> Option<PathBuf> {
    get_config_dir().map(|dir| dir.join("custom-skin"))
}

/// The list of files the current skin brought, written when it's extracted.
const SKIN_FILE_LIST: &str = "SKIN-FILES.TXT";

/// The extracted skin's files, as listed when it was put on. Reads go through
/// this, so a file from the skin before that couldn't be deleted (Windows can
/// hold one for a moment, an antivirus scanning it) never bleeds into this one:
/// a stale GENEX.BMP gave pink skins the previous skin's grey buttons. A skin
/// extracted before the list existed reads everything, as it used to.
struct SkinFiles {
    dir: PathBuf,
    listed: Option<std::collections::HashSet<String>>,
}

impl SkinFiles {
    fn open(dir: PathBuf) -> Self {
        let listed = std::fs::read_to_string(dir.join(SKIN_FILE_LIST)).ok().map(|text| {
            text.lines()
                .map(|line| line.trim().to_uppercase())
                .filter(|line| !line.is_empty())
                .collect()
        });
        Self { dir, listed }
    }

    fn has(&self, name: &str) -> bool {
        self.listed.as_ref().is_none_or(|listed| listed.contains(name)) && self.dir.join(name).exists()
    }

    fn read(&self, name: &str) -> Option<Vec<u8>> {
        if !self.has(name) {
            return None;
        }
        std::fs::read(self.dir.join(name)).ok()
    }
}

/// Extract the BMP sprite sheets from a .wsz/.zip file into the config dir.
fn extract_wsz(path: &std::path::Path) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Could not open skin ({e})"))?;
    extract_wsz_bytes(&bytes)
}

/// Extract the BMP sprite sheets from raw .wsz/.zip bytes into the config dir.
fn extract_wsz_bytes(bytes: &[u8]) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("Not a valid .wsz/.zip ({e})"))?;

    let dir = custom_skin_dir().ok_or("no config dir")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create skin dir ({e})"))?;
    // Delete every file from a previously-loaded skin, one by one
    // (`remove_dir_all` can fail silently on Windows). A file can still be held
    // for a moment (the antivirus scanning it), so try a few times; whatever
    // stays is left out of the new skin's file list anyway (SkinFiles).
    for attempt in 0..5 {
        let mut left = false;
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                left |= std::fs::remove_file(entry.path()).is_err();
            }
        }
        if !left {
            break;
        }
        if attempt < 4 {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    let mut written = Vec::new();
    let mut found_main = false;
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| format!("Bad zip entry ({e})"))?;
        if !entry.is_file() {
            continue;
        }
        // skins usually nest the sheets in a folder — match by basename
        let name = entry
            .name()
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("")
            .to_uppercase();
        if SPRITES.iter().any(|(bmp, _)| *bmp == name)
            || name == "NUMS_EX.BMP"
            || EXTRA_FILES.contains(&name.as_str())
        {
            // Real sprite sheets are a few hundred KB at most; refuse anything
            // that would unpack to something huge (skins also come from the
            // internet now, via the Skin Museum).
            if entry.size() > 16 * 1024 * 1024 {
                return Err(format!("{name} in this skin is far too large"));
            }
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|e| format!("Could not read {name} ({e})"))?;
            std::fs::write(dir.join(&name), bytes)
                .map_err(|e| format!("Could not save {name} ({e})"))?;
            if name == "MAIN.BMP" {
                found_main = true;
            }
            written.push(name);
        }
    }
    if !found_main {
        return Err("No MAIN.BMP in the archive — not a Winamp 2.x skin".into());
    }
    std::fs::write(dir.join(SKIN_FILE_LIST), written.join("\n"))
        .map_err(|e| format!("Could not save the skin's file list ({e})"))?;
    Ok(())
}

/// Open a file picker for a .wsz and activate it as the current skin.
/// Returns the skin's file name, or None when the user cancelled.
#[tauri::command]
pub async fn pick_and_load_skin(app_handle: AppHandle) -> Result<Option<String>, String> {
    let Some(path) = app_handle
        .dialog()
        .file()
        .add_filter("Winamp skin", &["wsz", "zip"])
        .blocking_pick_file()
        .and_then(|file_path| file_path.into_path().ok())
    else {
        return Ok(None); // cancelled
    };

    extract_wsz(&path)?;
    Settings::current_mut().skin = "custom".to_string();
    Ok(Some(
        path.file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default(),
    ))
}

/// Put on a skin from raw .wsz bytes (the Skin Museum's downloads).
pub fn apply_wsz_bytes(bytes: &[u8]) -> Result<(), String> {
    extract_wsz_bytes(bytes)
}

/// The display names of the skins embedded in the binary, for the skin menu.
#[tauri::command]
pub fn list_bundled_skins() -> Vec<String> {
    BUNDLED_SKINS
        .iter()
        .map(|(name, _)| name.to_string())
        .collect()
}

/// Activate one of the embedded skins by display name (from list_bundled_skins).
#[tauri::command(async)]
pub fn load_bundled_skin(name: String) -> Result<(), String> {
    let Some((_, bytes)) = BUNDLED_SKINS.iter().find(|(n, _)| *n == name) else {
        return Err(format!("bundled skin '{name}' not found"));
    };
    extract_wsz_bytes(bytes)?;
    Settings::current_mut().skin = "custom".to_string();
    Ok(())
}

/// The extracted custom skin as data-URLs keyed by CSS variable suffix
/// ("main" → `--skin-main`). Sheets a skin doesn't ship are simply absent —
/// the frontend keeps the base art for those.
#[tauri::command(async)]
pub fn get_custom_skin() -> Result<HashMap<String, String>, String> {
    let files = SkinFiles::open(custom_skin_dir().ok_or("no config dir")?);
    let mut sprites = HashMap::new();
    for (bmp, var) in SPRITES {
        let mut name = bmp;
        // some skins ship NUMS_EX.BMP instead of NUMBERS.BMP
        if var == "numbers" && !files.has(name) {
            name = "NUMS_EX.BMP";
        }
        // skins without BALANCE.BMP reuse the volume art (Winamp behaviour)
        if var == "balance" && !files.has(name) {
            name = "VOLUME.BMP";
        }
        let Some(bytes) = files.read(name) else {
            continue;
        };
        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
        sprites.insert(var.to_string(), format!("data:image/bmp;base64,{b64}"));
    }

    // Winamp draws the playlist's time display in TEXT.BMP's letters; ours is
    // plain text, so it takes their colour (skins without one keep the green)
    if let Some(c) = files
        .read("TEXT.BMP")
        .and_then(|b| image::load_from_memory(&b).ok())
        .and_then(|sheet| text_colour(&sheet))
    {
        sprites.insert("textcolor".into(), format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]));
    }

    // The library / visualizer / lyrics titlebars are built from three tiles —
    // left corner, repeating fill, right corner — plus an optional title plate.
    //
    // GEN.BMP is the proper source, but most classic 2.x skins never shipped it
    // (it's a later addition), so those windows used to fall back to the default
    // blue on the majority of loaded skins. PLEDIT.BMP, on the other hand, is in
    // every skin — so when GEN.BMP is missing we take the equivalent pieces from
    // the playlist titlebar instead, and the windows finally match the skin.
    let gen_img = files.read("GEN.BMP").and_then(|b| image::load_from_memory(&b).ok());
    let from_gen = gen_img.is_some();
    let sheet = gen_img.or_else(|| {
        files.read("PLEDIT.BMP").and_then(|b| image::load_from_memory(&b).ok())
    });

    if let Some(sheet) = sheet {
        // Left corner, repeating fill, right corner. In PLEDIT the plain-bars
        // fill is at x=127 — the title area at 26..126 has "WINAMP PLAYLIST"
        // baked in, so it can't be used as a generic tile.
        let tiles = if from_gen {
            [
                ("gentl", 0u32, 0u32, 25u32, 20u32),
                ("genfill", 82, 0, 8, 20),
                ("gentr", 140, 0, 15, 20),
            ]
        } else {
            [
                ("gentl", 0, 0, 25, 20),
                ("genfill", 127, 0, 25, 20),
                ("gentr", 153, 0, 25, 20),
            ]
        };
        for (var, x, y, w, h) in tiles {
            let tile = sheet.crop_imm(x, y, w, h);
            let mut png = Vec::new();
            if tile
                .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .is_ok()
            {
                let b64 = base64::engine::general_purpose::STANDARD.encode(png);
                sprites.insert(var.to_string(), format!("data:image/png;base64,{b64}"));
            }
        }

        // Sample the titlebar's own colour so the window frame and the title
        // text key off it, not off the window body (genexwndbg). Those two are
        // often different — which is exactly why a gold-bodied skin ended up
        // with a gold frame wrapped around a silver titlebar. The frame then
        // matches the titlebar, and the title text picks black or white for
        // contrast so it stays readable on every skin (this is what fixes the
        // unreadable dark title text some skins shipped).
        let (fx, fw) = if from_gen { (82u32, 8u32) } else { (127u32, 25u32) };
        {
            use image::GenericImageView;
            let (sw, sh) = sheet.dimensions();
            let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
            for yy in 0..20u32.min(sh) {
                for xx in fx..(fx + fw).min(sw) {
                    let p = sheet.get_pixel(xx, yy).0;
                    r += p[0] as u64;
                    g += p[1] as u64;
                    b += p[2] as u64;
                    n += 1;
                }
            }
            if n > 0 {
                let (r, g, b) = ((r / n) as u8, (g / n) as u8, (b / n) as u8);
                sprites.insert("titlebarcolor".into(), format!("#{r:02X}{g:02X}{b:02X}"));
                let lum = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
                let text = if lum > 140.0 { "#101014" } else { "#f0f2f8" };
                sprites.insert("titletext".into(), text.into());
            }
        }

        // The title plate is the plain area some GEN.BMP titlebars leave between
        // the bars. Its X isn't fixed — a hardcoded offset hit the wrong pixels
        // on other skins — so it's found by scanning (see locate_title_plate).
        // PLEDIT has no textless plate, and neither do smooth GEN titlebars, so
        // in those cases the title just sits on the bars (transparent plate).
        let plate = if from_gen {
            locate_title_plate(&sheet)
        } else {
            None
        };
        match plate {
            Some(center) => {
                let tile = sheet.crop_imm(center.saturating_sub(2), 0, 4, 20);
                let mut png = Vec::new();
                if tile
                    .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                    .is_ok()
                {
                    let b64 = base64::engine::general_purpose::STANDARD.encode(png);
                    sprites.insert("gentitle".to_string(), format!("data:image/png;base64,{b64}"));
                }
            }
            None => {
                sprites.insert("gentitle".to_string(), "transparent".to_string());
            }
        }
    }

    // GENEX.BMP is how real Winamp colours plugin windows (the media library):
    // single pixels along the top row define the UI palette, and the sheet
    // carries the generic button face (normal + pressed).
    if let Some(genex_bytes) = files.read("GENEX.BMP")
        && let Ok(genex) = image::load_from_memory(&genex_bytes)
    {
        use image::GenericImageView;
        // documented colour pixels at (x, 0)
        for (var, x) in [
            ("genexitembg", 48u32),  // list/edit background
            ("genexitemfg", 50),     // list/edit text (the classic green)
            ("genexwndbg", 52),      // window background
            ("genexbtntext", 54),    // button label
            ("genexwndtext", 56),    // window text / labels
            ("genexdivider", 58),    // dividers and sunken borders
            ("genexselbg", 60),      // list selection bar
            ("genexhdrbg", 62),      // listview header background
            ("genexhdrtext", 64),    // listview header text
        ] {
            if x < genex.width() && genex.height() > 0 {
                let p = genex.get_pixel(x, 0);
                sprites.insert(
                    var.to_string(),
                    format!("#{:02X}{:02X}{:02X}", p[0], p[1], p[2]),
                );
            }
        }
        // generic button face, normal + pressed (used via border-image)
        for (var, y) in [("genexbtn", 0u32), ("genexbtnp", 15u32)] {
            if genex.width() >= 47 && genex.height() >= y + 15 {
                let tile = genex.crop_imm(0, y, 47, 15);
                let mut png = Vec::new();
                if tile
                    .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                    .is_ok()
                {
                    let b64 = base64::engine::general_purpose::STANDARD.encode(png);
                    sprites.insert(var.to_string(), format!("data:image/png;base64,{b64}"));
                }
            }
        }
    }

    // Playlist colours (PLEDIT.TXT) — also drive the library list. Values are
    // plain #RRGGBB strings (the frontend sets them raw, not as url()).
    if let Some(bytes) = files.read("PLEDIT.TXT") {
        let text = String::from_utf8_lossy(&bytes);
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim().trim_matches('"');
            if !value.starts_with('#') {
                continue;
            }
            let Some(value) = value.get(..7).map(str::to_string) else {
                continue;
            };
            match key.trim().to_ascii_lowercase().as_str() {
                "normal" => {
                    sprites.insert("plnormal".to_string(), value);
                }
                "current" => {
                    sprites.insert("plcurrent".to_string(), value);
                }
                "normalbg" => {
                    sprites.insert("plbg".to_string(), value);
                }
                "selectedbg" => {
                    sprites.insert("plselbg".to_string(), value);
                }
                _ => {}
            }
        }
    }

    // Most classic 2.x skins ship no GENEX.BMP (the media-library palette came
    // later). Without it the library would keep the BASE genex colours, so it
    // wouldn't follow the loaded skin. Derive the library palette from the
    // skin's playlist colours (PLEDIT.TXT) instead, so EVERY skin re-colours it.
    if !sprites.contains_key("genexwndbg") {
        let derive = |sprites: &mut HashMap<String, String>, key: &str, from: &str| {
            if let Some(v) = sprites.get(from).cloned() {
                sprites.entry(key.to_string()).or_insert(v);
            }
        };
        derive(&mut sprites, "genexwndbg", "plbg");
        derive(&mut sprites, "genexitembg", "plbg");
        derive(&mut sprites, "genexhdrbg", "plbg");
        derive(&mut sprites, "genexitemfg", "plnormal");
        derive(&mut sprites, "genexwndtext", "plnormal");
        derive(&mut sprites, "genexhdrtext", "plnormal");
        derive(&mut sprites, "genexbtntext", "plnormal");
        derive(&mut sprites, "genexselbg", "plselbg");
        derive(&mut sprites, "genexdivider", "plselbg");
    }

    // No button face of its own either: the library buttons borrowed the base
    // skin's grey one, with this skin's label colour on it (pink on grey, hard
    // to read, out of place). "none" makes the page draw them in the skin's
    // frame colour instead, with a label colour that reads on that face.
    if !sprites.contains_key("genexbtn") {
        sprites.insert("genexbtn".into(), "none".into());
        sprites.insert("genexbtnp".into(), "none".into());
        let frame = ["titlebarcolor", "genexwndbg", "plbg"]
            .iter()
            .find_map(|key| sprites.get(*key).and_then(|v| hex_rgb(v)));
        if let Some(frame) = frame {
            let own = sprites.get("genexbtntext").and_then(|v| hex_rgb(v));
            let text = button_label_colour(frame, own);
            sprites.insert(
                "genexbtntext".into(),
                format!("#{:02X}{:02X}{:02X}", text[0], text[1], text[2]),
            );
        }
    }

    Ok(sprites)
}

fn hex_rgb(value: &str) -> Option<[u8; 3]> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// WCAG contrast ratio between two colours (1 = none, 21 = black on white).
fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    let luminance = |c: [u8; 3]| {
        let lin = |v: u8| {
            let s = v as f64 / 255.0;
            if s <= 0.03928 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
    };
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// The label colour for a library button drawn in the frame colour (the
/// page's face: 82 % frame, 18 % white): the skin's own if it reads well
/// there, else near-black or near-white, whichever reads better.
fn button_label_colour(frame: [u8; 3], own: Option<[u8; 3]>) -> [u8; 3] {
    let face = frame.map(|c| (c as f64 * 0.82 + 255.0 * 0.18).round() as u8);
    match own {
        Some(own) if contrast(own, face) >= 4.5 => own,
        _ => {
            let (dark, light) = ([16, 16, 20], [240, 242, 248]);
            if contrast(dark, face) >= contrast(light, face) { dark } else { light }
        }
    }
}

/// The colour of TEXT.BMP's letters: the commonest colour that stands out
/// from the commonest one (the background). None when nothing does.
fn text_colour(sheet: &image::DynamicImage) -> Option<[u8; 3]> {
    let mut counts: HashMap<[u8; 3], u32> = HashMap::new();
    for p in sheet.to_rgb8().pixels() {
        *counts.entry(p.0).or_default() += 1;
    }
    let bg = *counts.iter().max_by_key(|(c, n)| (**n, **c))?.0;
    counts
        .into_iter()
        .filter(|(c, _)| contrast(*c, bg) >= 3.0)
        .max_by_key(|(c, n)| (*n, *c))
        .map(|(c, _)| c)
}

#[cfg(test)]
mod text_colour_tests {
    use super::*;

    #[test]
    fn the_letters_not_the_background_or_their_soft_edges() {
        let mut sheet = image::RgbImage::from_pixel(20, 6, image::Rgb([0, 0, 0]));
        for x in 0..12 {
            sheet.put_pixel(x, 2, image::Rgb([255, 160, 40])); // letters
        }
        for x in 0..4 {
            sheet.put_pixel(x, 3, image::Rgb([40, 24, 6])); // their dim edges
        }
        let sheet = image::DynamicImage::ImageRgb8(sheet);
        assert_eq!(text_colour(&sheet), Some([255, 160, 40]));
        // one colour only: nothing stands out
        let flat = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(4, 4, image::Rgb([9, 9, 9])));
        assert_eq!(text_colour(&flat), None);
    }
}

#[cfg(test)]
mod button_label_tests {
    use super::*;

    #[test]
    fn keeps_a_readable_skin_colour_and_replaces_an_unreadable_one() {
        // light grey frame, the skin's dark label: kept
        assert_eq!(button_label_colour([200, 200, 200], Some([20, 20, 60])), [20, 20, 60]);
        // hot pink frame, a pale pink label: swapped for one that reads
        assert_eq!(button_label_colour([230, 80, 160], Some([255, 200, 230])), [16, 16, 20]);
        // dark frame, no label colour: light text
        assert_eq!(button_label_colour([30, 20, 40], None), [240, 242, 248]);
        assert_eq!(hex_rgb("#FF8000"), Some([255, 128, 0]));
        assert_eq!(hex_rgb("none"), None);
    }
}

#[cfg(test)]
mod skin_files_tests {
    use super::*;

    #[test]
    fn a_file_left_from_the_skin_before_is_ignored() {
        let dir = std::env::temp_dir().join(format!("spotiamp-skinfiles-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("MAIN.BMP"), b"main").unwrap();
        std::fs::write(dir.join("GENEX.BMP"), b"stale").unwrap();

        // no list yet (a skin put on by an older version): everything reads
        let files = SkinFiles::open(dir.clone());
        assert!(files.has("MAIN.BMP") && files.has("GENEX.BMP"));

        // listed: only what this skin brought
        std::fs::write(dir.join(SKIN_FILE_LIST), "MAIN.BMP\nPLEDIT.TXT").unwrap();
        let files = SkinFiles::open(dir.clone());
        assert_eq!(files.read("MAIN.BMP").as_deref(), Some(&b"main"[..]));
        assert!(!files.has("GENEX.BMP"));
        assert!(files.read("GENEX.BMP").is_none());
        // listed but not on disk
        assert!(!files.has("PLEDIT.TXT"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
