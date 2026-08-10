use std::path::Path;

mod codex_events;
mod codex_provider;
mod codex_source;
mod hub;
mod model;
mod provider;

pub(crate) use codex_events::{CodexInputNeed, CodexTaskSnapshot, CodexTerminalEvent};
#[allow(unused_imports)]
pub(crate) use codex_source::{CodexSourceSnapshot, CodexTaskReader, MergedCodexTask};
#[allow(unused_imports)]
pub(crate) use hub::DisplayHub;
#[allow(unused_imports)]
pub(crate) use model::{DisplayItem, DisplayPriority, DisplaySnapshot, DisplayState, SourceHealth};
#[allow(unused_imports)]
pub(crate) use provider::{DisplayProvider, ProviderRegistry, ProviderUpdate};

pub(crate) fn built_in_provider_registry(
    app_home_fallback: &Path,
    cursor_store_path: &Path,
) -> Result<ProviderRegistry, &'static str> {
    let metadata = codex_source::SystemCodexMetadataClient::new(app_home_fallback.join(".codex"));
    let source = codex_source::CodexTaskSource::new(
        Box::new(metadata),
        app_home_fallback,
        cursor_store_path,
    )?;
    let mut registry = ProviderRegistry::default();
    registry.register(Box::new(codex_provider::CodexDisplayProvider::new(source)))?;
    Ok(registry)
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::built_in_provider_registry;

    #[test]
    fn built_in_registry_contains_exactly_the_codex_provider() {
        let temp = TempDir::new().unwrap();
        let registry = built_in_provider_registry(
            temp.path(),
            &temp.path().join("display/codex-cursors-v1.json"),
        )
        .unwrap();

        assert_eq!(registry.source_ids(), ["codex"]);
    }
}
