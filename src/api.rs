use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as AnyhowContext, Result};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{response::Html, Json, Router};
use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;
use tokio::sync::Mutex;

use crate::context::AppContext;
use crate::contract::{CommandEnvelope, ReplayCoreContract};
use crate::provider::{MusicProvider, ProviderKind, ResolvedMedia, SearchItem};
use crate::provider_accounts::{ProviderAccountSummary, ProviderAccountWrite};
use crate::service::ReplayCoreService;
use crate::web::youtube_player_page;

#[derive(Clone)]
pub struct ApiState {
    pub context: Arc<Mutex<AppContext>>,
    pub service: ReplayCoreService,
}

#[derive(Debug, Clone, Serialize)]
pub struct RefreshSummary {
    pub tracks_scanned: usize,
    pub catalog_hash: Option<String>,
    pub enabled_source_ids: Vec<String>,
}

pub fn run_server(context: AppContext, addr: SocketAddr) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to create Tokio runtime")?;

    runtime.block_on(serve(context, addr))
}

async fn serve(context: AppContext, addr: SocketAddr) -> Result<()> {
    let state = ApiState {
        context: Arc::new(Mutex::new(context)),
        service: ReplayCoreService::new()?,
    };

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/v1/contract", get(contract_handler))
        .route("/v1/library/refresh", post(refresh_handler))
        .route("/v1/providers", get(providers_handler))
        .route("/v1/youtube/search", get(youtube_search_handler))
        .route("/v1/youtube/resolve", get(youtube_resolve_handler))
        .route("/web/youtube", get(youtube_page_handler))
        .route(
            "/v1/provider-accounts/{provider_id}",
            post(provider_account_upsert_handler).delete(provider_account_clear_handler),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context("failed to bind API listener")?;

    println!("ReplayCore API listening on {}", addr);
    axum::serve(listener, app)
        .await
        .context("ReplayCore API server stopped unexpectedly")?;

    Ok(())
}

async fn health_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "ok": true,
        "service": "replaycore"
    }))
}

async fn contract_handler(
    State(state): State<ApiState>,
) -> Result<Json<ReplayCoreContract>, (StatusCode, String)> {
    let (context_snapshot, runtime) = {
        let context = state.context.lock().await;
        (
            context.clone(),
            crate::service::ReplayCoreService::headless_runtime(&context),
        )
    };

    let contract = state.service.build_contract(&context_snapshot, runtime);
    state
        .service
        .validate_contract(&contract)
        .map_err(internal_error)?;

    Ok(Json(contract))
}

async fn refresh_handler(
    State(state): State<ApiState>,
) -> Result<Json<CommandEnvelope<RefreshSummary>>, (StatusCode, String)> {
    let summary = {
        let mut context = state.context.lock().await;
        let tracks_scanned = context.reload_local_library().map_err(internal_error)?;
        let enabled_source_ids = context
            .catalog
            .sources
            .iter()
            .filter(|source| source.enabled)
            .map(|source| source.id.clone())
            .collect();

        RefreshSummary {
            tracks_scanned,
            catalog_hash: context.catalog.catalog_hash.clone(),
            enabled_source_ids,
        }
    };

    Ok(Json(CommandEnvelope::ok(summary)))
}

async fn providers_handler(
    State(state): State<ApiState>,
) -> Result<Json<CommandEnvelope<Vec<ProviderAccountSummary>>>, (StatusCode, String)> {
    let (context_snapshot, repository) = {
        let context = state.context.lock().await;
        (context.clone(), context.repository.clone())
    };

    let providers = if let Some(repository) = repository {
        repository
            .load_provider_accounts(&context_snapshot.user_id)
            .await
            .map_err(internal_error)?
    } else {
        context_snapshot
            .catalog
            .sources
            .iter()
            .map(ProviderAccountSummary::from_source)
            .collect()
    };

    Ok(Json(CommandEnvelope::ok(providers)))
}

#[derive(Debug, Deserialize)]
struct YouTubePageQuery {
    q: Option<String>,
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YouTubeSearchQuery {
    q: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YouTubeResolveQuery {
    url: Option<String>,
}

async fn youtube_page_handler(Query(params): Query<YouTubePageQuery>) -> Html<String> {
    Html(youtube_player_page(
        params.q.as_deref(),
        params.url.as_deref(),
    ))
}

async fn youtube_search_handler(
    State(state): State<ApiState>,
    Query(params): Query<YouTubeSearchQuery>,
) -> Result<Json<CommandEnvelope<Vec<SearchItem>>>, (StatusCode, String)> {
    let query = params.q.unwrap_or_default();
    let normalized = query.trim().to_string();
    if normalized.is_empty() {
        return Ok(Json(CommandEnvelope::ok(Vec::new())));
    }

    youtube_provider(&state).await?;
    let api_key = youtube_api_key().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "youtube api key is not configured".to_string(),
        )
    })?;

    let items = youtube_search_items(&api_key, &normalized, 12)
        .await
        .map_err(internal_error)?;

    Ok(Json(CommandEnvelope::ok(items)))
}

async fn youtube_resolve_handler(
    State(state): State<ApiState>,
    Query(params): Query<YouTubeResolveQuery>,
) -> Result<Json<CommandEnvelope<ResolvedMedia>>, (StatusCode, String)> {
    let Some(url) = params.url else {
        return Err(bad_request("missing url"));
    };

    let normalized = url.trim().to_string();
    if normalized.is_empty() {
        return Err(bad_request("missing url"));
    }

    youtube_provider(&state).await?;
    let api_key = youtube_api_key().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "youtube api key is not configured".to_string(),
        )
    })?;

    let media = youtube_resolve_media(&api_key, &normalized)
        .await
        .map_err(internal_error)?;

    Ok(Json(CommandEnvelope::ok(media)))
}

async fn provider_account_upsert_handler(
    axum::extract::Path(provider_id): axum::extract::Path<String>,
    State(state): State<ApiState>,
    Json(input): Json<ProviderAccountWrite>,
) -> Result<Json<CommandEnvelope<ProviderAccountSummary>>, (StatusCode, String)> {
    let (context_snapshot, repository, token_vault) = {
        let context = state.context.lock().await;
        (
            context.clone(),
            context.repository.clone(),
            context.token_vault.clone(),
        )
    };

    let repository = repository
        .ok_or_else(|| internal_error("database is not configured for provider account writes"))?;
    let token_vault =
        token_vault.ok_or_else(|| internal_error("token encryption key is not configured"))?;

    let summary = repository
        .upsert_provider_account(
            &context_snapshot.user_id,
            &provider_id,
            &input,
            &token_vault,
        )
        .await
        .map_err(internal_error)?;

    Ok(Json(CommandEnvelope::ok(summary)))
}

async fn provider_account_clear_handler(
    axum::extract::Path(provider_id): axum::extract::Path<String>,
    State(state): State<ApiState>,
) -> Result<Json<CommandEnvelope<ProviderAccountSummary>>, (StatusCode, String)> {
    let (context_snapshot, repository) = {
        let context = state.context.lock().await;
        (context.clone(), context.repository.clone())
    };

    let repository = repository
        .ok_or_else(|| internal_error("database is not configured for provider account writes"))?;

    let summary = repository
        .clear_provider_account(&context_snapshot.user_id, &provider_id)
        .await
        .map_err(internal_error)?;

    Ok(Json(CommandEnvelope::ok(summary)))
}

fn internal_error(error: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

fn bad_request(error: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, error.to_string())
}

fn youtube_api_key() -> Option<String> {
    std::env::var("REPLAYCORE_YOUTUBE_API_KEY")
        .ok()
        .or_else(|| std::env::var("YOUTUBE_API_KEY").ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn youtube_http_client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("ReplayCore/0.1")
        .build()
        .map_err(|err| err.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct YouTubeSearchResponse {
    items: Vec<YouTubeSearchItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct YouTubeSearchItem {
    id: YouTubeSearchItemId,
    snippet: YouTubeSnippet,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct YouTubeSearchItemId {
    kind: String,
    video_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct YouTubeSnippet {
    title: String,
    channel_title: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct YouTubeVideosResponse {
    items: Vec<YouTubeVideoItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct YouTubeVideoItem {
    id: String,
    snippet: YouTubeSnippet,
}

async fn youtube_search_items(
    api_key: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchItem>, String> {
    let client = youtube_http_client()?;
    let limit = limit.clamp(1, 50);
    let url = format!(
        "https://www.googleapis.com/youtube/v3/search?part=snippet&type=video&safeSearch=none&videoEmbeddable=true&maxResults={limit}&q={}&key={}",
        urlencoding::encode(query),
        urlencoding::encode(api_key)
    );

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|err| err.to_string())?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("youtube search returned HTTP {}: {}", status, body));
    }

    let body = response.text().await.map_err(|err| err.to_string())?;
    let parsed: YouTubeSearchResponse =
        serde_json::from_str(&body).map_err(|err| err.to_string())?;

    let mut items = Vec::new();
    for item in parsed.items {
        if item.id.kind != "youtube#video" {
            continue;
        }

        let Some(video_id) = item.id.video_id else {
            continue;
        };

        items.push(SearchItem {
            provider: ProviderKind::YouTube,
            kind: crate::provider::MediaKind::Track,
            title: item.snippet.title,
            artist: Some(item.snippet.channel_title),
            url: youtube_watch_url(&video_id),
            playable: true,
            preview_url: Some(youtube_embed_url(&video_id)),
        });
    }

    if limit > 0 && items.len() > limit {
        items.truncate(limit);
    }

    Ok(items)
}

async fn youtube_resolve_media(api_key: &str, url: &str) -> Result<ResolvedMedia, String> {
    let client = youtube_http_client()?;
    let video_id = crate::provider::youtube::youtube_video_id_from_url(url)
        .ok_or_else(|| format!("unsupported youtube url: {url}"))?;
    let resolve_url = format!(
        "https://www.googleapis.com/youtube/v3/videos?part=snippet&id={}&key={}",
        urlencoding::encode(&video_id),
        urlencoding::encode(api_key)
    );

    let response = client
        .get(resolve_url)
        .send()
        .await
        .map_err(|err| err.to_string())?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "youtube resolve returned HTTP {}: {}",
            status, body
        ));
    }

    let body = response.text().await.map_err(|err| err.to_string())?;
    let parsed: YouTubeVideosResponse =
        serde_json::from_str(&body).map_err(|err| err.to_string())?;

    let Some(video) = parsed.items.into_iter().next() else {
        return Err(format!("youtube video not found: {video_id}"));
    };

    Ok(ResolvedMedia {
        provider: ProviderKind::YouTube,
        kind: crate::provider::MediaKind::Track,
        title: video.snippet.title,
        artist: Some(video.snippet.channel_title),
        page_url: youtube_watch_url(&video.id),
        preview_url: Some(youtube_embed_url(&video.id)),
        playable: true,
    })
}

fn youtube_watch_url(video_id: &str) -> String {
    format!("https://www.youtube.com/watch?v={video_id}")
}

fn youtube_embed_url(video_id: &str) -> String {
    format!("https://www.youtube.com/embed/{video_id}?enablejsapi=1")
}

async fn youtube_provider(
    state: &ApiState,
) -> Result<Arc<dyn MusicProvider>, (StatusCode, String)> {
    let provider = {
        let context = state.context.lock().await;
        context
            .search_service
            .registry()
            .find(ProviderKind::YouTube)
    };

    provider.ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "youtube provider is not registered".to_string(),
        )
    })
}
