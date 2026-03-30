use std::collections::HashSet;
use std::convert::Infallible;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result};
use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::header::{self, HeaderValue};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Row, SqlitePool};
use tokio::net::TcpListener;

use crate::config::AppConfig;
use crate::music::library::{scan_music_dir, Track};

const INDEX_HTML: &str = include_str!("../web/index.html");
const APP_JS: &str = include_str!("../web/app.js");
const STYLES_CSS: &str = include_str!("../web/styles.css");

type Body = Full<Bytes>;

pub async fn serve(config: AppConfig) -> Result<()> {
    let state = Arc::new(AppState::bootstrap(config).await?);
    let initial_sync = state.sync_library().await?;
    let addr = format!("{}:{}", state.host, state.port);
    let listener = TcpListener::bind(&addr)
        .await
        .with_context(|| format!("failed to bind http server on {}", addr))?;

    println!(
        "ReplayCore web server listening at http://{} ({} tracks from {} root(s))",
        addr, initial_sync.track_count, initial_sync.root_count
    );

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let state = Arc::clone(&state);

        tokio::spawn(async move {
            let service = service_fn(move |request| handle_request(request, Arc::clone(&state)));
            if let Err(error) = http1::Builder::new().serve_connection(io, service).await {
                eprintln!("http connection error: {error}");
            }
        });
    }
}

struct AppState {
    host: String,
    port: u16,
    local_music_roots: Vec<PathBuf>,
    pool: SqlitePool,
}

#[derive(Debug, Clone, Serialize)]
struct HealthResponse {
    ok: bool,
    roots: Vec<String>,
    track_count: i64,
}

#[derive(Debug, Clone, Serialize)]
struct SyncResponse {
    track_count: usize,
    root_count: usize,
}

#[derive(Debug, Clone, Serialize)]
struct TrackListResponse {
    tracks: Vec<ApiTrack>,
}

#[derive(Debug, Clone, Serialize)]
struct TrackResponse {
    track: ApiTrack,
}

#[derive(Debug, Clone, Serialize)]
struct ApiError {
    code: &'static str,
    title: &'static str,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
struct ErrorResponse {
    error: ApiError,
}

#[derive(Debug, Clone, Serialize)]
struct ApiTrack {
    id: String,
    title: String,
    artist: String,
    album: String,
    duration_ms: Option<i64>,
    mime_type: String,
    source_label: String,
    stream_url: String,
}

#[derive(Debug, Clone)]
struct StoredTrack {
    id: String,
    path: String,
    file_name: String,
    title: String,
    artist: String,
    album: String,
    duration_ms: Option<i64>,
    mime_type: String,
}

#[derive(Debug, Clone)]
struct DiscoveredTrack {
    id: String,
    path: String,
    file_name: String,
    title: String,
    artist: String,
    album: String,
    duration_ms: Option<i64>,
    mime_type: String,
    size_bytes: i64,
    modified_unix_ms: i64,
}

#[derive(Debug, Clone, Copy)]
struct ByteRange {
    start: u64,
    end: u64,
}

impl AppState {
    async fn bootstrap(config: AppConfig) -> Result<Self> {
        let sqlite_path = config.web.sqlite_path.clone();
        if let Some(parent) = sqlite_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("failed to create sqlite dir {}", parent.display()))?;
        }

        let options = SqliteConnectOptions::new()
            .filename(&sqlite_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .with_context(|| format!("failed to open sqlite db {}", sqlite_path.display()))?;

        initialize_schema(&pool).await?;

        Ok(Self {
            host: config.web.host,
            port: config.web.port,
            local_music_roots: config.local_music_roots,
            pool,
        })
    }

    async fn sync_library(&self) -> Result<SyncResponse> {
        let roots = self.local_music_roots.clone();
        let discovered = tokio::task::spawn_blocking(move || discover_tracks(roots))
            .await
            .context("music scan task panicked")??;

        let mut tx = self.pool.begin().await?;
        let mut seen_ids = HashSet::with_capacity(discovered.len());

        for track in &discovered {
            seen_ids.insert(track.id.clone());

            sqlx::query(
                r#"
                INSERT INTO library_tracks (
                    id, path, file_name, title, artist, album, duration_ms, mime_type, size_bytes, modified_unix_ms
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ON CONFLICT(id) DO UPDATE SET
                    path = excluded.path,
                    file_name = excluded.file_name,
                    title = excluded.title,
                    artist = excluded.artist,
                    album = excluded.album,
                    duration_ms = excluded.duration_ms,
                    mime_type = excluded.mime_type,
                    size_bytes = excluded.size_bytes,
                    modified_unix_ms = excluded.modified_unix_ms,
                    updated_at = CURRENT_TIMESTAMP
                "#,
            )
            .bind(&track.id)
            .bind(&track.path)
            .bind(&track.file_name)
            .bind(&track.title)
            .bind(&track.artist)
            .bind(&track.album)
            .bind(track.duration_ms)
            .bind(&track.mime_type)
            .bind(track.size_bytes)
            .bind(track.modified_unix_ms)
            .execute(&mut *tx)
            .await?;
        }

        let existing_ids: Vec<String> = sqlx::query_scalar("SELECT id FROM library_tracks")
            .fetch_all(&mut *tx)
            .await?;

        for id in existing_ids {
            if !seen_ids.contains(&id) {
                sqlx::query("DELETE FROM library_tracks WHERE id = ?1")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
        }

        tx.commit().await?;

        Ok(SyncResponse {
            track_count: discovered.len(),
            root_count: self.local_music_roots.len(),
        })
    }

    async fn health(&self) -> Result<HealthResponse> {
        let track_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM library_tracks")
            .fetch_one(&self.pool)
            .await?;

        Ok(HealthResponse {
            ok: true,
            roots: self
                .local_music_roots
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            track_count,
        })
    }

    async fn list_tracks(&self) -> Result<Vec<ApiTrack>> {
        let rows = sqlx::query(
            r#"
            SELECT id, path, file_name, title, artist, album, duration_ms, mime_type
            FROM library_tracks
            ORDER BY LOWER(COALESCE(artist, '')), LOWER(COALESCE(title, '')), LOWER(file_name), id
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(row_to_track)
            .map(stored_track_to_api)
            .collect())
    }

    async fn get_track(&self, track_id: &str) -> Result<Option<StoredTrack>> {
        let row = sqlx::query(
            r#"
            SELECT id, path, file_name, title, artist, album, duration_ms, mime_type
            FROM library_tracks
            WHERE id = ?1
            "#,
        )
        .bind(track_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(row_to_track))
    }
}

async fn handle_request(
    request: Request<Incoming>,
    state: Arc<AppState>,
) -> Result<Response<Body>, Infallible> {
    let response = match dispatch_request(request, state).await {
        Ok(response) => response,
        Err(error) => json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            "Internal error",
            error.to_string(),
        ),
    };

    Ok(response)
}

async fn dispatch_request(request: Request<Incoming>, state: Arc<AppState>) -> Result<Response<Body>> {
    let path = request.uri().path().to_string();
    let method = request.method().clone();

    match (method, path.as_str()) {
        (Method::GET, "/") => Ok(html_response(INDEX_HTML)),
        (Method::GET, "/app.js") => Ok(js_response(APP_JS)),
        (Method::GET, "/styles.css") => Ok(css_response(STYLES_CSS)),
        (Method::GET, "/api/health") => Ok(json_response(StatusCode::OK, &state.health().await?)),
        (Method::GET, "/api/tracks") => {
            let tracks = state.list_tracks().await?;
            Ok(json_response(StatusCode::OK, &TrackListResponse { tracks }))
        }
        (Method::POST, "/api/library/rescan") => {
            let sync = state.sync_library().await?;
            Ok(json_response(StatusCode::OK, &sync))
        }
        _ if path.starts_with("/api/tracks/") && path.ends_with("/stream") && request.method() == Method::GET => {
            handle_stream(request, state, &path).await
        }
        _ if path.starts_with("/api/tracks/") && request.method() == Method::GET => {
            handle_track_details(state, &path).await
        }
        _ => Ok(not_found()),
    }
}

async fn handle_track_details(state: Arc<AppState>, path: &str) -> Result<Response<Body>> {
    let track_id = path.trim_start_matches("/api/tracks/");
    if track_id.is_empty() || track_id.contains('/') {
        return Ok(not_found());
    }

    let Some(track) = state.get_track(track_id).await? else {
        return Ok(json_error(
            StatusCode::NOT_FOUND,
            "TRACK_NOT_FOUND",
            "Track not found",
            "В библиотеке нет трека с таким id.".to_string(),
        ));
    };

    Ok(json_response(
        StatusCode::OK,
        &TrackResponse {
            track: stored_track_to_api(track),
        },
    ))
}

async fn handle_stream(
    request: Request<Incoming>,
    state: Arc<AppState>,
    path: &str,
) -> Result<Response<Body>> {
    let Some(track_id) = path
        .strip_prefix("/api/tracks/")
        .and_then(|value| value.strip_suffix("/stream"))
        .filter(|value| !value.is_empty() && !value.contains('/'))
    else {
        return Ok(not_found());
    };

    let Some(track) = state.get_track(track_id).await? else {
        return Ok(json_error(
            StatusCode::NOT_FOUND,
            "TRACK_NOT_FOUND",
            "Track not found",
            "В библиотеке нет трека с таким id.".to_string(),
        ));
    };

    let path = PathBuf::from(&track.path);
    let metadata = match tokio::fs::metadata(&path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(json_error(
                StatusCode::NOT_FOUND,
                "TRACK_FILE_MISSING",
                "Track file missing",
                "Файл пропал с диска. Пересканируй библиотеку.".to_string(),
            ));
        }
        Err(error) => {
            return Err(error).with_context(|| format!("failed to stat {}", path.display()));
        }
    };

    let file_size = metadata.len();
    let range = match parse_range(request.headers().get(header::RANGE), file_size) {
        Ok(range) => range,
        Err(_) => return Ok(range_not_satisfiable(file_size)),
    };
    let bytes = read_file_range(path.clone(), range).await?;

    let mut builder = Response::builder()
        .header(header::CONTENT_TYPE, track.mime_type)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CACHE_CONTROL, "no-store");

    if let Some(range) = range {
        let content_range = format!("bytes {}-{}/{}", range.start, range.end, file_size);
        builder = builder
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_RANGE, content_range)
            .header(header::CONTENT_LENGTH, bytes.len().to_string());
    } else {
        builder = builder
            .status(StatusCode::OK)
            .header(header::CONTENT_LENGTH, file_size.to_string());
    }

    Ok(builder
        .body(Full::new(Bytes::from(bytes)))
        .unwrap_or_else(|_| internal_server_error("failed to build stream response")))
}

async fn initialize_schema(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS library_tracks (
            id TEXT PRIMARY KEY,
            path TEXT NOT NULL UNIQUE,
            file_name TEXT NOT NULL,
            title TEXT NOT NULL,
            artist TEXT NOT NULL,
            album TEXT NOT NULL,
            duration_ms INTEGER NULL,
            mime_type TEXT NOT NULL,
            size_bytes INTEGER NOT NULL,
            modified_unix_ms INTEGER NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

fn discover_tracks(roots: Vec<PathBuf>) -> Result<Vec<DiscoveredTrack>> {
    let mut discovered = Vec::new();

    for root in roots {
        if !root.exists() {
            continue;
        }

        let tracks = scan_music_dir(&root)
            .with_context(|| format!("failed to scan music root {}", root.display()))?;

        for track in tracks {
            discovered.push(track_to_discovered(track)?);
        }
    }

    discovered.sort_by(|left, right| {
        left.artist
            .cmp(&right.artist)
            .then_with(|| left.title.cmp(&right.title))
            .then_with(|| left.file_name.cmp(&right.file_name))
    });

    Ok(discovered)
}

fn track_to_discovered(track: Track) -> Result<DiscoveredTrack> {
    let canonical_path = track
        .path
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", track.path.display()))?;
    let metadata = std::fs::metadata(&canonical_path)
        .with_context(|| format!("failed to read metadata for {}", canonical_path.display()))?;

    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default();

    let path_string = clean_windows_path(&canonical_path);
    let id = blake3::hash(path_string.as_bytes()).to_hex().to_string();

    Ok(DiscoveredTrack {
        id,
        path: path_string,
        file_name: track.file_name,
        title: track.title.unwrap_or_default(),
        artist: track.artist.unwrap_or_default(),
        album: track.album.unwrap_or_default(),
        duration_ms: track.duration.map(|duration| duration.as_millis() as i64),
        mime_type: mime_for_path(&canonical_path).to_string(),
        size_bytes: metadata.len() as i64,
        modified_unix_ms: modified,
    })
}

fn row_to_track(row: sqlx::sqlite::SqliteRow) -> StoredTrack {
    StoredTrack {
        id: row.get("id"),
        path: row.get("path"),
        file_name: row.get("file_name"),
        title: row.get("title"),
        artist: row.get("artist"),
        album: row.get("album"),
        duration_ms: row.get("duration_ms"),
        mime_type: row.get("mime_type"),
    }
}

fn stored_track_to_api(track: StoredTrack) -> ApiTrack {
    let title = if track.title.trim().is_empty() {
        track.file_name.clone()
    } else {
        track.title.clone()
    };

    ApiTrack {
        id: track.id.clone(),
        title,
        artist: track.artist,
        album: track.album,
        duration_ms: track.duration_ms,
        mime_type: track.mime_type,
        source_label: track.file_name,
        stream_url: format!("/api/tracks/{}/stream", track.id),
    }
}

fn parse_range(header_value: Option<&HeaderValue>, file_size: u64) -> Result<Option<ByteRange>> {
    let Some(header_value) = header_value else {
        return Ok(None);
    };

    let value = header_value
        .to_str()
        .context("invalid range header encoding")?;

    if !value.starts_with("bytes=") {
        anyhow::bail!("unsupported range unit");
    }

    let range_value = value.trim_start_matches("bytes=");
    let (start_raw, end_raw) = range_value
        .split_once('-')
        .context("invalid range header format")?;

    if start_raw.is_empty() && end_raw.is_empty() {
        anyhow::bail!("empty range header");
    }

    let (start, end) = if start_raw.is_empty() {
        let suffix_len: u64 = end_raw.parse().context("invalid suffix range")?;
        let start = file_size.saturating_sub(suffix_len);
        (start, file_size.saturating_sub(1))
    } else {
        let start: u64 = start_raw.parse().context("invalid range start")?;
        let end = if end_raw.is_empty() {
            file_size.saturating_sub(1)
        } else {
            end_raw.parse().context("invalid range end")?
        };
        (start, end)
    };

    if start >= file_size || end >= file_size || start > end {
        anyhow::bail!("range outside file bounds");
    }

    Ok(Some(ByteRange { start, end }))
}

async fn read_file_range(path: PathBuf, range: Option<ByteRange>) -> Result<Vec<u8>> {
    tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        let mut file = File::open(&path)
            .with_context(|| format!("failed to open audio file {}", path.display()))?;

        let mut buffer = Vec::new();
        if let Some(range) = range {
            let byte_len = (range.end - range.start + 1) as usize;
            buffer.resize(byte_len, 0);
            file.seek(SeekFrom::Start(range.start))
                .with_context(|| format!("failed to seek {}", path.display()))?;
            file.read_exact(&mut buffer)
                .with_context(|| format!("failed to read range from {}", path.display()))?;
        } else {
            file.read_to_end(&mut buffer)
                .with_context(|| format!("failed to read {}", path.display()))?;
        }

        Ok(buffer)
    })
    .await
    .context("file read task panicked")?
}

fn mime_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("mp3") => "audio/mpeg",
        Some("flac") => "audio/flac",
        Some("wav") => "audio/wav",
        Some("m4a") => "audio/mp4",
        Some("ogg") => "audio/ogg",
        _ => "application/octet-stream",
    }
}

fn clean_windows_path(path: &Path) -> String {
    let raw = path.display().to_string();
    if let Some(cleaned) = raw.strip_prefix(r"\\?\") {
        return cleaned.to_string();
    }

    raw
}

fn json_response<T: Serialize>(status: StatusCode, payload: &T) -> Response<Body> {
    match serde_json::to_vec(payload) {
        Ok(body) => Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, "application/json; charset=utf-8")
            .header(header::CACHE_CONTROL, "no-store")
            .body(Full::new(Bytes::from(body)))
            .unwrap_or_else(|_| internal_server_error("failed to build json response")),
        Err(error) => internal_server_error(&format!("failed to serialize json: {error}")),
    }
}

fn json_error(status: StatusCode, code: &'static str, title: &'static str, message: String) -> Response<Body> {
    json_response(
        status,
        &ErrorResponse {
            error: ApiError { code, title, message },
        },
    )
}

fn html_response(body: &str) -> Response<Body> {
    text_response(StatusCode::OK, "text/html; charset=utf-8", body)
}

fn js_response(body: &str) -> Response<Body> {
    text_response(StatusCode::OK, "text/javascript; charset=utf-8", body)
}

fn css_response(body: &str) -> Response<Body> {
    text_response(StatusCode::OK, "text/css; charset=utf-8", body)
}

fn text_response(status: StatusCode, content_type: &'static str, body: &str) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "no-store")
        .body(Full::new(Bytes::copy_from_slice(body.as_bytes())))
        .unwrap_or_else(|_| internal_server_error("failed to build text response"))
}

fn not_found() -> Response<Body> {
    json_error(
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
        "Not found",
        "Такого route здесь нет.".to_string(),
    )
}

fn range_not_satisfiable(file_size: u64) -> Response<Body> {
    Response::builder()
        .status(StatusCode::RANGE_NOT_SATISFIABLE)
        .header(header::CONTENT_RANGE, format!("bytes */{}", file_size))
        .header(header::CACHE_CONTROL, "no-store")
        .body(Full::new(Bytes::new()))
        .unwrap_or_else(|_| internal_server_error("failed to build range error response"))
}

fn internal_server_error(message: &str) -> Response<Body> {
    json_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
        "Internal error",
        message.to_string(),
    )
}
