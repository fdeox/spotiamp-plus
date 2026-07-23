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
    // Delete every file from a previously-loaded skin. `remove_dir_all` can fail
    // silently on Windows (a lingering handle from the antivirus/indexer), which
    // used to leave stale sheets behind — e.g. a GENEX.BMP from one skin bleeding
    // into the next skin's media library. Removing files one by one is robust.
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let _ = std::fs::remove_file(entry.path());
        }
    }

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
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|e| format!("Could not read {name} ({e})"))?;
            std::fs::write(dir.join(&name), bytes)
                .map_err(|e| format!("Could not save {name} ({e})"))?;
            if name == "MAIN.BMP" {
                found_main = true;
            }
        }
    }
    if !found_main {
        return Err("No MAIN.BMP in the archive — not a Winamp 2.x skin".into());
    }
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

/// The display names of the skins embedded in the binary, for the skin menu.
#[tauri::command]
pub fn list_bundled_skins() -> Vec<String> {
    BUNDLED_SKINS
        .iter()
        .map(|(name, _)| name.to_string())
        .collect()
}

/// Activate one of the embedded skins by display name (from list_bundled_skins).
#[tauri::command]
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
#[tauri::command]
pub fn get_custom_skin() -> Result<HashMap<String, String>, String> {
    let dir = custom_skin_dir().ok_or("no config dir")?;
    let mut sprites = HashMap::new();
    for (bmp, var) in SPRITES {
        let mut path = dir.join(bmp);
        // some skins ship NUMS_EX.BMP instead of NUMBERS.BMP
        if var == "numbers" && !path.exists() {
            path = dir.join("NUMS_EX.BMP");
        }
        // skins without BALANCE.BMP reuse the volume art (Winamp behaviour)
        if var == "balance" && !path.exists() {
            path = dir.join("VOLUME.BMP");
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
        sprites.insert(var.to_string(), format!("data:image/bmp;base64,{b64}"));
    }

    // The library / visualizer / lyrics titlebars are built from three tiles —
    // left corner, repeating fill, right corner — plus an optional title plate.
    //
    // GEN.BMP is the proper source, but most classic 2.x skins never shipped it
    // (it's a later addition), so those windows used to fall back to the default
    // blue on the majority of loaded skins. PLEDIT.BMP, on the other hand, is in
    // every skin — so when GEN.BMP is missing we take the equivalent pieces from
    // the playlist titlebar instead, and the windows finally match the skin.
    let gen_img = std::fs::read(dir.join("GEN.BMP"))
        .ok()
        .and_then(|b| image::load_from_memory(&b).ok());
    let from_gen = gen_img.is_some();
    let sheet = gen_img.or_else(|| {
        std::fs::read(dir.join("PLEDIT.BMP"))
            .ok()
            .and_then(|b| image::load_from_memory(&b).ok())
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
    if let Ok(genex_bytes) = std::fs::read(dir.join("GENEX.BMP"))
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
    if let Ok(bytes) = std::fs::read(dir.join("PLEDIT.TXT")) {
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

    Ok(sprites)
}
