// Provider fan-out for search and page resolution.
use anyhow::{anyhow, Result};
use std::sync::Arc;

use crate::provider::registry::ProviderRegistry;
use crate::provider::{MusicProvider, ProviderKind, ResolvedMedia, SearchItem, SearchQuery};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSearchError {
    pub provider: ProviderKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchReport {
    pub query: String,
    pub items: Vec<SearchItem>,
    pub errors: Vec<ProviderSearchError>,
}

#[derive(Clone)]
pub struct SearchService {
    registry: ProviderRegistry,
}

impl SearchService {
    pub fn new(registry: ProviderRegistry) -> Self {
        Self { registry }
    }

    pub fn registry(&self) -> &ProviderRegistry {
        &self.registry
    }

    pub fn search(&self, query: SearchQuery) -> SearchReport {
        let normalized_query = query.normalized_text();

        if normalized_query.is_empty() {
            return SearchReport {
                query: normalized_query,
                items: Vec::new(),
                errors: Vec::new(),
            };
        }

        let effective_limit = if query.limit == 0 { 10 } else { query.limit };

        let effective_query = SearchQuery {
            text: normalized_query.clone(),
            limit: effective_limit,
        };

        let mut items = Vec::new();
        let mut errors = Vec::new();

        for provider in self.registry.providers() {
            if !provider.capabilities().search {
                continue;
            }

            match provider.search(&effective_query) {
                Ok(mut provider_items) => {
                    items.append(&mut provider_items);
                }
                Err(err) => {
                    errors.push(ProviderSearchError {
                        provider: provider.kind(),
                        message: err.to_string(),
                    });
                }
            }
        }

        if items.len() > effective_limit {
            items.truncate(effective_limit);
        }

        SearchReport {
            query: normalized_query,
            items,
            errors,
        }
    }

    pub fn resolve(&self, url: &str) -> Result<ResolvedMedia> {
        let normalized = url.trim();
        if normalized.is_empty() {
            return Err(anyhow!("empty url"));
        }

        let providers = self.resolve_providers_for_url(normalized);
        if providers.is_empty() {
            return Err(anyhow!("no resolve-capable provider is registered"));
        }

        let mut last_error = None;
        for provider in providers {
            match provider.resolve_page(normalized) {
                Ok(media) => return Ok(media),
                Err(err) => last_error = Some(err),
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("unable to resolve url")))
    }

    fn resolve_providers_for_url(&self, url: &str) -> Vec<Arc<dyn MusicProvider>> {
        let registry = self.registry();
        let normalized = url.trim();

        let mut providers = Vec::new();

        if is_bandcamp_url(normalized) {
            if let Some(provider) = registry.find(ProviderKind::Bandcamp) {
                providers.push(provider);
            }
        } else {
            for provider in registry.providers() {
                if provider.capabilities().resolve {
                    providers.push(provider.clone());
                }
            }
        }

        providers
    }
}

fn is_bandcamp_url(url: &str) -> bool {
    url.contains("bandcamp.com")
}
