#![allow(dead_code)]

use crate::{
    coordinator::{
        AssignmentDimension, ConnectionDimension, DeviceMode, DeviceStatus, IdentityDimension,
        RuntimeDimension,
    },
    hardware::DeviceId,
    model::ButtonDefinition,
    profile::ButtonAction,
    workspace::{Language, Workspace},
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
        title: format!(
            "{label} · {}{suffix}",
            format_action(first, SummaryKind::Primary)
        ),
        details: actions
            .iter()
            .map(|action| format_action(action, SummaryKind::Detail))
            .collect(),
    }
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
                                profile
                                    .actions
                                    .get(&button.id)
                                    .map(Vec::as_slice)
                                    .unwrap_or(&[]),
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
    without_controls
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        coordinator::{
            AssignmentDimension, ConnectionDimension, DeviceMode, DeviceStatus, IdentityDimension,
            RuntimeDimension,
        },
        hardware::DeviceId,
        model::{ButtonDefinition, ButtonGroup, ModelLayout},
        profile::{ButtonAction, DeviceProfile, HardwareProfile, PROFILE_SCHEMA_VERSION},
        workspace::{Language, RuntimeAssignment, Workspace},
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
                        buttons: vec![ButtonDefinition {
                            id: "B".into(),
                            label: "B".into(),
                        }],
                    },
                    ButtonGroup {
                        id: "bottom".into(),
                        columns: 1,
                        buttons: vec![ButtonDefinition {
                            id: "A".into(),
                            label: "A".into(),
                        }],
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
                vec![ButtonAction::Hotkey {
                    keys: vec!["cmd".into(), "b".into()],
                }],
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
        assert!(
            matches!(empty.devices, TrayDeviceSection::Empty(ref label) if label == "暂无可用设备")
        );

        let front = device("FRONT", "前台 & 键盘");
        let flat = TrayMenuModel::from_workspace(std::slice::from_ref(&front), &workspace);
        let TrayDeviceSection::Flat(buttons) = flat.devices else {
            panic!("expected flat buttons")
        };
        assert_eq!(
            buttons
                .iter()
                .map(|button| button.title.as_str())
                .collect::<Vec<_>>(),
            vec!["B · ⌘B", "A · 未配置"]
        );

        let back = device("BACK", "后台键盘");
        let grouped = TrayMenuModel::from_workspace(&[front, back], &workspace);
        let TrayDeviceSection::Grouped(devices) = grouped.devices else {
            panic!("expected grouped devices")
        };
        assert_eq!(
            devices
                .iter()
                .map(|device| device.name.as_str())
                .collect::<Vec<_>>(),
            vec!["前台 && 键盘", "后台键盘"]
        );
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
        missing_profile
            .runtime_assignment
            .as_mut()
            .unwrap()
            .device_profile_id = "missing".into();
        excluded.push(missing_profile);
        let mut missing_hardware = device("MISSING-HARDWARE", "Missing Hardware");
        missing_hardware
            .runtime_assignment
            .as_mut()
            .unwrap()
            .hardware_profile_id = "missing".into();
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
        assert_eq!(
            model.status_label,
            "3 台在线 · 1 台就绪 · 1 台引导模式 · 1 个错误"
        );
    }

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
            (
                vec!["home", "end", "page_up", "page_down"],
                "HomeEndPgUpPgDn",
            ),
        ];
        for (configured, expected) in keys {
            let action = ButtonAction::Hotkey {
                keys: configured.into_iter().map(str::to_owned).collect(),
            };
            assert_eq!(format_action(&action, SummaryKind::Primary), expected);
        }

        let long_paste = ButtonAction::Paste {
            text: "界".repeat(81),
        };
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
