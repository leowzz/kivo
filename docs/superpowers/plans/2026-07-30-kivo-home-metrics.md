# Kivo Home Metrics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a metrics-first Home view backed by a local SQLite database that persists per-button use and readable activity logs.

**Architecture:** `metrics.rs` owns SQLite schema, writes, retention, and Home queries. The serial worker records runtime activities through that store before it emits the existing `runtime-event`; `lib.rs` extends the initial snapshot with the Home data. The React app adds a `home` view and applies the existing runtime event to the displayed metrics and log without polling.

**Tech Stack:** Rust 2024, Tauri 2, `rusqlite` with bundled SQLite, React 19, TypeScript, Vitest, Testing Library.

## Global Constraints

- Keep metrics local in the Tauri application data directory; do not add a network service or frontend persistence.
- Do not block configured keyboard actions when telemetry persistence fails; expose the failure through the existing runtime error path.
- Count only `input_state` events where `pressed` is `true` and the input resolves to an active model button.
- Store cumulative counts and local-calendar-day counts keyed by model ID and button ID.
- Keep exactly the newest 500 activity log rows after every append.
- Preserve existing behavior, hardware, and layout editors; add Home as the first sidebar view.

---

### Task 1: SQLite Metrics Store

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/metrics.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: a SQLite path, Unix milliseconds, model ID, button ID, and readable log fields.
- Produces: `MetricsStore::open`, `MetricsStore::record_button_press`, `MetricsStore::record_activity`, and `MetricsStore::home_snapshot` for the Tauri layer.

- [ ] **Step 1: Write the failing metrics-store tests**

Add these tests at the bottom of `src-tauri/src/metrics.rs`; use a temporary `metrics.sqlite3` path and fixed timestamps so calendar-day assertions are deterministic.

```rust
#[test]
fn records_totals_days_and_retains_only_500_logs() {
    let directory = TestDirectory::new();
    let store = MetricsStore::open(&directory.path("metrics.sqlite3")).unwrap();
    store.record_button_press("phone", "ONE", 1_720_000_000_000).unwrap();
    store.record_button_press("phone", "ONE", 1_720_086_400_000).unwrap();
    for number in 0..501 {
        store.record_activity(1_720_000_000_000 + number, "device", &format!("event {number}")).unwrap();
    }

    let snapshot = store.home_snapshot("phone", 1_720_086_400_000).unwrap();
    assert_eq!(snapshot.total_presses, 2);
    assert_eq!(snapshot.today_presses, 1);
    assert_eq!(snapshot.active_button_count, 1);
    assert_eq!(snapshot.top_button.as_ref().map(|button| button.button_id.as_str()), Some("ONE"));
    assert_eq!(snapshot.logs.len(), 500);
    assert_eq!(snapshot.logs[0].message, "event 500");
}
```

- [ ] **Step 2: Run the focused test to verify it fails**

Run: `rtk cargo test records_totals_days_and_retains_only_500_logs`

Expected: FAIL because module `metrics` and `MetricsStore` do not exist.

- [ ] **Step 3: Add the SQLite dependency and store implementation**

Add this dependency to `src-tauri/Cargo.toml`:

```toml
rusqlite = { version = "0.37", features = ["bundled"] }
```

Create `src-tauri/src/metrics.rs`. `open` must execute these schema statements once:

```sql
CREATE TABLE IF NOT EXISTS button_metrics (
  model_id TEXT NOT NULL,
  button_id TEXT NOT NULL,
  total_presses INTEGER NOT NULL DEFAULT 0,
  last_pressed_at_ms INTEGER NOT NULL,
  PRIMARY KEY (model_id, button_id)
);
CREATE TABLE IF NOT EXISTS button_metric_days (
  model_id TEXT NOT NULL,
  button_id TEXT NOT NULL,
  day TEXT NOT NULL,
  presses INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (model_id, button_id, day)
);
CREATE TABLE IF NOT EXISTS activity_logs (
  id INTEGER PRIMARY KEY,
  occurred_at_ms INTEGER NOT NULL,
  kind TEXT NOT NULL,
  message TEXT NOT NULL
);
```

Implement `record_button_press` as one transaction. Compute the local calendar day inside SQLite with `strftime('%Y-%m-%d', ?1 / 1000, 'unixepoch', 'localtime')`, then upsert both count tables using `ON CONFLICT ... DO UPDATE SET`. Implement `record_activity` as a transaction that inserts one row and deletes rows excluded by `ORDER BY id DESC LIMIT -1 OFFSET 500`. Define serializable DTOs with camel-case fields:

```rust
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeMetricsSnapshot {
    pub total_presses: u64,
    pub today_presses: u64,
    pub active_button_count: u64,
    pub top_button: Option<ButtonMetric>,
    pub heatmap: Vec<ButtonDayMetric>,
    pub logs: Vec<ActivityLog>,
}
```

Add `mod metrics;` to `src-tauri/src/lib.rs`.

- [ ] **Step 4: Run the focused test to verify it passes**

Run: `rtk cargo test records_totals_days_and_retains_only_500_logs`

Expected: PASS.

- [ ] **Step 5: Commit the storage unit**

```bash
rtk git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/metrics.rs src-tauri/src/lib.rs
rtk git commit -m "feat: persist Kivo button metrics"
```

### Task 2: Persist Runtime Activities and Expose Home Snapshot

**Files:**
- Modify: `src-tauri/src/device.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/types.ts`
- Modify: `src/preview.ts`

**Interfaces:**
- Consumes: `MetricsStore`, the active runtime `ModelConfig`, and `RuntimeActivity` values emitted by the worker.
- Produces: `AppSnapshot.homeMetrics`, `RuntimeEvent.homeUpdate`, and frontend `HomeMetricsSnapshot` types.

- [ ] **Step 1: Write the failing backend integration test**

In the existing `src-tauri/src/lib.rs` test module, create `AppState` with a temporary metrics database, record a press for `UP`, and assert the command snapshot exposes it:

```rust
#[test]
fn snapshot_includes_active_model_metrics() {
    let directory = TestDirectory::new();
    let state = product_state(&directory.0, vec![product_model()]);
    state.metrics.record_button_press("red-phone-v1", "UP", 1_720_000_000_000).unwrap();

    let snapshot = snapshot(&state).unwrap();
    assert_eq!(snapshot.home_metrics.today_presses, 1);
    assert_eq!(snapshot.home_metrics.top_button.unwrap().button_id, "UP");
}
```

- [ ] **Step 2: Run the backend test to verify it fails**

Run: `rtk cargo test snapshot_includes_active_model_metrics`

Expected: FAIL because `AppState` and `AppSnapshot` have no metrics member.

- [ ] **Step 3: Wire storage through app setup and the worker**

In `lib.rs`, open `metrics.sqlite3` beneath `app_config_dir`, store it as `Arc<MetricsStore>` in `AppState`, and use it while constructing the worker. Add `home_metrics: HomeMetricsSnapshot` to `AppSnapshot`; when there is no active model return `HomeMetricsSnapshot::empty()`.

Extend `device::WorkerState` with:

```rust
pub metrics: Arc<MetricsStore>,
```

Before `emit_activity` serializes and sends each event, resolve a press against `active_model` using `ModelConfig::button_for`. For an `input_state` down event with a mapped button, call:

```rust
metrics.record_button_press(&model.model.id, button_id, timestamp_ms)?;
metrics.record_activity(timestamp_ms, "button", &format!("{button_id} pressed"))?;
```

For connection and configuration activities, append a human-readable message such as `Device connected` or `Topology active`; do not create a second event channel. If persistence returns an error, set `runtime_error` to `RuntimeActivity::new("metrics_write_failed").with_detail(error.to_string())` and still emit the original activity and perform the configured action.

Extend `RuntimeEvent` with `home_update: Option<HomeMetricsSnapshot>`, populated after a successful metric update. Mirror the exact camel-case types in `src/types.ts` and include sample `homeMetrics` in `previewSnapshot`.

- [ ] **Step 4: Run the backend test to verify it passes**

Run: `rtk cargo test snapshot_includes_active_model_metrics`

Expected: PASS.

- [ ] **Step 5: Run the Rust suite and commit the bridge**

Run: `rtk cargo test`

Expected: PASS.

```bash
rtk git add src-tauri/src/device.rs src-tauri/src/lib.rs src/types.ts src/preview.ts
rtk git commit -m "feat: expose Kivo home metrics"
```

### Task 3: Metrics-First Home View

**Files:**
- Create: `src/HomeDashboard.tsx`
- Modify: `src/App.tsx`
- Modify: `src/App.css`
- Modify: `src/i18n.ts`
- Modify: `src/App.test.tsx`

**Interfaces:**
- Consumes: `HomeMetricsSnapshot`, `ConnectionStatus`, active model layout, language, and `RuntimeEvent.homeUpdate`.
- Produces: an accessible Home dashboard and the first sidebar navigation item.

- [ ] **Step 1: Write the failing Home render/update test**

Add a `homeMetrics` fixture to `baseSnapshot`, then add this test in `src/App.test.tsx`:

```tsx
test("shows the home metrics dashboard and applies a runtime update", async () => {
  let handler: ((event: { payload: RuntimeEvent }) => void) | undefined;
  vi.mocked(listen).mockImplementation(async (_name, callback) => {
    handler = callback as typeof handler;
    return vi.fn();
  });
  render(<App />);

  await userEvent.setup().click(await screen.findByRole("button", { name: "首页" }));
  expect(screen.getByRole("heading", { name: "按键概览" })).toBeInTheDocument();
  expect(screen.getByText("今日按下")).toBeInTheDocument();
  handler?.({ payload: { ...runtimeEvent, homeUpdate: { ...baseSnapshot.homeMetrics, todayPresses: 2 } } });
  expect(await screen.findByText("2")).toBeInTheDocument();
});
```

- [ ] **Step 2: Run the frontend test to verify it fails**

Run: `rtk test npm run test -- src/App.test.tsx`

Expected: FAIL because the Home navigation button and `homeMetrics` do not exist.

- [ ] **Step 3: Add HomeDashboard and connect it to App**

Define `View` as `"home" | "behavior" | "hardware" | "layout"` and initialize it as `"home"`. Place an icon-only-plus-label `Home` navigation button before behavior. Render `<HomeDashboard>` in `content-panel` when `view === "home"`; retain `<ActionEditor>` for editor views and render an `<aside aria-label={t(language, "home.logs")}>` activity log on Home.

`HomeDashboard` must use semantic headings and show: connection status and port, today presses, active-button count, top button label resolved from the active layout, a seven-day CSS grid heat map, and newest-first activity messages. When metrics are unavailable, show the localized unavailable message without hiding the connection state.

Add these keys to both language maps:

```ts
"nav.home": "首页",
"home.title": "按键概览",
"home.todayPresses": "今日按下",
"home.activeButtons": "活跃按键",
"home.topButton": "最常用",
"home.heatmap": "最近 7 天",
"home.logs": "运行日志",
"home.unavailable": "指标暂不可用",
```

Use the existing restrained green/white styling. Keep desktop Home at two columns with a 360px log sidebar; below 980px make logs a full-width row and below 680px stack summary metrics without text overflow.

- [ ] **Step 4: Run the frontend test and complete checks**

Run: `rtk test npm run test -- src/App.test.tsx`

Expected: PASS.

Run: `rtk npm run build`

Expected: PASS with no TypeScript errors.

- [ ] **Step 5: Commit the Home view**

```bash
rtk git add src/HomeDashboard.tsx src/App.tsx src/App.css src/i18n.ts src/App.test.tsx
rtk git commit -m "feat: add Kivo metrics home"
```

### Task 4: Final Verification

**Files:**
- Verify only: `src-tauri/src/metrics.rs`, `src-tauri/src/device.rs`, `src-tauri/src/lib.rs`, `src/HomeDashboard.tsx`, `src/App.tsx`

**Interfaces:**
- Consumes: all completed implementation tasks.
- Produces: evidence that backend persistence and the responsive frontend build work together.

- [ ] **Step 1: Run all automated checks**

Run: `rtk cargo test && rtk npm run test && rtk npm run build && rtk git diff --check`

Expected: every command exits 0.

- [ ] **Step 2: Verify the Home view in preview mode**

Run: `rtk npm run dev -- --host 127.0.0.1`

Open the Vite URL with `?preview=1`, select Home, and verify the dashboard at 1280px and 390px widths: summary text fits, the log is visible, and existing configuration views still render after navigation.

- [ ] **Step 3: Commit only verification fixes, if any**

```bash
rtk git status
```

Expected: clean worktree; otherwise commit only changes required by a failing check with a focused message.
