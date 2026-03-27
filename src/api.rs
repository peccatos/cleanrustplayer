// HTTP surface for the local service and explicit database mode.
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as AnyhowContext, Result};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde::Serialize;
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;

use crate::context::AppContext;
use crate::contract::{CommandEnvelope, ReplayCoreContract};
use crate::provider_accounts::{ProviderAccountSummary, ProviderAccountWrite};
use crate::service::ReplayCoreService;

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
        .route("/v1/library/roots", post(update_library_roots_handler))
        .route("/v1/providers", get(providers_handler))
        .route("/v1/resolve", get(resolve_handler))
        .route(
            "/v1/provider-accounts/{provider_id}",
            post(provider_account_upsert_handler).delete(provider_account_clear_handler),
        )
        .layer(CorsLayer::permissive())
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

#[derive(Debug, Deserialize)]
struct UpdateLibraryRootsRequest {
    local_music_roots: Vec<String>,
}

async fn update_library_roots_handler(
    State(state): State<ApiState>,
    Json(input): Json<UpdateLibraryRootsRequest>,
) -> Result<Json<CommandEnvelope<RefreshSummary>>, (StatusCode, String)> {
    let summary = {
        let mut context = state.context.lock().await;
        let roots = input
            .local_music_roots
            .into_iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>();

        let tracks_scanned = context
            .set_local_music_roots(roots)
            .map_err(internal_error)?;
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
struct ResolveQuery {
    url: Option<String>,
}

async fn resolve_handler(
    State(state): State<ApiState>,
    Query(params): Query<ResolveQuery>,
) -> Result<Json<CommandEnvelope<crate::provider::ResolvedMedia>>, (StatusCode, String)> {
    let Some(url) = params.url else {
        return Err(bad_request("missing url"));
    };

    let normalized = url.trim().to_string();
    if normalized.is_empty() {
        return Err(bad_request("missing url"));
    }

    let provider = {
        let context = state.context.lock().await;
        context.search_service.resolve(&normalized)
    }
    .map_err(internal_error)?;

    Ok(Json(CommandEnvelope::ok(provider)))
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
