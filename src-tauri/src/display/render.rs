use std::{cmp::Reverse, collections::BTreeMap, sync::Arc, time::Instant};

use super::{DisplayItem, DisplaySnapshot, DisplayState, SourceHealth};

const PANEL_ID: &str = "ssd1306_128x32_mono";
const SUMMARY_ID: &str = "codex.summary";
const EMPTY_TEXT: &str = "";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Rect {
    pub(crate) const fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DrawOperation {
    ClearRegion,
    Text {
        x: u16,
        baseline_y: u16,
        font_id: u8,
        text: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DisplayRegion {
    pub slot: u8,
    pub id: &'static str,
    pub bounds: Rect,
    pub content_hash: u64,
    pub operations: Vec<DrawOperation>,
}

impl DisplayRegion {
    pub(crate) fn clear(slot: u8, id: &'static str, bounds: Rect) -> Self {
        Self::new(slot, id, bounds, vec![DrawOperation::ClearRegion])
    }

    pub(crate) fn new(
        slot: u8,
        id: &'static str,
        bounds: Rect,
        operations: Vec<DrawOperation>,
    ) -> Self {
        let content_hash = content_hash(slot, bounds, &operations);
        Self {
            slot,
            id,
            bounds,
            content_hash,
            operations,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RenderedScene {
    pub regions: Vec<DisplayRegion>,
}

impl RenderedScene {
    pub(crate) fn text(&self, id: &str) -> &str {
        self.regions
            .iter()
            .find(|region| region.id == id)
            .and_then(|region| {
                region
                    .operations
                    .iter()
                    .find_map(|operation| match operation {
                        DrawOperation::Text { text, .. } => Some(text.as_str()),
                        DrawOperation::ClearRegion => None,
                    })
            })
            .unwrap_or(EMPTY_TEXT)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DisplayCapabilities {
    pub width: u16,
    pub height: u16,
    pub ascii_font_id: u8,
    pub max_regions: u8,
    pub max_operations: u8,
    pub max_text_bytes: u8,
    pub tile_width: u8,
    pub tile_height: u8,
}

impl DisplayCapabilities {
    pub(crate) const fn ssd1306_128x32_mono() -> Self {
        Self {
            width: 128,
            height: 32,
            ascii_font_id: 0,
            max_regions: 8,
            max_operations: 24,
            max_text_bytes: 48,
            tile_width: 8,
            tile_height: 8,
        }
    }
}

pub(crate) trait DisplayRenderer: Send + Sync {
    fn panel_id(&self) -> &'static str;
    fn capabilities(&self) -> &DisplayCapabilities;
    fn render(&self, snapshot: &DisplaySnapshot) -> Result<RenderedScene, &'static str>;
}

#[derive(Default)]
pub(crate) struct RendererRegistry {
    renderers: BTreeMap<&'static str, Arc<dyn DisplayRenderer>>,
}

impl RendererRegistry {
    pub(crate) fn register(
        &mut self,
        renderer: Arc<dyn DisplayRenderer>,
    ) -> Result<(), &'static str> {
        let panel_id = renderer.panel_id();
        if self.renderers.contains_key(panel_id) {
            return Err("display_renderer_duplicate_panel");
        }
        self.renderers.insert(panel_id, renderer);
        Ok(())
    }

    pub(crate) fn renderer(
        &self,
        panel_id: &str,
    ) -> Result<Arc<dyn DisplayRenderer>, &'static str> {
        self.renderers
            .get(panel_id)
            .cloned()
            .ok_or("display_renderer_unsupported")
    }

    pub(crate) fn panel_ids(&self) -> Vec<&'static str> {
        self.renderers.keys().copied().collect()
    }
}

pub(crate) fn built_in_renderer_registry() -> RendererRegistry {
    let mut registry = RendererRegistry::default();
    registry
        .register(Arc::new(MonoText128x32Renderer))
        .expect("built-in renderer panel IDs are unique");
    registry
}

pub(crate) struct MonoText128x32Renderer;

impl DisplayRenderer for MonoText128x32Renderer {
    fn panel_id(&self) -> &'static str {
        PANEL_ID
    }

    fn capabilities(&self) -> &DisplayCapabilities {
        static CAPABILITIES: DisplayCapabilities = DisplayCapabilities::ssd1306_128x32_mono();
        &CAPABILITIES
    }

    fn render(&self, snapshot: &DisplaySnapshot) -> Result<RenderedScene, &'static str> {
        let (left, right, bottom) = match select_view(snapshot) {
            View::Task { label, message } => {
                let (left, right) = split_label(&label);
                (left, right, message)
            }
            View::Summary {
                running,
                needs_input,
            } => (
                "CODEX".to_owned(),
                format!("{running} RUN"),
                if needs_input == 0 {
                    String::new()
                } else {
                    format!("{needs_input} NEEDS INPUT")
                },
            ),
            View::Offline => (
                "CODEX".to_owned(),
                "OFFLINE".to_owned(),
                "KIVO READY".to_owned(),
            ),
            View::Idle => (
                "CODEX".to_owned(),
                "IDLE".to_owned(),
                "KIVO READY".to_owned(),
            ),
        };

        Ok(RenderedScene {
            regions: vec![
                text_region(0, "row0_left", Rect::new(0, 0, 64, 16), left),
                text_region(1, "row0_right", Rect::new(64, 0, 64, 16), right),
                text_region(2, "row1", Rect::new(0, 16, 128, 16), bottom),
            ],
        })
    }
}

enum View {
    Task { label: String, message: String },
    Summary { running: u32, needs_input: u32 },
    Offline,
    Idle,
}

fn select_view(snapshot: &DisplaySnapshot) -> View {
    if let Some(item) = newest(snapshot, |item| item.state == DisplayState::NeedsInput) {
        return task_view(
            item,
            if item.detail.as_deref() == Some("approval needed") {
                "APPROVAL NEEDED"
            } else {
                "NEEDS INPUT"
            },
        );
    }
    if let Some(item) = newest(snapshot, |item| item.state == DisplayState::Error) {
        return task_view(item, "CODEX ERROR");
    }
    if let Some(item) = newest(snapshot, |item| {
        item.state == DisplayState::Success
            && item
                .expires_at
                .is_none_or(|expires_at| Instant::now() < expires_at)
    }) {
        return task_view(item, "RESPONSE READY");
    }
    if let Some(item) = newest(snapshot, |item| item.state == DisplayState::Warning) {
        return task_view(item, "TASK STOPPED");
    }

    match snapshot.health("codex") {
        SourceHealth::Offline | SourceHealth::Stale => View::Offline,
        SourceHealth::Healthy | SourceHealth::Degraded => summary_view(snapshot),
    }
}

fn newest(
    snapshot: &DisplaySnapshot,
    matches: impl Fn(&DisplayItem) -> bool,
) -> Option<&DisplayItem> {
    snapshot
        .items
        .iter()
        .filter(|item| item.id != SUMMARY_ID && matches(item))
        .max_by_key(|item| (item.updated_at, Reverse(item.id.as_str())))
}

fn task_view(item: &DisplayItem, message: &str) -> View {
    View::Task {
        label: ascii_project_title(&item.title, thread_id(item)),
        message: message.to_owned(),
    }
}

fn summary_view(snapshot: &DisplaySnapshot) -> View {
    let Some(summary) = snapshot.items.iter().find(|item| item.id == SUMMARY_ID) else {
        return View::Idle;
    };
    let running = summary.metrics.get("running").copied().unwrap_or(0);
    let needs_input = summary.metrics.get("needs_input").copied().unwrap_or(0);
    if running == 0 && needs_input == 0 {
        View::Idle
    } else {
        View::Summary {
            running,
            needs_input,
        }
    }
}

fn thread_id(item: &DisplayItem) -> &str {
    item.id
        .strip_prefix("codex.task.")
        .unwrap_or(item.id.as_str())
}

pub(crate) fn ascii_project_title(project: &str, thread_id: &str) -> String {
    let mut title = String::new();
    let mut previous_was_space = true;
    for byte in project.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_') {
            title.push((byte as char).to_ascii_uppercase());
            previous_was_space = false;
        } else if !previous_was_space {
            title.push(' ');
            previous_was_space = true;
        }
    }
    while title.ends_with(' ') {
        title.pop();
    }
    if title.is_empty() {
        let suffix: String = thread_id
            .bytes()
            .filter(u8::is_ascii_alphanumeric)
            .map(|byte| (byte as char).to_ascii_uppercase())
            .take(4)
            .collect();
        title = format!("TASK {suffix}");
    }
    title.chars().take(16).collect()
}

fn split_label(label: &str) -> (String, String) {
    let mut chars = label.chars();
    (chars.by_ref().take(8).collect(), chars.take(8).collect())
}

fn text_region(slot: u8, id: &'static str, bounds: Rect, text: String) -> DisplayRegion {
    let mut operations = vec![DrawOperation::ClearRegion];
    if !text.is_empty() {
        operations.push(DrawOperation::Text {
            x: bounds.x,
            baseline_y: bounds.y + 12,
            font_id: 0,
            text,
        });
    }
    DisplayRegion::new(slot, id, bounds, operations)
}

fn content_hash(slot: u8, bounds: Rect, operations: &[DrawOperation]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    hash_byte(&mut hash, slot);
    for coordinate in [bounds.x, bounds.y, bounds.width, bounds.height] {
        hash_bytes(&mut hash, &coordinate.to_le_bytes());
    }
    for operation in operations {
        match operation {
            DrawOperation::ClearRegion => hash_byte(&mut hash, 0),
            DrawOperation::Text {
                x,
                baseline_y,
                font_id,
                text,
            } => {
                hash_byte(&mut hash, 1);
                hash_bytes(&mut hash, &x.to_le_bytes());
                hash_bytes(&mut hash, &baseline_y.to_le_bytes());
                hash_byte(&mut hash, *font_id);
                hash_bytes(&mut hash, &(text.len() as u32).to_le_bytes());
                hash_bytes(&mut hash, text.as_bytes());
            }
        }
    }
    hash
}

const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x00000100000001b3;

fn hash_byte(hash: &mut u64, byte: u8) {
    *hash ^= u64::from(byte);
    *hash = hash.wrapping_mul(FNV_PRIME);
}

fn hash_bytes(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        hash_byte(hash, *byte);
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        time::{Duration, Instant},
    };

    use super::*;
    use crate::display::{DisplayItem, DisplayPriority};

    fn snapshot(item: DisplayItem) -> DisplaySnapshot {
        DisplaySnapshot {
            items: vec![item],
            health: BTreeMap::from([("codex".to_owned(), SourceHealth::Healthy)]),
        }
    }

    #[test]
    fn approval_requests_override_newer_errors() {
        let now = Instant::now();
        let approval = DisplayItem::new(
            "codex.task.a3f2-rest",
            "codex",
            DisplayPriority::Critical,
            DisplayState::NeedsInput,
            "kivo",
        )
        .unwrap()
        .with_detail("approval needed")
        .with_updated_at(now);
        let error = DisplayItem::new(
            "codex.task.error",
            "codex",
            DisplayPriority::Critical,
            DisplayState::Error,
            "other",
        )
        .unwrap()
        .with_updated_at(now + Duration::from_secs(1));
        let scene = MonoText128x32Renderer
            .render(&DisplaySnapshot {
                items: vec![approval, error],
                health: BTreeMap::from([("codex".to_owned(), SourceHealth::Healthy)]),
            })
            .unwrap();

        assert_eq!(scene.text("row0_left"), "KIVO");
        assert_eq!(scene.text("row1"), "APPROVAL NEEDED");
    }

    #[test]
    fn expired_success_falls_back_to_idle() {
        let now = Instant::now();
        let item = DisplayItem::new(
            "codex.task.a3f2-rest",
            "codex",
            DisplayPriority::Attention,
            DisplayState::Success,
            "kivo",
        )
        .unwrap()
        .with_updated_at(now)
        .with_expiry(now - Duration::from_secs(1));

        let scene = MonoText128x32Renderer.render(&snapshot(item)).unwrap();

        assert_eq!(scene.text("row0_right"), "IDLE");
    }

    #[test]
    fn registry_rejects_duplicate_panel_ids() {
        let mut registry = RendererRegistry::default();
        registry.register(Arc::new(MonoText128x32Renderer)).unwrap();

        assert_eq!(
            registry
                .register(Arc::new(MonoText128x32Renderer))
                .unwrap_err(),
            "display_renderer_duplicate_panel"
        );
        assert_eq!(
            registry.renderer("unknown").map(|_| ()),
            Err("display_renderer_unsupported")
        );
    }

    #[test]
    fn content_hash_changes_with_text_and_stays_stable() {
        let bounds = Rect::new(0, 0, 64, 16);
        let first = text_region(0, "row0_left", bounds, "CODEX".into());
        let same = text_region(0, "row0_left", bounds, "CODEX".into());
        let changed = text_region(0, "row0_left", bounds, "KIVO".into());

        assert_eq!(first.content_hash, same.content_hash);
        assert_ne!(first.content_hash, changed.content_hash);
    }
}
