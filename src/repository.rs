// SQLx persistence for projections, provider definitions, and provider accounts.
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use sqlx::types::Json;
use sqlx::{Pool, Postgres};
use tokio::time::timeout;

use crate::contract::{
    CatalogIndex, SourceCapabilities, SourceRecord, TrackIdentity, TrackLocationRecord,
    TrackMetadata, TrackRecord,
};
use crate::music::library::Track;
use crate::provider_accounts::{ProviderAccountSummary, ProviderAccountWrite};
use crate::token_vault::TokenVault;

#[derive(Debug, Clone)]
pub struct ProjectionDefaults {
    pub local_music_roots: Vec<PathBuf>,
    pub volume_step: f64,
    pub cache_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct CatalogProjection {
    pub tracks: Vec<Track>,
    pub catalog: CatalogIndex,
    pub saved_track_ids: Vec<String>,
    pub hidden_track_ids: Vec<String>,
    pub local_music_roots: Vec<PathBuf>,
    pub volume_step: f64,
    pub cache_enabled: bool,
}

#[derive(Clone)]
pub struct SqlxRepository {
    pool: Pool<Postgres>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct SourceRow {
    id: String,
    kind: String,
    name: String,
    enabled: bool,
    priority: Option<i32>,
    scan: bool,
    stream: bool,
    download: bool,
    sync: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct CatalogTrackRow {
    track_id: String,
    primary_provider_id: String,
    primary_source_track_id: String,
    fingerprint: Option<String>,
    file_name: String,
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    album_artist: Option<String>,
    genre: Option<String>,
    track_number: Option<i32>,
    disc_number: Option<i32>,
    year: Option<i32>,
    duration_ms: Option<i64>,
    ownership_scope: String,
    availability: String,
    preferred_identifier_id: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct TrackIdentifierRow {
    id: String,
    track_id: String,
    provider_id: String,
    _identifier_kind: String,
    value: String,
    path: Option<String>,
    url: Option<String>,
    storage_kind: String,
    availability: String,
    is_preferred: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct UserSettingsRow {
    local_music_roots: Json<Vec<String>>,
    volume_step: f64,
    cache_enabled: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct UserLibraryItemRow {
    track_id: String,
    item_kind: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ProviderAccountSummaryRow {
    provider_id: String,
    provider_kind: String,
    provider_name: String,
    enabled: bool,
    priority: Option<i32>,
    external_account_id: Option<String>,
    scopes: Json<Vec<String>>,
    has_access_token: bool,
    has_refresh_token: bool,
    token_expires_at: Option<DateTime<Utc>>,
    status: String,
    settings: Json<Value>,
    scan: bool,
    stream: bool,
    download: bool,
    sync: bool,
}

impl SqlxRepository {
    pub async fn connect_from_env() -> Result<Option<Self>> {
        let url = match env::var("REPLAYCORE_DATABASE_URL").or_else(|_| env::var("DATABASE_URL")) {
            Ok(url) => url,
            Err(_) => return Ok(None),
        };

        let url = url.trim();
        if url.is_empty() {
            return Ok(None);
        }

        let connect_timeout = configured_database_connect_timeout();
        // Keep startup bounded so a dead Docker daemon does not stall the shell.
        let pool = match timeout(
            connect_timeout,
            PgPoolOptions::new()
                .max_connections(5)
                .acquire_timeout(connect_timeout)
                .connect(url),
        )
        .await
        {
            Ok(Ok(pool)) => pool,
            Ok(Err(_)) => return Ok(None),
            Err(_) => return Ok(None),
        };

        Ok(Some(Self { pool }))
    }

    pub async fn migrate(&self) -> Result<()> {
        sqlx::migrate!()
            .run(&self.pool)
            .await
            .context("failed to run SQLx migrations")?;
        Ok(())
    }

    pub async fn seed_provider_definitions(&self, sources: &[SourceRecord]) -> Result<()> {
        for source in sources {
            let capabilities = source
                .capabilities
                .clone()
                .unwrap_or_else(|| SourceCapabilities::new(false, false, false, false));

            sqlx::query(
                r#"
                INSERT INTO providers (id, kind, name, scan, stream, download, sync)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                ON CONFLICT (id) DO UPDATE
                SET kind = EXCLUDED.kind,
                    name = EXCLUDED.name,
                    scan = EXCLUDED.scan,
                    stream = EXCLUDED.stream,
                    download = EXCLUDED.download,
                    sync = EXCLUDED.sync,
                    updated_at = NOW()
                "#,
            )
            .bind(&source.id)
            .bind(enum_to_string(source.kind)?)
            .bind(source.name.clone().unwrap_or_else(|| source.id.clone()))
            .bind(capabilities.scan)
            .bind(capabilities.stream)
            .bind(capabilities.download)
            .bind(capabilities.sync)
            .execute(&self.pool)
            .await
            .with_context(|| format!("failed to seed provider {}", source.id))?;
        }

        Ok(())
    }

    pub async fn load_provider_accounts(
        &self,
        user_id: &str,
    ) -> Result<Vec<ProviderAccountSummary>> {
        let rows: Vec<ProviderAccountSummaryRow> = sqlx::query_as(
            r#"
            SELECT
                p.id AS provider_id,
                p.kind AS provider_kind,
                p.name AS provider_name,
                COALESCE(a.enabled, FALSE) AS enabled,
                a.priority,
                a.external_account_id,
                COALESCE(a.scopes, '[]'::jsonb) AS scopes,
                a.access_token_encrypted IS NOT NULL AS has_access_token,
                a.refresh_token_encrypted IS NOT NULL AS has_refresh_token,
                a.token_expires_at,
                COALESCE(a.status, CASE WHEN COALESCE(a.enabled, FALSE) THEN 'active' ELSE 'disabled' END) AS status,
                COALESCE(a.settings, '{}'::jsonb) AS settings,
                p.scan,
                p.stream,
                p.download,
                p.sync
            FROM providers p
            LEFT JOIN provider_accounts a
                ON a.provider_id = p.id
               AND a.user_id = $1
            ORDER BY COALESCE(a.priority, 2147483647), p.id
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .context("failed to load provider accounts")?;

        Ok(rows
            .into_iter()
            .map(provider_account_summary_from_row)
            .collect())
    }

    pub async fn upsert_provider_account(
        &self,
        user_id: &str,
        provider_id: &str,
        input: &ProviderAccountWrite,
        token_vault: &TokenVault,
    ) -> Result<ProviderAccountSummary> {
        let provider_exists: Option<(String,)> = sqlx::query_as(
            r#"
            SELECT id
            FROM providers
            WHERE id = $1
            "#,
        )
        .bind(provider_id)
        .fetch_optional(&self.pool)
        .await
        .context("failed to check provider existence")?;

        if provider_exists.is_none() {
            anyhow::bail!("unknown provider: {provider_id}");
        }

        let access_token_encrypted = match input.access_token.as_ref() {
            Some(access_token) => Some(
                token_vault
                    .encrypt(access_token)
                    .context("failed to encrypt access token")?,
            ),
            None => None,
        };
        let refresh_token_encrypted = match input.refresh_token.as_ref() {
            Some(refresh_token) => Some(
                token_vault
                    .encrypt(refresh_token)
                    .context("failed to encrypt refresh token")?,
            ),
            None => None,
        };
        let scopes = Json(input.scopes.clone());
        let status = input.status.clone().unwrap_or_else(|| {
            if input.enabled {
                "active".to_string()
            } else {
                "disabled".to_string()
            }
        });

        let mut tx = self
            .pool
            .begin()
            .await
            .context("failed to open provider account transaction")?;

        sqlx::query(
            r#"
            INSERT INTO provider_accounts (
                user_id,
                provider_id,
                enabled,
                priority,
                external_account_id,
                scopes,
                access_token_encrypted,
                refresh_token_encrypted,
                token_expires_at,
                status,
                settings
            )
            VALUES (
                $1,
                $2,
                $3,
                NULL,
                $4,
                $5,
                $6,
                $7,
                $8,
                $9,
                '{}'::jsonb
            )
            ON CONFLICT (user_id, provider_id) DO UPDATE
            SET enabled = EXCLUDED.enabled,
                external_account_id = EXCLUDED.external_account_id,
                scopes = EXCLUDED.scopes,
                access_token_encrypted = EXCLUDED.access_token_encrypted,
                refresh_token_encrypted = EXCLUDED.refresh_token_encrypted,
                token_expires_at = EXCLUDED.token_expires_at,
                status = EXCLUDED.status,
                updated_at = NOW()
            "#,
        )
        .bind(user_id)
        .bind(provider_id)
        .bind(input.enabled)
        .bind(&input.external_account_id)
        .bind(scopes)
        .bind(access_token_encrypted)
        .bind(refresh_token_encrypted)
        .bind(input.token_expires_at)
        .bind(status)
        .execute(&mut *tx)
        .await
        .with_context(|| format!("failed to upsert provider account {provider_id}"))?;

        tx.commit()
            .await
            .context("failed to commit provider account transaction")?;

        self.load_provider_accounts(user_id)
            .await
            .and_then(|accounts| {
                accounts
                    .into_iter()
                    .find(|account| account.provider_id == provider_id)
                    .ok_or_else(|| anyhow::anyhow!("failed to load provider account {provider_id}"))
            })
    }

    pub async fn clear_provider_account(
        &self,
        user_id: &str,
        provider_id: &str,
    ) -> Result<ProviderAccountSummary> {
        let provider_exists: Option<(String,)> = sqlx::query_as(
            r#"
            SELECT id
            FROM providers
            WHERE id = $1
            "#,
        )
        .bind(provider_id)
        .fetch_optional(&self.pool)
        .await
        .context("failed to check provider existence")?;

        if provider_exists.is_none() {
            anyhow::bail!("unknown provider: {provider_id}");
        }

        let mut tx = self
            .pool
            .begin()
            .await
            .context("failed to open provider account transaction")?;

        sqlx::query(
            r#"
            INSERT INTO provider_accounts (
                user_id,
                provider_id,
                enabled,
                priority,
                external_account_id,
                scopes,
                access_token_encrypted,
                refresh_token_encrypted,
                token_expires_at,
                status,
                settings
            )
            VALUES (
                $1,
                $2,
                FALSE,
                NULL,
                NULL,
                '[]'::jsonb,
                NULL,
                NULL,
                NULL,
                'disabled',
                '{}'::jsonb
            )
            ON CONFLICT (user_id, provider_id) DO UPDATE
            SET enabled = FALSE,
                external_account_id = NULL,
                scopes = '[]'::jsonb,
                access_token_encrypted = NULL,
                refresh_token_encrypted = NULL,
                token_expires_at = NULL,
                status = 'disabled',
                updated_at = NOW()
            "#,
        )
        .bind(user_id)
        .bind(provider_id)
        .execute(&mut *tx)
        .await
        .with_context(|| format!("failed to clear provider account {provider_id}"))?;

        tx.commit()
            .await
            .context("failed to commit provider account transaction")?;

        self.load_provider_accounts(user_id)
            .await
            .and_then(|accounts| {
                accounts
                    .into_iter()
                    .find(|account| account.provider_id == provider_id)
                    .ok_or_else(|| anyhow::anyhow!("failed to load provider account {provider_id}"))
            })
    }

    pub async fn load_projection(
        &self,
        user_id: &str,
        defaults: ProjectionDefaults,
    ) -> Result<CatalogProjection> {
        // Load persisted state first; local scans are only used when explicitly requested.
        let settings = self.load_settings(user_id).await?;
        let local_music_roots = settings
            .as_ref()
            .map(|row| row.local_music_roots.0.clone())
            .unwrap_or_else(|| {
                defaults
                    .local_music_roots
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect()
            });
        let volume_step = settings
            .as_ref()
            .map(|row| row.volume_step)
            .unwrap_or(defaults.volume_step);
        let cache_enabled = settings
            .as_ref()
            .map(|row| row.cache_enabled)
            .unwrap_or(defaults.cache_enabled);

        let source_rows: Vec<SourceRow> = sqlx::query_as(
            r#"
            SELECT
                p.id,
                p.kind,
                p.name,
                COALESCE(a.enabled, FALSE) AS enabled,
                a.priority,
                p.scan,
                p.stream,
                p.download,
                p.sync
            FROM providers p
            LEFT JOIN provider_accounts a
                ON a.provider_id = p.id
               AND a.user_id = $1
            ORDER BY COALESCE(a.priority, 2147483647), p.id
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .context("failed to load source rows")?;

        let catalog_track_rows: Vec<CatalogTrackRow> = sqlx::query_as(
            r#"
            SELECT
                id AS track_id,
                primary_provider_id,
                primary_source_track_id,
                fingerprint,
                file_name,
                title,
                artist,
                album,
                album_artist,
                genre,
                track_number,
                disc_number,
                year,
                duration_ms,
                ownership_scope,
                availability,
                preferred_identifier_id
            FROM catalog_tracks
            ORDER BY COALESCE(artist, ''), COALESCE(title, ''), file_name, track_id
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("failed to load catalog track rows")?;

        let identifier_rows: Vec<TrackIdentifierRow> = sqlx::query_as(
            r#"
            SELECT
                id,
                track_id,
                provider_id,
                identifier_kind AS _identifier_kind,
                value,
                path,
                url,
                storage_kind,
                availability,
                is_preferred
            FROM track_identifiers
            ORDER BY track_id, is_preferred DESC, id
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("failed to load track identifiers")?;

        let library_items: Vec<UserLibraryItemRow> = sqlx::query_as(
            r#"
            SELECT track_id, item_kind
            FROM user_library_items
            WHERE user_id = $1
            ORDER BY item_kind, track_id
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .context("failed to load user library items")?;

        let mut sources = Vec::with_capacity(source_rows.len());
        for row in source_rows {
            sources.push(SourceRecord {
                id: row.id,
                kind: parse_enum(&row.kind)?,
                name: Some(row.name),
                enabled: row.enabled,
                priority: row.priority.map(i64::from),
                capabilities: Some(SourceCapabilities::new(
                    row.scan,
                    row.stream,
                    row.download,
                    row.sync,
                )),
            });
        }

        let mut visible_track_ids = HashSet::new();
        let mut saved_track_ids = Vec::new();
        let mut hidden_track_ids = Vec::new();
        for item in &library_items {
            match item.item_kind.as_str() {
                "owned" | "saved" | "hidden" => {
                    visible_track_ids.insert(item.track_id.clone());
                }
                _ => {}
            }

            match item.item_kind.as_str() {
                "saved" => saved_track_ids.push(item.track_id.clone()),
                "hidden" => hidden_track_ids.push(item.track_id.clone()),
                _ => {}
            }
        }

        let identifier_lookup: HashMap<String, TrackIdentifierRow> = identifier_rows
            .iter()
            .cloned()
            .map(|row| (row.id.clone(), row))
            .collect();

        let mut identifiers_by_track: HashMap<String, Vec<TrackIdentifierRow>> = HashMap::new();
        for row in identifier_rows {
            if visible_track_ids.contains(&row.track_id) {
                identifiers_by_track
                    .entry(row.track_id.clone())
                    .or_default()
                    .push(row);
            }
        }

        let mut tracks = Vec::new();
        let mut catalog_tracks = Vec::new();
        let mut locations = Vec::new();

        for row in catalog_track_rows {
            if !visible_track_ids.is_empty() && !visible_track_ids.contains(&row.track_id) {
                continue;
            }

            let identifiers = identifiers_by_track
                .get(&row.track_id)
                .cloned()
                .unwrap_or_default();

            if let Some(identifier) =
                choose_identifier(&row, &identifiers_by_track, &identifier_lookup)
            {
                if let Some(path) = local_path_for_identifier(&identifier) {
                    tracks.push(build_local_track(&row, path));
                }
            }

            catalog_tracks.push(TrackRecord {
                identity: TrackIdentity::new(
                    row.track_id.clone(),
                    row.primary_provider_id.clone(),
                    row.primary_source_track_id.clone(),
                    row.fingerprint.clone(),
                ),
                metadata: TrackMetadata {
                    title: row.title.clone().unwrap_or_else(|| row.file_name.clone()),
                    artist: row
                        .artist
                        .clone()
                        .unwrap_or_else(|| "<unknown>".to_string()),
                    album: row.album.clone().unwrap_or_else(|| "<unknown>".to_string()),
                    album_artist: row.album_artist.clone(),
                    genre: row.genre.clone(),
                    track_number: row.track_number.and_then(|value| u32::try_from(value).ok()),
                    disc_number: row.disc_number.and_then(|value| u32::try_from(value).ok()),
                    year: row.year.and_then(|value| u32::try_from(value).ok()),
                    duration_ms: row.duration_ms.and_then(|value| u64::try_from(value).ok()),
                },
                ownership_scope: parse_enum(&row.ownership_scope)?,
                availability: parse_enum(&row.availability)?,
                preferred_location_id: row.preferred_identifier_id.clone(),
            });

            for identifier in identifiers {
                locations.push(TrackLocationRecord {
                    id: identifier.id,
                    track_id: identifier.track_id,
                    source_id: identifier.provider_id,
                    storage_kind: parse_enum(&identifier.storage_kind)?,
                    path: identifier.path,
                    url: identifier.url,
                    availability: parse_enum(&identifier.availability)?,
                });
            }
        }

        Ok(CatalogProjection {
            tracks,
            catalog: CatalogIndex::new(catalog_tracks, sources, locations),
            saved_track_ids,
            hidden_track_ids,
            local_music_roots: local_music_roots.into_iter().map(PathBuf::from).collect(),
            volume_step,
            cache_enabled,
        })
    }

    pub async fn save_projection(
        &self,
        user_id: &str,
        projection: &CatalogProjection,
    ) -> Result<()> {
        let local_music_roots: Vec<String> = projection
            .local_music_roots
            .iter()
            .map(|path| path.display().to_string())
            .collect();

        let mut tx = self
            .pool
            .begin()
            .await
            .context("failed to open transaction")?;

        sqlx::query(
            r#"
            INSERT INTO users (id)
            VALUES ($1)
            ON CONFLICT (id) DO NOTHING
            "#,
        )
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .context("failed to upsert user")?;

        sqlx::query(
            r#"
            INSERT INTO user_settings (user_id, local_music_roots, volume_step, cache_enabled)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id) DO UPDATE
            SET local_music_roots = EXCLUDED.local_music_roots,
                volume_step = EXCLUDED.volume_step,
                cache_enabled = EXCLUDED.cache_enabled,
                updated_at = NOW()
            "#,
        )
        .bind(user_id)
        .bind(Json(local_music_roots))
        .bind(projection.volume_step)
        .bind(projection.cache_enabled)
        .execute(&mut *tx)
        .await
        .context("failed to upsert user settings")?;

        for source in &projection.catalog.sources {
            let capabilities = source
                .capabilities
                .clone()
                .unwrap_or_else(|| SourceCapabilities::new(false, false, false, false));

            sqlx::query(
                r#"
                INSERT INTO providers (id, kind, name, scan, stream, download, sync)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                ON CONFLICT (id) DO UPDATE
                SET kind = EXCLUDED.kind,
                    name = EXCLUDED.name,
                    scan = EXCLUDED.scan,
                    stream = EXCLUDED.stream,
                    download = EXCLUDED.download,
                    sync = EXCLUDED.sync,
                    updated_at = NOW()
                "#,
            )
            .bind(&source.id)
            .bind(enum_to_string(source.kind)?)
            .bind(source.name.clone().unwrap_or_else(|| source.id.clone()))
            .bind(capabilities.scan)
            .bind(capabilities.stream)
            .bind(capabilities.download)
            .bind(capabilities.sync)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("failed to persist provider {}", source.id))?;
        }

        for source in &projection.catalog.sources {
            let enabled = source.id == "local_import" || source.enabled;
            let status = if enabled { "active" } else { "disabled" };
            let priority = source.priority.and_then(|value| i32::try_from(value).ok());

            if source.id == "local_import" {
                sqlx::query(
                    r#"
                    INSERT INTO provider_accounts (
                        user_id, provider_id, enabled, priority, scopes, status, settings
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7)
                    ON CONFLICT (user_id, provider_id) DO UPDATE
                    SET enabled = EXCLUDED.enabled,
                        priority = EXCLUDED.priority,
                        status = EXCLUDED.status,
                        settings = EXCLUDED.settings,
                        updated_at = NOW()
                    "#,
                )
                .bind(user_id)
                .bind(&source.id)
                .bind(enabled)
                .bind(priority)
                .bind(Json(Vec::<String>::new()))
                .bind(status)
                .bind(Json(serde_json::json!({})))
                .execute(&mut *tx)
                .await
                .with_context(|| format!("failed to persist provider account {}", source.id))?;
            } else {
                sqlx::query(
                    r#"
                    INSERT INTO provider_accounts (
                        user_id, provider_id, enabled, priority, scopes, status, settings
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7)
                    ON CONFLICT (user_id, provider_id) DO NOTHING
                    "#,
                )
                .bind(user_id)
                .bind(&source.id)
                .bind(enabled)
                .bind(priority)
                .bind(Json(Vec::<String>::new()))
                .bind(status)
                .bind(Json(serde_json::json!({})))
                .execute(&mut *tx)
                .await
                .with_context(|| format!("failed to seed provider account {}", source.id))?;
            }
        }

        sqlx::query(
            r#"
            DELETE FROM user_library_items
            WHERE user_id = $1
              AND item_kind = 'owned'
              AND track_id IN (
                  SELECT id
                  FROM catalog_tracks
                  WHERE primary_provider_id = 'local_import'
              )
            "#,
        )
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .context("failed to clear owned local tracks")?;

        for track in &projection.catalog.tracks {
            let metadata = &track.metadata;
            let file_name = Path::new(&track.identity.source_track_id)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| metadata.title.clone());

            sqlx::query(
                r#"
                INSERT INTO catalog_tracks (
                    id, primary_provider_id, primary_source_track_id, fingerprint,
                    file_name, title, artist, album, album_artist, genre,
                    track_number, disc_number, year, duration_ms,
                    ownership_scope, availability, preferred_identifier_id
                )
                VALUES (
                    $1, $2, $3, $4,
                    $5, $6, $7, $8, $9, $10,
                    $11, $12, $13, $14,
                    $15, $16, $17
                )
                ON CONFLICT (id) DO UPDATE
                SET primary_provider_id = EXCLUDED.primary_provider_id,
                    primary_source_track_id = EXCLUDED.primary_source_track_id,
                    fingerprint = EXCLUDED.fingerprint,
                    file_name = EXCLUDED.file_name,
                    title = EXCLUDED.title,
                    artist = EXCLUDED.artist,
                    album = EXCLUDED.album,
                    album_artist = EXCLUDED.album_artist,
                    genre = EXCLUDED.genre,
                    track_number = EXCLUDED.track_number,
                    disc_number = EXCLUDED.disc_number,
                    year = EXCLUDED.year,
                    duration_ms = EXCLUDED.duration_ms,
                    ownership_scope = EXCLUDED.ownership_scope,
                    availability = EXCLUDED.availability,
                    preferred_identifier_id = EXCLUDED.preferred_identifier_id,
                    updated_at = NOW()
                "#,
            )
            .bind(&track.identity.track_id)
            .bind(&track.identity.source_id)
            .bind(&track.identity.source_track_id)
            .bind(&track.identity.fingerprint)
            .bind(&file_name)
            .bind(&metadata.title)
            .bind(&metadata.artist)
            .bind(&metadata.album)
            .bind(&metadata.album_artist)
            .bind(&metadata.genre)
            .bind(
                metadata
                    .track_number
                    .and_then(|value| i32::try_from(value).ok()),
            )
            .bind(
                metadata
                    .disc_number
                    .and_then(|value| i32::try_from(value).ok()),
            )
            .bind(metadata.year.and_then(|value| i32::try_from(value).ok()))
            .bind(
                metadata
                    .duration_ms
                    .and_then(|value| i64::try_from(value).ok()),
            )
            .bind(enum_to_string(track.ownership_scope)?)
            .bind(enum_to_string(track.availability)?)
            .bind(&track.preferred_location_id)
            .execute(&mut *tx)
            .await
            .with_context(|| {
                format!(
                    "failed to persist catalog track {}",
                    track.identity.track_id
                )
            })?;

            sqlx::query(
                r#"
                INSERT INTO user_library_items (user_id, track_id, item_kind)
                VALUES ($1, $2, 'owned')
                ON CONFLICT (user_id, track_id, item_kind) DO NOTHING
                "#,
            )
            .bind(user_id)
            .bind(&track.identity.track_id)
            .execute(&mut *tx)
            .await
            .with_context(|| {
                format!(
                    "failed to persist owned library item {}",
                    track.identity.track_id
                )
            })?;
        }

        let preferred_ids: HashMap<String, String> = projection
            .catalog
            .tracks
            .iter()
            .filter_map(|track| {
                track
                    .preferred_location_id
                    .as_ref()
                    .map(|preferred| (track.identity.track_id.clone(), preferred.clone()))
            })
            .collect();

        for location in &projection.catalog.locations {
            let identifier_kind = enum_to_string(location.storage_kind)?;
            let value = location
                .path
                .clone()
                .or_else(|| location.url.clone())
                .unwrap_or_else(|| location.track_id.clone());
            let is_preferred = preferred_ids
                .get(&location.track_id)
                .map(|preferred| preferred == &location.id)
                .unwrap_or(false);

            sqlx::query(
                r#"
                INSERT INTO track_identifiers (
                    id, track_id, provider_id, identifier_kind, value,
                    path, url, storage_kind, availability, is_preferred
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                ON CONFLICT (id) DO UPDATE
                SET track_id = EXCLUDED.track_id,
                    provider_id = EXCLUDED.provider_id,
                    identifier_kind = EXCLUDED.identifier_kind,
                    value = EXCLUDED.value,
                    path = EXCLUDED.path,
                    url = EXCLUDED.url,
                    storage_kind = EXCLUDED.storage_kind,
                    availability = EXCLUDED.availability,
                    is_preferred = EXCLUDED.is_preferred,
                    updated_at = NOW()
                "#,
            )
            .bind(&location.id)
            .bind(&location.track_id)
            .bind(&location.source_id)
            .bind(&identifier_kind)
            .bind(&value)
            .bind(&location.path)
            .bind(&location.url)
            .bind(&identifier_kind)
            .bind(enum_to_string(location.availability)?)
            .bind(is_preferred)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("failed to persist identifier {}", location.id))?;
        }

        for track_id in &projection.saved_track_ids {
            sqlx::query(
                r#"
                INSERT INTO user_library_items (user_id, track_id, item_kind)
                VALUES ($1, $2, 'saved')
                ON CONFLICT (user_id, track_id, item_kind) DO NOTHING
                "#,
            )
            .bind(user_id)
            .bind(track_id)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("failed to persist saved track {track_id}"))?;
        }

        for track_id in &projection.hidden_track_ids {
            sqlx::query(
                r#"
                INSERT INTO user_library_items (user_id, track_id, item_kind)
                VALUES ($1, $2, 'hidden')
                ON CONFLICT (user_id, track_id, item_kind) DO NOTHING
                "#,
            )
            .bind(user_id)
            .bind(track_id)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("failed to persist hidden track {track_id}"))?;
        }

        sqlx::query(
            r#"
            DELETE FROM catalog_tracks
            WHERE primary_provider_id = 'local_import'
              AND NOT EXISTS (
                  SELECT 1
                  FROM user_library_items
                  WHERE user_library_items.track_id = catalog_tracks.id
              )
            "#,
        )
        .execute(&mut *tx)
        .await
        .context("failed to clear orphaned local tracks")?;

        tx.commit()
            .await
            .context("failed to commit catalog transaction")?;
        Ok(())
    }

    async fn load_settings(&self, user_id: &str) -> Result<Option<UserSettingsRow>> {
        let settings = sqlx::query_as(
            r#"
            SELECT local_music_roots, volume_step, cache_enabled
            FROM user_settings
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .context("failed to load settings")?;

        Ok(settings)
    }
}

fn configured_database_connect_timeout() -> Duration {
    env::var("REPLAYCORE_DATABASE_CONNECT_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .map(Duration::from_millis)
        .unwrap_or_else(|| Duration::from_secs(1))
}

fn choose_identifier(
    track_row: &CatalogTrackRow,
    identifiers_by_track: &HashMap<String, Vec<TrackIdentifierRow>>,
    identifiers_by_id: &HashMap<String, TrackIdentifierRow>,
) -> Option<TrackIdentifierRow> {
    if let Some(preferred_identifier_id) = track_row.preferred_identifier_id.as_ref() {
        if let Some(identifier) = identifiers_by_id.get(preferred_identifier_id) {
            return Some(identifier.clone());
        }
    }

    let identifiers = identifiers_by_track.get(&track_row.track_id)?;

    if let Some(identifier) = identifiers
        .iter()
        .find(|identifier| identifier.is_preferred)
    {
        return Some(identifier.clone());
    }

    if let Some(identifier) = identifiers
        .iter()
        .find(|identifier| identifier.storage_kind == "local_file")
    {
        return Some(identifier.clone());
    }

    identifiers.first().cloned()
}

fn build_local_track(row: &CatalogTrackRow, path: PathBuf) -> Track {
    let duration = row
        .duration_ms
        .and_then(|value| u64::try_from(value).ok())
        .map(Duration::from_millis);

    Track {
        path,
        file_name: row.file_name.clone(),
        title: row.title.clone(),
        artist: row.artist.clone(),
        album: row.album.clone(),
        duration,
    }
}

fn provider_account_summary_from_row(row: ProviderAccountSummaryRow) -> ProviderAccountSummary {
    ProviderAccountSummary {
        provider_id: row.provider_id,
        provider_kind: row.provider_kind,
        provider_name: row.provider_name,
        enabled: row.enabled,
        priority: row.priority.map(i64::from),
        external_account_id: row.external_account_id,
        scopes: row.scopes.0,
        has_access_token: row.has_access_token,
        has_refresh_token: row.has_refresh_token,
        token_expires_at: row.token_expires_at,
        status: row.status,
        settings: row.settings.0,
        scan: row.scan,
        stream: row.stream,
        download: row.download,
        sync: row.sync,
    }
}

fn local_path_for_identifier(identifier: &TrackIdentifierRow) -> Option<PathBuf> {
    identifier
        .path
        .clone()
        .or_else(|| {
            if matches!(
                identifier.storage_kind.as_str(),
                "local_file" | "cached_file"
            ) {
                Some(identifier.value.clone())
            } else {
                None
            }
        })
        .map(PathBuf::from)
}

fn enum_to_string<T: Serialize>(value: T) -> Result<String> {
    match serde_json::to_value(value).context("failed to serialize enum")? {
        Value::String(value) => Ok(value),
        other => Err(anyhow::anyhow!("expected enum string, got {}", other)),
    }
}

fn parse_enum<T: DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_value(Value::String(value.to_string())).context("failed to parse enum value")
}
