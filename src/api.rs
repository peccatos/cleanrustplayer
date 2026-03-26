use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context as AnyhowContext, Result};
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use tokio::sync::Mutex;

use crate::context::AppContext;
use crate::contract::{CommandEnvelope, ReplayCoreContract};
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

fn internal_error(error: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}
