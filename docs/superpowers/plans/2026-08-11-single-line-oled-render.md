# Single-Line OLED Render Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render a lone visual line on the 128x32 SSD1306 with the largest fitting centered font while preserving the existing two-line layout and protocol-v7 display behavior.

**Architecture:** The Host renderer remains responsible for line merging, font selection, and coordinates. `DeviceDisplayLink` limits the renderer to font 0 for protocol v7 and enables font IDs 0-2 for protocol v8; firmware validates those IDs and maps them to fixed U8g2 fonts without interpreting screen semantics.

**Tech Stack:** Rust/Tauri display renderer and serial protocol, C++17 firmware, U8g2 2.36.18, Cargo tests, PlatformIO native tests, RP2040 build.

## Global Constraints

- A visual row consists of the non-empty `row0_left` and `row0_right` values joined by exactly one ASCII space.
- Use a full `Rect::new(0, 0, 128, 32)` region only when exactly one visual row is non-empty.
- Font ID 2 is `u8g2_font_10x20_tf` with 10px advance and baseline 22; font ID 1 is `u8g2_font_9x18_tf` with 9px advance and baseline 21; font ID 0 is `u8g2_font_6x13_tf` with 6px advance and baseline 21 in single-line mode.
- Two-line scenes retain the existing three regions, font ID 0, and baselines 12 and 29.
- Protocol v7 retains the existing three-region renderer; protocol v8 enables adaptive single-line fonts.
- The basic display protocol floor remains 7 and the OLED configuration floor remains 4.
- Do not modify Device Profile YAML, ESP32-S3 behavior, text priority, or display copy.
- Preserve unrelated changes in `src/DeviceManagement.test.tsx` and `docs/superpowers/plans/2026-08-11-device-management-selection-flicker.md`.

---

### Task 1: Add Adaptive Host Layout

**Files:**
- Modify: `src-tauri/src/display/render.rs`
- Test: `src-tauri/src/display/render.rs`

**Interfaces:**
- Consumes: the existing `(left, right, bottom)` strings produced from `View`.
- Produces: `DisplayCapabilities::max_font_id: u8`, `DisplayRenderer::render_with_font_limit(&self, snapshot: &DisplaySnapshot, max_font_id: u8)`, and a single-region adaptive scene.

- [ ] **Step 1: Write failing renderer tests**

Add helpers that inspect one text operation and tests equivalent to:

```rust
#[test]
fn single_summary_row_uses_the_largest_fitting_centered_font() {
    let scene = MonoText128x32Renderer.render(&summary_snapshot(3, 0)).unwrap();
    assert_eq!(region_layout(&scene), vec![(0, "single_line", Rect::new(0, 0, 128, 32))]);
    assert_eq!(text_operation(&scene, "single_line"), Some((9, 22, 2, "CODEX 3 RUN")));
}

#[test]
fn single_row_falls_back_through_the_declared_font_sizes() {
    assert_eq!(single_line_region("CODEX 999+ RUN", 2).operations.last(),
               Some(&text(1, 21, "CODEX 999+ RUN")));
    assert_eq!(single_line_region("1234567890123456", 2).operations.last(),
               Some(&text(0, 21, "1234567890123456")));
}

#[test]
fn font_zero_limit_preserves_the_legacy_three_region_layout() {
    let scene = MonoText128x32Renderer
        .render_with_font_limit(&summary_snapshot(3, 0), 0)
        .unwrap();
    assert_eq!(region_layout(&scene), legacy_regions());
    assert_eq!(scene.text("row0_left"), "CODEX");
    assert_eq!(scene.text("row0_right"), "3 RUN");
}
```

- [ ] **Step 2: Run the renderer tests and confirm RED**

Run:

```bash
rtk cargo test --manifest-path src-tauri/Cargo.toml display::render::tests -- --nocapture
```

Expected: compilation failure because `max_font_id`, `render_with_font_limit`, and the adaptive single-line helpers do not exist.

- [ ] **Step 3: Implement the minimal adaptive layout**

Add fixed font metrics and the compatibility entry point:

```rust
const COMPACT_FONT: FontMetrics = FontMetrics { id: 0, advance: 6, baseline_y: 21 };
const MEDIUM_FONT: FontMetrics = FontMetrics { id: 1, advance: 9, baseline_y: 21 };
const LARGE_FONT: FontMetrics = FontMetrics { id: 2, advance: 10, baseline_y: 22 };

#[derive(Clone, Copy)]
struct FontMetrics {
    id: u8,
    advance: u16,
    baseline_y: u16,
}

pub(crate) trait DisplayRenderer: Send + Sync {
    fn render(&self, snapshot: &DisplaySnapshot) -> Result<RenderedScene, &'static str>;

    fn render_with_font_limit(
        &self,
        snapshot: &DisplaySnapshot,
        _max_font_id: u8,
    ) -> Result<RenderedScene, &'static str> {
        self.render(snapshot)
    }
}
```

Set `DisplayCapabilities::max_font_id` to 2. Override `render_with_font_limit` in `MonoText128x32Renderer`; have `render` call it with 2. Normalize the top row with one space, detect exactly one non-empty visual row, select the first fitting font from `[LARGE_FONT, MEDIUM_FONT, COMPACT_FONT]` whose ID is within the limit, and emit:

```rust
DisplayRegion::new(
    0,
    "single_line",
    Rect::new(0, 0, 128, 32),
    vec![
        DrawOperation::ClearRegion,
        DrawOperation::Text {
            x: (128 - text.len() as u16 * font.advance) / 2,
            baseline_y: font.baseline_y,
            font_id: font.id,
            text,
        },
    ],
)
```

When `max_font_id == 0` or two visual rows are non-empty, emit the current three regions unchanged.

- [ ] **Step 4: Run focused renderer tests and confirm GREEN**

Run:

```bash
rtk cargo test --manifest-path src-tauri/Cargo.toml display::render::tests -- --nocapture
```

Expected: all renderer tests pass.

- [ ] **Step 5: Commit the renderer behavior**

```bash
rtk git add src-tauri/src/display/render.rs
rtk git commit -m "feat: adapt single-line OLED layout"
```

### Task 2: Negotiate Large Fonts by Protocol Version

**Files:**
- Modify: `src-tauri/src/protocol.rs`
- Modify: `src-tauri/src/device.rs`
- Test: `src-tauri/src/protocol.rs`
- Test: `src-tauri/src/device.rs`

**Interfaces:**
- Consumes: `DisplayCapabilities::max_font_id` and `DisplayRenderer::render_with_font_limit` from Task 1.
- Produces: `DISPLAY_LARGE_FONT_PROTOCOL_VERSION: u16 = 8`; Host protocol validation for font IDs 0-2; per-link `max_font_id` selection.

- [ ] **Step 1: Write failing protocol and device-link tests**

Add protocol assertions that font IDs 1 and 2 encode successfully while 3 returns `display_font_unsupported`. Add a device-link compatibility test:

```rust
#[test]
fn protocol_seven_keeps_compact_layout_and_protocol_eight_uses_large_font() {
    let registry = built_in_renderer_registry();
    let mut v7 = DeviceDisplayLink::default();
    v7.configure(DISPLAY_PROTOCOL_VERSION, Some(&oled_runtime_model()), &registry);
    v7.update_desired(display_snapshot(3)).unwrap();
    let v7_lines = v7.next_lines(Instant::now()).unwrap();
    assert!(v7_lines.iter().any(|line| line.starts_with("DISPLAY_REGION 0 0 0 64 16")));

    let mut v8 = DeviceDisplayLink::default();
    v8.configure(DISPLAY_LARGE_FONT_PROTOCOL_VERSION, Some(&oled_runtime_model()), &registry);
    v8.update_desired(display_snapshot(3)).unwrap();
    let v8_lines = v8.next_lines(Instant::now()).unwrap();
    assert!(v8_lines.iter().any(|line| line.starts_with("DISPLAY_REGION 0 0 0 128 32")));
    assert!(v8_lines.iter().any(|line| line.contains(" 2 ")));
}
```

- [ ] **Step 2: Run focused Host tests and confirm RED**

```bash
rtk cargo test --manifest-path src-tauri/Cargo.toml protocol::tests::rejects_unsupported_or_oversized_display_text
rtk cargo test --manifest-path src-tauri/Cargo.toml device::tests::protocol_seven_keeps_compact_layout_and_protocol_eight_uses_large_font
```

Expected: font IDs 1-2 are rejected and the v8 constant/link behavior is missing.

- [ ] **Step 3: Implement protocol-aware font limits**

In `protocol.rs`, keep `DISPLAY_PROTOCOL_VERSION` at 7, set `HOST_PROTOCOL_VERSION` to 8, add `DISPLAY_LARGE_FONT_PROTOCOL_VERSION` at 8, replace the single allowed font constant with:

```rust
const DISPLAY_MAX_FONT_ID: u8 = 2;
```

Reject only `font_id > DISPLAY_MAX_FONT_ID`.

In `DeviceDisplayLink`, store `max_font_id: u8`. During configuration choose 0 for protocol 7 and `renderer.capabilities().max_font_id` for protocol 8+, include that value in the early-return comparison, reset it on disconnect, and call:

```rust
renderer.render_with_font_limit(snapshot, self.max_font_id)
```

- [ ] **Step 4: Run Host display/protocol/device tests and confirm GREEN**

```bash
rtk cargo test --manifest-path src-tauri/Cargo.toml display::
rtk cargo test --manifest-path src-tauri/Cargo.toml protocol::tests
rtk cargo test --manifest-path src-tauri/Cargo.toml device::tests
```

Expected: all selected Rust tests pass.

- [ ] **Step 5: Commit Host negotiation**

```bash
rtk git add src-tauri/src/protocol.rs src-tauri/src/device.rs
rtk git commit -m "feat: negotiate OLED large fonts"
```

### Task 3: Accept and Render Three Firmware Fonts

**Files:**
- Modify: `lib/gpio_trigger/src/Handshake.cpp`
- Modify: `lib/gpio_trigger/src/RemoteDisplay.h`
- Modify: `lib/gpio_trigger/src/RemoteDisplay.cpp`
- Modify: `src/platform/rp2040.cpp`
- Modify: `test/test_gpio_trigger/test_main.cpp`
- Modify: `test/test_release.sh`

**Interfaces:**
- Consumes: Host font IDs 0, 1, and 2 from Task 2.
- Produces: firmware HELLO protocol 8; bounded font validation; RP2040 font lookup.

- [ ] **Step 1: Write failing firmware contract tests**

Change the RemoteDisplay test to accept IDs 0, 1, and 2 and reject 3:

```cpp
for (std::uint8_t fontId = 0; fontId <= kRemoteDisplayMaxFontId; ++fontId) {
  RemoteDisplay display;
  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(1, 0, DisplayMode::Full));
  TEST_ASSERT_TRUE(display.region(0, {0, 0, 128, 32}));
  TEST_ASSERT_TRUE(display.text(0, 0, 21, fontId, "FONT"));
}
RemoteDisplay unsupported;
TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                  unsupported.begin(1, 0, DisplayMode::Full));
TEST_ASSERT_TRUE(unsupported.region(0, {0, 0, 128, 32}));
TEST_ASSERT_FALSE(unsupported.text(0, 0, 21, 3, "FONT"));
```

Add a RemoteDisplay transition test that commits a full-screen slot 0, then replaces it in a delta
with the compact slot 0 plus slots 1 and 2. Assert the commit's dirty bounds cover the union of the
old full-screen bounds and the three new bounds, so both layout directions clear all 128x32 pixels.
Update the HELLO expectation to protocol 8. In `test_release.sh`, require all three U8g2 symbols in
`rp2040.cpp`.

- [ ] **Step 2: Run native and release tests and confirm RED**

```bash
rtk direnv exec . uv run pio test -e native
rtk direnv exec . bash test/test_release.sh
```

Expected: native font validation and HELLO assertions fail; release checks fail because the large-font mapping is absent.

- [ ] **Step 3: Implement firmware validation and RP2040 mapping**

Rename the firmware bound to:

```cpp
constexpr std::uint8_t kRemoteDisplayMaxFontId = 2;
```

Accept `fontId <= kRemoteDisplayMaxFontId` in `RemoteDisplay::text` and `supportsRemoteScene`. Change `formatHello` to emit `HELLO 8`. Add a total font lookup in `rp2040.cpp`:

```cpp
const std::uint8_t *remoteDisplayFont(std::uint8_t fontId) {
  switch (fontId) {
    case 0: return u8g2_font_6x13_tf;
    case 1: return u8g2_font_9x18_tf;
    case 2: return u8g2_font_10x20_tf;
    default: return nullptr;
  }
}
```

Use the lookup both when validating and immediately before `drawStr`; reject the scene if lookup returns null.

- [ ] **Step 4: Run firmware tests and RP2040 build and confirm GREEN**

```bash
rtk direnv exec . uv run pio test -e native
rtk direnv exec . bash test/test_release.sh
rtk direnv exec . make build-rp2040
```

Expected: native tests, release contract, and RP2040 firmware build pass.

- [ ] **Step 5: Commit firmware font support**

```bash
rtk git add lib/gpio_trigger/src/Handshake.cpp lib/gpio_trigger/src/RemoteDisplay.h lib/gpio_trigger/src/RemoteDisplay.cpp src/platform/rp2040.cpp test/test_gpio_trigger/test_main.cpp test/test_release.sh
rtk git commit -m "feat: render adaptive OLED fonts"
```

### Task 4: Run the Full Acceptance Gate

**Files:**
- Verify only: all files changed in Tasks 1-3

**Interfaces:**
- Consumes: adaptive Host scenes, protocol negotiation, firmware validation, and U8g2 font mapping.
- Produces: fresh repository-wide automated verification evidence.

- [ ] **Step 1: Run the complete acceptance gate**

```bash
rtk direnv exec . make test
rtk direnv exec . make build-rp2040
rtk git diff --check
```

Expected: all repository tests, lint/build targets, RP2040 firmware build, and whitespace validation pass.

After automated verification, report physical OLED centering, readability, spacing, and residual-pixel inspection as **Not Run** unless a device upload and visual check are actually performed.
