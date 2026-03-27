// Provider account state and capability summaries for the DB layer.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::contract::{SourceKind, SourceRecord};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderAccountWrite {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderAccountSummary {
    pub provider_id: String,
    pub provider_kind: String,
    pub provider_name: String,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
    pub has_access_token: bool,
    pub has_refresh_token: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_expires_at: Option<DateTime<Utc>>,
    pub status: String,
    pub settings: Value,
    pub scan: bool,
    pub stream: bool,
    pub download: bool,
    pub sync: bool,
}

impl ProviderAccountSummary {
    pub fn from_source(source: &SourceRecord) -> Self {
        let capabilities = source.capabilities.as_ref();
        Self {
            provider_id: source.id.clone(),
            provider_kind: source_kind_label(source.kind).to_string(),
            provider_name: source.name.clone().unwrap_or_else(|| source.id.clone()),
            enabled: source.enabled,
            priority: source.priority,
            external_account_id: None,
            scopes: Vec::new(),
            has_access_token: false,
            has_refresh_token: false,
            token_expires_at: None,
            status: if source.enabled {
                "active".to_string()
            } else {
                "disabled".to_string()
            },
            settings: json!({}),
            scan: capabilities.map(|value| value.scan).unwrap_or(false),
            stream: capabilities.map(|value| value.stream).unwrap_or(false),
            download: capabilities.map(|value| value.download).unwrap_or(false),
            sync: capabilities.map(|value| value.sync).unwrap_or(false),
        }
    }
}

pub fn source_kind_label(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::LocalDisk => "local_disk",
        SourceKind::Bandcamp => "bandcamp",
    }
}
