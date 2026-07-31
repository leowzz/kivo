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

use coordinator::{ConnectionDimension, RuntimeCoordinator};
use device::{ConnectionState, ConnectionStatus, LearningTarget, RuntimeActivity};
use metrics::{HomeMetricsSnapshot, MetricsStore};
use paste::PasteCoordinator;
use profile::{DeviceProfile, ProfileChange};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    fs,
    path::Path,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::Manager;
use workspace::{AppError, BackupPreview, EditorSettingsPatch, ImportPreview, Language, Workspace};

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
    profiles: Vec<DeviceProfile>,
    editor_profile: Option<String>,
    language: Language,
    supported_gpios: Vec<u8>,
    connection: ConnectionStatus,
    runtime_error: Option<RuntimeActivity>,
    learning: Option<LearningTarget>,
    home_metrics: Option<HomeMetricsSnapshot>,
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

fn apply_profile_change(state: &AppState, change: &ProfileChange) -> Result<(), AppError> {
    if let Some(coordinator) = &state.coordinator {
        coordinator
            .lock()
            .map_err(|_| state_error("coordinator_unavailable"))?
            .apply_profile_change(change);
    }
    Ok(())
}

fn snapshot(state: &AppState) -> Result<AppSnapshot, AppError> {
    let workspace = state
        .workspace
        .read()
        .map_err(|_| state_error("workspace_unavailable"))?;
    let mut profiles = workspace.profiles.values().cloned().collect::<Vec<_>>();
    profiles.sort_by(|left, right| left.profile.name.cmp(&right.profile.name));
    let editor_profile = workspace.settings.editor_profile.clone();
    let language = workspace.settings.language;
    let home_metrics = state.metrics.as_ref().and_then(|metrics| {
        editor_profile
            .as_deref()
            .and_then(|profile_id| metrics.home_snapshot(profile_id, None, now_ms()).ok())
    });
    drop(workspace);
    let devices = state
        .coordinator
        .as_ref()
        .and_then(|coordinator| {
            coordinator
                .lock()
                .ok()
                .map(|coordinator| coordinator.devices())
        })
        .unwrap_or_default();
    let supported_gpios = devices
        .iter()
        .filter(|device| device.connection == ConnectionDimension::Online)
        .flat_map(|device| device.pins.iter().copied())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let online = devices
        .iter()
        .filter(|device| device.connection == ConnectionDimension::Online)
        .collect::<Vec<_>>();
    let connection = if online.is_empty() {
        ConnectionStatus::searching()
    } else {
        ConnectionStatus {
            state: ConnectionState::Connected,
            port: (online.len() == 1)
                .then(|| online[0].port.clone())
                .flatten(),
        }
    };
    let runtime_error = devices.iter().find_map(|device| {
        device.latest_error.as_ref().map(|detail| {
            let mut activity = RuntimeActivity::new("device_runtime_error");
            activity.detail = Some(detail.clone());
            activity
        })
    });
    let learning = devices.iter().find_map(|device| device.learning.clone());
    Ok(AppSnapshot {
        profiles,
        editor_profile,
        language,
        supported_gpios,
        connection,
        runtime_error,
        learning,
        home_metrics,
    })
}

fn save_profile_inner(state: &AppState, profile: DeviceProfile) -> Result<AppSnapshot, AppError> {
    let mut workspace = state
        .workspace
        .write()
        .map_err(|_| state_error("workspace_unavailable"))?;
    let profile_id = profile.profile.id.clone();
    let old = workspace.profiles.get(&profile_id).cloned();
    workspace.save_profile(profile)?;
    let new = workspace.profiles.get(&profile_id).cloned();
    let change = ProfileChange::between(old.as_ref(), new.as_ref());
    drop(workspace);
    apply_profile_change(state, &change)?;
    snapshot(state)
}

fn save_settings_inner(
    state: &AppState,
    settings: EditorSettingsPatch,
) -> Result<AppSnapshot, AppError> {
    let mut workspace = state
        .workspace
        .write()
        .map_err(|_| state_error("workspace_unavailable"))?;
    workspace.save_settings(settings)?;
    drop(workspace);
    snapshot(state)
}

fn import_profile_inner(state: &AppState, path: &Path) -> Result<AppSnapshot, AppError> {
    let mut workspace = state
        .workspace
        .write()
        .map_err(|_| state_error("workspace_unavailable"))?;
    let old = workspace.profiles.clone();
    workspace.import_profile(path)?;
    let changes = workspace
        .profiles
        .iter()
        .filter(|(id, profile)| old.get(*id) != Some(*profile))
        .map(|(id, profile)| ProfileChange::between(old.get(id), Some(profile)))
        .collect::<Vec<_>>();
    drop(workspace);
    for change in &changes {
        apply_profile_change(state, change)?;
    }
    snapshot(state)
}

fn delete_profile_inner(state: &AppState, id: &str) -> Result<AppSnapshot, AppError> {
    let mut workspace = state
        .workspace
        .write()
        .map_err(|_| state_error("workspace_unavailable"))?;
    let old = workspace.profiles.get(id).cloned();
    workspace.delete_profile(id)?;
    let change = ProfileChange::between(old.as_ref(), None);
    drop(workspace);
    apply_profile_change(state, &change)?;
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
    drop(workspace);
    if let Some(coordinator) = coordinator.as_mut() {
        coordinator.sync_profiles();
    }
    drop(operation);
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
fn preview_profile_import(
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
fn import_profile(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<AppSnapshot, AppError> {
    import_profile_inner(&state, Path::new(&path))
}

#[tauri::command]
fn export_profile(
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
fn delete_profile(state: tauri::State<'_, AppState>, id: String) -> Result<AppSnapshot, AppError> {
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
                let stop = Arc::clone(&stop);
                #[cfg(target_os = "macos")]
                let app_handle = app.handle().clone();
                thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        let devices = {
                            let mut coordinator = coordinator
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner());
                            let _ = coordinator.scan_once();
                            coordinator.drain_worker_events();
                            coordinator.devices()
                        };
                        #[cfg(target_os = "macos")]
                        tray::update_registry(&app_handle, &devices);
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
            preview_profile_import,
            import_profile,
            export_profile,
            delete_profile,
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
                board_profile_id: "luatos-esp32s3-aio".into(),
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

        assert_eq!(snapshot.profiles, vec![updated]);
        assert!(directory.path("data/profiles/red-phone-v1.yaml").exists());
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
        let a = hardware::DeviceId::new("luatos-esp32s3-aio", "SAVE-A").unwrap();
        let b = hardware::DeviceId::new("luatos-esp32s3-aio", "SAVE-B").unwrap();
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
        let id = hardware::DeviceId::new("luatos-esp32s3-aio", "ABCDEF123456").unwrap();
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
        let device_id = hardware::DeviceId::new("luatos-esp32s3-aio", "ABCDEF123456").unwrap();
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

        assert!(snapshot.profiles.is_empty());
        assert_eq!(snapshot.editor_profile, None);
    }

    #[test]
    fn workspace_command_restore_replaces_runtime_snapshot() {
        let directory = TestDirectory::new();
        let state = product_state(&directory.0, vec![product_profile()]);
        let backup = directory.path("backup.yaml");
        let timestamp = now_ms();
        let attribution = MetricAttribution {
            device_id: hardware::DeviceId::new("luatos-esp32s3-aio", "ABCDEF123456").unwrap(),
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
    fn restore_waits_for_an_in_flight_metric_commit_before_swapping() {
        let directory = TestDirectory::new();
        let state = Arc::new(product_state(&directory.0, vec![product_profile()]));
        let backup = directory.path("backup.yaml");
        export_backup_inner(&state, &backup).unwrap();
        let attribution = MetricAttribution {
            device_id: hardware::DeviceId::new("luatos-esp32s3-aio", "ABCDEF123456").unwrap(),
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
                device_id: hardware::DeviceId::new("luatos-esp32s3-aio", "ABCDEF123456").unwrap(),
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

        assert_eq!(snapshot.profiles, vec![replacement]);
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
