//! Playback state, adaptive polling and playback commands.

use crate::lyrics::{Line, Lyrics, Translation};
use crate::spotify::{ApiError, Resp, Spotify};
use crate::store::{now_ms, Config, Secrets};
use reqwest::{Client, Method};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;

const SLOW_AFTER_MS: i64 = 5 * 60 * 1000;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    pub uri: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub artists: Vec<String>,
    pub album: String,
    pub cover: Option<String>,
    pub duration_ms: i64,
    pub is_local: bool,
    pub url: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct NextUp {
    pub name: String,
    pub artists: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Device {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerState {
    pub status: String,
    pub track: Option<Track>,
    pub progress_ms: i64,
    pub at: i64,
    pub volume: Option<i64>,
    pub shuffle: bool,
    pub repeat: String,
    pub device: Option<Device>,
    pub liked: Option<bool>,
    pub next_up: Option<NextUp>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsState {
    pub track_id: Option<String>,
    pub status: String,
    pub synced: bool,
    pub lines: Vec<Line>,
    pub translation: Option<Translation>,
    pub instrumental: bool,
    pub error: bool,
    pub episode: bool,
}

impl LyricsState {
    fn empty() -> Self {
        Self { track_id: None, status: "idle".into(), synced: false, lines: vec![], translation: None, instrumental: false, error: false, episode: false }
    }

    fn with_status(track_id: &str, status: &str) -> Self {
        Self { track_id: Some(track_id.into()), status: status.into(), ..Self::empty() }
    }
}

pub struct Core {
    pub app: AppHandle,
    pub dir: PathBuf,
    pub config: Arc<Mutex<Config>>,
    pub secrets: Arc<Mutex<Secrets>>,
    pub spotify: Spotify,
    pub lyrics: Lyrics,
    state: Mutex<PlayerState>,
    lyrics_state: Mutex<LyricsState>,
    wake: Notify,
    inactive_since: Mutex<Option<i64>>,
    lyrics_token: AtomicU64,
    last_rate_toast: AtomicI64,
}

impl Core {
    pub fn new(app: AppHandle, dir: PathBuf, config: Arc<Mutex<Config>>, secrets: Arc<Mutex<Secrets>>, http: Client) -> Self {
        let spotify = Spotify::new(config.clone(), secrets.clone(), http.clone());
        let status = if spotify.is_logged_in() { "loading" } else { "auth" };
        Self {
            lyrics: Lyrics::new(dir.clone(), http),
            spotify,
            state: Mutex::new(PlayerState {
                status: status.into(),
                track: None,
                progress_ms: 0,
                at: now_ms(),
                volume: None,
                shuffle: false,
                repeat: "off".into(),
                device: None,
                liked: None,
                next_up: None,
                message: None,
            }),
            lyrics_state: Mutex::new(LyricsState::empty()),
            wake: Notify::new(),
            inactive_since: Mutex::new(Some(now_ms())),
            lyrics_token: AtomicU64::new(0),
            last_rate_toast: AtomicI64::new(0),
            app,
            dir,
            config,
            secrets,
        }
    }

    pub fn config(&self) -> Config {
        self.config.lock().unwrap().clone()
    }

    pub fn player(&self) -> PlayerState {
        self.state.lock().unwrap().clone()
    }

    pub fn lyrics_snapshot(&self) -> LyricsState {
        self.lyrics_state.lock().unwrap().clone()
    }

    pub fn inactive_ms(&self) -> i64 {
        self.inactive_since.lock().unwrap().map_or(0, |since| now_ms() - since)
    }

    pub fn toast(&self, message: impl Into<String>) {
        let _ = self.app.emit("toast", message.into());
    }

    /// Asks the polling loop to run again after `ms`.
    pub fn poke(self: &Arc<Self>, ms: u64) {
        if ms == 0 {
            self.wake.notify_one();
            return;
        }
        let me = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(ms)).await;
            me.wake.notify_one();
        });
    }

    fn update(&self, change: impl FnOnce(&mut PlayerState)) {
        let (previous_track, snapshot) = {
            let mut state = self.state.lock().unwrap();
            let previous = state.track.as_ref().map(|t| t.id.clone());
            change(&mut state);
            let mut inactive = self.inactive_since.lock().unwrap();
            if state.status == "playing" {
                *inactive = None;
            } else if inactive.is_none() {
                *inactive = Some(now_ms());
            }
            (previous, state.clone())
        };
        let _ = self.app.emit("player", &snapshot);
        crate::shell::player_changed(&self.app, previous_track, &snapshot);
    }

    fn current_progress(&self) -> i64 {
        let s = self.state.lock().unwrap();
        let Some(track) = &s.track else { return 0 };
        let p = if s.status == "playing" { s.progress_ms + (now_ms() - s.at) } else { s.progress_ms };
        p.min(track.duration_ms)
    }

    // ------------------------------------------------------------------- loop
    pub async fn run(self: Arc<Self>) {
        loop {
            match self.clone().poll().await {
                Some(ms) => {
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_millis(ms)) => {}
                        _ = self.wake.notified() => {}
                    }
                }
                None => self.wake.notified().await,
            }
        }
    }

    async fn poll(self: Arc<Self>) -> Option<u64> {
        if !self.spotify.is_logged_in() {
            self.set_signed_out();
            return None;
        }
        let slow = self.inactive_ms() > SLOW_AFTER_MS;
        let result = self.spotify.request(Method::GET, "/me/player", &[("additional_types", "episode".to_string())]).await;

        let res = match result {
            Ok(res) => res,
            Err(ApiError::Auth) => {
                self.set_signed_out();
                return None;
            }
            Err(ApiError::RateLimit(secs)) => {
                if now_ms() - self.last_rate_toast.load(Ordering::Relaxed) > 60_000 {
                    self.last_rate_toast.store(now_ms(), Ordering::Relaxed);
                    self.toast(format!("Spotify API rate limit: waiting {secs}s"));
                }
                return Some(secs * 1000);
            }
            Err(ApiError::Other(e)) => {
                eprintln!("[player] poll: {e}");
                self.update(|s| {
                    if s.track.is_none() {
                        s.status = "error".into();
                    }
                    s.message = Some("No connection to Spotify".into());
                });
                return Some(8000);
            }
        };

        let at = now_ms();
        if res.status == 204 || (res.ok() && res.data.get("item").is_none_or(Value::is_null)) {
            if self.player().status != "idle" {
                self.update(|s| {
                    s.status = "idle".into();
                    s.track = None;
                    s.liked = None;
                    s.next_up = None;
                    s.message = None;
                });
                self.clear_lyrics();
            }
            return Some(if slow { 15_000 } else { 6_000 });
        }
        if !res.ok() {
            let code = res.status;
            self.update(|s| {
                if s.track.is_none() {
                    s.status = "error".into();
                }
                s.message = Some(format!("Spotify responded with HTTP {code}"));
            });
            return Some(10_000);
        }

        let d = &res.data;
        let track = normalize_track(&d["item"]);
        let playing = d["is_playing"].as_bool().unwrap_or(false);
        let progress = d["progress_ms"].as_i64().unwrap_or(0);
        let changed = self.player().track.as_ref().map(|t| t.id.as_str()) != Some(track.id.as_str());
        let next_track = track.clone();
        self.update(|s| {
            s.status = if playing { "playing" } else { "paused" }.into();
            s.track = Some(next_track);
            s.progress_ms = progress;
            s.at = at;
            s.volume = d["device"]["volume_percent"].as_i64();
            s.shuffle = d["shuffle_state"].as_bool().unwrap_or(false);
            s.repeat = d["repeat_state"].as_str().unwrap_or("off").into();
            s.device = d["device"]["name"].as_str().map(|name| Device { name: name.into(), kind: d["device"]["type"].as_str().unwrap_or("").into() });
            s.message = None;
            if changed {
                s.liked = None;
                s.next_up = None;
            }
        });
        if changed {
            self.on_track_changed(track.clone());
        }

        Some(if playing {
            // Poll right after the track ends so lyrics switch without delay.
            (track.duration_ms - progress + 350).clamp(1000, 2500) as u64
        } else if slow {
            15_000
        } else {
            5_000
        })
    }

    fn set_signed_out(&self) {
        self.update(|s| {
            s.status = "auth".into();
            s.track = None;
        });
        self.clear_lyrics();
    }

    fn on_track_changed(self: &Arc<Self>, track: Track) {
        let me = self.clone();
        let t = track.clone();
        tauri::async_runtime::spawn(async move { me.refresh_liked(t).await });
        self.refresh_next_up();
        let me = self.clone();
        tauri::async_runtime::spawn(async move { me.load_lyrics(track, false).await });
    }

    async fn refresh_liked(self: Arc<Self>, track: Track) {
        if track.kind != "track" || track.is_local {
            return;
        }
        let liked = self.spotify.is_saved(&track.id).await;
        if self.player().track.as_ref().map(|t| &t.id) == Some(&track.id) {
            self.update(|s| s.liked = Some(liked));
        }
    }

    pub fn refresh_next_up(self: &Arc<Self>) {
        let me = self.clone();
        tauri::async_runtime::spawn(async move {
            let Some(track) = me.player().track else { return };
            if !me.config().show_next_up {
                return;
            }
            let Ok(res) = me.spotify.request(Method::GET, "/me/player/queue", &[]).await else { return };
            // Spotify sometimes lists the playing track again at the head of the queue
            // (right after a track change, or during autoplay), so skip copies of it.
            let queue = res.data["queue"].as_array().cloned().unwrap_or_default();
            let next = queue.iter().find(|q| !is_same_track(q, &track)).map(|q| NextUp {
                name: q["name"].as_str().unwrap_or("").into(),
                artists: if q["type"] == "episode" {
                    vec![q["show"]["name"].as_str().unwrap_or("").into()]
                } else {
                    q["artists"].as_array().map(|a| a.iter().filter_map(|x| x["name"].as_str().map(String::from)).collect()).unwrap_or_default()
                },
            });
            if me.player().track.as_ref().map(|t| &t.id) == Some(&track.id) {
                me.update(|s| s.next_up = next);
            }
        });
    }

    // ----------------------------------------------------------------- lyrics
    fn set_lyrics(&self, next: LyricsState) {
        *self.lyrics_state.lock().unwrap() = next.clone();
        let _ = self.app.emit("lyrics", &next);
    }

    fn clear_lyrics(&self) {
        self.lyrics_token.fetch_add(1, Ordering::SeqCst);
        let current = self.lyrics_snapshot();
        if current.track_id.is_some() || current.status != "idle" {
            self.set_lyrics(LyricsState::empty());
        }
    }

    pub async fn load_lyrics(self: Arc<Self>, track: Track, force: bool) {
        let token = self.lyrics_token.fetch_add(1, Ordering::SeqCst) + 1;
        if track.kind == "episode" {
            self.set_lyrics(LyricsState { episode: true, ..LyricsState::with_status(&track.id, "ready") });
            return;
        }
        self.set_lyrics(LyricsState::with_status(&track.id, "loading"));
        if force {
            self.lyrics.delete_cache(&track.id);
        }
        let found = self.lyrics.get(&track).await;
        if token != self.lyrics_token.load(Ordering::SeqCst) {
            return;
        }
        self.set_lyrics(LyricsState {
            synced: found.synced,
            lines: found.lines,
            instrumental: found.instrumental,
            error: found.error,
            ..LyricsState::with_status(&track.id, "ready")
        });
        self.translate_current(token).await;
    }

    async fn translate_current(self: &Arc<Self>, token: u64) {
        let current = self.lyrics_snapshot();
        let Some(track_id) = current.track_id.clone() else { return };
        if current.status != "ready" || current.lines.is_empty() {
            return;
        }
        let cfg = self.config();
        if cfg.translation_mode == "off" {
            if current.translation.is_some() {
                self.set_lyrics(LyricsState { translation: None, ..current });
            }
            return;
        }
        let key = self.secrets.lock().unwrap().get_str("geminiApiKey");
        let translation = self.lyrics.translate(&track_id, &current.lines, &cfg, key).await;
        if token == self.lyrics_token.load(Ordering::SeqCst) && translation.is_some() {
            self.set_lyrics(LyricsState { translation, ..self.lyrics_snapshot() });
        }
    }

    pub fn retranslate(self: &Arc<Self>) {
        let me = self.clone();
        let token = self.lyrics_token.load(Ordering::SeqCst);
        tauri::async_runtime::spawn(async move { me.translate_current(token).await });
    }

    // --------------------------------------------------------------- commands
    pub async fn command(self: &Arc<Self>, name: &str, arg: Option<f64>) -> Value {
        let result: Result<Value, ApiError> = async {
            let s = self.player();
            match name {
                "toggle" => {
                    let playing = s.status == "playing";
                    let progress = self.current_progress();
                    self.update(|st| {
                        st.status = if playing { "paused" } else { "playing" }.into();
                        st.progress_ms = progress;
                        st.at = now_ms();
                    });
                    let path = if playing { "/me/player/pause" } else { "/me/player/play" };
                    Ok(self.finish(self.spotify.request(Method::PUT, path, &[]).await?, 400))
                }
                "next" => Ok(self.finish(self.spotify.request(Method::POST, "/me/player/next", &[]).await?, 350)),
                "prev" => {
                    if self.current_progress() > 4000 {
                        return self.seek(0).await;
                    }
                    Ok(self.finish(self.spotify.request(Method::POST, "/me/player/previous", &[]).await?, 350))
                }
                "seek" => self.seek(arg.unwrap_or(0.0).round() as i64).await,
                "volume" => {
                    let v = arg.unwrap_or(50.0).round().clamp(0.0, 100.0) as i64;
                    self.update(|st| st.volume = Some(v));
                    Ok(self.finish(self.spotify.request(Method::PUT, "/me/player/volume", &[("volume_percent", v.to_string())]).await?, 1500))
                }
                "shuffle" => {
                    let v = !s.shuffle;
                    self.update(|st| st.shuffle = v);
                    Ok(self.finish(self.spotify.request(Method::PUT, "/me/player/shuffle", &[("state", v.to_string())]).await?, 900))
                }
                "repeat" => {
                    let v = match s.repeat.as_str() {
                        "off" => "context",
                        "context" => "track",
                        _ => "off",
                    };
                    self.update(|st| st.repeat = v.into());
                    Ok(self.finish(self.spotify.request(Method::PUT, "/me/player/repeat", &[("state", v.to_string())]).await?, 900))
                }
                "like" => {
                    let Some(track) = s.track.filter(|t| t.kind == "track" && !t.is_local) else {
                        return Ok(json!({ "ok": false }));
                    };
                    let v = !s.liked.unwrap_or(false);
                    self.update(|st| st.liked = Some(v));
                    if !self.spotify.set_saved(&track.id, v).await {
                        self.update(|st| st.liked = Some(!v));
                        return Ok(self.fail("Couldn’t update your Liked Songs."));
                    }
                    self.toast(if v { "Added to Liked Songs" } else { "Removed from Liked Songs" });
                    Ok(json!({ "ok": true }))
                }
                "resync" => {
                    if let Some(track) = s.track {
                        self.toast("Fetching lyrics again…");
                        self.clone().load_lyrics(track, true).await;
                        self.poke(0);
                    }
                    Ok(json!({ "ok": true }))
                }
                _ => Ok(json!({ "ok": false })),
            }
        }
        .await;

        match result {
            Ok(v) => v,
            Err(ApiError::Auth) => self.fail("Connect your Spotify account in Settings."),
            Err(ApiError::RateLimit(_)) => self.fail("Spotify API rate limit: try again in a few seconds."),
            Err(ApiError::Other(_)) => self.fail("No connection to Spotify."),
        }
    }

    async fn seek(self: &Arc<Self>, ms: i64) -> Result<Value, ApiError> {
        let Some(track) = self.player().track else { return Ok(json!({ "ok": false })) };
        let ms = ms.clamp(0, track.duration_ms);
        self.update(|st| {
            st.progress_ms = ms;
            st.at = now_ms();
        });
        Ok(self.finish(self.spotify.request(Method::PUT, "/me/player/seek", &[("position_ms", ms.to_string())]).await?, 700))
    }

    fn finish(self: &Arc<Self>, res: Resp, delay: u64) -> Value {
        self.poke(delay);
        if res.ok() {
            return json!({ "ok": true });
        }
        let error = &res.data["error"];
        if res.status == 403 && error["reason"] == "PREMIUM_REQUIRED" {
            return self.fail("Playback controls require Spotify Premium.");
        }
        if res.status == 404 {
            return self.fail("No active Spotify device.");
        }
        match error["message"].as_str() {
            Some(m) => self.fail(&format!("Spotify: {m}")),
            None => self.fail(&format!("Spotify responded with HTTP {}", res.status)),
        }
    }

    fn fail(self: &Arc<Self>, message: &str) -> Value {
        self.toast(message);
        self.poke(300);
        json!({ "ok": false, "message": message })
    }
}

/// Whether a queue item is the given track. Relinked tracks can come back with
/// another id, so name and main artist are compared too.
fn is_same_track(item: &Value, track: &Track) -> bool {
    if item["id"].as_str() == Some(track.id.as_str()) || item["uri"].as_str() == Some(track.uri.as_str()) {
        return true;
    }
    let name = item["name"].as_str().unwrap_or("");
    let artist = item["artists"][0]["name"].as_str().or_else(|| item["show"]["name"].as_str()).unwrap_or("");
    !name.is_empty() && name.eq_ignore_ascii_case(&track.name) && track.artists.first().is_some_and(|a| a.eq_ignore_ascii_case(artist))
}

fn normalize_track(item: &Value) -> Track {
    let episode = item["type"] == "episode";
    let images = if episode {
        item["images"].as_array().or(item["show"]["images"].as_array())
    } else {
        item["album"]["images"].as_array()
    };
    Track {
        id: item["id"].as_str().or(item["uri"].as_str()).unwrap_or_default().to_string(),
        uri: item["uri"].as_str().unwrap_or_default().to_string(),
        kind: item["type"].as_str().unwrap_or("track").to_string(),
        name: item["name"].as_str().unwrap_or("Untitled").to_string(),
        artists: if episode {
            vec![item["show"]["name"].as_str().unwrap_or("Podcast").to_string()]
        } else {
            item["artists"].as_array().map(|a| a.iter().filter_map(|x| x["name"].as_str().map(String::from)).collect()).unwrap_or_default()
        },
        album: if episode { item["show"]["name"].as_str() } else { item["album"]["name"].as_str() }.unwrap_or_default().to_string(),
        cover: images.and_then(|i| i.first()).and_then(|i| i["url"].as_str()).map(String::from),
        duration_ms: item["duration_ms"].as_i64().unwrap_or(0),
        is_local: item["is_local"].as_bool().unwrap_or(false),
        url: item["external_urls"]["spotify"].as_str().map(String::from),
    }
}
