# Lyriglass

A lightweight glass overlay for **Spotify** on Windows, with **synced lyrics**, a karaoke effect, translation and playback controls. It sits on top of your other windows without getting in the way.

[![Latest release](https://img.shields.io/github/v/release/PedroGeib/lyriglass)](https://github.com/PedroGeib/lyriglass/releases/latest)
![Platform](https://img.shields.io/badge/platform-Windows-0078d4)
![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%202-24c8db)
[![MIT License](https://img.shields.io/badge/license-MIT-green)](LICENSE)

## Download

Download **`Lyriglass_<version>_x64-setup.exe`** from the [Releases page](https://github.com/PedroGeib/lyriglass/releases/latest) and run it.

The installer is not code-signed, so Windows SmartScreen may warn you the first time. Click **More info → Run anyway**.

## Features

- **Synced lyrics** from [LRCLIB](https://lrclib.net), with a karaoke effect, fine-tuning of the timing and an "Up next" line.
- **Lyrics translation** with Google Translate (free) or Gemini (with your own API key), shown below each line.
- **Offline cache**: songs you've already played open instantly, even without internet.
- **Three layouts**: horizontal, vertical and mini.
- **Controls on the overlay**: play/pause, previous/next, shuffle, repeat, like, volume by scrolling over the cover, and click a line to jump to it.
- **Customizable look**: opacity, size, accent color from the album cover, collapse when there are no lyrics, and hide when nothing is playing.
- **Jam mode**: shows a QR code so friends can join your Jam or open the current song.
- **Global hotkeys** that work anywhere in Windows.
- Tray icon, launch with Windows, and the position is remembered between sessions.

## Getting started: connect Spotify

Lyriglass uses the official Spotify Web API, which requires each person to create a free app in Spotify's developer dashboard and use its **Client ID**. It takes about 2 minutes:

1. Open the **[Spotify Developer Dashboard](https://developer.spotify.com/dashboard)** and log in with your Spotify account. The first time, accept the developer terms.
2. Click **Create app**.
3. In **App name** and **App description**, enter anything (for example, `Lyriglass`).
4. Under **Redirect URIs**, add exactly:
   ```text
   http://127.0.0.1:8888/callback
   ```
5. Under **Which API/SDKs are you planning to use?**, check **Web API**, accept the terms and click **Save**.
6. On the new app's page, copy the **Client ID**.
7. In Lyriglass, open **Settings → Account**, paste the Client ID and click **Connect with Spotify**.

The same guide is available inside the app, in the **Account** tab.

- New apps start in **development mode**: only accounts added under **User Management** in the app's dashboard can log in. Your own account, as the app owner, already has access.
- **Controlling playback** (play/pause, skip, volume) requires **Spotify Premium**. This is a Spotify API requirement. Seeing the current song and the lyrics works with any account.
- Login uses **PKCE**: no Client Secret is needed or stored.

## Hotkeys

| Hotkey | Action |
| --- | --- |
| `Ctrl+Alt+H` | Show / hide the overlay |
| `Ctrl+Alt+S` | Turn click-through on / off |
| `Ctrl+Alt+P` | Play / pause |
| `Ctrl+Alt+L` | Switch layout |
| `Ctrl+Alt+T` | Show / hide translation |
| `Ctrl+Alt+]` | Show lyrics earlier (+250 ms) |
| `Ctrl+Alt+[` | Show lyrics later (−250 ms) |

On the overlay:

| Gesture | Action |
| --- | --- |
| Scroll over the cover | Volume |
| Click a line | Jump to that part of the song |
| Scroll the lyrics | Browse freely for 4 s |
| Right-click | Quick menu |
| Drag the top handle | Move the overlay |

## Privacy

Everything stays on your computer, in `%APPDATA%\com.pedrogeib.lyriglass`:

- **Settings**: `config.json`
- **Spotify tokens and Gemini key**: `secrets.dat`, encrypted with Windows data protection (DPAPI) so only your Windows user can read it
- **Lyrics cache**: `lyrics-cache\`

The app only talks to Spotify, LRCLIB and, if translation is turned on, Google Translate or Gemini.

## Building from source

Requirements:

- Windows 10 or 11
- [Node.js](https://nodejs.org/) 18 or newer
- [Rust](https://www.rust-lang.org/tools/install)
- The [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for Windows (Microsoft C++ Build Tools and WebView2)

```bash
git clone https://github.com/PedroGeib/lyriglass.git
```

```bash
cd lyriglass
```

```bash
npm install
```

Run in development mode:

```bash
npm run dev
```

Build the executable and installer:

```bash
npm run build
```

The output goes to `src-tauri\target\release\` (the `.exe`) and `src-tauri\target\release\bundle\nsis\` (the installer).

## Project structure

```text
ui/                  Interface (plain HTML, CSS and JavaScript, no bundler)
  api.js             Bridge to the Tauri backend
  overlay/           Overlay window
  settings/          Settings window
src-tauri/           Rust backend (Tauri 2)
  src/main.rs        Startup and command registration
  src/shell.rs       Windows, tray, global hotkeys and events
  src/commands.rs    Commands exposed to the interface
  src/spotify.rs     PKCE login and Spotify Web API client
  src/player.rs      Playback state and lyrics sync
  src/lyrics.rs      LRCLIB lookup, cache and translation
  src/store.rs       Settings and encrypted secrets
assets/              Source icon
```

## License

[MIT](LICENSE)

## Disclaimer

Lyriglass is an independent project and is not affiliated with or endorsed by Spotify. Spotify is a trademark of Spotify AB. Lyrics come from LRCLIB and belong to their respective owners.
