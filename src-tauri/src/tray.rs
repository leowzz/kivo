mod model;

use crate::coordinator::DeviceStatus;
use model::{TrayButton, TrayDeviceSection, TrayMenuModel};
use std::sync::Mutex;
use tauri::{
    App, AppHandle, Manager, Runtime,
    image::Image,
    menu::{Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu},
    tray::{TrayIcon, TrayIconBuilder},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrayAction {
    Open,
    Quit,
}

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
                )
                .map(MenuItemKind::MenuItem);
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

fn build_menu<R: Runtime>(app: &AppHandle<R>, model: &TrayMenuModel) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(
        app,
        "status",
        &model.status_label,
        false,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    match &model.devices {
        TrayDeviceSection::Empty(label) => {
            menu.append(&MenuItem::with_id(
                app,
                "no-device",
                label,
                false,
                None::<&str>,
            )?)?;
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
    menu.append(&MenuItem::with_id(
        app,
        "open-main",
        &model.open_label,
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "quit-app",
        &model.quit_label,
        true,
        None::<&str>,
    )?)?;
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

    app.manage(TrayState {
        model: Mutex::new(model),
        tray,
    });
    Ok(())
}

pub fn update(app: &AppHandle, devices: &[DeviceStatus], workspace: &crate::workspace::Workspace) {
    let next = TrayMenuModel::from_workspace(devices, workspace);
    let state_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(state) = state_app.try_state::<TrayState>() else {
            return;
        };
        let Ok(mut current) = state.model.lock() else {
            return;
        };
        let _ = install_if_changed(&mut current, next, |model| {
            let menu = build_menu(&state_app, model)?;
            state.tray.set_menu(Some(menu))?;
            let _ = state
                .tray
                .set_tooltip(Some(format!("Kivo - {}", model.status_label)));
            Ok::<_, tauri::Error>(())
        });
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        hardware::DeviceId,
        tray::model::{TrayButton, TrayDevice, TrayDeviceSection, TrayMenuModel},
    };
    use std::cell::Cell;

    fn tray_model(section: TrayDeviceSection) -> TrayMenuModel {
        TrayMenuModel {
            status_label: "status".into(),
            devices: section,
            open_label: "Open Kivo".into(),
            quit_label: "Quit Kivo".into(),
        }
    }

    #[test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "muda menus require the process main thread; Tauri mock tests run on a worker"
    )]
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
            &tray_model(TrayDeviceSection::Flat(vec![
                configured.clone(),
                unconfigured.clone(),
            ])),
        )
        .unwrap();
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
                menu_item.text().unwrap() == "B · Not configured"
                    && !menu_item.is_enabled().unwrap()
            })
        }));

        let empty = build_menu(
            app.handle(),
            &tray_model(TrayDeviceSection::Empty("No available device".into())),
        )
        .unwrap();
        assert!(empty.items().unwrap().iter().any(|item| {
            item.as_menuitem().is_some_and(|menu_item| {
                menu_item.text().unwrap() == "No available device"
                    && !menu_item.is_enabled().unwrap()
            })
        }));

        let grouped = build_menu(
            app.handle(),
            &tray_model(TrayDeviceSection::Grouped(vec![TrayDevice {
                id: DeviceId::new("luatos-esp32s3-aio", "A").unwrap(),
                name: "Desk".into(),
                buttons: vec![configured],
            }])),
        )
        .unwrap();
        let device_menu = grouped
            .items()
            .unwrap()
            .into_iter()
            .find_map(|item| item.as_submenu().cloned())
            .expect("device submenu");
        assert_eq!(device_menu.text().unwrap(), "Desk");
        assert!(
            device_menu
                .items()
                .unwrap()
                .iter()
                .any(|item| item.as_submenu().is_some())
        );
    }

    #[test]
    fn installs_only_changed_models_and_keeps_current_after_failure() {
        let mut current = tray_model(TrayDeviceSection::Empty("old".into()));
        let calls = Cell::new(0);
        let same = current.clone();
        assert!(
            !install_if_changed(&mut current, same, |_| {
                calls.set(calls.get() + 1);
                Ok::<_, &'static str>(())
            })
            .unwrap()
        );
        assert_eq!(calls.get(), 0);

        let next = tray_model(TrayDeviceSection::Empty("new".into()));
        assert!(
            install_if_changed(&mut current, next.clone(), |_| {
                calls.set(calls.get() + 1);
                Ok::<_, &'static str>(())
            })
            .unwrap()
        );
        assert_eq!(current, next);
        assert_eq!(calls.get(), 1);

        let failed = tray_model(TrayDeviceSection::Empty("failed".into()));
        assert_eq!(
            install_if_changed(&mut current, failed, |_| Err::<(), _>("failed")),
            Err("failed"),
        );
        assert_eq!(current, next);
    }

    #[test]
    fn routes_only_known_menu_ids() {
        assert_eq!(action_for("open-main"), Some(TrayAction::Open));
        assert_eq!(action_for("quit-app"), Some(TrayAction::Quit));
        assert_eq!(action_for("status"), None);
        assert_eq!(action_for("unknown"), None);
    }
}
