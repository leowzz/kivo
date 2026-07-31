use crate::{
    device::{LearningTarget, RuntimeActivity, RuntimeProfileSnapshot},
    hardware::{
        BoardProfile, DeviceId, board_by_bootloader_usb, board_by_id, board_by_runtime_usb,
    },
    metrics::MetricAttribution,
    paste::PasteHandle,
    profile::ProfileChange,
    protocol::{HelloCapabilities, InputState, PhysicalInput, validate_hello},
    workspace::{AppError, AssignmentResolution, RuntimeAssignment, Workspace},
};
use nusb::MaybeFuture;
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, RwLock, mpsc},
};

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

pub struct SystemUsbEnumerator;

impl UsbEnumerator for SystemUsbEnumerator {
    fn serial_ports(&self) -> Result<Vec<SerialObservation>, String> {
        serialport::available_ports()
            .map_err(|error| error.to_string())
            .map(|ports| {
                ports
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
    pub raw_serial: String,
    pub port: Option<String>,
    pub controller_family_id: String,
    pub board_profile_id: String,
    pub firmware_build_id: Option<String>,
    pub pins: Vec<u8>,
    pub runtime_assignment: Option<RuntimeAssignment>,
    pub latest_error: Option<String>,
    pub learning: Option<LearningTarget>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateStatus {
    pub key: String,
    pub device_id: Option<DeviceId>,
    pub mode: DeviceMode,
    pub identity: IdentityDimension,
    pub raw_serial: Option<String>,
    pub port: Option<String>,
    pub controller_family_id: String,
    pub board_profile_id: String,
    pub latest_error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerStart {
    pub device_id: DeviceId,
    pub port: String,
    pub board_profile_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkerCommand {
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
        event_id: u64,
        input: PhysicalInput,
        state: InputState,
        occurred_at_ms: u64,
    },
    Shutdown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkerEvent {
    HelloValidated {
        device_id: DeviceId,
        capabilities: HelloCapabilities,
    },
    Input {
        device_id: DeviceId,
        event_id: u64,
        input: PhysicalInput,
        state: InputState,
        occurred_at_ms: u64,
    },
    SequenceFinished {
        device_id: DeviceId,
        receive_sequence: u64,
    },
    Activity {
        device_id: DeviceId,
        activity: RuntimeActivity,
    },
    Disconnected {
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
}

struct WorkerSlot {
    worker: Box<dyn DeviceWorker>,
    port: String,
    firmware_revision: u32,
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
    enumerator: Arc<dyn UsbEnumerator>,
    launcher: Arc<dyn WorkerLauncher>,
    workspace: Arc<RwLock<Workspace>>,
    paste: Option<PasteHandle>,
    workers: BTreeMap<DeviceId, WorkerSlot>,
    devices: BTreeMap<DeviceId, DeviceStatus>,
    candidates: Vec<CandidateStatus>,
    event_sender: mpsc::Sender<WorkerEvent>,
    event_receiver: mpsc::Receiver<WorkerEvent>,
    receive_sequence: u64,
    sequence_owners: BTreeMap<u64, DeviceId>,
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

    pub fn with_paste(
        enumerator: Arc<dyn UsbEnumerator>,
        launcher: Arc<dyn WorkerLauncher>,
        workspace: Arc<RwLock<Workspace>>,
        paste: Option<PasteHandle>,
    ) -> Self {
        let (event_sender, event_receiver) = mpsc::channel();
        Self {
            enumerator,
            launcher,
            workspace,
            paste,
            workers: BTreeMap::new(),
            devices: BTreeMap::new(),
            candidates: Vec::new(),
            event_sender,
            event_receiver,
            receive_sequence: 0,
            sequence_owners: BTreeMap::new(),
        }
    }

    pub fn scan_once(&mut self) -> Result<(), String> {
        let serial = self.enumerator.serial_ports()?;
        let bootloader = self.enumerator.usb_devices()?;
        let mut classified = Vec::new();
        for observation in serial {
            if let Some(board) = board_by_runtime_usb(observation.vid, observation.pid) {
                classified.push(ClassifiedObservation::Runtime { board, observation });
            }
        }
        for observation in bootloader {
            if let Some(board) = board_by_bootloader_usb(observation.vid, observation.pid) {
                classified.push(ClassifiedObservation::Bootloader { board, observation });
            }
        }
        self.reconcile(classified);
        Ok(())
    }

    fn reconcile(&mut self, observations: Vec<ClassifiedObservation>) {
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
            match DeviceId::new(observation.board().id, serial) {
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
                    status.latest_error = Some("duplicate_identity".into());
                    status.port = None;
                    status.firmware_build_id = None;
                    status.pins.clear();
                    status.learning = None;
                }
                continue;
            }
            let observation = &group[0];
            match observation {
                ClassifiedObservation::Bootloader { .. } => {
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
                        slot.port = observation.port.clone();
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
                    let start = WorkerStart {
                        device_id: device_id.clone(),
                        port: observation.port.clone(),
                        board_profile_id: board.id.into(),
                    };
                    match self.launcher.start(start, self.event_sender.clone()) {
                        Ok(worker) => {
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
                status.pins.clear();
                status.latest_error = None;
                status.learning = None;
            }
        }
    }

    pub fn drain_worker_events(&mut self) {
        while let Ok(event) = self.event_receiver.try_recv() {
            self.handle_worker_event(event);
        }
    }

    pub fn handle_worker_event(&mut self, event: WorkerEvent) {
        match event {
            WorkerEvent::HelloValidated {
                device_id,
                capabilities,
            } => self.accept_hello(device_id, capabilities),
            WorkerEvent::Input {
                device_id,
                event_id,
                input,
                state,
                occurred_at_ms,
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
                            event_id,
                            input,
                            state,
                            occurred_at_ms,
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
            }
            WorkerEvent::SequenceFinished {
                device_id,
                receive_sequence,
            } => {
                if self.sequence_owners.get(&receive_sequence) == Some(&device_id) {
                    self.sequence_owners.remove(&receive_sequence);
                    if let Some(paste) = &self.paste {
                        let _ = paste.finish_sequence(receive_sequence);
                    }
                }
            }
            WorkerEvent::Activity {
                device_id,
                activity,
            } => {
                if let Some(status) = self.devices.get_mut(&device_id) {
                    if activity.code == "topology_active" {
                        status.runtime = RuntimeDimension::Ready;
                        status.latest_error = None;
                    } else if activity.code == "topology_rejected"
                        || activity.code.ends_with("failed")
                        || activity.code.ends_with("mismatch")
                        || activity.code.ends_with("timeout")
                    {
                        status.runtime = RuntimeDimension::RuntimeError;
                        status.latest_error = Some(activity.code);
                    }
                }
            }
            WorkerEvent::Disconnected { device_id, error } => {
                if !self.workers.contains_key(&device_id) {
                    return;
                }
                self.stop_worker(&device_id);
                if let Some(status) = self.devices.get_mut(&device_id) {
                    status.connection = ConnectionDimension::Offline;
                    status.mode = None;
                    status.runtime = RuntimeDimension::Inactive;
                    status.firmware_build_id = None;
                    status.pins.clear();
                    status.port = None;
                    status.learning = None;
                    status.latest_error = error;
                } else if let Some(candidate) = self
                    .candidates
                    .iter_mut()
                    .find(|candidate| candidate.device_id.as_ref() == Some(&device_id))
                {
                    candidate.latest_error = error;
                }
            }
        }
    }

    fn accept_hello(&mut self, device_id: DeviceId, capabilities: HelloCapabilities) {
        if !self.workers.contains_key(&device_id) {
            return;
        }
        let Some(board) = board_by_id(device_id.board_profile_id()) else {
            self.stop_worker(&device_id);
            return;
        };
        if let Err(error) = validate_hello(board, &capabilities) {
            self.reject_worker(&device_id, error.code);
            return;
        }
        let profile = {
            let mut workspace = self
                .workspace
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            match workspace.enroll_device(device_id.clone()) {
                Ok(_) => Ok(runtime_profile(&workspace, &device_id)),
                Err(error) => Err(error),
            }
        };
        let profile = match profile {
            Ok(profile) => profile,
            Err(error) => {
                self.reject_worker(&device_id, error.code);
                return;
            }
        };
        self.rebuild_device(&device_id);
        if let Some(status) = self.devices.get_mut(&device_id) {
            status.connection = ConnectionDimension::Online;
            status.mode = Some(DeviceMode::Runtime);
            status.identity = IdentityDimension::Valid;
            status.firmware_build_id = Some(capabilities.firmware_build_id.clone());
            status.pins = capabilities.pins;
            status.port = self.workers.get(&device_id).map(|slot| slot.port.clone());
            status.runtime = if profile.is_some() {
                RuntimeDimension::Configuring
            } else {
                RuntimeDimension::Inactive
            };
        }
        if profile.is_some() {
            if let Err(error) = self.reconfigure_worker(&device_id, profile.map(Arc::new))
                && let Some(status) = self.devices.get_mut(&device_id)
            {
                status.runtime = RuntimeDimension::RuntimeError;
                status.latest_error = Some(error);
            }
        } else if let Some(slot) = self.workers.get(&device_id) {
            let _ = slot.worker.send(WorkerCommand::UpdateSnapshot(None));
        }
        self.candidates
            .retain(|candidate| candidate.device_id.as_ref() != Some(&device_id));
    }

    fn rebuild_offline_devices(&mut self) {
        let previous = std::mem::take(&mut self.devices);
        let workspace = self
            .workspace
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.devices = workspace
            .settings
            .devices
            .keys()
            .map(|id| (id.clone(), offline_status(&workspace, id)))
            .collect();
        for id in self.workers.keys() {
            if let Some(status) = previous.get(id) {
                self.devices.insert(id.clone(), status.clone());
            }
        }
    }

    fn rebuild_device(&mut self, id: &DeviceId) {
        let workspace = self
            .workspace
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if workspace.settings.devices.contains_key(id) {
            self.devices
                .insert(id.clone(), offline_status(&workspace, id));
        }
    }

    fn stop_worker(&mut self, id: &DeviceId) {
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
            status.latest_error = Some(error);
        } else if let Some(candidate) = self
            .candidates
            .iter_mut()
            .find(|candidate| candidate.device_id.as_ref() == Some(id))
        {
            candidate.latest_error = Some(error);
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

    pub fn apply_profile_change(&mut self, change: &ProfileChange) {
        let updates = {
            let workspace = self
                .workspace
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            workspace
                .settings
                .devices
                .iter()
                .filter_map(|(id, device)| {
                    let assignment = device.runtime_assignment.as_ref()?;
                    (assignment.device_profile_id == change.device_profile_id).then(|| {
                        (
                            id.clone(),
                            assignment.hardware_profile_id.clone(),
                            runtime_profile(&workspace, id).map(Arc::new),
                        )
                    })
                })
                .collect::<Vec<_>>()
        };
        for (id, hardware_profile_id, snapshot) in updates {
            if !self.workers.contains_key(&id) {
                continue;
            }
            if change
                .topology_hardware_profile_ids
                .contains(&hardware_profile_id)
            {
                match self.reconfigure_worker(&id, snapshot) {
                    Ok(_) => {
                        if let Some(status) = self.devices.get_mut(&id) {
                            status.runtime = RuntimeDimension::Configuring;
                            status.learning = None;
                            status.latest_error = None;
                        }
                    }
                    Err(error) => {
                        if let Some(status) = self.devices.get_mut(&id) {
                            status.runtime = RuntimeDimension::RuntimeError;
                            status.latest_error = Some(error);
                        }
                    }
                }
            } else if change.host_mapping_changed
                && let Some(slot) = self.workers.get(&id)
                && let Err(error) = slot.worker.send(WorkerCommand::UpdateSnapshot(snapshot))
                && let Some(status) = self.devices.get_mut(&id)
            {
                status.runtime = RuntimeDimension::RuntimeError;
                status.latest_error = Some(error);
            }
        }
        self.refresh_persisted_status();
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
        let board = board_by_id(device_id.board_profile_id())
            .ok_or_else(|| AppError::new("unknown_board_profile"))?;
        {
            let workspace = self
                .workspace
                .read()
                .map_err(|_| AppError::new("workspace_unavailable"))?;
            let profile = workspace
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
        let snapshot = {
            let workspace = self
                .workspace
                .read()
                .map_err(|_| AppError::new("workspace_unavailable"))?;
            runtime_profile(&workspace, device_id).map(Arc::new)
        };
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

    pub fn cancel_learning(&mut self, device_id: &DeviceId) -> Result<(), AppError> {
        self.end_learning(device_id)
    }

    pub fn sync_profiles(&mut self) {
        let updates = {
            let workspace = self
                .workspace
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            self.workers
                .keys()
                .map(|id| (id.clone(), runtime_profile(&workspace, id).map(Arc::new)))
                .collect::<Vec<_>>()
        };
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
                        status.latest_error = Some(error);
                    }
                }
            }
        }
        self.refresh_persisted_status();
    }

    fn refresh_persisted_status(&mut self) {
        let workspace = self
            .workspace
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for (id, status) in &mut self.devices {
            if workspace.settings.devices.contains_key(id) {
                let persisted = offline_status(&workspace, id);
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

    #[cfg(test)]
    pub fn candidates(&self) -> &[CandidateStatus] {
        &self.candidates
    }

    #[cfg(test)]
    pub fn last_receive_sequence(&self) -> u64 {
        self.receive_sequence
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
    CandidateStatus {
        key: format!("runtime:{}", observation.port),
        device_id,
        mode: DeviceMode::Runtime,
        identity,
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
    CandidateStatus {
        key: observation.key(),
        device_id,
        mode: observation.mode(),
        identity,
        raw_serial: observation.serial().map(str::to_owned),
        port: observation.port(),
        controller_family_id: observation.board().family_id.into(),
        board_profile_id: observation.board().id.into(),
        latest_error,
    }
}

fn offline_status(workspace: &Workspace, id: &DeviceId) -> DeviceStatus {
    let record = &workspace.settings.devices[id];
    let board = board_by_id(&record.board_profile_id).expect("validated persisted board profile");
    let (assignment, runtime_assignment) = match workspace.assignment_resolution(id) {
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
        pins: Vec::new(),
        runtime_assignment,
        latest_error: None,
        learning: None,
    }
}

fn runtime_profile(workspace: &Workspace, id: &DeviceId) -> Option<RuntimeProfileSnapshot> {
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
        hardware::DeviceId,
        paste::{ClipboardWriter, PasteCoordinator, PasteReply, PasteRequest},
        profile::{DeviceProfile, HardwareProfile, PROFILE_SCHEMA_VERSION, ProfileChange},
        protocol::HelloCapabilities,
        workspace::Workspace,
    };
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
        hellos: Mutex<BTreeMap<String, HelloCapabilities>>,
        stopped: Arc<Mutex<Vec<DeviceId>>>,
        commands: Arc<Mutex<BTreeMap<DeviceId, Vec<WorkerCommand>>>>,
    }

    impl FakeLauncher {
        fn fail_port(&self, port: &str, error: &str) {
            self.failures
                .lock()
                .unwrap()
                .insert(port.into(), error.into());
        }

        fn starts(&self) -> Vec<WorkerStart> {
            self.starts.lock().unwrap().clone()
        }

        fn set_hello(&self, port: &str, hello: HelloCapabilities) {
            self.hellos.lock().unwrap().insert(port.into(), hello);
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
    }

    struct FakeWorker {
        device_id: DeviceId,
        stopped: Arc<Mutex<Vec<DeviceId>>>,
        commands: Arc<Mutex<BTreeMap<DeviceId, Vec<WorkerCommand>>>>,
    }

    impl DeviceWorker for FakeWorker {
        fn send(&self, command: WorkerCommand) -> Result<(), String> {
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

        fn join(&mut self) {}
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
            events
                .send(WorkerEvent::HelloValidated {
                    device_id: start.device_id.clone(),
                    capabilities,
                })
                .unwrap();
            Ok(Box::new(FakeWorker {
                device_id: start.device_id,
                stopped: Arc::clone(&self.stopped),
                commands: Arc::clone(&self.commands),
            }))
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
            profile: serde_json::from_str(include_str!("../../models/red-phone-v1.json")).unwrap(),
            hardware_profiles: vec![HardwareProfile {
                id: "esp".into(),
                name: "ESP".into(),
                board_profile_id: "luatos-esp32s3-aio".into(),
                debounce_ms: 30,
                inputs: Vec::new(),
            }],
            actions: BTreeMap::new(),
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
            protocol: 3,
            controller_family_id: board.family_id.into(),
            board_profile_id: board.id.into(),
            firmware_build_id: "test-build".into(),
            pins: board.safe_pins.to_vec(),
        }
    }

    fn scan(coordinator: &mut RuntimeCoordinator) {
        coordinator.scan_once().unwrap();
        coordinator.drain_worker_events();
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
    fn stale_disconnect_after_coordinator_stop_does_not_override_bootloader_mode() {
        let (_directory, enumerator, _launcher, mut coordinator) = harness();
        enumerator.set(
            vec![serial("/dev/rp", 0x2e8a, 0x102e, Some("MODE"))],
            Vec::new(),
        );
        scan(&mut coordinator);
        let id = DeviceId::new("vccgnd-yd-rp2040", "MODE").unwrap();

        enumerator.set(Vec::new(), vec![boot("1-1", "MODE")]);
        scan(&mut coordinator);
        coordinator.handle_worker_event(WorkerEvent::Disconnected {
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
        let rejected = DeviceId::new("luatos-esp32s3-aio", "A").unwrap();

        coordinator.handle_worker_event(WorkerEvent::Activity {
            device_id: rejected,
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
        assert_eq!(rejected.latest_error.as_deref(), Some("topology_rejected"));
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
                protocol: 3,
                controller_family_id: "wrong-family".into(),
                board_profile_id: "luatos-esp32s3-aio".into(),
                firmware_build_id: "bad".into(),
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
        let id = DeviceId::new("luatos-esp32s3-aio", "STALE").unwrap();
        let board = crate::hardware::board_by_id(id.board_profile_id()).unwrap();

        coordinator.handle_worker_event(WorkerEvent::HelloValidated {
            device_id: id,
            capabilities: HelloCapabilities {
                protocol: 3,
                controller_family_id: board.family_id.into(),
                board_profile_id: board.id.into(),
                firmware_build_id: "stale".into(),
                pins: board.safe_pins.to_vec(),
            },
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
        let id = DeviceId::new("luatos-esp32s3-aio", "A").unwrap();
        coordinator.handle_worker_event(WorkerEvent::Input {
            device_id: id,
            event_id: 9,
            input: crate::protocol::PhysicalInput::Direct { gpio: 6 },
            state: crate::protocol::InputState::Down,
            occurred_at_ms: 10,
        });

        enumerator.set(Vec::new(), Vec::new());
        scan(&mut coordinator);
        handle.register_sequence(2).unwrap();
        let (reply, replies) = std::sync::mpsc::channel();
        let next_id = DeviceId::new("luatos-esp32s3-aio", "B").unwrap();
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
        let id = DeviceId::new("luatos-esp32s3-aio", "A").unwrap();

        coordinator.handle_worker_event(WorkerEvent::Input {
            device_id: id.clone(),
            event_id: 8,
            input: crate::protocol::PhysicalInput::Direct { gpio: 6 },
            state: crate::protocol::InputState::Down,
            occurred_at_ms: 10,
        });
        coordinator.handle_worker_event(WorkerEvent::Input {
            device_id: id,
            event_id: 9,
            input: crate::protocol::PhysicalInput::Direct { gpio: 6 },
            state: crate::protocol::InputState::Down,
            occurred_at_ms: 11,
        });

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
        let owner = DeviceId::new("luatos-esp32s3-aio", "A").unwrap();
        coordinator.handle_worker_event(WorkerEvent::Input {
            device_id: owner.clone(),
            event_id: 8,
            input: crate::protocol::PhysicalInput::Direct { gpio: 6 },
            state: crate::protocol::InputState::Down,
            occurred_at_ms: 10,
        });

        coordinator.handle_worker_event(WorkerEvent::SequenceFinished {
            device_id: DeviceId::new("luatos-esp32s3-aio", "B").unwrap(),
            receive_sequence: 1,
        });
        assert_eq!(coordinator.sequence_owners.get(&1), Some(&owner));
        coordinator.handle_worker_event(WorkerEvent::SequenceFinished {
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
        let a = DeviceId::new("luatos-esp32s3-aio", "A").unwrap();
        let b = DeviceId::new("luatos-esp32s3-aio", "B").unwrap();
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
            vec![crate::profile::ButtonAction::Paste {
                text: "updated".into(),
            }],
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
    fn live_update_topology_targets_exact_hardware_with_independent_nonzero_revisions() {
        let (_directory, enumerator, launcher, mut coordinator) = harness();
        let mut expanded = profile();
        expanded.hardware_profiles.push(HardwareProfile {
            id: "esp-secondary".into(),
            name: "ESP secondary".into(),
            board_profile_id: "luatos-esp32s3-aio".into(),
            debounce_ms: 30,
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
        let a = DeviceId::new("luatos-esp32s3-aio", "A").unwrap();
        let b = DeviceId::new("luatos-esp32s3-aio", "B").unwrap();
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
        let a = DeviceId::new("luatos-esp32s3-aio", "A").unwrap();
        let b = DeviceId::new("luatos-esp32s3-aio", "B").unwrap();
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
