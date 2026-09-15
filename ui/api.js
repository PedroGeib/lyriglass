// Bridge between the UI and the Rust backend. Exposes the same `api` object the
// pages already use, backed by Tauri commands and events.
(() => {
  const { invoke } = window.__TAURI__.core;
  const { listen } = window.__TAURI__.event;

  const on = (event) => (callback) => {
    let unlisten = null;
    listen(event, (e) => callback(e.payload)).then((fn) => { unlisten = fn; });
    return () => unlisten?.();
  };

  window.api = {
    getConfig: () => invoke('config_get'),
    setConfig: (patch) => invoke('config_set', { patch }),
    getState: () => invoke('state_get'),
    command: (name, arg) => invoke('player_command', { name, arg: arg ?? null }),

    setCompact: (value) => invoke('overlay_compact', { value }),
    hideOverlay: () => invoke('overlay_hide'),
    showMenu: () => invoke('overlay_menu'),
    movePreset: (preset) => invoke('overlay_preset', { preset }),
    openSettings: (tab) => invoke('settings_open', { tab: tab ?? null }),

    getAuth: () => invoke('auth_get'),
    login: () => invoke('auth_login'),
    cancelLogin: () => invoke('auth_cancel'),
    logout: () => invoke('auth_logout'),
    hasGeminiKey: () => invoke('gemini_has'),
    setGeminiKey: (key) => invoke('gemini_set', { key }),
    clearLyricsCache: () => invoke('lyrics_clear_cache'),

    jamQr: (text) => invoke('jam_qr', { text }),
    copy: (text) => invoke('clipboard_write', { text }),
    openExternal: (url) => invoke('external_open', { url }),
    appInfo: () => invoke('app_info'),
    openDataFolder: () => invoke('app_open_data'),

    onConfig: on('config'),
    onPlayer: on('player'),
    onLyrics: on('lyrics'),
    onAuth: on('auth'),
    onToast: on('toast'),
    onClickThrough: on('overlay:click-through'),
    onJamToggle: on('jam:toggle'),
    onSettingsTab: on('settings:tab'),
  };
})();
