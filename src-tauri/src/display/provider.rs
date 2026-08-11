use std::time::Instant;

use super::{DisplayItem, SourceHealth};

pub struct ProviderUpdate {
    pub source: &'static str,
    pub health: SourceHealth,
    pub items: Vec<DisplayItem>,
}

pub trait DisplayProvider: Send {
    fn source_id(&self) -> &'static str;
    fn poll(&mut self, now: Instant) -> Result<ProviderUpdate, &'static str>;
}

#[derive(Default)]
pub struct ProviderRegistry {
    providers: Vec<Box<dyn DisplayProvider>>,
}

impl ProviderRegistry {
    pub fn register(&mut self, provider: Box<dyn DisplayProvider>) -> Result<(), &'static str> {
        if self
            .providers
            .iter()
            .any(|existing| existing.source_id() == provider.source_id())
        {
            return Err("display_provider_duplicate");
        }
        self.providers.push(provider);
        Ok(())
    }

    pub fn providers_mut(
        &mut self,
    ) -> impl Iterator<Item = &mut (dyn DisplayProvider + 'static)> + '_ {
        self.providers.iter_mut().map(Box::as_mut)
    }

    pub fn source_ids(&self) -> Vec<&'static str> {
        self.providers
            .iter()
            .map(|provider| provider.source_id())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::{DisplayProvider, ProviderRegistry, ProviderUpdate};

    struct FakeProvider {
        source: &'static str,
    }

    impl FakeProvider {
        fn new(source: &'static str) -> Self {
            Self { source }
        }
    }

    impl DisplayProvider for FakeProvider {
        fn source_id(&self) -> &'static str {
            self.source
        }

        fn poll(&mut self, _now: Instant) -> Result<ProviderUpdate, &'static str> {
            unreachable!()
        }
    }

    #[test]
    fn provider_registry_rejects_duplicate_source_ids() {
        let mut registry = ProviderRegistry::default();
        registry
            .register(Box::new(FakeProvider::new("codex")))
            .unwrap();
        assert_eq!(
            registry
                .register(Box::new(FakeProvider::new("codex")))
                .unwrap_err(),
            "display_provider_duplicate"
        );
    }

    #[test]
    fn provider_registry_preserves_registration_order() {
        let mut registry = ProviderRegistry::default();
        registry
            .register(Box::new(FakeProvider::new("codex")))
            .unwrap();
        registry
            .register(Box::new(FakeProvider::new("other")))
            .unwrap();

        let sources = registry
            .providers_mut()
            .map(|provider| provider.source_id())
            .collect::<Vec<_>>();
        assert_eq!(sources, ["codex", "other"]);
    }
}
