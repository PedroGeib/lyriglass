//! Commands invoked from the UI through `ui/api.js`.

use crate::player::Core;
use crate::shell::{self, Shell};
use crate::store::Config;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use qrcode::render::svg;
use qrcode::{EcLevel, QrCode};
use serde_json::{json, Value};
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;

type CoreState<'a> = State<'a, Arc<Core>>;

#[tauri::command]
pub fn config_get(core: CoreState) -> Config {
    core.config()
}

#[tauri::command]
pub fn config_set(app: AppHandle, patch: Value) -> Config {
    shell::set_config(&app, patch)
}

#[tauri::command]
pub fn state_get(core: CoreState, shell: State<Shell>) -> Value {
    json!({
        "player": core.player(),
        "lyrics": core.lyrics_snapshot(),
        "clickThrough": shell.click_through.load(Relaxed),
        "demo": false,
    })
}

#[tauri::command]
pub async fn player_command(core: CoreState<'_>, name: String, arg: Option<f64>) -> Result<Value, String> {
    let core = core.inner().clone();
    Ok(core.command(&name, arg).await)
}

#[tauri::command]
pub fn overlay_compact(app: AppHandle, shell: State<Shell>, value: bool) {
    if shell.compact.swap(value, Relaxed) != value {
        shell::resize_overlay(&app);
    }
}

/// Width the lyrics column needs for the current song (horizontal layout).
#[tauri::command]
pub fn overlay_lyrics_width(app: AppHandle, shell: State<Shell>, value: f64) {
    if !value.is_finite() {
        return;
    }
    let changed = shell.lyrics_width.lock().unwrap().replace(value) != Some(value);
    if changed {
        shell::resize_overlay(&app);
    }
}

#[tauri::command]
pub fn overlay_hide(app: AppHandle) {
    shell::hide_overlay(&app, true);
}

#[tauri::command]
pub fn overlay_menu(app: AppHandle) {
    shell::popup_menu(&app);
}

#[tauri::command]
pub fn overlay_preset(app: AppHandle, preset: String) {
    shell::move_to_preset(&app, &preset);
}

// Window creation must happen off the main thread on Windows, hence async.
#[tauri::command]
pub async fn settings_open(app: AppHandle, tab: Option<String>) -> Result<(), String> {
    shell::open_settings(&app, tab);
    Ok(())
}

#[tauri::command]
pub fn auth_get(core: CoreState) -> Value {
    shell::auth_info(&core)
}

#[tauri::command]
pub async fn auth_login(app: AppHandle, core: CoreState<'_>) -> Result<Value, String> {
    let core = core.inner().clone();
    let opener = app.clone();
    let result = core
        .spotify
        .login(move |url| {
            let _ = opener.opener().open_url(url, None::<&str>);
        })
        .await;
    let _ = app.emit("auth", shell::auth_info(&core));
    Ok(match result {
        Ok(profile) => {
            core.poke(0);
            json!({ "ok": true, "profile": profile })
        }
        Err(error) => json!({ "ok": false, "error": error }),
    })
}

#[tauri::command]
pub fn auth_cancel(core: CoreState) {
    core.spotify.cancel_login();
}

#[tauri::command]
pub fn auth_logout(app: AppHandle, core: CoreState) -> Value {
    core.spotify.logout();
    core.poke(0);
    let info = shell::auth_info(&core);
    let _ = app.emit("auth", &info);
    info
}

#[tauri::command]
pub fn gemini_has(core: CoreState) -> bool {
    core.secrets.lock().unwrap().get_str("geminiApiKey").is_some()
}

#[tauri::command]
pub fn gemini_set(core: CoreState, key: String) -> bool {
    let key = key.trim().to_string();
    core.secrets.lock().unwrap().set("geminiApiKey", Some(Value::String(key)));
    core.retranslate();
    core.secrets.lock().unwrap().get_str("geminiApiKey").is_some()
}

#[tauri::command]
pub async fn lyrics_clear_cache(core: CoreState<'_>) -> Result<usize, String> {
    let core = core.inner().clone();
    let count = core.lyrics.clear_cache();
    if core.player().track.is_some() {
        core.command("resync", None).await;
    }
    Ok(count)
}

#[tauri::command]
pub fn jam_qr(text: String) -> Result<String, String> {
    let text: String = text.chars().take(1000).collect();
    let code = QrCode::with_error_correction_level(text.as_bytes(), EcLevel::M).map_err(|e| e.to_string())?;
    let image = code
        .render::<svg::Color>()
        .min_dimensions(360, 360)
        .quiet_zone(false)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build();
    Ok(format!("data:image/svg+xml;base64,{}", STANDARD.encode(image)))
}

#[tauri::command]
pub fn clipboard_write(app: AppHandle, text: String) -> Result<(), String> {
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn external_open(app: AppHandle, url: String) {
    if url.starts_with("https://") || url.starts_with("spotify:") {
        let _ = app.opener().open_url(url, None::<&str>);
    }
}

#[tauri::command]
pub fn app_info(app: AppHandle, core: CoreState, shell: State<Shell>) -> Value {
    json!({
        "version": app.package_info().version.to_string(),
        "hotkeys": *shell.hotkey_status.lock().unwrap(),
        "demo": false,
        "dataPath": core.dir.to_string_lossy(),
    })
}

#[tauri::command]
pub fn app_open_data(app: AppHandle, core: CoreState) {
    let _ = app.opener().open_path(core.dir.to_string_lossy().to_string(), None::<&str>);
}
