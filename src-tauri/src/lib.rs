mod device;
pub mod hardware;
mod metrics;
mod model;
mod profile;
mod protocol;
mod storage;
#[cfg(target_os = "macos")]
mod tray;
mod workspace;

use device::{
    ConnectionStatus, DeviceCapabilities, LearningSession, RuntimeActivity, RuntimeProfile,
};
use metrics::{HomeMetricsSnapshot, MetricsStore};
use profile::DeviceProfile;
use serde::Serialize;
use std::{
    collections::{BTreeSet, VecDeque},
    fs,
    path::Path,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::Manager;
use workspace::{AppError, BackupPreview, ImportPreview, Language, SettingsDocument, Workspace};

struct AppState {
    workspace: Arc<RwLock<Workspace>>,
    active_runtime_profile: Arc<RwLock<Option<RuntimeProfile>>>,
    connection: Arc<RwLock<ConnectionStatus>>,
    capabilities: Arc<RwLock<Option<DeviceCapabilities>>>,
    runtime_error: Arc<RwLock<Option<RuntimeActivity>>>,
    learning: Arc<RwLock<Option<LearningSession>>>,
    metrics: Option<Arc<MetricsStore>>,
    next_learning_revision: Mutex<u32>,
    device_controls: Arc<Mutex<VecDeque<String>>>,
    stop: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
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
    learning: Option<LearningSession>,
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

fn sync_runtime(state: &AppState, workspace: &Workspace) -> Result<(), AppError> {
    let _ = workspace;
    *state
        .active_runtime_profile
        .write()
        .map_err(|_| state_error("runtime_profile_unavailable"))? = None;
    Ok(())
}

fn snapshot(state: &AppState) -> Result<AppSnapshot, AppError> {
    let workspace = state
        .workspace
        .read()
        .map_err(|_| state_error("workspace_unavailable"))?;
    let mut profiles = workspace.profiles.values().cloned().collect::<Vec<_>>();
    profiles.sort_by(|left, right| left.profile.name.cmp(&right.profile.name));
    let home_metrics = state.metrics.as_ref().and_then(|metrics| {
        workspace
            .settings
            .editor_profile
            .as_deref()
            .and_then(|model_id| metrics.home_snapshot(model_id, now_ms()).ok())
    });
    Ok(AppSnapshot {
        profiles,
        editor_profile: workspace.settings.editor_profile.clone(),
        language: workspace.settings.language,
        supported_gpios: state
            .capabilities
            .read()
            .map_err(|_| state_error("device_capabilities_unavailable"))?
            .as_ref()
            .map(|capabilities| capabilities.pins.clone())
            .unwrap_or_default(),
        connection: state
            .connection
            .read()
            .map_err(|_| state_error("connection_unavailable"))?
            .clone(),
        runtime_error: state
            .runtime_error
            .read()
            .map_err(|_| state_error("runtime_error_unavailable"))?
            .clone(),
        learning: state
            .learning
            .read()
            .map_err(|_| state_error("learning_state_unavailable"))?
            .clone(),
        home_metrics,
    })
}

fn save_profile_inner(state: &AppState, profile: DeviceProfile) -> Result<AppSnapshot, AppError> {
    let mut workspace = state
        .workspace
        .write()
        .map_err(|_| state_error("workspace_unavailable"))?;
    workspace.save_profile(profile)?;
    sync_runtime(state, &workspace)?;
    drop(workspace);
    snapshot(state)
}

fn save_settings_inner(
    state: &AppState,
    settings: SettingsDocument,
) -> Result<AppSnapshot, AppError> {
    let mut workspace = state
        .workspace
        .write()
        .map_err(|_| state_error("workspace_unavailable"))?;
    workspace.save_settings(settings)?;
    sync_runtime(state, &workspace)?;
    drop(workspace);
    snapshot(state)
}

fn import_profile_inner(state: &AppState, path: &Path) -> Result<AppSnapshot, AppError> {
    let mut workspace = state
        .workspace
        .write()
        .map_err(|_| state_error("workspace_unavailable"))?;
    workspace.import_profile(path)?;
    sync_runtime(state, &workspace)?;
    drop(workspace);
    snapshot(state)
}

fn delete_profile_inner(state: &AppState, id: &str) -> Result<AppSnapshot, AppError> {
    let mut workspace = state
        .workspace
        .write()
        .map_err(|_| state_error("workspace_unavailable"))?;
    workspace.delete_profile(id)?;
    sync_runtime(state, &workspace)?;
    drop(workspace);
    snapshot(state)
}

fn restore_backup_inner(state: &AppState, path: &Path) -> Result<AppSnapshot, AppError> {
    let mut workspace = state
        .workspace
        .write()
        .map_err(|_| state_error("workspace_unavailable"))?;
    workspace.restore_backup(path)?;
    sync_runtime(state, &workspace)?;
    drop(workspace);
    snapshot(state)
}

fn begin_learning_inner(state: &AppState, mut pins: Vec<u8>) -> Result<AppSnapshot, AppError> {
    if pins.is_empty() || pins.iter().copied().collect::<BTreeSet<_>>().len() != pins.len() {
        return Err(state_error("invalid_learning_pins"));
    }
    let capabilities = state
        .capabilities
        .read()
        .map_err(|_| state_error("device_capabilities_unavailable"))?
        .clone()
        .ok_or_else(|| state_error("device_not_connected"))?;
    if capabilities.protocol != 3 {
        return Err(state_error("protocol_mismatch"));
    }
    if let Some(gpio) = pins.iter().find(|gpio| !capabilities.pins.contains(gpio)) {
        return Err(AppError::new("unsupported_gpio").with_param("gpio", gpio.to_string()));
    }
    let mut learning = state
        .learning
        .write()
        .map_err(|_| state_error("learning_state_unavailable"))?;
    if learning.is_some() {
        return Err(state_error("learning_already_active"));
    }
    pins.sort_unstable();
    let mut revision = state
        .next_learning_revision
        .lock()
        .map_err(|_| state_error("learning_revision_unavailable"))?;
    *revision = revision.wrapping_add(1).max(1);
    let session = LearningSession {
        revision: *revision,
        pins: pins.clone(),
    };
    state
        .device_controls
        .lock()
        .map_err(|_| state_error("device_control_unavailable"))?
        .push_back(format!(
            "LEARN_BEGIN {} {} {}\n",
            session.revision,
            pins.len(),
            pins.iter().map(u8::to_string).collect::<Vec<_>>().join(" ")
        ));
    *learning = Some(session);
    drop(learning);
    snapshot(state)
}

fn end_learning_inner(state: &AppState) -> Result<AppSnapshot, AppError> {
    let mut learning = state
        .learning
        .write()
        .map_err(|_| state_error("learning_state_unavailable"))?;
    let session = learning
        .take()
        .ok_or_else(|| state_error("learning_not_active"))?;
    state
        .device_controls
        .lock()
        .map_err(|_| state_error("device_control_unavailable"))?
        .push_back(format!("LEARN_END {}\n", session.revision));
    drop(learning);
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
    settings: SettingsDocument,
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
    state
        .workspace
        .read()
        .map_err(|_| state_error("workspace_unavailable"))?
        .export_backup(Path::new(&path))?;
    snapshot(&state)
}

#[tauri::command]
fn restore_backup(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<AppSnapshot, AppError> {
    restore_backup_inner(&state, Path::new(&path))
}

#[tauri::command]
fn begin_learning(
    state: tauri::State<'_, AppState>,
    pins: Vec<u8>,
) -> Result<AppSnapshot, AppError> {
    begin_learning_inner(&state, pins)
}

#[tauri::command]
fn end_learning(state: tauri::State<'_, AppState>) -> Result<AppSnapshot, AppError> {
    end_learning_inner(&state)
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
            let metrics = MetricsStore::open(&config_directory.join("metrics.sqlite3"))
                .ok()
                .map(Arc::new);
            let active_runtime_profile = Arc::new(RwLock::new(None));
            let workspace = Arc::new(RwLock::new(workspace));
            let initial_connection = ConnectionStatus::searching();
            #[cfg(target_os = "macos")]
            tray::setup(app, &initial_connection)?;
            let connection = Arc::new(RwLock::new(initial_connection));
            let capabilities = Arc::new(RwLock::new(None));
            let runtime_error = Arc::new(RwLock::new(None));
            let learning = Arc::new(RwLock::new(None));
            let device_controls = Arc::new(Mutex::new(VecDeque::new()));
            let stop = Arc::new(AtomicBool::new(false));
            let worker = {
                let app_handle = app.handle().clone();
                let active_runtime_profile = Arc::clone(&active_runtime_profile);
                let connection = Arc::clone(&connection);
                let capabilities = Arc::clone(&capabilities);
                let runtime_error = Arc::clone(&runtime_error);
                let learning = Arc::clone(&learning);
                let metrics = metrics.clone();
                let device_controls = Arc::clone(&device_controls);
                let stop = Arc::clone(&stop);
                thread::spawn(move || {
                    device::run_worker(
                        app_handle,
                        device::WorkerState {
                            active_profile: active_runtime_profile,
                            connection,
                            capabilities,
                            runtime_error,
                            learning,
                            metrics,
                            controls: device_controls,
                            stop,
                        },
                    )
                })
            };
            app.manage(AppState {
                workspace,
                active_runtime_profile,
                connection,
                capabilities,
                runtime_error,
                learning,
                metrics,
                next_learning_revision: Mutex::new(0),
                device_controls,
                stop,
                worker: Mutex::new(Some(worker)),
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
            begin_learning,
            end_learning,
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
            if let Some(worker) = state
                .worker
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take()
            {
                let _ = worker.join();
            }
        }
        _ => {}
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{ButtonAction, HardwareProfile, InputSource, PROFILE_SCHEMA_VERSION};
    use std::{
        collections::BTreeMap,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering as AtomicOrdering},
        time::{SystemTime, UNIX_EPOCH},
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
            active_runtime_profile: Arc::new(RwLock::new(None)),
            connection: Arc::new(RwLock::new(ConnectionStatus::searching())),
            capabilities: Arc::new(RwLock::new(None)),
            runtime_error: Arc::new(RwLock::new(None)),
            learning: Arc::new(RwLock::new(None)),
            metrics: MetricsStore::open(&directory.join("metrics.sqlite3"))
                .ok()
                .map(Arc::new),
            next_learning_revision: Mutex::new(0),
            device_controls: Arc::new(Mutex::new(VecDeque::new())),
            stop: Arc::new(AtomicBool::new(false)),
            worker: Mutex::new(None),
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
        assert!(state.active_runtime_profile.read().unwrap().is_none());
        assert!(directory.path("data/profiles/red-phone-v1.yaml").exists());
    }

    #[test]
    fn snapshot_includes_editor_profile_metrics() {
        let directory = TestDirectory::new();
        let state = product_state(&directory.0, vec![product_profile()]);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        state
            .metrics
            .as_ref()
            .unwrap()
            .record_button_press("red-phone-v1", "UP", timestamp)
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
        assert!(state.active_runtime_profile.read().unwrap().is_none());
    }

    #[test]
    fn workspace_command_restore_replaces_runtime_snapshot() {
        let directory = TestDirectory::new();
        let state = product_state(&directory.0, vec![product_profile()]);
        let backup = directory.path("backup.yaml");
        state
            .workspace
            .read()
            .unwrap()
            .export_backup(&backup)
            .unwrap();
        delete_profile_inner(&state, "red-phone-v1").unwrap();

        let snapshot = restore_backup_inner(&state, &backup).unwrap();

        assert_eq!(snapshot.editor_profile.as_deref(), Some("red-phone-v1"));
        assert!(state.active_runtime_profile.read().unwrap().is_none());
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
    fn workspace_command_rejects_learning_pin_outside_device_allowlist() {
        let directory = TestDirectory::new();
        let state = product_state(&directory.0, vec![product_profile()]);
        *state.capabilities.write().unwrap() = Some(DeviceCapabilities {
            protocol: 3,
            controller_family_id: "esp32s3".into(),
            board_profile_id: "luatos-esp32s3-aio".into(),
            firmware_build_id: "test".into(),
            pins: vec![1, 2],
        });

        let error = begin_learning_inner(&state, vec![1, 6]).unwrap_err();

        assert_eq!(error.code, "unsupported_gpio");
        assert!(state.device_controls.lock().unwrap().is_empty());
    }

    #[test]
    fn settings_reject_unknown_language() {
        assert!(
            serde_yaml_ng::from_str::<SettingsDocument>(
                "schema_version: 2\neditor_profile: null\nlanguage: fr-FR\ndevices: {}\n"
            )
            .is_err()
        );
    }
}
