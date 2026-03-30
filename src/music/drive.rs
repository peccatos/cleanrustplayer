use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use reqwest::blocking::Client;
use reqwest::Url;
use serde::Deserialize;

use crate::music::library::Track;

const DRIVE_API_BASE: &str = "https://www.googleapis.com/drive/v3";

#[derive(Debug, Clone)]
pub struct GoogleDriveConfig {
    pub folder_id: String,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub api_key: Option<String>,
    pub cache_dir: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DriveListResponse {
    #[serde(default)]
    next_page_token: Option<String>,
    files: Vec<DriveFile>,
}

#[derive(Debug, Deserialize)]
struct DriveFile {
    id: String,
    name: String,
    #[serde(rename = "mimeType")]
    mime_type: String,
}

pub fn sync_drive_folder(config: &GoogleDriveConfig) -> Result<Vec<Track>> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .context("failed to build Google Drive client")?;

    let access_token = resolve_access_token(&client, config)?;
    let mut runtime_config = config.clone();
    runtime_config.access_token = access_token;

    fs::create_dir_all(&config.cache_dir)
        .with_context(|| format!("failed to create cache dir {}", config.cache_dir.display()))?;

    let files = list_drive_files(&client, &runtime_config)?;
    let mut tracks = Vec::new();

    for file in files {
        if file.mime_type == "application/vnd.google-apps.folder" {
            continue;
        }
        if !is_supported_audio_name(&file.name) {
            continue;
        }

        let safe_name = sanitize_filename(&file.name);
        let cache_path = config.cache_dir.join(format!("{}_{}", file.id, safe_name));
        download_drive_file(&client, &runtime_config, &file.id, &cache_path)?;
        tracks.push(Track::from_path(cache_path)?);
    }

    Ok(tracks)
}

fn list_drive_files(client: &Client, config: &GoogleDriveConfig) -> Result<Vec<DriveFile>> {
    let mut page_token: Option<String> = None;
    let mut files = Vec::new();

    loop {
        let mut url = Url::parse(&format!("{DRIVE_API_BASE}/files"))
            .context("failed to build Google Drive list URL")?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("q", &format!("'{}' in parents and trashed = false", config.folder_id));
            query.append_pair("fields", "nextPageToken,files(id,name,mimeType)");
            query.append_pair("pageSize", "1000");
            query.append_pair("supportsAllDrives", "true");
            query.append_pair("includeItemsFromAllDrives", "true");
            if let Some(api_key) = config.api_key.as_deref() {
                query.append_pair("key", api_key);
            }
            if let Some(token) = page_token.as_deref() {
                query.append_pair("pageToken", token);
            }
        }

        let mut request = client.get(url);

        if let Some(token) = config.access_token.as_deref() {
            request = request.bearer_auth(token);
        }

        let response = request.send().context("failed to query Google Drive")?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            let hint = if config.access_token.is_some() {
                "check that the token has Drive read access to this folder"
            } else {
                "if the folder is not public, add REPLAYCORE_GOOGLE_DRIVE_ACCESS_TOKEN or refresh token settings"
            };
            anyhow::bail!(
                "Google Drive list failed: {status}. {hint}. response: {body}"
            );
        }

        let mut body = String::new();
        let mut reader = response;
        reader
            .read_to_string(&mut body)
            .context("failed to read Google Drive list response")?;
        let payload: DriveListResponse =
            serde_json::from_str(&body).context("failed to decode Google Drive list response")?;
        files.extend(payload.files);

        page_token = payload.next_page_token;
        if page_token.is_none() {
            break;
        }
    }

    Ok(files)
}

fn download_drive_file(
    client: &Client,
    config: &GoogleDriveConfig,
    file_id: &str,
    cache_path: &Path,
) -> Result<()> {
    let mut url = Url::parse(&format!("{DRIVE_API_BASE}/files/{file_id}"))
        .context("failed to build Google Drive download URL")?;
    url.query_pairs_mut().append_pair("alt", "media");
    if let Some(api_key) = config.api_key.as_deref() {
        url.query_pairs_mut().append_pair("key", api_key);
    }

    let mut request = client.get(url);
    if let Some(token) = config.access_token.as_deref() {
        request = request.bearer_auth(token);
    }

    let mut response = request
        .send()
        .with_context(|| format!("failed to download file {file_id}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        let hint = if config.access_token.is_some() {
            "check that the token can read this file"
        } else {
            "if the file is not public, add REPLAYCORE_GOOGLE_DRIVE_ACCESS_TOKEN or refresh token settings"
        };
        anyhow::bail!(
            "Google Drive download failed for {file_id}: {status}. {hint}. response: {body}"
        );
    }

    let mut output = fs::File::create(cache_path)
        .with_context(|| format!("failed to create cache file {}", cache_path.display()))?;
    response
        .copy_to(&mut output)
        .with_context(|| format!("failed to write cache file {}", cache_path.display()))?;

    Ok(())
}

fn resolve_access_token(client: &Client, config: &GoogleDriveConfig) -> Result<Option<String>> {
    if config.access_token.is_some() {
        return Ok(config.access_token.clone());
    }

    let Some(refresh_token) = config.refresh_token.as_deref() else {
        return Ok(None);
    };
    let client_id = config
        .client_id
        .as_deref()
        .context("REPLAYCORE_GOOGLE_DRIVE_CLIENT_ID is required when using refresh token")?;
    let client_secret = config
        .client_secret
        .as_deref()
        .context("REPLAYCORE_GOOGLE_DRIVE_CLIENT_SECRET is required when using refresh token")?;

    let response = client
        .post("https://oauth2.googleapis.com/token")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!(
            "client_id={}&client_secret={}&refresh_token={}&grant_type=refresh_token",
            urlencoding::encode(client_id),
            urlencoding::encode(client_secret),
            urlencoding::encode(refresh_token),
        ))
        .send()
        .context("failed to refresh Google Drive access token")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        anyhow::bail!(
            "Google Drive token refresh failed: {status}. response: {body}"
        );
    }

    let body = response
        .text()
        .context("failed to read Google Drive token refresh response")?;
    let payload: OAuthTokenResponse = serde_json::from_str(&body)
        .context("failed to decode Google Drive token refresh response")?;

    Ok(Some(payload.access_token))
}

#[derive(Debug, Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
}

fn is_supported_audio_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".mp3")
        || lower.ends_with(".flac")
        || lower.ends_with(".wav")
        || lower.ends_with(".m4a")
        || lower.ends_with(".aac")
        || lower.ends_with(".ogg")
        || lower.ends_with(".oga")
        || lower.ends_with(".opus")
        || lower.ends_with(".webm")
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|ch| match ch {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => ch,
        })
        .collect()
}
