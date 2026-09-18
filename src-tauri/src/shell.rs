//! Windows, tray, menus, global shortcuts and reactions to settings changes.

use crate::player::{Core, PlayerState};
use crate::spotify::REDIRECT_URI;
use crate::store::{Config, Secrets};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent, Wry};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
use tauri_plugin_opener::OpenerExt;

const LAYOUTS: [(&str, &str); 3] = [("horizontal", "Horizontal"), ("vertical", "Vertical"), ("mini", "Mini (bar)")];
const PRESETS: [(&str, &str); 9] = [
    ("top-left", "Top left"),
    ("top-center", "Top center"),
    ("top-right", "Top right"),
    ("middle-left", "Middle left"),
    ("middle-center", "Center"),
    ("middle-right", "Middle right"),
    ("bottom-left", "Bottom left"),
    ("bottom-center", "Bottom center"),
    ("bottom-right", "Bottom right"),
];

struct Hotkey {
    accel: &'static str,
    display: &'static str,
    label: &'static str,
    action: &'static str,
}

const HOTKEYS: [Hotkey; 7] = [
    Hotkey { accel: "Ctrl+Alt+KeyH", display: "Ctrl+Alt+H", label: "Show / hide the overlay", action: "toggle_overlay" },
    Hotkey { accel: "Ctrl+Alt+KeyS", display: "Ctrl+Alt+S", label: "Turn click-through on / off", action: "click_through" },
    Hotkey { accel: "Ctrl+Alt+KeyP", display: "Ctrl+Alt+P", label: "Play / pause", action: "play_pause" },
    Hotkey { accel: "Ctrl+Alt+KeyL", display: "Ctrl+Alt+L", label: "Switch layout", action: "cycle_layout" },
    Hotkey { accel: "Ctrl+Alt+KeyT", display: "Ctrl+Alt+T", label: "Show / hide translation", action: "toggle_translation" },
    Hotkey { accel: "Ctrl+Alt+BracketRight", display: "Ctrl+Alt+]", label: "Show lyrics earlier (+250 ms)", action: "offset_plus" },
    Hotkey { accel: "Ctrl+Alt+BracketLeft", display: "Ctrl+Alt+[", label: "Show lyrics later (−250 ms)", action: "offset_minus" },
];

/// WebView2 flags for a small footprint: software rendering keeps the GPU process
/// inside the browser process, and background services an overlay never uses are off.
/// Every webview must share the same flags, so both windows use this constant.
const BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,Translate,AutofillServerCommunication,MediaRouter \
     --disable-gpu --in-process-gpu --disable-background-networking --disable-component-update --disable-extensions --no-first-run";

#[derive(Default)]
pub struct Shell {
    pub compact: AtomicBool,
    /// Width the lyrics column needs for the current song, reported by the overlay.
    pub lyrics_width: Mutex<Option<f64>>,
    pub click_through: AtomicBool,
    user_hidden: AtomicBool,
    auto_hidden: AtomicBool,
    save_generation: AtomicU64,
    hotkeys: Mutex<Vec<(Shortcut, &'static str)>>,
    pub hotkey_status: Mutex<Vec<Value>>,
}

fn core(app: &AppHandle) -> Arc<Core> {
    app.state::<Arc<Core>>().inner().clone()
}

fn shell(app: &AppHandle) -> State<'_, Shell> {
    app.state::<Shell>()
}

fn overlay(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("overlay")
}

// -------------------------------------------------------------------- setup
pub fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&dir)?;
    let config = Config::load_or_migrate(&dir);
    config.save(&dir);
    let secrets = Secrets::load(&dir);
    let http = reqwest::Client::builder().build()?;

    let core = Arc::new(Core::new(app.clone(), dir, Arc::new(Mutex::new(config)), Arc::new(Mutex::new(secrets)), http));
    app.manage(core.clone());
    app.manage(Shell::default());

    create_overlay(app)?;
    create_tray(app)?;
    register_hotkeys(app);
    tauri::async_runtime::spawn(core.clone().run());

    if core.spotify.is_logged_in() {
        tauri::async_runtime::spawn(async move {
            if core.spotify.refresh_profile().await.is_ok() {
                let _ = core.app.emit("auth", auth_info(&core));
            }
        });
    } else {
        open_settings(app, Some("account".into()));
    }
    Ok(())
}

pub fn auth_info(core: &Core) -> Value {
    let auth = core.spotify.auth();
    json!({
        "loggedIn": core.spotify.is_logged_in(),
        "profile": auth.and_then(|a| a.profile),
        "redirectUri": REDIRECT_URI,
    })
}

// ------------------------------------------------------------------ overlay
/// Horizontal layout: bounds of the lyrics column, which fits the widest line of
/// the current song (reported by the overlay). The rest of the window is the
/// 200px cover plus the 8px margins. The mini layout has a fixed width and wraps
/// long lines instead.
const LYRICS_COLUMN: (f64, f64) = (200.0, 344.0);

fn overlay_size(cfg: &Config, compact: bool, lyrics_width: Option<f64>) -> (f64, f64) {
    let lyrics = lyrics_width.unwrap_or(LYRICS_COLUMN.1).clamp(LYRICS_COLUMN.0, LYRICS_COLUMN.1);
    let (full, small) = match cfg.layout.as_str() {
        "vertical" => ((296.0, 548.0), (296.0, 296.0)),
        "mini" => ((440.0, 104.0), (440.0, 104.0)),
        _ => ((216.0 + lyrics, 216.0), (216.0, 216.0)),
    };
    let (w, h) = if compact { small } else { full };
    (w * cfg.scale, h * cfg.scale)
}

fn create_overlay(app: &AppHandle) -> tauri::Result<()> {
    let cfg = core(app).config();
    let (w, h) = overlay_size(&cfg, false, None);
    let win = WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("overlay/index.html".into()))
        .title("Lyriglass")
        .inner_size(w, h)
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .skip_taskbar(true)
        .always_on_top(cfg.always_on_top)
        .focused(false)
        .visible(false)
        .additional_browser_args(BROWSER_ARGS)
        .build()?;

    place_initially(&win, &cfg, w, h);
    win.show()?;
    #[cfg(windows)]
    if let Ok(hwnd) = win.hwnd() {
        topmost::install(hwnd.0 as isize, cfg.always_on_top);
    }

    let handle = app.clone();
    win.on_window_event(move |event| match event {
        WindowEvent::Moved(_) => schedule_save_position(&handle),
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            hide_overlay(&handle, true);
        }
        _ => {}
    });
    Ok(())
}

/// Work area (x, y, width, height) and scale factor of the monitor showing `win`.
fn work_area(win: &WebviewWindow) -> Option<(i32, i32, i32, i32, f64)> {
    let monitor = win.current_monitor().ok().flatten().or_else(|| win.primary_monitor().ok().flatten())?;
    let area = monitor.work_area();
    Some((area.position.x, area.position.y, area.size.width as i32, area.size.height as i32, monitor.scale_factor()))
}

fn place_initially(win: &WebviewWindow, cfg: &Config, w: f64, h: f64) {
    if let Some(p) = cfg.position {
        let visible = win.available_monitors().unwrap_or_default().iter().any(|m| {
            let a = m.work_area();
            p.x >= a.position.x - 50
                && p.y >= a.position.y - 10
                && p.x < a.position.x + a.size.width as i32 - 50
                && p.y < a.position.y + a.size.height as i32 - 50
        });
        if visible {
            let _ = win.set_position(PhysicalPosition::new(p.x, p.y));
            clamp_overlay(win);
            return;
        }
    }
    if let Ok(Some(monitor)) = win.primary_monitor() {
        let a = monitor.work_area();
        let sf = monitor.scale_factor();
        let x = a.position.x + a.size.width as i32 - ((w + 16.0) * sf) as i32;
        let y = a.position.y + a.size.height as i32 - ((h + 16.0) * sf) as i32;
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }
}

fn clamp_overlay(win: &WebviewWindow) {
    let (Ok(pos), Ok(size)) = (win.outer_position(), win.outer_size()) else { return };
    let Some((ax, ay, aw, ah, _)) = work_area(win) else { return };
    let x = pos.x.clamp(ax, (ax + aw - size.width as i32).max(ax));
    let y = pos.y.clamp(ay, (ay + ah - size.height as i32).max(ay));
    if x != pos.x || y != pos.y {
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }
}

/// Resizes the overlay keeping the edge closest to the screen corner in place.
pub fn resize_overlay(app: &AppHandle) {
    let Some(win) = overlay(app) else { return };
    let cfg = core(app).config();
    let sh = shell(app);
    let lyrics_width = *sh.lyrics_width.lock().unwrap();
    let (w, h) = overlay_size(&cfg, sh.compact.load(Relaxed), lyrics_width);
    let (Ok(pos), Ok(size)) = (win.outer_position(), win.outer_size()) else { return };
    let Some((ax, ay, aw, ah, sf)) = work_area(&win) else { return };

    let (nw, nh) = ((w * sf).round() as i32, (h * sf).round() as i32);
    let (ow, oh) = (size.width as i32, size.height as i32);
    if nw == ow && nh == oh {
        return;
    }
    let x = if pos.x + ow / 2 > ax + aw / 2 { pos.x + ow - nw } else { pos.x };
    let y = if pos.y + oh / 2 > ay + ah / 2 { pos.y + oh - nh } else { pos.y };
    let _ = win.set_size(PhysicalSize::new(nw as u32, nh as u32));
    let _ = win.set_position(PhysicalPosition::new(x.clamp(ax, (ax + aw - nw).max(ax)), y.clamp(ay, (ay + ah - nh).max(ay))));
}

fn schedule_save_position(app: &AppHandle) {
    let generation = shell(app).save_generation.fetch_add(1, Relaxed) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        if shell(&app).save_generation.load(Relaxed) != generation {
            return;
        }
        if let Some(pos) = overlay(&app).and_then(|w| w.outer_position().ok()) {
            set_config(&app, json!({ "position": { "x": pos.x, "y": pos.y } }));
        }
    });
}

pub fn move_to_preset(app: &AppHandle, preset: &str) {
    let Some(win) = overlay(app) else { return };
    let Ok(size) = win.outer_size() else { return };
    let Some((ax, ay, aw, ah, sf)) = work_area(&win) else { return };
    let margin = (16.0 * sf) as i32;
    let (w, h) = (size.width as i32, size.height as i32);
    let (row, col) = preset.split_once('-').unwrap_or(("bottom", "right"));
    let x = match col {
        "left" => ax + margin,
        "right" => ax + aw - w - margin,
        _ => ax + (aw - w) / 2,
    };
    let y = match row {
        "top" => ay + margin,
        "bottom" => ay + ah - h - margin,
        _ => ay + (ah - h) / 2,
    };
    let _ = win.set_position(PhysicalPosition::new(x, y));
    show_overlay(app);
}

pub fn show_overlay(app: &AppHandle) {
    let Some(win) = overlay(app) else { return };
    let sh = shell(app);
    sh.user_hidden.store(false, Relaxed);
    sh.auto_hidden.store(false, Relaxed);
    let _ = win.show();
    let _ = win.set_always_on_top(core(app).config().always_on_top);
    topmost::raise();
    refresh_tray(app);
}

pub fn hide_overlay(app: &AppHandle, by_user: bool) {
    let Some(win) = overlay(app) else { return };
    let _ = win.hide();
    if by_user {
        shell(app).user_hidden.store(true, Relaxed);
    }
    refresh_tray(app);
}

fn toggle_overlay(app: &AppHandle) {
    match overlay(app).and_then(|w| w.is_visible().ok()) {
        Some(true) => hide_overlay(app, true),
        _ => show_overlay(app),
    }
}

fn set_click_through(app: &AppHandle, value: bool) {
    shell(app).click_through.store(value, Relaxed);
    if let Some(win) = overlay(app) {
        let _ = win.set_ignore_cursor_events(value);
    }
    let _ = app.emit("overlay:click-through", value);
    if value {
        show_overlay(app);
    }
    refresh_tray(app);
}

/// Called by the player after every state update.
pub fn player_changed(app: &AppHandle, previous_track: Option<String>, state: &PlayerState) {
    let track_id = state.track.as_ref().map(|t| t.id.clone());
    if track_id != previous_track {
        refresh_tray(app);
        if let Some(tray) = app.tray_by_id("tray") {
            let tip = state
                .track
                .as_ref()
                .map(|t| truncate(&format!("{} — {}", t.name, t.artists.join(", ")), 120))
                .unwrap_or_else(|| "Lyriglass".into());
            let _ = tray.set_tooltip(Some(tip));
        }
    }

    let core = core(app);
    let cfg = core.config();
    if !cfg.auto_hide_when_idle {
        return;
    }
    let sh = shell(app);
    if state.status == "playing" {
        if sh.auto_hidden.load(Relaxed) && !sh.user_hidden.load(Relaxed) {
            show_overlay(app);
        }
    } else if state.status != "auth"
        && !sh.auto_hidden.load(Relaxed)
        && !sh.user_hidden.load(Relaxed)
        && core.inactive_ms() > (cfg.idle_minutes * 60_000.0) as i64
    {
        if let Some(win) = overlay(app).filter(|w| w.is_visible().unwrap_or(false)) {
            sh.auto_hidden.store(true, Relaxed);
            let _ = win.hide();
            refresh_tray(app);
        }
    }
}

// ------------------------------------------------------------------- config
pub fn set_config(app: &AppHandle, patch: Value) -> Config {
    let core = core(app);
    let (cfg, changed) = {
        let mut current = core.config.lock().unwrap();
        let changed = current.apply_patch(&patch);
        if !changed.is_empty() {
            current.save(&core.dir);
        }
        (current.clone(), changed)
    };
    if changed.is_empty() {
        return cfg;
    }
    let has = |keys: &[&str]| changed.iter().any(|k| keys.contains(&k.as_str()));

    if has(&["layout", "scale"]) {
        if has(&["layout"]) {
            // The width reported for the old layout doesn't apply to the new one;
            // the overlay reports a fresh one right after switching.
            *shell(app).lyrics_width.lock().unwrap() = None;
        }
        resize_overlay(app);
    }
    if has(&["alwaysOnTop"]) {
        if let Some(win) = overlay(app) {
            let _ = win.set_always_on_top(cfg.always_on_top);
        }
        topmost::set_enabled(cfg.always_on_top);
    }
    if has(&["launchAtLogin"]) {
        use tauri_plugin_autostart::ManagerExt;
        let autostart = app.autolaunch();
        let _ = if cfg.launch_at_login { autostart.enable() } else { autostart.disable() };
    }
    if has(&["translationMode", "translationTarget", "geminiModel"]) {
        core.retranslate();
    }
    if has(&["showNextUp"]) && cfg.show_next_up {
        core.refresh_next_up();
    }
    if has(&["autoHideWhenIdle"]) && !cfg.auto_hide_when_idle && shell(app).auto_hidden.load(Relaxed) {
        show_overlay(app);
    }
    if !changed.iter().all(|k| k == "position") {
        refresh_tray(app);
    }
    let _ = app.emit("config", &cfg);
    cfg
}

// ----------------------------------------------------------------- settings
pub fn open_settings(app: &AppHandle, tab: Option<String>) {
    let tab = tab.filter(|t| t.chars().all(|c| c.is_ascii_lowercase()));
    if let Some(win) = app.get_webview_window("settings") {
        if let Some(tab) = tab {
            let _ = app.emit("settings:tab", tab);
        }
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        return;
    }
    let script = format!("window.__LYRIGLASS_TAB__ = {};", json!(tab.unwrap_or_else(|| "account".into())));
    let _ = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings/index.html".into()))
        .title("Settings — Lyriglass")
        .inner_size(880.0, 660.0)
        .min_inner_size(720.0, 520.0)
        .center()
        .initialization_script(&script)
        .additional_browser_args(BROWSER_ARGS)
        .build();
}

// --------------------------------------------------------------------- tray
fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;
    let mut builder = TrayIconBuilder::with_id("tray").tooltip("Lyriglass").menu(&menu).show_menu_on_left_click(false);
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                toggle_overlay(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn refresh_tray(app: &AppHandle) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id("tray"), build_menu(app)) {
        let _ = tray.set_menu(Some(menu));
    }
}

pub fn popup_menu(app: &AppHandle) {
    if let (Some(win), Ok(menu)) = (overlay(app), build_menu(app)) {
        let _ = win.popup_menu(&menu);
    }
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let core = core(app);
    let cfg = core.config();
    let state = core.player();
    let sh = shell(app);
    let none = None::<&str>;
    let menu = Menu::new(app)?;

    let now = state
        .track
        .as_ref()
        .map(|t| truncate(&format!("{} — {}", t.name, t.artists.join(", ")), 52))
        .unwrap_or_else(|| "Nothing playing".into());
    menu.append(&MenuItem::with_id(app, "now", now, false, none)?)?;
    if let Some(track) = &state.track {
        menu.append(&MenuItem::with_id(app, "open_spotify", "Open in Spotify", true, none)?)?;
        menu.append(&MenuItem::with_id(app, "copy_link", "Copy song link", track.url.is_some(), none)?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    let visible = overlay(app).and_then(|w| w.is_visible().ok()).unwrap_or(false);
    let toggle_label = if visible { "Hide overlay\tCtrl+Alt+H" } else { "Show overlay\tCtrl+Alt+H" };
    menu.append(&MenuItem::with_id(app, "toggle_overlay", toggle_label, true, none)?)?;
    menu.append(&CheckMenuItem::with_id(app, "click_through", "Click-through\tCtrl+Alt+S", true, sh.click_through.load(Relaxed), none)?)?;
    menu.append(&CheckMenuItem::with_id(app, "lock_position", "Lock position", true, cfg.lock_position, none)?)?;

    let layout = Submenu::with_id(app, "layout", "Layout", true)?;
    for (value, label) in LAYOUTS {
        layout.append(&CheckMenuItem::with_id(app, format!("layout:{value}"), label, true, cfg.layout == value, none)?)?;
    }
    menu.append(&layout)?;
    let position = Submenu::with_id(app, "position", "Position", true)?;
    for (value, label) in PRESETS {
        position.append(&MenuItem::with_id(app, format!("preset:{value}"), label, true, none)?)?;
    }
    menu.append(&position)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    menu.append(&CheckMenuItem::with_id(app, "show_lyrics", "Show lyrics", true, cfg.show_lyrics, none)?)?;
    menu.append(&CheckMenuItem::with_id(app, "show_translation", "Show translation", cfg.translation_mode != "off", cfg.show_translation, none)?)?;
    menu.append(&MenuItem::with_id(app, "resync", "Fetch lyrics again", state.track.is_some(), none)?)?;
    if cfg.jam_enabled {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        menu.append(&MenuItem::with_id(app, "jam", "Show Jam QR code", true, none)?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "settings", "Settings…", true, none)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Quit", true, none)?)?;
    Ok(menu)
}

pub fn on_menu(app: &AppHandle, id: &str) {
    let core = core(app);
    match id {
        "open_spotify" => {
            if let Some(track) = core.player().track {
                let url = if track.uri.starts_with("spotify:track:") { track.uri } else { track.url.unwrap_or_else(|| "https://open.spotify.com".into()) };
                let _ = app.opener().open_url(url, None::<&str>);
            }
        }
        "copy_link" => {
            if let Some(url) = core.player().track.and_then(|t| t.url) {
                let _ = app.clipboard().write_text(url);
            }
        }
        "toggle_overlay" => toggle_overlay(app),
        "click_through" => set_click_through(app, !shell(app).click_through.load(Relaxed)),
        "lock_position" => {
            set_config(app, json!({ "lockPosition": !core.config().lock_position }));
        }
        "show_lyrics" => {
            set_config(app, json!({ "showLyrics": !core.config().show_lyrics }));
        }
        "show_translation" => {
            set_config(app, json!({ "showTranslation": !core.config().show_translation }));
        }
        "resync" => {
            tauri::async_runtime::spawn(async move {
                core.command("resync", None).await;
            });
        }
        "jam" => {
            show_overlay(app);
            let _ = app.emit("jam:toggle", ());
        }
        "settings" => open_settings(app, None),
        "quit" => app.exit(0),
        other => {
            if let Some(layout) = other.strip_prefix("layout:") {
                set_config(app, json!({ "layout": layout }));
            } else if let Some(preset) = other.strip_prefix("preset:") {
                move_to_preset(app, preset);
            }
        }
    }
    // Check items flip themselves when clicked; rebuild so the tray matches the real state.
    refresh_tray(app);
}

// ------------------------------------------------------------------ hotkeys
fn register_hotkeys(app: &AppHandle) {
    let shortcuts = app.global_shortcut();
    let sh = shell(app);
    let mut registered = sh.hotkeys.lock().unwrap();
    let mut status = sh.hotkey_status.lock().unwrap();
    for hotkey in &HOTKEYS {
        let ok = match hotkey.accel.parse::<Shortcut>() {
            Ok(shortcut) => {
                let ok = shortcuts.register(shortcut).is_ok();
                if ok {
                    registered.push((shortcut, hotkey.action));
                }
                ok
            }
            Err(e) => {
                eprintln!("[hotkey] {}: {e}", hotkey.accel);
                false
            }
        };
        status.push(json!({ "accel": hotkey.display, "label": hotkey.label, "ok": ok }));
    }
}

pub fn on_hotkey(app: &AppHandle, shortcut: &Shortcut) {
    let action = shell(app).hotkeys.lock().unwrap().iter().find(|(s, _)| s == shortcut).map(|(_, a)| *a);
    let Some(action) = action else { return };
    let core = core(app);
    match action {
        "toggle_overlay" => toggle_overlay(app),
        "click_through" => set_click_through(app, !shell(app).click_through.load(Relaxed)),
        "play_pause" => {
            tauri::async_runtime::spawn(async move {
                core.command("toggle", None).await;
            });
        }
        "cycle_layout" => {
            let order: Vec<&str> = LAYOUTS.iter().map(|(v, _)| *v).collect();
            let current = order.iter().position(|v| *v == core.config().layout).unwrap_or(0);
            set_config(app, json!({ "layout": order[(current + 1) % order.len()] }));
        }
        "toggle_translation" => {
            set_config(app, json!({ "showTranslation": !core.config().show_translation }));
        }
        "offset_plus" | "offset_minus" => {
            let delta = if action == "offset_plus" { 250.0 } else { -250.0 };
            let value = (core.config().lyrics_offset_ms + delta).clamp(-5000.0, 5000.0);
            set_config(app, json!({ "lyricsOffsetMs": value }));
            core.toast(format!("Lyrics offset: {}{} ms", if value > 0.0 { "+" } else { "" }, value));
        }
        _ => {}
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() > max {
        format!("{}…", text.chars().take(max - 1).collect::<String>())
    } else {
        text.to_string()
    }
}

/// Keeps the overlay above other windows, including borderless-fullscreen games.
/// Setting "always on top" once is not enough on Windows: the flag can be lost and
/// the most recently raised window wins, so the overlay re-raises itself every time
/// another app takes the foreground (event-driven, no polling).
#[cfg(windows)]
mod topmost {
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering::Relaxed};
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, EVENT_SYSTEM_FOREGROUND, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, WINEVENT_OUTOFCONTEXT,
        WINEVENT_SKIPOWNPROCESS,
    };

    static OVERLAY: AtomicIsize = AtomicIsize::new(0);
    static ENABLED: AtomicBool = AtomicBool::new(false);

    /// Must be called on the main thread: the hook is delivered through its message loop.
    pub fn install(hwnd: isize, enabled: bool) {
        OVERLAY.store(hwnd, Relaxed);
        ENABLED.store(enabled, Relaxed);
        unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                std::ptr::null_mut(),
                Some(on_foreground),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            );
        }
        raise();
    }

    pub fn set_enabled(enabled: bool) {
        ENABLED.store(enabled, Relaxed);
        raise();
    }

    pub fn raise() {
        let hwnd = OVERLAY.load(Relaxed);
        if hwnd != 0 && ENABLED.load(Relaxed) {
            unsafe {
                SetWindowPos(hwnd as HWND, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER);
            }
        }
    }

    unsafe extern "system" fn on_foreground(_: HWINEVENTHOOK, _: u32, _: HWND, _: i32, _: i32, _: u32, _: u32) {
        raise();
    }
}

#[cfg(not(windows))]
mod topmost {
    pub fn install(_: isize, _: bool) {}
    pub fn set_enabled(_: bool) {}
    pub fn raise() {}
}
