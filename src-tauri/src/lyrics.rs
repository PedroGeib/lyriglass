//! Lyrics lookup (LRCLIB), on-disk cache and translation (free Google endpoint or Gemini).

use crate::player::Track;
use crate::store::{now_ms, Config};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

const UA: &str = "Lyriglass/1.0 (https://github.com/PedroGeib/lyriglass)";
const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36";
const CACHE_VERSION: u32 = 2;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Line {
    pub t: i64,
    pub text: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Translation {
    pub lang: String,
    pub provider: String,
    pub same: bool,
    pub lines: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Found {
    pub source: String,
    pub synced: bool,
    pub lines: Vec<Line>,
    pub instrumental: bool,
    pub error: bool,
    pub translation: Option<Translation>,
    pub v: u32,
    pub saved_at: i64,
}

pub struct Lyrics {
    dir: PathBuf,
    http: Client,
}

impl Lyrics {
    pub fn new(data_dir: PathBuf, http: Client) -> Self {
        let dir = data_dir.join("lyrics-cache");
        let _ = fs::create_dir_all(&dir);
        Self { dir, http }
    }

    pub async fn get(&self, track: &Track) -> Found {
        if let Some(cached) = self.read_cache(&track.id) {
            return cached;
        }
        match self.fetch_lrclib(track).await {
            Ok(found) => {
                self.write_cache(&track.id, &found);
                found
            }
            Err(e) => {
                eprintln!("[lyrics] LRCLIB failed: {e}");
                Found { source: "none".into(), error: true, ..Default::default() }
            }
        }
    }

    async fn lrclib(&self, endpoint: &str, params: &[(&str, String)]) -> Result<Option<Value>, String> {
        let params: Vec<&(&str, String)> = params.iter().filter(|(_, v)| !v.is_empty()).collect();
        let res = self
            .http
            .get(format!("https://lrclib.net{endpoint}"))
            .query(&params)
            .header("User-Agent", UA)
            .timeout(Duration::from_secs(8))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if res.status().as_u16() == 404 {
            return Ok(None);
        }
        if !res.status().is_success() {
            return Err(format!("LRCLIB HTTP {}", res.status()));
        }
        res.json::<Value>().await.map(Some).map_err(|e| e.to_string())
    }

    async fn fetch_lrclib(&self, track: &Track) -> Result<Found, String> {
        let artist = track.artists.first().cloned().unwrap_or_default();
        let title = clean_title(&track.name);
        let duration = (track.duration_ms as f64 / 1000.0).round() as i64;
        let synced = |v: &Value| v["syncedLyrics"].as_str().filter(|s| !s.trim().is_empty()).map(String::from);
        let plain_of = |v: &Value| v["plainLyrics"].as_str().filter(|s| !s.trim().is_empty()).map(String::from);

        let attempts = [
            vec![
                ("track_name", track.name.clone()),
                ("artist_name", artist.clone()),
                ("album_name", track.album.clone()),
                ("duration", duration.to_string()),
            ],
            vec![("track_name", title.clone()), ("artist_name", artist.clone()), ("duration", duration.to_string())],
        ];
        let mut plain: Option<String> = None;
        for params in &attempts {
            if let Some(v) = self.lrclib("/api/get", params).await? {
                if let Some(s) = synced(&v) {
                    return Ok(from_lrc(&s));
                }
                if plain.is_none() {
                    plain = plain_of(&v);
                }
            }
        }

        if let Some(Value::Array(results)) = self.lrclib("/api/search", &[("track_name", title), ("artist_name", artist)]).await? {
            let mut scored: Vec<(&Value, i64)> = results
                .iter()
                .map(|r| (r, (r["duration"].as_f64().unwrap_or(0.0).round() as i64 - duration).abs()))
                .filter(|(_, diff)| *diff <= 6)
                .collect();
            scored.sort_by(|a, b| synced(b.0).is_some().cmp(&synced(a.0).is_some()).then(a.1.cmp(&b.1)));
            if let Some((best, _)) = scored.first() {
                if let Some(s) = synced(best) {
                    return Ok(from_lrc(&s));
                }
                if plain.is_none() {
                    plain = plain_of(best);
                }
                if best["instrumental"].as_bool() == Some(true) {
                    return Ok(Found { source: "lrclib".into(), instrumental: true, ..Default::default() });
                }
            }
        }

        Ok(match plain {
            Some(text) => Found {
                source: "lrclib".into(),
                synced: false,
                lines: text.lines().map(str::trim).filter(|l| !l.is_empty()).map(|l| Line { t: 0, text: l.into() }).collect(),
                ..Default::default()
            },
            None => Found { source: "none".into(), ..Default::default() },
        })
    }

    // ------------------------------------------------------------ translation
    pub async fn translate(&self, track_id: &str, lines: &[Line], cfg: &Config, gemini_key: Option<String>) -> Option<Translation> {
        if cfg.translation_mode == "off" || lines.is_empty() {
            return None;
        }
        let target = cfg.translation_target.clone();
        let cached = self.read_cache(track_id);
        if let Some(t) = cached.as_ref().and_then(|c| c.translation.as_ref()) {
            if t.lang == target && t.lines.len() == lines.len() {
                return Some(t.clone());
            }
        }

        let texts: Vec<String> = lines.iter().map(|l| l.text.clone()).collect();
        let mut translation = None;
        if cfg.translation_mode == "gemini" {
            if let Some(key) = gemini_key {
                match self.gemini(&texts, &target, &cfg.gemini_model, &key).await {
                    Ok(t) => translation = Some(t),
                    Err(e) => eprintln!("[translate] Gemini failed, using Google: {e}"),
                }
            }
        }
        if translation.is_none() {
            translation = self.google(&texts, &target).await;
        }
        let translation = translation?;

        if let Some(mut base) = cached {
            base.translation = Some(translation.clone());
            self.write_cache(track_id, &base);
        }
        Some(translation)
    }

    async fn google(&self, texts: &[String], target: &str) -> Option<Translation> {
        // Batches of ~1800 chars keep the GET URL within limits.
        let mut batches: Vec<Vec<usize>> = vec![];
        let mut current = vec![];
        let mut size = 0;
        for (i, t) in texts.iter().enumerate() {
            if size + t.len() > 1800 && !current.is_empty() {
                batches.push(std::mem::take(&mut current));
                size = 0;
            }
            current.push(i);
            size += t.len() + 1;
        }
        if !current.is_empty() {
            batches.push(current);
        }

        let mut result = vec![String::new(); texts.len()];
        let mut same_language = true;
        for batch in batches {
            let indices: Vec<usize> = batch.into_iter().filter(|i| !texts[*i].is_empty()).collect();
            if indices.is_empty() {
                continue;
            }
            let query = indices.iter().map(|i| texts[*i].as_str()).collect::<Vec<_>>().join("\n");
            let data = self.google_call(&query, target).await?;
            if let Some(detected) = data.get(2).and_then(Value::as_str) {
                if !detected.to_lowercase().starts_with(&target.to_lowercase()) {
                    same_language = false;
                }
            }
            let joined = google_segments(&data);
            let parts: Vec<&str> = joined.split('\n').collect();
            if parts.len() == indices.len() {
                for (k, i) in indices.iter().enumerate() {
                    result[*i] = parts[k].trim().to_string();
                }
            } else {
                // Line count drifted: translate this batch line by line.
                for i in &indices {
                    if let Some(d) = self.google_call(&texts[*i], target).await {
                        result[*i] = google_segments(&d).trim().to_string();
                    }
                }
            }
        }

        if same_language {
            return Some(Translation { lang: target.into(), provider: "google".into(), same: true, lines: vec![String::new(); texts.len()] });
        }
        Some(Translation { lang: target.into(), provider: "google".into(), same: false, lines: result })
    }

    async fn google_call(&self, text: &str, target: &str) -> Option<Value> {
        let res = self
            .http
            .get("https://translate.googleapis.com/translate_a/single")
            .query(&[("client", "gtx"), ("sl", "auto"), ("tl", target), ("dt", "t"), ("q", text)])
            .header("User-Agent", BROWSER_UA)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .ok()?;
        if !res.status().is_success() {
            eprintln!("[translate] Google HTTP {}", res.status());
            return None;
        }
        res.json().await.ok()
    }

    async fn gemini(&self, texts: &[String], target: &str, model: &str, key: &str) -> Result<Translation, String> {
        let numbered: Vec<Value> = texts.iter().enumerate().map(|(i, t)| json!({ "i": i, "t": t })).collect();
        let prompt = format!(
            "Translate each line of these song lyrics into the language \"{target}\", naturally and keeping the meaning.\n\
             Empty lines stay empty. If a line is already in the target language, repeat it.\n\
             Reply ONLY with a JSON array of strings, in the same order, with exactly {} items.\n{}",
            texts.len(),
            Value::Array(numbered)
        );
        let model: String = model.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')).collect();
        let res = self
            .http
            .post(format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"))
            .header("x-goog-api-key", key)
            .json(&json!({
                "contents": [{ "parts": [{ "text": prompt }] }],
                "generationConfig": { "responseMimeType": "application/json", "temperature": 0.3 }
            }))
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !res.status().is_success() {
            return Err(format!("HTTP {}", res.status()));
        }
        let data: Value = res.json().await.map_err(|e| e.to_string())?;
        let raw = data["candidates"][0]["content"]["parts"][0]["text"].as_str().unwrap_or("").trim();
        let raw = raw.trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
        let items: Vec<Value> = serde_json::from_str(raw).map_err(|e| e.to_string())?;
        if items.len() != texts.len() {
            return Err("response has a different line count".into());
        }
        let lines = items
            .iter()
            .map(|v| match v {
                Value::String(s) => s.clone(),
                Value::Null => String::new(),
                other => other.to_string(),
            })
            .collect();
        Ok(Translation { lang: target.into(), provider: "gemini".into(), same: false, lines })
    }

    // ------------------------------------------------------------------ cache
    fn cache_path(&self, track_id: &str) -> PathBuf {
        let safe: String = track_id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        self.dir.join(format!("{safe}.json"))
    }

    fn read_cache(&self, track_id: &str) -> Option<Found> {
        let found: Found = serde_json::from_slice(&fs::read(self.cache_path(track_id)).ok()?).ok()?;
        if found.v != CACHE_VERSION {
            return None;
        }
        // "Not found" results expire after 3 days; lyrics may be added to LRCLIB later.
        if found.lines.is_empty() && now_ms() - found.saved_at > 3 * 86_400_000 {
            return None;
        }
        Some(found)
    }

    fn write_cache(&self, track_id: &str, found: &Found) {
        let mut entry = found.clone();
        entry.v = CACHE_VERSION;
        entry.saved_at = now_ms();
        if let Ok(bytes) = serde_json::to_vec(&entry) {
            let _ = fs::write(self.cache_path(track_id), bytes);
        }
    }

    pub fn delete_cache(&self, track_id: &str) {
        let _ = fs::remove_file(self.cache_path(track_id));
    }

    pub fn clear_cache(&self) -> usize {
        let Ok(entries) = fs::read_dir(&self.dir) else { return 0 };
        entries
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter(|e| fs::remove_file(e.path()).is_ok())
            .count()
    }
}

fn google_segments(data: &Value) -> String {
    data.get(0)
        .and_then(Value::as_array)
        .map(|segments| segments.iter().filter_map(|s| s.get(0).and_then(Value::as_str)).collect())
        .unwrap_or_default()
}

fn from_lrc(text: &str) -> Found {
    let lines = parse_lrc(text);
    Found { source: "lrclib".into(), synced: !lines.is_empty(), lines, ..Default::default() }
}

pub fn parse_lrc(text: &str) -> Vec<Line> {
    let mut offset = 0i64;
    let mut out = vec![];
    for raw in text.lines() {
        let mut rest = raw.trim_start();
        let mut times = vec![];
        while rest.starts_with('[') {
            let Some(end) = rest.find(']') else { break };
            let tag = &rest[1..end];
            if let Some(t) = parse_timestamp(tag) {
                times.push(t);
            } else if let Some(o) = tag.strip_prefix("offset:") {
                offset = o.trim().parse().unwrap_or(0);
            }
            rest = &rest[end + 1..];
        }
        if times.is_empty() {
            continue;
        }
        // "♪" marks an instrumental break: store as an empty line (rendered as a pause).
        let text = rest.trim();
        let text = if text.chars().all(|c| c == '♪' || c == '♫' || c.is_whitespace()) { "" } else { text };
        for t in times {
            out.push(Line { t, text: text.to_string() });
        }
    }
    for line in &mut out {
        line.t = (line.t - offset).max(0);
    }
    out.sort_by_key(|l| l.t);

    // Drop leading and repeated empty lines.
    let mut result: Vec<Line> = Vec::with_capacity(out.len());
    for line in out {
        if line.text.is_empty() && result.last().is_none_or(|prev| prev.text.is_empty()) {
            continue;
        }
        result.push(line);
    }
    result
}

fn parse_timestamp(tag: &str) -> Option<i64> {
    let (min, rest) = tag.split_once(':')?;
    let min: i64 = min.trim().parse().ok()?;
    let (sec, frac) = rest.split_once(['.', ':']).unwrap_or((rest, ""));
    let sec: i64 = sec.trim().parse().ok()?;
    let ms = if frac.is_empty() {
        0
    } else {
        let digits: String = frac.chars().take(3).collect();
        format!("{digits:0<3}").parse::<i64>().ok()?
    };
    Some(min * 60_000 + sec * 1000 + ms)
}

/// Strips "(feat. X)", "- Remastered 2011", "(Live)" and similar noise from a title.
pub fn clean_title(title: &str) -> String {
    const NOISE: [&str; 13] = ["feat", "ft.", "with ", "remaster", "live", "version", "edit", "mix", "mono", "stereo", "deluxe", "bonus", "acoustic"];
    let noisy = |s: &str| {
        let lower = s.to_lowercase();
        NOISE.iter().any(|k| lower.contains(k))
    };

    let mut out = String::with_capacity(title.len());
    let mut rest = title;
    while let Some(start) = rest.find(['(', '[']) {
        let close = if rest.as_bytes()[start] == b'(' { ')' } else { ']' };
        let Some(len) = rest[start..].find(close) else { break };
        let group = &rest[start..start + len + 1];
        out.push_str(&rest[..start]);
        if !noisy(group) {
            out.push_str(group);
        }
        rest = &rest[start + len + 1..];
    }
    out.push_str(rest);

    if let Some(idx) = out.find(" - ") {
        if noisy(&out[idx..]) {
            out.truncate(idx);
        }
    }
    let cleaned = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() { title.to_string() } else { cleaned }
}
