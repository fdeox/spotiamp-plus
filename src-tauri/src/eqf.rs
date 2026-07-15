//! Import classic Winamp `.EQF` equalizer preset files.
//!
//! Format: a 31-byte signature (`Winamp EQ library file v1.1\x1a!--`) followed by
//! one or more preset entries. Each entry is a 257-byte null-padded name, then 11
//! bytes — the 10 band sliders (60/170/310/600/1k/3k/6k/12k/14k/16k Hz) and the
//! preamp last. Each slider is `0..=63` where 0 = +20 dB and 63 = -20 dB.

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

const MAGIC: &[u8] = b"Winamp EQ library file v1.1";
const HEADER_LEN: usize = 31;
const NAME_LEN: usize = 257;
const VALUES_LEN: usize = 11; // 10 bands + preamp
const ENTRY_LEN: usize = NAME_LEN + VALUES_LEN;

#[derive(Serialize)]
pub struct EqfPreset {
    name: String,
    /// Preamp gain in dB.
    preamp: f32,
    /// The 10 band gains in dB, low → high frequency.
    bands: Vec<f32>,
}

/// Winamp stores each slider as a byte in `0..=63`, linear from +20 dB (0) to
/// -20 dB (63); 31/32 is roughly flat.
fn byte_to_db(value: u8) -> f32 {
    20.0 - (value.min(63) as f32) * (40.0 / 63.0)
}

/// Parse the first preset out of a `.EQF`/`.q1`/`.q2` file.
fn parse_first_preset(bytes: &[u8]) -> Option<EqfPreset> {
    if !bytes.starts_with(MAGIC) {
        return None;
    }
    let entry = bytes.get(HEADER_LEN..HEADER_LEN + ENTRY_LEN)?;
    let name = entry[..NAME_LEN]
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as char)
        .collect::<String>();
    let values = &entry[NAME_LEN..NAME_LEN + VALUES_LEN];
    let bands = values[..10].iter().map(|&v| byte_to_db(v)).collect();
    let preamp = byte_to_db(values[10]);
    Some(EqfPreset {
        name: if name.trim().is_empty() {
            "Preset".to_string()
        } else {
            name
        },
        preamp,
        bands,
    })
}

/// Show a file picker and return the first preset in the chosen `.EQF` file (or
/// `None` when the user cancels). The frontend clamps the dB values to the EQ's
/// own ±12 dB range.
#[tauri::command]
pub async fn import_eqf(app_handle: AppHandle) -> Result<Option<EqfPreset>, String> {
    let Some(path) = app_handle
        .dialog()
        .file()
        .add_filter("Winamp EQ preset", &["eqf", "q1", "q2"])
        .blocking_pick_file()
        .and_then(|file_path| file_path.into_path().ok())
    else {
        return Ok(None); // cancelled
    };

    let bytes = std::fs::read(&path).map_err(|e| format!("Could not read file ({e})"))?;
    parse_first_preset(&bytes)
        .map(Some)
        .ok_or_else(|| "Not a valid Winamp .EQF preset file".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synth_eqf(name: &str, bands: [u8; 10], preamp: u8) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(MAGIC); // 27 bytes
        v.extend_from_slice(&[0x1A, b'!', b'-', b'-']); // -> 31-byte header
        let mut name_field = [0u8; NAME_LEN];
        name_field[..name.len()].copy_from_slice(name.as_bytes());
        v.extend_from_slice(&name_field);
        v.extend_from_slice(&bands);
        v.push(preamp);
        v
    }

    #[test]
    fn parses_name_bands_and_preamp() {
        let file = synth_eqf("Rock", [0, 63, 31, 31, 31, 31, 31, 31, 31, 31], 0);
        let preset = parse_first_preset(&file).expect("valid preset");
        assert_eq!(preset.name, "Rock");
        assert_eq!(preset.bands.len(), 10);
        // 0 -> +20 dB, 63 -> -20 dB, 31 -> ~0 dB, and the preamp (0) -> +20 dB.
        assert!((preset.bands[0] - 20.0).abs() < 0.01);
        assert!((preset.bands[1] + 20.0).abs() < 0.01);
        assert!(preset.bands[2].abs() < 0.5);
        assert!((preset.preamp - 20.0).abs() < 0.01);
    }

    #[test]
    fn rejects_non_eqf() {
        assert!(parse_first_preset(b"not an eqf file at all").is_none());
    }
}
