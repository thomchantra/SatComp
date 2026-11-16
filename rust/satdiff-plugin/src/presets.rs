//! Presets: JSON files of parameter values keyed by parameter id.
//! Factory bank = every `presets/*.json` embedded at build time (+ built-in INIT).
//! User presets = `*.satdiff` files; the dropdown lists the user folder, "Load preset..." opens any file.

use crate::params::TABLE;
use include_dir::{include_dir, Dir};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

static FACTORY_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/presets");

pub const EXT: &str = "satdiff";
/// Never stored in a preset: Bypass is a performance switch.
pub const EXCLUDED: [&str; 1] = ["bypass"];

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Preset {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub params: BTreeMap<String, f32>,
    /// Custom waveshaper curve points (absent in older presets = linear). Backward compatible: version stays 1.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub curve: Vec<(f32, f32)>,
    /// Curve handles / LINEAR | CURVE of the custom curve (`Curve::ext`; absent = plain).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub curve_ext: Vec<f32>,
    /// Same for scene B.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub curve_ext_b: Vec<f32>,
    /// Scene B's custom curve (absent = linear).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub curve_b: Vec<(f32, f32)>,
    /// ASYM (0..100 %) per waveshaper (absent / empty = all 100 %). Backward compatible: version stays 1.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub asym: Vec<f32>,
    /// Scene B's ASYM per waveshaper (absent / empty = all 100 %).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub asym_b: Vec<f32>,
}

impl Preset {
    pub fn new(name: &str, params: BTreeMap<String, f32>) -> Self {
        Self { format: "satdiff-preset".into(), version: 1, name: name.into(), params, curve: Vec::new(), curve_ext: Vec::new(), curve_ext_b: Vec::new(), curve_b: Vec::new(), asym: Vec::new(), asym_b: Vec::new() }
    }

    /// All defaults.
    pub fn init() -> Self {
        let params = TABLE.iter().filter(|e| !EXCLUDED.contains(&e.1)).map(|e| (e.1.to_string(), e.4)).collect();
        Self::new("INIT", params)
    }

    pub fn from_json(s: &str) -> Option<Self> {
        let p: Preset = serde_json::from_str(s).ok()?;
        (p.format == "satdiff-preset").then_some(p)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
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

/// ~/Library/Application Support/SatDiff/Presets (macOS), %APPDATA%\SatDiff\Presets (Windows), ~/.config/SatDiff/Presets
pub fn user_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"));
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(not(any(target_os = "macos", windows)))]
    let base = std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"));
    base.map(|b| b.join("SatDiff").join("Presets"))
}

/// (display name, path) of the user presets, sorted case-insensitively.
pub fn scan_user() -> Vec<(String, PathBuf)> {
    let mut v = Vec::new();
    // tests are hermetic: the machine's own user presets must not change what the editor lists (the UI golden steps through them)
    if cfg!(test) {
        return v;
    }
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
    let mut d = rfd::FileDialog::new().add_filter("SatDiff preset", &[EXT, "json"]).set_title("Load preset");
    if let Some(u) = user_dir().filter(|u| u.exists()) {
        d = d.set_directory(u);
    }
    d.pick_file()
}

#[cfg(any(target_os = "macos", windows))]
pub fn save_dialog(default_name: &str) -> Option<PathBuf> {
    let mut d = rfd::FileDialog::new()
        .add_filter("SatDiff preset", &[EXT])
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
    fn init_roundtrips_and_excludes_the_performance_switch() {
        let p = Preset::init();
        assert!(!p.params.contains_key("bypass"));
        assert_eq!(p.params["drive"], 0.0);
        assert_eq!(p.params["lim_on"], 1.0);
        assert_eq!(p.params["gain_match"], 0.0);
        let q = Preset::from_json(&p.to_json()).unwrap();
        assert_eq!(q.params, p.params);
        assert!(Preset::from_json("{\"format\":\"nope\"}").is_none());
        assert!(Preset::from_json("{\"format\":\"satcomp-preset\",\"version\":1,\"name\":\"x\",\"params\":{}}").is_none());
    }

    #[test]
    fn asym_and_curve_roundtrip_and_old_presets_load_without_them() {
        let mut p = Preset::init();
        assert!(!p.to_json().contains("asym") && !p.to_json().contains("curve"));
        p.asym = vec![100.0, 35.0];
        p.curve = vec![(-1.0, -1.0), (0.0, 0.3), (1.0, 1.0)];
        let q = Preset::from_json(&p.to_json()).unwrap();
        assert_eq!((q.asym.clone(), q.curve.clone()), (p.asym, p.curve));
        let old = r#"{"format":"satdiff-preset","version":1,"name":"x","params":{}}"#;
        let o = Preset::from_json(old).unwrap();
        assert!(o.asym.is_empty() && o.curve.is_empty());
    }

    #[test]
    fn every_factory_value_is_in_range() {
        for p in &factory() {
            for (k, v) in &p.params {
                let e = TABLE.iter().find(|e| e.1 == k).unwrap();
                assert!(*v >= e.2 - 1e-3 && *v <= e.3 + 1e-3, "{}: {k} = {v} outside {}..{}", p.name, e.2, e.3);
            }
        }
    }

    #[test]
    fn the_factory_bank_has_the_owners_eight_and_every_value_is_in_range() {
        let f = factory();
        for want in ["Amen Hell", "Bump Me Up", "Burnt Popcorn", "Shrooms", "Soft2Hard", "Tube2Brick", "You Rang", "Zap"] {
            assert!(f.iter().any(|p| p.name == want), "missing factory preset {want}");
        }
        for p in &f {
            for (k, v) in &p.params {
                let e = TABLE.iter().find(|e| e.1 == k).unwrap();
                assert!(*v >= e.2 - 1e-3 && *v <= e.3 + 1e-3, "{}: {k} = {v} outside {}..{}", p.name, e.2, e.3);
            }
        }
        // the drawn curves and their handles ride along
        let n = |name: &str| f.iter().find(|p| p.name == name).unwrap();
        assert_eq!(n("Zap").curve.len(), 4);
        assert_eq!(n("Burnt Popcorn").curve.len(), 6);
        assert_eq!(n("Bump Me Up").curve_b.len(), 14);
        assert_eq!(n("Shrooms").curve_ext_b.len(), n("Shrooms").curve_b.len());
        assert_eq!(n("Shrooms").asym_b.len(), 15);
    }

    #[test]
    fn factory_starts_with_init_and_every_preset_uses_known_ids() {
        let f = factory();
        assert_eq!(f[0].name, "INIT");
        for p in &f {
            for k in p.params.keys() {
                assert!(TABLE.iter().any(|e| e.1 == k), "{}: unknown id {k}", p.name);
            }
        }
    }
}
