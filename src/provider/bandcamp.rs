// Bandcamp adapter for search and page resolution.
use anyhow::{Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};

use crate::provider::bandcamp_extract::{parse_release_page, parse_search_results};
use crate::provider::{
    MusicProvider, ProviderCapabilities, ProviderHttpConfig, ProviderKind, ResolvedMedia,
    SearchItem, SearchQuery,
};

pub struct BandcampProvider {
    client: Client,
}

impl BandcampProvider {
    pub fn new(config: ProviderHttpConfig) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(&config.user_agent).context("invalid provider user agent")?,
        );

        let client = Client::builder()
            .default_headers(headers)
            .timeout(config.timeout)
            .build()
            .context("failed to build Bandcamp HTTP client")?;

        Ok(Self { client })
    }

    fn build_search_url(query: &str) -> String {
        format!(
            "https://bandcamp.com/search?q={}",
            urlencoding::encode(query)
        )
    }
}

impl MusicProvider for BandcampProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Bandcamp
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            search: true,
            resolve: true,
            preview_stream: true,
            full_stream: false,
        }
    }

    fn search(&self, query: &SearchQuery) -> Result<Vec<SearchItem>> {
        let normalized = query.normalized_text();

        if normalized.is_empty() {
            return Ok(Vec::new());
        }

        let url = Self::build_search_url(&normalized);

        let response = self
            .client
            .get(&url)
            .send()
            .context("bandcamp search request failed")?;

        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("bandcamp search returned HTTP {}", status);
        }

        let html = response
            .text()
            .context("failed to read bandcamp search response body")?;

        let mut items = parse_search_results(&html)?;

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

        let response = self
            .client
            .get(normalized)
            .send()
            .with_context(|| format!("bandcamp resolve request failed: {normalized}"))?;

        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("bandcamp resolve returned HTTP {}", status);
        }

        let html = response
            .text()
            .context("failed to read bandcamp resolve response body")?;

        parse_release_page(normalized, &html)
    }
}
