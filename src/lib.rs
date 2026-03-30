pub mod app;
pub mod command;
pub mod config;
pub mod domain;
pub mod context;
pub mod contract;
pub mod music;
pub mod player;
pub mod provider;
pub mod provider_accounts;
pub mod queue;
pub mod repository;
pub mod search;
pub mod service;
pub mod snapshot;
pub mod token_vault;
pub mod web;

use anyhow::Result;

use crate::context::AppContext;

pub fn sync_drive_library() -> Result<usize> {
    let mut context = AppContext::bootstrap()?;
    context.reload_cloud_library()
}
