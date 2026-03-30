use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackSource {
    Local,
    Cloud,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackRef {
    pub source: TrackSource,
    pub track_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackItem {
    pub id: String,
    pub source: TrackSource,
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub artist: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub album: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_sec: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artwork_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    pub backend_track_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud: Option<CloudTrackInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudTrackInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}
