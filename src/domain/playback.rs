use serde::{Deserialize, Serialize};

use super::track::TrackSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackKind {
    Url,
    StreamEndpoint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybackSource {
    pub kind: PlaybackKind,
    pub source: TrackSource,
    pub track_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}
