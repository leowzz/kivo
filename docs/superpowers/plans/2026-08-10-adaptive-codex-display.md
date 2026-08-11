# Kivo Adaptive Codex Display Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the RP2040 OLED debug-only screen with a dimension-independent Codex attention display that supports atomic region transactions and SSD1306 tile updates without regressing key scanning.

**Architecture:** A Rust display service reads Codex metadata plus rollout lifecycle events and reduces them through a semantic Hub. It broadcasts device-independent snapshots; each device worker selects a built-in Renderer from its panel ID, owns an acknowledged-scene tracker, and serializes only changed regions. Firmware stages bounded draw operations, commits them atomically to retained region state, arbitrates local errors over remote content, and flushes dirty SSD1306 tiles within the main-loop budget.

**Tech Stack:** Rust 2024, `serde_json`, `base64 0.22`, `notify 8.2`, Tauri 2, `std::process`/`std::sync::mpsc`, C++17, PlatformIO Unity native tests, Arduino RP2040, U8g2 2.36.18, line-oriented serial protocol.

## Global Constraints

- The approved design is `docs/superpowers/specs/2026-08-10-adaptive-codex-display-design.md`; implementation must not broaden its scope.
- V1 uses built-in Provider and Renderer registries only; do not add dynamic plugin loading, manifests, external Provider IPC, or an installation UI.
- V1 registers only `CodexDisplayProvider` and `ssd1306_128x32_mono`.
- Existing `Ssd1306Config { sda, scl }` maps internally to `ssd1306_128x32_mono`; do not migrate Device Profile YAML.
- Provider output stays semantic and Unicode-capable; only the 128x32 Renderer performs ASCII fallback, truncation, and layout.
- Codex integration is read-only: `thread/list` always uses `useStateDbOnly: true`; never call `thread/resume`, `turn/start`, approval responses, or mutation methods.
- Do not retain or log conversation text, reasoning, commands, tool arguments, file changes, or final reply bodies.
- A task completion means `RESPONSE READY`, not business success. Never render `SUCCESS` or `DONE` for Codex completion.
- `needs_input` persists until resolution; response-ready and interrupted events expire after 8 seconds; system errors expire after 15 seconds.
- Only explicit `waitingOnApproval` may render `APPROVAL NEEDED`; inactivity or a long-running tool must never be inferred as approval wait.
- Display protocol version is 7. Protocol 3-6 devices remain supported and receive no `DISPLAY_*` commands.
- Every protocol line remains shorter than 255 bytes. Decoded text is at most 48 bytes; one transaction is at most 8 regions and 24 draw operations.
- V1 firmware primitives are exactly `ClearRegion` and `Text`; do not add icons, progress bars, animation, or bitmap transfer.
- SSD1306 rotation 0 uses 8x8 tile updates. Other rotations and unsupported panel drivers fall back to full refresh.
- Firmware local configuration/runtime errors, learning mode, startup, and Helper disconnect always override remote content.
- An uncommitted or invalid display transaction must not modify retained remote state or the framebuffer.
- `DISPLAY_OK` means accepted and queued, not physically flushed.
- Every shell command run during implementation is prefixed with `rtk`.
- Preserve unrelated worktree changes and stage only the files named by the current task.
- Automated tests/builds do not replace physical OLED legibility, I2C byte-count, or missed-key acceptance.

## File Map

- Create `src-tauri/src/display/mod.rs`: module exports and built-in registry construction.
- Create `src-tauri/src/display/model.rs`: semantic items, source health, snapshots, and deterministic ordering types.
- Create `src-tauri/src/display/provider.rs`: Provider contract, updates, and the static built-in Provider registry.
- Create `src-tauri/src/display/hub.rs`: source replacement, TTL, stale/offline reduction, and deduplication.
- Create `src-tauri/src/display/codex_events.rs`: privacy-minimal rollout line parser and per-thread lifecycle index.
- Create `src-tauri/src/display/codex_source.rs`: App Server metadata client, filesystem watcher, and normalized Codex task snapshots.
- Create `src-tauri/src/display/codex_provider.rs`: Codex task-to-`DisplayItem` mapping.
- Create `src-tauri/src/display/render.rs`: Renderer contract/registry, `DisplayCapabilities`, the 128x32 Renderer, ASCII fallback, and stable regions.
- Create `src-tauri/src/display/scene.rs`: draw operations, region hashes, scene revisions, full/delta calculation, and ack tracking.
- Create `src-tauri/src/display/service.rs`: background Provider/Hub loop that emits semantic snapshots.
- Create `src-tauri/tests/fixtures/codex/rollout-lifecycle.jsonl`: synthetic rollout lifecycle fixture with no user content.
- Create `src-tauri/tests/fixtures/codex/thread-list-response.json`: synthetic App Server response fixture.
- Create `src-tauri/tests/fixtures/codex/app-server-v2-subset.json`: generated `ThreadStatus`/active-flag schema subset.
- Modify `src-tauri/Cargo.toml` and `src-tauri/Cargo.lock`: direct `base64 0.22` and stable `notify 8.2` dependencies.
- Modify `src-tauri/src/lib.rs`: start/stop display service and fan updates into `RuntimeCoordinator`.
- Modify `src-tauri/src/coordinator.rs`: `WorkerCommand::UpdateDisplay` and all-worker scene fan-out.
- Modify `src-tauri/src/device.rs`: per-device display link, protocol gating, scene coalescing, ack/resync handling, and serial output.
- Modify `src-tauri/src/protocol.rs`: protocol 7, display replies, and bounded display command encoding.
- Create `lib/gpio_trigger/src/RemoteDisplay.h`: bounded firmware scene/transaction types.
- Create `lib/gpio_trigger/src/RemoteDisplay.cpp`: base64 validation, staging, commit, revision checks, and retained slots.
- Create `lib/gpio_trigger/src/DisplayController.h`: local/remote display arbitration interface.
- Create `lib/gpio_trigger/src/DisplayController.cpp`: local override lifecycle and committed remote restoration.
- Create `lib/gpio_trigger/src/DirtyTiles.h`: pure 16x4 tile bitmap and bounded run scheduler.
- Create `lib/gpio_trigger/src/DirtyTiles.cpp`: region marking, coalescing, and byte-budget dequeue.
- Modify `lib/gpio_trigger/src/TriggerProtocol.h` and `.cpp`: parse display transaction commands and format replies.
- Modify `lib/gpio_trigger/src/Handshake.cpp`: firmware protocol 7 HELLO.
- Modify `src/platform/Platform.h`: remote scene apply, local frame apply, reset, and display service calls.
- Modify `src/platform/rp2040.cpp`: retained U8g2 framebuffer drawing and `updateDisplayArea` scheduling.
- Modify `src/platform/esp32s3.cpp`: no-op implementations for the extended display interface.
- Modify `src/main.cpp`: display transaction dispatch, local override transitions, disconnect reset, and per-loop display service.
- Modify `test/test_gpio_trigger/test_main.cpp`: firmware parser, transaction, arbitration, dirty tile, and protocol 7 tests.
- Modify `scripts/verify_runtime_firmware.py`: expect current firmware protocol 7.
- Modify `test/test_upload_targeting.py`, `test/test_release.sh`, and firmware protocol assertions: current-version fixtures become 7 while legacy compatibility fixtures remain 3-6.
- Modify `README.md`: Codex screen behavior, privacy boundary, compatibility fallback, and physical verification notes.

---

### Task 1: Add The Semantic Display Model And Hub

**Files:**
- Create: `src-tauri/src/display/mod.rs`
- Create: `src-tauri/src/display/model.rs`
- Create: `src-tauri/src/display/provider.rs`
- Create: `src-tauri/src/display/hub.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: Provider updates stamped with `std::time::Instant`.
- Produces: `DisplayPriority`, `DisplayState`, `DisplayItem`, `SourceHealth`, `ProviderUpdate`, `DisplayProvider`, `ProviderRegistry`, `DisplaySnapshot`, and `DisplayHub::{replace_source,snapshot}`.

- [ ] **Step 1: Register an empty display module and write failing model/Hub tests**

Add `mod display;` beside the other module declarations in `src-tauri/src/lib.rs`. In `display/model.rs`, write tests against these exact constructors and fields:

```rust
#[test]
fn rejects_progress_above_one_hundred() {
    let item = DisplayItem::new("codex.summary", "codex", DisplayPriority::Ambient,
        DisplayState::Running, "Codex").unwrap()
        .with_progress(101);
    assert_eq!(item.unwrap_err(), "display_progress_out_of_range");
}

#[test]
fn item_identity_is_source_plus_id() {
    let item = DisplayItem::new("codex.summary", "codex", DisplayPriority::Ambient,
        DisplayState::Running, "Codex").unwrap();
    assert_eq!(item.key(), ("codex", "codex.summary"));
}
```

In `display/hub.rs`, add deterministic-time tests:

```rust
#[test]
fn expires_transient_items_but_keeps_summary() {
    let now = Instant::now();
    let mut hub = DisplayHub::default();
    hub.replace_source(
        now,
        "codex",
        SourceHealth::Healthy,
        vec![summary(now), response_ready(now, now + Duration::from_secs(8))],
    );

    assert_eq!(hub.snapshot(now + Duration::from_secs(7)).items.len(), 2);
    assert_eq!(hub.snapshot(now + Duration::from_secs(8)).items, vec![summary(now)]);
}

#[test]
fn source_only_goes_offline_when_both_channels_fail() {
    let now = Instant::now();
    let mut hub = DisplayHub::default();
    hub.replace_source(now, "codex", SourceHealth::Healthy, vec![summary(now)]);
    hub.replace_source(
        now + Duration::from_secs(1),
        "codex",
        SourceHealth::Degraded,
        vec![summary(now)],
    );
    assert_eq!(hub.snapshot(now + Duration::from_secs(14)).health("codex"), SourceHealth::Degraded);

    hub.mark_unavailable(now + Duration::from_secs(15), "codex");
    assert_eq!(hub.snapshot(now + Duration::from_secs(31)).health("codex"), SourceHealth::Offline);
}
```

In `display/provider.rs`, prove the registry is static, ordered, and rejects duplicate IDs:

```rust
#[test]
fn provider_registry_rejects_duplicate_source_ids() {
    let mut registry = ProviderRegistry::default();
    registry.register(Box::new(FakeProvider::new("codex"))).unwrap();
    assert_eq!(registry.register(Box::new(FakeProvider::new("codex"))).unwrap_err(),
        "display_provider_duplicate");
}
```

- [ ] **Step 2: Run focused tests and verify the red state**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml display::`

Expected: compilation fails because the display types and methods do not exist.

- [ ] **Step 3: Implement the semantic types**

Create `display/model.rs` with these public shapes:

```rust
use std::{collections::BTreeMap, time::Instant};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DisplayPriority { Ambient, Normal, Attention, Critical }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayState { Idle, Running, NeedsInput, Success, Warning, Error }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisplayItem {
    pub id: String,
    pub source: String,
    pub priority: DisplayPriority,
    pub state: DisplayState,
    pub title: String,
    pub detail: Option<String>,
    pub metrics: BTreeMap<String, u32>,
    pub progress: Option<u8>,
    pub expires_at: Option<Instant>,
    pub updated_at: Instant,
}

impl DisplayItem {
    pub fn new(
        id: impl Into<String>, source: impl Into<String>,
        priority: DisplayPriority, state: DisplayState, title: impl Into<String>,
    ) -> Result<Self, &'static str> {
        let id = id.into();
        let source = source.into();
        if id.is_empty() || source.is_empty() { return Err("display_identity_empty"); }
        Ok(Self { id, source, priority, state, title: title.into(), detail: None,
            metrics: BTreeMap::new(), progress: None, expires_at: None,
            updated_at: Instant::now() })
    }

    pub fn with_progress(mut self, progress: u8) -> Result<Self, &'static str> {
        if progress > 100 { return Err("display_progress_out_of_range"); }
        self.progress = Some(progress);
        Ok(self)
    }

    pub fn key(&self) -> (&str, &str) { (&self.source, &self.id) }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceHealth { Healthy, Degraded, Stale, Offline }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisplaySnapshot {
    pub items: Vec<DisplayItem>,
    pub health: BTreeMap<String, SourceHealth>,
}
```

Keep test construction deterministic by adding `with_updated_at`, `with_expiry`, `with_detail`, and `with_metric` builder methods; none may mutate unrelated fields.

Create `display/provider.rs` with this object-safe boundary:

```rust
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
```

`ProviderRegistry::register` rejects duplicate `source_id` values and otherwise retains insertion order. `providers_mut` exposes only mutable trait-object iteration to `DisplayService`; it does not support directory discovery, manifests, dynamic loading, or external IPC. `display/mod.rs` later constructs the V1 registry with exactly one `CodexDisplayProvider`.

- [ ] **Step 4: Implement Hub replacement, TTL, and ordering**

Create `display/hub.rs` around this state:

```rust
#[derive(Default)]
pub struct DisplayHub {
    sources: BTreeMap<String, SourceState>,
}

struct SourceState {
    items: BTreeMap<String, DisplayItem>,
    health: SourceHealth,
    last_healthy_at: Option<Instant>,
    unavailable_since: Option<Instant>,
}
```

Implement these exact rules:

- `replace_source` replaces the complete source item map and preserves an existing item's `updated_at` when every semantic field except `updated_at` is equal.
- `replace_source` with `Healthy` or `Degraded` records a successful channel check, clears `unavailable_since`, and stores `Some(now)` in `last_healthy_at`.
- `mark_unavailable` creates an empty source entry when necessary and starts one unavailable interval; repeated calls do not reset it.
- `snapshot` removes items at `now >= expires_at`.
- After 5 seconds unavailable, remove transient task items, change the retained `codex.summary` state to `Warning`, and report `Stale`; after 15 seconds report `Offline` and remove all source items.
- Sort by priority descending, then state rank `NeedsInput, Error, Success, Warning, Running, Idle`, then `updated_at` descending, then stable `source/id` ascending.
- Add `DisplaySnapshot::health(&self, source: &str) -> SourceHealth`, returning `Offline` for an absent source.

Export the public types from `display/mod.rs` with `pub(crate) use` rather than making the entire module public to Tauri callers.

- [ ] **Step 5: Run focused tests and commit**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml display::model display::hub`

Expected: all new model and Hub tests pass.

Run: `rtk cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`

Expected: exit 0.

Commit:

```bash
rtk git add src-tauri/src/lib.rs src-tauri/src/display
rtk git commit -m "feat: add display semantic hub"
```

---

### Task 2: Parse Codex Rollout Lifecycle Without Retaining Content

**Files:**
- Create: `src-tauri/src/display/codex_events.rs`
- Create: `src-tauri/tests/fixtures/codex/rollout-lifecycle.jsonl`
- Modify: `src-tauri/src/display/mod.rs`

**Interfaces:**
- Consumes: one complete JSONL line at a time.
- Produces: `CodexRolloutEvent`, `CodexTaskSnapshot`, and `CodexRolloutIndex::{apply_line,current_tasks}`.

- [ ] **Step 1: Add the privacy-safe synthetic fixture**

Create `rollout-lifecycle.jsonl` with no message content:

```jsonl
{"timestamp":"2026-08-10T08:00:00Z","type":"session_meta","payload":{"id":"thread-a","cwd":"/work/kivo"}}
{"timestamp":"2026-08-10T08:00:01Z","type":"event_msg","payload":{"type":"task_started","turn_id":"turn-a","started_at":1}}
{"timestamp":"2026-08-10T08:00:02Z","type":"response_item","payload":{"type":"function_call","name":"request_user_input","call_id":"call-a"}}
{"timestamp":"2026-08-10T08:00:03Z","type":"response_item","payload":{"type":"function_call_output","call_id":"call-a"}}
{"timestamp":"2026-08-10T08:00:04Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-a","completed_at":4}}
{"timestamp":"2026-08-10T08:01:00Z","type":"event_msg","payload":{"type":"task_started","turn_id":"turn-b","started_at":60}}
{"timestamp":"2026-08-10T08:01:01Z","type":"event_msg","payload":{"type":"turn_aborted","turn_id":"turn-b","reason":"interrupted","completed_at":61}}
```

- [ ] **Step 2: Write failing lifecycle and privacy tests**

Add tests in `codex_events.rs`:

```rust
#[test]
fn tracks_running_input_ready_and_interrupted_lifecycle() {
    let mut index = CodexRolloutIndex::default();
    let fixture = include_str!("../../tests/fixtures/codex/rollout-lifecycle.jsonl");
    let mut states = Vec::new();
    for line in fixture.lines() {
        index.apply_line(line).unwrap();
        states.push(index.current_tasks());
    }
    assert!(states[1][0].running);
    assert_eq!(states[2][0].input_need, Some(CodexInputNeed::UserInput));
    assert_eq!(states[3][0].input_need, None);
    assert_eq!(states[4][0].event, Some(CodexTerminalEvent::ResponseReady));
    assert_eq!(states[6][0].event, Some(CodexTerminalEvent::Interrupted));
}

#[test]
fn ignores_body_fields_and_unknown_events() {
    let line = r#"{"type":"response_item","payload":{"type":"message","content":"secret"}}"#;
    assert_eq!(parse_rollout_line(line).unwrap(), None);
    assert!(!format!("{:?}", parse_rollout_line(line)).contains("secret"));
}

#[test]
fn leaves_truncated_last_line_for_the_next_read() {
    assert_eq!(parse_rollout_line("{\"type\":").unwrap_err().code(), "incomplete_json");
}
```

- [ ] **Step 3: Run the focused test and verify the red state**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml display::codex_events`

Expected: compilation fails because `CodexRolloutIndex` and the parser are absent.

- [ ] **Step 4: Implement the minimal event parser and lifecycle index**

Use `serde_json::Value`, but immediately project into these content-free types:

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CodexRolloutEvent {
    Session { thread_id: String, cwd: PathBuf },
    TurnStarted { turn_id: String },
    TurnCompleted { turn_id: String },
    TurnInterrupted { turn_id: String },
    UserInputRequested { call_id: String },
    UserInputResolved { call_id: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodexTerminalEvent { ResponseReady, Interrupted }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodexInputNeed { UserInput, Approval }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodexTaskSnapshot {
    pub thread_id: String,
    pub cwd: PathBuf,
    pub running: bool,
    pub input_need: Option<CodexInputNeed>,
    pub terminal_sequence: u64,
    pub event: Option<CodexTerminalEvent>,
}
```

`parse_rollout_line` must inspect only `type`, `payload.type`, `payload.id`, `payload.cwd`, `turn_id`, `name`, `call_id`, and `reason`. It must return `None` for messages, reasoning, tool arguments, task-complete bodies, unknown events, and unknown abort reasons. `CodexRolloutIndex` must keep sets of open turn IDs and input call IDs; terminal events increment `terminal_sequence` so the Provider can emit each completion once.

Add `apply_initial_scan` which restores open turns/calls but suppresses terminal events found before the watcher starts.

- [ ] **Step 5: Run tests and commit**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml display::codex_events`

Expected: all lifecycle, unknown-event, truncation, and initial-scan tests pass.

Commit:

```bash
rtk git add src-tauri/src/display/codex_events.rs src-tauri/src/display/mod.rs src-tauri/tests/fixtures/codex/rollout-lifecycle.jsonl
rtk git commit -m "feat: parse Codex rollout lifecycle"
```

---

### Task 3: Merge App Server Metadata With The Rollout Watcher

**Files:**
- Create: `src-tauri/src/display/codex_source.rs`
- Create: `src-tauri/src/display/codex_provider.rs`
- Create: `src-tauri/tests/fixtures/codex/thread-list-response.json`
- Create: `src-tauri/tests/fixtures/codex/app-server-v2-subset.json`
- Modify: `src-tauri/src/display/mod.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`

**Interfaces:**
- Consumes: App Server `thread/list` pages, rollout file events, and a caller-supplied default Codex home.
- Produces: `CodexSourceSnapshot { health, tasks }`, `CodexMetadataClient`, `SystemCodexMetadataClient`, `CodexTaskSource::poll`, and `CodexDisplayProvider` implementing `DisplayProvider`.

- [ ] **Step 1: Add exact dependencies and generated-format fixtures**

Add these direct dependencies:

```toml
base64 = "0.22"
notify = "8.2"
```

Generate the compatibility input with the installed CLI before extracting the committed subset:

```bash
rtk codex app-server generate-json-schema --out src-tauri/target/kivo-codex-app-server-schema
rtk proxy rg -n 'ThreadStatus|ThreadActiveFlag|waitingOnApproval|waitingOnUserInput' src-tauri/target/kivo-codex-app-server-schema
```

Create `thread-list-response.json`:

```json
{"id":2,"result":{"data":[
  {"id":"thread-a","name":"OLED design","cwd":"/work/kivo","updatedAt":10,
   "status":{"type":"active","activeFlags":["waitingOnUserInput"]}},
  {"id":"thread-b","name":null,"cwd":"/work/mindcraft","updatedAt":9,
   "status":{"type":"notLoaded"}}
],"nextCursor":null}}
```

Generate `app-server-v2-subset.json` from the installed CLI schema and retain only `ThreadStatus` and `ThreadActiveFlag`. The committed fixture must contain these exact enum values:

```json
{"ThreadStatus":["notLoaded","idle","systemError","active"],
 "ThreadActiveFlag":["waitingOnApproval","waitingOnUserInput"]}
```

- [ ] **Step 2: Write failing metadata, merge, and health tests**

Use a fake metadata client and `tempfile::TempDir`:

```rust
#[test]
fn explicit_active_flags_override_not_loaded_but_rollout_running_survives_it() {
    let now = Instant::now();
    let metadata = vec![
        thread("thread-a", "/work/kivo", AppServerStatus::Active(
            BTreeSet::from([ActiveFlag::WaitingOnUserInput]))),
        thread("thread-b", "/work/mindcraft", AppServerStatus::NotLoaded),
    ];
    let rollout = vec![task("thread-b", "/work/mindcraft", true, false)];

    let snapshot = merge_codex_sources(now, metadata, rollout, ChannelHealth::Healthy,
        ChannelHealth::Healthy);

    assert_eq!(snapshot.task("thread-a").input_need, Some(CodexInputNeed::UserInput));
    assert!(snapshot.task("thread-b").running);
}

#[test]
fn one_healthy_channel_keeps_the_combined_source_degraded_not_offline() {
    let snapshot = merge_codex_sources(Instant::now(), vec![], vec![],
        ChannelHealth::Unavailable, ChannelHealth::Healthy);
    assert_eq!(snapshot.health, SourceHealth::Degraded);
}

#[test]
fn request_payload_is_read_only_and_incremental() {
    assert_eq!(thread_list_params(None), serde_json::json!({
        "archived": false, "limit": 100, "sortKey": "updated_at",
        "sortDirection": "desc", "useStateDbOnly": true
    }));
}

#[test]
fn provider_maps_normalized_tasks_to_semantic_items() {
    let now = Instant::now();
    let source = FakeCodexTaskReader::once(codex_snapshot(
        SourceHealth::Healthy,
        vec![running_task("thread-a"), approval_task("thread-b")],
    ));
    let mut provider = CodexDisplayProvider::new(source);

    let update = provider.poll(now).unwrap();
    assert_eq!(update.source, "codex");
    assert_eq!(metric(&update.items, "running"), 2);
    assert_eq!(metric(&update.items, "needs_input"), 1);
    assert_eq!(update.items.iter().find(|item| item.id == "codex.task.thread-b").unwrap().detail,
        Some("approval needed".into()));
}
```

- [ ] **Step 3: Run tests and verify the red state**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml display::codex_source`

Expected: compilation fails because the source types are absent.

- [ ] **Step 4: Implement the App Server client boundary**

Define deserialization types that ignore every field outside this subset:

```rust
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum AppServerStatus {
    NotLoaded,
    Idle,
    SystemError,
    Active { #[serde(default)] active_flags: BTreeSet<ActiveFlag> },
}

#[derive(Deserialize, Ord, PartialOrd, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
enum ActiveFlag { WaitingOnApproval, WaitingOnUserInput }

pub trait CodexMetadataClient: Send {
    fn codex_home(&self) -> &Path;
    fn poll_updated(&mut self, last_seen: Option<u64>) -> Result<Vec<CodexThreadMetadata>, String>;
}
```

`SystemCodexMetadataClient` must:

1. Locate `codex` from `PATH`; on macOS also try `/Applications/Codex.app/Contents/Resources/codex`.
2. Spawn `codex app-server --listen stdio://` with piped stdin/stdout and inherited-or-null stderr.
3. Send newline-delimited `initialize`, then `initialized`.
4. Store `result.codexHome` and page `thread/list` with `useStateDbOnly: true`.
5. Ignore notifications and match responses by numeric `id`.
6. Kill and reap the child on drop; never start/resume/modify a thread.

- [ ] **Step 5: Implement bounded startup recovery and file watching**

`CodexTaskSource` owns the metadata client, `notify::RecommendedWatcher`, file cursors, and `CodexRolloutIndex` instances. Its constructor accepts both the Codex home fallback and a Kivo-owned cursor-store path. Implement these concrete behaviors:

- App Server paths: read the first complete `session_meta` line and reverse-read 64 KiB chunks until the latest turn boundary is found.
- CLI unavailable: resolve `${CODEX_HOME}` when non-empty, otherwise use the app-provided home joined with `.codex`; scan only files modified in the last 24 hours.
- Runtime: consume notify events, then stat known files once per second to catch missed notifications.
- Keep a trailing incomplete byte buffer per file and parse only newline-terminated records.
- Persist no conversation fields; cursor persistence contains only canonical path, inode/file identity, byte offset, thread ID, cwd, open turn IDs, and open call IDs. Write `<app_data_dir>/display/codex-cursors-v1.json` through a sibling temporary file plus atomic rename; ignore an invalid or version-mismatched cursor file and rebuild from bounded startup recovery.
- Poll App Server every 2 seconds; poll filesystem health every 1 second; publish at most 5 semantic updates per second.
- Give each App Server request a 1-second response deadline. A dedicated stdout reader matches numeric response IDs through a channel; timeout or EOF marks only the metadata channel unavailable, kills/reaps that child, and retries initialization on the next 2-second metadata deadline while rollout watching continues.

Normalize both channels into domain-only tasks. App Server `waitingOnApproval` maps to `CodexInputNeed::Approval`, `waitingOnUserInput` maps to `CodexInputNeed::UserInput`, and an open rollout request maps to `UserInput`. Explicit App Server flags outrank rollout input state; rollout-confirmed running outranks `notLoaded`. Use these source types:

```rust
pub struct CodexSourceSnapshot {
    pub health: SourceHealth,
    pub tasks: Vec<MergedCodexTask>,
}

pub struct MergedCodexTask {
    pub thread_id: String,
    pub name: Option<String>,
    pub cwd: PathBuf,
    pub updated_at: Instant,
    pub running: bool,
    pub input_need: Option<CodexInputNeed>,
    pub system_error: bool,
    pub terminal_event: Option<CodexTerminalEvent>,
    pub terminal_sequence: u64,
}

pub trait CodexTaskReader: Send {
    fn poll_tasks(&mut self, now: Instant) -> Result<CodexSourceSnapshot, &'static str>;
}
```

`CodexTaskSource` implements `CodexTaskReader` and contains no display priority, title formatting, or screen text.

In `codex_provider.rs`, implement `CodexDisplayProvider<R: CodexTaskReader>`. Its `DisplayProvider::poll` maps the source snapshot to a `ProviderUpdate`. The summary ID is `codex.summary`; task IDs are `codex.task.<thread_id>`. Select one task state by `NeedsInput > Error > ResponseReady > Interrupted > Running > Idle`; preserve the running count independently in the summary. Map user input/approval to `NeedsInput` with details `user input requested`/`approval needed`, completion to `Success`, interruption to `Warning`, and explicit system errors to `Error`. Set transient expiry from the source event's stable `updated_at`, never from each poll's `now`, so repeated polling cannot extend 8-second response/interrupted or 15-second system-error TTLs. A `notLoaded` metadata result must never clear rollout-confirmed running/input state. Register exactly this provider in `display::built_in_provider_registry`, and assert its `source_ids()` is exactly `["codex"]`.

`CodexTaskSource::poll_tasks` returns `Healthy` when both channels pass and `Degraded` when exactly one passes. When both channels are unavailable it returns `Err("codex_channels_unavailable")` instead of a replacement snapshot, so `DisplayService` calls `DisplayHub::mark_unavailable` and the 5-second stale/15-second offline timers preserve the last good summary.

- [ ] **Step 6: Run focused tests, dependency checks, and commit**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml display::codex_source display::codex_provider display::codex_events`

Expected: fake-client, temporary-file, merge, privacy, rollover, and health tests pass without launching a real Codex process.

Run: `rtk cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`

Expected: exit 0.

Commit:

```bash
rtk git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/display src-tauri/tests/fixtures/codex
rtk git commit -m "feat: add Codex display source"
```

---

### Task 4: Render Stable 128x32 Regions And Compute Scene Deltas

**Files:**
- Create: `src-tauri/src/display/render.rs`
- Create: `src-tauri/src/display/scene.rs`
- Modify: `src-tauri/src/display/mod.rs`

**Interfaces:**
- Consumes: `DisplaySnapshot` plus `DisplayCapabilities`.
- Produces: `DisplayRenderer`, `RendererRegistry`, `RenderedScene`, `DisplayRegion`, `DrawOperation`, `SceneUpdate`, and `SceneTracker::{prepare,ack,resync}`.

- [ ] **Step 1: Write failing golden-layout and delta tests**

Add these exact assertions:

```rust
#[test]
fn renders_running_summary_into_three_tile_aligned_regions() {
    let scene = MonoText128x32Renderer.render(&snapshot(3, 1)).unwrap();
    assert_eq!(scene.regions.iter().map(|r| (r.id.as_str(), r.bounds)).collect::<Vec<_>>(), vec![
        ("row0_left", Rect::new(0, 0, 64, 16)),
        ("row0_right", Rect::new(64, 0, 64, 16)),
        ("row1", Rect::new(0, 16, 128, 16)),
    ]);
    assert_eq!(scene.text("row0_left"), "CODEX");
    assert_eq!(scene.text("row0_right"), "3 RUN");
    assert_eq!(scene.text("row1"), "1 NEEDS INPUT");
}

#[test]
fn changing_only_running_count_emits_only_row0_right() {
    let mut tracker = SceneTracker::default();
    let first = tracker.prepare(MonoText128x32Renderer.render(&snapshot(3, 1)).unwrap()).unwrap();
    tracker.ack(first.new_revision).unwrap();
    let second = tracker.prepare(MonoText128x32Renderer.render(&snapshot(4, 1)).unwrap()).unwrap();
    assert_eq!(second.mode, SceneMode::Delta);
    assert_eq!(second.regions.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), vec!["row0_right"]);
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
```

- [ ] **Step 2: Run focused tests and verify the red state**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml display::render display::scene`

Expected: compilation fails because Renderer and scene types are absent.

- [ ] **Step 3: Implement capabilities, regions, and deterministic hashes**

Use these types:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rect { pub x: u16, pub y: u16, pub width: u16, pub height: u16 }

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DrawOperation {
    ClearRegion,
    Text { x: u16, baseline_y: u16, font_id: u8, text: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisplayRegion {
    pub slot: u8,
    pub id: &'static str,
    pub bounds: Rect,
    pub content_hash: u64,
    pub operations: Vec<DrawOperation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedScene { pub regions: Vec<DisplayRegion> }
```

Compute `content_hash` with a private stable FNV-1a 64 implementation over slot, bounds, operation tags, coordinates, font ID, and text bytes. Do not use `DefaultHasher` because its algorithm is not a persistence contract.

`DisplayCapabilities::ssd1306_128x32_mono()` must declare width 128, height 32, ASCII font ID 0, max 8 regions, max 24 ops, max 48 text bytes, and 8x8 tile updates.

Add the static Renderer boundary:

```rust
pub trait DisplayRenderer: Send + Sync {
    fn panel_id(&self) -> &'static str;
    fn capabilities(&self) -> &DisplayCapabilities;
    fn render(&self, snapshot: &DisplaySnapshot) -> Result<RenderedScene, &'static str>;
}

#[derive(Default)]
pub struct RendererRegistry {
    renderers: BTreeMap<&'static str, Arc<dyn DisplayRenderer>>,
}
```

`RendererRegistry::register` rejects duplicate panel IDs; `renderer(panel_id)` performs exact lookup and returns `display_renderer_unsupported` when absent. `built_in_renderer_registry()` registers exactly one `MonoText128x32Renderer`. There is no manifest scan, dynamic library, or Provider-specific renderer branch.

- [ ] **Step 4: Implement the exact 128x32 view selection**

`MonoText128x32Renderer` selects the first matching view:

1. newest `NeedsInput`: project title plus `NEEDS INPUT` or `APPROVAL NEEDED`;
2. newest `Error`: project title plus `CODEX ERROR`;
3. newest unexpired `Success`: project title plus `RESPONSE READY`;
4. newest `Warning`: project title plus `TASK STOPPED`;
5. healthy/degraded summary: `CODEX`, `<N> RUN`, `<M> NEEDS INPUT` or blank row;
6. offline: `CODEX OFFLINE` / `KIVO READY`;
7. idle: `CODEX IDLE` / `KIVO READY`.

Use cwd basename first, uppercase ASCII alphanumeric plus `-_`, collapse other runs to one space, and cap task labels at 16 glyphs. If no printable ASCII remains, use the first four uppercase ASCII alphanumeric characters of the thread ID as `TASK XXXX`.

- [ ] **Step 5: Implement ack-based full/delta tracking**

Use:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SceneMode { Full, Delta }

pub struct SceneUpdate {
    pub new_revision: u32,
    pub base_revision: u32,
    pub mode: SceneMode,
    pub regions: Vec<DisplayRegion>,
}
```

`SceneTracker` keeps `acked`, `pending`, `desired`, and `next_revision`. `prepare` returns `None` for an unchanged desired scene, emits full with base 0 when no scene is acked, and emits only changed/new/removed slots for delta. Represent removed slots as a region with the old bounds and only `ClearRegion`. While an update is pending, replace `desired` but do not emit another update. `ack` advances only on exact pending revision, then immediately makes a newer desired scene eligible. `resync` clears acked/pending and forces the latest desired scene to full. Before `u32::MAX`, force full revision 1/base 0.

- [ ] **Step 6: Run tests and commit**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml display::render display::scene`

Expected: golden layout, ASCII fallback, unchanged scene, one-region delta, removed-region, pending coalescing, ack mismatch, resync, and revision wrap tests pass.

Commit:

```bash
rtk git add src-tauri/src/display/render.rs src-tauri/src/display/scene.rs src-tauri/src/display/mod.rs
rtk git commit -m "feat: render and diff display scenes"
```

---

### Task 5: Add Protocol 7 Display Commands To The Helper Worker

**Files:**
- Modify: `src-tauri/src/protocol.rs`
- Modify: `src-tauri/src/coordinator.rs`
- Modify: `src-tauri/src/device.rs`

**Interfaces:**
- Consumes: `Arc<DisplaySnapshot>` from the coordinator, the device panel ID, and `DISPLAY_*` replies from the device.
- Produces: protocol 7 lines, `DeviceMessage::{DisplayOk,DisplayResync,DisplayError}`, `WorkerCommand::UpdateDisplay`, and `RuntimeCoordinator::update_display`.

- [ ] **Step 1: Write failing protocol encoder/parser tests**

Add Rust tests:

```rust
#[test]
fn encodes_bounded_display_delta_with_base64_text() {
    let update = display_update(2, 1, SceneMode::Delta, "4 RUN");
    assert_eq!(display_commands(&update).unwrap(), vec![
        "DISPLAY_BEGIN 2 1 delta\n",
        "DISPLAY_REGION 1 64 0 64 16\n",
        "DISPLAY_CLEAR 1\n",
        "DISPLAY_TEXT 1 64 12 0 NCBSVU4=\n",
        "DISPLAY_COMMIT 2\n",
    ]);
}

#[test]
fn parses_display_ack_resync_and_error() {
    assert_eq!(parse_device("DISPLAY_OK 9\n"), Some(DeviceMessage::DisplayOk { revision: 9 }));
    assert_eq!(parse_device("DISPLAY_RESYNC 7\n"), Some(DeviceMessage::DisplayResync { current_revision: 7 }));
    assert_eq!(parse_device("DISPLAY_ERROR 9 invalid_text\n"), Some(DeviceMessage::DisplayError {
        revision: 9, code: "invalid_text".into()
    }));
}
```

Add coordinator tests proving one scene is sent to every active worker and `WorkerCommand::UpdateDisplay` does not affect profile revision.

- [ ] **Step 2: Run focused tests and verify the red state**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml protocol::tests coordinator::tests`

Expected: compilation fails on missing display variants and encoder.

- [ ] **Step 3: Bump only the current protocol and encode strict transactions**

Set:

```rust
pub const HOST_PROTOCOL_VERSION: u16 = 7;
pub const DISPLAY_PROTOCOL_VERSION: u16 = 7;
```

Keep `MIN_SUPPORTED_PROTOCOL_VERSION = 3`. Extend `DeviceMessage` with the three reply variants. `display_commands` must:

- reject more than 8 regions or 24 total operations;
- reject out-of-bounds coordinates and decoded text over 48 bytes;
- reject non-ASCII text for font 0;
- encode with `base64::engine::general_purpose::STANDARD`;
- verify each finished line is under 255 bytes before returning it;
- output full/delta lowercase exactly as in the design.

Do not change firmware `HELLO`, upload verification, or release-script assertions in this task; Task 6 advances those together with the firmware parser. The Host must continue accepting explicit protocol 3-6 compatibility fixtures.

- [ ] **Step 4: Add the per-device link and coordinator fan-out**

Add:

```rust
pub enum WorkerCommand {
    UpdatePort(String),
    UpdateSnapshot(Option<Arc<RuntimeProfileSnapshot>>),
    Reconfigure { snapshot: Option<Arc<RuntimeProfileSnapshot>>, revision: u32 },
    BeginLearning(LearningTarget),
    EndLearning { snapshot: Option<Arc<RuntimeProfileSnapshot>>, revision: u32 },
    Input { receive_sequence: u64, captured: CapturedInput },
    UpdateDisplay(Arc<DisplaySnapshot>),
    Shutdown,
}

pub struct DeviceDisplayLink {
    tracker: SceneTracker,
    enabled: bool,
    renderer: Option<Arc<dyn DisplayRenderer>>,
    pending_since: Option<Instant>,
}
```

Give `RuntimeCoordinator` and each spawned device worker an `Arc<RendererRegistry>`. `DeviceDisplayLink::configure` enables remote scenes only when HELLO protocol is at least 7 and the current `RuntimeProfileSnapshot` resolves a hardware profile whose panel ID exists in the registry. Existing `Ssd1306Config` resolves internally to `ssd1306_128x32_mono`. `update_desired` renders the semantic snapshot through that selected Renderer and stores/coalesces the resulting scene. `next_lines` emits at most one transaction and starts `pending_since`. `on_message` acknowledges exact `DISPLAY_OK`, calls resync for `DISPLAY_RESYNC`, and forces full after `DISPLAY_ERROR` without disconnecting the worker. If no matching reply arrives within 2 seconds, clear acked/pending state and send the latest desired scene as full/base 0; rate-limit the timeout log to one entry per retry.

Add `RuntimeCoordinator::update_display(&mut self, snapshot: Arc<DisplaySnapshot>)` that sends `WorkerCommand::UpdateDisplay` to every live worker. A send failure follows the existing worker-error path; the method does not rewrite profile revisions.

Integrate the link in `run_isolated_worker_inner`: process update commands before serial reads, write pending display lines through the existing writer/flush error mapping, and route display replies to the link instead of `DeviceSession`.

- [ ] **Step 5: Verify old-device silence and coalescing**

Add tests for:

- protocol 6 + OLED profile: zero `DISPLAY_*` lines;
- protocol 7 without OLED profile: zero lines;
- protocol 7 + OLED: first scene full/base 0;
- two protocol 7 workers mapped to different test Renderer IDs produce different scenes from the same semantic snapshot;
- newer scene while ack pending: no second transaction until ack;
- missing ack for 2 seconds: latest desired scene is retried full/base 0;
- resync: latest desired scene sent full;
- display error: worker remains alive and next update is full.

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml protocol::tests device::tests coordinator::tests`

Expected: all display and existing action/profile worker tests pass.

- [ ] **Step 6: Run the Rust gate and commit**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml`

Run: `rtk cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`

Expected: the Rust suite and Clippy pass; protocol 3-6 fixtures still negotiate without display traffic.

Commit:

```bash
rtk git add src-tauri/src/protocol.rs src-tauri/src/coordinator.rs src-tauri/src/device.rs
rtk git commit -m "feat: add host display protocol v7"
```

---

### Task 6: Stage And Commit Bounded Display Transactions In Firmware

**Files:**
- Create: `lib/gpio_trigger/src/RemoteDisplay.h`
- Create: `lib/gpio_trigger/src/RemoteDisplay.cpp`
- Modify: `lib/gpio_trigger/src/TriggerProtocol.h`
- Modify: `lib/gpio_trigger/src/TriggerProtocol.cpp`
- Modify: `lib/gpio_trigger/src/Handshake.cpp`
- Modify: `src/main.cpp`
- Modify: `test/test_gpio_trigger/test_main.cpp`
- Modify: `scripts/verify_runtime_firmware.py`
- Modify: `test/test_upload_targeting.py`
- Modify: `test/test_release.sh`

**Interfaces:**
- Consumes: parsed `DISPLAY_BEGIN`, `DISPLAY_REGION`, `DISPLAY_CLEAR`, `DISPLAY_TEXT`, and `DISPLAY_COMMIT` commands.
- Produces: retained `RemoteDisplayScene`, `RemoteDisplayCommit`, and exact OK/resync/error lines.

- [ ] **Step 1: Write failing firmware parser and atomicity tests**

Add Unity tests:

```cpp
void test_display_transaction_commits_atomically() {
  RemoteDisplay display;
  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(2, 0, DisplayMode::Full));
  TEST_ASSERT_TRUE(display.region(0, {0, 0, 64, 16}));
  TEST_ASSERT_TRUE(display.clear(0));
  TEST_ASSERT_TRUE(display.text(0, 0, 12, 0, "KIVO"));
  TEST_ASSERT_FALSE(display.committed().has_value());

  const auto commit = display.commit(2);
  TEST_ASSERT_TRUE(commit.has_value());
  TEST_ASSERT_EQUAL_UINT32(2, display.revision());
  TEST_ASSERT_EQUAL_STRING("KIVO", display.committed()->regions[0].textOps[0].text.c_str());
}

void test_display_revision_mismatch_requests_resync_without_mutation() {
  RemoteDisplay display;
  commitFullScene(display, 4);
  TEST_ASSERT_EQUAL(DisplayResult::Resync,
                    display.begin(5, 3, DisplayMode::Delta));
  TEST_ASSERT_EQUAL_UINT32(4, display.revision());
}

void test_new_begin_discards_uncommitted_transaction() {
  RemoteDisplay display;
  TEST_ASSERT_EQUAL(DisplayResult::Accepted, display.begin(1, 0, DisplayMode::Full));
  TEST_ASSERT_TRUE(display.region(0, {0, 0, 64, 16}));
  TEST_ASSERT_EQUAL(DisplayResult::Accepted, display.begin(2, 0, DisplayMode::Full));
  TEST_ASSERT_FALSE(display.commit(1).has_value());
}
```

Add parser tests for every command and malformed base64, bounds, duplicate slot, 9 regions, 25 ops, text over 48 decoded bytes, commit revision mismatch, and 255-byte input.

- [ ] **Step 2: Run native firmware tests and verify the red state**

Run: `rtk uv run pio test -e native`

Expected: compilation fails because `RemoteDisplay` and display command kinds do not exist.

- [ ] **Step 3: Add bounded command data and parsing**

Extend `HelperCommandKind` with `DisplayBegin`, `DisplayRegion`, `DisplayClear`, `DisplayText`, and `DisplayCommit`. Extend `HelperCommand` with:

```cpp
std::uint32_t baseRevision = 0;
bool displayFull = false;
std::uint8_t displaySlot = 0;
std::uint16_t displayX = 0;
std::uint16_t displayY = 0;
std::uint16_t displayWidth = 0;
std::uint16_t displayHeight = 0;
std::uint8_t displayFontId = 0;
std::string displayText;
```

Parsing must require exact token counts, unsigned decimal coordinates, lowercase `full|delta`, base64 alphabet/padding, and no trailing tokens. Decode base64 in `RemoteDisplay.cpp`; reject decoded NUL/control/non-ASCII bytes and decoded length above 48.

- [ ] **Step 4: Implement fixed-capacity staged and committed state**

Use fixed arrays and counts, not heap-grown vectors for transaction cardinality:

```cpp
constexpr std::size_t kMaxDisplayRegions = 8;
constexpr std::size_t kMaxDisplayOps = 24;
constexpr std::size_t kMaxDisplayTextBytes = 48;

struct DisplayRect { std::uint16_t x, y, width, height; };
struct DisplayTextOp {
  std::uint8_t slot;
  std::uint16_t x;
  std::uint16_t baselineY;
  std::uint8_t fontId;
  std::string text;
};
struct RemoteDisplayCommit {
  std::uint32_t revision;
  bool full;
  std::array<DisplayRegionState, kMaxDisplayRegions> regions;
  std::size_t regionCount;
  std::array<DisplayRect, kMaxDisplayRegions> dirtyBounds;
  std::size_t dirtyCount;
};
```

`begin` validates base revision before creating staging state. Region bounds must fit 128x32 and align to 8x8. Every clear/text op must reference a region already declared in the transaction and stay inside it. `commit` copies full state or replaces only delta slots, calculates dirty bounds from old/new slot unions, and changes logical revision only after every staged operation validates. Any error clears staging and leaves committed state untouched.

Add formatting helpers returning exactly `DISPLAY_OK <rev>\n`, `DISPLAY_RESYNC <current>\n`, and `DISPLAY_ERROR <new> <code>\n`.

- [ ] **Step 5: Dispatch transactions without drawing yet**

Add a global `RemoteDisplay remoteDisplay` in `src/main.cpp`. Route each display command to it. On a successful commit, store the returned commit for Task 7 and immediately reply `DISPLAY_OK`; on mismatch reply `DISPLAY_RESYNC`; on validation failure reply `DISPLAY_ERROR` with stable codes `invalid_begin`, `invalid_region`, `invalid_text`, `invalid_commit`, or `unsupported_display`.

Bump `Handshake.cpp` to `HELLO 7`. Update current-firmware expectations from 6 to 7 in `verify_runtime_firmware.py`, its pytest assertions, `test_release.sh`, and the C++ current HELLO assertion. Leave every explicit protocol 3-6 compatibility fixture unchanged.

- [ ] **Step 6: Run native tests, builds, and commit**

Run: `rtk uv run pio test -e native`

Expected: all existing and new Unity tests pass.

Run: `rtk pytest test/test_upload_targeting.py -q`

Run: `rtk test bash test/test_release.sh`

Expected: upload-targeting and static release checks pass with current protocol 7 while retaining legacy 3-6 compatibility coverage.

Run: `rtk direnv exec . make build-rp2040`

Run: `rtk direnv exec . make build-esp32s3`

Expected: both firmware builds succeed; ESP32-S3 receives no display commands in normal Host operation.

Commit:

```bash
rtk git add lib/gpio_trigger/src/RemoteDisplay.h lib/gpio_trigger/src/RemoteDisplay.cpp lib/gpio_trigger/src/TriggerProtocol.h lib/gpio_trigger/src/TriggerProtocol.cpp lib/gpio_trigger/src/Handshake.cpp src/main.cpp test/test_gpio_trigger/test_main.cpp scripts/verify_runtime_firmware.py test/test_upload_targeting.py test/test_release.sh
rtk git commit -m "feat: stage firmware display transactions"
```

---

### Task 7: Arbitrate Local Status Over Committed Remote Scenes

**Files:**
- Create: `lib/gpio_trigger/src/DisplayController.h`
- Create: `lib/gpio_trigger/src/DisplayController.cpp`
- Modify: `src/platform/Platform.h`
- Modify: `src/platform/rp2040.cpp`
- Modify: `src/platform/esp32s3.cpp`
- Modify: `src/main.cpp`
- Modify: `test/test_gpio_trigger/test_main.cpp`
- Modify: `test/test_release.sh`

**Interfaces:**
- Consumes: local `DisplayFrame`, committed `RemoteDisplayCommit`, Helper connection, and local override state.
- Produces: `DisplayController::visibleFrame`, platform full-frame apply calls, and correct restoration after local override.

- [ ] **Step 1: Write failing arbitration state-machine tests**

Add:

```cpp
void test_local_critical_overrides_and_then_restores_latest_remote_scene() {
  DisplayController controller;
  const auto remote1 = remoteScene(1, "CODEX", "1 RUN");
  const auto remote2 = remoteScene(2, "KIVO", "NEEDS INPUT");
  controller.commitRemote(remote1);
  TEST_ASSERT_EQUAL(DisplaySource::Remote, controller.source());

  controller.showLocal(localFrame("CONFIG ERROR"), LocalDisplayPriority::Critical);
  controller.commitRemote(remote2);
  TEST_ASSERT_EQUAL(DisplaySource::Local, controller.source());

  controller.clearLocalOverride();
  TEST_ASSERT_EQUAL(DisplaySource::Remote, controller.source());
  TEST_ASSERT_EQUAL_UINT32(2, controller.remoteRevision());
}

void test_disconnect_discards_remote_and_requires_new_full_scene() {
  DisplayController controller;
  controller.commitRemote(remoteScene(4, "CODEX", "2 RUN"));
  controller.helperDisconnected(localFrame("HELPER OFFLINE"));
  TEST_ASSERT_EQUAL(DisplaySource::Local, controller.source());
  TEST_ASSERT_FALSE(controller.hasRemote());
}
```

- [ ] **Step 2: Run native tests and verify the red state**

Run: `rtk uv run pio test -e native`

Expected: compilation fails because `DisplayController` does not exist.

- [ ] **Step 3: Implement the pure arbitration controller**

Define:

```cpp
enum class DisplaySource { Local, Remote };
enum class LocalDisplayPriority { Startup, Normal, Critical };

class DisplayController {
 public:
  DisplayUpdate showLocal(const DisplayFrame &, LocalDisplayPriority);
  DisplayUpdate clearLocalOverride();
  DisplayUpdate commitRemote(const RemoteDisplayCommit &);
  DisplayUpdate helperDisconnected(const DisplayFrame &offline);
  DisplaySource source() const;
  bool hasRemote() const;
  std::uint32_t remoteRevision() const;
};
```

The controller retains the newest committed remote slot state while a local critical/learning screen is visible. `clearLocalOverride` returns a full remote redraw when remote state exists; disconnect clears remote state/revision and returns the local offline frame. Startup/Ready local content stays visible until the first full remote scene is committed.

- [ ] **Step 4: Add full-frame platform rendering for remote primitives**

Extend `Platform.h` with:

```cpp
void renderLocalDisplay(const DisplayFrame &frame);
void renderRemoteDisplay(const RemoteDisplayCommit &scene, bool fullRedraw);
void resetRemoteDisplay();
void serviceDisplay();
```

In RP2040, clear the U8g2 buffer on full redraw, clear each dirty region before drawing its retained slot text, select only font ID 0 (`u8g2_font_6x13_tf`), and call `sendBuffer()` for now. ESP32-S3 implements no-ops. Keep `configureDisplay` before input pin-mode application.

- [ ] **Step 5: Route every local transition through the controller**

Replace direct `renderStatus()` behavior:

- startup/Ready before first remote: local normal;
- configuration error and OLED failure: local critical;
- learning begin/inputs/end: local critical until runtime returns;
- GPIO input debug changes: update retained local model but do not overwrite an active remote scene;
- Helper disconnect: clear remote state and show `HELPER OFFLINE`;
- Helper reconnect: keep local Ready until a new full scene commits;
- remote commit during local override: retain/ack it without physical draw;
- override clear: redraw latest retained remote scene.

Update `test_release.sh` static assertions to use `renderLocalDisplay`/controller names while retaining the configure-before-pin-mode contract.

- [ ] **Step 6: Run tests/builds and commit**

Run: `rtk uv run pio test -e native`

Run: `rtk test bash test/test_release.sh`

Run: `rtk direnv exec . make build-rp2040`

Expected: arbitration tests, release assertions, and RP2040 build pass.

Commit:

```bash
rtk git add lib/gpio_trigger/src/DisplayController.h lib/gpio_trigger/src/DisplayController.cpp src/platform/Platform.h src/platform/rp2040.cpp src/platform/esp32s3.cpp src/main.cpp test/test_gpio_trigger/test_main.cpp test/test_release.sh
rtk git commit -m "feat: arbitrate local and remote displays"
```

---

### Task 8: Flush Dirty SSD1306 Tiles Within The Key-Scan Budget

**Files:**
- Create: `lib/gpio_trigger/src/DirtyTiles.h`
- Create: `lib/gpio_trigger/src/DirtyTiles.cpp`
- Modify: `src/platform/rp2040.cpp`
- Modify: `src/platform/esp32s3.cpp`
- Modify: `src/main.cpp`
- Modify: `test/test_gpio_trigger/test_main.cpp`

**Interfaces:**
- Consumes: committed dirty pixel bounds and a data-byte budget.
- Produces: coalesced `TileRun { tx, ty, tw, th }` values for `U8G2::updateDisplayArea`.

- [ ] **Step 1: Write failing dirty-tile scheduler tests**

Add:

```cpp
void test_dirty_tiles_emit_only_changed_counter_region() {
  DirtyTiles dirty(16, 4);
  dirty.markPixels({64, 0, 64, 16});
  std::size_t bytes = 0;
  while (const auto run = dirty.takeRun(64)) bytes += run->dataBytes();
  TEST_ASSERT_EQUAL_UINT32(128, bytes);
}

void test_dirty_tiles_respect_per_loop_budget_and_coalesce_updates() {
  DirtyTiles dirty(16, 4);
  dirty.markPixels({0, 0, 128, 32});
  const auto first = dirty.takeRun(64);
  TEST_ASSERT_TRUE(first.has_value());
  TEST_ASSERT_LESS_OR_EQUAL_UINT32(64, first->dataBytes());
  dirty.markPixels({64, 0, 64, 16});
  TEST_ASSERT_TRUE(dirty.hasDirty());
}

void test_rotated_or_unsupported_panel_requests_full_refresh() {
  TEST_ASSERT_EQUAL(RefreshMode::Full, selectRefreshMode(false, 0));
  TEST_ASSERT_EQUAL(RefreshMode::Full, selectRefreshMode(true, 90));
  TEST_ASSERT_EQUAL(RefreshMode::Tiles, selectRefreshMode(true, 0));
}
```

- [ ] **Step 2: Run native tests and verify the red state**

Run: `rtk uv run pio test -e native`

Expected: compilation fails because `DirtyTiles` is absent.

- [ ] **Step 3: Implement the 16x4 bitmap and bounded runs**

`DirtyTiles` stores one 64-bit bitmap. `markPixels` rounds outward to 8x8 boundaries and sets covered bits. `takeRun(maxDataBytes)` scans row-major, takes adjacent tiles from one row only, caps width at `maxDataBytes / 8`, clears only returned bits, and returns no run for a budget below 8 bytes. `TileRun::dataBytes()` equals `tw * 8 * th`.

Full local redraw marks all 64 tiles. A delta commit marks the union bounds returned by `RemoteDisplayCommit`; newer commits OR into unsent dirty bits, so stale framebuffer bytes are never deliberately sent before the latest buffer contents.

- [ ] **Step 4: Replace synchronous remote refresh with scheduled tile service**

In `rp2040.cpp`:

- local critical/startup full draws may call `sendBuffer()` immediately;
- remote full/delta draws mutate the U8g2 full buffer and mark tiles without calling `sendBuffer()`;
- `serviceDisplay()` takes one run with a 64-byte payload budget and calls
  `display->updateDisplayArea(run.tx, run.ty, run.tw, run.th)`;
- if rotation is non-zero or partial updates are unavailable, the first service call uses one full `sendBuffer()` and clears the bitmap;
- `stopDisplay` clears the queue before powering down.

Call `platform::serviceDisplay()` once after runtime/learning input scanning and before the existing 1ms delay in `loop()`. Do not service I2C before scanning keys.

- [ ] **Step 5: Run tests, builds, and commit**

Run: `rtk uv run pio test -e native`

Expected: dirty byte counts are 128 for the right half/two rows and 512 for full screen; budget/coalescing tests pass.

Run: `rtk direnv exec . make build-rp2040`

Run: `rtk direnv exec . make build-esp32s3`

Expected: both builds pass; RP2040 links `updateDisplayArea`, ESP32-S3 remains no-op.

Commit:

```bash
rtk git add lib/gpio_trigger/src/DirtyTiles.h lib/gpio_trigger/src/DirtyTiles.cpp src/platform/rp2040.cpp src/platform/esp32s3.cpp src/main.cpp test/test_gpio_trigger/test_main.cpp
rtk git commit -m "perf: update OLED by dirty tiles"
```

---

### Task 9: Wire The Display Service End To End And Complete Acceptance

**Files:**
- Create: `src-tauri/src/display/service.rs`
- Modify: `src-tauri/src/display/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/coordinator.rs`
- Modify: `README.md`

**Interfaces:**
- Consumes: `ProviderRegistry`, `DisplayHub`, a shared stop flag, and a semantic snapshot sender.
- Produces: `DisplayService::spawn(...) -> JoinHandle<()>` and live `Arc<DisplaySnapshot>` fan-out; device workers perform panel-specific rendering.

- [ ] **Step 1: Write failing service dedupe and shutdown tests**

Inject a fake Provider clock rather than launching Codex:

```rust
#[test]
fn service_emits_only_semantically_changed_snapshots() {
    let provider = FakeProvider::from_updates(vec![running(1), running(1), running(2)]);
    let snapshots = run_service_steps(provider, 3);
    assert_eq!(snapshots.len(), 2);
    assert_eq!(metric(&snapshots[0].items, "running"), 1);
    assert_eq!(metric(&snapshots[1].items, "running"), 2);
}

#[test]
fn service_stops_and_drops_providers_when_stop_is_set() {
    let stop = Arc::new(AtomicBool::new(false));
    let (service, probe) = spawn_test_service(Arc::clone(&stop));
    stop.store(true, Ordering::Relaxed);
    service.join().unwrap();
    assert!(probe.provider_dropped.load(Ordering::Relaxed));
}
```

- [ ] **Step 2: Run focused tests and verify the red state**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml display::service`

Expected: compilation fails because `DisplayService` is absent.

- [ ] **Step 3: Implement the background service**

Use this public boundary:

```rust
pub struct UnavailableDisplayProvider {
    error_code: &'static str,
}

pub struct DisplayService;

impl DisplayService {
    pub fn spawn(
        providers: ProviderRegistry,
        stop: Arc<AtomicBool>,
        snapshots: mpsc::Sender<Arc<DisplaySnapshot>>,
    ) -> io::Result<JoinHandle<()>>;
}
```

The loop polls each Provider, validates every returned item's `source` against `ProviderUpdate.source`, calls `replace_source` or `mark_unavailable`, snapshots the Hub, compares the complete semantic `DisplaySnapshot` to the last emitted value, and sends only changes. Sleep no longer than 100ms so needs-input latency stays below one second. Logging contains Provider IDs, health/error codes, and counts only, never task/message text.

`UnavailableDisplayProvider::new("codex", "codex_source_init")` implements `DisplayProvider` by returning that stable error code from every `poll`; this drives the same stale/offline Hub path without terminating the service thread. `built_in_provider_registry` registers either one working `CodexDisplayProvider` or this unavailable Codex Provider, never both.

- [ ] **Step 4: Start, fan out, and stop the service with Tauri**

During `setup`:

1. Resolve the fallback home with `app.path().home_dir()?.join(".codex")` and the cursor path with `app.path().app_data_dir()?.join("display/codex-cursors-v1.json")`; prefer `codexHome` returned by App Server initialization inside `CodexTaskSource`.
2. Create `mpsc::channel::<Arc<DisplaySnapshot>>()`.
3. Construct `built_in_provider_registry` with `CodexTaskSource`, construct `Arc::new(built_in_renderer_registry())`, and give the Renderer registry to `RuntimeCoordinator` so it is cloned into each device worker.
4. Start `DisplayService` with the Provider registry and existing shared stop flag. In the existing coordinator thread, drain the snapshot receiver before each 5ms sleep and call `RuntimeCoordinator::update_display` only for the newest queued semantic snapshot.
5. Add `display_thread: Mutex<Option<JoinHandle<()>>>` to `AppState`.
6. On `RunEvent::Exit`, set stop, join display service, then join coordinator, then shut down paste/logging.

If source initialization fails, the built-in registry uses `UnavailableDisplayProvider::new("codex", "codex_source_init")`; Kivo startup and device Runtime must still succeed and render `CODEX OFFLINE` once a protocol 7 screen is ready.

- [ ] **Step 5: Add user-facing operational documentation**

Add a concise README section covering:

- default screens `CODEX <N> RUN`, `NEEDS INPUT`, `APPROVAL NEEDED`, `RESPONSE READY`, and `CODEX OFFLINE`;
- only task identity/cwd/status is consumed; no conversation/tool content is displayed or retained;
- protocol 3-6 firmware keeps the previous local debug screen;
- current V1 panel is SSD1306 128x32 rotation 0; profile YAML remains unchanged;
- physical OLED validation is required after flashing protocol 7 firmware.

- [ ] **Step 6: Run the complete automated gate**

Run: `rtk make test`

Expected: release tests, Python tests, PlatformIO native tests, Rust tests, Clippy, frontend tests, and production build all pass.

Run: `rtk direnv exec . make build-rp2040`

Run: `rtk direnv exec . make build-esp32s3`

Expected: both firmware builds succeed.

Run: `rtk git diff --check`

Expected: no whitespace errors.

- [ ] **Step 7: Commit the integrated feature**

```bash
rtk git add src-tauri/src/display/service.rs src-tauri/src/display/mod.rs src-tauri/src/lib.rs src-tauri/src/coordinator.rs README.md
rtk git commit -m "feat: show Codex status on Kivo displays"
```

- [ ] **Step 8: Upload and perform physical acceptance**

Run the interactive target selector and upload:

```bash
rtk direnv exec . make upload-rp2040
```

Then verify on the 18-key RP2040 + SSD1306 device:

1. Start two Codex Desktop tasks; the display reaches `CODEX 2 RUN` within one second.
2. Trigger `request_user_input`; the project label and `NEEDS INPUT` appear within one second and clear after answering.
3. Complete a response; `RESPONSE READY` remains for 8 seconds, then returns to summary.
4. Disconnect Helper; local `HELPER OFFLINE` replaces Codex content. Reconnect; the first remote update is full, later count changes are delta.
5. Capture SDA/SCL with a logic analyzer at 100kHz: a `row0_right` count change transfers 128 framebuffer data bytes rather than 512; record command/address overhead separately.
6. Press keys continuously during repeated count changes; confirm no missed press, duplicate trigger, visible malformed text, or uncommitted half-frame.
7. Rotate only through a test build/configuration; verify non-zero rotation takes the full-refresh fallback rather than calling tile update with rotated coordinates.

Record automated results separately from physical results. If no device or logic analyzer is available, mark those checks `Not Run`; do not claim physical acceptance from builds.

---

## Final Review Checklist

- [ ] Every requirement in the approved design maps to at least one task above.
- [ ] Built-in registries contain exactly `CodexDisplayProvider` and `ssd1306_128x32_mono`; no plugin discovery/loading exists.
- [ ] Provider code never imports Renderer or firmware types.
- [ ] Renderer consumes only semantic snapshot/capabilities.
- [ ] Semantic snapshots are fanned out before rendering, so each device selects its own panel Renderer.
- [ ] Old protocol devices receive zero display commands.
- [ ] App Server requests include `useStateDbOnly: true` and no mutation methods.
- [ ] Rollout fixtures and logs contain no real user/task content.
- [ ] Scene base revision advances only after matching `DISPLAY_OK`.
- [ ] Firmware applies no staged operation before matching commit.
- [ ] Local critical state wins while remote commits continue to be retained.
- [ ] Dirty tile service runs after key scanning and respects the 64-byte payload budget.
- [ ] `rtk make test`, both firmware builds, and `rtk git diff --check` pass.
- [ ] Physical OLED and input acceptance results are reported honestly.
