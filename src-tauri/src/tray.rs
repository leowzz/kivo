use crate::device::{ConnectionState, ConnectionStatus};
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
    let label = status_label(connection);
    let app = app.clone();
    let state_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(state) = state_app.try_state::<TrayState>() else {
            return;
        };
        let _ = state.status.set_text(&label);
        let _ = state.tray.set_tooltip(Some(format!("Vibe Tool - {label}")));
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

fn status_label(connection: &ConnectionStatus) -> String {
    match (&connection.state, &connection.port) {
        (ConnectionState::Connected, Some(port)) => format!("Connected - {port}"),
        _ => "Waiting for device".to_owned(),
    }
}

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
