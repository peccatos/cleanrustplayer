use anyhow::Result;
use std::fmt;
use std::time::Duration;

pub mod bandcamp;
pub mod bandcamp_extract;
pub mod registry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderKind {
    Local,
    Bandcamp,
}

impl fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderKind::Local => write!(f, "local"),
            ProviderKind::Bandcamp => write!(f, "bandcamp"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaKind {
    Track,
    Album,
    Artist,
}

impl fmt::Display for MediaKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MediaKind::Track => write!(f, "track"),
            MediaKind::Album => write!(f, "album"),
            MediaKind::Artist => write!(f, "artist"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub text: String,
    pub limit: usize,
}

impl SearchQuery {
    pub fn new(text: impl Into<String>, limit: usize) -> Self {
        Self {
            text: text.into(),
            limit,
        }
    }

    pub fn normalized_text(&self) -> String {
        self.text.trim().to_string()
    }

    pub fn is_empty(&self) -> bool {
        self.normalized_text().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchItem {
    pub provider: ProviderKind,
    pub kind: MediaKind,
    pub title: String,
    pub artist: Option<String>,
    pub url: String,
    pub playable: bool,
    pub preview_url: Option<String>,
}

impl SearchItem {
    pub fn display_title(&self) -> String {
        match &self.artist {
            Some(artist) if !artist.trim().is_empty() => format!("{artist} - {}", self.title),
            _ => self.title.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMedia {
    pub provider: ProviderKind,
    pub kind: MediaKind,
    pub title: String,
    pub artist: Option<String>,
    pub page_url: String,
    pub preview_url: Option<String>,
    pub playable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProviderCapabilities {
    pub search: bool,
    pub resolve: bool,
    pub preview_stream: bool,
    pub full_stream: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderHttpConfig {
    pub timeout: Duration,
    pub user_agent: String,
}

impl Default for ProviderHttpConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(10),
            user_agent: "ReplayCore/0.1".to_string(),
        }
    }
}

pub trait MusicProvider: Send + Sync {
    fn kind(&self) -> ProviderKind;
    fn capabilities(&self) -> ProviderCapabilities;
    fn search(&self, query: &SearchQuery) -> Result<Vec<SearchItem>>;
    fn resolve_page(&self, url: &str) -> Result<ResolvedMedia>;
}
