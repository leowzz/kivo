use std::{cmp::Reverse, collections::BTreeMap, sync::Arc};

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
    pub pixel_format: PixelFormat,
    pub rotation_degrees: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PixelFormat {
    Mono1,
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
            pixel_format: PixelFormat::Mono1,
            rotation_degrees: 0,
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
            View::Task {
                label,
                message,
                additional_waits,
            } => {
                let (left, right) = split_task_label(&label, additional_waits);
                (left, right, message)
            }
            View::Summary {
                running,
                needs_input,
            } => (
                "CODEX".to_owned(),
                format!("{} RUN", compact_count(running)),
                if needs_input == 0 {
                    String::new()
                } else {
                    format!("{} NEEDS INPUT", compact_count(needs_input))
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
    Task {
        label: String,
        message: String,
        additional_waits: u32,
    },
    Summary {
        running: u32,
        needs_input: u32,
    },
    Offline,
    Idle,
}

fn select_view(snapshot: &DisplaySnapshot) -> View {
    if let Some(item) = newest(snapshot, |item| item.state == DisplayState::NeedsInput) {
        let additional_waits = snapshot
            .items
            .iter()
            .filter(|item| item.id != SUMMARY_ID && item.state == DisplayState::NeedsInput)
            .count()
            .saturating_sub(1)
            .try_into()
            .unwrap_or(u32::MAX);
        return task_view(
            item,
            if item.detail.as_deref() == Some("approval needed") {
                "APPROVAL NEEDED"
            } else {
                "NEEDS INPUT"
            },
            additional_waits,
        );
    }
    if let Some(item) = newest(snapshot, |item| item.state == DisplayState::Error) {
        return task_view(item, "CODEX ERROR", 0);
    }
    if let Some(item) = newest(snapshot, |item| item.state == DisplayState::Success) {
        return task_view(item, "RESPONSE READY", 0);
    }
    if let Some(item) = newest(snapshot, |item| item.state == DisplayState::Warning) {
        return task_view(item, "TASK STOPPED", 0);
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

fn task_view(item: &DisplayItem, message: &str, additional_waits: u32) -> View {
    View::Task {
        label: ascii_project_title(&item.title, thread_id(item)),
        message: message.to_owned(),
        additional_waits,
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

fn compact_count(count: u32) -> String {
    if count > 999 {
        "999+".to_owned()
    } else {
        count.to_string()
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

fn split_task_label(label: &str, additional_waits: u32) -> (String, String) {
    if additional_waits == 0 {
        return split_label(label);
    }
    let indicator = format!("+{}", compact_count(additional_waits));
    let mut chars = label.chars();
    let left = chars.by_ref().take(8).collect();
    let right_capacity = 8usize.saturating_sub(indicator.len());
    let label_suffix: String = chars.take(right_capacity.saturating_sub(1)).collect();
    let right = if label_suffix.is_empty() {
        indicator
    } else {
        format!("{label_suffix} {indicator}")
    };
    (left, right)
}

fn text_region(slot: u8, id: &'static str, bounds: Rect, text: String) -> DisplayRegion {
    let mut operations = vec![DrawOperation::ClearRegion];
    if !text.is_empty() {
        operations.push(DrawOperation::Text {
            x: bounds.x,
            baseline_y: if bounds.y == 0 { 12 } else { 29 },
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
    fn renderer_trusts_the_snapshot_even_when_success_expiry_is_past() {
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

        assert_eq!(scene.text("row1"), "RESPONSE READY");
    }

    #[test]
    fn row_text_operations_use_the_exact_baselines() {
        let summary = DisplayItem::new(
            "codex.summary",
            "codex",
            DisplayPriority::Ambient,
            DisplayState::Running,
            "Codex",
        )
        .unwrap()
        .with_metric("running", 1)
        .with_metric("needs_input", 1);
        let scene = MonoText128x32Renderer
            .render(&DisplaySnapshot {
                items: vec![summary],
                health: BTreeMap::from([("codex".to_owned(), SourceHealth::Healthy)]),
            })
            .unwrap();

        assert_eq!(text_position(&scene, "row0_left"), Some((0, 12)));
        assert_eq!(text_position(&scene, "row0_right"), Some((64, 12)));
        assert_eq!(text_position(&scene, "row1"), Some((0, 29)));
    }

    #[test]
    fn capabilities_declare_mono_one_bit_pixels_without_rotation() {
        let capabilities = MonoText128x32Renderer.capabilities();

        assert_eq!(capabilities.pixel_format, PixelFormat::Mono1);
        assert_eq!(capabilities.rotation_degrees, 0);
    }

    #[test]
    fn summary_counts_use_compact_width_bounded_text() {
        let summary = DisplayItem::new(
            "codex.summary",
            "codex",
            DisplayPriority::Ambient,
            DisplayState::Running,
            "Codex",
        )
        .unwrap()
        .with_metric("running", u32::MAX)
        .with_metric("needs_input", u32::MAX);
        let scene = MonoText128x32Renderer
            .render(&DisplaySnapshot {
                items: vec![summary],
                health: BTreeMap::from([("codex".to_owned(), SourceHealth::Healthy)]),
            })
            .unwrap();

        assert_eq!(scene.text("row0_right"), "999+ RUN");
        assert_eq!(scene.text("row1"), "999+ NEEDS INPUT");
        assert!(scene.text("row0_right").len() <= 8);
        assert!(scene.text("row1").len() <= 16);
    }

    #[test]
    fn raw_unicode_project_title_uses_thread_id_fallback_in_the_renderer() {
        let item = DisplayItem::new(
            "codex.task.a3f2-rest",
            "codex",
            DisplayPriority::Critical,
            DisplayState::Error,
            "中文项目",
        )
        .unwrap();

        let scene = MonoText128x32Renderer.render(&snapshot(item)).unwrap();

        assert_eq!(scene.text("row0_left"), "TASK A3F");
        assert_eq!(scene.text("row0_right"), "2");
        assert_eq!(scene.text("row1"), "CODEX ERROR");
    }

    #[test]
    fn golden_views_cover_terminal_health_and_priority_selection() {
        let now = Instant::now();
        let task = |id: &str, state, title: &str| {
            DisplayItem::new(id, "codex", DisplayPriority::Attention, state, title)
                .unwrap()
                .with_updated_at(now)
        };
        let cases = vec![
            (
                DisplaySnapshot {
                    items: vec![task("codex.task.error", DisplayState::Error, "error")],
                    health: BTreeMap::from([("codex".to_owned(), SourceHealth::Healthy)]),
                },
                ["ERROR", "", "CODEX ERROR"],
            ),
            (
                DisplaySnapshot {
                    items: vec![task("codex.task.ready", DisplayState::Success, "ready")],
                    health: BTreeMap::from([("codex".to_owned(), SourceHealth::Healthy)]),
                },
                ["READY", "", "RESPONSE READY"],
            ),
            (
                DisplaySnapshot {
                    items: vec![task("codex.task.stopped", DisplayState::Warning, "stopped")],
                    health: BTreeMap::from([("codex".to_owned(), SourceHealth::Healthy)]),
                },
                ["STOPPED", "", "TASK STOPPED"],
            ),
            (
                DisplaySnapshot {
                    items: vec![],
                    health: BTreeMap::from([("codex".to_owned(), SourceHealth::Offline)]),
                },
                ["CODEX", "OFFLINE", "KIVO READY"],
            ),
            (
                DisplaySnapshot {
                    items: vec![
                        DisplayItem::new(
                            "codex.summary",
                            "codex",
                            DisplayPriority::Ambient,
                            DisplayState::Idle,
                            "Codex",
                        )
                        .unwrap(),
                    ],
                    health: BTreeMap::from([("codex".to_owned(), SourceHealth::Healthy)]),
                },
                ["CODEX", "IDLE", "KIVO READY"],
            ),
            (
                DisplaySnapshot {
                    items: vec![
                        task("codex.task.error", DisplayState::Error, "error")
                            .with_updated_at(now + Duration::from_secs(1)),
                        task("codex.task.input", DisplayState::NeedsInput, "input"),
                    ],
                    health: BTreeMap::from([("codex".to_owned(), SourceHealth::Healthy)]),
                },
                ["INPUT", "", "NEEDS INPUT"],
            ),
        ];

        for (snapshot, expected) in cases {
            let scene = MonoText128x32Renderer.render(&snapshot).unwrap();
            assert_eq!(
                [
                    scene.text("row0_left"),
                    scene.text("row0_right"),
                    scene.text("row1"),
                ],
                expected
            );
        }
    }

    #[test]
    fn error_outranks_a_newer_success() {
        let now = Instant::now();
        let error = DisplayItem::new(
            "codex.task.error",
            "codex",
            DisplayPriority::Critical,
            DisplayState::Error,
            "error",
        )
        .unwrap()
        .with_updated_at(now);
        let success = DisplayItem::new(
            "codex.task.success",
            "codex",
            DisplayPriority::Attention,
            DisplayState::Success,
            "success",
        )
        .unwrap()
        .with_updated_at(now + Duration::from_secs(1));

        let scene = MonoText128x32Renderer
            .render(&DisplaySnapshot {
                items: vec![error, success],
                health: BTreeMap::from([("codex".to_owned(), SourceHealth::Healthy)]),
            })
            .unwrap();

        assert_eq!(scene.text("row1"), "CODEX ERROR");
    }

    #[test]
    fn success_outranks_a_newer_warning() {
        let now = Instant::now();
        let success = DisplayItem::new(
            "codex.task.success",
            "codex",
            DisplayPriority::Attention,
            DisplayState::Success,
            "success",
        )
        .unwrap()
        .with_updated_at(now);
        let warning = DisplayItem::new(
            "codex.task.warning",
            "codex",
            DisplayPriority::Attention,
            DisplayState::Warning,
            "warning",
        )
        .unwrap()
        .with_updated_at(now + Duration::from_secs(1));

        let scene = MonoText128x32Renderer
            .render(&DisplaySnapshot {
                items: vec![success, warning],
                health: BTreeMap::from([("codex".to_owned(), SourceHealth::Healthy)]),
            })
            .unwrap();

        assert_eq!(scene.text("row1"), "RESPONSE READY");
    }

    #[test]
    fn two_waiting_tasks_show_the_newest_short_label_with_plus_one() {
        let now = Instant::now();
        let older = waiting_item("codex.task.older", "other", now);
        let newer = waiting_item("codex.task.newer", "kivo", now + Duration::from_secs(1));

        let scene = MonoText128x32Renderer
            .render(&DisplaySnapshot {
                items: vec![older, newer],
                health: BTreeMap::from([("codex".to_owned(), SourceHealth::Healthy)]),
            })
            .unwrap();

        assert_eq!(scene.text("row0_left"), "KIVO");
        assert_eq!(scene.text("row0_right"), "+1");
        assert_eq!(scene.text("row1"), "NEEDS INPUT");
    }

    #[test]
    fn multi_wait_indicator_reserves_bounded_space_after_a_long_label() {
        let now = Instant::now();
        let selected = waiting_item("codex.task.a", "1234567890ABCDEF", now);
        let other = waiting_item("codex.task.b", "other", now - Duration::from_secs(1));

        let scene = MonoText128x32Renderer
            .render(&DisplaySnapshot {
                items: vec![selected, other],
                health: BTreeMap::from([("codex".to_owned(), SourceHealth::Healthy)]),
            })
            .unwrap();

        assert_eq!(scene.text("row0_left"), "12345678");
        assert_eq!(scene.text("row0_right"), "90ABC +1");
        assert!(scene.text("row0_left").len() <= 8);
        assert!(scene.text("row0_right").len() <= 8);
    }

    #[test]
    fn equal_time_wait_selection_is_stable_by_id_and_input_order_independent() {
        let now = Instant::now();
        for items in [
            vec![
                waiting_item("codex.task.b", "beta", now),
                waiting_item("codex.task.a", "alpha", now),
            ],
            vec![
                waiting_item("codex.task.a", "alpha", now),
                waiting_item("codex.task.b", "beta", now),
            ],
        ] {
            let scene = MonoText128x32Renderer
                .render(&DisplaySnapshot {
                    items,
                    health: BTreeMap::from([("codex".to_owned(), SourceHealth::Healthy)]),
                })
                .unwrap();

            assert_eq!(scene.text("row0_left"), "ALPHA");
            assert_eq!(scene.text("row0_right"), "+1");
        }
    }

    #[test]
    fn large_multi_wait_counts_are_bounded() {
        let now = Instant::now();
        let items = (0..1001)
            .map(|index| {
                waiting_item(
                    &format!("codex.task.{index:04}"),
                    if index == 0 { "kivo" } else { "other" },
                    now,
                )
            })
            .collect();

        let scene = MonoText128x32Renderer
            .render(&DisplaySnapshot {
                items,
                health: BTreeMap::from([("codex".to_owned(), SourceHealth::Healthy)]),
            })
            .unwrap();

        assert_eq!(scene.text("row0_left"), "KIVO");
        assert_eq!(scene.text("row0_right"), "+999+");
        assert!(scene.text("row0_right").len() <= 8);
    }

    #[test]
    fn clearing_all_waits_returns_to_summary_without_changing_regions() {
        let now = Instant::now();
        let waiting_scene = MonoText128x32Renderer
            .render(&DisplaySnapshot {
                items: vec![
                    waiting_item("codex.task.a", "kivo", now),
                    waiting_item("codex.task.b", "other", now),
                ],
                health: BTreeMap::from([("codex".to_owned(), SourceHealth::Healthy)]),
            })
            .unwrap();
        let summary = DisplayItem::new(
            "codex.summary",
            "codex",
            DisplayPriority::Ambient,
            DisplayState::Running,
            "Codex",
        )
        .unwrap()
        .with_metric("running", 2)
        .with_metric("needs_input", 0);
        let summary_scene = MonoText128x32Renderer
            .render(&DisplaySnapshot {
                items: vec![summary],
                health: BTreeMap::from([("codex".to_owned(), SourceHealth::Healthy)]),
            })
            .unwrap();

        assert_eq!(summary_scene.text("row0_left"), "CODEX");
        assert_eq!(summary_scene.text("row0_right"), "2 RUN");
        assert_eq!(summary_scene.text("row1"), "");
        assert_eq!(region_layout(&waiting_scene), region_layout(&summary_scene));
    }

    fn waiting_item(id: &str, title: &str, updated_at: Instant) -> DisplayItem {
        DisplayItem::new(
            id,
            "codex",
            DisplayPriority::Attention,
            DisplayState::NeedsInput,
            title,
        )
        .unwrap()
        .with_updated_at(updated_at)
    }

    fn region_layout(scene: &RenderedScene) -> Vec<(u8, &'static str, Rect)> {
        scene
            .regions
            .iter()
            .map(|region| (region.slot, region.id, region.bounds))
            .collect()
    }

    fn text_position(scene: &RenderedScene, id: &str) -> Option<(u16, u16)> {
        scene
            .regions
            .iter()
            .find(|region| region.id == id)
            .and_then(|region| {
                region
                    .operations
                    .iter()
                    .find_map(|operation| match operation {
                        DrawOperation::Text { x, baseline_y, .. } => Some((*x, *baseline_y)),
                        DrawOperation::ClearRegion => None,
                    })
            })
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
