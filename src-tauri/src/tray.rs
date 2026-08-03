use crate::coordinator::{ConnectionDimension, DeviceMode, DeviceStatus, RuntimeDimension};
use tauri::{
    App, AppHandle, Manager,
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{TrayIcon, TrayIconBuilder},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrayAction {
    Open,
    Quit,
}

struct TrayState {
    status: MenuItem<tauri::Wry>,
    tray: TrayIcon<tauri::Wry>,
}

pub fn setup(app: &mut App, initial: &[DeviceStatus]) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);

    let label = status_label(initial);
    let status = MenuItem::with_id(app, "status", &label, false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open-main", "Open Kivo", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit-app", "Quit Kivo", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&status, &separator, &open, &quit])?;
    let icon = Image::from_bytes(include_bytes!("../icons/tray-icon.png"))?;
    let tooltip = format!("Kivo - {label}");
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

pub fn update_registry(app: &AppHandle, devices: &[DeviceStatus]) {
    let label = status_label(devices);
    let state_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(state) = state_app.try_state::<TrayState>() else {
            return;
        };
        let _ = state.status.set_text(&label);
        let _ = state.tray.set_tooltip(Some(format!("Kivo - {label}")));
    });
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

fn action_for(id: &str) -> Option<TrayAction> {
    match id {
        "open-main" => Some(TrayAction::Open),
        "quit-app" => Some(TrayAction::Quit),
        _ => None,
    }
}

fn status_label(devices: &[DeviceStatus]) -> String {
    let online = devices
        .iter()
        .filter(|device| device.connection == ConnectionDimension::Online)
        .collect::<Vec<_>>();
    if online.is_empty() {
        return "Waiting for device".to_owned();
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
    format!(
        "{} online · {ready} ready · {bootloader} bootloader · {errors} errors",
        online.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        coordinator::{AssignmentDimension, IdentityDimension},
        hardware::DeviceId,
    };

    fn status_fixture() -> DeviceStatus {
        DeviceStatus {
            device_id: DeviceId::new("luatos-esp32s3-aio", "TRAY").unwrap(),
            name: "Desk".into(),
            connection: ConnectionDimension::Online,
            mode: Some(DeviceMode::Runtime),
            identity: IdentityDimension::Valid,
            assignment: AssignmentDimension::Unassigned,
            runtime: RuntimeDimension::Inactive,
            raw_serial: "TRAY".into(),
            port: Some("/dev/test".into()),
            controller_family_id: "esp32s3".into(),
            board_profile_id: "luatos-esp32s3-aio".into(),
            firmware_build_id: Some("test".into()),
            pins: vec![1],
            runtime_assignment: None,
            latest_error: None,
            learning: None,
        }
    }

    #[test]
    fn formats_registry_summary() {
        assert_eq!(status_label(&[]), "Waiting for device");
        let mut ready = status_fixture();
        ready.runtime = RuntimeDimension::Ready;
        let mut bootloader = status_fixture();
        bootloader.mode = Some(DeviceMode::Bootloader);
        bootloader.runtime = RuntimeDimension::Inactive;
        assert_eq!(
            status_label(&[ready, bootloader]),
            "2 online · 1 ready · 1 bootloader · 0 errors"
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
