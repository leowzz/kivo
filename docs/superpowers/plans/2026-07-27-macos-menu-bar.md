# macOS Menu Bar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run Vibe Tool without a Dock icon and expose live device status, main-window access, and clean exit through a native macOS menu-bar item.

**Architecture:** A focused Rust `tray` module owns the Tauri tray icon, native menu, status text, and menu action routing. The existing serial worker remains the authoritative connection-state producer and asks the tray module to refresh only when that state changes; the existing React window and shutdown flow remain unchanged.

**Tech Stack:** Tauri 2.11 Rust tray/menu APIs, macOS `Accessory` activation policy, existing Rust/React application, Lucide USB artwork rasterized with `rsvg-convert`.

## Global Constraints

- Target macOS only and do not show a Dock icon.
- Use a native tray menu, not a custom popover or second window.
- Menu order is status, separator, `Open Vibe Tool`, `Quit Vibe Tool`.
- Status text is `Connected - <port>` or `Waiting for device` and updates from the existing `ConnectionStatus` value.
- Closing the main window hides it; opening from the menu restores and focuses the same window.
- Quitting must stop and join the existing single serial worker.
- Do not add a new dependency, frontend state, startup-at-login behavior, or protocol change.
- Preserve unrelated worktree changes and do not push.

---

### Task 1: Tray Status And Action Contract

**Files:**
- Create: `src-tauri/src/tray.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `device::ConnectionStatus` and `device::ConnectionState`.
- Produces: `status_label(&ConnectionStatus) -> String`.
- Produces: private `TrayAction::{Open, Quit}` and `action_for(&str) -> Option<TrayAction>`.

- [ ] **Step 1: Add the module and failing contract tests**

Add `mod tray;` next to the existing modules in `src-tauri/src/lib.rs`. Create
`src-tauri/src/tray.rs` with tests that name the required functions before they
exist:

```rust
use crate::device::{ConnectionState, ConnectionStatus};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_waiting_and_connected_status() {
        assert_eq!(
            status_label(&ConnectionStatus {
                state: ConnectionState::Searching,
                port: None,
            }),
            "Waiting for device"
        );
        assert_eq!(
            status_label(&ConnectionStatus {
                state: ConnectionState::Connected,
                port: Some("/dev/cu.test".to_owned()),
            }),
            "Connected - /dev/cu.test"
        );
    }

    #[test]
    fn routes_only_known_menu_ids() {
        assert_eq!(action_for("open-main"), Some(TrayAction::Open));
        assert_eq!(action_for("quit-app"), Some(TrayAction::Quit));
        assert_eq!(action_for("status"), None);
        assert_eq!(action_for("unknown"), None);
    }
}
```

- [ ] **Step 2: Run the tests and confirm the red state**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml tray::tests`

Expected: compilation fails because `status_label`, `action_for`, and
`TrayAction` are not defined.

- [ ] **Step 3: Implement the smallest pure contract**

Add these definitions above the test module:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrayAction {
    Open,
    Quit,
}

fn action_for(id: &str) -> Option<TrayAction> {
    match id {
        "open-main" => Some(TrayAction::Open),
        "quit-app" => Some(TrayAction::Quit),
        _ => None,
    }
}

fn status_label(connection: &ConnectionStatus) -> String {
    match (&connection.state, &connection.port) {
        (ConnectionState::Connected, Some(port)) => format!("Connected - {port}"),
        _ => "Waiting for device".to_owned(),
    }
}
```

- [ ] **Step 4: Verify the focused tests pass**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml tray::tests`

Expected: 2 tray tests pass.

- [ ] **Step 5: Commit the contract**

```bash
rtk git add src-tauri/src/tray.rs src-tauri/src/lib.rs
rtk git commit -m "test: define menu bar behavior"
```

---

### Task 2: Native Tray And Lifecycle Integration

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify: `src-tauri/src/tray.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/device.rs`
- Create: `src-tauri/icons/tray-icon.svg`
- Create: `src-tauri/icons/tray-icon.png`

**Interfaces:**
- Consumes: Task 1's `status_label` and `action_for` functions.
- Produces: `tray::setup(&mut tauri::App, &ConnectionStatus) -> tauri::Result<()>`.
- Produces: `tray::update_connection(&tauri::AppHandle, &ConnectionStatus)`.
- Stores: one managed `TrayState` containing the disabled status `MenuItem` and registered `TrayIcon`.

- [ ] **Step 1: Enable Tauri's existing optional tray and PNG features**

Change the existing dependency without adding a crate:

```toml
tauri = { version = "2.11.5", features = ["image-png", "tray-icon"] }
```

Run: `rtk cargo check --manifest-path src-tauri/Cargo.toml`

Expected: Cargo resolves the optional Tauri features and exits 0; the lockfile
updates mechanically if those optional packages were not already selected.

- [ ] **Step 2: Add the Lucide-derived template icon**

Create `src-tauri/icons/tray-icon.svg` with the installed Lucide `Usb` geometry:

```svg
<svg xmlns="http://www.w3.org/2000/svg" width="36" height="36" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
  <circle cx="10" cy="7" r="1"/>
  <circle cx="4" cy="20" r="1"/>
  <path d="M4.7 19.3 19 5"/>
  <path d="m21 3-3 1 2 2Z"/>
  <path d="M9.26 7.68 5 12l2 5"/>
  <path d="m10 14 5 2 3.5-3.5"/>
  <path d="m18 12 1-1 1 1-1 1Z"/>
</svg>
```

Run: `rtk rsvg-convert -w 36 -h 36 -o src-tauri/icons/tray-icon.png src-tauri/icons/tray-icon.svg`

Run: `rtk file src-tauri/icons/tray-icon.png`

Expected: a 36 x 36 RGBA PNG.

- [ ] **Step 3: Implement native tray setup and updates**

Extend `src-tauri/src/tray.rs` with these imports and functions while retaining
Task 1's tests:

```rust
use tauri::{
    App, AppHandle, Manager,
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{TrayIcon, TrayIconBuilder},
};

struct TrayState {
    status: MenuItem<tauri::Wry>,
    tray: TrayIcon<tauri::Wry>,
}

pub fn setup(app: &mut App, initial: &ConnectionStatus) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);

    let label = status_label(initial);
    let status = MenuItem::with_id(app, "status", &label, false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open-main", "Open Vibe Tool", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit-app", "Quit Vibe Tool", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&status, &separator, &open, &quit])?;
    let icon = Image::from_bytes(include_bytes!("../icons/tray-icon.png"))?;
    let tooltip = format!("Vibe Tool - {label}");
    let tray = TrayIconBuilder::with_id("menu-bar")
        .icon(icon)
        .icon_as_template(true)
        .tooltip(&tooltip)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            if let Some(action) = action_for(event.id().as_ref()) {
                handle_action(app, action);
            }
        })
        .build(app)?;

    app.manage(TrayState { status, tray });
    Ok(())
}

pub fn update_connection(app: &AppHandle, connection: &ConnectionStatus) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    let label = status_label(connection);
    let _ = state.status.set_text(&label);
    let _ = state.tray.set_tooltip(Some(format!("Vibe Tool - {label}")));
}

fn handle_action(app: &AppHandle, action: TrayAction) {
    match action {
        TrayAction::Open => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        TrayAction::Quit => app.exit(0),
    }
}
```

- [ ] **Step 4: Initialize the tray before the serial worker**

In `src-tauri/src/lib.rs`, replace the current connection initialization with
an owned initial value, and initialize the tray before `thread::spawn`:

```rust
let initial_connection = ConnectionStatus::searching();
tray::setup(app, &initial_connection)?;
let connection = Arc::new(RwLock::new(initial_connection));
```

Keep the existing window-close hide behavior and exit-thread join behavior.

- [ ] **Step 5: Refresh the tray from authoritative connection changes**

In `device::set_connection`, retain `next` for the tray update by cloning it
when writing shared state:

```rust
*current = next.clone();
```

Then update the native status before emitting the frontend event:

```rust
if changed {
    crate::tray::update_connection(app, &next);
    emit(
        app,
        connection,
        EventLevel::Info,
        message.unwrap_or_else(|| "Waiting for device".to_owned()),
    );
}
```

- [ ] **Step 6: Run focused and complete Rust verification**

Run: `rtk cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml`

Run: `rtk cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`

Expected: formatting passes, 15 Rust tests pass, and Clippy reports no issues.

- [ ] **Step 7: Commit the native integration**

```bash
rtk git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/tray.rs src-tauri/src/lib.rs src-tauri/src/device.rs src-tauri/icons/tray-icon.svg src-tauri/icons/tray-icon.png
rtk git commit -m "feat: add macOS menu bar controls"
```

---

### Task 3: Packaged macOS Verification

**Files:**
- Modify only files required to correct failures found by the smoke test.

**Interfaces:**
- Proves: the signed packaged application is accessible from the menu bar without a Dock icon and preserves serial behavior.

- [ ] **Step 1: Run all repository verification**

Run: `rtk make test`

Run: `rtk npm run build`

Run: `rtk cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`

Run: `rtk git diff --check`

Expected: firmware 10 tests, Rust 15 tests, and React 5 tests pass; production
frontend build and Clippy exit 0; diff check prints no errors.

- [ ] **Step 2: Build and verify the signed application**

Run: `rtk npm run tauri build -- --bundles app`

Run: `rtk codesign --verify --deep --strict 'src-tauri/target/release/bundle/macos/Vibe Tool.app'`

Expected: both commands exit 0.

- [ ] **Step 3: Launch one packaged process and verify Accessory policy**

Quit any Vibe Tool process started by this plan, then run:

```bash
rtk open 'src-tauri/target/release/bundle/macos/Vibe Tool.app'
rtk pgrep -fl '/Vibe Tool.app/Contents/MacOS/vibe-tool'
```

Use the returned PID in:

```bash
rtk swift -e 'import AppKit; let pid = pid_t(<PID>); print(NSRunningApplication(processIdentifier: pid)?.activationPolicy.rawValue ?? -1)'
```

Expected: exactly one packaged process and activation-policy raw value `1`
(`Accessory`), with no Vibe Tool Dock icon.

- [ ] **Step 4: Inspect and exercise the native menu**

Capture the display containing the menu-bar icon and inspect it at original
resolution. Click the icon and verify the native menu contains, in order, the
disabled live status, separator, `Open Vibe Tool`, and `Quit Vibe Tool`.

Close the main window, choose `Open Vibe Tool`, and verify the same window is
visible and focused. Unplug the USB device and verify the status becomes
`Waiting for device`; reconnect it and verify the menu returns to
`Connected - /dev/cu.usbmodem...`.

- [ ] **Step 5: Verify clean quit and relaunch**

Choose `Quit Vibe Tool` from the menu. Verify the process exits and
`rtk lsof '/dev/cu.usbmodem...'` finds no owner. Relaunch the packaged app and
verify the tray item returns and the device reconnects.

- [ ] **Step 6: Commit any smoke-test corrections**

If the smoke test required code corrections, stage only those files and commit:

```bash
rtk git commit -m "fix: complete menu bar lifecycle"
```

If no files changed, do not create an empty commit.
