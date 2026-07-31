use kivo_lib::{
    BootloaderObservation, ButtonAction, ButtonDefinition, ButtonGroup, ClipboardWriter, Clock,
    ConnectionDimension, DeviceMode, DeviceProfile, HardwareProfile, InputSource, ModelLayout,
    PROFILE_SCHEMA_VERSION, PasteCoordinator, RuntimeAssignment, RuntimeCoordinator,
    RuntimeDimension, SerialObservation, SerialTransport, SerialTransportFactory,
    SystemWorkerLauncher, UsbEnumerator, Workspace, WorkspaceRevision,
    hardware::{BOARD_PROFILES, BoardProfile, DeviceId},
};
use std::{
    collections::{BTreeMap, VecDeque},
    io::{self, ErrorKind, Read, Write},
    path::PathBuf,
    sync::{Arc, Condvar, Mutex, RwLock},
    thread,
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(2);

#[derive(Default)]
struct FakeUsbEnumerator {
    observations: Mutex<(Vec<SerialObservation>, Vec<BootloaderObservation>)>,
}

impl FakeUsbEnumerator {
    fn set(&self, serial: Vec<SerialObservation>, bootloader: Vec<BootloaderObservation>) {
        *self.observations.lock().unwrap() = (serial, bootloader);
    }
}

impl UsbEnumerator for FakeUsbEnumerator {
    fn serial_ports(&self) -> Result<Vec<SerialObservation>, String> {
        Ok(self.observations.lock().unwrap().0.clone())
    }

    fn usb_devices(&self) -> Result<Vec<BootloaderObservation>, String> {
        Ok(self.observations.lock().unwrap().1.clone())
    }
}

struct FakeClock {
    base: Instant,
    elapsed_ms: Mutex<u64>,
}

impl FakeClock {
    fn new() -> Self {
        Self {
            base: Instant::now(),
            elapsed_ms: Mutex::new(0),
        }
    }

    fn advance(&self, duration: Duration) {
        *self.elapsed_ms.lock().unwrap() += duration.as_millis() as u64;
    }
}

impl Clock for FakeClock {
    fn monotonic_now(&self) -> Instant {
        self.base + Duration::from_millis(*self.elapsed_ms.lock().unwrap())
    }

    fn unix_time_ms(&self) -> u64 {
        1_720_000_000_000 + *self.elapsed_ms.lock().unwrap()
    }
}

#[derive(Clone, Default)]
struct RecordingClipboard(Arc<Mutex<Vec<String>>>);

impl RecordingClipboard {
    fn writes(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

impl ClipboardWriter for RecordingClipboard {
    fn write(&self, text: &str) -> Result<(), String> {
        self.0.lock().unwrap().push(text.to_owned());
        Ok(())
    }
}

#[derive(Default)]
struct EndpointState {
    inbound: VecDeque<u8>,
    outbound: Vec<String>,
    disconnected: bool,
}

#[derive(Clone)]
struct FakeEndpoint {
    port: String,
    state: Arc<(Mutex<EndpointState>, Condvar)>,
}

impl FakeEndpoint {
    fn new(port: &str, hello: String) -> Self {
        let endpoint = Self {
            port: port.into(),
            state: Arc::new((Mutex::new(EndpointState::default()), Condvar::new())),
        };
        endpoint.emit(&hello);
        endpoint
    }

    fn emit(&self, line: &str) {
        let (state, ready) = &*self.state;
        let mut state = state.lock().unwrap();
        state.inbound.extend(line.as_bytes());
        ready.notify_all();
    }

    fn disconnect(&self) {
        let (state, ready) = &*self.state;
        state.lock().unwrap().disconnected = true;
        ready.notify_all();
    }

    fn lines(&self) -> Vec<String> {
        self.state.0.lock().unwrap().outbound.clone()
    }

    fn wait_for_line(&self, predicate: impl Fn(&str) -> bool) -> String {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(line) = self.lines().into_iter().find(|line| predicate(line)) {
                return line;
            }
            assert!(
                Instant::now() < deadline,
                "{} did not emit expected line",
                self.port
            );
            thread::sleep(Duration::from_millis(1));
        }
    }

    fn config_revision(&self) -> u32 {
        self.wait_for_line(|line| line.starts_with("CONFIG_BEGIN "))
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap()
    }
}

struct FakeTransport {
    endpoint: FakeEndpoint,
    pending_write: Vec<u8>,
    global_writes: Arc<Mutex<Vec<(String, String)>>>,
}

impl Read for FakeTransport {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let (state, ready) = &*self.endpoint.state;
        let state = state.lock().unwrap();
        let (mut state, _) = ready
            .wait_timeout_while(state, Duration::from_millis(2), |state| {
                state.inbound.is_empty() && !state.disconnected
            })
            .unwrap();
        if state.inbound.is_empty() {
            return if state.disconnected {
                Ok(0)
            } else {
                Err(io::Error::new(ErrorKind::TimedOut, "no fake serial input"))
            };
        }
        let count = buffer.len().min(state.inbound.len());
        for byte in &mut buffer[..count] {
            *byte = state.inbound.pop_front().unwrap();
        }
        Ok(count)
    }
}

impl Write for FakeTransport {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.pending_write.extend_from_slice(buffer);
        while let Some(end) = self.pending_write.iter().position(|byte| *byte == b'\n') {
            let bytes = self.pending_write.drain(..=end).collect::<Vec<_>>();
            let line = String::from_utf8(bytes).unwrap();
            self.endpoint
                .state
                .0
                .lock()
                .unwrap()
                .outbound
                .push(line.clone());
            self.global_writes
                .lock()
                .unwrap()
                .push((self.endpoint.port.clone(), line));
        }
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl SerialTransport for FakeTransport {
    fn prepare(&mut self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Default)]
struct FakeTransportFactory {
    endpoints: Mutex<BTreeMap<String, VecDeque<FakeEndpoint>>>,
    global_writes: Arc<Mutex<Vec<(String, String)>>>,
}

impl FakeTransportFactory {
    fn add(&self, endpoint: FakeEndpoint) {
        self.endpoints
            .lock()
            .unwrap()
            .entry(endpoint.port.clone())
            .or_default()
            .push_back(endpoint);
    }

    fn action_lines(&self, prefix: &str) -> Vec<(String, String)> {
        self.global_writes
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, line)| line.starts_with(prefix))
            .cloned()
            .collect()
    }
}

impl SerialTransportFactory for FakeTransportFactory {
    fn open(&self, port: &str) -> Result<Box<dyn SerialTransport>, String> {
        let endpoint = self
            .endpoints
            .lock()
            .unwrap()
            .get_mut(port)
            .and_then(VecDeque::pop_front)
            .ok_or_else(|| format!("missing fake transport for {port}"))?;
        Ok(Box::new(FakeTransport {
            endpoint,
            pending_write: Vec::new(),
            global_writes: Arc::clone(&self.global_writes),
        }))
    }
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "kivo-parallel-devices-{}-{:?}",
            std::process::id(),
            Instant::now()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn board(id: &str) -> &'static BoardProfile {
    BOARD_PROFILES.iter().find(|board| board.id == id).unwrap()
}

fn hello(board: &BoardProfile, build: &str) -> String {
    format!(
        "HELLO 3 {} {} {} {} {}\n",
        board.family_id,
        board.id,
        build,
        board.safe_pins.len(),
        board
            .safe_pins
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn serial(port: &str, board: &BoardProfile, serial: &str) -> SerialObservation {
    SerialObservation {
        port: port.into(),
        vid: board.runtime_usb.vid,
        pid: board.runtime_usb.pid,
        serial_number: Some(serial.into()),
    }
}

fn bootloader(location: &str, board: &BoardProfile, serial: &str) -> BootloaderObservation {
    let usb = board.bootloader_usb.unwrap();
    BootloaderObservation {
        location: location.into(),
        vid: usb.vid,
        pid: usb.pid,
        serial_number: Some(serial.into()),
    }
}

fn profile(
    id: &str,
    hardware_id: &str,
    board_id: &str,
    hot_pin: u8,
    paste_pin: u8,
) -> DeviceProfile {
    DeviceProfile {
        schema_version: PROFILE_SCHEMA_VERSION,
        profile: ModelLayout {
            id: id.into(),
            name: id.into(),
            groups: vec![ButtonGroup {
                id: "keys".into(),
                columns: 2,
                buttons: vec![
                    ButtonDefinition {
                        id: "HOT".into(),
                        label: "Hotkey".into(),
                    },
                    ButtonDefinition {
                        id: "PASTE".into(),
                        label: "Paste".into(),
                    },
                ],
            }],
        },
        hardware_profiles: vec![HardwareProfile {
            id: hardware_id.into(),
            name: hardware_id.into(),
            board_profile_id: board_id.into(),
            debounce_ms: 30,
            inputs: vec![InputSource::Direct {
                id: "direct".into(),
                keys: BTreeMap::from([("HOT".into(), hot_pin), ("PASTE".into(), paste_pin)]),
            }],
        }],
        actions: BTreeMap::from([
            (
                "HOT".into(),
                vec![
                    ButtonAction::Hotkey {
                        keys: vec!["enter".into()],
                    },
                    ButtonAction::Hotkey {
                        keys: vec!["tab".into()],
                    },
                ],
            ),
            (
                "PASTE".into(),
                vec![ButtonAction::Paste {
                    text: format!("paste-{id}"),
                }],
            ),
        ]),
    }
}

fn wait_until(
    coordinator: &mut RuntimeCoordinator,
    predicate: impl Fn(&RuntimeCoordinator) -> bool,
) {
    let deadline = Instant::now() + WAIT;
    loop {
        coordinator.drain_worker_events();
        if predicate(coordinator) {
            return;
        }
        assert!(Instant::now() < deadline, "runtime state did not settle");
        thread::sleep(Duration::from_millis(1));
    }
}

fn ready_count(coordinator: &RuntimeCoordinator) -> usize {
    coordinator
        .devices()
        .iter()
        .filter(|device| device.runtime == RuntimeDimension::Ready)
        .count()
}

fn wait_for_action_count(
    coordinator: &mut RuntimeCoordinator,
    factory: &FakeTransportFactory,
    prefix: &str,
    count: usize,
) {
    let deadline = Instant::now() + WAIT;
    while factory.action_lines(prefix).len() < count {
        coordinator.drain_worker_events();
        assert!(Instant::now() < deadline, "expected {count} {prefix} lines");
        thread::sleep(Duration::from_millis(1));
    }
}

fn wait_for_input_event(coordinator: &mut RuntimeCoordinator, serial: &str) {
    let deadline = Instant::now() + WAIT;
    loop {
        if coordinator
            .drain_worker_events()
            .iter()
            .any(|event| event.raw_serial == serial && event.activity.code == "input_state")
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "missing input event for {serial}"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn four_concurrent_devices_keep_runtime_and_global_paste_isolated() {
    let esp = board("luatos-esp32s3-aio");
    let rp = board("vccgnd-yd-rp2040");
    let specs = [
        (
            "ESP-A",
            "/dev/fake-esp-a",
            esp,
            "profile-esp-a",
            "hardware-esp-a",
            6,
            7,
        ),
        (
            "ESP-B",
            "/dev/fake-esp-b",
            esp,
            "profile-esp-b",
            "hardware-esp-b",
            8,
            9,
        ),
        (
            "RP-A",
            "/dev/fake-rp-a",
            rp,
            "profile-rp-a",
            "hardware-rp-a",
            10,
            11,
        ),
        (
            "RP-B",
            "/dev/fake-rp-b",
            rp,
            "profile-rp-b",
            "hardware-rp-b",
            12,
            13,
        ),
    ];
    let directory = TestDirectory::new();
    let workspace = Arc::new(RwLock::new(
        Workspace::create(
            &directory.0,
            specs
                .iter()
                .map(|(_, _, board, profile_id, hardware_id, hot, paste)| {
                    profile(profile_id, hardware_id, board.id, *hot, *paste)
                })
                .collect(),
        )
        .unwrap(),
    ));
    let enumerator = Arc::new(FakeUsbEnumerator::default());
    let transports = Arc::new(FakeTransportFactory::default());
    let clock = Arc::new(FakeClock::new());
    let clipboard = RecordingClipboard::default();
    let paste =
        PasteCoordinator::with_clock(clipboard.clone(), Duration::from_secs(60), clock.clone());
    let endpoints = specs
        .iter()
        .map(|(serial, port, board, ..)| {
            let endpoint = FakeEndpoint::new(port, hello(board, &format!("build-{serial}")));
            transports.add(endpoint.clone());
            ((*serial).to_owned(), endpoint)
        })
        .collect::<BTreeMap<_, _>>();
    enumerator.set(
        specs
            .iter()
            .map(|(id, port, board, ..)| serial(port, board, id))
            .collect(),
        Vec::new(),
    );
    let launcher = Arc::new(SystemWorkerLauncher::with_runtime(
        paste.handle(),
        None,
        Arc::new(RwLock::new(())),
        transports.clone(),
        clock.clone(),
    ));
    let mut coordinator = RuntimeCoordinator::with_paste(
        enumerator.clone(),
        launcher,
        workspace.clone(),
        Some(paste.handle()),
    );

    coordinator.scan_once().unwrap();
    wait_until(&mut coordinator, |coordinator| {
        coordinator.devices().len() == 4
    });
    {
        let mut workspace = workspace.write().unwrap();
        for (serial, _, board, profile_id, hardware_id, ..) in specs {
            let id = DeviceId::new(board.id, serial).unwrap();
            workspace
                .set_assignment(
                    &id,
                    RuntimeAssignment {
                        device_profile_id: profile_id.into(),
                        hardware_profile_id: hardware_id.into(),
                    },
                )
                .unwrap();
        }
        coordinator.apply_workspace_revision(WorkspaceRevision::capture(&workspace));
    }
    for (serial, endpoint) in &endpoints {
        let revision = endpoint.config_revision();
        let (_, _, _, _, _, hot_pin, paste_pin) = specs
            .iter()
            .find(|(expected, ..)| expected == serial)
            .unwrap();
        let lines = endpoint.lines();
        assert!(lines.contains(&format!("CONFIG_BEGIN {revision} 30\n")));
        assert!(lines.contains(&format!(
            "CONFIG_DIRECT {revision} 0 2 {hot_pin} {paste_pin}\n"
        )));
        assert!(lines.contains(&format!("CONFIG_COMMIT {revision}\n")));
        endpoint.emit(&format!("CONFIG_OK {revision}\n"));
        assert!(
            endpoint.lines().iter().any(|line| line == "HELLO\n"),
            "{serial}"
        );
    }
    wait_until(&mut coordinator, |coordinator| {
        ready_count(coordinator) == 4
    });
    for (serial, _, board, profile_id, hardware_id, ..) in specs {
        let id = DeviceId::new(board.id, serial).unwrap();
        let status = coordinator
            .devices()
            .into_iter()
            .find(|status| status.device_id == id)
            .unwrap();
        assert_eq!(
            status.runtime_assignment.unwrap(),
            RuntimeAssignment {
                device_profile_id: profile_id.into(),
                hardware_profile_id: hardware_id.into(),
            }
        );
    }

    // All four sessions hold independent action queues while their DONE replies interleave.
    for (index, (serial, _, _, _, _, hot_pin, _)) in specs.iter().enumerate() {
        endpoints[*serial].emit(&format!("STATE {} DIRECT {hot_pin} DOWN\n", 100 + index));
    }
    wait_for_action_count(&mut coordinator, &transports, "HOTKEY ", 4);
    for index in [2usize, 0, 3, 1] {
        let (serial, ..) = specs[index];
        endpoints[serial].emit(&format!("DONE {} 1\n", 100 + index));
    }
    wait_for_action_count(&mut coordinator, &transports, "HOTKEY ", 8);
    for index in [1usize, 3, 0, 2] {
        let (serial, ..) = specs[index];
        endpoints[serial].emit(&format!("DONE {} 2\n", 100 + index));
    }
    let hotkeys = transports.action_lines("HOTKEY ");
    for (index, (_, port, ..)) in specs.iter().enumerate() {
        assert!(hotkeys.iter().any(|(actual_port, line)| {
            actual_port == port && line.starts_with(&format!("HOTKEY {} 1 2 ", 100 + index))
        }));
        assert!(hotkeys.iter().any(|(actual_port, line)| {
            actual_port == port && line.starts_with(&format!("HOTKEY {} 2 2 ", 100 + index))
        }));
    }

    // One disconnect is local, and the same stable identity resumes its assignment on reconnect.
    endpoints["ESP-A"].disconnect();
    wait_until(&mut coordinator, |coordinator| {
        ready_count(coordinator) == 3
    });
    assert_eq!(
        coordinator
            .devices()
            .into_iter()
            .find(|status| status.raw_serial == "ESP-A")
            .unwrap()
            .connection,
        ConnectionDimension::Offline
    );
    let esp_a_reconnected =
        FakeEndpoint::new("/dev/fake-esp-a", hello(esp, "build-ESP-A-reconnected"));
    transports.add(esp_a_reconnected.clone());
    coordinator.scan_once().unwrap();
    wait_until(&mut coordinator, |coordinator| {
        coordinator.devices().iter().any(|status| {
            status.raw_serial == "ESP-A" && status.runtime == RuntimeDimension::Configuring
        })
    });
    let revision = esp_a_reconnected.config_revision();
    esp_a_reconnected.emit(&format!("CONFIG_OK {revision}\n"));
    wait_until(&mut coordinator, |coordinator| {
        ready_count(coordinator) == 4
    });

    // A rejected topology changes only ESP-B, then a repeated HELLO/config handshake recovers it.
    let esp_b = &endpoints["ESP-B"];
    esp_b.emit(&hello(esp, "build-ESP-B"));
    let second_revision = esp_b
        .wait_for_line(|line| line.starts_with("CONFIG_BEGIN 2 "))
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse::<u32>()
        .unwrap();
    esp_b.emit(&format!("CONFIG_ERROR {second_revision} invalid_direct\n"));
    wait_until(&mut coordinator, |coordinator| {
        ready_count(coordinator) == 3
            && coordinator.devices().iter().any(|status| {
                status.raw_serial == "ESP-B" && status.runtime == RuntimeDimension::RuntimeError
            })
    });
    esp_b.emit(&hello(esp, "build-ESP-B"));
    esp_b.wait_for_line(|line| line.starts_with("CONFIG_BEGIN 3 "));
    esp_b.emit("CONFIG_OK 3\n");
    wait_until(&mut coordinator, |coordinator| {
        ready_count(coordinator) == 4
    });

    // Runtime port renumbering updates only the observation while retaining identity/assignment.
    enumerator.set(
        specs
            .iter()
            .map(|(id, port, board, ..)| {
                serial(
                    if *id == "RP-B" {
                        "/dev/fake-rp-b-renumbered"
                    } else {
                        port
                    },
                    board,
                    id,
                )
            })
            .collect(),
        Vec::new(),
    );
    coordinator.scan_once().unwrap();
    let rp_b_id = DeviceId::new(rp.id, "RP-B").unwrap();
    let rp_b_status = coordinator
        .devices()
        .into_iter()
        .find(|status| status.device_id == rp_b_id)
        .unwrap();
    assert_eq!(
        rp_b_status.port.as_deref(),
        Some("/dev/fake-rp-b-renumbered")
    );
    assert_eq!(
        rp_b_status.runtime_assignment.unwrap().hardware_profile_id,
        "hardware-rp-b"
    );
    assert_eq!(ready_count(&coordinator), 4);

    // Paste ownership follows central receive order, while unrelated hotkeys remain live.
    let paste_order = [
        ("ESP-B", 201u64, 9u8),
        ("RP-A", 202u64, 11u8),
        ("ESP-A", 203u64, 7u8),
    ];
    for (serial, event_id, pin) in paste_order {
        let endpoint = if serial == "ESP-A" {
            &esp_a_reconnected
        } else {
            &endpoints[serial]
        };
        endpoint.emit(&format!("STATE {event_id} DIRECT {pin} DOWN\n"));
        wait_for_input_event(&mut coordinator, serial);
    }
    wait_for_action_count(&mut coordinator, &transports, "PASTE ", 1);
    assert_eq!(clipboard.writes(), ["paste-profile-esp-b"]);
    endpoints["ESP-B"].emit("DONE 201 1\n");
    wait_for_action_count(&mut coordinator, &transports, "PASTE ", 2);
    assert_eq!(
        clipboard.writes(),
        ["paste-profile-esp-b", "paste-profile-rp-a"]
    );
    endpoints["RP-B"].emit("STATE 250 DIRECT 12 DOWN\n");
    wait_for_action_count(&mut coordinator, &transports, "HOTKEY ", 9);
    endpoints["RP-B"].emit("DONE 250 1\n");
    wait_for_action_count(&mut coordinator, &transports, "HOTKEY ", 10);
    endpoints["RP-B"].emit("DONE 250 2\n");
    assert_eq!(transports.action_lines("PASTE ").len(), 2);

    // The fourth paste request wakes the actor after the injected deadline advances.
    clock.advance(Duration::from_secs(61));
    endpoints["RP-B"].emit("STATE 204 DIRECT 13 DOWN\n");
    wait_for_input_event(&mut coordinator, "RP-B");
    wait_until(&mut coordinator, |_| {
        transports.action_lines("PASTE ").len() >= 3
    });
    esp_a_reconnected.emit("DONE 203 1\n");
    wait_for_action_count(&mut coordinator, &transports, "PASTE ", 4);
    endpoints["RP-B"].emit("DONE 204 1\n");
    wait_until(&mut coordinator, |coordinator| {
        coordinator.devices().iter().any(|status| {
            status.raw_serial == "RP-A" && status.runtime == RuntimeDimension::RuntimeError
        })
    });
    assert!(coordinator.devices().iter().all(|status| {
        status.raw_serial == "RP-A" || status.runtime == RuntimeDimension::Ready
    }));
    assert_eq!(
        clipboard.writes(),
        [
            "paste-profile-esp-b",
            "paste-profile-rp-a",
            "paste-profile-esp-a",
            "paste-profile-rp-b",
        ]
    );
    assert_eq!(
        transports
            .action_lines("PASTE ")
            .into_iter()
            .map(|(port, _)| port)
            .collect::<Vec<_>>(),
        [
            "/dev/fake-esp-b",
            "/dev/fake-rp-a",
            "/dev/fake-esp-a",
            "/dev/fake-rp-b",
        ]
    );

    // A known RP2040 bootloader observation changes only RP-A.
    enumerator.set(
        vec![
            serial("/dev/fake-esp-a", esp, "ESP-A"),
            serial("/dev/fake-esp-b", esp, "ESP-B"),
            serial("/dev/fake-rp-b-renumbered", rp, "RP-B"),
        ],
        vec![bootloader("1:9", rp, "RP-A")],
    );
    coordinator.scan_once().unwrap();
    let statuses = coordinator.devices();
    let rp_a = statuses
        .iter()
        .find(|status| status.raw_serial == "RP-A")
        .unwrap();
    assert_eq!(rp_a.mode, Some(DeviceMode::Bootloader));
    assert_eq!(rp_a.runtime, RuntimeDimension::Inactive);
    assert!(
        statuses
            .iter()
            .filter(|status| status.raw_serial != "RP-A")
            .all(|status| {
                status.mode == Some(DeviceMode::Runtime)
                    && status.runtime == RuntimeDimension::Ready
            })
    );

    coordinator.shutdown();
    paste.shutdown();
}
