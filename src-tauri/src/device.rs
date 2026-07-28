use crate::{
    config::{ButtonAction, MappingConfig},
    protocol::{InputState, Press, parse_input, reply},
};
use serde::Serialize;
use serialport::{SerialPortInfo, SerialPortType};
use std::{
    io::{BufRead, BufReader, ErrorKind, Write},
    process::{Command, Stdio},
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter};

const USB_VENDOR_ID: u16 = 0x303a;
const USB_PRODUCT_ID: u16 = 0x4002;
const CLIPBOARD_COMMAND: &str = if cfg!(target_os = "windows") {
    "clip.exe"
} else {
    "/usr/bin/pbcopy"
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionState {
    Searching,
    Connected,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    pub state: ConnectionState,
    pub port: Option<String>,
}

impl ConnectionStatus {
    pub fn searching() -> Self {
        Self {
            state: ConnectionState::Searching,
            port: None,
        }
    }

    fn connected(port: String) -> Self {
        Self {
            state: ConnectionState::Connected,
            port: Some(port),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EventLevel {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEvent {
    pub timestamp_ms: u64,
    pub level: EventLevel,
    pub message: String,
    pub connection: ConnectionStatus,
    pub gpio: Option<u8>,
    pub pressed: Option<bool>,
}

pub fn is_target_port(port: &SerialPortInfo) -> bool {
    matches!(
        &port.port_type,
        SerialPortType::UsbPort(info)
            if info.vid == USB_VENDOR_ID
                && info.pid == USB_PRODUCT_ID
    )
}

pub fn run_worker(
    app: AppHandle,
    mappings: Arc<RwLock<MappingConfig>>,
    connection: Arc<RwLock<ConnectionStatus>>,
    capture_next_gpio: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
) {
    while !stop.load(Ordering::Relaxed) {
        let port = match serialport::available_ports() {
            Ok(ports) => ports.into_iter().find(is_target_port),
            Err(error) => {
                emit(
                    &app,
                    &connection,
                    EventLevel::Warning,
                    format!("Serial scan failed: {error}"),
                    None,
                    None,
                );
                wait(&stop);
                continue;
            }
        };
        let Some(port) = port else {
            set_connection(&app, &connection, ConnectionStatus::searching(), None);
            wait(&stop);
            continue;
        };

        let device = match serialport::new(&port.port_name, 115_200)
            .timeout(Duration::from_millis(500))
            .open()
        {
            Ok(device) => device,
            Err(error) => {
                emit(
                    &app,
                    &connection,
                    EventLevel::Warning,
                    format!("Open {} failed: {error}", port.port_name),
                    None,
                    None,
                );
                wait(&stop);
                continue;
            }
        };

        set_connection(
            &app,
            &connection,
            ConnectionStatus::connected(port.port_name.clone()),
            Some(format!("Connected to {}", port.port_name)),
        );
        let mut device = BufReader::new(device);
        let mut line = Vec::new();
        while !stop.load(Ordering::Relaxed) {
            line.clear();
            match device.read_until(b'\n', &mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let Ok(text) = std::str::from_utf8(&line) else {
                        continue;
                    };
                    let Some(input) = parse_input(text) else {
                        continue;
                    };
                    if input.state == InputState::Down {
                        let action = mappings
                            .read()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .resolved_action(input.gpio);
                        let response = reply(
                            Press {
                                event_id: input.event_id,
                                gpio: input.gpio,
                            },
                            action_for_press(&capture_next_gpio, action),
                            copy_to_clipboard,
                        );
                        let level = if response.message.contains("(clipboard:") {
                            EventLevel::Error
                        } else {
                            EventLevel::Info
                        };
                        emit(
                            &app,
                            &connection,
                            level,
                            response.message,
                            Some(input.gpio),
                            Some(true),
                        );
                        if let Err(error) = device
                            .get_mut()
                            .write_all(response.line.as_bytes())
                            .and_then(|()| device.get_mut().flush())
                        {
                            emit(
                                &app,
                                &connection,
                                EventLevel::Error,
                                format!("Serial write failed: {error}"),
                                None,
                                None,
                            );
                            break;
                        }
                    } else {
                        emit(
                            &app,
                            &connection,
                            EventLevel::Info,
                            format!("GPIO{}: UP {}", input.gpio, input.event_id),
                            Some(input.gpio),
                            Some(false),
                        );
                    }
                }
                Err(error) if error.kind() == ErrorKind::TimedOut => continue,
                Err(error) => {
                    emit(
                        &app,
                        &connection,
                        EventLevel::Warning,
                        format!("Device disconnected: {error}"),
                        None,
                        None,
                    );
                    break;
                }
            }
        }
        set_connection(&app, &connection, ConnectionStatus::searching(), None);
    }
}

fn action_for_press(
    capture_next_gpio: &AtomicBool,
    configured: Option<ButtonAction>,
) -> Option<ButtonAction> {
    if capture_next_gpio.swap(false, Ordering::Relaxed) {
        None
    } else {
        configured
    }
}

fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let mut child = clipboard_command()
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| format!("start clipboard command: {error}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "open clipboard command stdin".to_owned())?
        .write_all(text.as_bytes())
        .map_err(|error| format!("write clipboard command: {error}"))?;
    let status = child
        .wait()
        .map_err(|error| format!("wait for clipboard command: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("clipboard command exited {status}"))
    }
}

fn clipboard_command() -> Command {
    let command = Command::new(CLIPBOARD_COMMAND);
    #[cfg(target_os = "macos")]
    let command = {
        let mut command = command;
        command.env("LC_CTYPE", "UTF-8");
        command
    };
    command
}

fn set_connection(
    app: &AppHandle,
    connection: &RwLock<ConnectionStatus>,
    next: ConnectionStatus,
    message: Option<String>,
) {
    let changed = {
        let mut current = connection
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if *current == next {
            false
        } else {
            *current = next.clone();
            true
        }
    };
    if changed {
        #[cfg(target_os = "macos")]
        crate::tray::update_connection(app, &next);
        emit(
            app,
            connection,
            EventLevel::Info,
            message.unwrap_or_else(|| "Waiting for device".to_owned()),
            None,
            None,
        );
    }
}

fn emit(
    app: &AppHandle,
    connection: &RwLock<ConnectionStatus>,
    level: EventLevel,
    message: String,
    gpio: Option<u8>,
    pressed: Option<bool>,
) {
    let payload = RuntimeEvent {
        timestamp_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        level,
        message,
        connection: connection
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone(),
        gpio,
        pressed,
    };
    let _ = app.emit("runtime-event", payload);
}

fn wait(stop: &AtomicBool) {
    for _ in 0..10 {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ButtonAction;
    use serialport::{SerialPortInfo, SerialPortType, UsbPortInfo};

    fn usb_port(vid: u16, pid: u16, product: Option<&str>) -> SerialPortInfo {
        SerialPortInfo {
            port_name: "/dev/cu.test".to_owned(),
            port_type: SerialPortType::UsbPort(UsbPortInfo {
                vid,
                pid,
                serial_number: None,
                manufacturer: None,
                product: product.map(str::to_owned),
            }),
        }
    }

    #[test]
    fn identifies_only_the_expected_usb_device() {
        assert!(is_target_port(&usb_port(
            0x303a,
            0x4002,
            Some("USB Serial Device (COM3)")
        )));
        assert!(is_target_port(&usb_port(0x303a, 0x4002, None)));
        assert!(!is_target_port(&usb_port(
            0x303b,
            0x4002,
            Some("Kivo Keyboard")
        )));
        assert!(!is_target_port(&usb_port(
            0x303a,
            0x4003,
            Some("Kivo Keyboard")
        )));
        assert!(!is_target_port(&SerialPortInfo {
            port_name: "/dev/cu.Bluetooth".to_owned(),
            port_type: SerialPortType::BluetoothPort,
        }));
    }

    #[test]
    fn capture_skips_action_and_clears_itself() {
        let capture = AtomicBool::new(true);
        let configured = Some(ButtonAction::Paste { text: "x".into() });

        assert_eq!(action_for_press(&capture, configured.clone()), None);
        assert!(!capture.load(Ordering::Relaxed));
        assert_eq!(action_for_press(&capture, configured.clone()), configured);
    }

    #[test]
    fn runtime_events_serialize_input_state_or_null() {
        let event = RuntimeEvent {
            timestamp_ms: 1,
            level: EventLevel::Info,
            message: "Waiting for device".into(),
            connection: ConnectionStatus::searching(),
            gpio: None,
            pressed: None,
        };
        let value = serde_json::to_value(&event).unwrap();
        assert!(value["gpio"].is_null());
        assert!(value["pressed"].is_null());

        let down = RuntimeEvent {
            gpio: Some(6),
            pressed: Some(true),
            ..event.clone()
        };
        assert_eq!(serde_json::to_value(down).unwrap()["pressed"], true);
        let up = RuntimeEvent {
            gpio: Some(6),
            pressed: Some(false),
            ..event
        };
        assert_eq!(serde_json::to_value(up).unwrap()["pressed"], false);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn clipboard_command_uses_utf8_locale() {
        let command = clipboard_command();

        assert!(command.get_envs().any(|(key, value)| {
            key == "LC_CTYPE" && value == Some(std::ffi::OsStr::new("UTF-8"))
        }));
    }
}
