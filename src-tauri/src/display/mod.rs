use std::path::Path;

mod codex_events;
mod codex_provider;
mod codex_source;
mod hub;
mod model;
mod provider;
mod render;
mod scene;

pub(crate) use codex_events::{CodexInputNeed, CodexTaskSnapshot, CodexTerminalEvent};
#[allow(unused_imports)]
pub(crate) use codex_source::{CodexSourceSnapshot, CodexTaskReader, MergedCodexTask};
#[allow(unused_imports)]
pub(crate) use hub::DisplayHub;
#[allow(unused_imports)]
pub(crate) use model::{DisplayItem, DisplayPriority, DisplaySnapshot, DisplayState, SourceHealth};
#[allow(unused_imports)]
pub(crate) use provider::{DisplayProvider, ProviderRegistry, ProviderUpdate};
#[allow(unused_imports)]
pub(crate) use render::{
    DisplayCapabilities, DisplayRegion, DisplayRenderer, DrawOperation, MonoText128x32Renderer,
    PixelFormat, Rect, RenderedScene, RendererRegistry, ascii_project_title,
    built_in_renderer_registry,
};
#[allow(unused_imports)]
pub(crate) use scene::{SceneMode, SceneTracker, SceneUpdate};

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
    use std::{collections::BTreeMap, time::Instant};

    use tempfile::TempDir;

    use super::{
        DisplayItem, DisplayPriority, DisplayRenderer, DisplaySnapshot, DisplayState,
        MonoText128x32Renderer, Rect, SceneMode, SceneTracker, SourceHealth, ascii_project_title,
        built_in_provider_registry, built_in_renderer_registry,
    };

    fn snapshot(running: u32, needs_input: u32) -> DisplaySnapshot {
        let now = Instant::now();
        let summary = DisplayItem::new(
            "codex.summary",
            "codex",
            DisplayPriority::Ambient,
            DisplayState::Running,
            "Codex",
        )
        .unwrap()
        .with_metric("running", running)
        .with_metric("needs_input", needs_input)
        .with_updated_at(now);
        DisplaySnapshot {
            items: vec![summary],
            health: BTreeMap::from([("codex".to_owned(), SourceHealth::Healthy)]),
        }
    }

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

    #[test]
    fn renders_running_summary_into_three_tile_aligned_regions() {
        let scene = MonoText128x32Renderer.render(&snapshot(3, 1)).unwrap();
        assert_eq!(
            scene
                .regions
                .iter()
                .map(|r| (r.id, r.bounds))
                .collect::<Vec<_>>(),
            vec![
                ("row0_left", Rect::new(0, 0, 64, 16)),
                ("row0_right", Rect::new(64, 0, 64, 16)),
                ("row1", Rect::new(0, 16, 128, 16)),
            ]
        );
        assert_eq!(scene.text("row0_left"), "CODEX");
        assert_eq!(scene.text("row0_right"), "3 RUN");
        assert_eq!(scene.text("row1"), "1 NEEDS INPUT");
    }

    #[test]
    fn changing_only_running_count_emits_only_row0_right() {
        let mut tracker = SceneTracker::default();
        let first = tracker
            .prepare(MonoText128x32Renderer.render(&snapshot(3, 1)).unwrap())
            .unwrap();
        tracker.ack(first.new_revision).unwrap();
        let second = tracker
            .prepare(MonoText128x32Renderer.render(&snapshot(4, 1)).unwrap())
            .unwrap();
        assert_eq!(second.mode, SceneMode::Delta);
        assert_eq!(
            second.regions.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec!["row0_right"]
        );
    }

    #[test]
    fn non_ascii_or_empty_project_uses_thread_id_fallback() {
        assert_eq!(ascii_project_title("中文", "a3f2-rest"), "TASK A3F2");
    }

    #[test]
    fn built_in_renderer_registry_contains_only_the_v1_panel() {
        let registry = built_in_renderer_registry();
        assert_eq!(registry.panel_ids(), vec!["ssd1306_128x32_mono"]);
    }
}
