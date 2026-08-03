# macOS Menu Bar Action Summary Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show each available macOS Device's assigned button-action summaries in Kivo's native menu bar menu, with flat single-Device presentation and hover submenus for full action sequences.

**Architecture:** Add a pure Rust tray-menu projection under `src-tauri/src/tray/model.rs`, leaving Tauri object construction in the existing `tray.rs` adapter. The background scanner derives a complete comparable model from the current `DeviceStatus` list and `Workspace`; the main thread replaces the native menu only when that model changes.

**Tech Stack:** Rust 2024 edition, Tauri 2.11.5 native `Menu` / `Submenu` / `TrayIcon` APIs, Tauri mock runtime for menu-structure tests, existing Kivo `Workspace`, `DeviceStatus`, `DeviceProfile`, and `ButtonAction` types.

## Global Constraints

- Run every shell command through `rtk`.
- Keep the feature macOS-only under the existing `#[cfg(target_os = "macos")]` module boundary.
- Add no production dependency and do not change the persisted schema or Runtime protocol.
- The menu is read-only; only `open-main` and `quit-app` remain executable menu IDs.
- Available Devices are online Runtime-mode Devices with valid identity, valid Runtime Assignment, and resolvable Device Profile plus Hardware Profile.
- Keep Devices with `configuring`, `learning`, or `runtime_error` Runtime dimensions in the behavior menu.
- Show zero Devices as an empty message, one Device as flat buttons, and two or more Devices as Device submenus.
- Keep every button in Device Profile group/button declaration order, including unconfigured buttons.
- Primary paste summaries use 12 Unicode characters; hover detail summaries use 80 Unicode characters.
- A primary button summary shows only the first action and `+N`; its submenu shows every action step in execution order.
- Static menu copy follows `Workspace.settings.language` for `zh-CN` and `en-US`.
- Never claim macOS native hover acceptance from unit tests or builds; mark it Not Run if no usable physical Device is available.

## File Structure

- Create `src-tauri/src/tray/model.rs`: pure menu-model structs, localized static copy, text sanitation, action formatting, Device filtering, assignment resolution, and flat/grouped projection.
- Modify `src-tauri/src/tray.rs`: native Tauri menu construction, model-change gate, Tray state, setup/update entry points, and fixed command routing.
- Modify `src-tauri/src/lib.rs`: pass the current read-only Workspace into initial Tray setup and each Device-scan refresh.
- Modify `src-tauri/Cargo.toml`: add the dev-only `tempfile` fixture dependency and enable Tauri's `test` feature only for dev builds so native menu structure can be tested with `tauri::test::mock_app`.

---

### Task 1: Format localized button-action summaries

**Files:**
- Create: `src-tauri/src/tray/model.rs`
- Modify: `src-tauri/src/tray.rs:1`

**Interfaces:**
- Consumes: `crate::model::ButtonDefinition`, `crate::profile::ButtonAction`, and `crate::workspace::Language`.
- Produces: `TrayCopy`, `TrayButton`, `copy(Language) -> TrayCopy`, and `button_summary(&ButtonDefinition, &[ButtonAction], Language) -> TrayButton` for Task 2.

- [ ] **Step 1: Add the module and write failing formatting tests**

Add `mod model;` at the top of `src-tauri/src/tray.rs`. Create `src-tauri/src/tray/model.rs` with tests that state the complete formatting contract before defining the referenced functions:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::ButtonDefinition,
        profile::ButtonAction,
        workspace::Language,
    };

    #[test]
    fn formats_paste_preview_detail_and_remaining_count() {
        let button = ButtonDefinition {
            id: "CONFIRM".into(),
            label: "确认 & 发送".into(),
        };
        let actions = vec![
            ButtonAction::Paste {
                text: "  订单号\n1234567890  ".into(),
            },
            ButtonAction::Hotkey {
                keys: vec!["cmd".into(), "shift".into(), "k".into()],
            },
            ButtonAction::Paste {
                text: "A\r\nB\0C".into(),
            },
        ];

        let summary = button_summary(&button, &actions, Language::ZhCn);

        assert_eq!(summary.title, "确认 && 发送 · “订单号 12345678…” +2");
        assert_eq!(
            summary.details,
            vec!["“订单号 ↵ 1234567890”", "⌘⇧K", "“A ↵ BC”"],
        );
    }

    #[test]
    fn formats_macos_key_abbreviations_and_unconfigured_copy() {
        let button = ButtonDefinition {
            id: "KEY".into(),
            label: "Key".into(),
        };
        let keys = [
            (vec!["cmd", "c"], "⌘C"),
            (vec!["ctrl", "alt", "shift", "enter"], "⌃⌥⇧↩"),
            (vec!["option", "1"], "⌥1"),
            (vec!["tab"], "⇥"),
            (vec!["backspace"], "⌫"),
            (vec!["escape"], "Esc"),
            (vec!["delete"], "Del"),
            (vec!["space"], "Space"),
            (vec!["up", "down", "left", "right"], "↑↓←→"),
            (vec!["home", "end", "page_up", "page_down"], "HomeEndPgUpPgDn"),
        ];
        for (configured, expected) in keys {
            let action = ButtonAction::Hotkey {
                keys: configured.into_iter().map(str::to_owned).collect(),
            };
            assert_eq!(format_action(&action, SummaryKind::Primary), expected);
        }

        let long_paste = ButtonAction::Paste { text: "界".repeat(81) };
        assert_eq!(
            format_action(&long_paste, SummaryKind::Detail),
            format!("“{}…”", "界".repeat(80)),
        );

        assert_eq!(
            button_summary(&button, &[], Language::ZhCn).title,
            "Key · 未配置",
        );
        assert_eq!(
            button_summary(&button, &[], Language::EnUs).title,
            "Key · Not configured",
        );
    }

    #[test]
    fn localizes_all_static_tray_copy() {
        assert_eq!(
            copy(Language::ZhCn),
            TrayCopy {
                waiting: "等待设备",
                empty: "暂无可用设备",
                unconfigured: "未配置",
                open: "打开 Kivo",
                quit: "退出 Kivo",
            },
        );
        assert_eq!(
            copy(Language::EnUs),
            TrayCopy {
                waiting: "Waiting for device",
                empty: "No available device",
                unconfigured: "Not configured",
                open: "Open Kivo",
                quit: "Quit Kivo",
            },
        );
    }
}
```

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```bash
rtk cargo test --manifest-path src-tauri/Cargo.toml tray::model::tests --no-fail-fast
```

Expected: compilation fails because `TrayCopy`, `SummaryKind`, `copy`, `format_action`, and `button_summary` do not exist yet. The failure must come from the missing formatting API, not from a malformed fixture.

- [ ] **Step 3: Implement the minimal formatting model**

Add these types and helpers above the tests in `src-tauri/src/tray/model.rs`:

```rust
use crate::{
    model::ButtonDefinition,
    profile::ButtonAction,
    workspace::Language,
};

const PRIMARY_PASTE_LIMIT: usize = 12;
const DETAIL_PASTE_LIMIT: usize = 80;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TrayCopy {
    pub waiting: &'static str,
    pub empty: &'static str,
    pub unconfigured: &'static str,
    pub open: &'static str,
    pub quit: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TrayButton {
    pub title: String,
    pub details: Vec<String>,
}

#[derive(Clone, Copy)]
enum SummaryKind {
    Primary,
    Detail,
}

pub(super) fn copy(language: Language) -> TrayCopy {
    match language {
        Language::ZhCn => TrayCopy {
            waiting: "等待设备",
            empty: "暂无可用设备",
            unconfigured: "未配置",
            open: "打开 Kivo",
            quit: "退出 Kivo",
        },
        Language::EnUs => TrayCopy {
            waiting: "Waiting for device",
            empty: "No available device",
            unconfigured: "Not configured",
            open: "Open Kivo",
            quit: "Quit Kivo",
        },
    }
}

pub(super) fn button_summary(
    button: &ButtonDefinition,
    actions: &[ButtonAction],
    language: Language,
) -> TrayButton {
    let label = escape_menu_text(&collapse_whitespace(&button.label));
    let Some(first) = actions.first() else {
        return TrayButton {
            title: format!("{label} · {}", copy(language).unconfigured),
            details: Vec::new(),
        };
    };
    let remaining = actions.len() - 1;
    let suffix = if remaining == 0 {
        String::new()
    } else {
        format!(" +{remaining}")
    };
    TrayButton {
        title: format!("{label} · {}{suffix}", format_action(first, SummaryKind::Primary)),
        details: actions
            .iter()
            .map(|action| format_action(action, SummaryKind::Detail))
            .collect(),
    }
}

fn format_action(action: &ButtonAction, kind: SummaryKind) -> String {
    match action {
        ButtonAction::Paste { text } => {
            let (cleaned, limit) = match kind {
                SummaryKind::Primary => (collapse_whitespace(text), PRIMARY_PASTE_LIMIT),
                SummaryKind::Detail => (visible_line_breaks(text), DETAIL_PASTE_LIMIT),
            };
            format!("“{}”", escape_menu_text(&truncate_chars(&cleaned, limit)))
        }
        ButtonAction::Hotkey { keys } => keys
            .iter()
            .map(|key| key_abbreviation(key))
            .collect::<Vec<_>>()
            .join(""),
    }
}

fn key_abbreviation(key: &str) -> String {
    match key.to_ascii_lowercase().as_str() {
        "cmd" => "⌘".into(),
        "alt" | "option" => "⌥".into(),
        "ctrl" => "⌃".into(),
        "shift" => "⇧".into(),
        "enter" => "↩".into(),
        "tab" => "⇥".into(),
        "backspace" => "⌫".into(),
        "escape" => "Esc".into(),
        "delete" => "Del".into(),
        "space" => "Space".into(),
        "up" => "↑".into(),
        "down" => "↓".into(),
        "left" => "←".into(),
        "right" => "→".into(),
        "home" => "Home".into(),
        "end" => "End".into(),
        "page_up" | "pageup" => "PgUp".into(),
        "page_down" | "pagedown" => "PgDn".into(),
        other => other.to_uppercase(),
    }
}

fn collapse_whitespace(value: &str) -> String {
    let without_controls = value
        .chars()
        .filter(|character| !character.is_control() || character.is_whitespace())
        .collect::<String>();
    without_controls.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn visible_line_breaks(value: &str) -> String {
    let lines = value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', " ↵ ");
    collapse_whitespace(&lines)
}

fn truncate_chars(value: &str, limit: usize) -> String {
    let mut characters = value.chars();
    let prefix = characters.by_ref().take(limit).collect::<String>();
    if characters.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

fn escape_menu_text(value: &str) -> String {
    value.replace('&', "&&")
}
```

- [ ] **Step 4: Run the focused test and verify GREEN**

Run:

```bash
rtk cargo test --manifest-path src-tauri/Cargo.toml tray::model::tests --no-fail-fast
```

Expected: all three formatting tests pass, with no warnings.

- [ ] **Step 5: Commit the formatting unit**

```bash
rtk git add src-tauri/src/tray.rs src-tauri/src/tray/model.rs
rtk git commit -m "feat: format tray action summaries"
```

### Task 2: Project available Devices into flat or grouped menu sections

**Files:**
- Modify: `src-tauri/src/tray/model.rs`

**Interfaces:**
- Consumes: `button_summary` and `copy` from Task 1, `DeviceStatus`, and `Workspace`.
- Produces: `TrayMenuModel::from_workspace(devices: &[DeviceStatus], workspace: &Workspace) -> TrayMenuModel`, plus `TrayDeviceSection::{Empty, Flat, Grouped}` for Task 3.

- [ ] **Step 1: Write failing Device projection tests**

Add the dev-only fixture dependency, then extend the `tests` module in `src-tauri/src/tray/model.rs` with fixture helpers and these assertions. The helpers construct real Kivo domain values rather than renderer-only stand-ins:

```toml
[dev-dependencies]
tempfile = "3"
```

```rust
use crate::{
    coordinator::{
        AssignmentDimension, ConnectionDimension, DeviceMode, DeviceStatus,
        IdentityDimension, RuntimeDimension,
    },
    hardware::DeviceId,
    model::{ButtonGroup, ModelLayout},
    profile::{DeviceProfile, HardwareProfile, PROFILE_SCHEMA_VERSION},
    workspace::{RuntimeAssignment, Workspace},
};
use std::collections::BTreeMap;

fn profile() -> DeviceProfile {
    DeviceProfile {
        schema_version: PROFILE_SCHEMA_VERSION,
        profile: ModelLayout {
            id: "desk-profile".into(),
            name: "Desk Profile".into(),
            groups: vec![
                ButtonGroup {
                    id: "top".into(),
                    columns: 1,
                    buttons: vec![ButtonDefinition { id: "B".into(), label: "B".into() }],
                },
                ButtonGroup {
                    id: "bottom".into(),
                    columns: 1,
                    buttons: vec![ButtonDefinition { id: "A".into(), label: "A".into() }],
                },
            ],
        },
        hardware_profiles: vec![HardwareProfile {
            id: "hardware".into(),
            name: "Hardware".into(),
            board_profile_id: "luatos-esp32s3-aio".into(),
            debounce_ms: 30,
            inputs: Vec::new(),
        }],
        actions: BTreeMap::from([(
            "B".into(),
            vec![ButtonAction::Hotkey { keys: vec!["cmd".into(), "b".into()] }],
        )]),
    }
}

fn device(serial: &str, name: &str) -> DeviceStatus {
    DeviceStatus {
        device_id: DeviceId::new("luatos-esp32s3-aio", serial).unwrap(),
        name: name.into(),
        connection: ConnectionDimension::Online,
        mode: Some(DeviceMode::Runtime),
        identity: IdentityDimension::Valid,
        assignment: AssignmentDimension::Valid,
        runtime: RuntimeDimension::Ready,
        raw_serial: serial.into(),
        port: Some(format!("/dev/{serial}")),
        controller_family_id: "esp32s3".into(),
        board_profile_id: "luatos-esp32s3-aio".into(),
        firmware_build_id: Some("test".into()),
        pins: vec![1],
        runtime_assignment: Some(RuntimeAssignment {
            device_profile_id: "desk-profile".into(),
            hardware_profile_id: "hardware".into(),
        }),
        latest_error: None,
        learning: None,
    }
}

fn workspace_with_profile(profile: DeviceProfile) -> Workspace {
    let directory = tempfile::tempdir().unwrap();
    Workspace::create(directory.path(), vec![profile]).unwrap()
}

#[test]
fn selects_empty_flat_and_grouped_device_sections() {
    let workspace = workspace_with_profile(profile());
    let empty = TrayMenuModel::from_workspace(&[], &workspace);
    assert!(matches!(empty.devices, TrayDeviceSection::Empty(ref label) if label == "暂无可用设备"));

    let front = device("FRONT", "前台 & 键盘");
    let flat = TrayMenuModel::from_workspace(std::slice::from_ref(&front), &workspace);
    let TrayDeviceSection::Flat(buttons) = flat.devices else { panic!("expected flat buttons") };
    assert_eq!(buttons.iter().map(|button| button.title.as_str()).collect::<Vec<_>>(), vec!["B · ⌘B", "A · 未配置"]);

    let back = device("BACK", "后台键盘");
    let grouped = TrayMenuModel::from_workspace(&[front, back], &workspace);
    let TrayDeviceSection::Grouped(devices) = grouped.devices else { panic!("expected grouped devices") };
    assert_eq!(devices.iter().map(|device| device.name.as_str()).collect::<Vec<_>>(), vec!["前台 && 键盘", "后台键盘"]);
    assert!(devices.iter().all(|device| device.buttons.len() == 2));
}

#[test]
fn filters_only_addressable_runtime_devices_without_filtering_runtime_dimension() {
    let workspace = workspace_with_profile(profile());
    let valid = device("VALID", "Valid");
    for runtime in [
        RuntimeDimension::Configuring,
        RuntimeDimension::Learning,
        RuntimeDimension::Ready,
        RuntimeDimension::RuntimeError,
    ] {
        let mut candidate = valid.clone();
        candidate.runtime = runtime;
        let model = TrayMenuModel::from_workspace(&[candidate], &workspace);
        assert!(matches!(model.devices, TrayDeviceSection::Flat(_)));
    }

    let mut excluded = Vec::new();
    let mut offline = valid.clone();
    offline.connection = ConnectionDimension::Offline;
    excluded.push(offline);
    let mut bootloader = valid.clone();
    bootloader.mode = Some(DeviceMode::Bootloader);
    excluded.push(bootloader);
    let mut invalid_identity = valid.clone();
    invalid_identity.identity = IdentityDimension::InvalidIdentity;
    excluded.push(invalid_identity);
    let mut unassigned = valid.clone();
    unassigned.assignment = AssignmentDimension::Unassigned;
    unassigned.runtime_assignment = None;
    excluded.push(unassigned);
    let mut missing_profile = valid;
    missing_profile.runtime_assignment.as_mut().unwrap().device_profile_id = "missing".into();
    excluded.push(missing_profile);
    let mut missing_hardware = device("MISSING-HARDWARE", "Missing Hardware");
    missing_hardware.runtime_assignment.as_mut().unwrap().hardware_profile_id = "missing".into();
    excluded.push(missing_hardware);

    let model = TrayMenuModel::from_workspace(&excluded, &workspace);
    assert!(matches!(model.devices, TrayDeviceSection::Empty(_)));
}

#[test]
fn localizes_registry_status_without_hiding_nonready_online_devices() {
    let workspace = workspace_with_profile(profile());
    let ready = device("READY", "Ready");
    let mut error = device("ERROR", "Error");
    error.runtime = RuntimeDimension::RuntimeError;
    let mut bootloader = device("BOOT", "Boot");
    bootloader.mode = Some(DeviceMode::Bootloader);
    bootloader.runtime = RuntimeDimension::Inactive;

    let model = TrayMenuModel::from_workspace(&[ready, error, bootloader], &workspace);
    assert_eq!(model.status_label, "3 台在线 · 1 台就绪 · 1 台引导模式 · 1 个错误");
}
```

- [ ] **Step 2: Run the projection tests and verify RED**

Run:

```bash
rtk cargo test --manifest-path src-tauri/Cargo.toml tray::model::tests --no-fail-fast
```

Expected: compilation fails because `TrayMenuModel`, `TrayDeviceSection`, and `TrayMenuModel::from_workspace` do not exist.

- [ ] **Step 3: Implement Device eligibility and menu projection**

Add the projection types and constructor above the tests in `src-tauri/src/tray/model.rs`:

```rust
use crate::{
    coordinator::{
        AssignmentDimension, ConnectionDimension, DeviceMode, DeviceStatus,
        IdentityDimension, RuntimeDimension,
    },
    hardware::DeviceId,
    workspace::Workspace,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TrayMenuModel {
    pub status_label: String,
    pub devices: TrayDeviceSection,
    pub open_label: String,
    pub quit_label: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum TrayDeviceSection {
    Empty(String),
    Flat(Vec<TrayButton>),
    Grouped(Vec<TrayDevice>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TrayDevice {
    pub id: DeviceId,
    pub name: String,
    pub buttons: Vec<TrayButton>,
}

impl TrayMenuModel {
    pub(super) fn from_workspace(devices: &[DeviceStatus], workspace: &Workspace) -> Self {
        let language = workspace.settings.language;
        let labels = copy(language);
        let available = devices
            .iter()
            .filter_map(|device| {
                if device.connection != ConnectionDimension::Online
                    || device.mode != Some(DeviceMode::Runtime)
                    || device.identity != IdentityDimension::Valid
                    || device.assignment != AssignmentDimension::Valid
                    || !matches!(
                        device.runtime,
                        RuntimeDimension::Configuring
                            | RuntimeDimension::Learning
                            | RuntimeDimension::Ready
                            | RuntimeDimension::RuntimeError
                    )
                {
                    return None;
                }
                let assignment = device.runtime_assignment.as_ref()?;
                let profile = workspace.profiles.get(&assignment.device_profile_id)?;
                profile.hardware_profile(&assignment.hardware_profile_id)?;
                Some(TrayDevice {
                    id: device.device_id.clone(),
                    name: escape_menu_text(&collapse_whitespace(&device.name)),
                    buttons: profile
                        .profile
                        .groups
                        .iter()
                        .flat_map(|group| &group.buttons)
                        .map(|button| {
                            button_summary(
                                button,
                                profile.actions.get(&button.id).map(Vec::as_slice).unwrap_or(&[]),
                                language,
                            )
                        })
                        .collect(),
                })
            })
            .collect::<Vec<_>>();
        let section = match available.len() {
            0 => TrayDeviceSection::Empty(labels.empty.into()),
            1 => TrayDeviceSection::Flat(available.into_iter().next().unwrap().buttons),
            _ => TrayDeviceSection::Grouped(available),
        };
        Self {
            status_label: registry_status(devices, language),
            devices: section,
            open_label: labels.open.into(),
            quit_label: labels.quit.into(),
        }
    }
}

fn registry_status(devices: &[DeviceStatus], language: Language) -> String {
    let online = devices
        .iter()
        .filter(|device| device.connection == ConnectionDimension::Online)
        .collect::<Vec<_>>();
    if online.is_empty() {
        return copy(language).waiting.into();
    }
    let ready = online
        .iter()
        .filter(|device| device.runtime == RuntimeDimension::Ready)
        .count();
    let bootloader = online
        .iter()
        .filter(|device| device.mode == Some(DeviceMode::Bootloader))
        .count();
    let errors = online
        .iter()
        .filter(|device| device.runtime == RuntimeDimension::RuntimeError)
        .count();
    match language {
        Language::ZhCn => format!(
            "{} 台在线 · {ready} 台就绪 · {bootloader} 台引导模式 · {errors} 个错误",
            online.len(),
        ),
        Language::EnUs => format!(
            "{} online · {ready} ready · {bootloader} bootloader · {errors} errors",
            online.len(),
        ),
    }
}
```

- [ ] **Step 4: Run the model tests and full tray filter**

Run:

```bash
rtk cargo test --manifest-path src-tauri/Cargo.toml tray::model::tests --no-fail-fast
rtk cargo test --manifest-path src-tauri/Cargo.toml tray --no-fail-fast
```

Expected: all new model tests and the two existing Tray tests pass. Resolve warnings before continuing.

- [ ] **Step 5: Commit the projection unit**

```bash
rtk git add src-tauri/src/tray/model.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
rtk git commit -m "feat: project device actions into tray menu"
```

`Cargo.toml` and `Cargo.lock` are part of this commit because `tempfile` is a direct dev dependency.

### Task 3: Rebuild the native menu only when the projection changes

**Files:**
- Modify: `src-tauri/src/tray.rs`
- Modify: `src-tauri/src/lib.rs:770-820`
- Modify: `src-tauri/Cargo.toml`

**Interfaces:**
- Consumes: `TrayMenuModel`, `TrayDeviceSection`, `TrayButton`, and `Workspace` from Tasks 1-2.
- Produces: `setup(app: &mut App, initial: &[DeviceStatus], workspace: &Workspace) -> tauri::Result<()>`, `update(app: &AppHandle, devices: &[DeviceStatus], workspace: &Workspace)`, and a complete native macOS menu.

- [ ] **Step 1: Enable the mock runtime and write failing native-menu/change-gate tests**

Extend the existing dev dependencies without changing the production Tauri dependency:

```toml
[dev-dependencies]
tempfile = "3"
tauri = { version = "2.11.5", features = ["test"] }
```

Replace the existing `tray.rs` test expectations for `status_label` with model tests from Task 2, keep `routes_only_known_menu_ids`, and add:

```rust
use std::cell::Cell;
use tauri::Manager;

fn tray_model(section: TrayDeviceSection) -> TrayMenuModel {
    TrayMenuModel {
        status_label: "status".into(),
        devices: section,
        open_label: "Open Kivo".into(),
        quit_label: "Quit Kivo".into(),
    }
}

#[test]
fn builds_flat_buttons_and_grouped_devices_as_native_submenus() {
    let app = tauri::test::mock_app();
    let configured = TrayButton {
        title: "A · ⌘A +1".into(),
        details: vec!["⌘A".into(), "↩".into()],
    };
    let unconfigured = TrayButton {
        title: "B · Not configured".into(),
        details: Vec::new(),
    };

    let flat = build_menu(
        app.handle(),
        &tray_model(TrayDeviceSection::Flat(vec![configured.clone(), unconfigured.clone()])),
    ).unwrap();
    let flat_items = flat.items().unwrap();
    let action_menu = flat_items
        .iter()
        .find_map(|item| item.as_submenu())
        .expect("button action submenu");
    assert_eq!(action_menu.text().unwrap(), "A · ⌘A +1");
    assert_eq!(
        action_menu
            .items()
            .unwrap()
            .iter()
            .filter_map(|item| item.as_menuitem())
            .map(|item| item.text().unwrap())
            .collect::<Vec<_>>(),
        vec!["1. ⌘A", "2. ↩"],
    );
    assert!(flat_items.iter().any(|item| {
        item.as_menuitem().is_some_and(|menu_item| {
            menu_item.text().unwrap() == "B · Not configured" && !menu_item.is_enabled().unwrap()
        })
    }));

    let empty = build_menu(
        app.handle(),
        &tray_model(TrayDeviceSection::Empty("No available device".into())),
    ).unwrap();
    assert!(empty.items().unwrap().iter().any(|item| {
        item.as_menuitem().is_some_and(|menu_item| {
            menu_item.text().unwrap() == "No available device" && !menu_item.is_enabled().unwrap()
        })
    }));

    let grouped = build_menu(
        app.handle(),
        &tray_model(TrayDeviceSection::Grouped(vec![TrayDevice {
            id: DeviceId::new("luatos-esp32s3-aio", "A").unwrap(),
            name: "Desk".into(),
            buttons: vec![configured],
        }])),
    ).unwrap();
    let device_menu = grouped
        .items()
        .unwrap()
        .into_iter()
        .find_map(|item| item.as_submenu().cloned())
        .expect("device submenu");
    assert_eq!(device_menu.text().unwrap(), "Desk");
    assert!(device_menu.items().unwrap().iter().any(|item| item.as_submenu().is_some()));
}

#[test]
fn installs_only_changed_models_and_keeps_current_after_failure() {
    let mut current = tray_model(TrayDeviceSection::Empty("old".into()));
    let calls = Cell::new(0);
    let same = current.clone();
    assert!(!install_if_changed(&mut current, same, |_| {
        calls.set(calls.get() + 1);
        Ok::<_, &'static str>(())
    }).unwrap());
    assert_eq!(calls.get(), 0);

    let next = tray_model(TrayDeviceSection::Empty("new".into()));
    assert!(install_if_changed(&mut current, next.clone(), |_| {
        calls.set(calls.get() + 1);
        Ok::<_, &'static str>(())
    }).unwrap());
    assert_eq!(current, next);
    assert_eq!(calls.get(), 1);

    let failed = tray_model(TrayDeviceSection::Empty("failed".into()));
    assert_eq!(
        install_if_changed(&mut current, failed, |_| Err::<(), _>("failed")),
        Err("failed"),
    );
    assert_eq!(current, next);
}
```

- [ ] **Step 2: Run the focused native-menu test and verify RED**

Run:

```bash
rtk cargo test --manifest-path src-tauri/Cargo.toml tray::tests --no-fail-fast
```

Expected: compilation fails because `build_menu` and `install_if_changed` do not exist. Tauri 2.11.5's `items`, `text`, and `is_enabled` getters return `tauri::Result`, so the test uses `.unwrap()` on each getter.

- [ ] **Step 3: Implement generic native menu construction**

Replace the static `status` handle in `TrayState` with the complete installed model and add generic builders in `src-tauri/src/tray.rs`:

```rust
mod model;

use model::{TrayButton, TrayDeviceSection, TrayMenuModel};
use std::sync::Mutex;
use tauri::{
    App, AppHandle, Manager, Runtime,
    image::Image,
    menu::{Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu},
    tray::{TrayIcon, TrayIconBuilder},
};

struct TrayState {
    model: Mutex<TrayMenuModel>,
    tray: TrayIcon<tauri::Wry>,
}

fn button_items<R: Runtime>(
    app: &AppHandle<R>,
    device_index: usize,
    buttons: &[TrayButton],
) -> tauri::Result<Vec<MenuItemKind<R>>> {
    buttons
        .iter()
        .enumerate()
        .map(|(button_index, button)| {
            if button.details.is_empty() {
                return MenuItem::with_id(
                    app,
                    format!("button-empty-{device_index}-{button_index}"),
                    &button.title,
                    false,
                    None::<&str>,
                ).map(MenuItemKind::MenuItem);
            }
            let submenu = Submenu::with_id(
                app,
                format!("button-summary-{device_index}-{button_index}"),
                &button.title,
                true,
            )?;
            for (action_index, summary) in button.details.iter().enumerate() {
                submenu.append(&MenuItem::with_id(
                    app,
                    format!("action-summary-{device_index}-{button_index}-{action_index}"),
                    format!("{}. {summary}", action_index + 1),
                    false,
                    None::<&str>,
                )?)?;
            }
            Ok(MenuItemKind::Submenu(submenu))
        })
        .collect()
}

fn build_menu<R: Runtime>(
    app: &AppHandle<R>,
    model: &TrayMenuModel,
) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(app, "status", &model.status_label, false, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    match &model.devices {
        TrayDeviceSection::Empty(label) => {
            menu.append(&MenuItem::with_id(app, "no-device", label, false, None::<&str>)?)?;
        }
        TrayDeviceSection::Flat(buttons) => {
            for item in button_items(app, 0, buttons)? {
                menu.append(&item)?;
            }
        }
        TrayDeviceSection::Grouped(devices) => {
            for (device_index, device) in devices.iter().enumerate() {
                let submenu = Submenu::with_id(
                    app,
                    format!("device-summary-{device_index}-{}", device.id.as_str()),
                    &device.name,
                    true,
                )?;
                for item in button_items(app, device_index, &device.buttons)? {
                    submenu.append(&item)?;
                }
                menu.append(&submenu)?;
            }
        }
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "open-main", &model.open_label, true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "quit-app", &model.quit_label, true, None::<&str>)?)?;
    Ok(menu)
}

fn install_if_changed<E>(
    current: &mut TrayMenuModel,
    next: TrayMenuModel,
    install: impl FnOnce(&TrayMenuModel) -> Result<(), E>,
) -> Result<bool, E> {
    if *current == next {
        return Ok(false);
    }
    install(&next)?;
    *current = next;
    Ok(true)
}
```

Keep `action_for` and `handle_action` restricted to `open-main` and `quit-app`.

- [ ] **Step 4: Update setup and refresh through the change gate**

Change `setup` and replace `update_registry` in `src-tauri/src/tray.rs`:

```rust
pub fn setup(
    app: &mut App,
    initial: &[DeviceStatus],
    workspace: &crate::workspace::Workspace,
) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);

    let model = TrayMenuModel::from_workspace(initial, workspace);
    let menu = build_menu(app.handle(), &model)?;
    let icon = Image::from_bytes(include_bytes!("../icons/tray-icon.png"))?;
    let tray = TrayIconBuilder::with_id("menu-bar")
        .icon(icon)
        .icon_as_template(true)
        .tooltip(format!("Kivo - {}", model.status_label))
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            if let Some(action) = action_for(event.id().as_ref()) {
                handle_action(app, action);
            }
        })
        .build(app)?;
    app.manage(TrayState { model: Mutex::new(model), tray });
    Ok(())
}

pub fn update(
    app: &AppHandle,
    devices: &[DeviceStatus],
    workspace: &crate::workspace::Workspace,
) {
    let next = TrayMenuModel::from_workspace(devices, workspace);
    let state_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(state) = state_app.try_state::<TrayState>() else { return };
        let Ok(mut current) = state.model.lock() else { return };
        let _ = install_if_changed(&mut current, next, |model| {
            let menu = build_menu(&state_app, model)?;
            state.tray.set_menu(Some(menu))?;
            let _ = state.tray.set_tooltip(Some(format!("Kivo - {}", model.status_label)));
            Ok::<_, tauri::Error>(())
        });
    });
}
```

The closure records the model only after `set_menu` succeeds. Tooltip failure is non-fatal because the installed menu is already current and the tooltip contains only duplicate status text.

- [ ] **Step 5: Wire the current Workspace into setup and each scan**

In `src-tauri/src/lib.rs`, replace the initial empty setup call with:

```rust
#[cfg(target_os = "macos")]
{
    let workspace_guard = workspace
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    tray::setup(app, &[], &workspace_guard)?;
}
```

Replace the background `tray::update_registry` call with:

```rust
#[cfg(target_os = "macos")]
if let Some(devices) = devices.as_deref()
    && let Ok(workspace) = workspace.read()
{
    tray::update(&app_handle, devices, &workspace);
}
```

Keep the existing `#[cfg(not(target_os = "macos"))] let _ = devices;` branch so non-macOS compilation remains warning-free.

- [ ] **Step 6: Run focused tests and verify GREEN**

Run:

```bash
rtk cargo test --manifest-path src-tauri/Cargo.toml tray --no-fail-fast
```

Expected: formatting, projection, native menu structure, change-gate, and fixed event-routing tests all pass without warnings.

- [ ] **Step 7: Run full automated verification**

Run each command separately:

```bash
rtk cargo test --manifest-path src-tauri/Cargo.toml
rtk cargo build --manifest-path src-tauri/Cargo.toml
rtk npm test
rtk npm run build
rtk git diff --check
```

Expected: all Rust and frontend tests pass, Rust and frontend production builds finish successfully, and `git diff --check` prints no errors. If a repository-wide failure predates this work, record the exact failing test and prove the focused Tray tests still pass; do not relabel the full suite as passing.

- [ ] **Step 8: Inspect the native macOS menu**

Run:

```bash
rtk npm run tauri dev
```

With one available physical Device, verify direct button rows, disabled unconfigured buttons, first-action `+N`, and action submenus on hover. With two available physical Devices, verify the additional Device submenu layer and that same-profile Devices remain separate. Leave the dev process running only during inspection, then stop it with `Ctrl-C`.

If only zero or one physical Device is available, record the untestable multi-Device cases as Not Run. Automated fixtures and mock menus are not physical macOS interaction evidence.

- [ ] **Step 9: Review and commit the native integration**

Inspect only the intended scope:

```bash
rtk git status --short
rtk git diff -- src-tauri/src/tray.rs src-tauri/src/tray/model.rs src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
rtk git diff --check
```

Then commit:

```bash
rtk git add src-tauri/src/tray.rs src-tauri/src/tray/model.rs src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
rtk git commit -m "feat: show device actions in macOS menu bar"
```

Stage `Cargo.lock` only if enabling the dev feature changed it. Do not stage unrelated user changes.

## Final Evidence

The implementation handoff must report:

- Focused Tray test result and test count.
- Full Rust test result.
- Rust build result.
- Frontend Vitest result.
- Frontend production build result.
- `git diff --check` result.
- Exact physical Device count used for native macOS inspection.
- Which of single-Device, multi-Device, hover, long-text width, and unchanged-scan stability were physically observed versus Not Run.
- Final commit IDs and confirmation that the worktree contains no unintended changes.
