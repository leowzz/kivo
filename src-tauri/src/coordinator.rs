#[cfg(test)]
use crate::profile::{TriggerActions, TriggerSettings};
use crate::{
    device::{LearningTarget, RuntimeActivity, RuntimeProfileSnapshot},
    display::{DisplaySnapshot, RendererRegistry, built_in_renderer_registry},
    hardware::{BoardProfile, DeviceId, HardwareRegistry, compiled_registry},
    metrics::{HomeMetricsSnapshot, MetricAttribution},
    paste::PasteHandle,
    product::ProductDefinition,
    profile::{DeviceProfile, ProfileChange},
    protocol::{HelloCapabilities, InputState, PhysicalInput, validate_hello},
    usage::UsageSnapshot,
    workspace::{
        AppError, AssignmentResolution, ProductDeviceConfig, RuntimeAssignment, SettingsDocument,
        Workspace,
    },
};
use nusb::MaybeFuture;
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, RwLock, mpsc},
    time::{Duration, Instant},
};

const WORKER_RECONNECT_BACKOFF: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SerialObservation {
    pub port: String,
    pub vid: u16,
    pub pid: u16,
    pub serial_number: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BootloaderObservation {
    pub location: String,
    pub vid: u16,
    pub pid: u16,
    pub serial_number: Option<String>,
}

pub trait UsbEnumerator: Send + Sync {
    fn serial_ports(&self) -> Result<Vec<SerialObservation>, String>;
    fn usb_devices(&self) -> Result<Vec<BootloaderObservation>, String>;
}

pub(crate) struct DeviceScan {
    serial: Vec<SerialObservation>,
    bootloader: Vec<BootloaderObservation>,
}

pub(crate) fn enumerate_devices(enumerator: &dyn UsbEnumerator) -> Result<DeviceScan, String> {
    Ok(DeviceScan {
        serial: enumerator
            .serial_ports()
            .map_err(|_| "serial_enumeration_failed".to_owned())?,
        bootloader: enumerator
            .usb_devices()
            .map_err(|_| "usb_enumeration_failed".to_owned())?,
    })
}

pub struct SystemUsbEnumerator;

fn collapse_serial_port_aliases(
    ports: Vec<serialport::SerialPortInfo>,
) -> Vec<serialport::SerialPortInfo> {
    let callout_suffixes = ports
        .iter()
        .filter_map(|port| port.port_name.strip_prefix("/dev/cu."))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();

    ports
        .into_iter()
        .filter(|port| match port.port_name.strip_prefix("/dev/tty.") {
            Some(suffix) => !callout_suffixes.contains(suffix),
            None => true,
        })
        .collect()
}

impl UsbEnumerator for SystemUsbEnumerator {
    fn serial_ports(&self) -> Result<Vec<SerialObservation>, String> {
        serialport::available_ports()
            .map_err(|error| error.to_string())
            .map(|ports| {
                collapse_serial_port_aliases(ports)
                    .into_iter()
                    .filter_map(|port| match port.port_type {
                        serialport::SerialPortType::UsbPort(info) => Some(SerialObservation {
                            port: port.port_name,
                            vid: info.vid,
                            pid: info.pid,
                            serial_number: info.serial_number,
                        }),
                        _ => None,
                    })
                    .collect()
            })
    }

    fn usb_devices(&self) -> Result<Vec<BootloaderObservation>, String> {
        nusb::list_devices()
            .wait()
            .map_err(|error| error.to_string())
            .map(|devices| {
                devices
                    .map(|device| BootloaderObservation {
                        location: format!("{}:{}", device.bus_id(), device.device_address()),
                        vid: device.vendor_id(),
                        pid: device.product_id(),
                        serial_number: device.serial_number().map(str::to_owned),
                    })
                    .collect()
            })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionDimension {
    Online,
    Offline,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceMode {
    Runtime,
    Bootloader,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityDimension {
    Validating,
    Valid,
    InvalidIdentity,
    DuplicateIdentity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentDimension {
    Unassigned,
    Valid,
    InvalidAssignment,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeDimension {
    Inactive,
    Configuring,
    Learning,
    Ready,
    RuntimeError,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStatus {
    pub device_id: DeviceId,
    pub name: String,
    pub connection: ConnectionDimension,
    pub mode: Option<DeviceMode>,
    pub identity: IdentityDimension,
    pub assignment: AssignmentDimension,
    pub runtime: RuntimeDimension,
    #[serde(rename = "hardwareSerial")]
    pub raw_serial: String,
    pub port: Option<String>,
    pub controller_family_id: String,
    pub board_profile_id: String,
    pub firmware_build_id: Option<String>,
    pub product_version_id: Option<String>,
    pub product_definition: Option<ProductDefinition>,
    pub product_config: Option<ProductDeviceConfig>,
    #[serde(rename = "firmwareProtocol")]
    pub firmware_protocol: Option<u16>,
    #[serde(rename = "capabilities")]
    pub pins: Vec<u8>,
    pub runtime_assignment: Option<RuntimeAssignment>,
    pub latest_error: Option<RuntimeActivity>,
    pub learning: Option<LearningTarget>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateStatus {
    pub key: String,
    pub device_id: Option<DeviceId>,
    pub mode: DeviceMode,
    pub identity: IdentityDimension,
    pub issue: CandidateIssue,
    pub raw_serial: Option<String>,
    pub port: Option<String>,
    pub controller_family_id: String,
    pub board_profile_id: String,
    pub latest_error: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateIssue {
    Validating,
    FirmwareNotResponding,
    FirmwareIncompatible,
    Bootloader,
    PortUnavailable,
    InvalidIdentity,
    DuplicateIdentity,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
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
    pub device_id: DeviceId,
    pub raw_serial: String,
    pub controller_family_id: String,
    pub board_profile_id: String,
    pub port: Option<String>,
    pub device_profile_id: Option<String>,
    pub hardware_profile_id: Option<String>,
    pub home_update: Option<HomeMetricsSnapshot>,
    #[serde(flatten)]
    pub activity: RuntimeActivity,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeEventContext {
    pub(crate) timestamp_ms: u64,
    pub(crate) port: Option<String>,
    pub(crate) device_profile_id: Option<String>,
    pub(crate) hardware_profile_id: Option<String>,
}

impl RuntimeEventContext {
    pub(crate) fn unassigned(timestamp_ms: u64) -> Self {
        Self {
            timestamp_ms,
            port: None,
            device_profile_id: None,
            hardware_profile_id: None,
        }
    }

    pub(crate) fn with_timestamp(&self, timestamp_ms: u64) -> Self {
        Self {
            timestamp_ms,
            port: self.port.clone(),
            device_profile_id: self.device_profile_id.clone(),
            hardware_profile_id: self.hardware_profile_id.clone(),
        }
    }

    pub(crate) fn from_snapshot(
        timestamp_ms: u64,
        snapshot: Option<&RuntimeProfileSnapshot>,
    ) -> Self {
        Self {
            timestamp_ms,
            port: None,
            device_profile_id: snapshot
                .map(|snapshot| snapshot.metric_attribution.device_profile_id.clone()),
            hardware_profile_id: snapshot.map(|snapshot| snapshot.hardware_profile_id.clone()),
        }
    }

    pub(crate) fn from_learning(timestamp_ms: u64, target: &LearningTarget) -> Self {
        Self {
            timestamp_ms,
            port: None,
            device_profile_id: Some(target.device_profile_id.clone()),
            hardware_profile_id: Some(target.hardware_profile_id.clone()),
        }
    }

    pub(crate) fn with_port(mut self, port: impl Into<String>) -> Self {
        self.port = Some(port.into());
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapturedInput {
    pub(crate) context: RuntimeEventContext,
    pub(crate) runtime_profile: Option<Arc<RuntimeProfileSnapshot>>,
    pub(crate) monotonic_ms: u64,
    pub(crate) event_id: u64,
    pub(crate) input: PhysicalInput,
    pub(crate) state: InputState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerStart {
    pub generation: u64,
    pub device_id: DeviceId,
    pub port: String,
    pub board_profile_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkerCommand {
    UpdatePort(String),
    UpdateSnapshot(Option<Arc<RuntimeProfileSnapshot>>),
    Reconfigure {
        snapshot: Option<Arc<RuntimeProfileSnapshot>>,
        revision: u32,
    },
    BeginLearning(LearningTarget),
    EndLearning {
        snapshot: Option<Arc<RuntimeProfileSnapshot>>,
        revision: u32,
    },
    Input {
        receive_sequence: u64,
        captured: CapturedInput,
    },
    UpdateDisplay(Arc<DisplaySnapshot>),
    UpdateUsage(Arc<UsageSnapshot>),
    Shutdown,
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkerEvent {
    HelloValidated {
        generation: u64,
        device_id: DeviceId,
        capabilities: HelloCapabilities,
        product_definition: Option<ProductDefinition>,
    },
    Input {
        generation: u64,
        device_id: DeviceId,
        captured: CapturedInput,
    },
    SequenceFinished {
        generation: u64,
        device_id: DeviceId,
        receive_sequence: u64,
    },
    Activity {
        generation: u64,
        device_id: DeviceId,
        context: RuntimeEventContext,
        activity: RuntimeActivity,
    },
    UsageView {
        generation: u64,
        device_id: DeviceId,
        active: bool,
    },
    Disconnected {
        generation: u64,
        device_id: DeviceId,
        error: Option<String>,
    },
}

pub trait DeviceWorker: Send {
    fn send(&self, command: WorkerCommand) -> Result<(), String>;
    fn stop(&mut self);
    fn join(&mut self);
}

pub trait WorkerLauncher: Send + Sync {
    fn start(
        &self,
        start: WorkerStart,
        events: mpsc::Sender<WorkerEvent>,
    ) -> Result<Box<dyn DeviceWorker>, String>;

    fn start_with_renderers(
        &self,
        start: WorkerStart,
        events: mpsc::Sender<WorkerEvent>,
        _renderers: WorkerRendererRegistry,
    ) -> Result<Box<dyn DeviceWorker>, String> {
        self.start(start, events)
    }
}

/// Opaque carrier that keeps the renderer registry internal across the public launcher boundary.
pub struct WorkerRendererRegistry(Arc<RendererRegistry>);

impl WorkerRendererRegistry {
    pub(crate) fn new(registry: Arc<RendererRegistry>) -> Self {
        Self(registry)
    }

    pub(crate) fn into_inner(self) -> Arc<RendererRegistry> {
        self.0
    }
}

struct WorkerSlot {
    worker: Box<dyn DeviceWorker>,
    port: String,
    firmware_revision: u32,
}

pub(crate) struct RetiredWorkers(Vec<Box<dyn DeviceWorker>>);

impl RetiredWorkers {
    pub(crate) fn join(mut self) {
        for worker in &mut self.0 {
            worker.join();
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkspaceWorkerUpdate {
    Snapshot,
    Reconfigure,
}

impl WorkerSlot {
    fn next_revision(&mut self) -> u32 {
        self.firmware_revision = self.firmware_revision.wrapping_add(1).max(1);
        self.firmware_revision
    }
}

enum ClassifiedObservation {
    Runtime {
        board: &'static BoardProfile,
        observation: SerialObservation,
    },
    Bootloader {
        board: &'static BoardProfile,
        observation: BootloaderObservation,
    },
}

impl ClassifiedObservation {
    fn board(&self) -> &'static BoardProfile {
        match self {
            Self::Runtime { board, .. } | Self::Bootloader { board, .. } => board,
        }
    }

    fn serial(&self) -> Option<&str> {
        match self {
            Self::Runtime { observation, .. } => observation.serial_number.as_deref(),
            Self::Bootloader { observation, .. } => observation.serial_number.as_deref(),
        }
    }

    fn key(&self) -> String {
        match self {
            Self::Runtime { observation, .. } => format!("runtime:{}", observation.port),
            Self::Bootloader { observation, .. } => {
                format!("bootloader:{}", observation.location)
            }
        }
    }

    fn mode(&self) -> DeviceMode {
        match self {
            Self::Runtime { .. } => DeviceMode::Runtime,
            Self::Bootloader { .. } => DeviceMode::Bootloader,
        }
    }

    fn port(&self) -> Option<String> {
        match self {
            Self::Runtime { observation, .. } => Some(observation.port.clone()),
            Self::Bootloader { .. } => None,
        }
    }
}

pub struct RuntimeCoordinator {
    registry: HardwareRegistry<'static>,
    enumerator: Arc<dyn UsbEnumerator>,
    launcher: Arc<dyn WorkerLauncher>,
    workspace: Arc<RwLock<Workspace>>,
    workspace_revision: WorkspaceRevision,
    paste: Option<PasteHandle>,
    renderers: Arc<RendererRegistry>,
    display_snapshot: Option<Arc<DisplaySnapshot>>,
    usage_snapshot: Option<Arc<UsageSnapshot>>,
    workers: BTreeMap<DeviceId, WorkerSlot>,
    usage_view_devices: BTreeSet<DeviceId>,
    recovering_devices: BTreeSet<DeviceId>,
    reconnect_not_before: BTreeMap<DeviceId, Instant>,
    devices: BTreeMap<DeviceId, DeviceStatus>,
    candidates: Vec<CandidateStatus>,
    event_sender: mpsc::Sender<WorkerEvent>,
    event_receiver: mpsc::Receiver<WorkerEvent>,
    generation: u64,
    receive_sequence: u64,
    sequence_owners: BTreeMap<u64, DeviceId>,
    product_definitions: BTreeMap<DeviceId, ProductDefinition>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceRevision {
    settings: SettingsDocument,
    profiles: BTreeMap<String, DeviceProfile>,
}

impl WorkspaceRevision {
    pub fn capture(workspace: &Workspace) -> Self {
        Self {
            settings: workspace.settings.clone(),
            profiles: workspace.profiles.clone(),
        }
    }

    fn assignment_resolution(&self, id: &DeviceId) -> AssignmentResolution<'_> {
        let Some(device) = self.settings.devices.get(id) else {
            return AssignmentResolution::UnknownDevice;
        };
        let Some(assignment) = device.runtime_assignment.as_ref() else {
            return AssignmentResolution::Unassigned { device };
        };
        let Some(profile) = self.profiles.get(&assignment.device_profile_id) else {
            return AssignmentResolution::InvalidAssignment { device, assignment };
        };
        let Some(hardware) = profile.hardware_profile(&assignment.hardware_profile_id) else {
            return AssignmentResolution::InvalidAssignment { device, assignment };
        };
        if hardware.board_profile_id != device.board_profile_id {
            return AssignmentResolution::InvalidAssignment { device, assignment };
        }
        AssignmentResolution::Valid {
            device,
            assignment,
            profile,
            hardware,
        }
    }
}

impl RuntimeCoordinator {
    #[cfg(test)]
    pub fn new(
        enumerator: Arc<dyn UsbEnumerator>,
        launcher: Arc<dyn WorkerLauncher>,
        workspace: Arc<RwLock<Workspace>>,
    ) -> Self {
        Self::with_paste(enumerator, launcher, workspace, None)
    }

    #[cfg(test)]
    fn new_with_registry(
        enumerator: Arc<dyn UsbEnumerator>,
        launcher: Arc<dyn WorkerLauncher>,
        workspace: Arc<RwLock<Workspace>>,
        registry: HardwareRegistry<'static>,
    ) -> Self {
        Self::with_registry_and_paste(enumerator, launcher, workspace, None, registry)
    }

    pub fn with_paste(
        enumerator: Arc<dyn UsbEnumerator>,
        launcher: Arc<dyn WorkerLauncher>,
        workspace: Arc<RwLock<Workspace>>,
        paste: Option<PasteHandle>,
    ) -> Self {
        Self::with_paste_and_renderers(
            enumerator,
            launcher,
            workspace,
            paste,
            Arc::new(built_in_renderer_registry()),
        )
    }

    pub(crate) fn with_paste_and_renderers(
        enumerator: Arc<dyn UsbEnumerator>,
        launcher: Arc<dyn WorkerLauncher>,
        workspace: Arc<RwLock<Workspace>>,
        paste: Option<PasteHandle>,
        renderers: Arc<RendererRegistry>,
    ) -> Self {
        Self::with_registry_paste_and_renderers(
            enumerator,
            launcher,
            workspace,
            paste,
            compiled_registry(),
            renderers,
        )
    }

    #[cfg(test)]
    fn with_registry_and_paste(
        enumerator: Arc<dyn UsbEnumerator>,
        launcher: Arc<dyn WorkerLauncher>,
        workspace: Arc<RwLock<Workspace>>,
        paste: Option<PasteHandle>,
        registry: HardwareRegistry<'static>,
    ) -> Self {
        Self::with_registry_paste_and_renderers(
            enumerator,
            launcher,
            workspace,
            paste,
            registry,
            Arc::new(built_in_renderer_registry()),
        )
    }

    fn with_registry_paste_and_renderers(
        enumerator: Arc<dyn UsbEnumerator>,
        launcher: Arc<dyn WorkerLauncher>,
        workspace: Arc<RwLock<Workspace>>,
        paste: Option<PasteHandle>,
        registry: HardwareRegistry<'static>,
        renderers: Arc<RendererRegistry>,
    ) -> Self {
        let (event_sender, event_receiver) = mpsc::channel();
        let workspace_revision = {
            let workspace = workspace
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            WorkspaceRevision::capture(&workspace)
        };
        Self {
            registry,
            enumerator,
            launcher,
            workspace,
            workspace_revision,
            paste,
            renderers,
            display_snapshot: None,
            usage_snapshot: None,
            workers: BTreeMap::new(),
            usage_view_devices: BTreeSet::new(),
            recovering_devices: BTreeSet::new(),
            reconnect_not_before: BTreeMap::new(),
            devices: BTreeMap::new(),
            candidates: Vec::new(),
            event_sender,
            event_receiver,
            generation: 1,
            receive_sequence: 0,
            sequence_owners: BTreeMap::new(),
            product_definitions: BTreeMap::new(),
        }
    }

    pub fn scan_once(&mut self) -> Result<(), String> {
        let scan = enumerate_devices(self.enumerator.as_ref())?;
        self.apply_scan(scan);
        Ok(())
    }

    pub(crate) fn apply_scan(&mut self, scan: DeviceScan) {
        let mut classified = Vec::new();
        for observation in scan.serial {
            if let Some(board) = self
                .registry
                .board_by_runtime_usb(observation.vid, observation.pid)
            {
                classified.push(ClassifiedObservation::Runtime { board, observation });
            }
        }
        for observation in scan.bootloader {
            if let Some(board) = self
                .registry
                .board_by_bootloader_usb(observation.vid, observation.pid)
            {
                classified.push(ClassifiedObservation::Bootloader { board, observation });
            }
        }
        self.reconcile(classified, Instant::now());
    }

    fn reconcile(&mut self, observations: Vec<ClassifiedObservation>, now: Instant) {
        self.candidates.clear();
        self.rebuild_offline_devices();
        let mut groups = BTreeMap::<DeviceId, Vec<ClassifiedObservation>>::new();
        for observation in observations {
            let Some(serial) = observation.serial() else {
                self.candidates.push(candidate_from(
                    &observation,
                    None,
                    IdentityDimension::InvalidIdentity,
                    None,
                ));
                continue;
            };
            match self.registry.device_id(observation.board().id, serial) {
                Ok(id) => groups.entry(id).or_default().push(observation),
                Err(_) => self.candidates.push(candidate_from(
                    &observation,
                    None,
                    IdentityDimension::InvalidIdentity,
                    None,
                )),
            }
        }

        let mut active_runtime = BTreeSet::new();
        for (device_id, group) in groups {
            if group.len() > 1 {
                self.stop_worker(&device_id);
                for observation in &group {
                    self.candidates.push(candidate_from(
                        observation,
                        Some(device_id.clone()),
                        IdentityDimension::DuplicateIdentity,
                        None,
                    ));
                }
                if let Some(status) = self.devices.get_mut(&device_id) {
                    status.connection = ConnectionDimension::Online;
                    status.mode = Some(group[0].mode());
                    status.identity = IdentityDimension::DuplicateIdentity;
                    status.runtime = RuntimeDimension::Inactive;
                    status.latest_error = Some(RuntimeActivity::new("duplicate_identity"));
                    status.port = None;
                    status.firmware_build_id = None;
                    status.firmware_protocol = None;
                    status.pins.clear();
                    status.learning = None;
                }
                continue;
            }
            let observation = &group[0];
            match observation {
                ClassifiedObservation::Bootloader { .. } => {
                    self.recovering_devices.remove(&device_id);
                    self.reconnect_not_before.remove(&device_id);
                    self.stop_worker(&device_id);
                    if let Some(status) = self.devices.get_mut(&device_id) {
                        set_observed(status, observation, IdentityDimension::Valid);
                        status.runtime = RuntimeDimension::Inactive;
                    } else {
                        self.candidates.push(candidate_from(
                            observation,
                            Some(device_id),
                            IdentityDimension::Valid,
                            None,
                        ));
                    }
                }
                ClassifiedObservation::Runtime { board, observation } => {
                    active_runtime.insert(device_id.clone());
                    if let Some(slot) = self.workers.get_mut(&device_id) {
                        let update_error = if slot.port != observation.port {
                            match slot
                                .worker
                                .send(WorkerCommand::UpdatePort(observation.port.clone()))
                            {
                                Ok(()) => {
                                    slot.port = observation.port.clone();
                                    None
                                }
                                Err(error) => Some(error),
                            }
                        } else {
                            None
                        };
                        if let Some(error) = update_error {
                            self.stop_worker(&device_id);
                            if let Some(status) = self.devices.get_mut(&device_id) {
                                status.connection = ConnectionDimension::Offline;
                                status.mode = None;
                                status.runtime = RuntimeDimension::Inactive;
                                status.port = None;
                                status.firmware_build_id = None;
                                status.firmware_protocol = None;
                                status.pins.clear();
                                status.learning = None;
                                status.latest_error = Some(runtime_error(error));
                            } else {
                                self.candidates.push(candidate_from_runtime(
                                    board,
                                    observation,
                                    Some(device_id),
                                    IdentityDimension::Validating,
                                    Some(error),
                                ));
                            }
                            continue;
                        }
                        if let Some(status) = self.devices.get_mut(&device_id) {
                            let identity = status.identity;
                            set_runtime_observed(status, board, observation, identity);
                        } else {
                            self.candidates.push(candidate_from_runtime(
                                board,
                                observation,
                                Some(device_id),
                                IdentityDimension::Validating,
                                None,
                            ));
                        }
                        continue;
                    }
                    if self
                        .reconnect_not_before
                        .get(&device_id)
                        .is_some_and(|retry_at| now < *retry_at)
                    {
                        continue;
                    }
                    self.reconnect_not_before.remove(&device_id);
                    let start = WorkerStart {
                        generation: self.generation,
                        device_id: device_id.clone(),
                        port: observation.port.clone(),
                        board_profile_id: board.id.into(),
                    };
                    match self.launcher.start_with_renderers(
                        start,
                        self.event_sender.clone(),
                        WorkerRendererRegistry::new(Arc::clone(&self.renderers)),
                    ) {
                        Ok(mut worker) => {
                            if let Some(error) =
                                self.display_snapshot.as_ref().and_then(|snapshot| {
                                    worker
                                        .send(WorkerCommand::UpdateDisplay(Arc::clone(snapshot)))
                                        .err()
                                })
                            {
                                worker.stop();
                                worker.join();
                                self.candidates.push(candidate_from_runtime(
                                    board,
                                    observation,
                                    Some(device_id),
                                    IdentityDimension::Validating,
                                    Some(error),
                                ));
                                continue;
                            }
                            if let Some(error) = self.usage_snapshot.as_ref().and_then(|snapshot| {
                                worker
                                    .send(WorkerCommand::UpdateUsage(Arc::clone(snapshot)))
                                    .err()
                            }) {
                                worker.stop();
                                worker.join();
                                self.candidates.push(candidate_from_runtime(
                                    board,
                                    observation,
                                    Some(device_id),
                                    IdentityDimension::Validating,
                                    Some(error),
                                ));
                                continue;
                            }
                            self.workers.insert(
                                device_id.clone(),
                                WorkerSlot {
                                    worker,
                                    port: observation.port.clone(),
                                    firmware_revision: 0,
                                },
                            );
                            if let Some(status) = self.devices.get_mut(&device_id) {
                                set_runtime_observed(
                                    status,
                                    board,
                                    observation,
                                    IdentityDimension::Validating,
                                );
                            } else {
                                self.candidates.push(candidate_from_runtime(
                                    board,
                                    observation,
                                    Some(device_id),
                                    IdentityDimension::Validating,
                                    None,
                                ));
                            }
                        }
                        Err(error) => self.candidates.push(candidate_from_runtime(
                            board,
                            observation,
                            Some(device_id),
                            IdentityDimension::Validating,
                            Some(error),
                        )),
                    }
                }
            }
        }

        let departed = self
            .workers
            .keys()
            .filter(|id| !active_runtime.contains(*id))
            .cloned()
            .collect::<Vec<_>>();
        for id in departed {
            self.stop_worker(&id);
            if let Some(status) = self.devices.get_mut(&id) {
                status.connection = ConnectionDimension::Offline;
                status.mode = None;
                status.runtime = RuntimeDimension::Inactive;
                status.port = None;
                status.firmware_build_id = None;
                status.firmware_protocol = None;
                status.pins.clear();
                status.latest_error = None;
                status.learning = None;
            }
        }
    }

    pub fn drain_worker_events(&mut self) -> Vec<RuntimeEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_receiver.try_recv() {
            if let Some(event) = self.handle_worker_event(event) {
                events.push(event);
            }
        }
        events
    }

    pub fn handle_worker_event(&mut self, event: WorkerEvent) -> Option<RuntimeEvent> {
        if event_generation(&event) != self.generation {
            return None;
        }
        match event {
            WorkerEvent::HelloValidated {
                generation: _,
                device_id,
                capabilities,
                product_definition,
            } => {
                self.accept_hello(device_id, capabilities, product_definition);
                None
            }
            WorkerEvent::Input {
                generation: _,
                device_id,
                captured,
            } => {
                self.receive_sequence = self
                    .receive_sequence
                    .checked_add(1)
                    .expect("receive sequence exhausted");
                let receive_sequence = self.receive_sequence;
                self.sequence_owners
                    .insert(receive_sequence, device_id.clone());
                if let Some(paste) = &self.paste {
                    let _ = paste.register_sequence(receive_sequence);
                }
                if let Some(slot) = self.workers.get(&device_id) {
                    if slot
                        .worker
                        .send(WorkerCommand::Input {
                            receive_sequence,
                            captured,
                        })
                        .is_err()
                    {
                        self.sequence_owners.remove(&receive_sequence);
                        if let Some(paste) = &self.paste {
                            let _ = paste.finish_sequence(receive_sequence);
                        }
                    }
                } else {
                    self.sequence_owners.remove(&receive_sequence);
                    if let Some(paste) = &self.paste {
                        let _ = paste.finish_sequence(receive_sequence);
                    }
                }
                None
            }
            WorkerEvent::SequenceFinished {
                generation: _,
                device_id,
                receive_sequence,
            } => {
                if self.sequence_owners.get(&receive_sequence) == Some(&device_id) {
                    self.sequence_owners.remove(&receive_sequence);
                    if let Some(paste) = &self.paste {
                        let _ = paste.finish_sequence(receive_sequence);
                    }
                }
                None
            }
            WorkerEvent::Activity {
                generation: _,
                device_id,
                context,
                activity,
            } => {
                if matches!(
                    activity.code.as_str(),
                    "topology_active" | "topology_cleared"
                ) {
                    self.recovering_devices.remove(&device_id);
                    self.reconnect_not_before.remove(&device_id);
                }
                let event = self.runtime_event(&device_id, context, activity.clone());
                if self.workers.contains_key(&device_id)
                    && let Some(status) = self.devices.get_mut(&device_id)
                {
                    if activity.code == "topology_active" {
                        status.runtime = RuntimeDimension::Ready;
                        status.latest_error = None;
                    } else if activity.code == "topology_cleared" {
                        status.runtime = RuntimeDimension::Inactive;
                        status.latest_error = None;
                    } else if activity.code == "topology_rejected"
                        || activity.code == "firmware_update_required"
                        || activity.code.ends_with("failed")
                        || activity.code.ends_with("mismatch")
                        || activity.code.ends_with("timeout")
                    {
                        status.runtime = RuntimeDimension::RuntimeError;
                        status.latest_error = Some(activity.clone());
                    }
                }
                Some(event)
            }
            WorkerEvent::UsageView {
                generation: _,
                device_id,
                active,
            } => {
                if active && self.workers.contains_key(&device_id) {
                    self.usage_view_devices.insert(device_id);
                } else {
                    self.usage_view_devices.remove(&device_id);
                }
                None
            }
            WorkerEvent::Disconnected {
                generation: _,
                device_id,
                error,
            } => {
                if !self.workers.contains_key(&device_id) {
                    return None;
                }
                if !self.recovering_devices.insert(device_id.clone()) {
                    self.reconnect_not_before
                        .insert(device_id.clone(), Instant::now() + WORKER_RECONNECT_BACKOFF);
                }
                self.stop_worker(&device_id);
                if let Some(status) = self.devices.get_mut(&device_id) {
                    status.connection = ConnectionDimension::Offline;
                    status.mode = None;
                    status.runtime = RuntimeDimension::Inactive;
                    status.firmware_build_id = None;
                    status.firmware_protocol = None;
                    status.pins.clear();
                    status.port = None;
                    status.learning = None;
                    status.latest_error = error.clone().map(runtime_error);
                } else if let Some(candidate) = self
                    .candidates
                    .iter_mut()
                    .find(|candidate| candidate.device_id.as_ref() == Some(&device_id))
                {
                    candidate.latest_error = error;
                    candidate.issue = candidate_issue(
                        candidate.mode,
                        candidate.identity,
                        candidate.latest_error.as_deref(),
                    );
                }
                None
            }
        }
    }

    fn runtime_event(
        &self,
        device_id: &DeviceId,
        context: RuntimeEventContext,
        activity: RuntimeActivity,
    ) -> RuntimeEvent {
        let board = self
            .registry
            .board_by_id(device_id.board_profile_id())
            .expect("validated Device ID references a compiled Board Profile");
        RuntimeEvent {
            timestamp_ms: context.timestamp_ms,
            level: activity_level(&activity.code),
            device_id: device_id.clone(),
            raw_serial: device_id.hardware_serial().into(),
            controller_family_id: board.family_id.into(),
            board_profile_id: board.id.into(),
            port: context.port,
            device_profile_id: context.device_profile_id,
            hardware_profile_id: context.hardware_profile_id,
            home_update: None,
            activity,
        }
    }

    fn accept_hello(
        &mut self,
        device_id: DeviceId,
        capabilities: HelloCapabilities,
        product_definition: Option<ProductDefinition>,
    ) {
        if !self.workers.contains_key(&device_id) {
            return;
        }
        let Some(board) = self.registry.board_by_id(device_id.board_profile_id()) else {
            self.stop_worker(&device_id);
            return;
        };
        if let Err(error) = validate_hello(board, &capabilities) {
            self.reject_worker(&device_id, error.code);
            return;
        }
        if capabilities.product_version_id.as_deref()
            != product_definition
                .as_ref()
                .map(|definition| definition.product.product_version_id.as_str())
        {
            self.reject_worker(&device_id, "product_version_id_mismatch".into());
            return;
        }
        let revision = {
            let mut workspace = self
                .workspace
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let enrolled = match product_definition.as_ref() {
                Some(definition) => workspace.enroll_product_device_with_registry(
                    self.registry,
                    device_id.clone(),
                    definition,
                ),
                None => workspace.enroll_device_with_registry(self.registry, device_id.clone()),
            };
            match enrolled {
                Ok(_) => Ok(WorkspaceRevision::capture(&workspace)),
                Err(error) => Err(error),
            }
        };
        let revision = match revision {
            Ok(revision) => revision,
            Err(error) => {
                self.reject_worker(&device_id, error.code);
                return;
            }
        };
        self.workspace_revision = revision;
        if let Some(definition) = product_definition {
            self.product_definitions
                .insert(device_id.clone(), definition);
        } else {
            self.product_definitions.remove(&device_id);
        }
        let profile = runtime_profile(
            &self.workspace_revision,
            &device_id,
            self.product_definitions.get(&device_id),
        );
        if profile.is_none() {
            self.recovering_devices.remove(&device_id);
            self.reconnect_not_before.remove(&device_id);
        }
        self.rebuild_device(&device_id);
        if let Some(status) = self.devices.get_mut(&device_id) {
            status.connection = ConnectionDimension::Online;
            status.mode = Some(DeviceMode::Runtime);
            status.identity = IdentityDimension::Valid;
            status.firmware_build_id = Some(capabilities.firmware_build_id.clone());
            status.product_version_id = capabilities.product_version_id.clone();
            status.product_definition = self.product_definitions.get(&device_id).cloned();
            status.product_config = self
                .workspace_revision
                .settings
                .devices
                .get(&device_id)
                .and_then(|device| device.product_config.clone());
            status.firmware_protocol = Some(capabilities.protocol);
            status.pins = capabilities.pins;
            status.port = self.workers.get(&device_id).map(|slot| slot.port.clone());
            status.runtime = if profile.is_some() {
                RuntimeDimension::Configuring
            } else {
                RuntimeDimension::Inactive
            };
        }
        if let Err(error) = self.reconfigure_worker(&device_id, profile.map(Arc::new))
            && let Some(status) = self.devices.get_mut(&device_id)
        {
            status.runtime = RuntimeDimension::RuntimeError;
            status.latest_error = Some(runtime_error(error));
        }
        self.candidates
            .retain(|candidate| candidate.device_id.as_ref() != Some(&device_id));
    }

    fn rebuild_offline_devices(&mut self) {
        let previous = std::mem::take(&mut self.devices);
        self.devices = self
            .workspace_revision
            .settings
            .devices
            .keys()
            .map(|id| {
                (
                    id.clone(),
                    offline_status(self.registry, &self.workspace_revision, id),
                )
            })
            .collect();
        for id in self.workers.keys() {
            if let Some(status) = previous.get(id) {
                self.devices.insert(id.clone(), status.clone());
            }
        }
    }

    fn rebuild_device(&mut self, id: &DeviceId) {
        if self.workspace_revision.settings.devices.contains_key(id) {
            self.devices.insert(
                id.clone(),
                offline_status(self.registry, &self.workspace_revision, id),
            );
        }
    }

    fn stop_worker(&mut self, id: &DeviceId) {
        self.usage_view_devices.remove(id);
        if let Some(mut slot) = self.workers.remove(id) {
            slot.worker.stop();
            slot.worker.join();
        }
        if let Some(paste) = &self.paste {
            let _ = paste.cancel_device(id);
            let sequences = self
                .sequence_owners
                .iter()
                .filter_map(|(sequence, owner)| (owner == id).then_some(*sequence))
                .collect::<Vec<_>>();
            for sequence in sequences {
                let _ = paste.finish_sequence(sequence);
                self.sequence_owners.remove(&sequence);
            }
        }
    }

    fn reject_worker(&mut self, id: &DeviceId, error: String) {
        self.stop_worker(id);
        if let Some(status) = self.devices.get_mut(id) {
            status.runtime = RuntimeDimension::RuntimeError;
            status.latest_error = Some(runtime_error(error.clone()));
        } else if let Some(candidate) = self
            .candidates
            .iter_mut()
            .find(|candidate| candidate.device_id.as_ref() == Some(id))
        {
            candidate.latest_error = Some(error);
            candidate.issue = candidate_issue(
                candidate.mode,
                candidate.identity,
                candidate.latest_error.as_deref(),
            );
        }
    }

    fn reconfigure_worker(
        &mut self,
        id: &DeviceId,
        snapshot: Option<Arc<RuntimeProfileSnapshot>>,
    ) -> Result<u32, String> {
        let slot = self
            .workers
            .get_mut(id)
            .ok_or_else(|| "device_offline".to_owned())?;
        let revision = slot.next_revision();
        slot.worker
            .send(WorkerCommand::Reconfigure { snapshot, revision })?;
        Ok(revision)
    }

    pub fn update_display(&mut self, snapshot: Arc<DisplaySnapshot>) {
        self.display_snapshot = Some(Arc::clone(&snapshot));
        let failures = self
            .workers
            .iter()
            .filter_map(|(id, slot)| {
                slot.worker
                    .send(WorkerCommand::UpdateDisplay(Arc::clone(&snapshot)))
                    .err()
                    .map(|error| (id.clone(), error))
            })
            .collect::<Vec<_>>();
        for (id, error) in failures {
            if let Some(status) = self.devices.get_mut(&id) {
                status.runtime = RuntimeDimension::RuntimeError;
                status.latest_error = Some(runtime_error(error));
            }
        }
    }

    pub fn update_usage(&mut self, snapshot: Arc<UsageSnapshot>) {
        self.usage_snapshot = Some(Arc::clone(&snapshot));
        let failures = self
            .workers
            .iter()
            .filter_map(|(id, slot)| {
                slot.worker
                    .send(WorkerCommand::UpdateUsage(Arc::clone(&snapshot)))
                    .err()
                    .map(|error| (id.clone(), error))
            })
            .collect::<Vec<_>>();
        for (id, error) in failures {
            if let Some(status) = self.devices.get_mut(&id) {
                status.runtime = RuntimeDimension::RuntimeError;
                status.latest_error = Some(runtime_error(error));
            }
        }
    }

    pub fn usage_requested(&self) -> bool {
        !self.usage_view_devices.is_empty()
    }

    pub(crate) fn product_definition(&self, id: &DeviceId) -> Option<&ProductDefinition> {
        self.product_definitions.get(id)
    }

    pub fn apply_workspace_revision(&mut self, next: WorkspaceRevision) {
        let updates = self
            .workers
            .keys()
            .filter_map(|id| {
                workspace_worker_update(&self.workspace_revision, &next, id).map(|update| {
                    (
                        id.clone(),
                        runtime_profile(&next, id, self.product_definitions.get(id)).map(Arc::new),
                        update,
                    )
                })
            })
            .collect::<Vec<_>>();
        self.workspace_revision = next;
        for (id, snapshot, update) in updates {
            if update == WorkspaceWorkerUpdate::Reconfigure {
                let has_assignment = snapshot.is_some();
                let result = self.reconfigure_worker(&id, snapshot);
                if let Some(status) = self.devices.get_mut(&id) {
                    match result {
                        Ok(_) => {
                            status.runtime = if has_assignment {
                                RuntimeDimension::Configuring
                            } else {
                                RuntimeDimension::Inactive
                            };
                            status.learning = None;
                            status.latest_error = None;
                        }
                        Err(error) => {
                            status.runtime = RuntimeDimension::RuntimeError;
                            status.latest_error = Some(runtime_error(error));
                        }
                    }
                }
            } else if let Some(slot) = self.workers.get(&id)
                && let Err(error) = slot.worker.send(WorkerCommand::UpdateSnapshot(snapshot))
                && let Some(status) = self.devices.get_mut(&id)
            {
                status.runtime = RuntimeDimension::RuntimeError;
                status.latest_error = Some(runtime_error(error));
            }
        }
        self.refresh_persisted_status();
    }

    pub(crate) fn activate_restored_revision(&mut self, next: WorkspaceRevision) -> RetiredWorkers {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("worker generation exhausted");
        self.workspace_revision = next;
        let mut retired = Vec::with_capacity(self.workers.len());
        for (_, mut slot) in std::mem::take(&mut self.workers) {
            slot.worker.stop();
            retired.push(slot.worker);
        }
        if let Some(paste) = &self.paste {
            for device_id in self.devices.keys() {
                let _ = paste.cancel_device(device_id);
            }
            for sequence in self.sequence_owners.keys().copied().collect::<Vec<_>>() {
                let _ = paste.finish_sequence(sequence);
            }
        }
        self.sequence_owners.clear();
        self.candidates.clear();
        self.devices = self
            .workspace_revision
            .settings
            .devices
            .keys()
            .map(|id| {
                (
                    id.clone(),
                    offline_status(self.registry, &self.workspace_revision, id),
                )
            })
            .collect();
        RetiredWorkers(retired)
    }

    #[cfg(test)]
    pub fn apply_profile_change(&mut self, _change: &ProfileChange) {
        let revision = {
            let workspace = self
                .workspace
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            WorkspaceRevision::capture(&workspace)
        };
        self.apply_workspace_revision(revision);
    }

    pub fn begin_learning(
        &mut self,
        device_id: &DeviceId,
        device_profile_id: &str,
        hardware_profile_id: &str,
        editing_revision: u64,
        pins: Vec<u8>,
    ) -> Result<LearningTarget, AppError> {
        let status = self
            .devices
            .get(device_id)
            .ok_or_else(|| AppError::new("unknown_device"))?
            .clone();
        if status.connection != ConnectionDimension::Online
            || status.mode != Some(DeviceMode::Runtime)
            || status.identity != IdentityDimension::Valid
            || !self.workers.contains_key(device_id)
        {
            return Err(AppError::new("device_not_available"));
        }
        if status.learning.is_some() {
            return Err(AppError::new("learning_session_active"));
        }
        let board = self
            .registry
            .board_by_id(device_id.board_profile_id())
            .ok_or_else(|| AppError::new("unknown_board_profile"))?;
        {
            let profile = self
                .workspace_revision
                .profiles
                .get(device_profile_id)
                .ok_or_else(|| AppError::new("unknown_profile"))?;
            let hardware = profile
                .hardware_profile(hardware_profile_id)
                .ok_or_else(|| AppError::new("unknown_hardware_profile"))?;
            if hardware.board_profile_id != board.id {
                return Err(AppError::new("learning_board_mismatch"));
            }
        }
        let unique = pins.iter().copied().collect::<BTreeSet<_>>();
        if pins.is_empty()
            || unique.len() != pins.len()
            || !unique.iter().all(|pin| board.safe_pins.contains(pin))
            || !unique.iter().all(|pin| status.pins.contains(pin))
        {
            return Err(AppError::new("invalid_learning_pins"));
        }
        let slot = self
            .workers
            .get_mut(device_id)
            .ok_or_else(|| AppError::new("device_not_available"))?;
        let firmware_revision = slot.next_revision();
        let target = LearningTarget {
            device_id: device_id.clone(),
            device_profile_id: device_profile_id.into(),
            hardware_profile_id: hardware_profile_id.into(),
            editing_revision,
            firmware_revision,
            pins,
        };
        slot.worker
            .send(WorkerCommand::BeginLearning(target.clone()))
            .map_err(|detail| AppError::new("learning_command_failed").with_detail(detail))?;
        if let Some(status) = self.devices.get_mut(device_id) {
            status.runtime = RuntimeDimension::Learning;
            status.learning = Some(target.clone());
            status.latest_error = None;
        }
        Ok(target)
    }

    pub fn end_learning(&mut self, device_id: &DeviceId) -> Result<(), AppError> {
        if self
            .devices
            .get(device_id)
            .and_then(|status| status.learning.as_ref())
            .is_none()
        {
            return Err(AppError::new("no_learning_session"));
        }
        let snapshot = runtime_profile(
            &self.workspace_revision,
            device_id,
            self.product_definitions.get(device_id),
        )
        .map(Arc::new);
        let has_assignment = snapshot.is_some();
        let slot = self
            .workers
            .get_mut(device_id)
            .ok_or_else(|| AppError::new("device_not_available"))?;
        let revision = slot.next_revision();
        slot.worker
            .send(WorkerCommand::EndLearning { snapshot, revision })
            .map_err(|detail| AppError::new("learning_command_failed").with_detail(detail))?;
        if let Some(status) = self.devices.get_mut(device_id) {
            status.learning = None;
            status.runtime = if has_assignment {
                RuntimeDimension::Configuring
            } else {
                RuntimeDimension::Inactive
            };
            status.latest_error = None;
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn cancel_learning(&mut self, device_id: &DeviceId) -> Result<(), AppError> {
        self.end_learning(device_id)
    }

    #[cfg(test)]
    pub fn sync_profiles(&mut self) {
        self.workspace_revision = {
            let workspace = self
                .workspace
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            WorkspaceRevision::capture(&workspace)
        };
        let updates = self
            .workers
            .keys()
            .map(|id| {
                (
                    id.clone(),
                    runtime_profile(
                        &self.workspace_revision,
                        id,
                        self.product_definitions.get(id),
                    )
                    .map(Arc::new),
                )
            })
            .collect::<Vec<_>>();
        for (id, snapshot) in updates {
            let has_assignment = snapshot.is_some();
            let result = self.reconfigure_worker(&id, snapshot);
            if let Some(status) = self.devices.get_mut(&id) {
                match result {
                    Ok(_) => {
                        status.learning = None;
                        status.runtime = if has_assignment {
                            RuntimeDimension::Configuring
                        } else {
                            RuntimeDimension::Inactive
                        };
                        status.latest_error = None;
                    }
                    Err(error) => {
                        status.runtime = RuntimeDimension::RuntimeError;
                        status.latest_error = Some(runtime_error(error));
                    }
                }
            }
        }
        self.refresh_persisted_status();
    }

    fn refresh_persisted_status(&mut self) {
        let registry = self.registry;
        self.devices
            .retain(|id, _| self.workspace_revision.settings.devices.contains_key(id));
        for id in self.workspace_revision.settings.devices.keys() {
            self.devices
                .entry(id.clone())
                .or_insert_with(|| offline_status(registry, &self.workspace_revision, id));
        }
        for (id, status) in &mut self.devices {
            if self.workspace_revision.settings.devices.contains_key(id) {
                let persisted = offline_status(registry, &self.workspace_revision, id);
                status.name = persisted.name;
                status.assignment = persisted.assignment;
                status.runtime_assignment = persisted.runtime_assignment;
            }
        }
    }

    pub fn shutdown(&mut self) {
        let ids = self.workers.keys().cloned().collect::<Vec<_>>();
        for id in ids {
            self.stop_worker(&id);
        }
    }

    pub fn devices(&self) -> Vec<DeviceStatus> {
        self.devices.values().cloned().collect()
    }

    pub fn candidates(&self) -> Vec<CandidateStatus> {
        self.candidates.clone()
    }

    pub fn retry_candidate(&mut self, device_id: &DeviceId) -> Result<(), String> {
        let matching = self
            .candidates
            .iter()
            .filter(|candidate| candidate.device_id.as_ref() == Some(device_id))
            .collect::<Vec<_>>();
        if matching.is_empty() {
            return Err("candidate_not_found".into());
        }
        if matching.len() != 1 || matching[0].identity == IdentityDimension::DuplicateIdentity {
            return Err("candidate_identity_conflict".into());
        }
        if matching[0].mode != DeviceMode::Runtime
            || matching[0].identity == IdentityDimension::InvalidIdentity
        {
            return Err("candidate_not_retryable".into());
        }
        self.reconnect_not_before.remove(device_id);
        self.stop_worker(device_id);
        Ok(())
    }

    #[cfg(test)]
    pub fn last_receive_sequence(&self) -> u64 {
        self.receive_sequence
    }
}

fn activity_level(code: &str) -> EventLevel {
    match code {
        "topology_active"
        | "topology_cleared"
        | "input_state"
        | "learning_ready"
        | "learning_input"
        | "feature_switch_changed"
        | "trigger_occurred"
        | "action_step_started"
        | "action_step_completed" => EventLevel::Info,
        "input_before_configuration"
        | "unexpected_action_acknowledgement"
        | "unmapped_input"
        | "empty_action_list"
        | "feature_disabled"
        | "no_runtime_assignment"
        | "invalid_assignment" => EventLevel::Warning,
        _ => EventLevel::Error,
    }
}

fn runtime_error(code: String) -> RuntimeActivity {
    RuntimeActivity::new(code)
}

fn event_generation(event: &WorkerEvent) -> u64 {
    match event {
        WorkerEvent::HelloValidated { generation, .. }
        | WorkerEvent::Input { generation, .. }
        | WorkerEvent::SequenceFinished { generation, .. }
        | WorkerEvent::Activity { generation, .. }
        | WorkerEvent::UsageView { generation, .. }
        | WorkerEvent::Disconnected { generation, .. } => *generation,
    }
}

fn set_runtime_observed(
    status: &mut DeviceStatus,
    board: &BoardProfile,
    observation: &SerialObservation,
    identity: IdentityDimension,
) {
    status.connection = ConnectionDimension::Online;
    status.mode = Some(DeviceMode::Runtime);
    status.identity = identity;
    status.port = Some(observation.port.clone());
    status.controller_family_id = board.family_id.into();
    status.board_profile_id = board.id.into();
}

fn candidate_from_runtime(
    board: &BoardProfile,
    observation: &SerialObservation,
    device_id: Option<DeviceId>,
    identity: IdentityDimension,
    latest_error: Option<String>,
) -> CandidateStatus {
    let issue = candidate_issue(DeviceMode::Runtime, identity, latest_error.as_deref());
    CandidateStatus {
        key: format!("runtime:{}", observation.port),
        device_id,
        mode: DeviceMode::Runtime,
        identity,
        issue,
        raw_serial: observation.serial_number.clone(),
        port: Some(observation.port.clone()),
        controller_family_id: board.family_id.into(),
        board_profile_id: board.id.into(),
        latest_error,
    }
}

fn set_observed(
    status: &mut DeviceStatus,
    observation: &ClassifiedObservation,
    identity: IdentityDimension,
) {
    status.connection = ConnectionDimension::Online;
    status.mode = Some(observation.mode());
    status.identity = identity;
    status.port = observation.port();
    status.controller_family_id = observation.board().family_id.into();
    status.board_profile_id = observation.board().id.into();
    if observation.mode() == DeviceMode::Bootloader {
        status.firmware_build_id = None;
        status.firmware_protocol = None;
        status.pins.clear();
        status.learning = None;
        status.latest_error = None;
    }
}

fn candidate_from(
    observation: &ClassifiedObservation,
    device_id: Option<DeviceId>,
    identity: IdentityDimension,
    latest_error: Option<String>,
) -> CandidateStatus {
    let mode = observation.mode();
    let issue = candidate_issue(mode, identity, latest_error.as_deref());
    CandidateStatus {
        key: observation.key(),
        device_id,
        mode,
        identity,
        issue,
        raw_serial: observation.serial().map(str::to_owned),
        port: observation.port(),
        controller_family_id: observation.board().family_id.into(),
        board_profile_id: observation.board().id.into(),
        latest_error,
    }
}

fn candidate_issue(
    mode: DeviceMode,
    identity: IdentityDimension,
    latest_error: Option<&str>,
) -> CandidateIssue {
    match identity {
        IdentityDimension::InvalidIdentity => CandidateIssue::InvalidIdentity,
        IdentityDimension::DuplicateIdentity => CandidateIssue::DuplicateIdentity,
        IdentityDimension::Validating | IdentityDimension::Valid => {
            if mode == DeviceMode::Bootloader {
                return CandidateIssue::Bootloader;
            }
            match latest_error {
                None => CandidateIssue::Validating,
                Some("serial_handshake_timeout" | "device_disconnected") => {
                    CandidateIssue::FirmwareNotResponding
                }
                Some(
                    "protocol_mismatch"
                    | "firmware_update_required"
                    | "controller_family_mismatch"
                    | "board_profile_mismatch"
                    | "capability_mismatch",
                ) => CandidateIssue::FirmwareIncompatible,
                Some(error)
                    if error.starts_with("serial_open_failed:")
                        || error.starts_with("serial_handshake_failed:")
                        || error.starts_with("serial_read_failed:") =>
                {
                    CandidateIssue::PortUnavailable
                }
                Some(_) => CandidateIssue::Unknown,
            }
        }
    }
}

fn workspace_worker_update(
    old: &WorkspaceRevision,
    new: &WorkspaceRevision,
    id: &DeviceId,
) -> Option<WorkspaceWorkerUpdate> {
    let old_device = old.settings.devices.get(id);
    let new_device = new.settings.devices.get(id);
    let old_assignment = old_device.and_then(|device| device.runtime_assignment.as_ref());
    let new_assignment = new_device.and_then(|device| device.runtime_assignment.as_ref());
    let old_product = old_device.and_then(|device| device.product_config.as_ref());
    let new_product = new_device.and_then(|device| device.product_config.as_ref());
    if old_product != new_product {
        return Some(WorkspaceWorkerUpdate::Snapshot);
    }
    if old_assignment != new_assignment {
        return Some(WorkspaceWorkerUpdate::Reconfigure);
    }
    let assignment = new_assignment?;
    if old_device.map(|device| device.name.as_str())
        != new_device.map(|device| device.name.as_str())
    {
        return Some(WorkspaceWorkerUpdate::Snapshot);
    }
    let old_profile = old.profiles.get(&assignment.device_profile_id);
    let new_profile = new.profiles.get(&assignment.device_profile_id);
    if old_profile == new_profile {
        return None;
    }
    if old_profile.map(DeviceProfile::minimum_protocol_version)
        != new_profile.map(DeviceProfile::minimum_protocol_version)
    {
        return Some(WorkspaceWorkerUpdate::Reconfigure);
    }
    let change = match (old_profile, new_profile) {
        (None, None) => return None,
        (old_profile, new_profile) => ProfileChange::between(old_profile, new_profile),
    };
    if change
        .topology_hardware_profile_ids
        .contains(&assignment.hardware_profile_id)
    {
        Some(WorkspaceWorkerUpdate::Reconfigure)
    } else if change.host_mapping_changed {
        Some(WorkspaceWorkerUpdate::Snapshot)
    } else {
        None
    }
}

fn offline_status(
    registry: HardwareRegistry<'_>,
    workspace: &WorkspaceRevision,
    id: &DeviceId,
) -> DeviceStatus {
    let record = &workspace.settings.devices[id];
    let board = registry
        .board_by_id(&record.board_profile_id)
        .expect("validated persisted board profile");
    let (assignment, runtime_assignment) = if record.product_config.is_some() {
        (
            AssignmentDimension::Valid,
            record.runtime_assignment.clone(),
        )
    } else {
        match workspace.assignment_resolution(id) {
            AssignmentResolution::Unassigned { device } => (
                AssignmentDimension::Unassigned,
                device.runtime_assignment.clone(),
            ),
            AssignmentResolution::Valid {
                device,
                assignment: _,
                profile: _,
                hardware: _,
            } => (
                AssignmentDimension::Valid,
                device.runtime_assignment.clone(),
            ),
            AssignmentResolution::InvalidAssignment {
                device,
                assignment: _,
            } => (
                AssignmentDimension::InvalidAssignment,
                device.runtime_assignment.clone(),
            ),
            AssignmentResolution::UnknownDevice => unreachable!("known device was read"),
        }
    };
    DeviceStatus {
        device_id: id.clone(),
        name: record.name.clone(),
        connection: ConnectionDimension::Offline,
        mode: None,
        identity: IdentityDimension::Valid,
        assignment,
        runtime: RuntimeDimension::Inactive,
        raw_serial: id.hardware_serial().into(),
        port: None,
        controller_family_id: board.family_id.into(),
        board_profile_id: board.id.into(),
        firmware_build_id: None,
        product_version_id: record
            .product_config
            .as_ref()
            .map(|config| config.product_version_id.clone()),
        product_definition: None,
        product_config: record.product_config.clone(),
        firmware_protocol: None,
        pins: Vec::new(),
        runtime_assignment,
        latest_error: None,
        learning: None,
    }
}

fn runtime_profile(
    workspace: &WorkspaceRevision,
    id: &DeviceId,
    product_definition: Option<&ProductDefinition>,
) -> Option<RuntimeProfileSnapshot> {
    let device = workspace.settings.devices.get(id)?;
    if let (Some(config), Some(definition)) = (&device.product_config, product_definition) {
        if config.product_version_id != definition.product.product_version_id {
            return None;
        }
        let profile =
            definition.as_runtime_profile(config.trigger_settings.clone(), config.actions.clone());
        let hardware_profile_id = definition.hardware_profile.id.clone();
        return Some(RuntimeProfileSnapshot {
            profile,
            hardware_profile_id: hardware_profile_id.clone(),
            metric_attribution: MetricAttribution {
                device_id: id.clone(),
                device_name: device.name.clone(),
                device_profile_id: config.product_version_id.clone(),
                hardware_profile_id,
            },
        });
    }
    let AssignmentResolution::Valid {
        device,
        assignment,
        profile,
        hardware: _,
    } = workspace.assignment_resolution(id)
    else {
        return None;
    };
    Some(RuntimeProfileSnapshot {
        profile: profile.clone(),
        hardware_profile_id: assignment.hardware_profile_id.clone(),
        metric_attribution: MetricAttribution {
            device_id: id.clone(),
            device_name: device.name.clone(),
            device_profile_id: assignment.device_profile_id.clone(),
            hardware_profile_id: assignment.hardware_profile_id.clone(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        device::DeviceSession,
        display::SourceHealth,
        hardware::{
            BOARD_PROFILES, BoardProfile, DeviceId, TEST_ESP32C3_BOARD_ID,
            TEST_SECOND_RP2040_BOARD_ID, test_registry,
        },
        paste::{ClipboardWriter, PasteCoordinator, PasteReply, PasteRequest},
        product::{PRODUCT_DEFINITION_SCHEMA_VERSION, ProductDefinition, ProductIdentity},
        profile::{
            ButtonAction, DeviceProfile, HardwareProfile, InputSource, PROFILE_SCHEMA_VERSION,
            ProfileChange,
        },
        protocol::{DeviceMessage, HelloCapabilities},
        workspace::Workspace,
    };
    use serialport::{SerialPortInfo, SerialPortType, UsbPortInfo};
    use std::{
        collections::{BTreeMap, BTreeSet},
        fs,
        path::PathBuf,
        sync::{
            Arc, Mutex,
            atomic::{AtomicU64, Ordering},
            mpsc::Sender,
        },
    };

    static TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn trigger_occurred_is_info() {
        assert_eq!(activity_level("trigger_occurred"), EventLevel::Info);
    }

    fn usb_serial_port(port_name: &str, serial_number: &str) -> SerialPortInfo {
        SerialPortInfo {
            port_name: port_name.into(),
            port_type: SerialPortType::UsbPort(UsbPortInfo {
                vid: 0x2e8a,
                pid: 0x102e,
                serial_number: Some(serial_number.into()),
                manufacturer: Some("YD".into()),
                product: Some("Kivo Keyboard RP2040".into()),
            }),
        }
    }

    #[test]
    fn paired_macos_serial_aliases_keep_only_the_callout_port() {
        for ports in [
            vec![
                usb_serial_port("/dev/tty.usbmodem2101", "50031519384E811C"),
                usb_serial_port("/dev/cu.usbmodem2101", "50031519384E811C"),
            ],
            vec![
                usb_serial_port("/dev/cu.usbmodem2101", "50031519384E811C"),
                usb_serial_port("/dev/tty.usbmodem2101", "50031519384E811C"),
            ],
        ] {
            let normalized = collapse_serial_port_aliases(ports);
            let names = normalized
                .iter()
                .map(|port| port.port_name.as_str())
                .collect::<Vec<_>>();
            assert_eq!(names, vec!["/dev/cu.usbmodem2101"]);
        }
    }

    #[test]
    fn unmatched_dialin_and_non_macos_ports_are_preserved() {
        let normalized = collapse_serial_port_aliases(vec![
            usb_serial_port("/dev/tty.usbmodem3101", "TTY-ONLY"),
            usb_serial_port("/dev/ttyACM0", "LINUX"),
            usb_serial_port("COM4", "WINDOWS"),
        ]);
        let names = normalized
            .iter()
            .map(|port| port.port_name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["/dev/tty.usbmodem3101", "/dev/ttyACM0", "COM4"]);
    }

    #[test]
    fn distinct_callout_ports_with_the_same_serial_are_not_deduplicated() {
        let normalized = collapse_serial_port_aliases(vec![
            usb_serial_port("/dev/cu.usbmodem2101", "DUPLICATE"),
            usb_serial_port("/dev/cu.usbmodem3101", "DUPLICATE"),
        ]);

        assert_eq!(normalized.len(), 2);
    }

    #[test]
    fn candidate_issue_covers_identity_mode_and_worker_failures() {
        use CandidateIssue::*;

        assert_eq!(
            candidate_issue(DeviceMode::Runtime, IdentityDimension::Validating, None),
            Validating
        );
        assert_eq!(
            candidate_issue(DeviceMode::Bootloader, IdentityDimension::Valid, None),
            Bootloader
        );
        assert_eq!(
            candidate_issue(
                DeviceMode::Runtime,
                IdentityDimension::InvalidIdentity,
                None
            ),
            InvalidIdentity
        );
        assert_eq!(
            candidate_issue(
                DeviceMode::Runtime,
                IdentityDimension::DuplicateIdentity,
                None
            ),
            DuplicateIdentity
        );
        assert_eq!(
            candidate_issue(
                DeviceMode::Runtime,
                IdentityDimension::Validating,
                Some("serial_handshake_timeout")
            ),
            FirmwareNotResponding
        );
        assert_eq!(
            candidate_issue(
                DeviceMode::Runtime,
                IdentityDimension::Validating,
                Some("protocol_mismatch")
            ),
            FirmwareIncompatible
        );
        assert_eq!(
            candidate_issue(
                DeviceMode::Runtime,
                IdentityDimension::Validating,
                Some("controller_family_mismatch")
            ),
            FirmwareIncompatible
        );
        assert_eq!(
            candidate_issue(
                DeviceMode::Runtime,
                IdentityDimension::Validating,
                Some("board_profile_mismatch")
            ),
            FirmwareIncompatible
        );
        assert_eq!(
            candidate_issue(
                DeviceMode::Runtime,
                IdentityDimension::Validating,
                Some("capability_mismatch")
            ),
            FirmwareIncompatible
        );
        assert_eq!(
            candidate_issue(
                DeviceMode::Runtime,
                IdentityDimension::Validating,
                Some("serial_open_failed: busy")
            ),
            PortUnavailable
        );
        assert_eq!(
            candidate_issue(
                DeviceMode::Runtime,
                IdentityDimension::Validating,
                Some("serial_handshake_failed: denied")
            ),
            PortUnavailable
        );
        assert_eq!(
            candidate_issue(
                DeviceMode::Runtime,
                IdentityDimension::Validating,
                Some("unclassified failure")
            ),
            Unknown
        );
    }

    #[test]
    fn candidate_status_serializes_issue_without_removing_raw_error() {
        let candidate = CandidateStatus {
            key: "runtime:/dev/cu.usbmodem1101".into(),
            device_id: None,
            mode: DeviceMode::Runtime,
            identity: IdentityDimension::Validating,
            issue: CandidateIssue::FirmwareNotResponding,
            raw_serial: Some("50031519384E811C".into()),
            port: Some("/dev/cu.usbmodem1101".into()),
            controller_family_id: "rp2040".into(),
            board_profile_id: crate::hardware::YD_RP2040_BOARD_ID.into(),
            latest_error: Some("serial_handshake_timeout".into()),
        };

        let value = serde_json::to_value(candidate).unwrap();
        assert_eq!(value["issue"], "firmware_not_responding");
        assert_eq!(value["latestError"], "serial_handshake_timeout");
    }

    #[test]
    fn retry_candidate_restarts_only_the_exact_identity() {
        let (_directory, enumerator, launcher, mut coordinator) = harness();
        launcher.set_hello(
            "/dev/a",
            HelloCapabilities {
                protocol: 4,
                controller_family_id: "wrong-family".into(),
                board_profile_id: crate::hardware::YD_ESP32_S3_BOARD_ID.into(),
                firmware_build_id: "bad".into(),
                product_version_id: None,
                pins: vec![0],
            },
        );
        enumerator.set(
            vec![
                serial("/dev/a", 0x303a, 0x4002, Some("RETRY-A")),
                serial("/dev/b", 0x303a, 0x4002, Some("RETRY-B")),
            ],
            Vec::new(),
        );
        scan(&mut coordinator);
        let a = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "RETRY-A").unwrap();
        let b = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "RETRY-B").unwrap();
        assert!(
            coordinator
                .candidates()
                .iter()
                .any(|candidate| candidate.device_id.as_ref() == Some(&a))
        );
        assert!(
            coordinator
                .devices()
                .iter()
                .any(|device| device.device_id == b)
        );

        coordinator.retry_candidate(&a).unwrap();

        let starts_after = launcher.starts();
        assert_eq!(
            starts_after
                .iter()
                .filter(|start| start.device_id == a)
                .count(),
            1
        );
        assert_eq!(
            starts_after
                .iter()
                .filter(|start| start.device_id == b)
                .count(),
            1
        );

        scan(&mut coordinator);

        let starts_after_scan = launcher.starts();
        assert_eq!(
            starts_after_scan
                .iter()
                .filter(|start| start.device_id == a)
                .count(),
            2
        );
        assert_eq!(
            starts_after_scan
                .iter()
                .filter(|start| start.device_id == b)
                .count(),
            1
        );
    }

    #[test]
    fn retry_candidate_rejects_missing_and_duplicate_identity() {
        let (_directory, enumerator, _launcher, mut coordinator) = harness();
        let missing = DeviceId::new(crate::hardware::YD_RP2040_BOARD_ID, "MISSING").unwrap();
        assert_eq!(
            coordinator.retry_candidate(&missing).unwrap_err(),
            "candidate_not_found"
        );

        enumerator.set(
            vec![
                serial("/dev/one", 0x2e8a, 0x102e, Some("DUPLICATE")),
                serial("/dev/two", 0x2e8a, 0x102e, Some("DUPLICATE")),
            ],
            Vec::new(),
        );
        scan(&mut coordinator);
        let duplicate = DeviceId::new(crate::hardware::YD_RP2040_BOARD_ID, "DUPLICATE").unwrap();
        assert_eq!(
            coordinator.retry_candidate(&duplicate).unwrap_err(),
            "candidate_identity_conflict"
        );
    }

    #[derive(Default)]
    struct FakeEnumerator {
        serial: Mutex<Vec<SerialObservation>>,
        bootloader: Mutex<Vec<BootloaderObservation>>,
    }

    impl FakeEnumerator {
        fn set(&self, serial: Vec<SerialObservation>, bootloader: Vec<BootloaderObservation>) {
            *self.serial.lock().unwrap() = serial;
            *self.bootloader.lock().unwrap() = bootloader;
        }
    }

    impl UsbEnumerator for FakeEnumerator {
        fn serial_ports(&self) -> Result<Vec<SerialObservation>, String> {
            Ok(self.serial.lock().unwrap().clone())
        }

        fn usb_devices(&self) -> Result<Vec<BootloaderObservation>, String> {
            Ok(self.bootloader.lock().unwrap().clone())
        }
    }

    #[derive(Default)]
    struct FakeLauncher {
        starts: Mutex<Vec<WorkerStart>>,
        failures: Mutex<BTreeMap<String, String>>,
        update_port_failures: Arc<Mutex<BTreeSet<String>>>,
        display_failures: Arc<Mutex<BTreeSet<DeviceId>>>,
        hellos: Mutex<BTreeMap<String, HelloCapabilities>>,
        product_definitions: Mutex<BTreeMap<String, ProductDefinition>>,
        stopped: Arc<Mutex<Vec<DeviceId>>>,
        joined: Arc<Mutex<Vec<DeviceId>>>,
        commands: Arc<Mutex<BTreeMap<DeviceId, Vec<WorkerCommand>>>>,
        renderers: Mutex<Option<Arc<RendererRegistry>>>,
    }

    impl FakeLauncher {
        fn fail_port(&self, port: &str, error: &str) {
            self.failures
                .lock()
                .unwrap()
                .insert(port.into(), error.into());
        }

        fn fail_update_port(&self, port: &str) {
            self.update_port_failures
                .lock()
                .unwrap()
                .insert(port.into());
        }

        fn fail_display(&self, device_id: &DeviceId) {
            self.display_failures
                .lock()
                .unwrap()
                .insert(device_id.clone());
        }

        fn starts(&self) -> Vec<WorkerStart> {
            self.starts.lock().unwrap().clone()
        }

        fn set_hello(&self, port: &str, hello: HelloCapabilities) {
            self.hellos.lock().unwrap().insert(port.into(), hello);
        }

        fn set_product_definition(&self, port: &str, definition: ProductDefinition) {
            self.product_definitions
                .lock()
                .unwrap()
                .insert(port.into(), definition);
        }

        fn sequences(&self) -> BTreeSet<u64> {
            self.commands
                .lock()
                .unwrap()
                .values()
                .flatten()
                .filter_map(|command| match command {
                    WorkerCommand::Input {
                        receive_sequence, ..
                    } => Some(*receive_sequence),
                    _ => None,
                })
                .collect()
        }

        fn clear_commands(&self) {
            self.commands.lock().unwrap().clear();
        }

        fn commands_for(&self, device_id: &DeviceId) -> Vec<WorkerCommand> {
            self.commands
                .lock()
                .unwrap()
                .get(device_id)
                .cloned()
                .unwrap_or_default()
        }

        fn renderers(&self) -> Option<Arc<RendererRegistry>> {
            self.renderers.lock().unwrap().clone()
        }
    }

    struct FakeWorker {
        device_id: DeviceId,
        stopped: Arc<Mutex<Vec<DeviceId>>>,
        joined: Arc<Mutex<Vec<DeviceId>>>,
        commands: Arc<Mutex<BTreeMap<DeviceId, Vec<WorkerCommand>>>>,
        update_port_failures: Arc<Mutex<BTreeSet<String>>>,
        display_failures: Arc<Mutex<BTreeSet<DeviceId>>>,
    }

    impl DeviceWorker for FakeWorker {
        fn send(&self, command: WorkerCommand) -> Result<(), String> {
            if let WorkerCommand::UpdatePort(port) = &command
                && self.update_port_failures.lock().unwrap().contains(port)
            {
                return Err("device_worker_stopped".into());
            }
            if matches!(command, WorkerCommand::UpdateDisplay(_))
                && self
                    .display_failures
                    .lock()
                    .unwrap()
                    .contains(&self.device_id)
            {
                return Err("device_worker_stopped".into());
            }
            self.commands
                .lock()
                .unwrap()
                .entry(self.device_id.clone())
                .or_default()
                .push(command);
            Ok(())
        }

        fn stop(&mut self) {
            self.stopped.lock().unwrap().push(self.device_id.clone());
        }

        fn join(&mut self) {
            self.joined.lock().unwrap().push(self.device_id.clone());
        }
    }

    impl WorkerLauncher for FakeLauncher {
        fn start(
            &self,
            start: WorkerStart,
            events: Sender<WorkerEvent>,
        ) -> Result<Box<dyn DeviceWorker>, String> {
            self.starts.lock().unwrap().push(start.clone());
            if let Some(error) = self.failures.lock().unwrap().get(&start.port).cloned() {
                return Err(error);
            }
            let capabilities = self
                .hellos
                .lock()
                .unwrap()
                .remove(&start.port)
                .unwrap_or_else(|| hello_for(&start));
            let product_definition = self.product_definitions.lock().unwrap().remove(&start.port);
            events
                .send(WorkerEvent::HelloValidated {
                    generation: start.generation,
                    device_id: start.device_id.clone(),
                    capabilities,
                    product_definition,
                })
                .unwrap();
            Ok(Box::new(FakeWorker {
                device_id: start.device_id,
                stopped: Arc::clone(&self.stopped),
                joined: Arc::clone(&self.joined),
                commands: Arc::clone(&self.commands),
                update_port_failures: Arc::clone(&self.update_port_failures),
                display_failures: Arc::clone(&self.display_failures),
            }))
        }

        fn start_with_renderers(
            &self,
            start: WorkerStart,
            events: Sender<WorkerEvent>,
            renderers: WorkerRendererRegistry,
        ) -> Result<Box<dyn DeviceWorker>, String> {
            *self.renderers.lock().unwrap() = Some(renderers.into_inner());
            self.start(start, events)
        }
    }

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "kivo-coordinator-{}-{}",
                std::process::id(),
                TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn profile() -> DeviceProfile {
        DeviceProfile {
            schema_version: PROFILE_SCHEMA_VERSION,
            profile: crate::profile::test_model_layout(),
            snapshot_metadata: None,
            trigger_settings: TriggerSettings::default(),
            hardware_profiles: vec![HardwareProfile {
                id: "esp".into(),
                name: "ESP".into(),
                board_profile_id: crate::hardware::YD_ESP32_S3_BOARD_ID.into(),
                debounce_ms: 30,
                ssd1306: None,
                sh1106: None,
                inputs: Vec::new(),
            }],
            actions: BTreeMap::new(),
        }
    }

    fn product_definition() -> ProductDefinition {
        let layout = crate::model::ModelLayout {
            id: "key-rp-k1".into(),
            name: "Kivo Key 1".into(),
            groups: vec![crate::model::ButtonGroup {
                id: "keys".into(),
                columns: 1,
                buttons: vec![crate::model::ButtonDefinition {
                    id: "K1".into(),
                    label: "K1".into(),
                }],
            }],
        };
        ProductDefinition {
            schema_version: PRODUCT_DEFINITION_SCHEMA_VERSION,
            product: ProductIdentity {
                display_name: "Kivo Key 1".into(),
                family_id: "key".into(),
                variant_id: "key-rp-k1".into(),
                hardware_revision: 1,
                product_version_id: "key-rp-k1-r01".into(),
                capabilities: Vec::new(),
            },
            layout,
            hardware_profile: HardwareProfile {
                id: "hardware".into(),
                name: "Hardware".into(),
                board_profile_id: crate::hardware::YD_RP2040_BOARD_ID.into(),
                debounce_ms: 30,
                ssd1306: None,
                sh1106: None,
                inputs: vec![InputSource::Direct {
                    id: "direct".into(),
                    keys: BTreeMap::from([("K1".into(), 0)]),
                }],
            },
        }
    }

    fn harness() -> (
        TestDirectory,
        Arc<FakeEnumerator>,
        Arc<FakeLauncher>,
        RuntimeCoordinator,
    ) {
        let directory = TestDirectory::new();
        let workspace = Workspace::create(&directory.0, vec![profile()]).unwrap();
        let enumerator = Arc::new(FakeEnumerator::default());
        let launcher = Arc::new(FakeLauncher::default());
        let coordinator = RuntimeCoordinator::new(
            enumerator.clone(),
            launcher.clone(),
            Arc::new(std::sync::RwLock::new(workspace)),
        );
        (directory, enumerator, launcher, coordinator)
    }

    fn serial(port: &str, vid: u16, pid: u16, serial: Option<&str>) -> SerialObservation {
        SerialObservation {
            port: port.into(),
            vid,
            pid,
            serial_number: serial.map(str::to_owned),
        }
    }

    fn boot(location: &str, serial: &str) -> BootloaderObservation {
        BootloaderObservation {
            location: location.into(),
            vid: 0x2e8a,
            pid: 0x0003,
            serial_number: Some(serial.into()),
        }
    }

    fn hello_for(start: &WorkerStart) -> HelloCapabilities {
        let board = crate::hardware::board_by_id(start.device_id.board_profile_id()).unwrap();
        HelloCapabilities {
            protocol: 4,
            controller_family_id: board.family_id.into(),
            board_profile_id: board.id.into(),
            firmware_build_id: "test-build".into(),
            product_version_id: None,
            pins: board.safe_pins.to_vec(),
        }
    }

    fn scan(coordinator: &mut RuntimeCoordinator) {
        coordinator.scan_once().unwrap();
        coordinator.drain_worker_events();
    }

    #[derive(Debug, Eq, PartialEq)]
    struct RegistryFlowShape {
        status_keys: Vec<String>,
        assignment_keys: Vec<String>,
        metric_keys: Vec<String>,
        command_kind: &'static str,
        command_has_snapshot: bool,
    }

    fn object_keys(value: &serde_json::Value) -> Vec<String> {
        value
            .as_object()
            .unwrap()
            .keys()
            .map(String::from)
            .collect()
    }

    fn exercise_registry_board(board: &'static BoardProfile) -> RegistryFlowShape {
        let registry = test_registry();
        let directory = TestDirectory::new();
        let mut workspace = Workspace::create(&directory.0, vec![profile()]).unwrap();
        let hardware = &mut workspace
            .profiles
            .get_mut("red-phone-v1")
            .unwrap()
            .hardware_profiles[0];
        hardware.id = "fixture-hardware".into();
        hardware.name = "Fixture hardware".into();
        hardware.board_profile_id = board.id.into();
        hardware.inputs = vec![InputSource::Direct {
            id: "fixture-input".into(),
            keys: BTreeMap::from([("UP".into(), 6)]),
        }];
        let enumerator = Arc::new(FakeEnumerator::default());
        let launcher = Arc::new(FakeLauncher::default());
        let port = format!("/dev/{}", board.id);
        launcher.set_hello(
            &port,
            HelloCapabilities {
                protocol: 4,
                controller_family_id: board.family_id.into(),
                board_profile_id: board.id.into(),
                firmware_build_id: "fixture-build".into(),
                product_version_id: None,
                pins: board.safe_pins.to_vec(),
            },
        );
        let mut coordinator = RuntimeCoordinator::new_with_registry(
            enumerator.clone(),
            launcher.clone(),
            Arc::new(RwLock::new(workspace)),
            registry,
        );
        enumerator.set(
            vec![serial(
                &port,
                board.runtime_usb.vid,
                board.runtime_usb.pid,
                Some("FIXTURE-SERIAL"),
            )],
            Vec::new(),
        );

        scan(&mut coordinator);

        let device_id = registry.device_id(board.id, "FIXTURE-SERIAL").unwrap();
        assert_eq!(launcher.starts()[0].device_id, device_id);
        assert_eq!(launcher.starts()[0].board_profile_id, board.id);
        launcher.clear_commands();
        let assignment = RuntimeAssignment {
            device_profile_id: "red-phone-v1".into(),
            hardware_profile_id: "fixture-hardware".into(),
        };
        coordinator
            .workspace
            .write()
            .unwrap()
            .set_assignment(&device_id, assignment.clone())
            .unwrap();
        coordinator.sync_profiles();

        let commands = launcher.commands_for(&device_id);
        let [
            WorkerCommand::Reconfigure {
                snapshot: Some(snapshot),
                revision,
            },
        ] = commands.as_slice()
        else {
            panic!("unexpected registry fixture commands: {commands:?}");
        };
        assert!(*revision > 0);
        assert_eq!(snapshot.metric_attribution.device_id, device_id);
        assert_eq!(snapshot.hardware_profile_id, "fixture-hardware");
        coordinator.handle_worker_event(WorkerEvent::Activity {
            generation: 1,
            device_id: device_id.clone(),
            context: RuntimeEventContext::from_snapshot(1_722_355_200_000, Some(snapshot))
                .with_port(&port),
            activity: RuntimeActivity::new("topology_active"),
        });

        let status = coordinator
            .devices()
            .into_iter()
            .find(|status| status.device_id == device_id)
            .unwrap();
        assert_eq!(status.runtime, RuntimeDimension::Ready);
        assert_eq!(status.controller_family_id, board.family_id);
        assert_eq!(status.board_profile_id, board.id);
        assert_eq!(status.runtime_assignment.as_ref(), Some(&assignment));
        let status = serde_json::to_value(status).unwrap();
        let assignment = serde_json::to_value(assignment).unwrap();
        let metric = serde_json::to_value(&snapshot.metric_attribution).unwrap();

        RegistryFlowShape {
            status_keys: object_keys(&status),
            assignment_keys: object_keys(&assignment),
            metric_keys: object_keys(&metric),
            command_kind: "reconfigure",
            command_has_snapshot: true,
        }
    }

    fn input_event(device_id: DeviceId, event_id: u64, timestamp_ms: u64) -> WorkerEvent {
        WorkerEvent::Input {
            generation: 1,
            device_id,
            captured: CapturedInput {
                context: RuntimeEventContext::unassigned(timestamp_ms),
                runtime_profile: None,
                monotonic_ms: timestamp_ms,
                event_id,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
        }
    }

    #[derive(Default)]
    struct FakeClipboard;

    impl ClipboardWriter for FakeClipboard {
        fn write(&self, _text: &str) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn starts_four_independent_workers_and_enrolls_each_valid_runtime_once() {
        let (_directory, enumerator, launcher, mut coordinator) = harness();
        enumerator.set(
            vec![
                serial("/dev/esp-a", 0x303a, 0x4002, Some("ESP-A")),
                serial("/dev/esp-b", 0x303a, 0x4002, Some("ESP-B")),
                serial("/dev/rp-a", 0x2e8a, 0x102e, Some("RP-A")),
                serial("/dev/rp-b", 0x2e8a, 0x102e, Some("RP-B")),
            ],
            Vec::new(),
        );

        scan(&mut coordinator);
        scan(&mut coordinator);

        assert_eq!(launcher.starts().len(), 4);
        assert_eq!(coordinator.devices().len(), 4);
        assert!(coordinator.devices().iter().all(|status| {
            status.connection == ConnectionDimension::Online
                && status.mode == Some(DeviceMode::Runtime)
                && status.identity == IdentityDimension::Valid
                && status.assignment == AssignmentDimension::Unassigned
        }));
    }

    #[test]
    fn display_snapshot_fans_out_without_mutating_worker_profile_revisions() {
        let (_directory, enumerator, launcher, mut coordinator) = harness();
        enumerator.set(
            vec![
                serial("/dev/esp-a", 0x303a, 0x4002, Some("ESP-A")),
                serial("/dev/esp-b", 0x303a, 0x4002, Some("ESP-B")),
            ],
            Vec::new(),
        );
        scan(&mut coordinator);
        launcher.clear_commands();
        let revisions = coordinator
            .workers
            .iter()
            .map(|(id, slot)| (id.clone(), slot.firmware_revision))
            .collect::<BTreeMap<_, _>>();
        let snapshot = Arc::new(DisplaySnapshot {
            items: Vec::new(),
            health: BTreeMap::new(),
        });

        coordinator.update_display(Arc::clone(&snapshot));

        assert_eq!(coordinator.workers.len(), 2);
        for id in coordinator.workers.keys() {
            assert!(matches!(
                launcher.commands_for(id).as_slice(),
                [WorkerCommand::UpdateDisplay(actual)] if Arc::ptr_eq(actual, &snapshot)
            ));
        }
        assert_eq!(
            coordinator
                .workers
                .iter()
                .map(|(id, slot)| (id.clone(), slot.firmware_revision))
                .collect::<BTreeMap<_, _>>(),
            revisions
        );
    }

    #[test]
    fn display_snapshot_received_before_hotplug_is_replayed_to_the_new_worker() {
        let (_directory, enumerator, launcher, mut coordinator) = harness();
        let snapshot = Arc::new(DisplaySnapshot {
            items: Vec::new(),
            health: BTreeMap::from([("codex".into(), SourceHealth::Offline)]),
        });
        coordinator.update_display(Arc::clone(&snapshot));
        enumerator.set(
            vec![serial("/dev/rp", 0x2e8a, 0x102e, Some("HOTPLUG"))],
            Vec::new(),
        );

        scan(&mut coordinator);

        let id = DeviceId::new(crate::hardware::YD_RP2040_BOARD_ID, "HOTPLUG").unwrap();
        assert!(matches!(
            launcher.commands_for(&id).first(),
            Some(WorkerCommand::UpdateDisplay(actual)) if Arc::ptr_eq(actual, &snapshot)
        ));
    }

    #[test]
    fn usage_snapshot_received_before_hotplug_is_replayed_to_the_new_worker() {
        let (_directory, enumerator, launcher, mut coordinator) = harness();
        let snapshot = Arc::new(UsageSnapshot {
            state: crate::usage::UsageState::Ready,
            has_data: true,
            cost_micros: 12_345_678,
            today_tokens: 1_234_567,
            tpm: 98_765,
            updated_at_ms: Some(1_788_224_400_000),
        });
        coordinator.update_usage(Arc::clone(&snapshot));
        enumerator.set(
            vec![serial("/dev/rp", 0x2e8a, 0x102e, Some("USAGE-HOTPLUG"))],
            Vec::new(),
        );

        scan(&mut coordinator);

        let id = DeviceId::new(crate::hardware::YD_RP2040_BOARD_ID, "USAGE-HOTPLUG").unwrap();
        assert!(matches!(
            launcher.commands_for(&id).first(),
            Some(WorkerCommand::UpdateUsage(actual)) if Arc::ptr_eq(actual, &snapshot)
        ));
    }

    #[test]
    fn restarted_worker_receives_only_the_latest_retained_display_snapshot() {
        let (_directory, enumerator, launcher, mut coordinator) = harness();
        enumerator.set(
            vec![serial("/dev/rp", 0x2e8a, 0x102e, Some("RESTART"))],
            Vec::new(),
        );
        scan(&mut coordinator);
        let stale = Arc::new(DisplaySnapshot {
            items: Vec::new(),
            health: BTreeMap::from([("codex".into(), SourceHealth::Healthy)]),
        });
        let latest = Arc::new(DisplaySnapshot {
            items: Vec::new(),
            health: BTreeMap::from([("codex".into(), SourceHealth::Offline)]),
        });
        coordinator.update_display(stale);
        coordinator.update_display(Arc::clone(&latest));
        enumerator.set(Vec::new(), Vec::new());
        scan(&mut coordinator);
        launcher.clear_commands();

        enumerator.set(
            vec![serial("/dev/rp", 0x2e8a, 0x102e, Some("RESTART"))],
            Vec::new(),
        );
        scan(&mut coordinator);

        let id = DeviceId::new(crate::hardware::YD_RP2040_BOARD_ID, "RESTART").unwrap();
        assert!(matches!(
            launcher.commands_for(&id).first(),
            Some(WorkerCommand::UpdateDisplay(actual)) if Arc::ptr_eq(actual, &latest)
        ));
    }

    #[test]
    fn failed_initial_display_replay_does_not_insert_the_worker() {
        let (_directory, enumerator, launcher, mut coordinator) = harness();
        let id = DeviceId::new(crate::hardware::YD_RP2040_BOARD_ID, "DISPLAY-FAIL").unwrap();
        coordinator.update_display(Arc::new(DisplaySnapshot {
            items: Vec::new(),
            health: BTreeMap::new(),
        }));
        launcher.fail_display(&id);
        enumerator.set(
            vec![serial("/dev/rp", 0x2e8a, 0x102e, Some("DISPLAY-FAIL"))],
            Vec::new(),
        );

        scan(&mut coordinator);

        assert!(!coordinator.workers.contains_key(&id));
        assert_eq!(*launcher.stopped.lock().unwrap(), vec![id.clone()]);
        assert_eq!(*launcher.joined.lock().unwrap(), vec![id.clone()]);
        assert!(coordinator.candidates().iter().any(|candidate| {
            candidate.device_id.as_ref() == Some(&id)
                && candidate.latest_error.as_deref() == Some("device_worker_stopped")
        }));
    }

    #[test]
    fn injected_renderer_registry_is_shared_with_spawned_workers() {
        let directory = TestDirectory::new();
        let workspace = Workspace::create(&directory.0, vec![profile()]).unwrap();
        let enumerator = Arc::new(FakeEnumerator::default());
        let launcher = Arc::new(FakeLauncher::default());
        let renderers = Arc::new(built_in_renderer_registry());
        let mut coordinator = RuntimeCoordinator::with_paste_and_renderers(
            enumerator.clone(),
            launcher.clone(),
            Arc::new(RwLock::new(workspace)),
            None,
            Arc::clone(&renderers),
        );
        enumerator.set(
            vec![serial("/dev/rp", 0x2e8a, 0x102e, Some("DISPLAY"))],
            Vec::new(),
        );

        scan(&mut coordinator);

        assert!(Arc::ptr_eq(&launcher.renderers().unwrap(), &renderers));
    }

    #[test]
    fn validated_hello_uses_one_worker_revision_for_clear_or_persisted_assignment() {
        let directory = TestDirectory::new();
        let mut workspace = Workspace::create(&directory.0, vec![profile()]).unwrap();
        let unassigned =
            DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "UNASSIGNED").unwrap();
        let assigned = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "ASSIGNED").unwrap();
        workspace.enroll_device(unassigned.clone()).unwrap();
        workspace.enroll_device(assigned.clone()).unwrap();
        workspace
            .set_assignment(
                &assigned,
                RuntimeAssignment {
                    device_profile_id: "red-phone-v1".into(),
                    hardware_profile_id: "esp".into(),
                },
            )
            .unwrap();
        let enumerator = Arc::new(FakeEnumerator::default());
        let launcher = Arc::new(FakeLauncher::default());
        let mut coordinator = RuntimeCoordinator::new(
            enumerator.clone(),
            launcher.clone(),
            Arc::new(RwLock::new(workspace)),
        );
        enumerator.set(
            vec![
                serial("/dev/unassigned", 0x303a, 0x4002, Some("UNASSIGNED")),
                serial("/dev/assigned", 0x303a, 0x4002, Some("ASSIGNED")),
            ],
            Vec::new(),
        );

        scan(&mut coordinator);

        assert!(matches!(
            launcher.commands_for(&unassigned).as_slice(),
            [WorkerCommand::Reconfigure {
                snapshot: None,
                revision: 1,
            }]
        ));
        assert!(matches!(
            launcher.commands_for(&assigned).as_slice(),
            [WorkerCommand::Reconfigure {
                snapshot: Some(_),
                revision: 1,
            }]
        ));
    }

    #[test]
    fn product_device_configures_without_a_runtime_assignment() {
        let (_directory, enumerator, launcher, mut coordinator) = harness();
        let port = "/dev/product";
        let serial_number = "PRODUCT001";
        let id = DeviceId::new(crate::hardware::YD_RP2040_BOARD_ID, serial_number).unwrap();
        let definition = product_definition();
        launcher.set_hello(
            port,
            HelloCapabilities {
                protocol: 9,
                controller_family_id: "rp2040".into(),
                board_profile_id: crate::hardware::YD_RP2040_BOARD_ID.into(),
                firmware_build_id: "product-build".into(),
                product_version_id: Some("key-rp-k1-r01".into()),
                pins: crate::hardware::board_by_id(crate::hardware::YD_RP2040_BOARD_ID)
                    .unwrap()
                    .safe_pins
                    .to_vec(),
            },
        );
        launcher.set_product_definition(port, definition);
        enumerator.set(
            vec![serial(port, 0x2e8a, 0x102e, Some(serial_number))],
            Vec::new(),
        );

        scan(&mut coordinator);

        let status = coordinator
            .devices()
            .into_iter()
            .find(|status| status.device_id == id)
            .unwrap();
        assert_eq!(status.product_version_id.as_deref(), Some("key-rp-k1-r01"));
        assert!(status.product_definition.is_some());
        assert!(status.product_config.is_some());
        let record = coordinator
            .workspace
            .read()
            .unwrap()
            .settings
            .devices
            .get(&id)
            .cloned()
            .unwrap();
        assert!(record.runtime_assignment.is_none());
        assert_eq!(
            record.product_config.unwrap().product_version_id,
            "key-rp-k1-r01"
        );
        assert!(matches!(
            launcher.commands_for(&id).as_slice(),
            [WorkerCommand::Reconfigure {
                snapshot: Some(_),
                revision: 1,
            }]
        ));
    }

    #[test]
    fn second_rp2040_board_traverses_injected_registry_domain_flow() {
        assert_eq!(
            exercise_registry_board(
                test_registry()
                    .board_by_id(TEST_SECOND_RP2040_BOARD_ID)
                    .unwrap(),
            ),
            exercise_registry_board(&BOARD_PROFILES[1])
        );
    }

    #[test]
    fn esp32c3_board_traverses_injected_registry_domain_flow() {
        assert_eq!(
            exercise_registry_board(test_registry().board_by_id(TEST_ESP32C3_BOARD_ID).unwrap()),
            exercise_registry_board(&BOARD_PROFILES[0])
        );
    }

    #[test]
    fn queued_input_keeps_serial_receipt_attribution_and_action_mapping() {
        let (_directory, enumerator, launcher, mut coordinator) = harness();
        enumerator.set(
            vec![serial("/dev/esp-event", 0x303a, 0x4002, Some("EVENT-A"))],
            Vec::new(),
        );
        scan(&mut coordinator);
        let device_id = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "EVENT-A").unwrap();
        let mut old_profile = profile();
        old_profile.hardware_profiles[0].inputs = vec![InputSource::Direct {
            id: "buttons".into(),
            keys: BTreeMap::from([("UP".into(), 6)]),
        }];
        old_profile.actions.insert(
            "UP".into(),
            TriggerActions::press(vec![ButtonAction::Paste {
                text: "old action".into(),
            }]),
        );
        let mut new_profile = old_profile.clone();
        new_profile.profile.id = "new-profile".into();
        new_profile.profile.name = "New profile".into();
        new_profile.actions.insert(
            "UP".into(),
            TriggerActions::press(vec![ButtonAction::Paste {
                text: "new action".into(),
            }]),
        );
        {
            let mut workspace = coordinator.workspace.write().unwrap();
            workspace.save_profile(old_profile).unwrap();
            workspace.save_profile(new_profile).unwrap();
            workspace
                .set_assignment(
                    &device_id,
                    RuntimeAssignment {
                        device_profile_id: "red-phone-v1".into(),
                        hardware_profile_id: "esp".into(),
                    },
                )
                .unwrap();
        }
        coordinator.sync_profiles();
        launcher.clear_commands();

        let generation = launcher.starts()[0].generation;
        let old_snapshot =
            Arc::new(runtime_profile(&coordinator.workspace_revision, &device_id, None).unwrap());
        let board = crate::hardware::board_by_id(device_id.board_profile_id()).unwrap();
        let mut session = DeviceSession::new((*old_snapshot).clone());
        session.on_message_deferred(
            DeviceMessage::Hello(HelloCapabilities {
                protocol: 4,
                controller_family_id: board.family_id.into(),
                board_profile_id: board.id.into(),
                firmware_build_id: "test".into(),
                product_version_id: None,
                pins: board.safe_pins.to_vec(),
            }),
            0,
            1,
        );
        session.on_message_deferred(DeviceMessage::ConfigOk { revision: 1 }, 0, 2);
        let current_context = RuntimeEventContext::from_snapshot(0, Some(old_snapshot.as_ref()))
            .with_port("/dev/esp-event");
        let captured = session.capture_input(
            &current_context,
            1_720_086_400_123,
            7,
            PhysicalInput::Direct { gpio: 6 },
            InputState::Down,
        );
        coordinator
            .event_sender
            .send(WorkerEvent::Input {
                generation,
                device_id: device_id.clone(),
                captured: captured.clone(),
            })
            .unwrap();

        {
            let mut workspace = coordinator.workspace.write().unwrap();
            workspace
                .set_assignment(
                    &device_id,
                    RuntimeAssignment {
                        device_profile_id: "new-profile".into(),
                        hardware_profile_id: "esp".into(),
                    },
                )
                .unwrap();
            let revision = WorkspaceRevision::capture(&workspace);
            drop(workspace);
            coordinator.apply_workspace_revision(revision);
        }
        coordinator.devices.get_mut(&device_id).unwrap().port = Some("/dev/changed".into());
        coordinator.workers.get_mut(&device_id).unwrap().port = "/dev/changed".into();
        assert!(coordinator.drain_worker_events().is_empty());

        let commands = launcher.commands_for(&device_id);
        let [
            WorkerCommand::Reconfigure {
                snapshot: Some(new_snapshot),
                revision: new_revision,
            },
            WorkerCommand::Input {
                receive_sequence,
                captured: forwarded,
            },
        ] = commands.as_slice()
        else {
            panic!("expected reconfiguration to overtake the queued input: {commands:?}");
        };
        assert_eq!(forwarded, &captured);
        assert_eq!(new_snapshot.profile.profile.id, "new-profile");
        assert_eq!(
            forwarded.runtime_profile.as_ref().unwrap().profile.actions["UP"].press[0],
            ButtonAction::Paste {
                text: "old action".into()
            }
        );

        session.reconfigure(Some(new_snapshot.clone()), *new_revision);
        let input_output = session.on_captured_input(forwarded, *receive_sequence);
        assert!(input_output.paste_requests.is_empty());
        let configured = session.on_message_deferred(
            DeviceMessage::ConfigOk {
                revision: *new_revision,
            },
            0,
            3,
        );
        assert_eq!(configured.paste_requests[0].text, "old action");

        let activity = input_output
            .activities
            .into_iter()
            .find(|activity| activity.code == "input_state")
            .unwrap();
        let event = coordinator
            .handle_worker_event(WorkerEvent::Activity {
                generation,
                device_id: device_id.clone(),
                context: forwarded.context.clone(),
                activity,
            })
            .unwrap();
        let value = serde_json::to_value(&event).unwrap();

        assert_eq!(event.timestamp_ms, 1_720_086_400_123);
        assert_eq!(event.device_id, device_id);
        assert_eq!(event.raw_serial, "EVENT-A");
        assert_eq!(
            event.controller_family_id,
            crate::hardware::ESP32S3_FAMILY_ID
        );
        assert_eq!(
            event.board_profile_id,
            crate::hardware::YD_ESP32_S3_BOARD_ID
        );
        assert_eq!(event.port.as_deref(), Some("/dev/esp-event"));
        assert_eq!(event.device_profile_id.as_deref(), Some("red-phone-v1"));
        assert_eq!(event.hardware_profile_id.as_deref(), Some("esp"));
        assert_eq!(value["level"], "info");
        assert_eq!(value["code"], "input_state");
        assert!(value["homeUpdate"].is_null());
    }

    #[test]
    fn port_rename_reuses_worker_and_one_departure_stops_only_that_worker() {
        let (_directory, enumerator, launcher, mut coordinator) = harness();
        enumerator.set(
            vec![
                serial("/dev/a", 0x303a, 0x4002, Some("ESP-A")),
                serial("/dev/b", 0x303a, 0x4002, Some("ESP-B")),
            ],
            Vec::new(),
        );
        scan(&mut coordinator);
        enumerator.set(
            vec![
                serial("/dev/a-renamed", 0x303a, 0x4002, Some("ESP-A")),
                serial("/dev/b", 0x303a, 0x4002, Some("ESP-B")),
            ],
            Vec::new(),
        );
        scan(&mut coordinator);
        assert_eq!(launcher.starts().len(), 2);
        assert_eq!(
            coordinator
                .devices()
                .iter()
                .find(|status| status.raw_serial == "ESP-A")
                .unwrap()
                .port
                .as_deref(),
            Some("/dev/a-renamed")
        );

        enumerator.set(
            vec![serial("/dev/b", 0x303a, 0x4002, Some("ESP-B"))],
            Vec::new(),
        );
        scan(&mut coordinator);

        assert_eq!(launcher.stopped.lock().unwrap().len(), 1);
        let departed = coordinator
            .devices()
            .into_iter()
            .find(|status| status.raw_serial == "ESP-A")
            .unwrap();
        assert_eq!(departed.connection, ConnectionDimension::Offline);
        assert_eq!(departed.mode, None);
        assert_eq!(
            coordinator
                .devices()
                .iter()
                .find(|status| status.raw_serial == "ESP-B")
                .unwrap()
                .connection,
            ConnectionDimension::Online
        );
    }

    #[test]
    fn rediscovered_worker_captures_renamed_port_for_next_physical_input() {
        let (_directory, enumerator, launcher, mut coordinator) = harness();
        enumerator.set(
            vec![serial("/dev/old", 0x303a, 0x4002, Some("PORT-A"))],
            Vec::new(),
        );
        scan(&mut coordinator);
        let device_id = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "PORT-A").unwrap();
        {
            let mut workspace = coordinator.workspace.write().unwrap();
            workspace
                .set_assignment(
                    &device_id,
                    RuntimeAssignment {
                        device_profile_id: "red-phone-v1".into(),
                        hardware_profile_id: "esp".into(),
                    },
                )
                .unwrap();
        }
        coordinator.sync_profiles();
        launcher.clear_commands();

        let snapshot =
            Arc::new(runtime_profile(&coordinator.workspace_revision, &device_id, None).unwrap());
        let mut session = DeviceSession::new((*snapshot).clone());
        let board = crate::hardware::board_by_id(device_id.board_profile_id()).unwrap();
        session.on_message_deferred(
            DeviceMessage::Hello(HelloCapabilities {
                protocol: 4,
                controller_family_id: board.family_id.into(),
                board_profile_id: board.id.into(),
                firmware_build_id: "test".into(),
                product_version_id: None,
                pins: board.safe_pins.to_vec(),
            }),
            0,
            1,
        );
        session.on_message_deferred(DeviceMessage::ConfigOk { revision: 1 }, 0, 2);
        let mut worker_port = "/dev/old".to_owned();
        let mut worker_context =
            RuntimeEventContext::from_snapshot(0, Some(snapshot.as_ref())).with_port(&worker_port);

        enumerator.set(
            vec![serial("/dev/renamed", 0x303a, 0x4002, Some("PORT-A"))],
            Vec::new(),
        );
        scan(&mut coordinator);
        let commands = launcher.commands_for(&device_id);
        assert_eq!(commands.len(), 1);
        crate::device::apply_worker_context_update(
            &mut worker_port,
            &mut worker_context,
            &commands[0],
        );

        let captured = session.capture_input(
            &worker_context,
            1_720_086_400_321,
            8,
            PhysicalInput::Direct { gpio: 6 },
            InputState::Down,
        );
        coordinator
            .event_sender
            .send(WorkerEvent::Input {
                generation: coordinator.generation,
                device_id: device_id.clone(),
                captured,
            })
            .unwrap();
        assert!(coordinator.drain_worker_events().is_empty());
        let forwarded = launcher
            .commands_for(&device_id)
            .into_iter()
            .find_map(|command| match command {
                WorkerCommand::Input {
                    receive_sequence,
                    captured,
                } => Some((receive_sequence, captured)),
                _ => None,
            })
            .unwrap();
        let output = session.on_captured_input(&forwarded.1, forwarded.0);
        crate::device::emit_worker_activities_for_test(
            &launcher.starts()[0],
            &coordinator.event_sender,
            output,
            &forwarded.1.context,
        );
        let events = coordinator.drain_worker_events();

        let event = events
            .into_iter()
            .find(|event| event.activity.code == "input_state")
            .unwrap();
        assert_eq!(event.device_id, device_id);
        assert_eq!(event.port.as_deref(), Some("/dev/renamed"));
        assert_eq!(event.device_profile_id.as_deref(), Some("red-phone-v1"));
        assert_eq!(event.hardware_profile_id.as_deref(), Some("esp"));
        assert_eq!(launcher.starts().len(), 1);
    }

    #[test]
    fn failed_port_update_retires_worker_and_restarts_on_renamed_port() {
        let (_directory, enumerator, launcher, mut coordinator) = harness();
        enumerator.set(
            vec![serial("/dev/old", 0x303a, 0x4002, Some("PORT-FAIL"))],
            Vec::new(),
        );
        scan(&mut coordinator);
        launcher.fail_update_port("/dev/renamed");

        enumerator.set(
            vec![serial("/dev/renamed", 0x303a, 0x4002, Some("PORT-FAIL"))],
            Vec::new(),
        );
        scan(&mut coordinator);

        let failed = coordinator
            .devices()
            .into_iter()
            .find(|status| status.raw_serial == "PORT-FAIL")
            .unwrap();
        assert_eq!(failed.connection, ConnectionDimension::Offline);
        assert_eq!(failed.port, None);
        assert_eq!(failed.runtime, RuntimeDimension::Inactive);
        assert_eq!(
            failed
                .latest_error
                .as_ref()
                .map(|error| error.code.as_str()),
            Some("device_worker_stopped")
        );
        assert_eq!(launcher.starts().len(), 1);
        assert_eq!(launcher.stopped.lock().unwrap().len(), 1);

        scan(&mut coordinator);

        let starts = launcher.starts();
        assert_eq!(starts.len(), 2);
        assert_eq!(starts[1].port, "/dev/renamed");
        let recovered = coordinator
            .devices()
            .into_iter()
            .find(|status| status.raw_serial == "PORT-FAIL")
            .unwrap();
        assert_eq!(recovered.connection, ConnectionDimension::Online);
        assert_eq!(recovered.port.as_deref(), Some("/dev/renamed"));
    }

    #[test]
    fn missing_and_duplicate_identities_are_quarantined_and_duplicates_stop_owner() {
        let (_directory, enumerator, launcher, mut coordinator) = harness();
        enumerator.set(
            vec![
                serial("/dev/valid", 0x303a, 0x4002, Some("DUP")),
                serial("/dev/missing", 0x2e8a, 0x102e, None),
            ],
            Vec::new(),
        );
        scan(&mut coordinator);
        assert!(coordinator.candidates().iter().any(|candidate| {
            candidate.port.as_deref() == Some("/dev/missing")
                && candidate.identity == IdentityDimension::InvalidIdentity
        }));

        enumerator.set(
            vec![
                serial("/dev/dup-a", 0x303a, 0x4002, Some("DUP")),
                serial("/dev/dup-b", 0x303a, 0x4002, Some("DUP")),
            ],
            Vec::new(),
        );
        scan(&mut coordinator);

        assert_eq!(launcher.stopped.lock().unwrap().len(), 1);
        assert_eq!(
            coordinator
                .candidates()
                .iter()
                .filter(|candidate| candidate.identity == IdentityDimension::DuplicateIdentity)
                .count(),
            2
        );
        assert!(coordinator.devices().iter().all(|status| {
            status.raw_serial != "DUP" || status.identity == IdentityDimension::DuplicateIdentity
        }));
    }

    #[test]
    fn bootloader_reconciles_known_identity_but_unknown_bootloader_stays_candidate() {
        let (_directory, enumerator, _launcher, mut coordinator) = harness();
        enumerator.set(
            vec![serial("/dev/rp", 0x2e8a, 0x102e, Some("KNOWN-RP"))],
            Vec::new(),
        );
        scan(&mut coordinator);
        enumerator.set(
            Vec::new(),
            vec![boot("1-1", "KNOWN-RP"), boot("1-2", "NEW-RP")],
        );
        scan(&mut coordinator);

        let known = coordinator
            .devices()
            .into_iter()
            .find(|status| status.raw_serial == "KNOWN-RP")
            .unwrap();
        assert_eq!(known.connection, ConnectionDimension::Online);
        assert_eq!(known.mode, Some(DeviceMode::Bootloader));
        assert!(coordinator.candidates().iter().any(|candidate| {
            candidate.raw_serial.as_deref() == Some("NEW-RP")
                && candidate.mode == DeviceMode::Bootloader
        }));
        assert!(
            coordinator
                .devices()
                .iter()
                .all(|status| status.raw_serial != "NEW-RP")
        );
    }

    #[test]
    fn simultaneous_runtime_and_bootloader_observations_quarantine_the_complete_identity_group() {
        let (_directory, enumerator, launcher, mut coordinator) = harness();
        enumerator.set(
            vec![serial("/dev/rp", 0x2e8a, 0x102e, Some("TRANSITION"))],
            vec![boot("1-1", "TRANSITION")],
        );

        scan(&mut coordinator);

        assert!(launcher.starts().is_empty());
        assert_eq!(
            coordinator
                .candidates()
                .iter()
                .filter(|candidate| candidate.identity == IdentityDimension::DuplicateIdentity)
                .count(),
            2
        );
    }

    #[test]
    fn usage_polling_tracks_active_views_across_devices_and_disconnects() {
        let (_directory, enumerator, _launcher, mut coordinator) = harness();
        enumerator.set(
            vec![
                serial("/dev/a", 0x303a, 0x4002, Some("A")),
                serial("/dev/b", 0x303a, 0x4002, Some("B")),
            ],
            Vec::new(),
        );
        scan(&mut coordinator);
        let a = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "A").unwrap();
        let b = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "B").unwrap();

        assert!(!coordinator.usage_requested());
        coordinator.handle_worker_event(WorkerEvent::UsageView {
            generation: 1,
            device_id: a.clone(),
            active: true,
        });
        coordinator.handle_worker_event(WorkerEvent::UsageView {
            generation: 1,
            device_id: b.clone(),
            active: true,
        });
        assert!(coordinator.usage_requested());

        coordinator.handle_worker_event(WorkerEvent::UsageView {
            generation: 1,
            device_id: a,
            active: false,
        });
        assert!(coordinator.usage_requested());

        coordinator.handle_worker_event(WorkerEvent::Disconnected {
            generation: 1,
            device_id: b,
            error: None,
        });
        assert!(!coordinator.usage_requested());
    }

    #[test]
    fn stale_disconnect_after_coordinator_stop_does_not_override_bootloader_mode() {
        let (_directory, enumerator, _launcher, mut coordinator) = harness();
        enumerator.set(
            vec![serial("/dev/rp", 0x2e8a, 0x102e, Some("MODE"))],
            Vec::new(),
        );
        scan(&mut coordinator);
        let id = DeviceId::new(crate::hardware::YD_RP2040_BOARD_ID, "MODE").unwrap();

        enumerator.set(Vec::new(), vec![boot("1-1", "MODE")]);
        scan(&mut coordinator);
        coordinator.handle_worker_event(WorkerEvent::Disconnected {
            generation: 1,
            device_id: id,
            error: None,
        });

        let status = coordinator
            .devices()
            .into_iter()
            .find(|status| status.raw_serial == "MODE")
            .unwrap();
        assert_eq!(status.connection, ConnectionDimension::Online);
        assert_eq!(status.mode, Some(DeviceMode::Bootloader));
    }

    #[test]
    fn repeated_worker_disconnect_backs_off_until_the_retry_deadline() {
        let (_directory, enumerator, launcher, mut coordinator) = harness();
        enumerator.set(
            vec![serial(
                "/dev/esp",
                0x303a,
                0x4002,
                Some("RECONNECT-BACKOFF"),
            )],
            Vec::new(),
        );
        scan(&mut coordinator);
        let id = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "RECONNECT-BACKOFF").unwrap();
        {
            let mut workspace = coordinator.workspace.write().unwrap();
            workspace
                .set_assignment(
                    &id,
                    RuntimeAssignment {
                        device_profile_id: "red-phone-v1".into(),
                        hardware_profile_id: "esp".into(),
                    },
                )
                .unwrap();
            let revision = WorkspaceRevision::capture(&workspace);
            drop(workspace);
            coordinator.apply_workspace_revision(revision);
        }

        coordinator.handle_worker_event(WorkerEvent::Disconnected {
            generation: 1,
            device_id: id.clone(),
            error: Some("serial_read_failed".into()),
        });
        scan(&mut coordinator);

        assert_eq!(launcher.starts().len(), 2);

        coordinator.handle_worker_event(WorkerEvent::Disconnected {
            generation: 1,
            device_id: id.clone(),
            error: Some("serial_read_failed".into()),
        });
        scan(&mut coordinator);

        assert_eq!(launcher.starts().len(), 2);

        coordinator
            .reconnect_not_before
            .insert(id, Instant::now() - Duration::from_millis(1));
        scan(&mut coordinator);

        assert_eq!(launcher.starts().len(), 3);
    }

    #[test]
    fn topology_rejection_moves_only_that_device_to_runtime_error() {
        let (_directory, enumerator, _launcher, mut coordinator) = harness();
        enumerator.set(
            vec![
                serial("/dev/a", 0x303a, 0x4002, Some("A")),
                serial("/dev/b", 0x303a, 0x4002, Some("B")),
            ],
            Vec::new(),
        );
        scan(&mut coordinator);
        let rejected = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "A").unwrap();

        coordinator.handle_worker_event(WorkerEvent::Activity {
            generation: 1,
            device_id: rejected,
            context: RuntimeEventContext::unassigned(1),
            activity: RuntimeActivity::new("topology_rejected"),
        });

        let devices = coordinator.devices();
        let rejected = devices
            .iter()
            .find(|status| status.raw_serial == "A")
            .unwrap();
        let unaffected = devices
            .iter()
            .find(|status| status.raw_serial == "B")
            .unwrap();
        assert_eq!(rejected.runtime, RuntimeDimension::RuntimeError);
        assert_eq!(
            rejected
                .latest_error
                .as_ref()
                .map(|error| error.code.as_str()),
            Some("topology_rejected")
        );
        assert_ne!(unaffected.runtime, RuntimeDimension::RuntimeError);
    }

    #[test]
    fn more_than_one_observation_per_mode_quarantines_the_complete_identity_group() {
        let (_directory, enumerator, launcher, mut coordinator) = harness();
        enumerator.set(
            vec![serial("/dev/rp", 0x2e8a, 0x102e, Some("DUP-BOOT"))],
            vec![boot("1-1", "DUP-BOOT"), boot("1-2", "DUP-BOOT")],
        );

        scan(&mut coordinator);

        assert!(launcher.starts().is_empty());
        assert_eq!(
            coordinator
                .candidates()
                .iter()
                .filter(|candidate| candidate.identity == IdentityDimension::DuplicateIdentity)
                .count(),
            3
        );
    }

    #[test]
    fn invalid_hello_never_enrolls_the_usb_identity() {
        let (_directory, enumerator, launcher, mut coordinator) = harness();
        launcher.set_hello(
            "/dev/bad-hello",
            HelloCapabilities {
                protocol: 4,
                controller_family_id: "wrong-family".into(),
                board_profile_id: crate::hardware::YD_ESP32_S3_BOARD_ID.into(),
                firmware_build_id: "bad".into(),
                product_version_id: None,
                pins: vec![0],
            },
        );
        enumerator.set(
            vec![serial(
                "/dev/bad-hello",
                0x303a,
                0x4002,
                Some("NOT-ENROLLED"),
            )],
            Vec::new(),
        );

        scan(&mut coordinator);

        assert!(coordinator.devices().is_empty());
        assert!(
            coordinator
                .workspace
                .read()
                .unwrap()
                .settings
                .devices
                .is_empty()
        );
        assert_eq!(launcher.stopped.lock().unwrap().len(), 1);
    }

    #[test]
    fn stale_hello_from_a_stopped_worker_cannot_enroll() {
        let (_directory, _enumerator, _launcher, mut coordinator) = harness();
        let id = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "STALE").unwrap();
        let board = crate::hardware::board_by_id(id.board_profile_id()).unwrap();

        coordinator.handle_worker_event(WorkerEvent::HelloValidated {
            generation: 1,
            device_id: id,
            capabilities: HelloCapabilities {
                protocol: 4,
                controller_family_id: board.family_id.into(),
                board_profile_id: board.id.into(),
                firmware_build_id: "stale".into(),
                product_version_id: None,
                pins: board.safe_pins.to_vec(),
            },
            product_definition: None,
        });

        assert!(
            coordinator
                .workspace
                .read()
                .unwrap()
                .settings
                .devices
                .is_empty()
        );
    }

    #[test]
    fn disconnect_releases_registered_but_not_yet_submitted_paste_sequence() {
        let directory = TestDirectory::new();
        let workspace = Workspace::create(&directory.0, vec![profile()]).unwrap();
        let enumerator = Arc::new(FakeEnumerator::default());
        let launcher = Arc::new(FakeLauncher::default());
        let paste =
            PasteCoordinator::with_timeout(FakeClipboard, std::time::Duration::from_secs(1));
        let handle = paste.handle();
        let mut coordinator = RuntimeCoordinator::with_paste(
            enumerator.clone(),
            launcher,
            Arc::new(std::sync::RwLock::new(workspace)),
            Some(handle.clone()),
        );
        enumerator.set(
            vec![serial("/dev/a", 0x303a, 0x4002, Some("A"))],
            Vec::new(),
        );
        scan(&mut coordinator);
        let id = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "A").unwrap();
        coordinator.handle_worker_event(input_event(id, 9, 10));

        enumerator.set(Vec::new(), Vec::new());
        scan(&mut coordinator);
        handle.register_sequence(2).unwrap();
        let (reply, replies) = std::sync::mpsc::channel();
        let next_id = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "B").unwrap();
        handle
            .submit(PasteRequest {
                receive_sequence: 2,
                device_id: next_id.clone(),
                event_id: 10,
                step: 1,
                text: "next".into(),
                reply,
            })
            .unwrap();

        assert_eq!(replies.recv().unwrap(), PasteReply::Granted);
        handle.complete(&next_id, 10, 1).unwrap();
        handle.finish_sequence(2).unwrap();
        paste.shutdown();
    }

    #[test]
    fn one_open_failure_is_device_local_and_other_worker_remains_online() {
        let (_directory, enumerator, launcher, mut coordinator) = harness();
        launcher.fail_port("/dev/bad", "handshake failed");
        enumerator.set(
            vec![
                serial("/dev/bad", 0x303a, 0x4002, Some("BAD")),
                serial("/dev/good", 0x2e8a, 0x102e, Some("GOOD")),
            ],
            Vec::new(),
        );

        scan(&mut coordinator);

        assert_eq!(launcher.starts().len(), 2);
        assert!(coordinator.devices().iter().any(|status| {
            status.raw_serial == "GOOD" && status.connection == ConnectionDimension::Online
        }));
        assert!(coordinator.candidates().iter().any(|candidate| {
            candidate.raw_serial.as_deref() == Some("BAD")
                && candidate.latest_error.as_deref() == Some("handshake failed")
        }));
    }

    #[test]
    fn central_receive_sequence_is_monotonic_and_returned_to_the_source_worker() {
        let (_directory, enumerator, launcher, mut coordinator) = harness();
        enumerator.set(
            vec![serial("/dev/a", 0x303a, 0x4002, Some("A"))],
            Vec::new(),
        );
        scan(&mut coordinator);
        let id = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "A").unwrap();

        coordinator.handle_worker_event(input_event(id.clone(), 8, 10));
        coordinator.handle_worker_event(input_event(id, 9, 11));

        assert_eq!(coordinator.last_receive_sequence(), 2);
        assert_eq!(launcher.sequences(), BTreeSet::from([1, 2]));
    }

    #[test]
    fn one_worker_cannot_finish_another_devices_receive_sequence() {
        let (_directory, enumerator, _launcher, mut coordinator) = harness();
        enumerator.set(
            vec![serial("/dev/a", 0x303a, 0x4002, Some("A"))],
            Vec::new(),
        );
        scan(&mut coordinator);
        let owner = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "A").unwrap();
        coordinator.handle_worker_event(input_event(owner.clone(), 8, 10));

        coordinator.handle_worker_event(WorkerEvent::SequenceFinished {
            generation: 1,
            device_id: DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "B").unwrap(),
            receive_sequence: 1,
        });
        assert_eq!(coordinator.sequence_owners.get(&1), Some(&owner));
        coordinator.handle_worker_event(WorkerEvent::SequenceFinished {
            generation: 1,
            device_id: owner,
            receive_sequence: 1,
        });
        assert!(!coordinator.sequence_owners.contains_key(&1));
    }

    #[test]
    fn live_update_action_only_swaps_both_assigned_snapshots_without_topology() {
        let (_directory, enumerator, launcher, mut coordinator) = harness();
        enumerator.set(
            vec![
                serial("/dev/a", 0x303a, 0x4002, Some("A")),
                serial("/dev/b", 0x303a, 0x4002, Some("B")),
            ],
            Vec::new(),
        );
        scan(&mut coordinator);
        let a = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "A").unwrap();
        let b = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "B").unwrap();
        {
            let mut workspace = coordinator.workspace.write().unwrap();
            for id in [&a, &b] {
                workspace
                    .set_assignment(
                        id,
                        RuntimeAssignment {
                            device_profile_id: "red-phone-v1".into(),
                            hardware_profile_id: "esp".into(),
                        },
                    )
                    .unwrap();
            }
        }
        coordinator.sync_profiles();
        launcher.clear_commands();
        let old = coordinator.workspace.read().unwrap().profiles["red-phone-v1"].clone();
        let mut new = old.clone();
        new.actions.insert(
            "UP".into(),
            TriggerActions::press(vec![crate::profile::ButtonAction::Paste {
                text: "updated".into(),
            }]),
        );
        coordinator
            .workspace
            .write()
            .unwrap()
            .save_profile(new.clone())
            .unwrap();

        coordinator.apply_profile_change(&ProfileChange::between(Some(&old), Some(&new)));

        for id in [&a, &b] {
            let commands = launcher.commands_for(id);
            assert!(matches!(
                commands.as_slice(),
                [WorkerCommand::UpdateSnapshot(Some(_))]
            ));
        }
    }

    #[test]
    fn topology_cleared_activity_leaves_the_unassigned_device_inactive() {
        let (_directory, enumerator, launcher, mut coordinator) = harness();
        enumerator.set(
            vec![serial("/dev/a", 0x303a, 0x4002, Some("A"))],
            Vec::new(),
        );
        scan(&mut coordinator);
        let id = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "A").unwrap();
        {
            let mut workspace = coordinator.workspace.write().unwrap();
            workspace
                .set_assignment(
                    &id,
                    RuntimeAssignment {
                        device_profile_id: "red-phone-v1".into(),
                        hardware_profile_id: "esp".into(),
                    },
                )
                .unwrap();
        }
        coordinator.sync_profiles();
        launcher.clear_commands();
        coordinator
            .workspace
            .write()
            .unwrap()
            .clear_assignment(&id)
            .unwrap();
        coordinator.sync_profiles();

        let commands = launcher.commands_for(&id);
        let [
            WorkerCommand::Reconfigure {
                snapshot: None,
                revision: _,
            },
        ] = commands.as_slice()
        else {
            panic!("expected clear reconfiguration: {commands:?}");
        };
        assert_eq!(
            coordinator.devices.get(&id).unwrap().runtime,
            RuntimeDimension::Inactive
        );

        coordinator.devices.get_mut(&id).unwrap().runtime = RuntimeDimension::Ready;
        let event = coordinator
            .handle_worker_event(WorkerEvent::Activity {
                generation: 1,
                device_id: id.clone(),
                context: RuntimeEventContext::unassigned(100),
                activity: RuntimeActivity::new("topology_cleared"),
            })
            .unwrap();

        assert_eq!(event.level, EventLevel::Info);
        assert_eq!(
            coordinator.devices.get(&id).unwrap().runtime,
            RuntimeDimension::Inactive
        );
        assert!(coordinator.devices.get(&id).unwrap().latest_error.is_none());
    }

    #[test]
    fn live_update_topology_targets_exact_hardware_with_independent_nonzero_revisions() {
        let (_directory, enumerator, launcher, mut coordinator) = harness();
        let mut expanded = profile();
        expanded.hardware_profiles.push(HardwareProfile {
            id: "esp-secondary".into(),
            name: "ESP secondary".into(),
            board_profile_id: crate::hardware::YD_ESP32_S3_BOARD_ID.into(),
            debounce_ms: 30,
            ssd1306: None,
            sh1106: None,
            inputs: Vec::new(),
        });
        coordinator
            .workspace
            .write()
            .unwrap()
            .save_profile(expanded.clone())
            .unwrap();
        enumerator.set(
            vec![
                serial("/dev/a", 0x303a, 0x4002, Some("A")),
                serial("/dev/b", 0x303a, 0x4002, Some("B")),
            ],
            Vec::new(),
        );
        scan(&mut coordinator);
        let a = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "A").unwrap();
        let b = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "B").unwrap();
        {
            let mut workspace = coordinator.workspace.write().unwrap();
            workspace
                .set_assignment(
                    &a,
                    RuntimeAssignment {
                        device_profile_id: "red-phone-v1".into(),
                        hardware_profile_id: "esp".into(),
                    },
                )
                .unwrap();
            workspace
                .set_assignment(
                    &b,
                    RuntimeAssignment {
                        device_profile_id: "red-phone-v1".into(),
                        hardware_profile_id: "esp-secondary".into(),
                    },
                )
                .unwrap();
        }
        coordinator.sync_profiles();
        launcher.clear_commands();

        let mut changed_a = expanded.clone();
        changed_a.hardware_profiles[0].debounce_ms = 40;
        coordinator
            .workspace
            .write()
            .unwrap()
            .save_profile(changed_a.clone())
            .unwrap();
        coordinator
            .apply_profile_change(&ProfileChange::between(Some(&expanded), Some(&changed_a)));

        let a_revision = match launcher.commands_for(&a).as_slice() {
            [WorkerCommand::Reconfigure { revision, .. }] => *revision,
            commands => panic!("unexpected A commands: {commands:?}"),
        };
        assert!(a_revision > 0);
        assert!(launcher.commands_for(&b).is_empty());

        launcher.clear_commands();
        let mut changed_b = changed_a.clone();
        changed_b.hardware_profiles[1].debounce_ms = 50;
        coordinator
            .workspace
            .write()
            .unwrap()
            .save_profile(changed_b.clone())
            .unwrap();
        coordinator
            .apply_profile_change(&ProfileChange::between(Some(&changed_a), Some(&changed_b)));
        let b_revision = match launcher.commands_for(&b).as_slice() {
            [WorkerCommand::Reconfigure { revision, .. }] => *revision,
            commands => panic!("unexpected B commands: {commands:?}"),
        };
        assert!(b_revision > 0);
        assert!(launcher.commands_for(&a).is_empty());
    }

    #[test]
    fn learning_targets_one_exact_device_keeps_draft_unpersisted_and_cancels_on_disconnect() {
        let (_directory, enumerator, launcher, mut coordinator) = harness();
        enumerator.set(
            vec![
                serial("/dev/a", 0x303a, 0x4002, Some("A")),
                serial("/dev/b", 0x303a, 0x4002, Some("B")),
            ],
            Vec::new(),
        );
        scan(&mut coordinator);
        let a = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "A").unwrap();
        let b = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "B").unwrap();
        let persisted = coordinator.workspace.read().unwrap().profiles["red-phone-v1"].clone();
        launcher.clear_commands();

        let target = coordinator
            .begin_learning(&a, "red-phone-v1", "esp", 17, vec![6, 7])
            .unwrap();

        assert_eq!(target.device_id, a);
        assert_eq!(target.device_profile_id, "red-phone-v1");
        assert_eq!(target.hardware_profile_id, "esp");
        assert_eq!(target.editing_revision, 17);
        assert!(target.firmware_revision > 0);
        assert!(matches!(
            launcher.commands_for(&a).as_slice(),
            [WorkerCommand::BeginLearning(sent)] if sent == &target
        ));
        assert!(launcher.commands_for(&b).is_empty());
        assert_eq!(
            coordinator.workspace.read().unwrap().profiles["red-phone-v1"],
            persisted
        );

        launcher.clear_commands();
        coordinator.cancel_learning(&a).unwrap();
        assert!(matches!(
            launcher.commands_for(&a).as_slice(),
            [WorkerCommand::EndLearning { .. }]
        ));
        assert!(launcher.commands_for(&b).is_empty());

        launcher.clear_commands();
        coordinator
            .begin_learning(&a, "red-phone-v1", "esp", 18, vec![6, 7])
            .unwrap();
        coordinator.handle_worker_event(WorkerEvent::Disconnected {
            generation: 1,
            device_id: a.clone(),
            error: None,
        });
        let statuses = coordinator.devices();
        assert!(
            statuses
                .iter()
                .find(|status| status.device_id == a)
                .unwrap()
                .learning
                .is_none()
        );
        assert_eq!(
            statuses
                .iter()
                .find(|status| status.device_id == b)
                .unwrap()
                .connection,
            ConnectionDimension::Online
        );
        assert_eq!(
            coordinator.workspace.read().unwrap().profiles["red-phone-v1"],
            persisted
        );
    }
}
