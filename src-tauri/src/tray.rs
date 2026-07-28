use crate::device::{ConnectionState, ConnectionStatus};

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
