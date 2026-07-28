use crate::{
    config::MappingConfig,
    protocol::{parse_press, reply},
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
const USB_PRODUCT_NAME: &str = "ESP Vibe Text Keyboard";

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
}

pub fn is_target_port(port: &SerialPortInfo) -> bool {
    matches!(
        &port.port_type,
        SerialPortType::UsbPort(info)
            if info.vid == USB_VENDOR_ID
                && info.product.as_deref() == Some(USB_PRODUCT_NAME)
    )
}

pub fn run_worker(
    app: AppHandle,
    mappings: Arc<RwLock<MappingConfig>>,
    connection: Arc<RwLock<ConnectionStatus>>,
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
                    let Some(press) = parse_press(text) else {
                        continue;
                    };
                    let action = mappings
                        .read()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .resolved_action(press.gpio);
                    let response = reply(press, action, copy_to_clipboard);
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
                        );
                        break;
                    }
                    let level = if response.message.contains("(clipboard:") {
                        EventLevel::Error
                    } else {
                        EventLevel::Info
                    };
                    emit(&app, &connection, level, response.message);
                }
                Err(error) if error.kind() == ErrorKind::TimedOut => continue,
                Err(error) => {
                    emit(
                        &app,
                        &connection,
                        EventLevel::Warning,
                        format!("Device disconnected: {error}"),
                    );
                    break;
                }
            }
        }
        set_connection(&app, &connection, ConnectionStatus::searching(), None);
    }
}

fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let mut child = Command::new("/usr/bin/pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| format!("start pbcopy: {error}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "open pbcopy stdin".to_owned())?
        .write_all(text.as_bytes())
        .map_err(|error| format!("write pbcopy: {error}"))?;
    let status = child
        .wait()
        .map_err(|error| format!("wait for pbcopy: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("pbcopy exited {status}"))
    }
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
            *current = next;
            true
        }
    };
    if changed {
        emit(
            app,
            connection,
            EventLevel::Info,
            message.unwrap_or_else(|| "Waiting for device".to_owned()),
        );
    }
}

fn emit(
    app: &AppHandle,
    connection: &RwLock<ConnectionStatus>,
    level: EventLevel,
    message: String,
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
    use serialport::{SerialPortInfo, SerialPortType, UsbPortInfo};

    fn usb_port(vid: u16, product: &str) -> SerialPortInfo {
        SerialPortInfo {
            port_name: "/dev/cu.test".to_owned(),
            port_type: SerialPortType::UsbPort(UsbPortInfo {
                vid,
                pid: 0x4002,
                serial_number: None,
                manufacturer: None,
                product: Some(product.to_owned()),
            }),
        }
    }

    #[test]
    fn identifies_only_the_expected_usb_device() {
        assert!(is_target_port(&usb_port(0x303a, "ESP Vibe Text Keyboard")));
        assert!(!is_target_port(&usb_port(0x303b, "ESP Vibe Text Keyboard")));
        assert!(!is_target_port(&usb_port(0x303a, "Other device")));
        assert!(!is_target_port(&SerialPortInfo {
            port_name: "/dev/cu.Bluetooth".to_owned(),
            port_type: SerialPortType::BluetoothPort,
        }));
    }
}
