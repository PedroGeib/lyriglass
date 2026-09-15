//! Settings (plain JSON) and secrets (encrypted with Windows DPAPI).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Writes to a temp file and renames it, so a crash never leaves a half-written file.
pub fn write_atomic(path: &Path, bytes: &[u8]) {
    let tmp = path.with_extension("tmp");
    if fs::write(&tmp, bytes).is_ok() {
        let _ = fs::rename(&tmp, path);
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub spotify_client_id: String,

    pub layout: String,
    pub opacity: f64,
    pub scale: f64,
    pub always_on_top: bool,
    pub lock_position: bool,
    pub launch_at_login: bool,
    pub accent_from_cover: bool,
    pub collapse_when_no_lyrics: bool,
    pub auto_hide_when_idle: bool,
    pub idle_minutes: f64,
    pub position: Option<Position>,

    pub show_lyrics: bool,
    pub karaoke: bool,
    pub lyrics_font_size: f64,
    pub lyrics_offset_ms: f64,
    pub show_translation: bool,
    pub translation_mode: String,
    pub translation_target: String,
    pub gemini_model: String,
    pub show_next_up: bool,

    pub jam_enabled: bool,
    pub jam_link: String,
    pub jam_open_on_start: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            spotify_client_id: String::new(),
            layout: "horizontal".into(),
            opacity: 0.88,
            scale: 1.0,
            always_on_top: true,
            lock_position: false,
            launch_at_login: false,
            accent_from_cover: true,
            collapse_when_no_lyrics: true,
            auto_hide_when_idle: false,
            idle_minutes: 5.0,
            position: None,
            show_lyrics: true,
            karaoke: true,
            lyrics_font_size: 14.0,
            lyrics_offset_ms: 0.0,
            show_translation: true,
            translation_mode: "off".into(),
            translation_target: "pt".into(),
            gemini_model: "gemini-2.5-flash".into(),
            show_next_up: true,
            jam_enabled: false,
            jam_link: String::new(),
            jam_open_on_start: false,
        }
    }
}

impl Config {
    /// Loads the config, importing settings from the previous Electron build on first run.
    pub fn load_or_migrate(dir: &Path) -> Self {
        let path = dir.join("config.json");
        if path.exists() {
            return fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        }
        let legacy = dir.parent().map(|p| p.join("SpotifyOverlay").join("config.json"));
        let mut cfg: Config = legacy
            .and_then(|p| fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        cfg.position = None; // Electron used logical pixels; ours are physical.
        cfg
    }

    pub fn save(&self, dir: &Path) {
        if let Ok(bytes) = serde_json::to_vec_pretty(self) {
            write_atomic(&dir.join("config.json"), &bytes);
        }
    }

    /// Merges known keys from `patch`; returns the keys that actually changed.
    pub fn apply_patch(&mut self, patch: &Value) -> Vec<String> {
        let Some(patch) = patch.as_object() else { return vec![] };
        let Ok(mut current) = serde_json::to_value(&*self) else { return vec![] };
        let Some(obj) = current.as_object_mut() else { return vec![] };

        let mut changed = vec![];
        for (key, value) in patch {
            if let Some(old) = obj.get(key) {
                if !same_value(old, value) {
                    obj.insert(key.clone(), value.clone());
                    changed.push(key.clone());
                }
            }
        }
        if changed.is_empty() {
            return changed;
        }
        match serde_json::from_value::<Config>(current) {
            Ok(next) => {
                *self = next;
                changed
            }
            Err(_) => vec![],
        }
    }
}

fn same_value(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => (x - y).abs() < 1e-9,
        _ => a == b,
    }
}

pub struct Secrets {
    file: PathBuf,
    data: Map<String, Value>,
}

impl Secrets {
    pub fn load(dir: &Path) -> Self {
        let file = dir.join("secrets.dat");
        let data = fs::read(&file)
            .ok()
            .and_then(|bytes| dpapi::unprotect(&bytes))
            .and_then(|plain| serde_json::from_slice(&plain).ok())
            .unwrap_or_default();
        Self { file, data }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.data.get(key)
    }

    pub fn get_str(&self, key: &str) -> Option<String> {
        self.data.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(String::from)
    }

    pub fn set(&mut self, key: &str, value: Option<Value>) {
        match value {
            Some(v) if !v.is_null() && v.as_str() != Some("") => {
                self.data.insert(key.to_string(), v);
            }
            _ => {
                self.data.remove(key);
            }
        }
        if let Ok(plain) = serde_json::to_vec(&self.data) {
            if let Some(encrypted) = dpapi::protect(&plain) {
                write_atomic(&self.file, &encrypted);
            }
        }
    }
}

#[cfg(windows)]
mod dpapi {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB};

    fn run(input: &[u8], encrypt: bool) -> Option<Vec<u8>> {
        unsafe {
            let data_in = CRYPT_INTEGER_BLOB { cbData: input.len() as u32, pbData: input.as_ptr() as *mut u8 };
            let mut data_out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: null_mut() };
            let ok = if encrypt {
                CryptProtectData(&data_in, null(), null(), null(), null(), 0, &mut data_out)
            } else {
                CryptUnprotectData(&data_in, null_mut(), null(), null(), null(), 0, &mut data_out)
            };
            if ok == 0 || data_out.pbData.is_null() {
                return None;
            }
            let out = std::slice::from_raw_parts(data_out.pbData, data_out.cbData as usize).to_vec();
            LocalFree(data_out.pbData as _);
            Some(out)
        }
    }

    pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
        run(data, true)
    }

    pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
        run(data, false)
    }
}

#[cfg(not(windows))]
mod dpapi {
    pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
        Some(data.to_vec())
    }

    pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
        Some(data.to_vec())
    }
}
