#![cfg_attr(feature = "product-studio", allow(dead_code, unused_imports))]

mod coordinator;
mod device;
#[allow(dead_code)]
mod display;
pub mod hardware;
mod metrics;
mod model;
mod paste;
pub mod product;
pub mod product_build;
mod profile;
mod protocol;
mod runtime_log;
mod storage;
#[cfg(feature = "product-studio")]
mod studio;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod tray;
#[allow(dead_code)]
mod trigger;
mod usage;
mod workspace;

use coordinator::{
    CandidateStatus, DeviceScan, DeviceStatus, IdentityDimension, RuntimeCoordinator, RuntimeEvent,
    UsbEnumerator, WorkspaceRevision, enumerate_devices,
};
use display::{
    DisplayService, DisplaySnapshot, built_in_provider_registry, built_in_renderer_registry,
};
use hardware::{BOARD_PROFILES, BoardProfile};
use metrics::{HomeMetricsSnapshot, MetricsStore};
use paste::PasteCoordinator;
use profile::{CreateDeviceProfileRequest, DeviceProfile};
use serde::Serialize;
use std::{
    fs,
    path::Path,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager};
use usage::{UsageService, UsageSettingsPatch, UsageSnapshot, UsageView};
use workspace::{
    AppError, AssignmentResolution, BackupPreview, CreateProductConfigurationRequest,
    DuplicateProfileForDeviceRequest, EditorSettingsPatch, ImportPreview, Language,
    ProductConfigurationProfile, RuntimeAssignment, Workspace,
};

const DEVICE_SCAN_INTERVAL: Duration = Duration::from_millis(500);
const RUNTIME_EVENT_POLL_INTERVAL: Duration = Duration::from_millis(5);
#[cfg(feature = "product-studio")]
const MAIN_APP_IDENTIFIER: &str = "cn.wleo.kivo";

struct BackgroundDeviceScanner {
    enumerator: Arc<dyn UsbEnumerator>,
    in_flight: Option<JoinHandle<Result<DeviceScan, String>>>,
    next_scan: Instant,
    rescan_requested: bool,
}

impl BackgroundDeviceScanner {
    fn new(enumerator: Arc<dyn UsbEnumerator>) -> Self {
        Self {
            enumerator,
            in_flight: None,
            next_scan: Instant::now(),
            rescan_requested: false,
        }
    }

    fn poll(&mut self) -> Option<Result<DeviceScan, String>> {
        if self.in_flight.as_ref().is_some_and(JoinHandle::is_finished) {
            let result = self
                .in_flight
                .take()
                .expect("finished device scan is present")
                .join()
                .unwrap_or_else(|_| Err("device_scan_thread_panicked".into()));
            self.next_scan = if self.rescan_requested {
                self.rescan_requested = false;
                Instant::now()
            } else {
                Instant::now() + DEVICE_SCAN_INTERVAL
            };
            return Some(result);
        }

        if self.in_flight.is_none() && Instant::now() >= self.next_scan {
            let enumerator = Arc::clone(&self.enumerator);
            self.in_flight = Some(thread::spawn(move || {
                enumerate_devices(enumerator.as_ref())
            }));
        }
        None
    }

    fn request_scan(&mut self) {
        if self.in_flight.is_some() {
            self.rescan_requested = true;
        } else {
            self.next_scan = Instant::now();
        }
    }
}

fn poll_runtime_coordinator(
    scanner: &mut BackgroundDeviceScanner,
    coordinator: &Mutex<RuntimeCoordinator>,
) -> RuntimePoll {
    let scan = scanner.poll();
    let mut coordinator = coordinator
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (scan, scan_error) = match scan {
        Some(Ok(scan)) => {
            coordinator.apply_scan(scan);
            (
                Some(RuntimeScanSnapshot {
                    devices: coordinator.devices(),
                    candidates: coordinator.candidates(),
                }),
                None,
            )
        }
        Some(Err(error)) => (None, Some(error)),
        None => (None, None),
    };
    let events = coordinator.drain_worker_events();
    RuntimePoll {
        scan,
        scan_error,
        events,
    }
}

fn newest_display_snapshot(
    snapshots: &std::sync::mpsc::Receiver<Arc<DisplaySnapshot>>,
) -> Option<Arc<DisplaySnapshot>> {
    snapshots.try_iter().last()
}

struct StopOnDrop {
    stop: Arc<AtomicBool>,
}

impl StopOnDrop {
    fn new(stop: Arc<AtomicBool>) -> Self {
        Self { stop }
    }
}

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

struct RuntimeScanSnapshot {
    devices: Vec<DeviceStatus>,
    candidates: Vec<CandidateStatus>,
}

struct RuntimePoll {
    scan: Option<RuntimeScanSnapshot>,
    scan_error: Option<String>,
    events: Vec<RuntimeEvent>,
}

#[doc(hidden)]
pub mod test_support {
    pub use crate::{
        coordinator::{
            BootloaderObservation, ConnectionDimension, DeviceMode, RuntimeCoordinator,
            RuntimeDimension, SerialObservation, UsbEnumerator, WorkspaceRevision,
        },
        device::{SerialTransport, SerialTransportFactory, SystemWorkerLauncher},
        model::{ButtonDefinition, ButtonGroup, ModelLayout},
        paste::{ClipboardWriter, Clock, PasteCoordinator},
        profile::{
            ButtonAction, DeviceProfile, HardwareProfile, InputSource, PROFILE_SCHEMA_VERSION,
            TriggerActions, TriggerSettings,
        },
        workspace::{RuntimeAssignment, Workspace},
    };

    pub fn wait_for_paste_request(
        paste: &PasteCoordinator,
        device_id: &crate::hardware::DeviceId,
        event_id: u64,
        step: u16,
        text: &str,
        timeout: std::time::Duration,
    ) -> Result<(), String> {
        paste.wait_for_request(device_id, event_id, step, text, timeout)
    }
}

struct AppState {
    workspace: Arc<RwLock<Workspace>>,
    operation_barrier: Arc<RwLock<()>>,
    metrics: Option<Arc<MetricsStore>>,
    coordinator: Option<Arc<Mutex<RuntimeCoordinator>>>,
    paste: Option<Arc<PasteCoordinator>>,
    usage: Option<Arc<UsageService>>,
    stop: Arc<AtomicBool>,
    scan_requested: Arc<AtomicBool>,
    display_thread: Mutex<Option<JoinHandle<()>>>,
    usage_thread: Mutex<Option<JoinHandle<()>>>,
    coordinator_thread: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppSnapshot {
    device_profiles: Vec<DeviceProfile>,
    product_configurations: Vec<ProductConfigurationProfile>,
    editor_profile: Option<String>,
    board_profiles: Vec<BoardProfileSummary>,
    devices: Vec<DeviceStatus>,
    candidates: Vec<CandidateStatus>,
    language: Language,
    home_metrics: Option<HomeMetricsSnapshot>,
    usage: Option<UsageView>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct BoardProfileSummary {
    id: String,
    controller_family_id: String,
    display_name: String,
    runtime_usb: String,
    bootloader_usb: Option<String>,
    supports_oled: bool,
    safe_pins: Vec<u8>,
}

impl From<&BoardProfile> for BoardProfileSummary {
    fn from(board: &BoardProfile) -> Self {
        Self {
            id: board.id.into(),
            controller_family_id: board.family_id.into(),
            display_name: board.display_name.into(),
            runtime_usb: format!(
                "{:04x}:{:04x}",
                board.runtime_usb.vid, board.runtime_usb.pid
            ),
            bootloader_usb: board
                .bootloader_usb
                .map(|usb| format!("{:04x}:{:04x}", usb.vid, usb.pid)),
            supports_oled: board.supports_oled,
            safe_pins: board.safe_pins.to_vec(),
        }
    }
}

fn state_error(code: &str) -> AppError {
    AppError::new(code)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn enrich_runtime_event(
    workspace: &RwLock<Workspace>,
    metrics: Option<&MetricsStore>,
    mut event: RuntimeEvent,
) -> RuntimeEvent {
    let editor_profile = workspace
        .read()
        .ok()
        .and_then(|workspace| workspace.settings.editor_profile.clone());
    let updates_home = (event.activity.code == "input_state"
        && event.activity.pressed == Some(true))
        || event.activity.code == "feature_disabled";
    let matches_editor = event.device_profile_id.is_some()
        && event.device_profile_id == editor_profile
        && updates_home;
    if matches_editor
        && let (Some(metrics), Some(device_profile_id)) =
            (metrics, event.device_profile_id.as_deref())
    {
        event.home_update = metrics
            .home_snapshot(device_profile_id, None, now_ms())
            .ok();
    }
    event
}

fn snapshot(state: &AppState) -> Result<AppSnapshot, AppError> {
    let coordinator = state
        .coordinator
        .as_ref()
        .map(|coordinator| {
            coordinator
                .lock()
                .map_err(|_| state_error("coordinator_unavailable"))
        })
        .transpose()?;
    let (devices, candidates) = coordinator
        .as_ref()
        .map(|coordinator| (coordinator.devices(), coordinator.candidates()))
        .unwrap_or_default();
    let workspace = state
        .workspace
        .read()
        .map_err(|_| state_error("workspace_unavailable"))?;
    let mut device_profiles = workspace.profiles.values().cloned().collect::<Vec<_>>();
    device_profiles.sort_by(|left, right| left.profile.name.cmp(&right.profile.name));
    let mut product_configurations = workspace
        .settings
        .product_configurations
        .values()
        .cloned()
        .collect::<Vec<_>>();
    product_configurations.sort_by(|left, right| left.name.cmp(&right.name));
    let editor_profile = workspace.settings.editor_profile.clone();
    let language = workspace.settings.language;
    drop(workspace);
    drop(coordinator);
    let home_metrics = state.metrics.as_ref().and_then(|metrics| {
        editor_profile
            .as_deref()
            .and_then(|profile_id| metrics.home_snapshot(profile_id, None, now_ms()).ok())
    });
    let usage = state.usage.as_ref().map(|usage| usage.view());
    Ok(AppSnapshot {
        device_profiles,
        product_configurations,
        editor_profile,
        board_profiles: BOARD_PROFILES
            .iter()
            .map(BoardProfileSummary::from)
            .collect(),
        devices,
        candidates,
        language,
        home_metrics,
        usage,
    })
}

fn mutate_workspace(
    state: &AppState,
    mutation: impl FnOnce(&mut Workspace, Option<&RuntimeCoordinator>) -> Result<(), AppError>,
) -> Result<AppSnapshot, AppError> {
    let mut coordinator = state
        .coordinator
        .as_ref()
        .map(|coordinator| {
            coordinator
                .lock()
                .map_err(|_| state_error("coordinator_unavailable"))
        })
        .transpose()?;
    let mut workspace = state
        .workspace
        .write()
        .map_err(|_| state_error("workspace_unavailable"))?;
    mutation(&mut workspace, coordinator.as_deref())?;
    let revision = WorkspaceRevision::capture(&workspace);
    drop(workspace);
    if let Some(coordinator) = coordinator.as_deref_mut() {
        coordinator.apply_workspace_revision(revision);
    }
    drop(coordinator);
    snapshot(state)
}

fn mutate_workspace_with_operation_barrier(
    state: &AppState,
    mutation: impl FnOnce(&mut Workspace, Option<&RuntimeCoordinator>) -> Result<(), AppError>,
) -> Result<AppSnapshot, AppError> {
    let mut coordinator = state
        .coordinator
        .as_ref()
        .map(|coordinator| {
            coordinator
                .lock()
                .map_err(|_| state_error("coordinator_unavailable"))
        })
        .transpose()?;
    let operation = state
        .operation_barrier
        .write()
        .map_err(|_| state_error("operation_barrier_unavailable"))?;
    let mut workspace = state
        .workspace
        .write()
        .map_err(|_| state_error("workspace_unavailable"))?;
    mutation(&mut workspace, coordinator.as_deref())?;
    let revision = WorkspaceRevision::capture(&workspace);
    drop(workspace);
    if let Some(coordinator) = coordinator.as_deref_mut() {
        coordinator.apply_workspace_revision(revision);
    }
    drop(operation);
    drop(coordinator);
    snapshot(state)
}

fn require_addressable_identity(
    coordinator: Option<&RuntimeCoordinator>,
    device_id: &hardware::DeviceId,
) -> Result<(), AppError> {
    if coordinator.is_some_and(|coordinator| {
        coordinator.devices().iter().any(|device| {
            device.device_id == *device_id
                && matches!(
                    device.identity,
                    IdentityDimension::InvalidIdentity | IdentityDimension::DuplicateIdentity
                )
        })
    }) {
        return Err(state_error("invalid_device_identity"));
    }
    Ok(())
}

fn save_profile_inner(state: &AppState, profile: DeviceProfile) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, move |workspace, _| workspace.save_profile(profile))
}

fn create_device_profile_inner(
    state: &AppState,
    request: CreateDeviceProfileRequest,
) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, move |workspace, _| {
        workspace.create_profile(request).map(|_| ())
    })
}

fn retry_candidate_inner(
    state: &AppState,
    device_id: &hardware::DeviceId,
) -> Result<AppSnapshot, AppError> {
    let coordinator = state
        .coordinator
        .as_ref()
        .ok_or_else(|| state_error("coordinator_unavailable"))?;
    {
        let mut coordinator = coordinator
            .lock()
            .map_err(|_| state_error("coordinator_unavailable"))?;
        coordinator
            .retry_candidate(device_id)
            .map_err(|error| state_error(&error))?;
    }
    state.scan_requested.store(true, Ordering::Relaxed);
    snapshot(state)
}

fn save_settings_inner(
    state: &AppState,
    settings: EditorSettingsPatch,
) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, move |workspace, _| workspace.save_settings(settings))
}

fn save_usage_settings_inner(
    state: &AppState,
    settings: UsageSettingsPatch,
) -> Result<AppSnapshot, AppError> {
    state
        .usage
        .as_ref()
        .ok_or_else(|| state_error("usage_service_unavailable"))?
        .save(settings)?;
    snapshot(state)
}

fn import_profile_inner(state: &AppState, path: &Path) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, |workspace, _| workspace.import_profile(path))
}

fn export_profile_inner(state: &AppState, id: &str, path: &Path) -> Result<AppSnapshot, AppError> {
    state
        .workspace
        .read()
        .map_err(|_| state_error("workspace_unavailable"))?
        .export_profile(id, path)?;
    snapshot(state)
}

fn delete_profile_inner(state: &AppState, id: &str) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, |workspace, _| workspace.delete_profile(id))
}

fn rename_device_inner(
    state: &AppState,
    device_id: &hardware::DeviceId,
    name: String,
) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, move |workspace, coordinator| {
        require_addressable_identity(coordinator, device_id)?;
        workspace.rename_device(device_id, name)
    })
}

fn save_product_configuration_inner(
    state: &AppState,
    device_id: &hardware::DeviceId,
    config: ProductConfigurationProfile,
) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, |workspace, coordinator| {
        let definition = coordinator
            .and_then(|coordinator| coordinator.product_definition(device_id))
            .cloned()
            .ok_or_else(|| AppError::new("product_definition_unavailable"))?;
        workspace.save_product_configuration(device_id, &definition, config)
    })
}

fn select_product_configuration_inner(
    state: &AppState,
    device_id: &hardware::DeviceId,
    configuration_id: &str,
) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, |workspace, coordinator| {
        let definition = coordinator
            .and_then(|coordinator| coordinator.product_definition(device_id))
            .cloned();
        workspace.select_product_configuration(device_id, configuration_id, definition.as_ref())
    })
}

fn create_product_configuration_inner(
    state: &AppState,
    request: CreateProductConfigurationRequest,
) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, move |workspace, _| {
        workspace.create_product_configuration(request)
    })
}

fn save_runtime_assignment_inner(
    state: &AppState,
    device_id: &hardware::DeviceId,
    assignment: RuntimeAssignment,
) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, move |workspace, coordinator| {
        require_addressable_identity(coordinator, device_id)?;
        validate_online_assignment_protocol(coordinator, workspace, device_id, &assignment)?;
        workspace.set_assignment(device_id, assignment)
    })
}

fn validate_online_assignment_protocol(
    coordinator: Option<&RuntimeCoordinator>,
    workspace: &Workspace,
    device_id: &hardware::DeviceId,
    assignment: &RuntimeAssignment,
) -> Result<(), AppError> {
    let profile = workspace
        .profiles
        .get(&assignment.device_profile_id)
        .ok_or_else(|| AppError::new("unknown_profile"))?;
    validate_online_firmware_protocol(coordinator, device_id, profile.minimum_protocol_version())
}

fn validate_online_firmware_protocol(
    coordinator: Option<&RuntimeCoordinator>,
    device_id: &hardware::DeviceId,
    minimum: u16,
) -> Result<(), AppError> {
    let Some(status) = coordinator.and_then(|coordinator| {
        coordinator
            .devices()
            .into_iter()
            .find(|status| status.device_id == *device_id)
    }) else {
        return Ok(());
    };
    if status.connection != coordinator::ConnectionDimension::Online {
        return Ok(());
    }
    let Some(actual) = status.firmware_protocol else {
        return Ok(());
    };
    if actual < minimum {
        return Err(AppError::new("firmware_update_required")
            .with_param("expected", minimum.to_string())
            .with_param("actual", actual.to_string()));
    }
    Ok(())
}

fn validate_setup_eligibility(
    connection: coordinator::ConnectionDimension,
    mode: Option<coordinator::DeviceMode>,
    identity: IdentityDimension,
) -> Result<(), AppError> {
    if connection != coordinator::ConnectionDimension::Online {
        return Err(state_error("device_offline"));
    }
    if mode != Some(coordinator::DeviceMode::Runtime) {
        return Err(state_error("device_not_runtime"));
    }
    if identity != IdentityDimension::Valid {
        return Err(state_error("invalid_device_identity"));
    }
    Ok(())
}

fn require_setup_device(
    coordinator: Option<&RuntimeCoordinator>,
    device_id: &hardware::DeviceId,
) -> Result<(), AppError> {
    let status = coordinator
        .and_then(|coordinator| {
            coordinator
                .devices()
                .into_iter()
                .find(|device| device.device_id == *device_id)
        })
        .ok_or_else(|| state_error("unknown_device"))?;
    validate_setup_eligibility(status.connection, status.mode, status.identity)
}

fn complete_device_setup_inner(
    state: &AppState,
    device_id: &hardware::DeviceId,
    name: String,
    assignment: RuntimeAssignment,
) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, move |workspace, coordinator| {
        require_setup_device(coordinator, device_id)?;
        validate_online_assignment_protocol(coordinator, workspace, device_id, &assignment)?;
        workspace.complete_device_setup(device_id, name, assignment)
    })
}

fn duplicate_profile_for_device_inner(
    state: &AppState,
    request: DuplicateProfileForDeviceRequest,
) -> Result<AppSnapshot, AppError> {
    let metrics = state.metrics.as_deref();
    mutate_workspace_with_operation_barrier(state, move |workspace, coordinator| {
        require_addressable_identity(coordinator, &request.device_id)?;
        validate_online_firmware_protocol(
            coordinator,
            &request.device_id,
            request.source_profile.minimum_protocol_version(),
        )?;
        if let Some(metrics) = metrics {
            workspace.duplicate_profile_for_device_with_metrics(request, metrics)?;
        } else {
            workspace.duplicate_profile_for_device(request)?;
        }
        Ok(())
    })
}

fn clear_runtime_assignment_inner(
    state: &AppState,
    device_id: &hardware::DeviceId,
) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, |workspace, coordinator| {
        require_addressable_identity(coordinator, device_id)?;
        workspace.clear_assignment(device_id)
    })
}

fn forget_device_inner(
    state: &AppState,
    device_id: &hardware::DeviceId,
) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, |workspace, coordinator| {
        require_addressable_identity(coordinator, device_id)?;
        let online = coordinator.is_some_and(|coordinator| {
            coordinator.devices().iter().any(|device| {
                device.device_id == *device_id
                    && device.connection == coordinator::ConnectionDimension::Online
            })
        });
        workspace.forget_offline_device(device_id, online)
    })
}

fn get_device_metrics_inner(
    state: &AppState,
    device_id: &hardware::DeviceId,
) -> Result<HomeMetricsSnapshot, AppError> {
    let coordinator = state
        .coordinator
        .as_ref()
        .map(|coordinator| {
            coordinator
                .lock()
                .map_err(|_| state_error("coordinator_unavailable"))
        })
        .transpose()?;
    require_addressable_identity(coordinator.as_deref(), device_id)?;
    let workspace = state
        .workspace
        .read()
        .map_err(|_| state_error("workspace_unavailable"))?;
    match workspace.assignment_resolution(device_id) {
        AssignmentResolution::UnknownDevice => return Err(state_error("unknown_device")),
        AssignmentResolution::Unassigned { .. }
        | AssignmentResolution::Valid { .. }
        | AssignmentResolution::InvalidAssignment { .. } => {}
    }
    drop(workspace);
    drop(coordinator);
    state
        .metrics
        .as_deref()
        .ok_or_else(|| state_error("metrics_unavailable"))?
        .device_snapshot(device_id, now_ms())
        .map_err(|error| state_error("metrics_unavailable").with_detail(error.to_string()))
}

fn begin_learning_inner(
    state: &AppState,
    device_id: &hardware::DeviceId,
    device_profile_id: &str,
    hardware_profile_id: &str,
    editing_revision: u64,
    pins: Vec<u8>,
) -> Result<AppSnapshot, AppError> {
    let coordinator = state
        .coordinator
        .as_ref()
        .ok_or_else(|| state_error("coordinator_unavailable"))?;
    coordinator
        .lock()
        .map_err(|_| state_error("coordinator_unavailable"))?
        .begin_learning(
            device_id,
            device_profile_id,
            hardware_profile_id,
            editing_revision,
            pins,
        )?;
    snapshot(state)
}

fn end_learning_inner(
    state: &AppState,
    device_id: &hardware::DeviceId,
) -> Result<AppSnapshot, AppError> {
    let coordinator = state
        .coordinator
        .as_ref()
        .ok_or_else(|| state_error("coordinator_unavailable"))?;
    coordinator
        .lock()
        .map_err(|_| state_error("coordinator_unavailable"))?
        .end_learning(device_id)?;
    snapshot(state)
}

fn restore_backup_inner(state: &AppState, path: &Path) -> Result<AppSnapshot, AppError> {
    let mut coordinator = state
        .coordinator
        .as_ref()
        .map(|coordinator| {
            coordinator
                .lock()
                .map_err(|_| state_error("coordinator_unavailable"))
        })
        .transpose()?;
    let operation = state
        .operation_barrier
        .write()
        .map_err(|_| state_error("operation_barrier_unavailable"))?;
    let mut workspace = state
        .workspace
        .write()
        .map_err(|_| state_error("workspace_unavailable"))?;
    workspace.restore_compatible_backup(path, state.metrics.as_deref())?;
    let revision = WorkspaceRevision::capture(&workspace);
    drop(workspace);
    let retired = coordinator
        .as_mut()
        .map(|coordinator| coordinator.activate_restored_revision(revision));
    drop(operation);
    if let Some(retired) = retired {
        retired.join();
    }
    drop(coordinator);
    snapshot(state)
}

fn export_backup_inner(state: &AppState, path: &Path) -> Result<AppSnapshot, AppError> {
    let workspace = state
        .workspace
        .read()
        .map_err(|_| state_error("workspace_unavailable"))?;
    workspace.export_user_backup(path)?;
    drop(workspace);
    snapshot(state)
}

fn device_operation_context(device_id: &hardware::DeviceId) -> serde_json::Value {
    serde_json::json!({"deviceId": device_id})
}

fn profile_operation_context(device_profile_id: &str) -> serde_json::Value {
    serde_json::json!({"deviceProfileId": device_profile_id})
}

fn assignment_operation_context(
    device_id: &hardware::DeviceId,
    assignment: &RuntimeAssignment,
) -> serde_json::Value {
    serde_json::json!({
        "deviceId": device_id,
        "deviceProfileId": assignment.device_profile_id,
        "hardwareProfileId": assignment.hardware_profile_id,
    })
}

fn create_profile_operation_context(request: &CreateDeviceProfileRequest) -> serde_json::Value {
    match request {
        CreateDeviceProfileRequest::Clone {
            source_profile_id, ..
        } => serde_json::json!({"kind": "clone", "sourceProfileId": source_profile_id}),
        CreateDeviceProfileRequest::Blank {
            board_profile_id, ..
        } => serde_json::json!({"kind": "blank", "boardProfileId": board_profile_id}),
    }
}

fn settings_operation_context(settings: &EditorSettingsPatch) -> serde_json::Value {
    serde_json::json!({
        "schemaVersion": settings.schema_version,
        "editorProfile": settings.editor_profile,
        "language": settings.language,
    })
}

fn learning_operation_context(
    device_id: &hardware::DeviceId,
    device_profile_id: &str,
    hardware_profile_id: &str,
    editing_revision: u64,
    pin_count: usize,
) -> serde_json::Value {
    serde_json::json!({
        "deviceId": device_id,
        "deviceProfileId": device_profile_id,
        "hardwareProfileId": hardware_profile_id,
        "editingRevision": editing_revision,
        "pinCount": pin_count,
    })
}

#[tauri::command]
fn get_snapshot(state: tauri::State<'_, AppState>) -> Result<AppSnapshot, AppError> {
    snapshot(&state)
}

#[tauri::command]
fn retry_candidate(
    state: tauri::State<'_, AppState>,
    device_id: hardware::DeviceId,
) -> Result<AppSnapshot, AppError> {
    let context = device_operation_context(&device_id);
    runtime_log::operation(now_ms(), "device_candidate_retry", context, || {
        retry_candidate_inner(&state, &device_id)
    })
}

#[tauri::command]
fn save_device_profile(
    state: tauri::State<'_, AppState>,
    profile: DeviceProfile,
) -> Result<AppSnapshot, AppError> {
    let context = profile_operation_context(&profile.profile.id);
    runtime_log::operation(now_ms(), "device_profile_saved", context, || {
        save_profile_inner(&state, profile)
    })
}

#[tauri::command]
fn create_device_profile(
    state: tauri::State<'_, AppState>,
    request: CreateDeviceProfileRequest,
) -> Result<AppSnapshot, AppError> {
    let context = create_profile_operation_context(&request);
    runtime_log::operation(now_ms(), "device_profile_created", context, || {
        create_device_profile_inner(&state, request)
    })
}

#[tauri::command]
fn duplicate_profile_for_device(
    state: tauri::State<'_, AppState>,
    request: DuplicateProfileForDeviceRequest,
) -> Result<AppSnapshot, AppError> {
    let context = device_operation_context(&request.device_id);
    runtime_log::operation(now_ms(), "profile_duplicated_for_device", context, || {
        duplicate_profile_for_device_inner(&state, request)
    })
}

#[tauri::command]
fn save_settings(
    state: tauri::State<'_, AppState>,
    settings: EditorSettingsPatch,
) -> Result<AppSnapshot, AppError> {
    let context = settings_operation_context(&settings);
    runtime_log::operation(now_ms(), "settings_saved", context, || {
        save_settings_inner(&state, settings)
    })
}

#[tauri::command]
fn save_usage_settings(
    state: tauri::State<'_, AppState>,
    settings: UsageSettingsPatch,
) -> Result<AppSnapshot, AppError> {
    let context = serde_json::json!({
        "enabled": settings.enabled,
        "baseUrl": settings.base_url,
        "email": settings.email,
        "intervalSeconds": settings.interval_seconds,
    });
    runtime_log::operation(now_ms(), "usage_settings_saved", context, || {
        save_usage_settings_inner(&state, settings)
    })
}

#[tauri::command]
fn rename_device(
    state: tauri::State<'_, AppState>,
    device_id: hardware::DeviceId,
    name: String,
) -> Result<AppSnapshot, AppError> {
    let context = device_operation_context(&device_id);
    runtime_log::operation(now_ms(), "device_renamed", context, || {
        rename_device_inner(&state, &device_id, name)
    })
}

#[tauri::command]
fn save_product_configuration(
    state: tauri::State<'_, AppState>,
    device_id: hardware::DeviceId,
    config: ProductConfigurationProfile,
) -> Result<AppSnapshot, AppError> {
    save_product_configuration_inner(&state, &device_id, config)
}

#[tauri::command]
fn select_product_configuration(
    state: tauri::State<'_, AppState>,
    device_id: hardware::DeviceId,
    configuration_id: String,
) -> Result<AppSnapshot, AppError> {
    select_product_configuration_inner(&state, &device_id, &configuration_id)
}

#[tauri::command]
fn create_product_configuration(
    state: tauri::State<'_, AppState>,
    request: CreateProductConfigurationRequest,
) -> Result<AppSnapshot, AppError> {
    create_product_configuration_inner(&state, request)
}

#[tauri::command]
fn save_runtime_assignment(
    state: tauri::State<'_, AppState>,
    device_id: hardware::DeviceId,
    assignment: RuntimeAssignment,
) -> Result<AppSnapshot, AppError> {
    let context = assignment_operation_context(&device_id, &assignment);
    runtime_log::operation(now_ms(), "runtime_assignment_saved", context, || {
        save_runtime_assignment_inner(&state, &device_id, assignment)
    })
}

#[tauri::command]
fn complete_device_setup(
    state: tauri::State<'_, AppState>,
    device_id: hardware::DeviceId,
    name: String,
    assignment: RuntimeAssignment,
) -> Result<AppSnapshot, AppError> {
    let context = assignment_operation_context(&device_id, &assignment);
    runtime_log::operation(now_ms(), "device_setup_completed", context, || {
        complete_device_setup_inner(&state, &device_id, name, assignment)
    })
}

#[tauri::command]
fn clear_runtime_assignment(
    state: tauri::State<'_, AppState>,
    device_id: hardware::DeviceId,
) -> Result<AppSnapshot, AppError> {
    let context = device_operation_context(&device_id);
    runtime_log::operation(now_ms(), "runtime_assignment_cleared", context, || {
        clear_runtime_assignment_inner(&state, &device_id)
    })
}

#[tauri::command]
fn forget_device(
    state: tauri::State<'_, AppState>,
    device_id: hardware::DeviceId,
) -> Result<AppSnapshot, AppError> {
    let context = device_operation_context(&device_id);
    runtime_log::operation(now_ms(), "device_forgotten", context, || {
        forget_device_inner(&state, &device_id)
    })
}

#[tauri::command]
fn get_device_metrics(
    state: tauri::State<'_, AppState>,
    device_id: hardware::DeviceId,
) -> Result<HomeMetricsSnapshot, AppError> {
    get_device_metrics_inner(&state, &device_id)
}

#[tauri::command]
fn begin_learning(
    state: tauri::State<'_, AppState>,
    device_id: hardware::DeviceId,
    device_profile_id: String,
    hardware_profile_id: String,
    editing_revision: u64,
    pins: Vec<u8>,
) -> Result<AppSnapshot, AppError> {
    let context = learning_operation_context(
        &device_id,
        &device_profile_id,
        &hardware_profile_id,
        editing_revision,
        pins.len(),
    );
    runtime_log::operation(now_ms(), "learning_started", context, || {
        begin_learning_inner(
            &state,
            &device_id,
            &device_profile_id,
            &hardware_profile_id,
            editing_revision,
            pins,
        )
    })
}

#[tauri::command]
fn end_learning(
    state: tauri::State<'_, AppState>,
    device_id: hardware::DeviceId,
) -> Result<AppSnapshot, AppError> {
    let context = device_operation_context(&device_id);
    runtime_log::operation(now_ms(), "learning_ended", context, || {
        end_learning_inner(&state, &device_id)
    })
}

#[tauri::command]
fn preview_device_profile_import(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<ImportPreview, AppError> {
    state
        .workspace
        .read()
        .map_err(|_| state_error("workspace_unavailable"))?
        .preview_profile(Path::new(&path))
}

#[tauri::command]
fn import_device_profile(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<AppSnapshot, AppError> {
    runtime_log::operation(
        now_ms(),
        "device_profile_imported",
        serde_json::json!({}),
        || import_profile_inner(&state, Path::new(&path)),
    )
}

#[tauri::command]
fn export_device_profile(
    state: tauri::State<'_, AppState>,
    id: String,
    path: String,
) -> Result<AppSnapshot, AppError> {
    let context = profile_operation_context(&id);
    runtime_log::operation(now_ms(), "device_profile_exported", context, || {
        export_profile_inner(&state, &id, Path::new(&path))
    })
}

#[tauri::command]
fn delete_device_profile(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<AppSnapshot, AppError> {
    let context = profile_operation_context(&id);
    runtime_log::operation(now_ms(), "device_profile_deleted", context, || {
        delete_profile_inner(&state, &id)
    })
}

#[tauri::command]
fn preview_backup(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<BackupPreview, AppError> {
    state
        .workspace
        .read()
        .map_err(|_| state_error("workspace_unavailable"))?
        .preview_backup(Path::new(&path))
}

#[tauri::command]
fn export_backup(state: tauri::State<'_, AppState>, path: String) -> Result<AppSnapshot, AppError> {
    runtime_log::operation(now_ms(), "backup_exported", serde_json::json!({}), || {
        export_backup_inner(&state, Path::new(&path))
    })
}

#[tauri::command]
fn restore_backup(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<AppSnapshot, AppError> {
    runtime_log::operation(now_ms(), "backup_restored", serde_json::json!({}), || {
        restore_backup_inner(&state, Path::new(&path))
    })
}

type SetupResult = Result<(), Box<dyn std::error::Error>>;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StartupFailure {
    code: String,
    detail: String,
}

impl StartupFailure {
    fn from_error(error: &(dyn std::error::Error + 'static)) -> Self {
        let code = error
            .downcast_ref::<AppError>()
            .map_or_else(|| "startup_failed".into(), |error| error.code.clone());
        Self {
            code,
            detail: error.to_string(),
        }
    }
}

#[derive(Default)]
struct StartupState {
    failure: RwLock<Option<StartupFailure>>,
}

fn settle_setup_result(
    result: SetupResult,
    report_failure: impl FnOnce(&(dyn std::error::Error + 'static)),
) -> bool {
    match result {
        Ok(()) => true,
        Err(error) => {
            report_failure(error.as_ref());
            false
        }
    }
}

fn report_startup_failure(app: &mut tauri::App, error: &(dyn std::error::Error + 'static)) {
    eprintln!("failed to start Kivo: {error}");
    runtime_log::emit_lifecycle(
        runtime_log::RuntimeLogEntry::new(
            now_ms(),
            runtime_log::RuntimeLogLevel::Error,
            "application_startup_failed",
            serde_json::json!({}),
        )
        .with_detail(error.to_string()),
    );
    if let Some(state) = app.try_state::<StartupState>()
        && let Ok(mut failure) = state.failure.write()
    {
        *failure = Some(StartupFailure::from_error(error));
    }
}

#[tauri::command]
fn get_startup_failure(state: tauri::State<'_, StartupState>) -> Option<StartupFailure> {
    state
        .failure
        .read()
        .map(|failure| failure.clone())
        .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.manage(StartupState::default());
            let result: SetupResult = (|| {
                #[cfg(feature = "product-studio")]
                let studio_repo_root = studio::setup(app)?;
                #[cfg(feature = "product-studio")]
                let config_directory = app.path().config_dir()?.join(MAIN_APP_IDENTIFIER);
                #[cfg(not(feature = "product-studio"))]
                let config_directory = app.path().app_config_dir()?;
                let codex_home_fallback = app.path().home_dir()?.join(".codex");
                let app_data_directory = app.path().app_data_dir()?;
                let codex_cursor_store = app_data_directory.join("display/codex-cursors-v1.json");
                fs::create_dir_all(&config_directory)?;
                if let Err(error) = runtime_log::install(app.handle(), &config_directory) {
                    eprintln!("failed to install runtime logger: {error}");
                }
                runtime_log::emit_lifecycle(runtime_log::RuntimeLogEntry::new(
                    now_ms(),
                    runtime_log::RuntimeLogLevel::Info,
                    "application_started",
                    serde_json::json!({"version": env!("CARGO_PKG_VERSION")}),
                ));
                #[cfg(feature = "product-studio")]
                let bundled_profiles = studio_repo_root.map_or_else(
                    || {
                        app.path()
                            .resource_dir()
                            .map(|directory| directory.join("models"))
                    },
                    |repo_root| Ok(repo_root.join("models/prod")),
                )?;
                #[cfg(not(feature = "product-studio"))]
                let bundled_profiles = app.path().resource_dir()?.join("models");
                let workspace = match Workspace::load(&config_directory, &bundled_profiles) {
                    Ok(workspace) => workspace,
                    Err(error) => return Err(error.into()),
                };
                let metrics =
                    match MetricsStore::open(&config_directory.join("data/metrics.sqlite3")) {
                        Ok(metrics) => Some(Arc::new(metrics)),
                        Err(error) => {
                            runtime_log::emit(
                                runtime_log::RuntimeLogEntry::new(
                                    now_ms(),
                                    runtime_log::RuntimeLogLevel::Error,
                                    "metrics_initialization_failed",
                                    serde_json::json!({}),
                                )
                                .with_detail(
                                    runtime_log::metrics_initialization_failure_detail(&error),
                                ),
                            );
                            None
                        }
                    };
                let operation_barrier = Arc::new(RwLock::new(()));
                let workspace = Arc::new(RwLock::new(workspace));
                #[cfg(all(
                    any(target_os = "macos", target_os = "windows"),
                    not(feature = "product-studio")
                ))]
                {
                    let workspace_guard = workspace
                        .read()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    tray::setup(app, &[], &workspace_guard)?;
                }
                let paste = Arc::new(PasteCoordinator::system());
                let launcher = Arc::new(device::SystemWorkerLauncher::new(
                    paste.handle(),
                    metrics.clone(),
                    Arc::clone(&operation_barrier),
                    &config_directory,
                ));
                let providers =
                    built_in_provider_registry(&codex_home_fallback, &codex_cursor_store);
                let renderers = Arc::new(built_in_renderer_registry());
                let (display_snapshot_sender, display_snapshots) =
                    mpsc::channel::<Arc<DisplaySnapshot>>();
                let enumerator: Arc<dyn UsbEnumerator> = Arc::new(coordinator::SystemUsbEnumerator);
                let coordinator =
                    Arc::new(Mutex::new(RuntimeCoordinator::with_paste_and_renderers(
                        Arc::clone(&enumerator),
                        launcher,
                        Arc::clone(&workspace),
                        Some(paste.handle()),
                        Arc::clone(&renderers),
                    )));
                let stop = Arc::new(AtomicBool::new(false));
                let scan_requested = Arc::new(AtomicBool::new(false));
                let (usage_snapshot_sender, usage_snapshots) =
                    mpsc::channel::<Arc<UsageSnapshot>>();
                let (usage, usage_thread) = UsageService::spawn(
                    &app_data_directory,
                    Arc::clone(&stop),
                    usage_snapshot_sender,
                )?;
                let display_thread =
                    DisplayService::spawn(providers, Arc::clone(&stop), display_snapshot_sender)?;
                let coordinator_thread = {
                    let coordinator = Arc::clone(&coordinator);
                    let usage = Arc::clone(&usage);
                    let workspace = Arc::clone(&workspace);
                    let metrics = metrics.clone();
                    let stop = Arc::clone(&stop);
                    let scan_requested = Arc::clone(&scan_requested);
                    let app_handle = app.handle().clone();
                    thread::spawn(move || {
                        let _stop_on_drop = StopOnDrop::new(Arc::clone(&stop));
                        let mut scanner = BackgroundDeviceScanner::new(enumerator);
                        let mut log_inventory = runtime_log::DeviceLogInventory::default();
                        let mut usage_active = false;
                        while !stop.load(Ordering::Relaxed) {
                            if scan_requested.swap(false, Ordering::Relaxed) {
                                scanner.request_scan();
                            }
                            let RuntimePoll {
                                scan,
                                scan_error,
                                events,
                            } = poll_runtime_coordinator(&mut scanner, coordinator.as_ref());
                            let timestamp_ms = now_ms();
                            if let Some(error) = scan_error.as_deref() {
                                for entry in
                                    log_inventory.observe_scan_error(timestamp_ms, Some(error))
                                {
                                    runtime_log::emit(entry);
                                }
                            }
                            if let Some(scan) = scan {
                                for entry in log_inventory.observe_scan_error(timestamp_ms, None) {
                                    runtime_log::emit(entry);
                                }
                                for entry in log_inventory.observe(
                                    timestamp_ms,
                                    &scan.devices,
                                    &scan.candidates,
                                ) {
                                    runtime_log::emit(entry);
                                }
                                #[cfg(any(target_os = "macos", target_os = "windows"))]
                                if let Ok(workspace) = workspace.read() {
                                    tray::update(&app_handle, &scan.devices, &workspace);
                                }
                            }
                            for event in events {
                                let payload =
                                    enrich_runtime_event(&workspace, metrics.as_deref(), event);
                                runtime_log::emit_runtime_event(&payload);
                                let _ = app_handle.emit("runtime-event", payload);
                            }
                            let usage_requested = coordinator
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner())
                                .usage_requested();
                            if usage_requested != usage_active
                                && usage.set_active(usage_requested).is_ok()
                            {
                                usage_active = usage_requested;
                            }
                            if let Some(snapshot) = newest_display_snapshot(&display_snapshots) {
                                coordinator
                                    .lock()
                                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                                    .update_display(snapshot);
                            }
                            if let Some(snapshot) = usage_snapshots.try_iter().last() {
                                coordinator
                                    .lock()
                                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                                    .update_usage(Arc::clone(&snapshot));
                            }
                            thread::sleep(RUNTIME_EVENT_POLL_INTERVAL);
                        }
                        coordinator
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .shutdown();
                    })
                };
                app.manage(AppState {
                    workspace,
                    operation_barrier,
                    metrics,
                    coordinator: Some(coordinator),
                    paste: Some(paste),
                    usage: Some(usage),
                    stop,
                    scan_requested,
                    display_thread: Mutex::new(Some(display_thread)),
                    usage_thread: Mutex::new(Some(usage_thread)),
                    coordinator_thread: Mutex::new(Some(coordinator_thread)),
                });
                runtime_log::emit_lifecycle(runtime_log::RuntimeLogEntry::new(
                    now_ms(),
                    runtime_log::RuntimeLogLevel::Info,
                    "application_ready",
                    serde_json::json!({}),
                ));
                Ok(())
            })();
            settle_setup_result(result, |error| report_startup_failure(app, error));
            Ok(())
        });

    #[cfg(not(feature = "product-studio"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        get_startup_failure,
        get_snapshot,
        retry_candidate,
        save_device_profile,
        create_device_profile,
        duplicate_profile_for_device,
        save_settings,
        save_usage_settings,
        rename_device,
        save_product_configuration,
        select_product_configuration,
        create_product_configuration,
        save_runtime_assignment,
        complete_device_setup,
        clear_runtime_assignment,
        forget_device,
        get_device_metrics,
        begin_learning,
        end_learning,
        preview_device_profile_import,
        import_device_profile,
        export_device_profile,
        delete_device_profile,
        preview_backup,
        export_backup,
        restore_backup,
    ]);

    #[cfg(feature = "product-studio")]
    let builder = builder.invoke_handler(tauri::generate_handler![
        get_startup_failure,
        get_snapshot,
        retry_candidate,
        save_device_profile,
        create_device_profile,
        duplicate_profile_for_device,
        save_settings,
        save_usage_settings,
        rename_device,
        save_product_configuration,
        select_product_configuration,
        create_product_configuration,
        save_runtime_assignment,
        complete_device_setup,
        clear_runtime_assignment,
        forget_device,
        get_device_metrics,
        begin_learning,
        end_learning,
        preview_device_profile_import,
        import_device_profile,
        export_device_profile,
        delete_device_profile,
        preview_backup,
        export_backup,
        restore_backup,
        studio::studio_select_repository,
        studio::studio_get_snapshot,
        studio::studio_load_product,
        studio::studio_validate_product,
        studio::studio_save_product,
        studio::studio_copy_product,
        studio::studio_delete_product,
        studio::studio_build_product,
    ]);

    let app = builder
        .build(tauri::generate_context!())
        .expect("error while building Kivo");

    app.run(|app_handle, event| match event {
        #[cfg(feature = "product-studio")]
        tauri::RunEvent::WindowEvent {
            event: tauri::WindowEvent::CloseRequested { api, .. },
            ..
        } if studio::cancel_active_build_for_shutdown(app_handle) => {
            api.prevent_close();
        }
        #[cfg(not(feature = "product-studio"))]
        tauri::RunEvent::WindowEvent {
            label,
            event: tauri::WindowEvent::CloseRequested { api, .. },
            ..
        } => {
            api.prevent_close();
            if let Some(window) = app_handle.get_webview_window(&label) {
                let _ = window.hide();
            }
        }
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Reopen {
            has_visible_windows: false,
            ..
        } => {
            if let Some(window) = app_handle.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        #[cfg(feature = "product-studio")]
        tauri::RunEvent::ExitRequested { api, .. }
            if studio::cancel_active_build_for_shutdown(app_handle) =>
        {
            api.prevent_exit();
        }
        tauri::RunEvent::ExitRequested { .. } => {
            runtime_log::emit_lifecycle(runtime_log::RuntimeLogEntry::new(
                now_ms(),
                runtime_log::RuntimeLogLevel::Info,
                "application_exit_requested",
                serde_json::json!({}),
            ));
            if let Some(state) = app_handle.try_state::<AppState>() {
                state.stop.store(true, Ordering::Relaxed);
            }
        }
        tauri::RunEvent::Exit => {
            if let Some(state) = app_handle.try_state::<AppState>() {
                state.stop.store(true, Ordering::Relaxed);
                if let Some(display) = state
                    .display_thread
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .take()
                {
                    let _ = display.join();
                }
                if let Some(usage) = state
                    .usage_thread
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .take()
                {
                    let _ = usage.join();
                }
                if let Some(coordinator) = state
                    .coordinator_thread
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .take()
                {
                    let _ = coordinator.join();
                }
                if let Some(paste) = &state.paste {
                    paste.shutdown();
                }
            }
            runtime_log::shutdown_with_entry(runtime_log::RuntimeLogEntry::new(
                now_ms(),
                runtime_log::RuntimeLogLevel::Info,
                "application_stopped",
                serde_json::json!({}),
            ));
        }
        _ => {}
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        coordinator::{
            BootloaderObservation, DeviceWorker, SerialObservation, UsbEnumerator, WorkerCommand,
            WorkerEvent, WorkerLauncher, WorkerStart,
        },
        display::DisplaySnapshot,
        metrics::MetricAttribution,
        profile::{
            ButtonAction, HardwareProfile, InputSource, PROFILE_SCHEMA_VERSION, TriggerActions,
            TriggerSettings,
        },
    };
    use std::{
        collections::BTreeMap,
        path::PathBuf,
        sync::{
            atomic::{AtomicBool as TestAtomicBool, AtomicU64, Ordering as AtomicOrdering},
            mpsc,
        },
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn startup_schema_failure_is_consumed_instead_of_reaching_tauri() {
        let error = AppError::new("unsupported_profile_schema");
        let failure = StartupFailure::from_error(&error);
        let mut reported = None;
        let ready = settle_setup_result(
            Err::<(), Box<dyn std::error::Error>>(Box::new(error)),
            |error| reported = Some(error.to_string()),
        );

        assert!(!ready);
        assert_eq!(reported.as_deref(), Some("unsupported_profile_schema"));
        assert_eq!(failure.code, "unsupported_profile_schema");
        assert_eq!(failure.detail, "unsupported_profile_schema");
    }

    #[test]
    fn repeated_operation_contexts_use_camel_case_identifiers() {
        let device_id =
            hardware::DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "PRIVATE-SERIAL")
                .unwrap();
        let assignment = RuntimeAssignment {
            device_profile_id: "profile-1".into(),
            hardware_profile_id: "hardware-1".into(),
        };

        assert_eq!(
            device_operation_context(&device_id),
            serde_json::json!({"deviceId": device_id})
        );
        assert_eq!(
            profile_operation_context("profile-1"),
            serde_json::json!({"deviceProfileId": "profile-1"})
        );
        assert_eq!(
            assignment_operation_context(&device_id, &assignment),
            serde_json::json!({
                "deviceId": device_id,
                "deviceProfileId": "profile-1",
                "hardwareProfileId": "hardware-1",
            })
        );
    }

    #[test]
    fn create_and_settings_operation_contexts_exclude_requested_names() {
        let clone = CreateDeviceProfileRequest::Clone {
            name: "Private Clone Name".into(),
            source_profile_id: "source-profile".into(),
        };
        let blank = CreateDeviceProfileRequest::Blank {
            name: "Private Blank Name".into(),
            board_profile_id: "board-profile".into(),
        };
        let settings = EditorSettingsPatch {
            schema_version: workspace::SETTINGS_SCHEMA_VERSION,
            editor_profile: None,
            language: Language::EnUs,
        };

        let clone_context = create_profile_operation_context(&clone);
        let blank_context = create_profile_operation_context(&blank);
        assert_eq!(
            clone_context,
            serde_json::json!({"kind": "clone", "sourceProfileId": "source-profile"})
        );
        assert_eq!(
            blank_context,
            serde_json::json!({"kind": "blank", "boardProfileId": "board-profile"})
        );
        assert_eq!(
            settings_operation_context(&settings),
            serde_json::json!({
                "schemaVersion": workspace::SETTINGS_SCHEMA_VERSION,
                "editorProfile": null,
                "language": "en-US",
            })
        );
        assert_eq!(
            settings_operation_context(&EditorSettingsPatch {
                schema_version: workspace::SETTINGS_SCHEMA_VERSION,
                editor_profile: Some("source-profile".into()),
                language: Language::ZhCn,
            }),
            serde_json::json!({
                "schemaVersion": workspace::SETTINGS_SCHEMA_VERSION,
                "editorProfile": "source-profile",
                "language": "zh-CN",
            })
        );
        let serialized = format!("{clone_context}{blank_context}");
        assert!(!serialized.contains("Private Clone Name"));
        assert!(!serialized.contains("Private Blank Name"));
    }

    #[test]
    fn learning_operation_context_counts_without_serializing_pins() {
        let device_id =
            hardware::DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "LEARNING").unwrap();
        let pins = [6, 7, 8];

        let context =
            learning_operation_context(&device_id, "profile-1", "hardware-1", 42, pins.len());

        assert_eq!(
            context,
            serde_json::json!({
                "deviceId": device_id,
                "deviceProfileId": "profile-1",
                "hardwareProfileId": "hardware-1",
                "editingRevision": 42,
                "pinCount": 3,
            })
        );
        assert!(context.get("pins").is_none());
    }

    #[test]
    fn board_summaries_report_sh1106_support() {
        let summaries = BOARD_PROFILES
            .iter()
            .map(BoardProfileSummary::from)
            .map(|summary| serde_json::to_value(summary).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(summaries[0]["supportsOled"], false);
        assert_eq!(summaries[1]["supportsOled"], true);
    }

    #[test]
    fn runtime_event_polling_stays_within_interactive_latency_budget() {
        assert!(
            RUNTIME_EVENT_POLL_INTERVAL + crate::device::SERIAL_COMMAND_POLL_INTERVAL
                <= Duration::from_millis(20)
        );
    }

    #[test]
    fn display_snapshot_drain_returns_only_the_newest_queued_value() {
        let (sender, snapshots) = mpsc::channel();
        let first = Arc::new(DisplaySnapshot {
            items: Vec::new(),
            health: BTreeMap::new(),
        });
        let newest = Arc::new(DisplaySnapshot {
            items: Vec::new(),
            health: BTreeMap::new(),
        });
        sender.send(first).unwrap();
        sender.send(Arc::clone(&newest)).unwrap();

        let drained = newest_display_snapshot(&snapshots).unwrap();

        assert!(Arc::ptr_eq(&drained, &newest));
        assert!(newest_display_snapshot(&snapshots).is_none());
    }

    #[test]
    fn coordinator_exit_guard_sets_stop_on_normal_drop() {
        let stop = Arc::new(AtomicBool::new(false));

        {
            let _guard = StopOnDrop::new(Arc::clone(&stop));
            assert!(!stop.load(Ordering::Relaxed));
        }

        assert!(stop.load(Ordering::Relaxed));
    }

    #[test]
    fn coordinator_exit_guard_sets_stop_during_unwind() {
        let stop = Arc::new(AtomicBool::new(false));
        let unwind_stop = Arc::clone(&stop);

        let _ = std::panic::catch_unwind(move || {
            let _guard = StopOnDrop::new(unwind_stop);
            panic!("test coordinator unwind");
        });

        assert!(stop.load(Ordering::Relaxed));
    }

    #[test]
    fn slow_device_discovery_does_not_block_interactive_polling() {
        let directory = TestDirectory::new();
        let workspace = Workspace::create(&directory.0, vec![product_profile()]).unwrap();
        let coordinator = Mutex::new(RuntimeCoordinator::new(
            Arc::new(EmptyEnumerator),
            Arc::new(UnusedLauncher),
            Arc::new(RwLock::new(workspace)),
        ));
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let enumerator = Arc::new(BlockingEnumerator {
            started: started_tx,
            release: Mutex::new(release_rx),
        });
        let mut scanner = BackgroundDeviceScanner::new(enumerator);

        let poll = poll_runtime_coordinator(&mut scanner, &coordinator);
        assert!(poll.scan.is_none());
        assert!(poll.scan_error.is_none());
        assert!(poll.events.is_empty());
        started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("device scan did not start");

        let started = Instant::now();
        let poll = poll_runtime_coordinator(&mut scanner, &coordinator);
        assert!(poll.scan.is_none());
        assert!(poll.scan_error.is_none());
        assert!(poll.events.is_empty());
        let elapsed = started.elapsed();
        for _ in 0..8 {
            let poll = poll_runtime_coordinator(&mut scanner, &coordinator);
            assert!(poll.scan.is_none());
            assert!(poll.scan_error.is_none());
            assert!(poll.events.is_empty());
        }
        assert_eq!(started_rx.try_recv(), Err(mpsc::TryRecvError::Empty));

        release_tx.send(()).unwrap();
        assert!(
            elapsed <= Duration::from_millis(20),
            "polling blocked for {elapsed:?} while discovery was running"
        );

        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let poll = poll_runtime_coordinator(&mut scanner, &coordinator);
            if let Some(scan) = poll.scan {
                assert!(scan.devices.is_empty());
                assert!(scan.candidates.is_empty());
                assert!(poll.scan_error.is_none());
                assert!(poll.events.is_empty());
                break;
            }
            assert!(poll.scan_error.is_none());
            assert!(poll.events.is_empty());
            if Instant::now() < deadline {
                thread::yield_now();
            } else {
                panic!("device scan did not finish");
            }
        }
    }

    #[test]
    fn device_scan_errors_are_sanitized_in_runtime_poll() {
        let directory = TestDirectory::new();
        let workspace = Workspace::create(&directory.0, vec![product_profile()]).unwrap();
        let coordinator = Mutex::new(RuntimeCoordinator::new(
            Arc::new(EmptyEnumerator),
            Arc::new(UnusedLauncher),
            Arc::new(RwLock::new(workspace)),
        ));
        let mut scanner = BackgroundDeviceScanner::new(Arc::new(FailingEnumerator));

        let first = poll_runtime_coordinator(&mut scanner, &coordinator);
        assert!(first.scan.is_none());
        assert!(first.scan_error.is_none());
        assert!(first.events.is_empty());

        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let poll = poll_runtime_coordinator(&mut scanner, &coordinator);
            if let Some(error) = poll.scan_error {
                assert_eq!(error, "serial_enumeration_failed");
                assert!(poll.scan.is_none());
                assert!(poll.events.is_empty());
                break;
            }
            if Instant::now() < deadline {
                thread::yield_now();
            } else {
                panic!("device scan error was discarded");
            }
        }
    }

    #[test]
    fn scan_requested_during_discovery_runs_immediately_after_completion() {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let enumerator = Arc::new(BlockingEnumerator {
            started: started_tx,
            release: Mutex::new(release_rx),
        });
        let mut scanner = BackgroundDeviceScanner::new(enumerator);

        assert!(scanner.poll().is_none());
        started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("first device scan did not start");
        scanner.request_scan();
        assert!(scanner.poll().is_none());
        assert_eq!(started_rx.try_recv(), Err(mpsc::TryRecvError::Empty));

        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            match scanner.poll() {
                Some(result) => {
                    result.unwrap();
                    break;
                }
                None if Instant::now() < deadline => thread::yield_now(),
                None => panic!("first device scan did not finish"),
            }
        }

        assert!(scanner.poll().is_none());
        started_rx
            .recv_timeout(Duration::from_millis(100))
            .expect("requested device scan did not start immediately");
        release_tx.send(()).unwrap();
    }

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "kivo-command-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT_DIRECTORY.fetch_add(1, AtomicOrdering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn product_profile() -> DeviceProfile {
        DeviceProfile {
            schema_version: PROFILE_SCHEMA_VERSION,
            profile: profile::test_model_layout(),
            snapshot_metadata: None,
            trigger_settings: TriggerSettings::default(),
            hardware_profiles: vec![HardwareProfile {
                id: "esp-primary".into(),
                name: "ESP primary".into(),
                board_profile_id: crate::hardware::YD_ESP32_S3_BOARD_ID.into(),
                debounce_ms: 30,
                ssd1306: None,
                sh1106: None,
                inputs: vec![InputSource::Direct {
                    id: "direct".into(),
                    keys: BTreeMap::from([("UP".into(), 6)]),
                }],
            }],
            actions: BTreeMap::new(),
        }
    }

    fn product_state(directory: &Path, profiles: Vec<DeviceProfile>) -> AppState {
        let workspace = Workspace::create(directory, profiles).unwrap();
        AppState {
            workspace: Arc::new(RwLock::new(workspace)),
            operation_barrier: Arc::new(RwLock::new(())),
            metrics: MetricsStore::open(&directory.join("data/metrics.sqlite3"))
                .ok()
                .map(Arc::new),
            coordinator: None,
            paste: None,
            usage: None,
            stop: Arc::new(AtomicBool::new(false)),
            scan_requested: Arc::new(AtomicBool::new(false)),
            display_thread: Mutex::new(None),
            usage_thread: Mutex::new(None),
            coordinator_thread: Mutex::new(None),
        }
    }

    struct EmptyEnumerator;

    impl UsbEnumerator for EmptyEnumerator {
        fn serial_ports(&self) -> Result<Vec<SerialObservation>, String> {
            Ok(Vec::new())
        }

        fn usb_devices(&self) -> Result<Vec<BootloaderObservation>, String> {
            Ok(Vec::new())
        }
    }

    struct FailingEnumerator;

    impl UsbEnumerator for FailingEnumerator {
        fn serial_ports(&self) -> Result<Vec<SerialObservation>, String> {
            Err("serial discovery unavailable".into())
        }

        fn usb_devices(&self) -> Result<Vec<BootloaderObservation>, String> {
            Ok(Vec::new())
        }
    }

    struct BlockingEnumerator {
        started: mpsc::Sender<()>,
        release: Mutex<mpsc::Receiver<()>>,
    }

    impl UsbEnumerator for BlockingEnumerator {
        fn serial_ports(&self) -> Result<Vec<SerialObservation>, String> {
            self.started.send(()).unwrap();
            self.release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(1))
                .map_err(|error| error.to_string())?;
            Ok(Vec::new())
        }

        fn usb_devices(&self) -> Result<Vec<BootloaderObservation>, String> {
            Ok(Vec::new())
        }
    }

    struct InvalidEnumerator;

    impl UsbEnumerator for InvalidEnumerator {
        fn serial_ports(&self) -> Result<Vec<SerialObservation>, String> {
            Ok(vec![SerialObservation {
                port: "/dev/invalid".into(),
                vid: 0x2e8a,
                pid: 0x102e,
                serial_number: None,
            }])
        }

        fn usb_devices(&self) -> Result<Vec<BootloaderObservation>, String> {
            Ok(Vec::new())
        }
    }

    struct UnusedLauncher;

    impl WorkerLauncher for UnusedLauncher {
        fn start(
            &self,
            _start: WorkerStart,
            _events: mpsc::Sender<WorkerEvent>,
        ) -> Result<Box<dyn DeviceWorker>, String> {
            unreachable!("empty enumeration never starts a worker")
        }
    }

    struct CandidateLauncher;

    impl WorkerLauncher for CandidateLauncher {
        fn start(
            &self,
            _start: WorkerStart,
            _events: mpsc::Sender<WorkerEvent>,
        ) -> Result<Box<dyn DeviceWorker>, String> {
            Err("serial_handshake_timeout".into())
        }
    }

    struct SaveEnumerator;

    impl UsbEnumerator for SaveEnumerator {
        fn serial_ports(&self) -> Result<Vec<SerialObservation>, String> {
            Ok(vec![
                SerialObservation {
                    port: "/dev/save-a".into(),
                    vid: 0x303a,
                    pid: 0x4002,
                    serial_number: Some("SAVE-A".into()),
                },
                SerialObservation {
                    port: "/dev/save-b".into(),
                    vid: 0x303a,
                    pid: 0x4002,
                    serial_number: Some("SAVE-B".into()),
                },
            ])
        }

        fn usb_devices(&self) -> Result<Vec<BootloaderObservation>, String> {
            Ok(Vec::new())
        }
    }

    struct RestoreEnumerator;

    impl UsbEnumerator for RestoreEnumerator {
        fn serial_ports(&self) -> Result<Vec<SerialObservation>, String> {
            Ok(vec![
                SerialObservation {
                    port: "/dev/restore-esp".into(),
                    vid: 0x303a,
                    pid: 0x4002,
                    serial_number: Some("RESTORE-ESP".into()),
                },
                SerialObservation {
                    port: "/dev/restore-rp".into(),
                    vid: 0x2e8a,
                    pid: 0x102e,
                    serial_number: Some("RESTORE-RP".into()),
                },
            ])
        }

        fn usb_devices(&self) -> Result<Vec<BootloaderObservation>, String> {
            Ok(Vec::new())
        }
    }

    #[derive(Default)]
    struct RestoreLifecycle {
        starts: Vec<WorkerStart>,
        stopped: Vec<hardware::DeviceId>,
        joined: Vec<hardware::DeviceId>,
        joins_saw_released_barrier: bool,
    }

    struct RestoreLauncher {
        lifecycle: Arc<Mutex<RestoreLifecycle>>,
        operation_barrier: Arc<RwLock<()>>,
    }

    struct RestoreWorker {
        device_id: hardware::DeviceId,
        lifecycle: Arc<Mutex<RestoreLifecycle>>,
        operation_barrier: Arc<RwLock<()>>,
    }

    impl DeviceWorker for RestoreWorker {
        fn send(&self, _command: WorkerCommand) -> Result<(), String> {
            Ok(())
        }

        fn stop(&mut self) {
            self.lifecycle
                .lock()
                .unwrap()
                .stopped
                .push(self.device_id.clone());
        }

        fn join(&mut self) {
            let barrier_released = self.operation_barrier.try_read().is_ok();
            let mut lifecycle = self.lifecycle.lock().unwrap();
            lifecycle.joined.push(self.device_id.clone());
            lifecycle.joins_saw_released_barrier |= barrier_released;
        }
    }

    impl WorkerLauncher for RestoreLauncher {
        fn start(
            &self,
            start: WorkerStart,
            events: mpsc::Sender<WorkerEvent>,
        ) -> Result<Box<dyn DeviceWorker>, String> {
            self.lifecycle.lock().unwrap().starts.push(start.clone());
            let board = hardware::board_by_id(&start.board_profile_id).unwrap();
            events
                .send(WorkerEvent::HelloValidated {
                    generation: start.generation,
                    device_id: start.device_id.clone(),
                    capabilities: protocol::HelloCapabilities {
                        protocol: 4,
                        controller_family_id: board.family_id.into(),
                        board_profile_id: board.id.into(),
                        firmware_build_id: "restore-test".into(),
                        product_version_id: None,
                        pins: board.safe_pins.to_vec(),
                    },
                    product_definition: None,
                })
                .unwrap();
            Ok(Box::new(RestoreWorker {
                device_id: start.device_id,
                lifecycle: Arc::clone(&self.lifecycle),
                operation_barrier: Arc::clone(&self.operation_barrier),
            }))
        }
    }

    #[derive(Default)]
    struct SaveLauncher {
        commands: Arc<Mutex<BTreeMap<hardware::DeviceId, Vec<WorkerCommand>>>>,
    }

    struct SaveWorker {
        device_id: hardware::DeviceId,
        commands: Arc<Mutex<BTreeMap<hardware::DeviceId, Vec<WorkerCommand>>>>,
    }

    impl DeviceWorker for SaveWorker {
        fn send(&self, command: WorkerCommand) -> Result<(), String> {
            self.commands
                .lock()
                .unwrap()
                .entry(self.device_id.clone())
                .or_default()
                .push(command);
            Ok(())
        }

        fn stop(&mut self) {}

        fn join(&mut self) {}
    }

    impl WorkerLauncher for SaveLauncher {
        fn start(
            &self,
            start: WorkerStart,
            events: mpsc::Sender<WorkerEvent>,
        ) -> Result<Box<dyn DeviceWorker>, String> {
            let board = hardware::board_by_id(&start.board_profile_id).unwrap();
            events
                .send(WorkerEvent::HelloValidated {
                    generation: start.generation,
                    device_id: start.device_id.clone(),
                    capabilities: protocol::HelloCapabilities {
                        protocol: 4,
                        controller_family_id: board.family_id.into(),
                        board_profile_id: board.id.into(),
                        firmware_build_id: "save-test".into(),
                        product_version_id: None,
                        pins: board.safe_pins.to_vec(),
                    },
                    product_definition: None,
                })
                .unwrap();
            Ok(Box::new(SaveWorker {
                device_id: start.device_id,
                commands: Arc::clone(&self.commands),
            }))
        }
    }

    #[test]
    fn workspace_command_saves_profile_without_creating_a_runtime_assignment() {
        let directory = TestDirectory::new();
        let state = product_state(&directory.0, vec![product_profile()]);
        let mut updated = product_profile();
        updated.actions.insert(
            "UP".into(),
            TriggerActions::press(vec![ButtonAction::Paste {
                text: "离线保存".into(),
            }]),
        );

        let snapshot = save_profile_inner(&state, updated.clone()).unwrap();

        assert_eq!(snapshot.device_profiles, vec![updated]);
        assert!(directory.path("data/profiles/red-phone-v1.yaml").exists());
    }

    #[test]
    fn command_boundary_snapshot_is_structured_per_device() {
        let directory = TestDirectory::new();
        let state = product_state(&directory.0, vec![product_profile()]);

        let value = serde_json::to_value(snapshot(&state).unwrap()).unwrap();

        for field in [
            "deviceProfiles",
            "editorProfile",
            "boardProfiles",
            "devices",
            "candidates",
            "homeMetrics",
            "language",
        ] {
            assert!(value.get(field).is_some(), "missing {field}");
        }
        for singleton in ["connection", "supportedGpios", "runtimeError", "learning"] {
            assert!(value.get(singleton).is_none(), "obsolete {singleton}");
        }
        let boards = value["boardProfiles"].as_array().unwrap();
        let rp2040 = boards
            .iter()
            .find(|board| board["id"] == crate::hardware::YD_RP2040_BOARD_ID)
            .unwrap();
        assert_eq!(
            rp2040["controllerFamilyId"],
            crate::hardware::RP2040_FAMILY_ID
        );
        assert_eq!(rp2040["runtimeUsb"], "2e8a:102e");
        assert_eq!(rp2040["bootloaderUsb"], "2e8a:0003");
        assert_eq!(
            rp2040["safePins"],
            serde_json::to_value((0_u8..=23).chain(26..=29).collect::<Vec<_>>()).unwrap()
        );
    }

    #[test]
    fn retry_candidate_command_returns_an_authoritative_snapshot() {
        let directory = TestDirectory::new();
        let mut state = product_state(&directory.0, vec![product_profile()]);
        let mut coordinator = RuntimeCoordinator::new(
            Arc::new(SaveEnumerator),
            Arc::new(CandidateLauncher),
            Arc::clone(&state.workspace),
        );
        coordinator.scan_once().unwrap();
        let id = hardware::DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "SAVE-A").unwrap();
        state.coordinator = Some(Arc::new(Mutex::new(coordinator)));
        assert!(!state.scan_requested.load(AtomicOrdering::Relaxed));

        let snapshot = retry_candidate_inner(&state, &id).unwrap();

        assert!(state.scan_requested.load(AtomicOrdering::Relaxed));
        assert!(snapshot.candidates.iter().any(|candidate| {
            candidate.device_id.as_ref() == Some(&id)
                && candidate.issue == coordinator::CandidateIssue::FirmwareNotResponding
        }));
        let missing =
            hardware::DeviceId::new(crate::hardware::YD_RP2040_BOARD_ID, "MISSING").unwrap();
        assert_eq!(
            retry_candidate_inner(&state, &missing).unwrap_err().code,
            "candidate_not_found"
        );
    }

    #[test]
    fn create_device_profile_command_returns_new_editor_snapshot_without_assignment() {
        let directory = TestDirectory::new();
        let state = product_state(&directory.0, vec![product_profile()]);
        let original = state.workspace.read().unwrap().profiles["red-phone-v1"].clone();

        let snapshot = create_device_profile_inner(
            &state,
            CreateDeviceProfileRequest::Clone {
                name: "Operator Copy".into(),
                source_profile_id: "red-phone-v1".into(),
            },
        )
        .unwrap();

        assert_eq!(snapshot.editor_profile.as_deref(), Some("operator-copy"));
        assert_eq!(
            snapshot
                .device_profiles
                .iter()
                .filter(|profile| profile.profile.id == "operator-copy")
                .count(),
            1
        );
        assert_eq!(
            state.workspace.read().unwrap().profiles["red-phone-v1"],
            original
        );
        assert!(state.workspace.read().unwrap().settings.devices.is_empty());
    }

    #[test]
    fn setup_eligibility_requires_online_valid_runtime_device() {
        assert!(
            validate_setup_eligibility(
                coordinator::ConnectionDimension::Online,
                Some(coordinator::DeviceMode::Runtime),
                IdentityDimension::Valid,
            )
            .is_ok()
        );
        assert_eq!(
            validate_setup_eligibility(
                coordinator::ConnectionDimension::Offline,
                None,
                IdentityDimension::Valid,
            )
            .unwrap_err()
            .code,
            "device_offline"
        );
        assert_eq!(
            validate_setup_eligibility(
                coordinator::ConnectionDimension::Online,
                Some(coordinator::DeviceMode::Bootloader),
                IdentityDimension::Valid,
            )
            .unwrap_err()
            .code,
            "device_not_runtime"
        );
        assert_eq!(
            validate_setup_eligibility(
                coordinator::ConnectionDimension::Online,
                Some(coordinator::DeviceMode::Runtime),
                IdentityDimension::DuplicateIdentity,
            )
            .unwrap_err()
            .code,
            "invalid_device_identity"
        );
    }

    #[test]
    fn online_assignment_rejects_profiles_that_need_newer_firmware() {
        let directory = TestDirectory::new();
        let mut state = product_state(&directory.0, vec![product_profile()]);
        let mut gated_profile = product_profile();
        gated_profile.actions.insert(
            "UP".into(),
            TriggerActions {
                release: vec![ButtonAction::Paste {
                    text: "release".into(),
                }],
                ..TriggerActions::default()
            },
        );
        state
            .workspace
            .write()
            .unwrap()
            .save_profile(gated_profile)
            .unwrap();

        let launcher = Arc::new(SaveLauncher::default());
        let mut coordinator = RuntimeCoordinator::new(
            Arc::new(SaveEnumerator),
            launcher,
            Arc::clone(&state.workspace),
        );
        coordinator.scan_once().unwrap();
        coordinator.drain_worker_events();
        state.coordinator = Some(Arc::new(Mutex::new(coordinator)));

        let device =
            hardware::DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "SAVE-A").unwrap();
        let assignment = RuntimeAssignment {
            device_profile_id: "red-phone-v1".into(),
            hardware_profile_id: "esp-primary".into(),
        };
        let error = save_runtime_assignment_inner(&state, &device, assignment).unwrap_err();
        assert_eq!(error.code, "firmware_update_required");
        assert_eq!(error.params.get("expected"), Some(&"6".to_owned()));
        assert_eq!(
            state.workspace.read().unwrap().settings.devices[&device].runtime_assignment,
            None
        );

        let draft = state.workspace.read().unwrap().profiles["red-phone-v1"].clone();
        let duplicate_error = duplicate_profile_for_device_inner(
            &state,
            DuplicateProfileForDeviceRequest {
                device_id: device.clone(),
                source_profile: draft,
                name: "Upgrade copy".into(),
            },
        )
        .unwrap_err();
        assert_eq!(duplicate_error.code, "firmware_update_required");
        assert_eq!(state.workspace.read().unwrap().profiles.len(), 1);
    }

    #[test]
    fn duplicate_profile_for_device_waits_for_an_in_flight_metric_commit() {
        let directory = TestDirectory::new();
        let state = Arc::new(product_state(&directory.0, vec![product_profile()]));
        let device =
            hardware::DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "DUPLICATE-A").unwrap();
        {
            let mut workspace = state.workspace.write().unwrap();
            workspace.enroll_device(device.clone()).unwrap();
            workspace
                .set_assignment(
                    &device,
                    RuntimeAssignment {
                        device_profile_id: "red-phone-v1".into(),
                        hardware_profile_id: "esp-primary".into(),
                    },
                )
                .unwrap();
        }
        let attribution = MetricAttribution {
            device_id: device.clone(),
            device_name: "Duplicate desk".into(),
            device_profile_id: "red-phone-v1".into(),
            hardware_profile_id: "esp-primary".into(),
        };
        let (press_started_tx, press_started_rx) = mpsc::channel();
        let (release_press_tx, release_press_rx) = mpsc::channel();
        let (press_committed_tx, press_committed_rx) = mpsc::channel();
        let press_state = Arc::clone(&state);
        let press = thread::spawn(move || {
            let _operation = press_state.operation_barrier.read().unwrap();
            press_started_tx.send(()).unwrap();
            release_press_rx.recv().unwrap();
            press_state
                .metrics
                .as_deref()
                .unwrap()
                .record_button_press(&attribution, "UP", 1_720_086_400_000)
                .unwrap();
            press_committed_tx.send(()).unwrap();
        });
        press_started_rx.recv().unwrap();

        let draft = state.workspace.read().unwrap().profiles["red-phone-v1"].clone();
        let duplicate_state = Arc::clone(&state);
        let (duplicate_done_tx, duplicate_done_rx) = mpsc::channel();
        let duplicate = thread::spawn(move || {
            let result = duplicate_profile_for_device_inner(
                &duplicate_state,
                DuplicateProfileForDeviceRequest {
                    device_id: device,
                    source_profile: draft,
                    name: "Metric copy".into(),
                },
            );
            duplicate_done_tx.send(result.map(|_| ())).unwrap();
        });

        assert!(matches!(
            duplicate_done_rx.recv_timeout(Duration::from_secs(1)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        release_press_tx.send(()).unwrap();
        press_committed_rx.recv().unwrap();
        press.join().unwrap();
        duplicate_done_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .unwrap();
        duplicate.join().unwrap();

        state
            .metrics
            .as_deref()
            .unwrap()
            .record_button_press(
                &MetricAttribution {
                    device_id: hardware::DeviceId::new(
                        crate::hardware::YD_ESP32_S3_BOARD_ID,
                        "DUPLICATE-A",
                    )
                    .unwrap(),
                    device_name: "Duplicate desk".into(),
                    device_profile_id: "red-phone-v1".into(),
                    hardware_profile_id: "esp-primary".into(),
                },
                "UP",
                1_720_086_400_001,
            )
            .unwrap();
    }

    #[test]
    fn command_boundary_mutations_and_metrics_target_exactly_one_device() {
        let directory = TestDirectory::new();
        let mut state = product_state(&directory.0, vec![product_profile()]);
        let launcher = Arc::new(SaveLauncher::default());
        let mut coordinator = RuntimeCoordinator::new(
            Arc::new(SaveEnumerator),
            launcher.clone(),
            Arc::clone(&state.workspace),
        );
        coordinator.scan_once().unwrap();
        coordinator.drain_worker_events();
        let a = hardware::DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "SAVE-A").unwrap();
        let b = hardware::DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "SAVE-B").unwrap();
        state.coordinator = Some(Arc::new(Mutex::new(coordinator)));
        launcher.commands.lock().unwrap().clear();
        let assignment = workspace::RuntimeAssignment {
            device_profile_id: "red-phone-v1".into(),
            hardware_profile_id: "esp-primary".into(),
        };

        let completed =
            complete_device_setup_inner(&state, &a, "Setup A".into(), assignment.clone()).unwrap();
        assert_eq!(
            state.workspace.read().unwrap().settings.devices[&a].name,
            "Setup A"
        );
        assert_eq!(
            state.workspace.read().unwrap().settings.devices[&a].runtime_assignment,
            Some(assignment.clone())
        );
        assert_ne!(
            state.workspace.read().unwrap().settings.devices[&b].name,
            "Setup A"
        );
        assert_eq!(
            state.workspace.read().unwrap().settings.devices[&b].runtime_assignment,
            None
        );
        assert!(completed.devices.iter().any(|device| {
            device.device_id == a
                && device.name == "Setup A"
                && device.runtime_assignment.as_ref() == Some(&assignment)
        }));
        assert!(matches!(
            launcher.commands.lock().unwrap().get(&a).unwrap().last(),
            Some(WorkerCommand::Reconfigure { .. })
        ));
        assert!(launcher.commands.lock().unwrap().get(&b).is_none());

        let assigned = save_runtime_assignment_inner(&state, &a, assignment.clone()).unwrap();
        let assigned_json = serde_json::to_value(&assigned).unwrap();
        let assigned_device = assigned_json["devices"]
            .as_array()
            .unwrap()
            .iter()
            .find(|device| device["deviceId"] == a.as_str())
            .unwrap();
        assert_eq!(assigned_device["hardwareSerial"], "SAVE-A");
        assert!(assigned_device.get("capabilities").is_some());
        assert!(assigned_device.get("rawSerial").is_none());
        assert!(assigned_device.get("pins").is_none());
        assert!(assigned_device.get("latestError").is_some());
        assert_eq!(
            state.workspace.read().unwrap().settings.devices[&a].runtime_assignment,
            Some(assignment.clone())
        );
        assert_eq!(
            state.workspace.read().unwrap().settings.devices[&b].runtime_assignment,
            None
        );
        assert!(matches!(
            launcher.commands.lock().unwrap().get(&a).unwrap().last(),
            Some(WorkerCommand::Reconfigure { .. })
        ));
        assert!(launcher.commands.lock().unwrap().get(&b).is_none());

        launcher.commands.lock().unwrap().clear();
        let invalid_assignment = workspace::RuntimeAssignment {
            device_profile_id: "red-phone-v1".into(),
            hardware_profile_id: "missing".into(),
        };
        assert_eq!(
            save_runtime_assignment_inner(&state, &a, invalid_assignment)
                .unwrap_err()
                .code,
            "unknown_hardware_profile"
        );
        assert!(launcher.commands.lock().unwrap().is_empty());
        assert_eq!(
            state.workspace.read().unwrap().settings.devices[&a].runtime_assignment,
            Some(assignment.clone())
        );

        rename_device_inner(&state, &a, "Primary".into()).unwrap();
        assert_eq!(
            state.workspace.read().unwrap().settings.devices[&a].name,
            "Primary"
        );
        assert_ne!(
            state.workspace.read().unwrap().settings.devices[&b].name,
            "Primary"
        );

        let timestamp = now_ms();
        state
            .metrics
            .as_deref()
            .unwrap()
            .record_button_press(
                &MetricAttribution {
                    device_id: a.clone(),
                    device_name: "Archived Primary".into(),
                    device_profile_id: "archived-profile".into(),
                    hardware_profile_id: "archived-hardware".into(),
                },
                "UP",
                timestamp.saturating_sub(1),
            )
            .unwrap();
        for (device_id, button_id) in [(&a, "UP"), (&b, "OTHER")] {
            state
                .metrics
                .as_deref()
                .unwrap()
                .record_button_press(
                    &MetricAttribution {
                        device_id: device_id.clone(),
                        device_name: device_id.hardware_serial().into(),
                        device_profile_id: "red-phone-v1".into(),
                        hardware_profile_id: "esp-primary".into(),
                    },
                    button_id,
                    timestamp,
                )
                .unwrap();
        }
        let metrics = get_device_metrics_inner(&state, &a).unwrap();
        assert_eq!(metrics.total_presses, 2);
        assert_eq!(metrics.logs.len(), 2);
        assert_eq!(metrics.logs[1].device_name, "Archived Primary");
        assert_eq!(metrics.logs[1].device_profile_id, "archived-profile");
        assert_eq!(metrics.logs[1].hardware_profile_id, "archived-hardware");
        assert_eq!(metrics.top_button.unwrap().button_id, "UP");

        begin_learning_inner(&state, &a, "red-phone-v1", "esp-primary", 42, vec![6]).unwrap();
        assert!(matches!(
            launcher.commands.lock().unwrap().get(&a).unwrap().last(),
            Some(WorkerCommand::BeginLearning(target)) if target.editing_revision == 42
        ));
        end_learning_inner(&state, &a).unwrap();
        assert!(matches!(
            launcher.commands.lock().unwrap().get(&a).unwrap().last(),
            Some(WorkerCommand::EndLearning { .. })
        ));

        clear_runtime_assignment_inner(&state, &a).unwrap();
        assert_eq!(
            state.workspace.read().unwrap().settings.devices[&a].runtime_assignment,
            None
        );
        assert_eq!(
            state.workspace.read().unwrap().settings.devices[&b].runtime_assignment,
            None
        );
        let unassigned_metrics = get_device_metrics_inner(&state, &a).unwrap();
        assert_eq!(unassigned_metrics.total_presses, 2);
        assert_eq!(unassigned_metrics.logs.len(), 2);

        assert_eq!(
            forget_device_inner(&state, &b).unwrap_err().code,
            "device_online"
        );
        state.coordinator = None;
        forget_device_inner(&state, &b).unwrap();
        let devices = &state.workspace.read().unwrap().settings.devices;
        assert!(devices.contains_key(&a));
        assert!(!devices.contains_key(&b));
    }

    #[test]
    fn invalid_identity_candidate_cannot_enter_device_commands() {
        let directory = TestDirectory::new();
        let mut state = product_state(&directory.0, vec![product_profile()]);
        let mut coordinator = RuntimeCoordinator::new(
            Arc::new(InvalidEnumerator),
            Arc::new(UnusedLauncher),
            Arc::clone(&state.workspace),
        );
        coordinator.scan_once().unwrap();
        assert_eq!(coordinator.candidates().len(), 1);
        assert!(coordinator.candidates()[0].device_id.is_none());
        state.coordinator = Some(Arc::new(Mutex::new(coordinator)));
        let unregistered =
            hardware::DeviceId::new(crate::hardware::YD_RP2040_BOARD_ID, "NOT-ENROLLED").unwrap();

        assert_eq!(
            rename_device_inner(&state, &unregistered, "Nope".into())
                .unwrap_err()
                .code,
            "unknown_device"
        );
        assert_eq!(
            save_runtime_assignment_inner(
                &state,
                &unregistered,
                workspace::RuntimeAssignment {
                    device_profile_id: "red-phone-v1".into(),
                    hardware_profile_id: "esp-primary".into(),
                },
            )
            .unwrap_err()
            .code,
            "unknown_device"
        );
        assert_eq!(
            complete_device_setup_inner(
                &state,
                &unregistered,
                "Candidate".into(),
                workspace::RuntimeAssignment {
                    device_profile_id: "red-phone-v1".into(),
                    hardware_profile_id: "esp-primary".into(),
                },
            )
            .unwrap_err()
            .code,
            "unknown_device"
        );
        assert_eq!(snapshot(&state).unwrap().candidates.len(), 1);
    }

    #[test]
    fn runtime_event_home_update_is_editor_profile_aggregate_only() {
        let directory = TestDirectory::new();
        let state = product_state(&directory.0, vec![product_profile()]);
        let a = hardware::DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "EVENT-A").unwrap();
        let b = hardware::DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "EVENT-B").unwrap();
        let timestamp = now_ms();
        for device_id in [&a, &b] {
            state
                .metrics
                .as_deref()
                .unwrap()
                .record_button_press(
                    &MetricAttribution {
                        device_id: device_id.clone(),
                        device_name: device_id.hardware_serial().into(),
                        device_profile_id: "red-phone-v1".into(),
                        hardware_profile_id: "esp-primary".into(),
                    },
                    "UP",
                    timestamp,
                )
                .unwrap();
        }
        let mut activity = device::RuntimeActivity::new("input_state");
        activity.input = Some(protocol::PhysicalInput::Direct { gpio: 6 });
        activity.pressed = Some(true);
        let event = coordinator::RuntimeEvent {
            timestamp_ms: timestamp,
            level: coordinator::EventLevel::Info,
            device_id: a,
            raw_serial: "EVENT-A".into(),
            controller_family_id: crate::hardware::ESP32S3_FAMILY_ID.into(),
            board_profile_id: crate::hardware::YD_ESP32_S3_BOARD_ID.into(),
            port: Some("/dev/event-a".into()),
            device_profile_id: Some("red-phone-v1".into()),
            hardware_profile_id: Some("esp-primary".into()),
            home_update: None,
            activity,
        };

        let matching = enrich_runtime_event(&state.workspace, state.metrics.as_deref(), event);
        assert_eq!(matching.home_update.as_ref().unwrap().total_presses, 2);

        let blocked = enrich_runtime_event(
            &state.workspace,
            state.metrics.as_deref(),
            coordinator::RuntimeEvent {
                activity: device::RuntimeActivity::new("feature_disabled"),
                home_update: None,
                ..matching.clone()
            },
        );
        assert!(blocked.home_update.is_some());

        let mismatched = enrich_runtime_event(
            &state.workspace,
            state.metrics.as_deref(),
            coordinator::RuntimeEvent {
                device_profile_id: Some("another-profile".into()),
                home_update: None,
                ..matching
            },
        );
        assert!(mismatched.home_update.is_none());
    }

    #[test]
    fn live_update_save_fans_changed_hardware_out_to_every_exact_assignment() {
        let directory = TestDirectory::new();
        let mut state = product_state(&directory.0, vec![product_profile()]);
        let launcher = Arc::new(SaveLauncher::default());
        let mut coordinator = RuntimeCoordinator::new(
            Arc::new(SaveEnumerator),
            launcher.clone(),
            Arc::clone(&state.workspace),
        );
        coordinator.scan_once().unwrap();
        coordinator.drain_worker_events();
        let a = hardware::DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "SAVE-A").unwrap();
        let b = hardware::DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "SAVE-B").unwrap();
        {
            let mut workspace = state.workspace.write().unwrap();
            for id in [&a, &b] {
                workspace
                    .set_assignment(
                        id,
                        workspace::RuntimeAssignment {
                            device_profile_id: "red-phone-v1".into(),
                            hardware_profile_id: "esp-primary".into(),
                        },
                    )
                    .unwrap();
            }
        }
        coordinator.sync_profiles();
        launcher.commands.lock().unwrap().clear();
        state.coordinator = Some(Arc::new(Mutex::new(coordinator)));
        let mut updated = product_profile();
        updated.hardware_profiles[0].debounce_ms = 55;

        save_profile_inner(&state, updated).unwrap();

        let commands = launcher.commands.lock().unwrap();
        for id in [&a, &b] {
            assert!(matches!(
                commands.get(id).unwrap().as_slice(),
                [WorkerCommand::Reconfigure { revision, .. }] if *revision > 0
            ));
        }
    }

    #[test]
    fn live_update_save_sends_action_only_snapshot_to_assigned_workers() {
        let directory = TestDirectory::new();
        let mut state = product_state(&directory.0, vec![product_profile()]);
        let launcher = Arc::new(SaveLauncher::default());
        let mut coordinator = RuntimeCoordinator::new(
            Arc::new(SaveEnumerator),
            launcher.clone(),
            Arc::clone(&state.workspace),
        );
        coordinator.scan_once().unwrap();
        coordinator.drain_worker_events();
        let device_id =
            hardware::DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "SAVE-A").unwrap();
        {
            let mut workspace = state.workspace.write().unwrap();
            workspace
                .set_assignment(
                    &device_id,
                    workspace::RuntimeAssignment {
                        device_profile_id: "red-phone-v1".into(),
                        hardware_profile_id: "esp-primary".into(),
                    },
                )
                .unwrap();
        }
        coordinator.sync_profiles();
        launcher.commands.lock().unwrap().clear();
        state.coordinator = Some(Arc::new(Mutex::new(coordinator)));
        let mut updated = product_profile();
        updated.actions.insert(
            "UP".into(),
            TriggerActions::press(vec![ButtonAction::Paste {
                text: "online paste".into(),
            }]),
        );

        save_profile_inner(&state, updated).unwrap();

        let commands = launcher.commands.lock().unwrap();
        let [WorkerCommand::UpdateSnapshot(Some(snapshot))] =
            commands.get(&device_id).unwrap().as_slice()
        else {
            panic!("expected one action-only snapshot update");
        };
        assert_eq!(
            snapshot
                .profile
                .actions
                .get("UP")
                .map(|triggers| &triggers.press),
            Some(&vec![ButtonAction::Paste {
                text: "online paste".into(),
            }])
        );
    }

    #[test]
    fn live_update_reconfigures_when_an_action_requires_newer_firmware() {
        let directory = TestDirectory::new();
        let mut state = product_state(&directory.0, vec![product_profile()]);
        let launcher = Arc::new(SaveLauncher::default());
        let mut coordinator = RuntimeCoordinator::new(
            Arc::new(SaveEnumerator),
            launcher.clone(),
            Arc::clone(&state.workspace),
        );
        coordinator.scan_once().unwrap();
        coordinator.drain_worker_events();
        let device_id =
            hardware::DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "SAVE-A").unwrap();
        {
            let mut workspace = state.workspace.write().unwrap();
            workspace
                .set_assignment(
                    &device_id,
                    RuntimeAssignment {
                        device_profile_id: "red-phone-v1".into(),
                        hardware_profile_id: "esp-primary".into(),
                    },
                )
                .unwrap();
        }
        coordinator.sync_profiles();
        launcher.commands.lock().unwrap().clear();
        state.coordinator = Some(Arc::new(Mutex::new(coordinator)));
        let mut updated = product_profile();
        updated.actions.insert(
            "UP".into(),
            TriggerActions {
                release: vec![ButtonAction::Paste {
                    text: "requires protocol 6".into(),
                }],
                ..TriggerActions::default()
            },
        );

        save_profile_inner(&state, updated).unwrap();

        let commands = launcher.commands.lock().unwrap();
        assert!(matches!(
            commands.get(&device_id).unwrap().as_slice(),
            [WorkerCommand::Reconfigure {
                snapshot: Some(snapshot),
                revision: _,
            }] if snapshot.profile.minimum_protocol_version() == crate::protocol::ACTION_RUN_PROTOCOL_VERSION
        ));
    }

    #[test]
    fn workspace_command_saves_only_editor_preferences() {
        let directory = TestDirectory::new();
        let state = product_state(&directory.0, vec![product_profile()]);
        let id =
            hardware::DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "ABCDEF123456").unwrap();
        {
            let mut workspace = state.workspace.write().unwrap();
            workspace.enroll_device(id.clone()).unwrap();
            workspace
                .set_assignment(
                    &id,
                    workspace::RuntimeAssignment {
                        device_profile_id: "red-phone-v1".into(),
                        hardware_profile_id: "esp-primary".into(),
                    },
                )
                .unwrap();
        }
        let devices_before = state.workspace.read().unwrap().settings.devices.clone();

        let snapshot = save_settings_inner(
            &state,
            EditorSettingsPatch {
                schema_version: workspace::SETTINGS_SCHEMA_VERSION,
                editor_profile: Some("red-phone-v1".into()),
                language: Language::EnUs,
            },
        )
        .unwrap();

        assert_eq!(snapshot.language, Language::EnUs);
        assert_eq!(
            state.workspace.read().unwrap().settings.devices,
            devices_before
        );
    }

    #[test]
    fn snapshot_includes_editor_profile_metrics() {
        let directory = TestDirectory::new();
        let state = product_state(&directory.0, vec![product_profile()]);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let device_id =
            hardware::DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "ABCDEF123456").unwrap();
        state
            .metrics
            .as_ref()
            .unwrap()
            .record_button_press(
                &MetricAttribution {
                    device_id,
                    device_name: "Desk".into(),
                    device_profile_id: "red-phone-v1".into(),
                    hardware_profile_id: "esp-primary".into(),
                },
                "UP",
                timestamp,
            )
            .unwrap();

        let snapshot = snapshot(&state).unwrap();

        assert_eq!(snapshot.home_metrics.as_ref().unwrap().today_presses, 1);
        assert_eq!(
            snapshot.home_metrics.unwrap().top_button.unwrap().button_id,
            "UP"
        );
    }

    #[test]
    fn workspace_command_deletes_last_profile_and_clears_editor() {
        let directory = TestDirectory::new();
        let state = product_state(&directory.0, vec![product_profile()]);

        let snapshot = delete_profile_inner(&state, "red-phone-v1").unwrap();

        assert!(snapshot.device_profiles.is_empty());
        assert_eq!(snapshot.editor_profile, None);
    }

    #[test]
    fn workspace_command_restore_replaces_runtime_snapshot() {
        let directory = TestDirectory::new();
        let state = product_state(&directory.0, vec![product_profile()]);
        let backup = directory.path("backup.yaml");
        let timestamp = now_ms();
        let attribution = MetricAttribution {
            device_id: hardware::DeviceId::new(
                crate::hardware::YD_ESP32_S3_BOARD_ID,
                "ABCDEF123456",
            )
            .unwrap(),
            device_name: "Desk".into(),
            device_profile_id: "red-phone-v1".into(),
            hardware_profile_id: "esp-primary".into(),
        };
        state
            .metrics
            .as_deref()
            .unwrap()
            .record_button_press(&attribution, "UP", timestamp)
            .unwrap();
        state
            .workspace
            .read()
            .unwrap()
            .export_backup(&backup, state.metrics.as_deref().unwrap())
            .unwrap();
        state
            .metrics
            .as_deref()
            .unwrap()
            .record_button_press(&attribution, "DOWN", timestamp + 1)
            .unwrap();
        delete_profile_inner(&state, "red-phone-v1").unwrap();

        let snapshot = restore_backup_inner(&state, &backup).unwrap();

        assert_eq!(snapshot.editor_profile.as_deref(), Some("red-phone-v1"));
        assert_eq!(snapshot.home_metrics.as_ref().unwrap().total_presses, 1);
        assert_eq!(
            snapshot.home_metrics.unwrap().top_button.unwrap().button_id,
            "UP"
        );
    }

    #[test]
    fn failed_restore_does_not_replace_coordinator_revision_or_worker_state() {
        let directory = TestDirectory::new();
        let mut state = product_state(&directory.0, vec![product_profile()]);
        let launcher = Arc::new(SaveLauncher::default());
        let mut coordinator = RuntimeCoordinator::new(
            Arc::new(SaveEnumerator),
            launcher.clone(),
            Arc::clone(&state.workspace),
        );
        coordinator.scan_once().unwrap();
        coordinator.drain_worker_events();
        let device_id =
            hardware::DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "SAVE-A").unwrap();
        state.coordinator = Some(Arc::new(Mutex::new(coordinator)));
        save_runtime_assignment_inner(
            &state,
            &device_id,
            workspace::RuntimeAssignment {
                device_profile_id: "red-phone-v1".into(),
                hardware_profile_id: "esp-primary".into(),
            },
        )
        .unwrap();
        launcher.commands.lock().unwrap().clear();
        let before = state.workspace.read().unwrap().settings.clone();
        let malformed = directory.path("malformed-backup.yaml");
        fs::write(&malformed, "schema_version: 2\nsettings: [\n").unwrap();

        assert!(restore_backup_inner(&state, &malformed).is_err());

        assert_eq!(state.workspace.read().unwrap().settings, before);
        assert!(launcher.commands.lock().unwrap().is_empty());
        let device = snapshot(&state)
            .unwrap()
            .devices
            .into_iter()
            .find(|device| device.device_id == device_id)
            .unwrap();
        assert_eq!(device.assignment, coordinator::AssignmentDimension::Valid);
    }

    #[test]
    fn successful_restore_offlines_all_workers_and_rejects_old_generation_events() {
        let directory = TestDirectory::new();
        let mut state = product_state(&directory.0, vec![product_profile()]);
        let lifecycle = Arc::new(Mutex::new(RestoreLifecycle::default()));
        let launcher = Arc::new(RestoreLauncher {
            lifecycle: Arc::clone(&lifecycle),
            operation_barrier: Arc::clone(&state.operation_barrier),
        });
        let mut coordinator = RuntimeCoordinator::new(
            Arc::new(RestoreEnumerator),
            launcher,
            Arc::clone(&state.workspace),
        );
        coordinator.scan_once().unwrap();
        coordinator.drain_worker_events();
        let backup = directory.path("live-backup.yaml");
        export_backup_inner(&state, &backup).unwrap();
        let old_generation = lifecycle.lock().unwrap().starts[0].generation;
        state.coordinator = Some(Arc::new(Mutex::new(coordinator)));

        let restored_snapshot = restore_backup_inner(&state, &backup).unwrap();

        assert_eq!(restored_snapshot.devices.len(), 2);
        assert!(restored_snapshot.devices.iter().all(|device| {
            device.connection == coordinator::ConnectionDimension::Offline
                && device.runtime == coordinator::RuntimeDimension::Inactive
                && device.mode.is_none()
                && device.pins.is_empty()
                && device.learning.is_none()
        }));
        let lifecycle_after_restore = lifecycle.lock().unwrap();
        assert_eq!(lifecycle_after_restore.stopped.len(), 2);
        assert_eq!(lifecycle_after_restore.joined.len(), 2);
        assert!(lifecycle_after_restore.joins_saw_released_barrier);
        drop(lifecycle_after_restore);

        let esp =
            hardware::DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "RESTORE-ESP").unwrap();
        let stale = state
            .coordinator
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .handle_worker_event(WorkerEvent::Activity {
                generation: old_generation,
                device_id: esp.clone(),
                context: coordinator::RuntimeEventContext {
                    timestamp_ms: 1,
                    port: Some("/dev/restore-esp".into()),
                    device_profile_id: None,
                    hardware_profile_id: None,
                },
                activity: device::RuntimeActivity::new("topology_rejected"),
            });
        assert!(stale.is_none());
        assert_eq!(
            snapshot(&state)
                .unwrap()
                .devices
                .into_iter()
                .find(|device| device.device_id == esp)
                .unwrap()
                .runtime,
            coordinator::RuntimeDimension::Inactive
        );

        let mut coordinator = state.coordinator.as_ref().unwrap().lock().unwrap();
        coordinator.scan_once().unwrap();
        coordinator.drain_worker_events();
        assert!(
            coordinator
                .devices()
                .iter()
                .all(|device| device.connection == coordinator::ConnectionDimension::Online)
        );
        let lifecycle = lifecycle.lock().unwrap();
        assert_eq!(lifecycle.starts.len(), 4);
        assert!(
            lifecycle.starts[2..]
                .iter()
                .all(|start| start.generation > old_generation)
        );
    }

    #[test]
    fn restore_waits_for_an_in_flight_metric_commit_before_swapping() {
        let directory = TestDirectory::new();
        let state = Arc::new(product_state(&directory.0, vec![product_profile()]));
        let backup = directory.path("backup.yaml");
        export_backup_inner(&state, &backup).unwrap();
        let attribution = MetricAttribution {
            device_id: hardware::DeviceId::new(
                crate::hardware::YD_ESP32_S3_BOARD_ID,
                "ABCDEF123456",
            )
            .unwrap(),
            device_name: "Desk".into(),
            device_profile_id: "red-phone-v1".into(),
            hardware_profile_id: "esp-primary".into(),
        };
        let (press_started_tx, press_started_rx) = mpsc::channel();
        let (release_press_tx, release_press_rx) = mpsc::channel();
        let (press_committed_tx, press_committed_rx) = mpsc::channel();
        let press_state = Arc::clone(&state);
        let press = thread::spawn(move || {
            let _operation = press_state.operation_barrier.read().unwrap();
            press_started_tx.send(()).unwrap();
            release_press_rx.recv().unwrap();
            press_state
                .metrics
                .as_deref()
                .unwrap()
                .record_button_press(&attribution, "UP", 1_720_086_400_000)
                .unwrap();
            press_committed_tx.send(()).unwrap();
        });
        press_started_rx.recv().unwrap();
        let restore_started = Arc::new(TestAtomicBool::new(false));
        let restore_finished = Arc::new(TestAtomicBool::new(false));
        let restore_state = Arc::clone(&state);
        let restore_backup = backup.clone();
        let restore_started_thread = Arc::clone(&restore_started);
        let restore_finished_thread = Arc::clone(&restore_finished);
        let restore = thread::spawn(move || {
            restore_started_thread.store(true, AtomicOrdering::SeqCst);
            let result = restore_backup_inner(&restore_state, &restore_backup);
            restore_finished_thread.store(true, AtomicOrdering::SeqCst);
            result
        });
        while !restore_started.load(AtomicOrdering::SeqCst) {
            thread::yield_now();
        }

        assert!(!restore_finished.load(AtomicOrdering::SeqCst));
        release_press_tx.send(()).unwrap();
        press_committed_rx.recv().unwrap();
        press.join().unwrap();
        restore.join().unwrap().unwrap();
        assert!(restore_finished.load(AtomicOrdering::SeqCst));
    }

    #[test]
    fn restore_barrier_prevents_a_new_metric_commit_from_starting() {
        let directory = TestDirectory::new();
        let state = Arc::new(product_state(&directory.0, vec![product_profile()]));
        let restore_operation = state.operation_barrier.write().unwrap();
        let commit_started = Arc::new(TestAtomicBool::new(false));
        let (commit_attempted_tx, commit_attempted_rx) = mpsc::channel();
        let commit_state = Arc::clone(&state);
        let commit_started_thread = Arc::clone(&commit_started);
        let commit = thread::spawn(move || {
            commit_attempted_tx.send(()).unwrap();
            let _operation = commit_state.operation_barrier.read().unwrap();
            commit_started_thread.store(true, AtomicOrdering::SeqCst);
            let attribution = MetricAttribution {
                device_id: hardware::DeviceId::new(
                    crate::hardware::YD_ESP32_S3_BOARD_ID,
                    "ABCDEF123456",
                )
                .unwrap(),
                device_name: "Desk".into(),
                device_profile_id: "red-phone-v1".into(),
                hardware_profile_id: "esp-primary".into(),
            };
            commit_state
                .metrics
                .as_deref()
                .unwrap()
                .record_button_press(&attribution, "UP", 1_720_086_400_000)
                .unwrap();
        });
        commit_attempted_rx.recv().unwrap();

        assert!(!commit_started.load(AtomicOrdering::SeqCst));
        drop(restore_operation);
        commit.join().unwrap();
        assert!(commit_started.load(AtomicOrdering::SeqCst));
    }

    #[test]
    fn restore_waiting_for_coordinator_does_not_hold_the_operation_barrier() {
        let directory = TestDirectory::new();
        let mut state = product_state(&directory.0, vec![product_profile()]);
        let backup = directory.path("backup.yaml");
        export_backup_inner(&state, &backup).unwrap();
        let coordinator = Arc::new(Mutex::new(RuntimeCoordinator::new(
            Arc::new(EmptyEnumerator),
            Arc::new(UnusedLauncher),
            Arc::clone(&state.workspace),
        )));
        state.coordinator = Some(Arc::clone(&coordinator));
        let state = Arc::new(state);
        let coordinator_guard = coordinator.lock().unwrap();
        let restore_state = Arc::clone(&state);
        let (started_tx, started_rx) = mpsc::channel();
        let restore = thread::spawn(move || {
            started_tx.send(()).unwrap();
            restore_backup_inner(&restore_state, &backup)
        });
        started_rx.recv().unwrap();
        thread::sleep(Duration::from_millis(50));

        let barrier_available = state.operation_barrier.try_read().is_ok();
        drop(coordinator_guard);
        restore.join().unwrap().unwrap();

        assert!(barrier_available);
    }

    #[test]
    fn workspace_command_import_replaces_same_id() {
        let directory = TestDirectory::new();
        let state = product_state(&directory.0, vec![product_profile()]);
        let mut replacement = product_profile();
        replacement.profile.name = "替换型号".into();
        let path = directory.path("replacement.yaml");
        fs::write(&path, serde_yaml_ng::to_string(&replacement).unwrap()).unwrap();

        let snapshot = import_profile_inner(&state, &path).unwrap();

        assert_eq!(snapshot.device_profiles, vec![replacement]);
    }

    #[test]
    fn settings_reject_unknown_language() {
        assert!(
            serde_yaml_ng::from_str::<EditorSettingsPatch>(
                "schema_version: 2\neditor_profile: null\nlanguage: fr-FR\n"
            )
            .is_err()
        );
    }
}
