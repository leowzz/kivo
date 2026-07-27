# Helper Tauri 2 Desktop App Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Python curses helper with a packaged macOS Tauri 2 application whose Rust backend owns helper behavior and whose React/TypeScript frontend provides mapping editing and live events.

**Architecture:** Tauri commands load and atomically save the YAML configuration while a single Rust background worker owns serial discovery, clipboard writes, and protocol replies. React keeps only editable UI state and receives low-volume runtime updates through Tauri events.

**Tech Stack:** Tauri 2.11, Rust 1.94, React 19, TypeScript, Vite 8, Vitest, `serialport`, `serde_yaml_ng`, macOS `pbcopy`.

## Global Constraints

- Target macOS only for the first release.
- Preserve firmware and the `PRESS`, `PASTE`, and `SKIP` serial protocol.
- Support GPIO `0-9` and `12-18`; show the GPIO0 boot-mode warning.
- Store UTF-8 `config.yaml` in the Tauri app configuration directory and import the project-root file once when available.
- Save mappings atomically and omit empty values.
- Send `PASTE` only after the mapped text reaches `pbcopy`; otherwise send `SKIP`.
- Keep exactly one reconnecting serial worker and no Python sidecar.
- Preserve unrelated worktree changes.

---

### Task 1: Tauri And React Shell

**Files:**
- Create: `package.json`, `package-lock.json`, `index.html`, `tsconfig.json`, `tsconfig.node.json`, `vite.config.ts`
- Create: `src/main.tsx`, `src/App.tsx`, `src/App.css`, `src/types.ts`, `src/vite-env.d.ts`
- Create: `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`
- Create: `src-tauri/capabilities/default.json`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`
- Modify: `.gitignore`

**Interfaces:**
- Produces: `npm run dev`, `npm run build`, `npm run test`, and `npm run tauri`.
- Produces: a Tauri window titled `Vibe Tool` with frontend dev URL `http://localhost:1420` and built assets from `../dist`.

- [ ] **Step 1: Create the minimal package and Tauri manifests**

Use current compatible package versions and these scripts:

```json
{
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "test": "vitest run",
    "tauri": "tauri"
  }
}
```

Set `src-tauri/Cargo.toml` package name to `vibe-tool`, library name to
`vibe_tool_lib`, and include `tauri`, `serde`, `serde_json`, `serde_yaml_ng`, and
`serialport`. Use `tauri-build` as the only build dependency.

- [ ] **Step 2: Create a renderable application shell**

`src/main.tsx` mounts `<App />`; the initial `App` renders a `main` landmark and
the heading `Vibe Tool`. `src-tauri/src/main.rs` calls `vibe_tool_lib::run()` and
`lib.rs` runs a default `tauri::Builder`.

- [ ] **Step 3: Install and verify both toolchains**

Run: `rtk npm install`

Run: `rtk npm run build`

Expected: TypeScript and Vite exit 0 and create `dist/`.

Run: `rtk cargo check --manifest-path src-tauri/Cargo.toml`

Expected: Cargo exits 0.

- [ ] **Step 4: Commit**

```bash
rtk git add .gitignore package.json package-lock.json index.html tsconfig.json tsconfig.node.json vite.config.ts src src-tauri
rtk git commit -m "feat: scaffold Tauri helper app"
```

### Task 2: Rust Configuration And Protocol Core

**Files:**
- Create: `src-tauri/src/config.rs`
- Create: `src-tauri/src/protocol.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `SUPPORTED_GPIOS: [u8; 17]`.
- Produces: `MappingConfig { buttons: BTreeMap<u8, String> }`.
- Produces: `load(path: &Path) -> Result<MappingConfig, String>` and `save(path: &Path, config: &MappingConfig) -> Result<(), String>`.
- Produces: `parse_press(line: &str) -> Option<Press>` and `reply(press: Press, mappings: &MappingConfig, copy: impl FnOnce(&str) -> Result<(), String>) -> Reply`.

- [ ] **Step 1: Write failing configuration tests**

Cover Unicode/multiline YAML, unsupported GPIO rejection, missing file as an
empty mapping, empty-value omission, and replacement failure preserving the
old file. The primary assertions are:

```rust
assert_eq!(config.buttons[&6], "你好\nsecond");
assert!(load(&path_with_gpio_10).unwrap_err().contains("GPIO10"));
assert_eq!(load(&missing).unwrap(), MappingConfig::default());
assert!(!saved_yaml.contains("7:"));
```

- [ ] **Step 2: Verify configuration tests fail**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml config::tests`

Expected: FAIL because `config.rs` is not implemented.

- [ ] **Step 3: Implement validation and same-directory atomic save**

Deserialize through a private YAML document, validate all keys against
`SUPPORTED_GPIOS`, filter empty strings on serialization, write
`.config.yaml.tmp`, `sync_all`, and rename it over the destination. Remove the
temporary file after any error.

- [ ] **Step 4: Write failing protocol tests**

```rust
assert_eq!(parse_press("PRESS 12 6\n"), Some(Press { event_id: 12, gpio: 6 }));
assert_eq!(parse_press("OTHER 12 6\n"), None);
assert_eq!(mapped_reply.line, "PASTE 12\n");
assert_eq!(unmapped_reply.line, "SKIP 12\n");
assert_eq!(clipboard_failure.line, "SKIP 12\n");
```

- [ ] **Step 5: Implement minimal protocol logic and run tests**

The reply includes its serial line and a user-facing log message. It calls the
clipboard closure only for non-empty mappings and returns `SKIP` when that call
fails.

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml`

Expected: all Rust tests pass.

- [ ] **Step 6: Commit**

```bash
rtk git add src-tauri/src/config.rs src-tauri/src/protocol.rs src-tauri/src/lib.rs
rtk git commit -m "feat: add Rust helper core"
```

### Task 3: Serial Worker And Tauri Commands

**Files:**
- Create: `src-tauri/src/device.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: serial filter `is_target_port(&SerialPortInfo) -> bool` for VID `0x303A` and product `ESP Vibe Text Keyboard`.
- Produces: frontend payloads `AppSnapshot { buttons, config_path, connection, config_error }`, `RuntimeEvent`, and `ConnectionStatus`, all serializable with camel-case fields.
- Produces commands `get_snapshot(State<AppState>) -> AppSnapshot` and `save_mappings(State<AppState>, BTreeMap<u8, String>) -> Result<AppSnapshot, String>`.
- Emits: `runtime-event` with a bounded frontend-compatible payload.

- [ ] **Step 1: Write failing target-device tests**

Build `SerialPortInfo` values for matching VID/product, wrong VID, wrong product,
and non-USB ports. Assert that only the exact match passes.

- [ ] **Step 2: Verify device test fails**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml device::tests`

Expected: FAIL because `device.rs` is not implemented.

- [ ] **Step 3: Implement the single worker**

The worker loops until an `AtomicBool` stop flag is set, discovers a matching
port, opens it at `115200` baud with a 500 ms timeout, parses complete lines,
calls `pbcopy` through `std::process::Command`, writes and flushes replies, and
emits connection and press events through `AppHandle::emit`. It waits 500 ms
between discovery/reconnect attempts and never owns a second mapping copy.

- [ ] **Step 4: Implement app state and commands**

During setup, resolve `app.path().app_config_dir()/config.yaml`, create the
directory, import root `config.yaml` only when the app file is absent, load the
config, manage `AppState`, and start one worker. A load failure creates explicit
empty mappings plus `config_error`; it must not silently use partial data.
`save_mappings` validates and saves before updating the shared `RwLock`, and a
successful save clears `config_error`.

Register both commands in one `generate_handler!` call. On Tauri exit, set the
stop flag and join the worker once.

- [ ] **Step 5: Run Rust formatting, tests, and checks**

Run: `rtk cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml`

Run: `rtk cargo check --manifest-path src-tauri/Cargo.toml`

Expected: all commands exit 0.

- [ ] **Step 6: Commit**

```bash
rtk git add src-tauri/src/device.rs src-tauri/src/lib.rs
rtk git commit -m "feat: run helper in Tauri backend"
```

### Task 4: Graphical Mapping Editor And Event Log

**Files:**
- Modify: `src/App.tsx`, `src/App.css`, `src/types.ts`
- Create: `src/App.test.tsx`, `src/test/setup.ts`
- Modify: `vite.config.ts`, `package.json`, `package-lock.json`

**Interfaces:**
- Consumes: `invoke<AppSnapshot>("get_snapshot")`, `invoke<AppSnapshot>("save_mappings", { buttons })`, and `listen<RuntimeEvent>("runtime-event", ...)`.
- Produces: accessible mapping selection/editor, dirty/save state, connection status, GPIO0 warning, and a 200-entry event log.

- [ ] **Step 1: Write the failing UI behavior test**

Mock Tauri `invoke` and `listen`, then verify:

```tsx
expect(await screen.findByDisplayValue("hello")).toBeInTheDocument();
await userEvent.clear(screen.getByRole("textbox", { name: "GPIO0 mapping" }));
await userEvent.type(screen.getByRole("textbox", { name: "GPIO0 mapping" }), "你好");
expect(screen.getByRole("button", { name: "Save mappings" })).toBeEnabled();
await userEvent.click(screen.getByRole("button", { name: "Save mappings" }));
expect(invoke).toHaveBeenCalledWith("save_mappings", expect.any(Object));
expect(screen.getByText(/download mode/i)).toBeInTheDocument();
```

Add separate assertions for a rejected save retaining edited text and an emitted
runtime event appearing in the log.

- [ ] **Step 2: Verify UI tests fail**

Run: `rtk npm test`

Expected: FAIL because the graphical workflow is not implemented.

- [ ] **Step 3: Implement the complete UI**

Load once on mount, subscribe once to `runtime-event`, and always run the
returned unlisten function on unmount. Keep `savedButtons` and editable
`buttons`; derive dirty state by comparing supported GPIO values. Implement
`Cmd+S`, disabled/loading save states, inline errors, row selection, multiline
textarea, and the bounded event list.

Use `Save`, `Usb`, `UsbOff`, and `AlertTriangle` from `lucide-react`. Use native
buttons and textarea, visible focus styles, fixed grid tracks, and a responsive
single-column breakpoint. Do not add a component library, router, state library,
gradient, nested cards, or decorative animation.

- [ ] **Step 4: Run UI tests and production build**

Run: `rtk npm test`

Run: `rtk npm run build`

Expected: both commands exit 0.

- [ ] **Step 5: Commit**

```bash
rtk git add package.json package-lock.json vite.config.ts src
rtk git commit -m "feat: add graphical helper interface"
```

### Task 5: Replace Python Entry Point And Package The App

**Files:**
- Delete: `host/__init__.py`, `host/text_helper.py`, `test/test_helper.py`
- Modify: `Makefile`, `pyproject.toml`, `uv.lock`, `.gitignore`
- Modify: `docs/superpowers/specs/2026-07-26-gpio-text-keyboard-design.md`
- Create: `src-tauri/icons/*` generated from `assets/tel.jpg`

**Interfaces:**
- Changes: `make helper` launches `npm run tauri dev`.
- Produces: `make helper-build` runs `npm run tauri build -- --bundles app`.
- Keeps: `make test` covers PlatformIO, Rust, and frontend tests.

- [ ] **Step 1: Generate Tauri icons**

Run: `rtk npm run tauri icon assets/tel.jpg`

Expected: Tauri creates the required icon set under `src-tauri/icons`.

- [ ] **Step 2: Switch development commands and dependencies**

Remove `pyserial` and `pyyaml` from `pyproject.toml`, refresh `uv.lock`, change
`helper` to the Tauri dev command, add `helper-build`, and make `test` run the
existing native firmware test plus `cargo test` and `npm test`.

- [ ] **Step 3: Remove the replaced implementation**

Delete the Python helper package and its tests only after Rust and frontend
parity tests are green. Update the GPIO keyboard design's helper and verification
sections to describe the Tauri app and app-config YAML path.

- [ ] **Step 4: Run all automated verification**

Run: `rtk make test`

Run: `rtk npm run build`

Run: `rtk cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`

Run: `rtk git diff --check`

Expected: all commands exit 0.

- [ ] **Step 5: Build the macOS application**

Run: `rtk npm run tauri build -- --bundles app`

Expected: exit 0 and create `src-tauri/target/release/bundle/macos/Vibe Tool.app`.

- [ ] **Step 6: Commit**

```bash
rtk git add Makefile pyproject.toml uv.lock .gitignore docs/superpowers/specs/2026-07-26-gpio-text-keyboard-design.md src-tauri/icons host test
rtk git commit -m "refactor: replace Python helper with Tauri app"
```

### Task 6: Real Desktop And Device Verification

**Files:**
- Modify only files needed to correct issues exposed by the checks.

**Interfaces:**
- Proves: the built Tauri app is visible, interactive, persists Unicode mappings, reconnects, and drives real firmware behavior.

- [ ] **Step 1: Launch the development app**

Run: `rtk npm run tauri dev`

Expected: a native `Vibe Tool` window opens without frontend or Rust errors.

- [ ] **Step 2: Inspect desktop and narrow layouts**

Capture the real window at its default size and near its minimum width. Verify
that the mapping list, selected editor, Save action, connection state, and event
log are visible with no overlap, clipping, blank render, or unstable resizing.

- [ ] **Step 3: Verify persistence**

Enter a multiline Unicode value, save, restart the app, and verify the exact
value reloads from the displayed app configuration path.

- [ ] **Step 4: Verify attached hardware when available**

Press one mapped and one unmapped GPIO. Verify the event log shows `PASTE` and
`SKIP`, respectively, and read the macOS clipboard to confirm the mapped Unicode
text. Disconnect and reconnect the USB device and verify status recovery.

- [ ] **Step 5: Final audit**

Confirm the repository has no Python helper imports or entry points, all explicit
design requirements have direct evidence, unrelated pre-existing worktree
changes remain, and the packaged `.app` launches.
