#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod lyrics;
mod player;
mod shell;
mod spotify;
mod store;

use tauri_plugin_global_shortcut::ShortcutState;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| shell::show_overlay(app)))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        shell::on_hotkey(app, shortcut);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .on_menu_event(|app, event| shell::on_menu(app, event.id().as_ref()))
        .setup(|app| {
            shell::setup(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::config_get,
            commands::config_set,
            commands::state_get,
            commands::player_command,
            commands::overlay_compact,
            commands::overlay_hide,
            commands::overlay_menu,
            commands::overlay_preset,
            commands::settings_open,
            commands::auth_get,
            commands::auth_login,
            commands::auth_cancel,
            commands::auth_logout,
            commands::gemini_has,
            commands::gemini_set,
            commands::lyrics_clear_cache,
            commands::jam_qr,
            commands::clipboard_write,
            commands::external_open,
            commands::app_info,
            commands::app_open_data,
        ])
        .build(tauri::generate_context!())
        .expect("failed to start Lyriglass")
        .run(|_app, event| {
            // The app lives in the tray: closing windows must not quit it.
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
