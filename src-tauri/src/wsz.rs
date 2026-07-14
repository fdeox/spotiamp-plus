//! Loading classic Winamp 2.x skins (.wsz — a plain zip of BMP sprite sheets).
//! The sheets are extracted once into the config dir and served to every
//! window as data-URLs that override the `--skin-*` CSS variables.

use std::{collections::HashMap, fs::File, io::Read, path::PathBuf};

use base64::Engine;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

use crate::settings::{Settings, get_config_dir};

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

/// Extract the BMP sprite sheets from a .wsz/.zip into the config dir.
fn extract_wsz(path: &std::path::Path) -> Result<(), String> {
    let file = File::open(path).map_err(|e| format!("Could not open skin ({e})"))?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|e| format!("Not a valid .wsz/.zip ({e})"))?;

    let dir = custom_skin_dir().ok_or("no config dir")?;
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create skin dir ({e})"))?;

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

/// The .wsz files bundled with the app (installer resources; the repo's
/// `skins/` folder in dev). Returned as file stems for the skin menu.
#[tauri::command]
pub fn list_bundled_skins(app_handle: AppHandle) -> Vec<String> {
    let Ok(dir) = app_handle.path().resolve("skins", BaseDirectory::Resource) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()?.to_ascii_lowercase() != "wsz" {
                return None;
            }
            Some(path.file_stem()?.to_string_lossy().into_owned())
        })
        .collect();
    names.sort();
    names
}

/// Activate one of the bundled skins by file stem (from `list_bundled_skins`).
#[tauri::command]
pub fn load_bundled_skin(name: String, app_handle: AppHandle) -> Result<(), String> {
    // stems only — no path tricks
    if name.contains(['/', '\\']) || name.contains("..") {
        return Err("invalid skin name".to_string());
    }
    let dir = app_handle
        .path()
        .resolve("skins", BaseDirectory::Resource)
        .map_err(|e| format!("no bundled skins dir ({e})"))?;
    let path = dir.join(format!("{name}.wsz"));
    if !path.exists() {
        return Err(format!("bundled skin '{name}' not found"));
    }
    extract_wsz(&path)?;
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

    // The library/visualizer titlebars use three tiles cropped out of the
    // generic-window sheet (GEN.BMP): left corner, repeating fill, right
    // corner with the close button.
    if let Ok(gen_bytes) = std::fs::read(dir.join("GEN.BMP"))
        && let Ok(gen_sheet) = image::load_from_memory(&gen_bytes)
    {
        for (var, x, y, w, h) in [
            ("gentl", 0u32, 0u32, 25u32, 20u32),
            ("genfill", 82, 0, 8, 20),
            ("gentr", 140, 0, 15, 20),
        ] {
            let tile = gen_sheet.crop_imm(x, y, w, h);
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

    Ok(sprites)
}
