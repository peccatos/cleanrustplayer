use crate::provider::registry::ProviderRegistry;
use crate::provider::{ProviderKind, SearchItem, SearchQuery};

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

impl SearchReport {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

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
}
