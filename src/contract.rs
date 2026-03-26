use std::path::PathBuf;

use blake3::Hash;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackIdentity {
    pub track_id: String,
    pub source_id: String,
    pub source_track_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_number: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc_number: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackRecord {
    pub identity: TrackIdentity,
    pub metadata: TrackMetadata,
    pub ownership_scope: OwnershipScope,
    pub availability: AvailabilityState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_location_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceCapabilities {
    pub scan: bool,
    pub stream: bool,
    pub download: bool,
    pub sync: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceRecord {
    pub id: String,
    pub kind: SourceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<SourceCapabilities>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackLocationRecord {
    pub id: String,
    pub track_id: String,
    pub source_id: String,
    pub storage_kind: StorageKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub availability: AvailabilityState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogIndex {
    pub tracks: Vec<TrackRecord>,
    pub sources: Vec<SourceRecord>,
    pub locations: Vec<TrackLocationRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserLibrary {
    pub user_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub saved_track_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hidden_track_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybackState {
    pub status: PlaybackStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_track_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_location_id: Option<String>,
    pub position_ms: u64,
    pub volume: f64,
    pub muted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueEntry {
    pub id: String,
    pub track_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueState {
    pub entries: Vec<QueueEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_index: Option<usize>,
    pub shuffle: bool,
    pub repeat: QueueRepeatMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enabled_source_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub local_music_roots: Vec<String>,
    pub volume_step: f64,
    pub cache_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayCoreContract {
    pub catalog: CatalogIndex,
    pub user_library: UserLibrary,
    pub playback: PlaybackState,
    pub queue: QueueState,
    pub settings: Settings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandEnvelope<T> {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<CommandError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnershipScope {
    UserOwned,
    Shared,
    ExternalCatalog,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AvailabilityState {
    Available,
    Unavailable,
    Restricted,
    PendingSync,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageKind {
    LocalFile,
    RemoteStream,
    RemoteFile,
    CachedFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    LocalDisk,
    StreamingService,
    CloudStorage,
    SharedLibrary,
    RemoteCatalog,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackStatus {
    Stopped,
    Playing,
    Paused,
    Buffering,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueRepeatMode {
    Off,
    One,
    All,
}

impl TrackIdentity {
    pub fn new(
        track_id: impl Into<String>,
        source_id: impl Into<String>,
        source_track_id: impl Into<String>,
        fingerprint: Option<String>,
    ) -> Self {
        Self {
            track_id: track_id.into(),
            source_id: source_id.into(),
            source_track_id: source_track_id.into(),
            fingerprint,
        }
    }
}

impl TrackMetadata {
    pub fn new(
        title: impl Into<String>,
        artist: impl Into<String>,
        album: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            artist: artist.into(),
            album: album.into(),
            album_artist: None,
            genre: None,
            track_number: None,
            disc_number: None,
            year: None,
            duration_ms: None,
        }
    }
}

impl TrackRecord {
    pub fn new(
        identity: TrackIdentity,
        metadata: TrackMetadata,
        ownership_scope: OwnershipScope,
        availability: AvailabilityState,
        preferred_location_id: Option<String>,
    ) -> Self {
        Self {
            identity,
            metadata,
            ownership_scope,
            availability,
            preferred_location_id,
        }
    }
}

impl SourceCapabilities {
    pub fn new(scan: bool, stream: bool, download: bool, sync: bool) -> Self {
        Self {
            scan,
            stream,
            download,
            sync,
        }
    }
}

impl SourceRecord {
    pub fn new(
        id: impl Into<String>,
        kind: SourceKind,
        name: impl Into<String>,
        enabled: bool,
        priority: i64,
        capabilities: SourceCapabilities,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            name: Some(name.into()),
            enabled,
            priority: Some(priority),
            capabilities: Some(capabilities),
        }
    }

    pub fn local_import(enabled: bool) -> Self {
        Self::new(
            "local_import",
            SourceKind::LocalDisk,
            "Local import",
            enabled,
            0,
            SourceCapabilities::new(true, true, false, true),
        )
    }

    pub fn bandcamp(enabled: bool) -> Self {
        Self::new(
            "bandcamp",
            SourceKind::RemoteCatalog,
            "Bandcamp",
            enabled,
            10,
            SourceCapabilities::new(false, true, false, false),
        )
    }

    pub fn youtube(enabled: bool) -> Self {
        Self::new(
            "youtube",
            SourceKind::StreamingService,
            "YouTube",
            enabled,
            20,
            SourceCapabilities::new(false, true, false, false),
        )
    }

    pub fn spotify(enabled: bool) -> Self {
        Self::new(
            "spotify",
            SourceKind::StreamingService,
            "Spotify",
            enabled,
            30,
            SourceCapabilities::new(false, true, false, false),
        )
    }

    pub fn apple_music(enabled: bool) -> Self {
        Self::new(
            "apple_music",
            SourceKind::StreamingService,
            "Apple Music",
            enabled,
            40,
            SourceCapabilities::new(false, true, false, false),
        )
    }

    pub fn soundcloud(enabled: bool) -> Self {
        Self::new(
            "soundcloud",
            SourceKind::StreamingService,
            "SoundCloud",
            enabled,
            50,
            SourceCapabilities::new(false, true, false, false),
        )
    }

    pub fn remote_catalog(enabled: bool) -> Self {
        Self::new(
            "remote_catalog",
            SourceKind::RemoteCatalog,
            "Remote catalog",
            enabled,
            60,
            SourceCapabilities::new(false, true, false, false),
        )
    }
}

impl TrackLocationRecord {
    pub fn new(
        id: impl Into<String>,
        track_id: impl Into<String>,
        source_id: impl Into<String>,
        storage_kind: StorageKind,
        path: Option<String>,
        url: Option<String>,
        availability: AvailabilityState,
    ) -> Self {
        Self {
            id: id.into(),
            track_id: track_id.into(),
            source_id: source_id.into(),
            storage_kind,
            path,
            url,
            availability,
        }
    }
}

impl CatalogIndex {
    pub fn new(
        tracks: Vec<TrackRecord>,
        sources: Vec<SourceRecord>,
        locations: Vec<TrackLocationRecord>,
    ) -> Self {
        let hash_payload = serde_json::json!({
            "tracks": tracks,
            "sources": sources,
            "locations": locations,
        });
        let catalog_hash = Some(format!("blake3:{}", hash_json(&hash_payload)));

        Self {
            tracks,
            sources,
            locations,
            catalog_hash,
        }
    }
}

impl UserLibrary {
    pub fn new(user_id: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
            saved_track_ids: Vec::new(),
            hidden_track_ids: Vec::new(),
        }
    }
}

impl PlaybackState {
    pub fn stopped(volume: f64) -> Self {
        Self {
            status: PlaybackStatus::Stopped,
            current_track_id: None,
            current_location_id: None,
            position_ms: 0,
            volume,
            muted: false,
        }
    }
}

impl QueueState {
    pub fn new(
        entries: Vec<QueueEntry>,
        current_index: Option<usize>,
        shuffle: bool,
        repeat: QueueRepeatMode,
    ) -> Self {
        Self {
            entries,
            current_index,
            shuffle,
            repeat,
        }
    }
}

impl Settings {
    pub fn new(
        enabled_source_ids: Vec<String>,
        local_music_roots: Vec<String>,
        volume_step: f64,
        cache_enabled: bool,
    ) -> Self {
        Self {
            enabled_source_ids,
            local_music_roots,
            volume_step,
            cache_enabled,
        }
    }
}

impl<T> CommandEnvelope<T> {
    pub fn ok(data: T) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(CommandError {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}

fn hash_json(value: &serde_json::Value) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    let hash: Hash = blake3::hash(&bytes);
    hash.to_hex().to_string()
}

pub fn stable_id(prefix: &str, parts: &[&str]) -> String {
    let payload = parts.join("|");
    format!(
        "{}_{}",
        prefix,
        hash_json(&serde_json::json!({ "value": payload }))
    )
}

pub fn local_track_identity(
    source_id: &str,
    source_track_id: &str,
    fingerprint_seed: &str,
) -> TrackIdentity {
    let track_id = stable_id("track", &[source_id, source_track_id]);
    let fingerprint = Some(stable_id(
        "fingerprint",
        &[source_id, source_track_id, fingerprint_seed],
    ));

    TrackIdentity::new(track_id, source_id, source_track_id, fingerprint)
}

pub fn local_track_location_id(source_id: &str, source_track_id: &str) -> String {
    stable_id("location", &[source_id, source_track_id])
}

pub fn local_path_string(path: &PathBuf) -> String {
    path.display().to_string()
}
