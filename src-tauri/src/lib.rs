mod config;
mod device;
mod model;
mod protocol;
mod storage;

use config::MappingConfig;
use device::ConnectionStatus;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
};
use tauri::Manager;

struct AppState {
    mappings: Arc<RwLock<MappingConfig>>,
    config_path: PathBuf,
    connection: Arc<RwLock<ConnectionStatus>>,
    config_error: Mutex<Option<String>>,
    stop: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppSnapshot {
    buttons: BTreeMap<u8, String>,
    config_path: String,
    connection: ConnectionStatus,
    config_error: Option<String>,
}

fn snapshot(state: &AppState) -> Result<AppSnapshot, String> {
    Ok(AppSnapshot {
        buttons: state
            .mappings
            .read()
            .map_err(|_| "mapping state is unavailable")?
            .legacy_buttons
            .clone(),
        config_path: state.config_path.display().to_string(),
        connection: state
            .connection
            .read()
            .map_err(|_| "connection state is unavailable")?
            .clone(),
        config_error: state
            .config_error
            .lock()
            .map_err(|_| "configuration state is unavailable")?
            .clone(),
    })
}

fn save_mappings_inner(
    state: &AppState,
    buttons: BTreeMap<u8, String>,
) -> Result<AppSnapshot, String> {
    MappingConfig::from_buttons(buttons.clone())?;
    let mut config = state
        .mappings
        .read()
        .map_err(|_| "mapping state is unavailable")?
        .clone();
    config.legacy_buttons = buttons;
    config::save(&state.config_path, &config)?;
    *state
        .mappings
        .write()
        .map_err(|_| "mapping state is unavailable")? = config;
    *state
        .config_error
        .lock()
        .map_err(|_| "configuration state is unavailable")? = None;
    snapshot(state)
}

#[tauri::command]
fn get_snapshot(state: tauri::State<'_, AppState>) -> Result<AppSnapshot, String> {
    snapshot(&state)
}

#[tauri::command]
fn save_mappings(
    state: tauri::State<'_, AppState>,
    buttons: BTreeMap<u8, String>,
) -> Result<AppSnapshot, String> {
    save_mappings_inner(&state, buttons)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .setup(|app| {
            let config_directory = app.path().app_config_dir()?;
            fs::create_dir_all(&config_directory)?;
            let config_path = config_directory.join("config.yaml");
            let mut config_error = None;
            if !config_path.exists() {
                let legacy_path = std::env::current_dir()?.join("config.yaml");
                if legacy_path.exists()
                    && let Err(error) = fs::copy(&legacy_path, &config_path)
                {
                    config_error = Some(format!("import {}: {error}", legacy_path.display()));
                }
            }
            let mappings = match config::load(&config_path) {
                Ok(config) => config,
                Err(error) => {
                    config_error = Some(error);
                    MappingConfig::default()
                }
            };
            let mappings = Arc::new(RwLock::new(mappings));
            let connection = Arc::new(RwLock::new(ConnectionStatus::searching()));
            let stop = Arc::new(AtomicBool::new(false));
            let worker = {
                let app_handle = app.handle().clone();
                let mappings = Arc::clone(&mappings);
                let connection = Arc::clone(&connection);
                let stop = Arc::clone(&stop);
                thread::spawn(move || device::run_worker(app_handle, mappings, connection, stop))
            };
            app.manage(AppState {
                mappings,
                config_path,
                connection,
                config_error: Mutex::new(config_error),
                stop,
                worker: Mutex::new(Some(worker)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_snapshot, save_mappings])
        .build(tauri::generate_context!())
        .expect("error while building Vibe Tool");

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
    use std::{
        collections::BTreeMap,
        fs,
        sync::{Arc, Mutex, RwLock, atomic::AtomicBool},
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn save_state_persists_before_replacing_runtime_mappings() {
        let path = std::env::temp_dir().join(format!(
            "vibe-tool-state-{}-{}.yaml",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let state = AppState {
            mappings: Arc::new(RwLock::new(config::MappingConfig::default())),
            config_path: path.clone(),
            connection: Arc::new(RwLock::new(device::ConnectionStatus::searching())),
            config_error: Mutex::new(Some("old error".to_owned())),
            stop: Arc::new(AtomicBool::new(false)),
            worker: Mutex::new(None),
        };
        let buttons = BTreeMap::from([(6, "你好".to_owned())]);

        let saved = save_mappings_inner(&state, buttons.clone()).unwrap();

        assert_eq!(saved.buttons, buttons);
        assert_eq!(config::load(&path).unwrap().legacy_buttons, buttons);
        assert_eq!(*state.config_error.lock().unwrap(), None);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn save_mappings_preserves_typed_fields() {
        let path = std::env::temp_dir().join(format!(
            "vibe-tool-state-{}-{}.yaml",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let typed = config::MappingConfig {
            active_model: "red-phone-v1".into(),
            io_maps: BTreeMap::from([(
                "red-phone-v1".into(),
                BTreeMap::from([(6, "DIGIT_2".into())]),
            )]),
            actions: BTreeMap::from([(
                "DIGIT_2".into(),
                config::ButtonAction::Hotkey {
                    keys: vec!["cmd".into(), "shift".into(), "k".into()],
                },
            )]),
            legacy_buttons: BTreeMap::new(),
        };
        let state = AppState {
            mappings: Arc::new(RwLock::new(typed.clone())),
            config_path: path.clone(),
            connection: Arc::new(RwLock::new(device::ConnectionStatus::searching())),
            config_error: Mutex::new(None),
            stop: Arc::new(AtomicBool::new(false)),
            worker: Mutex::new(None),
        };
        let buttons = BTreeMap::from([(7, "legacy".to_owned())]);

        save_mappings_inner(&state, buttons.clone()).unwrap();

        let saved = state.mappings.read().unwrap().clone();
        assert_eq!(saved.active_model, typed.active_model);
        assert_eq!(saved.io_maps, typed.io_maps);
        assert_eq!(saved.actions, typed.actions);
        assert_eq!(saved.legacy_buttons, buttons);
        let persisted = config::load(&path).unwrap();
        assert_eq!(persisted.active_model, typed.active_model);
        assert_eq!(persisted.io_maps, typed.io_maps);
        assert_eq!(persisted.actions, typed.actions);
        assert_eq!(persisted.legacy_buttons, buttons);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn invalid_save_keeps_runtime_mappings() {
        let original = BTreeMap::from([(6, "old".to_owned())]);
        let state = AppState {
            mappings: Arc::new(RwLock::new(
                config::MappingConfig::from_buttons(original.clone()).unwrap(),
            )),
            config_path: std::env::temp_dir().join("unused-vibe-tool-config.yaml"),
            connection: Arc::new(RwLock::new(device::ConnectionStatus::searching())),
            config_error: Mutex::new(None),
            stop: Arc::new(AtomicBool::new(false)),
            worker: Mutex::new(None),
        };

        assert!(save_mappings_inner(&state, BTreeMap::from([(10, "unsafe".to_owned())])).is_err());
        assert_eq!(state.mappings.read().unwrap().legacy_buttons, original);
    }
}
