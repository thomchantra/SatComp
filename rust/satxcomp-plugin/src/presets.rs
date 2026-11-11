//! Presets: JSON files of parameter values keyed by parameter id, plus the three custom waveshaper curves.
//! Factory bank = every `presets/*.json` embedded at build time (+ built-in INIT).
//! User presets = `*.satxcomp` files; the dropdown lists the user folder, "Load preset..." opens any file.

use crate::params::TABLE;
use include_dir::{include_dir, Dir};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

static FACTORY_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/presets");

pub const EXT: &str = "satxcomp";
/// Never stored in a preset: Bypass is a performance switch, Gain Match is a level-dependent measurement, the sidechain
/// switch depends on the host's routing.
pub const EXCLUDED: [&str; 3] = ["bypass", "gain_match", "sc_on"];

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Preset {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub params: BTreeMap<String, f32>,
    /// Custom waveshaper curve points of the low, mid and high band (absent / empty = all linear).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub curves: Vec<Vec<(f32, f32)>>,
}

impl Preset {
    pub fn new(name: &str, params: BTreeMap<String, f32>) -> Self {
        Self { format: "satxcomp-preset".into(), version: 1, name: name.into(), params, curves: Vec::new() }
    }

    /// All defaults.
    pub fn init() -> Self {
        let params = TABLE.iter().filter(|e| !EXCLUDED.contains(&e.1)).map(|e| (e.1.to_string(), e.4)).collect();
        Self::new("INIT", params)
    }

    pub fn from_json(s: &str) -> Option<Self> {
        let p: Preset = serde_json::from_str(s).ok()?;
        (p.format == "satxcomp-preset").then_some(p)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Curve points of band `b` (empty slice = linear).
    pub fn curve(&self, b: usize) -> &[(f32, f32)] {
        self.curves.get(b).map(|v| &v[..]).unwrap_or(&[])
    }
}

pub fn factory() -> Vec<Preset> {
    let mut v = vec![Preset::init()];
    let mut files: Vec<_> = FACTORY_DIR
        .files()
        .filter(|f| f.path().extension().map(|e| e == "json").unwrap_or(false))
        .collect();
    files.sort_by_key(|f| f.path().to_path_buf());
    for f in files {
        if let Some(mut p) = f.contents_utf8().and_then(Preset::from_json) {
            if p.name.is_empty() {
                p.name = f.path().file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            }
            v.push(p);
        }
    }
    v
}

/// ~/Library/Application Support/SatXComp/Presets (macOS), %APPDATA%\SatXComp\Presets (Windows), ~/.config/SatXComp/Presets
pub fn user_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"));
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(not(any(target_os = "macos", windows)))]
    let base = std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"));
    base.map(|b| b.join("SatXComp").join("Presets"))
}

/// (display name, path) of the user presets, sorted case-insensitively.
pub fn scan_user() -> Vec<(String, PathBuf)> {
    let mut v = Vec::new();
    if let Some(d) = user_dir() {
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().map(|x| x == EXT).unwrap_or(false) {
                    if let Some(stem) = p.file_stem() {
                        v.push((stem.to_string_lossy().into_owned(), p));
                    }
                }
            }
        }
    }
    v.sort_by_key(|a| a.0.to_lowercase());
    v
}

pub fn load_file(path: &Path) -> Option<Preset> {
    Preset::from_json(&std::fs::read_to_string(path).ok()?)
}

pub fn save_file(path: &Path, p: &Preset) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(path, p.to_json())
}

#[cfg(any(target_os = "macos", windows))]
pub fn open_dialog() -> Option<PathBuf> {
    let mut d = rfd::FileDialog::new().add_filter("SatXComp preset", &[EXT, "json"]).set_title("Load preset");
    if let Some(u) = user_dir().filter(|u| u.exists()) {
        d = d.set_directory(u);
    }
    d.pick_file()
}

#[cfg(any(target_os = "macos", windows))]
pub fn save_dialog(default_name: &str) -> Option<PathBuf> {
    let mut d = rfd::FileDialog::new()
        .add_filter("SatXComp preset", &[EXT])
        .set_title("Save preset")
        .set_file_name(format!("{default_name}.{EXT}"));
    if let Some(u) = user_dir() {
        let _ = std::fs::create_dir_all(&u);
        d = d.set_directory(u);
    }
    d.save_file().map(|p| if p.extension().is_none() { p.with_extension(EXT) } else { p })
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn open_dialog() -> Option<PathBuf> {
    None
}
#[cfg(not(any(target_os = "macos", windows)))]
pub fn save_dialog(_default_name: &str) -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_is_all_defaults_roundtrips_and_excludes_performance_switches() {
        let p = Preset::init();
        for id in EXCLUDED {
            assert!(!p.params.contains_key(id), "{id} must not be stored");
        }
        assert_eq!(p.params["lo_drive"], 0.0, "init drive is 0");
        assert_eq!(p.params["mid_thresh"], 0.0, "init threshold is 0");
        assert_eq!(p.params.len(), TABLE.len() - EXCLUDED.len());
        let q = Preset::from_json(&p.to_json()).unwrap();
        assert_eq!(q.params, p.params);
        assert!(Preset::from_json("{\"format\":\"satcomp-preset\"}").is_none(), "a SatComp preset is not a SatXComp preset");
    }

    #[test]
    fn curves_roundtrip_and_old_presets_load_without_them() {
        let mut p = Preset::init();
        assert!(!p.to_json().contains("curves"), "not written when unused");
        p.curves = vec![vec![(-1.0, -1.0), (0.0, 0.3), (1.0, 1.0)], vec![], vec![(-1.0, 0.0), (1.0, 0.5)]];
        let q = Preset::from_json(&p.to_json()).unwrap();
        assert_eq!(q.curves, p.curves);
        assert_eq!(q.curve(0).len(), 3);
        assert!(q.curve(1).is_empty() && q.curve(7).is_empty());
        let old = r#"{"format":"satxcomp-preset","version":1,"name":"x","params":{}}"#;
        assert!(Preset::from_json(old).unwrap().curves.is_empty());
    }

    #[test]
    fn factory_starts_with_init() {
        assert_eq!(factory()[0].name, "INIT");
    }
}
