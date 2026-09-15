//! Spotify Web API client with PKCE login (no client secret needed).

use crate::store::{now_ms, Config, Secrets};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use reqwest::{Client, Method, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

pub const REDIRECT_URI: &str = "http://127.0.0.1:8888/callback";
const SCOPES: &str = "user-read-playback-state user-modify-playback-state user-read-currently-playing user-library-read user-library-modify";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Profile {
    pub name: String,
    pub product: Option<String>,
    pub image: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Auth {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    pub profile: Option<Profile>,
}

#[derive(Debug)]
pub enum ApiError {
    Auth,
    RateLimit(u64),
    Other(String),
}

pub struct Resp {
    pub status: u16,
    pub data: Value,
}

impl Resp {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

pub struct Spotify {
    config: Arc<Mutex<Config>>,
    secrets: Arc<Mutex<Secrets>>,
    http: Client,
    refresh_lock: tokio::sync::Mutex<()>,
    login_cancel: Mutex<Option<oneshot::Sender<()>>>,
    unified_library: AtomicBool,
}

impl Spotify {
    pub fn new(config: Arc<Mutex<Config>>, secrets: Arc<Mutex<Secrets>>, http: Client) -> Self {
        Self {
            config,
            secrets,
            http,
            refresh_lock: tokio::sync::Mutex::new(()),
            login_cancel: Mutex::new(None),
            unified_library: AtomicBool::new(false),
        }
    }

    pub fn auth(&self) -> Option<Auth> {
        let secrets = self.secrets.lock().unwrap();
        secrets.get("spotify").and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    fn save_auth(&self, auth: &Auth) {
        let value = serde_json::to_value(auth).ok();
        self.secrets.lock().unwrap().set("spotify", value);
    }

    pub fn is_logged_in(&self) -> bool {
        self.auth().is_some_and(|a| !a.refresh_token.is_empty())
    }

    pub fn logout(&self) {
        self.secrets.lock().unwrap().set("spotify", None);
    }

    fn client_id(&self) -> String {
        self.config.lock().unwrap().spotify_client_id.trim().to_string()
    }

    // ------------------------------------------------------------------ login
    pub fn cancel_login(&self) {
        if let Some(tx) = self.login_cancel.lock().unwrap().take() {
            let _ = tx.send(());
        }
    }

    pub async fn login(&self, open_browser: impl FnOnce(String) + Send) -> Result<Profile, String> {
        let client_id = self.client_id();
        if client_id.is_empty() {
            return Err("Informe o Client ID nas configurações.".into());
        }
        if client_id.len() != 32 || !client_id.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("O Client ID deve ter 32 caracteres (letras a–f e números).".into());
        }

        let verifier = URL_SAFE_NO_PAD.encode(random_bytes(64));
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let state: String = random_bytes(16).iter().map(|b| format!("{b:02x}")).collect();

        self.cancel_login();
        let listener = TcpListener::bind("127.0.0.1:8888").await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::AddrInUse {
                "A porta 8888 está em uso. Feche outros apps de overlay e tente de novo.".to_string()
            } else {
                e.to_string()
            }
        })?;
        let (tx, rx) = oneshot::channel();
        *self.login_cancel.lock().unwrap() = Some(tx);

        let url = Url::parse_with_params(
            "https://accounts.spotify.com/authorize",
            &[
                ("response_type", "code"),
                ("client_id", client_id.as_str()),
                ("scope", SCOPES),
                ("redirect_uri", REDIRECT_URI),
                ("state", state.as_str()),
                ("code_challenge_method", "S256"),
                ("code_challenge", challenge.as_str()),
            ],
        )
        .map_err(|e| e.to_string())?;
        open_browser(url.to_string());

        let code = tokio::select! {
            result = wait_for_callback(&listener, &state) => result,
            _ = rx => Err("Login cancelado.".to_string()),
            _ = tokio::time::sleep(Duration::from_secs(300)) => Err("Tempo esgotado aguardando o login.".to_string()),
        };
        self.login_cancel.lock().unwrap().take();
        drop(listener);
        let code = code?;

        let tokens = self
            .token_request(&[
                ("grant_type", "authorization_code"),
                ("code", code.as_str()),
                ("redirect_uri", REDIRECT_URI),
                ("client_id", client_id.as_str()),
                ("code_verifier", verifier.as_str()),
            ])
            .await?;
        self.store_tokens(&tokens);
        self.refresh_profile().await
    }

    pub async fn refresh_profile(&self) -> Result<Profile, String> {
        let r = self.request(Method::GET, "/me", &[]).await.map_err(|e| format!("{e:?}"))?;
        if !r.ok() {
            return Err(format!("Perfil indisponível (HTTP {})", r.status));
        }
        let mut images: Vec<(i64, String)> = r.data["images"]
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(|i| Some((i["width"].as_i64().unwrap_or(0), i["url"].as_str()?.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        images.sort_by_key(|i| i.0);
        let image = images.iter().find(|i| i.0 >= 96).or(images.last()).map(|i| i.1.clone());

        let profile = Profile {
            name: r.data["display_name"].as_str().or(r.data["id"].as_str()).unwrap_or("Spotify").to_string(),
            product: r.data["product"].as_str().map(String::from),
            image,
        };
        let mut auth = self.auth().unwrap_or_default();
        auth.profile = Some(profile.clone());
        self.save_auth(&auth);
        Ok(profile)
    }

    // ----------------------------------------------------------------- tokens
    async fn token_request(&self, params: &[(&str, &str)]) -> Result<Value, String> {
        let res = self
            .http
            .post("https://accounts.spotify.com/api/token")
            .form(params)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = res.status();
        let data: Value = res.json().await.unwrap_or(Value::Null);
        if !status.is_success() || data.get("access_token").is_none() {
            let msg = data["error_description"].as_str().or(data["error"].as_str());
            return Err(msg.map(String::from).unwrap_or_else(|| format!("Falha no token (HTTP {})", status.as_u16())));
        }
        Ok(data)
    }

    fn store_tokens(&self, data: &Value) {
        let mut auth = self.auth().unwrap_or_default();
        auth.access_token = data["access_token"].as_str().unwrap_or_default().to_string();
        if let Some(refresh) = data["refresh_token"].as_str() {
            auth.refresh_token = refresh.to_string();
        }
        auth.expires_at = now_ms() + data["expires_in"].as_i64().unwrap_or(3600) * 1000;
        self.save_auth(&auth);
    }

    async fn refresh(&self, force: bool) -> Result<(), ApiError> {
        let _guard = self.refresh_lock.lock().await;
        let auth = self.auth().ok_or(ApiError::Auth)?;
        if auth.refresh_token.is_empty() {
            return Err(ApiError::Auth);
        }
        if !force && now_ms() < auth.expires_at - 60_000 {
            return Ok(()); // another task refreshed while we waited
        }
        let client_id = self.client_id();
        match self
            .token_request(&[("grant_type", "refresh_token"), ("refresh_token", auth.refresh_token.as_str()), ("client_id", client_id.as_str())])
            .await
        {
            Ok(data) => {
                self.store_tokens(&data);
                Ok(())
            }
            Err(e) if e.contains("invalid_grant") || e.to_lowercase().contains("revoked") || e.contains("Invalid refresh token") => {
                self.logout();
                Err(ApiError::Auth)
            }
            Err(e) => Err(ApiError::Other(e)),
        }
    }

    // ---------------------------------------------------------------- request
    pub async fn request(&self, method: Method, path: &str, query: &[(&str, String)]) -> Result<Resp, ApiError> {
        for attempt in 0..2 {
            let auth = self.auth().ok_or(ApiError::Auth)?;
            if auth.refresh_token.is_empty() {
                return Err(ApiError::Auth);
            }
            if attempt == 1 || now_ms() > auth.expires_at - 60_000 {
                self.refresh(attempt == 1).await?;
            }
            let token = self.auth().ok_or(ApiError::Auth)?.access_token;

            let mut req = self
                .http
                .request(method.clone(), format!("https://api.spotify.com/v1{path}"))
                .bearer_auth(token)
                .query(query)
                .timeout(Duration::from_secs(8));
            if method == Method::PUT || method == Method::POST {
                req = req.body(Vec::new()); // Spotify rejects bodiless PUT/POST without Content-Length
            }
            let res = req.send().await.map_err(|e| ApiError::Other(e.to_string()))?;
            let status = res.status().as_u16();
            if status == 401 && attempt == 0 {
                continue;
            }
            if status == 429 {
                let secs = res
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(30);
                return Err(ApiError::RateLimit(secs));
            }
            let text = res.text().await.unwrap_or_default();
            let data = serde_json::from_str(&text).unwrap_or(Value::Null);
            return Ok(Resp { status, data });
        }
        Err(ApiError::Auth)
    }

    // ---------------------------------------------------------------- library
    // Spotify moved library endpoints; try the legacy ones and fall back to the unified API.
    pub async fn is_saved(&self, track_id: &str) -> bool {
        match self.library("contains", track_id).await {
            Ok(r) => r.data.get(0).and_then(Value::as_bool).unwrap_or(false),
            Err(_) => false,
        }
    }

    pub async fn set_saved(&self, track_id: &str, saved: bool) -> bool {
        self.library(if saved { "save" } else { "remove" }, track_id).await.map(|r| r.ok()).unwrap_or(false)
    }

    async fn library(&self, action: &str, track_id: &str) -> Result<Resp, ApiError> {
        let unified_first = self.unified_library.load(Ordering::Relaxed);
        let mut last = None;
        for unified in [unified_first, !unified_first] {
            let (method, path, param) = match (unified, action) {
                (false, "contains") => (Method::GET, "/me/tracks/contains", ("ids", track_id.to_string())),
                (false, "save") => (Method::PUT, "/me/tracks", ("ids", track_id.to_string())),
                (false, _) => (Method::DELETE, "/me/tracks", ("ids", track_id.to_string())),
                (true, "contains") => (Method::GET, "/me/library/contains", ("uris", format!("spotify:track:{track_id}"))),
                (true, "save") => (Method::PUT, "/me/library", ("uris", format!("spotify:track:{track_id}"))),
                (true, _) => (Method::DELETE, "/me/library", ("uris", format!("spotify:track:{track_id}"))),
            };
            let r = self.request(method, path, &[param]).await?;
            if r.ok() {
                self.unified_library.store(unified, Ordering::Relaxed);
                return Ok(r);
            }
            let retry = matches!(r.status, 403 | 404 | 410);
            last = Some(r);
            if !retry {
                break;
            }
        }
        last.ok_or_else(|| ApiError::Other("library".into()))
    }
}

async fn wait_for_callback(listener: &TcpListener, expected_state: &str) -> Result<String, String> {
    loop {
        let (mut socket, _) = listener.accept().await.map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 8192];
        let mut len = 0;
        loop {
            let n = socket.read(&mut buf[len..]).await.unwrap_or(0);
            if n == 0 {
                break;
            }
            len += n;
            if len == buf.len() || buf[..len].windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        let request = String::from_utf8_lossy(&buf[..len]);
        let target = request.split_whitespace().nth(1).unwrap_or("");
        let (route, query) = target.split_once('?').unwrap_or((target, ""));
        if route != "/callback" {
            let _ = socket.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
            continue;
        }

        let code = query_param(query, "code");
        let state = query_param(query, "state");
        let error = query_param(query, "error");
        let ok = code.is_some() && state.as_deref() == Some(expected_state);
        let body = callback_page(ok, error.as_deref());
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = socket.write_all(response.as_bytes()).await;
        return match (ok, code) {
            (true, Some(code)) => Ok(code),
            _ => Err(error.unwrap_or_else(|| "Resposta de login inválida.".into())),
        };
    }
}

fn random_bytes(n: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; n];
    getrandom::fill(&mut bytes).expect("system RNG unavailable");
    bytes
}

fn query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        (k == key).then(|| percent_decode(v))
    })
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() + 0 && i + 2 <= bytes.len() - 1 => {
                match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn callback_page(ok: bool, error: Option<&str>) -> String {
    let (title, icon, message) = if ok {
        ("Tudo certo!", "🎧", "Login concluído. Pode fechar esta aba e voltar para o Lyriglass.".to_string())
    } else {
        let safe: String = error.unwrap_or("erro desconhecido").chars().filter(|c| !matches!(c, '<' | '>' | '&' | '"')).collect();
        ("Não deu certo", "⚠️", format!("O Spotify retornou: {safe}"))
    };
    format!(
        r#"<!doctype html><meta charset="utf-8"><title>{title}</title>
<body style="margin:0;height:100vh;display:grid;place-items:center;background:#0d0f14;color:#e8eaf0;font-family:Segoe UI,system-ui,sans-serif">
<div style="text-align:center"><div style="font-size:48px">{icon}</div><h1 style="font-weight:600;margin:.4em 0">{title}</h1><p style="color:#9aa3b2">{message}</p></div></body>"#
    )
}
