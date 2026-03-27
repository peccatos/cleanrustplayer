// Build either local-only or database-backed application state.
use std::convert::TryFrom;
use std::env;
use std::future::Future;
use std::path::PathBuf;
use std::sync::OnceLock;

use anyhow::{Context, Result};

use crate::contract::{
    local_path_string, local_track_identity, local_track_location_id, AvailabilityState,
    CatalogIndex, OwnershipScope, StorageKind, TrackLocationRecord, TrackMetadata, TrackRecord,
};
use crate::music::library::{default_music_dir, load_music_library, Track};
use crate::provider::bandcamp::BandcampProvider;
use crate::provider::registry::ProviderRegistry;
use crate::provider::ProviderHttpConfig;
use crate::provider::ProviderKind;
use crate::provider_accounts::{ProviderAccountSummary, ProviderAccountWrite};
use crate::repository::{CatalogProjection, ProjectionDefaults, SqlxRepository};
use crate::search::SearchService;
use crate::service::source_records_for_context;
use crate::token_vault::TokenVault;

#[derive(Clone)]
pub struct AppContext {
    pub user_id: String,
    pub local_music_roots: Vec<PathBuf>,
    pub tracks: Vec<Track>,
    pub catalog: CatalogIndex,
    pub saved_track_ids: Vec<String>,
    pub hidden_track_ids: Vec<String>,
    pub search_service: SearchService,
    pub volume_step: f64,
    pub cache_enabled: bool,
    pub repository: Option<SqlxRepository>,
    pub token_vault: Option<TokenVault>,
}

impl AppContext {
    pub fn bootstrap() -> Result<Self> {
        Self::bootstrap_local()
    }

    pub fn bootstrap_local() -> Result<Self> {
        dotenvy::dotenv().ok();
        let user_id = configured_user_id();
        let bandcamp_enabled = configured_bandcamp_enabled();
        let defaults = ProjectionDefaults {
            local_music_roots: configured_music_roots(),
            volume_step: configured_volume_step(),
            cache_enabled: configured_cache_enabled(),
        };

        let mut registry = ProviderRegistry::new();
        if bandcamp_enabled {
            registry.register(BandcampProvider::new(ProviderHttpConfig::default())?);
        }

        let search_service = SearchService::new(registry.clone());

        Ok(Self {
            user_id,
            local_music_roots: defaults.local_music_roots,
            tracks: Vec::new(),
            catalog: build_catalog(&[], bandcamp_enabled),
            saved_track_ids: Vec::new(),
            hidden_track_ids: Vec::new(),
            search_service,
            volume_step: defaults.volume_step,
            cache_enabled: defaults.cache_enabled,
            repository: None,
            token_vault: None,
        })
    }

    pub fn bootstrap_database() -> Result<Self> {
        dotenvy::dotenv().ok();
        let user_id = configured_user_id();
        let bandcamp_enabled = configured_bandcamp_enabled();
        let defaults = ProjectionDefaults {
            local_music_roots: configured_music_roots(),
            volume_step: configured_volume_step(),
            cache_enabled: configured_cache_enabled(),
        };
        let repository = block_on_database(SqlxRepository::connect_from_env())?
            .context("database mode requires a reachable Postgres instance")?;
        let token_vault = TokenVault::from_env()?;

        let mut registry = ProviderRegistry::new();
        if bandcamp_enabled {
            registry.register(BandcampProvider::new(ProviderHttpConfig::default())?);
        }

        let search_service = SearchService::new(registry.clone());
        block_on_database(repository.migrate())?;
        block_on_database(repository.seed_provider_definitions(&source_records_for_context(
            bandcamp_enabled,
        )))?;
        let projection = block_on_database(repository.load_projection(&user_id, defaults))?;

        Ok(Self {
            user_id,
            local_music_roots: projection.local_music_roots,
            tracks: projection.tracks,
            catalog: projection.catalog,
            saved_track_ids: projection.saved_track_ids,
            hidden_track_ids: projection.hidden_track_ids,
            search_service,
            volume_step: projection.volume_step,
            cache_enabled: projection.cache_enabled,
            repository: Some(repository),
            token_vault,
        })
    }

    pub fn reload_local_library(&mut self) -> Result<usize> {
        let mut tracks = collect_tracks(&self.local_music_roots)?;
        sort_tracks(&mut tracks);

        self.tracks = tracks;
        self.catalog = build_catalog_with_sources(
            &self.tracks,
            if self.catalog.sources.is_empty() {
                let bandcamp_enabled = self
                    .search_service
                    .registry()
                    .find(ProviderKind::Bandcamp)
                    .is_some();
                source_records_for_context(bandcamp_enabled)
            } else {
                self.catalog.sources.clone()
            },
        );

        if let Some(repository) = self.repository.clone() {
            let projection = CatalogProjection {
                tracks: self.tracks.clone(),
                catalog: self.catalog.clone(),
                saved_track_ids: self.saved_track_ids.clone(),
                hidden_track_ids: self.hidden_track_ids.clone(),
                local_music_roots: self.local_music_roots.clone(),
                volume_step: self.volume_step,
                cache_enabled: self.cache_enabled,
            };
            block_on_database(repository.save_projection(&self.user_id, &projection))?;
        }

        Ok(self.tracks.len())
    }

    pub fn set_local_music_roots(&mut self, roots: Vec<PathBuf>) -> Result<usize> {
        self.local_music_roots = roots;
        self.reload_local_library()
    }

    pub fn provider_accounts_snapshot(&self) -> Result<Vec<ProviderAccountSummary>> {
        if let Some(repository) = self.repository.clone() {
            return block_on_database(repository.load_provider_accounts(&self.user_id));
        }

        Ok(self
            .catalog
            .sources
            .iter()
            .map(ProviderAccountSummary::from_source)
            .collect())
    }

    pub fn upsert_provider_account(
        &self,
        provider_id: &str,
        input: ProviderAccountWrite,
    ) -> Result<ProviderAccountSummary> {
        let repository = self
            .repository
            .clone()
            .context("database is not configured")?;
        let token_vault = self
            .token_vault
            .as_ref()
            .context("token encryption key is not configured")?;

        block_on_database(repository.upsert_provider_account(
            &self.user_id,
            provider_id,
            &input,
            token_vault,
        ))
    }

    pub fn clear_provider_account(&self, provider_id: &str) -> Result<ProviderAccountSummary> {
        let repository = self
            .repository
            .clone()
            .context("database is not configured")?;

        block_on_database(repository.clear_provider_account(&self.user_id, provider_id))
    }
}

pub fn build_catalog(
    tracks: &[Track],
    bandcamp_enabled: bool,
) -> CatalogIndex {
    build_catalog_with_sources(tracks, source_records_for_context(bandcamp_enabled))
}

pub fn build_catalog_with_sources(
    tracks: &[Track],
    sources: Vec<crate::contract::SourceRecord>,
) -> CatalogIndex {
    let mut catalog_tracks = Vec::with_capacity(tracks.len());
    let mut locations = Vec::with_capacity(tracks.len());

    for track in tracks {
        let source_id = "local_import";
        let source_track_id = local_path_string(&track.path);
        let title = track
            .title
            .clone()
            .unwrap_or_else(|| track.file_name.clone());
        let artist = track
            .artist
            .clone()
            .unwrap_or_else(|| "<unknown>".to_string());
        let album = track
            .album
            .clone()
            .unwrap_or_else(|| "<unknown>".to_string());
        let duration_ms = track
            .duration
            .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX));
        let fingerprint_seed = format!(
            "{}|{}|{}|{}",
            source_track_id,
            title,
            artist,
            duration_ms.unwrap_or_default()
        );
        let identity = local_track_identity(source_id, &source_track_id, &fingerprint_seed);
        let location_id = local_track_location_id(source_id, &source_track_id);

        let metadata = TrackMetadata {
            title,
            artist,
            album,
            album_artist: track.artist.clone(),
            genre: None,
            track_number: None,
            disc_number: None,
            year: None,
            duration_ms,
        };

        catalog_tracks.push(TrackRecord::new(
            identity.clone(),
            metadata,
            OwnershipScope::UserOwned,
            AvailabilityState::Available,
            Some(location_id.clone()),
        ));

        locations.push(TrackLocationRecord::new(
            location_id,
            identity.track_id,
            source_id,
            StorageKind::LocalFile,
            Some(track.path.display().to_string()),
            None,
            AvailabilityState::Available,
        ));
    }

    CatalogIndex::new(catalog_tracks, sources, locations)
}

fn configured_user_id() -> String {
    env::var("REPLAYCORE_USER_ID")
        .or_else(|_| env::var("USERNAME"))
        .or_else(|_| env::var("USER"))
        .unwrap_or_else(|_| "local-user".to_string())
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

fn configured_volume_step() -> f64 {
    env::var("REPLAYCORE_VOLUME_STEP")
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| *value > 0.0)
        .unwrap_or(0.05)
}

fn configured_cache_enabled() -> bool {
    env::var("REPLAYCORE_CACHE_ENABLED")
        .ok()
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn configured_bandcamp_enabled() -> bool {
    env::var("REPLAYCORE_BANDCAMP_ENABLED")
        .ok()
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(true)
}

fn collect_tracks(roots: &[PathBuf]) -> Result<Vec<Track>> {
    let mut tracks = Vec::new();

    for root in roots {
        let mut root_tracks = load_music_library(root)
            .with_context(|| format!("failed to import local library from {}", root.display()))?;
        tracks.append(&mut root_tracks);
    }

    Ok(tracks)
}

fn sort_tracks(tracks: &mut [Track]) {
    tracks.sort_by(|a, b| {
        a.artist
            .as_deref()
            .unwrap_or("")
            .cmp(b.artist.as_deref().unwrap_or(""))
            .then_with(|| {
                a.title
                    .as_deref()
                    .unwrap_or("")
                    .cmp(b.title.as_deref().unwrap_or(""))
            })
            .then_with(|| a.file_name.cmp(&b.file_name))
    });
}

fn block_on_database<T, F>(future: F) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    static DATABASE_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

    let runtime = DATABASE_RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("failed to build database runtime")
    });

    runtime.block_on(future)
}
