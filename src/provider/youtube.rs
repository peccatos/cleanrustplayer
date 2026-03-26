use anyhow::{Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use reqwest::Url;
use serde::Deserialize;

use crate::provider::{
    MediaKind, MusicProvider, ProviderCapabilities, ProviderHttpConfig, ProviderKind,
    ResolvedMedia, SearchItem, SearchQuery,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct YouTubeSearchResponse {
    items: Vec<YouTubeSearchItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct YouTubeSearchItem {
    id: YouTubeSearchItemId,
    snippet: YouTubeSnippet,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct YouTubeSearchItemId {
    kind: String,
    video_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct YouTubeSnippet {
    title: String,
    channel_title: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct YouTubeVideosResponse {
    items: Vec<YouTubeVideoItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct YouTubeVideoItem {
    id: String,
    snippet: YouTubeSnippet,
}

pub struct YouTubeProvider {
    client: Client,
    api_key: String,
}

impl YouTubeProvider {
    pub fn new(api_key: impl Into<String>, config: ProviderHttpConfig) -> Result<Self> {
        let api_key = api_key.into();
        let api_key = api_key.trim().to_string();
        if api_key.is_empty() {
            anyhow::bail!("youtube api key is empty");
        }

        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(&config.user_agent).context("invalid provider user agent")?,
        );

        let client = Client::builder()
            .default_headers(headers)
            .timeout(config.timeout)
            .build()
            .context("failed to build YouTube HTTP client")?;

        Ok(Self { client, api_key })
    }

    fn search_url(&self, query: &str, limit: usize) -> String {
        let limit = limit.clamp(1, 50);
        format!(
            "https://www.googleapis.com/youtube/v3/search?part=snippet&type=video&safeSearch=none&videoEmbeddable=true&maxResults={limit}&q={}&key={}",
            urlencoding::encode(query),
            urlencoding::encode(&self.api_key)
        )
    }

    fn videos_url(&self, video_id: &str) -> String {
        format!(
            "https://www.googleapis.com/youtube/v3/videos?part=snippet&id={}&key={}",
            urlencoding::encode(video_id),
            urlencoding::encode(&self.api_key)
        )
    }
}

impl MusicProvider for YouTubeProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::YouTube
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            search: true,
            resolve: true,
            preview_stream: false,
            full_stream: false,
        }
    }

    fn search(&self, query: &SearchQuery) -> Result<Vec<SearchItem>> {
        let normalized = query.normalized_text();
        if normalized.is_empty() {
            return Ok(Vec::new());
        }

        let url = self.search_url(&normalized, query.limit);
        let response = self
            .client
            .get(&url)
            .send()
            .context("youtube search request failed")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            anyhow::bail!("youtube search returned HTTP {}: {}", status, body);
        }

        let body = response
            .text()
            .context("failed to read youtube search response body")?;
        let parsed: YouTubeSearchResponse =
            serde_json::from_str(&body).context("failed to decode youtube search response")?;

        let mut items = Vec::new();
        for item in parsed.items {
            if item.id.kind != "youtube#video" {
                continue;
            }

            let video_id = match item.id.video_id {
                Some(video_id) => video_id,
                None => continue,
            };

            items.push(SearchItem {
                provider: ProviderKind::YouTube,
                kind: MediaKind::Track,
                title: item.snippet.title,
                artist: Some(item.snippet.channel_title),
                url: youtube_watch_url(&video_id),
                playable: true,
                preview_url: Some(youtube_embed_url(&video_id)),
            });
        }

        if query.limit > 0 && items.len() > query.limit {
            items.truncate(query.limit);
        }

        Ok(items)
    }

    fn resolve_page(&self, url: &str) -> Result<ResolvedMedia> {
        let normalized = url.trim();
        if normalized.is_empty() {
            anyhow::bail!("empty page url");
        }

        let video_id = youtube_video_id_from_url(normalized)
            .ok_or_else(|| anyhow::anyhow!("unsupported youtube url: {normalized}"))?;

        let response = self
            .client
            .get(self.videos_url(&video_id))
            .send()
            .with_context(|| format!("youtube resolve request failed: {normalized}"))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            anyhow::bail!("youtube resolve returned HTTP {}: {}", status, body);
        }

        let body = response
            .text()
            .context("failed to read youtube resolve response body")?;
        let parsed: YouTubeVideosResponse =
            serde_json::from_str(&body).context("failed to decode youtube resolve response")?;

        let Some(video) = parsed.items.into_iter().next() else {
            anyhow::bail!("youtube video not found: {video_id}");
        };

        Ok(ResolvedMedia {
            provider: ProviderKind::YouTube,
            kind: MediaKind::Track,
            title: video.snippet.title,
            artist: Some(video.snippet.channel_title),
            page_url: youtube_watch_url(&video.id),
            preview_url: Some(youtube_embed_url(&video.id)),
            playable: true,
        })
    }
}

pub fn youtube_video_id_from_url(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let url = Url::parse(trimmed).ok()?;
    let host = url.host_str()?;
    let host = host.strip_prefix("www.").unwrap_or(host);
    let host = host.strip_prefix("m.").unwrap_or(host);

    if host == "youtu.be" {
        return url
            .path_segments()?
            .find(|segment| !segment.is_empty())
            .map(|segment| segment.to_string());
    }

    if host == "youtube.com" || host.ends_with(".youtube.com") || host == "youtube-nocookie.com" {
        let mut segments = url.path_segments()?;
        let first = segments.next()?;

        return match first {
            "watch" => url
                .query_pairs()
                .find(|(key, _)| key == "v")
                .map(|(_, value)| value.into_owned()),
            "embed" | "shorts" | "live" | "v" => segments.next().map(|segment| segment.to_string()),
            _ => None,
        };
    }

    None
}

fn youtube_watch_url(video_id: &str) -> String {
    format!("https://www.youtube.com/watch?v={video_id}")
}

fn youtube_embed_url(video_id: &str) -> String {
    format!("https://www.youtube.com/embed/{video_id}?enablejsapi=1")
}

#[cfg(test)]
mod tests {
    use anyhow::{Context, Result};

    use crate::provider::{MusicProvider, ProviderHttpConfig, ProviderKind, SearchQuery};

    use super::youtube_video_id_from_url;
    use super::YouTubeProvider;

    #[test]
    fn parses_watch_url() {
        assert_eq!(
            youtube_video_id_from_url("https://www.youtube.com/watch?v=M7lc1UVf-VE"),
            Some("M7lc1UVf-VE".to_string())
        );
    }

    #[test]
    fn parses_short_url() {
        assert_eq!(
            youtube_video_id_from_url("https://youtu.be/M7lc1UVf-VE?t=12"),
            Some("M7lc1UVf-VE".to_string())
        );
    }

    #[test]
    fn parses_shorts_url() {
        assert_eq!(
            youtube_video_id_from_url("https://www.youtube.com/shorts/M7lc1UVf-VE"),
            Some("M7lc1UVf-VE".to_string())
        );
    }

    #[test]
    fn rejects_non_youtube_url() {
        assert_eq!(
            youtube_video_id_from_url("https://example.com/watch?v=abc"),
            None
        );
    }

    #[test]
    #[ignore]
    fn live_smoke_search_and_resolve() -> Result<()> {
        let api_key = std::env::var("REPLAYCORE_YOUTUBE_API_KEY")
            .context("REPLAYCORE_YOUTUBE_API_KEY is not set")?;
        let query = std::env::var("REPLAYCORE_YOUTUBE_SMOKE_QUERY")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "lofi hip hop".to_string());

        let provider = YouTubeProvider::new(api_key, ProviderHttpConfig::default())?;
        let results = provider.search(&SearchQuery::new(query, 5))?;

        assert!(!results.is_empty(), "expected live YouTube search results");

        let candidate = results
            .iter()
            .find(|item| item.playable)
            .or_else(|| results.first())
            .expect("at least one search result");

        let resolved = provider.resolve_page(&candidate.url)?;
        assert_eq!(resolved.provider, ProviderKind::YouTube);
        assert!(!resolved.title.trim().is_empty());
        assert!(resolved.playable);

        Ok(())
    }
}
