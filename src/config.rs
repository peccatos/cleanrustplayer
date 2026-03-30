use std::env;
use std::path::PathBuf;

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub local_music_roots: Vec<PathBuf>,
    pub cloud: Option<CloudConfig>,
    pub web: WebConfig,
}

#[derive(Debug, Clone)]
pub struct CloudConfig {
    pub folder_id: String,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub api_key: Option<String>,
    pub cache_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct WebConfig {
    pub host: String,
    pub port: u16,
    pub sqlite_path: PathBuf,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        let local_music_roots = configured_music_roots();
        let cloud = configured_cloud_config();
        let web = configured_web_config();

        Ok(Self {
            local_music_roots,
            cloud,
            web,
        })
    }
}

fn configured_music_roots() -> Vec<PathBuf> {
    if let Ok(raw) = env::var("REPLAYCORE_LOCAL_MUSIC_ROOTS") {
        let roots: Vec<PathBuf> = raw
            .split([';', ','])
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .collect();

        if !roots.is_empty() {
            return roots;
        }
    }

    if let Ok(raw) = env::var("REPLAYCORE_LOCAL_MUSIC_ROOT") {
        let root = raw.trim();
        if !root.is_empty() {
            return vec![PathBuf::from(root)];
        }
    }

    vec![default_music_dir()]
}

fn default_music_dir() -> PathBuf {
    PathBuf::from("library")
}

fn configured_cloud_config() -> Option<CloudConfig> {
    let raw = env::var("REPLAYCORE_GOOGLE_DRIVE_FOLDER_ID").ok()?;
    let folder_id = normalize_drive_folder_id(&raw)?;

    let access_token = env::var("REPLAYCORE_GOOGLE_DRIVE_ACCESS_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let refresh_token = env::var("REPLAYCORE_GOOGLE_DRIVE_REFRESH_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let client_id = env::var("REPLAYCORE_GOOGLE_DRIVE_CLIENT_ID")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let client_secret = env::var("REPLAYCORE_GOOGLE_DRIVE_CLIENT_SECRET")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let api_key = env::var("REPLAYCORE_GOOGLE_DRIVE_API_KEY")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    Some(CloudConfig {
        folder_id,
        access_token,
        refresh_token,
        client_id,
        client_secret,
        api_key,
        cache_dir: env::var("REPLAYCORE_GOOGLE_DRIVE_CACHE_DIR")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("replaycore-drive-cache")),
    })
}

fn configured_web_config() -> WebConfig {
    let host = env::var("REPLAYCORE_HTTP_HOST")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "127.0.0.1".to_string());

    let port = env::var("REPLAYCORE_HTTP_PORT")
        .ok()
        .and_then(|value| value.trim().parse::<u16>().ok())
        .unwrap_or(3001);

    let sqlite_path = env::var("REPLAYCORE_SQLITE_PATH")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data").join("replaycore.sqlite3"));

    WebConfig {
        host,
        port,
        sqlite_path,
    }
}

fn normalize_drive_folder_id(raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() {
        return None;
    }

    if let Some(folder_id) = extract_drive_folder_id_from_url(value) {
        return Some(folder_id);
    }

    Some(value.to_string())
}

fn extract_drive_folder_id_from_url(value: &str) -> Option<String> {
    let marker = "/folders/";
    let start = value.find(marker)? + marker.len();
    let tail = &value[start..];
    let folder_id = tail
        .split(['?', '&', '/'])
        .next()
        .map(str::trim)
        .filter(|segment| !segment.is_empty())?;

    Some(folder_id.to_string())
}
