#![allow(dead_code)]

#[cfg(test)]
use crate::profile::{TriggerActions, TriggerSettings};
use crate::{
    coordinator::{
        AssignmentDimension, ConnectionDimension, DeviceMode, DeviceStatus, IdentityDimension,
        RuntimeDimension,
    },
    hardware::DeviceId,
    model::ButtonDefinition,
    profile::{ButtonAction, MediaCommand},
    workspace::{AssignmentResolution, Language, Workspace},
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
                let AssignmentResolution::Valid { profile, .. } =
                    workspace.assignment_resolution(&device.device_id)
                else {
                    return None;
                };
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
                                    .map(|triggers| triggers.press.as_slice())
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
        ButtonAction::Delay { duration_ms } => format!("{duration_ms} ms"),
        ButtonAction::Media { command } => match command {
            MediaCommand::PlayPause => "Play/Pause",
            MediaCommand::PreviousTrack => "Previous",
            MediaCommand::NextTrack => "Next",
            MediaCommand::Stop => "Stop",
            MediaCommand::VolumeUp => "Volume +",
            MediaCommand::VolumeDown => "Volume -",
            MediaCommand::Mute => "Mute",
        }
        .into(),
        ButtonAction::Open { target } => {
            let limit = match kind {
                SummaryKind::Primary => PRIMARY_PASTE_LIMIT,
                SummaryKind::Detail => DETAIL_PASTE_LIMIT,
            };
            format!(
                "Open {}",
                truncate_chars(&collapse_whitespace(target), limit)
            )
        }
    }
}

fn key_abbreviation(key: &str) -> String {
    match key.to_ascii_lowercase().as_str() {
        "cmd" => "⌘".into(),
        "alt" | "option" => "⌥".into(),
        "ctrl" => "⌃".into(),
        "shift" => "⇧".into(),
        "primary" => "⌘/Ctrl".into(),
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
            trigger_settings: TriggerSettings::default(),
            hardware_profiles: vec![HardwareProfile {
                id: "hardware".into(),
                name: "Hardware".into(),
                board_profile_id: "yd-esp32-s3".into(),
                debounce_ms: 30,
                ssd1306: None,
                inputs: Vec::new(),
            }],
            actions: BTreeMap::from([(
                "B".into(),
                TriggerActions::press(vec![ButtonAction::Hotkey {
                    keys: vec!["cmd".into(), "b".into()],
                }]),
            )]),
        }
    }

    fn device(serial: &str, name: &str) -> DeviceStatus {
        DeviceStatus {
            device_id: DeviceId::new("yd-esp32-s3", serial).unwrap(),
            name: name.into(),
            connection: ConnectionDimension::Online,
            mode: Some(DeviceMode::Runtime),
            identity: IdentityDimension::Valid,
            assignment: AssignmentDimension::Valid,
            runtime: RuntimeDimension::Ready,
            raw_serial: serial.into(),
            port: Some(format!("/dev/{serial}")),
            controller_family_id: "esp32s3".into(),
            board_profile_id: "yd-esp32-s3".into(),
            firmware_build_id: Some("test".into()),
            product_version_id: None,
            product_definition: None,
            product_config: None,
            firmware_protocol: Some(6),
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

    fn default_assignment() -> RuntimeAssignment {
        RuntimeAssignment {
            device_profile_id: "desk-profile".into(),
            hardware_profile_id: "hardware".into(),
        }
    }

    fn enroll_and_assign(workspace: &mut Workspace, device: &DeviceStatus) {
        workspace.enroll_device(device.device_id.clone()).unwrap();
        workspace
            .set_assignment(&device.device_id, default_assignment())
            .unwrap();
    }

    #[test]
    fn selects_empty_flat_and_grouped_device_sections() {
        let directory = tempfile::tempdir().unwrap();
        let mut workspace = Workspace::create(directory.path(), vec![profile()]).unwrap();
        let empty = TrayMenuModel::from_workspace(&[], &workspace);
        assert!(
            matches!(empty.devices, TrayDeviceSection::Empty(ref label) if label == "暂无可用设备")
        );

        let front = device("FRONT", "前台 & 键盘");
        enroll_and_assign(&mut workspace, &front);
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
        enroll_and_assign(&mut workspace, &back);
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
        let directory = tempfile::tempdir().unwrap();
        let mut workspace = Workspace::create(directory.path(), vec![profile()]).unwrap();
        let valid = device("VALID", "Valid");
        enroll_and_assign(&mut workspace, &valid);
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
        let missing_profile = device("MISSING-PROFILE", "Missing Profile");
        enroll_and_assign(&mut workspace, &missing_profile);
        workspace
            .settings
            .devices
            .get_mut(&missing_profile.device_id)
            .unwrap()
            .runtime_assignment
            .as_mut()
            .unwrap()
            .device_profile_id = "missing".into();
        excluded.push(missing_profile);
        let missing_hardware = device("MISSING-HARDWARE", "Missing Hardware");
        enroll_and_assign(&mut workspace, &missing_hardware);
        workspace
            .settings
            .devices
            .get_mut(&missing_hardware.device_id)
            .unwrap()
            .runtime_assignment
            .as_mut()
            .unwrap()
            .hardware_profile_id = "missing".into();
        excluded.push(missing_hardware);

        let model = TrayMenuModel::from_workspace(&excluded, &workspace);
        assert!(matches!(model.devices, TrayDeviceSection::Empty(_)));
    }

    #[test]
    fn uses_current_workspace_assignment_instead_of_stale_status_assignment() {
        let directory = tempfile::tempdir().unwrap();
        let mut stale_profile = profile();
        stale_profile.profile.id = "stale-profile".into();
        stale_profile.profile.groups[0].buttons[0].label = "Stale".into();
        let mut workspace =
            Workspace::create(directory.path(), vec![profile(), stale_profile]).unwrap();
        let mut stale_status = device("STALE", "Stale");
        enroll_and_assign(&mut workspace, &stale_status);
        stale_status.runtime = RuntimeDimension::Configuring;
        stale_status.runtime_assignment = Some(RuntimeAssignment {
            device_profile_id: "stale-profile".into(),
            hardware_profile_id: "hardware".into(),
        });

        let model = TrayMenuModel::from_workspace(&[stale_status], &workspace);
        let TrayDeviceSection::Flat(buttons) = model.devices else {
            panic!("expected flat buttons")
        };
        assert_eq!(buttons[0].title, "B · ⌘B");
    }

    #[test]
    fn excludes_workspace_assignment_with_mismatched_hardware_board() {
        let directory = tempfile::tempdir().unwrap();
        let mut incompatible_profile = profile();
        incompatible_profile.hardware_profiles[0].board_profile_id = "yd-rp2040".into();
        let mut workspace =
            Workspace::create(directory.path(), vec![incompatible_profile]).unwrap();
        let status = device("MISMATCH", "Mismatch");
        workspace.enroll_device(status.device_id.clone()).unwrap();
        workspace
            .settings
            .devices
            .get_mut(&status.device_id)
            .unwrap()
            .runtime_assignment = Some(default_assignment());

        let model = TrayMenuModel::from_workspace(&[status], &workspace);
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
