use std::sync::Arc;

use crate::provider::{MusicProvider, ProviderKind};

#[derive(Default, Clone)]
pub struct ProviderRegistry {
    providers: Vec<Arc<dyn MusicProvider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    pub fn register<P>(&mut self, provider: P)
    where
        P: MusicProvider + 'static,
    {
        self.providers.push(Arc::new(provider));
    }

    pub fn providers(&self) -> &[Arc<dyn MusicProvider>] {
        &self.providers
    }

    pub fn len(&self) -> usize {
        self.providers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    pub fn find(&self, kind: ProviderKind) -> Option<Arc<dyn MusicProvider>> {
        self.providers
            .iter()
            .find(|provider| provider.kind() == kind)
            .cloned()
    }
}
