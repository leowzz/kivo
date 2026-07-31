mod coordinator;
mod device;
pub mod hardware;
mod metrics;
mod model;
mod paste;
mod profile;
mod protocol;
mod storage;
#[cfg(target_os = "macos")]
mod tray;
mod workspace;

use coordinator::{
    CandidateStatus, DeviceStatus, IdentityDimension, RuntimeCoordinator, RuntimeEvent,
    WorkspaceRevision,
};
use hardware::{BOARD_PROFILES, BoardProfile};
use metrics::{HomeMetricsSnapshot, MetricsStore};
use paste::PasteCoordinator;
use profile::DeviceProfile;
use serde::Serialize;
use std::{
    fs,
    path::Path,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager};
use workspace::{
    AppError, AssignmentResolution, BackupPreview, EditorSettingsPatch, ImportPreview, Language,
    RuntimeAssignment, Workspace,
};

struct AppState {
    workspace: Arc<RwLock<Workspace>>,
    operation_barrier: Arc<RwLock<()>>,
    metrics: Option<Arc<MetricsStore>>,
    coordinator: Option<Arc<Mutex<RuntimeCoordinator>>>,
    paste: Option<Arc<PasteCoordinator>>,
    stop: Arc<AtomicBool>,
    coordinator_thread: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppSnapshot {
    device_profiles: Vec<DeviceProfile>,
    editor_profile: Option<String>,
    board_profiles: Vec<BoardProfileSummary>,
    devices: Vec<DeviceStatus>,
    candidates: Vec<CandidateStatus>,
    language: Language,
    home_metrics: Option<HomeMetricsSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct BoardProfileSummary {
    id: String,
    controller_family_id: String,
    display_name: String,
    runtime_usb: String,
    bootloader_usb: Option<String>,
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
    let matches_editor = event.device_profile_id.is_some()
        && event.device_profile_id == editor_profile
        && event.activity.code == "input_state"
        && event.activity.pressed == Some(true);
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
    let editor_profile = workspace.settings.editor_profile.clone();
    let language = workspace.settings.language;
    drop(workspace);
    drop(coordinator);
    let home_metrics = state.metrics.as_ref().and_then(|metrics| {
        editor_profile
            .as_deref()
            .and_then(|profile_id| metrics.home_snapshot(profile_id, None, now_ms()).ok())
    });
    Ok(AppSnapshot {
        device_profiles,
        editor_profile,
        board_profiles: BOARD_PROFILES
            .iter()
            .map(BoardProfileSummary::from)
            .collect(),
        devices,
        candidates,
        language,
        home_metrics,
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

fn save_settings_inner(
    state: &AppState,
    settings: EditorSettingsPatch,
) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, move |workspace, _| workspace.save_settings(settings))
}

fn import_profile_inner(state: &AppState, path: &Path) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, |workspace, _| workspace.import_profile(path))
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

fn save_runtime_assignment_inner(
    state: &AppState,
    device_id: &hardware::DeviceId,
    assignment: RuntimeAssignment,
) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, move |workspace, coordinator| {
        require_addressable_identity(coordinator, device_id)?;
        workspace.set_assignment(device_id, assignment)
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
    let metrics = state
        .metrics
        .as_deref()
        .ok_or_else(|| state_error("metrics_unavailable"))?;
    workspace.restore_backup(path, metrics)?;
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
    let metrics = state
        .metrics
        .as_deref()
        .ok_or_else(|| state_error("metrics_unavailable"))?;
    workspace.export_backup(path, metrics)?;
    drop(workspace);
    snapshot(state)
}

#[tauri::command]
fn get_snapshot(state: tauri::State<'_, AppState>) -> Result<AppSnapshot, AppError> {
    snapshot(&state)
}

#[tauri::command]
fn save_device_profile(
    state: tauri::State<'_, AppState>,
    profile: DeviceProfile,
) -> Result<AppSnapshot, AppError> {
    save_profile_inner(&state, profile)
}

#[tauri::command]
fn save_settings(
    state: tauri::State<'_, AppState>,
    settings: EditorSettingsPatch,
) -> Result<AppSnapshot, AppError> {
    save_settings_inner(&state, settings)
}

#[tauri::command]
fn rename_device(
    state: tauri::State<'_, AppState>,
    device_id: hardware::DeviceId,
    name: String,
) -> Result<AppSnapshot, AppError> {
    rename_device_inner(&state, &device_id, name)
}

#[tauri::command]
fn save_runtime_assignment(
    state: tauri::State<'_, AppState>,
    device_id: hardware::DeviceId,
    assignment: RuntimeAssignment,
) -> Result<AppSnapshot, AppError> {
    save_runtime_assignment_inner(&state, &device_id, assignment)
}

#[tauri::command]
fn clear_runtime_assignment(
    state: tauri::State<'_, AppState>,
    device_id: hardware::DeviceId,
) -> Result<AppSnapshot, AppError> {
    clear_runtime_assignment_inner(&state, &device_id)
}

#[tauri::command]
fn forget_device(
    state: tauri::State<'_, AppState>,
    device_id: hardware::DeviceId,
) -> Result<AppSnapshot, AppError> {
    forget_device_inner(&state, &device_id)
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
    begin_learning_inner(
        &state,
        &device_id,
        &device_profile_id,
        &hardware_profile_id,
        editing_revision,
        pins,
    )
}

#[tauri::command]
fn end_learning(
    state: tauri::State<'_, AppState>,
    device_id: hardware::DeviceId,
) -> Result<AppSnapshot, AppError> {
    end_learning_inner(&state, &device_id)
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
    import_profile_inner(&state, Path::new(&path))
}

#[tauri::command]
fn export_device_profile(
    state: tauri::State<'_, AppState>,
    id: String,
    path: String,
) -> Result<AppSnapshot, AppError> {
    state
        .workspace
        .read()
        .map_err(|_| state_error("workspace_unavailable"))?
        .export_profile(&id, Path::new(&path))?;
    snapshot(&state)
}

#[tauri::command]
fn delete_device_profile(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<AppSnapshot, AppError> {
    delete_profile_inner(&state, &id)
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
    export_backup_inner(&state, Path::new(&path))
}

#[tauri::command]
fn restore_backup(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<AppSnapshot, AppError> {
    restore_backup_inner(&state, Path::new(&path))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let config_directory = app.path().app_config_dir()?;
            fs::create_dir_all(&config_directory)?;
            let bundled_profiles = app.path().resource_dir()?.join("models");
            let workspace = Workspace::load(&config_directory, &bundled_profiles)?;
            let metrics = MetricsStore::open(&config_directory.join("data/metrics.sqlite3"))
                .ok()
                .map(Arc::new);
            let operation_barrier = Arc::new(RwLock::new(()));
            let workspace = Arc::new(RwLock::new(workspace));
            #[cfg(target_os = "macos")]
            tray::setup(app, &[])?;
            let paste = Arc::new(PasteCoordinator::system());
            let launcher = Arc::new(device::SystemWorkerLauncher::new(
                paste.handle(),
                metrics.clone(),
                Arc::clone(&operation_barrier),
            ));
            let coordinator = Arc::new(Mutex::new(RuntimeCoordinator::with_paste(
                Arc::new(coordinator::SystemUsbEnumerator),
                launcher,
                Arc::clone(&workspace),
                Some(paste.handle()),
            )));
            let stop = Arc::new(AtomicBool::new(false));
            let coordinator_thread = {
                let coordinator = Arc::clone(&coordinator);
                let workspace = Arc::clone(&workspace);
                let metrics = metrics.clone();
                let stop = Arc::clone(&stop);
                let app_handle = app.handle().clone();
                thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        let (devices, events) = {
                            let mut coordinator = coordinator
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner());
                            let _ = coordinator.scan_once();
                            let events = coordinator.drain_worker_events();
                            (coordinator.devices(), events)
                        };
                        #[cfg(target_os = "macos")]
                        tray::update_registry(&app_handle, &devices);
                        for event in events {
                            let payload =
                                enrich_runtime_event(&workspace, metrics.as_deref(), event);
                            let _ = app_handle.emit("runtime-event", payload);
                        }
                        for _ in 0..10 {
                            if stop.load(Ordering::Relaxed) {
                                break;
                            }
                            thread::sleep(Duration::from_millis(50));
                        }
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
                stop,
                coordinator_thread: Mutex::new(Some(coordinator_thread)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            save_device_profile,
            save_settings,
            rename_device,
            save_runtime_assignment,
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
        ])
        .build(tauri::generate_context!())
        .expect("error while building Kivo");

    app.run(|app_handle, event| match event {
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
        tauri::RunEvent::ExitRequested { .. } => {
            app_handle
                .state::<AppState>()
                .stop
                .store(true, Ordering::Relaxed);
        }
        tauri::RunEvent::Exit => {
            let state = app_handle.state::<AppState>();
            state.stop.store(true, Ordering::Relaxed);
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
        metrics::MetricAttribution,
        profile::{ButtonAction, HardwareProfile, InputSource, PROFILE_SCHEMA_VERSION},
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
        let layout = serde_json::from_str(include_str!("../../models/red-phone-v1.json")).unwrap();
        DeviceProfile {
            schema_version: PROFILE_SCHEMA_VERSION,
            profile: layout,
            hardware_profiles: vec![HardwareProfile {
                id: "esp-primary".into(),
                name: "ESP primary".into(),
                board_profile_id: crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID.into(),
                debounce_ms: 30,
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
            stop: Arc::new(AtomicBool::new(false)),
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
                        protocol: 3,
                        controller_family_id: board.family_id.into(),
                        board_profile_id: board.id.into(),
                        firmware_build_id: "restore-test".into(),
                        pins: board.safe_pins.to_vec(),
                    },
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
                        protocol: 3,
                        controller_family_id: board.family_id.into(),
                        board_profile_id: board.id.into(),
                        firmware_build_id: "save-test".into(),
                        pins: board.safe_pins.to_vec(),
                    },
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
            vec![ButtonAction::Paste {
                text: "离线保存".into(),
            }],
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
            .find(|board| board["id"] == crate::hardware::VCCGND_YD_RP2040_BOARD_ID)
            .unwrap();
        assert_eq!(
            rp2040["controllerFamilyId"],
            crate::hardware::RP2040_FAMILY_ID
        );
        assert_eq!(rp2040["runtimeUsb"], "2e8a:102e");
        assert_eq!(rp2040["bootloaderUsb"], "2e8a:0003");
        assert_eq!(
            rp2040["safePins"],
            serde_json::to_value((0_u8..=22).collect::<Vec<_>>()).unwrap()
        );
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
        let a = hardware::DeviceId::new(crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID, "SAVE-A")
            .unwrap();
        let b = hardware::DeviceId::new(crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID, "SAVE-B")
            .unwrap();
        state.coordinator = Some(Arc::new(Mutex::new(coordinator)));
        launcher.commands.lock().unwrap().clear();
        let assignment = workspace::RuntimeAssignment {
            device_profile_id: "red-phone-v1".into(),
            hardware_profile_id: "esp-primary".into(),
        };

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
            hardware::DeviceId::new(crate::hardware::VCCGND_YD_RP2040_BOARD_ID, "NOT-ENROLLED")
                .unwrap();

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
        assert_eq!(snapshot(&state).unwrap().candidates.len(), 1);
    }

    #[test]
    fn runtime_event_home_update_is_editor_profile_aggregate_only() {
        let directory = TestDirectory::new();
        let state = product_state(&directory.0, vec![product_profile()]);
        let a = hardware::DeviceId::new(crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID, "EVENT-A")
            .unwrap();
        let b = hardware::DeviceId::new(crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID, "EVENT-B")
            .unwrap();
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
            board_profile_id: crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID.into(),
            port: Some("/dev/event-a".into()),
            device_profile_id: Some("red-phone-v1".into()),
            hardware_profile_id: Some("esp-primary".into()),
            home_update: None,
            activity,
        };

        let matching = enrich_runtime_event(&state.workspace, state.metrics.as_deref(), event);
        assert_eq!(matching.home_update.as_ref().unwrap().total_presses, 2);

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
        let a = hardware::DeviceId::new(crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID, "SAVE-A")
            .unwrap();
        let b = hardware::DeviceId::new(crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID, "SAVE-B")
            .unwrap();
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
    fn workspace_command_saves_only_editor_preferences() {
        let directory = TestDirectory::new();
        let state = product_state(&directory.0, vec![product_profile()]);
        let id =
            hardware::DeviceId::new(crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID, "ABCDEF123456")
                .unwrap();
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
            hardware::DeviceId::new(crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID, "ABCDEF123456")
                .unwrap();
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
                crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID,
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
            hardware::DeviceId::new(crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID, "SAVE-A")
                .unwrap();
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
            hardware::DeviceId::new(crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID, "RESTORE-ESP")
                .unwrap();
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
                crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID,
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
                    crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID,
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
