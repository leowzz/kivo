use crate::{
    error::AppError,
    handshake::{HelloCapabilities, is_hello_line, parse_hello, validate_hello},
    hardware::{DeviceId, board_by_id, board_by_runtime_usb},
    serial::collapse_serial_port_aliases,
};
use serde::Serialize;
use serialport::{SerialPort, SerialPortType};
use std::{
    collections::BTreeSet,
    io::{ErrorKind, Read, Write},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_LINE_BYTES: usize = 254;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GpioDevice {
    id: DeviceId,
    port: String,
    board_profile_id: String,
    name: String,
    serial: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct GpioPin {
    gpio: u8,
    high: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GpioSnapshot {
    session_id: u64,
    device_id: DeviceId,
    firmware_build_id: String,
    pins: Vec<GpioPin>,
}

struct Session {
    id: u64,
    device_id: DeviceId,
    port: Box<dyn SerialPort>,
    hello: HelloCapabilities,
}

#[derive(Default)]
struct Monitor {
    next_id: u64,
    session: Option<Session>,
    closed: bool,
}

#[derive(Default)]
pub(super) struct GpioState(Arc<Mutex<Monitor>>);

impl GpioState {
    pub(super) fn close(&self) {
        let monitor = Arc::clone(&self.0);
        // Window teardown may skip React cleanup. Release even an in-flight connect.
        tauri::async_runtime::spawn_blocking(move || {
            let mut monitor = monitor.lock().unwrap_or_else(|error| error.into_inner());
            monitor.closed = true;
            monitor.session = None;
        });
    }
}

fn list_devices() -> Result<Vec<GpioDevice>, AppError> {
    let ports = serialport::available_ports()
        .map_err(|error| AppError::new("gpio_enumeration_failed").with_detail(error.to_string()))?;
    let mut devices = Vec::new();
    for port in collapse_serial_port_aliases(ports) {
        let SerialPortType::UsbPort(usb) = port.port_type else {
            continue;
        };
        let Some(board) = board_by_runtime_usb(usb.vid, usb.pid) else {
            continue;
        };
        let Some(serial) = usb.serial_number else {
            continue;
        };
        let Ok(id) = DeviceId::new(board.id, &serial) else {
            continue;
        };
        devices.push(GpioDevice {
            id,
            port: port.port_name,
            board_profile_id: board.id.into(),
            name: board.display_name.into(),
            serial,
        });
    }
    devices.sort_by(|left, right| left.id.cmp(&right.id).then(left.port.cmp(&right.port)));
    Ok(devices)
}

fn io_error(error: std::io::Error) -> AppError {
    AppError::new("gpio_disconnected").with_detail(error.to_string())
}

fn exchange(
    port: &mut (impl Read + Write + ?Sized),
    command: &[u8],
    accepts: impl Fn(&str) -> bool,
) -> Result<String, AppError> {
    port.write_all(command).map_err(io_error)?;
    port.flush().map_err(io_error)?;
    let deadline = Instant::now() + RESPONSE_TIMEOUT;
    let mut line = Vec::new();
    // Ignore unrelated key/display frames while waiting for this response.
    while Instant::now() < deadline {
        let mut byte = [0];
        match port.read(&mut byte) {
            Ok(0) => return Err(AppError::new("gpio_disconnected")),
            Ok(_) if byte[0] == b'\n' => {
                let value = std::str::from_utf8(&line)
                    .map_err(|_| AppError::new("gpio_invalid_response"))?;
                if accepts(value) {
                    return Ok(value.to_owned());
                }
                line.clear();
            }
            Ok(_) => {
                if line.len() >= MAX_LINE_BYTES {
                    return Err(AppError::new("gpio_invalid_response"));
                }
                line.push(byte[0]);
            }
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::TimedOut | ErrorKind::WouldBlock | ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(io_error(error)),
        }
    }
    Err(AppError::new("gpio_response_timeout"))
}

fn parse_pins(line: &str, expected: &[u8]) -> Result<Vec<GpioPin>, AppError> {
    let invalid = || AppError::new("gpio_invalid_response");
    let mut tokens = line.split_whitespace();
    if tokens.next() != Some("GPIO_STATE") {
        return Err(invalid());
    }
    let count = tokens
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or_else(invalid)?;
    let mut seen = BTreeSet::new();
    let mut pins = Vec::new();
    for token in tokens {
        let (gpio, high) = token.split_once(':').ok_or_else(invalid)?;
        let gpio = gpio.parse::<u8>().map_err(|_| invalid())?;
        let high = match high {
            "0" => false,
            "1" => true,
            _ => return Err(invalid()),
        };
        if !expected.contains(&gpio) || !seen.insert(gpio) {
            return Err(invalid());
        }
        pins.push(GpioPin { gpio, high });
    }
    if count != pins.len() || seen.len() != expected.len() {
        return Err(invalid());
    }
    pins.sort_by_key(|pin| pin.gpio);
    Ok(pins)
}

impl Session {
    fn sample(&mut self) -> Result<GpioSnapshot, AppError> {
        let line = exchange(self.port.as_mut(), b"GPIO_READ\n", |line| {
            line.starts_with("GPIO_STATE")
        })?;
        Ok(GpioSnapshot {
            session_id: self.id,
            device_id: self.device_id.clone(),
            firmware_build_id: self.hello.firmware_build_id.clone(),
            pins: parse_pins(&line, &self.hello.pins)?,
        })
    }
}

impl Monitor {
    fn connect(&mut self, device_id: &DeviceId) -> Result<GpioSnapshot, AppError> {
        if self.closed {
            return Err(AppError::new("gpio_session_closed"));
        }
        if self.session.is_some() {
            return Err(AppError::new("gpio_session_active"));
        }
        let matches = list_devices()?
            .into_iter()
            .filter(|device| &device.id == device_id)
            .collect::<Vec<_>>();
        let device = match matches.as_slice() {
            [] => return Err(AppError::new("gpio_device_missing")),
            [device] => device,
            _ => return Err(AppError::new("duplicate_identity")),
        };
        let mut port = serialport::new(&device.port, 115_200)
            .timeout(Duration::from_millis(50))
            .open()
            .map_err(|error| {
                AppError::new("gpio_port_unavailable").with_detail(error.to_string())
            })?;
        port.write_data_terminal_ready(true)
            .and_then(|()| port.write_request_to_send(true))
            .map_err(|error| {
                AppError::new("gpio_handshake_failed").with_detail(error.to_string())
            })?;
        let line = exchange(port.as_mut(), b"HELLO\n", is_hello_line)?;
        let hello =
            parse_hello(&line).ok_or_else(|| AppError::new("gpio_incompatible_firmware"))?;
        validate_hello(
            board_by_id(&device.board_profile_id).expect("enumerated board"),
            &hello,
        )?;
        self.next_id += 1;
        let mut session = Session {
            id: self.next_id,
            device_id: device_id.clone(),
            port,
            hello,
        };
        let snapshot = session.sample().map_err(|error| {
            if error.code == "gpio_response_timeout" {
                AppError::new("gpio_monitor_unsupported")
            } else {
                error
            }
        })?;
        self.session = Some(session);
        Ok(snapshot)
    }

    fn sample(&mut self, session_id: u64) -> Result<GpioSnapshot, AppError> {
        let session = self
            .session
            .as_mut()
            .filter(|session| session.id == session_id)
            .ok_or_else(|| AppError::new("gpio_session_closed"))?;
        let result = session.sample();
        if result.is_err() {
            self.session = None;
        }
        result
    }

    fn disconnect(&mut self, session_id: u64) {
        if self
            .session
            .as_ref()
            .is_some_and(|session| session.id == session_id)
        {
            self.session = None;
        }
    }
}

#[tauri::command]
pub(super) async fn studio_list_devices() -> Result<Vec<GpioDevice>, AppError> {
    tauri::async_runtime::spawn_blocking(list_devices)
        .await
        .map_err(|error| AppError::new("gpio_task_failed").with_detail(error.to_string()))?
}

#[tauri::command]
pub(super) async fn studio_connect_gpio(
    state: tauri::State<'_, GpioState>,
    device_id: DeviceId,
) -> Result<GpioSnapshot, AppError> {
    let monitor = Arc::clone(&state.0);
    tauri::async_runtime::spawn_blocking(move || {
        monitor
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .connect(&device_id)
    })
    .await
    .map_err(|error| AppError::new("gpio_task_failed").with_detail(error.to_string()))?
}

#[tauri::command]
pub(super) async fn studio_read_gpio(
    state: tauri::State<'_, GpioState>,
    session_id: u64,
) -> Result<GpioSnapshot, AppError> {
    let monitor = Arc::clone(&state.0);
    tauri::async_runtime::spawn_blocking(move || {
        monitor
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .sample(session_id)
    })
    .await
    .map_err(|error| AppError::new("gpio_task_failed").with_detail(error.to_string()))?
}

#[tauri::command]
pub(super) async fn studio_disconnect_gpio(
    state: tauri::State<'_, GpioState>,
    session_id: u64,
) -> Result<(), AppError> {
    let monitor = Arc::clone(&state.0);
    tauri::async_runtime::spawn_blocking(move || {
        monitor
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .disconnect(session_id)
    })
    .await
    .map_err(|error| AppError::new("gpio_task_failed").with_detail(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn validates_complete_pin_snapshots_and_orders_by_gpio() {
        assert_eq!(
            parse_pins("GPIO_STATE 2 7:0 1:1\r", &[1, 7]).unwrap(),
            vec![
                GpioPin {
                    gpio: 1,
                    high: true
                },
                GpioPin {
                    gpio: 7,
                    high: false
                }
            ]
        );
        for invalid in [
            "GPIO_STATE 2 1:1",
            "GPIO_STATE 2 1:0 1:1",
            "GPIO_STATE 2 1:1 8:0",
            "GPIO_STATE 2 1:2 7:0",
            "GPIO_STATE 1 1:1 7:0",
        ] {
            assert_eq!(
                parse_pins(invalid, &[1, 7]).unwrap_err().code,
                "gpio_invalid_response"
            );
        }
    }

    #[test]
    fn closed_window_rejects_a_queued_connection_before_enumeration() {
        let mut monitor = Monitor {
            closed: true,
            ..Monitor::default()
        };
        let device_id = DeviceId::new("yd-rp2040", "TEST").unwrap();
        assert_eq!(
            monitor.connect(&device_id).unwrap_err().code,
            "gpio_session_closed"
        );
    }

    #[cfg(unix)]
    #[test]
    fn session_tokens_isolate_stale_requests_and_io_failure_releases_the_port() {
        use std::io::{BufRead, BufReader};
        let (mut board, mut port) = serialport::TTYPort::pair().unwrap();
        port.set_timeout(Duration::from_millis(50)).unwrap();
        board.set_timeout(Duration::from_secs(2)).unwrap();
        let mut monitor = Monitor {
            session: Some(Session {
                id: 7,
                device_id: DeviceId::new("yd-rp2040", "TEST").unwrap(),
                port: Box::new(port),
                hello: parse_hello("HELLO 13 rp2040 yd-rp2040 test - 1 1").unwrap(),
            }),
            ..Monitor::default()
        };
        monitor.disconnect(6);
        assert_eq!(monitor.sample(6).unwrap_err().code, "gpio_session_closed");
        assert!(monitor.session.is_some());
        let responder = std::thread::spawn(move || {
            let mut command = String::new();
            BufReader::new(&mut board).read_line(&mut command).unwrap();
            assert_eq!(command, "GPIO_READ\n");
            board.write_all(b"GPIO_STATE 1 1:1\n").unwrap();
            board
        });
        assert!(monitor.sample(7).unwrap().pins[0].high);
        drop(responder.join().unwrap());
        assert!(monitor.sample(7).is_err());
        assert!(monitor.session.is_none());
    }

    struct Transport {
        input: Cursor<Vec<u8>>,
        output: Vec<u8>,
    }
    impl Read for Transport {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            self.input.read(bytes)
        }
    }
    impl Write for Transport {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.output.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn passive_request_ignores_key_events_and_bounds_device_input() {
        let mut port = Transport {
            input: Cursor::new(b"STATE 3 DIRECT 1 DOWN\nGPIO_STATE 1 1:1\n".to_vec()),
            output: vec![],
        };
        assert_eq!(
            exchange(&mut port, b"GPIO_READ\n", |line| line
                .starts_with("GPIO_STATE"))
            .unwrap(),
            "GPIO_STATE 1 1:1"
        );
        assert_eq!(port.output, b"GPIO_READ\n");
        assert_eq!(
            exchange(&mut port, b"GPIO_READ\n", |_| true)
                .unwrap_err()
                .code,
            "gpio_disconnected"
        );
        port.input = Cursor::new(vec![b'x'; 256]);
        assert_eq!(
            exchange(&mut port, b"GPIO_READ\n", |_| true)
                .unwrap_err()
                .code,
            "gpio_invalid_response"
        );
    }
}
