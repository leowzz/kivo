mod config;
mod device;
mod model;
mod protocol;
mod storage;

use config::{ButtonAction, IoMaps, MappingConfig, SUPPORTED_GPIOS};
use device::ConnectionStatus;
use model::ModelLayout;
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
    models: Arc<RwLock<Vec<ModelLayout>>>,
    model_directory: PathBuf,
    config_path: PathBuf,
    connection: Arc<RwLock<ConnectionStatus>>,
    config_error: Mutex<Option<String>>,
    capture_next_gpio: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppSnapshot {
    models: Vec<ModelLayout>,
    active_model: String,
    io_maps: IoMaps,
    actions: BTreeMap<String, ButtonAction>,
    supported_gpios: Vec<u8>,
    config_path: String,
    connection: ConnectionStatus,
    config_error: Option<String>,
}

fn snapshot(state: &AppState) -> Result<AppSnapshot, String> {
    let mappings = state
        .mappings
        .read()
        .map_err(|_| "mapping state is unavailable")?;
    Ok(AppSnapshot {
        models: state
            .models
            .read()
            .map_err(|_| "model state is unavailable")?
            .clone(),
        active_model: mappings.active_model.clone(),
        io_maps: mappings.io_maps.clone(),
        actions: mappings.actions.clone(),
        supported_gpios: SUPPORTED_GPIOS.to_vec(),
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

fn save_workspace_inner(
    state: &AppState,
    active_model: String,
    io_maps: IoMaps,
    actions: BTreeMap<String, ButtonAction>,
    models: Vec<ModelLayout>,
) -> Result<AppSnapshot, String> {
    for model in &models {
        model.validate()?;
    }
    let current_models = state
        .models
        .read()
        .map_err(|_| "model state is unavailable")?
        .clone();
    let legacy_buttons = state
        .mappings
        .read()
        .map_err(|_| "mapping state is unavailable")?
        .legacy_buttons
        .clone();
    let mut config = MappingConfig {
        active_model,
        io_maps,
        actions,
        legacy_buttons,
    };
    config.migrate_legacy();
    config.validate(&models)?;

    for model in &models {
        if current_models.iter().find(|current| current.id == model.id) != Some(model) {
            model::save(&state.model_directory, model)?;
        }
    }
    config::save(&state.config_path, &config)?;
    let mut mappings_state = state
        .mappings
        .write()
        .map_err(|_| "mapping state is unavailable")?;
    let mut models_state = state
        .models
        .write()
        .map_err(|_| "model state is unavailable")?;
    *mappings_state = config;
    *models_state = models;
    drop(models_state);
    drop(mappings_state);
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
fn save_workspace(
    state: tauri::State<'_, AppState>,
    active_model: String,
    io_maps: IoMaps,
    actions: BTreeMap<String, ButtonAction>,
    models: Vec<ModelLayout>,
) -> Result<AppSnapshot, String> {
    save_workspace_inner(&state, active_model, io_maps, actions, models)
}

#[tauri::command]
fn set_io_capture(state: tauri::State<'_, AppState>, enabled: bool) {
    state.capture_next_gpio.store(enabled, Ordering::Relaxed);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .setup(|app| {
            let config_directory = app.path().app_config_dir()?;
            fs::create_dir_all(&config_directory)?;
            let model_directory = config_directory.join("models");
            model::seed_default(&model_directory).map_err(std::io::Error::other)?;
            let (models, model_errors) = model::load_all(&model_directory);
            let config_path = config_directory.join("config.yaml");
            let mut config_errors = model_errors;
            if !config_path.exists() {
                let legacy_path = std::env::current_dir()?.join("config.yaml");
                if legacy_path.exists()
                    && let Err(error) = fs::copy(&legacy_path, &config_path)
                {
                    config_errors.push(format!("import {}: {error}", legacy_path.display()));
                }
            }
            let mut mappings = match config::load(&config_path) {
                Ok(config) => config,
                Err(error) => {
                    config_errors.push(error);
                    MappingConfig::default()
                }
            };
            if mappings.active_model.is_empty()
                && let Some(model) = models.first()
            {
                mappings.active_model = model.id.clone();
            }
            if let Err(error) = mappings.validate(&models) {
                config_errors.push(error);
                mappings = MappingConfig::default();
                if let Some(model) = models.first() {
                    mappings.active_model = model.id.clone();
                }
            }
            let config_error = (!config_errors.is_empty()).then(|| config_errors.join("\n"));
            let mappings = Arc::new(RwLock::new(mappings));
            let models = Arc::new(RwLock::new(models));
            let connection = Arc::new(RwLock::new(ConnectionStatus::searching()));
            let capture_next_gpio = Arc::new(AtomicBool::new(false));
            let stop = Arc::new(AtomicBool::new(false));
            let worker = {
                let app_handle = app.handle().clone();
                let mappings = Arc::clone(&mappings);
                let connection = Arc::clone(&connection);
                let capture_next_gpio = Arc::clone(&capture_next_gpio);
                let stop = Arc::clone(&stop);
                thread::spawn(move || {
                    device::run_worker(app_handle, mappings, connection, capture_next_gpio, stop)
                })
            };
            app.manage(AppState {
                mappings,
                models,
                model_directory,
                config_path,
                connection,
                config_error: Mutex::new(config_error),
                capture_next_gpio,
                stop,
                worker: Mutex::new(Some(worker)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            save_workspace,
            set_io_capture
        ])
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
    use model::ModelLayout;
    use std::{
        collections::BTreeMap,
        fs,
        sync::{Arc, Mutex, RwLock, atomic::AtomicBool},
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn save_workspace_persists_files_before_replacing_runtime_state() {
        let directory = std::env::temp_dir().join(format!(
            "vibe-tool-workspace-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let model_directory = directory.join("models");
        fs::create_dir_all(&model_directory).unwrap();
        let config_path = directory.join("config.yaml");
        let original_model: ModelLayout =
            serde_json::from_str(include_str!("../../models/red-phone-v1.json")).unwrap();
        model::save(&model_directory, &original_model).unwrap();
        let state = AppState {
            mappings: Arc::new(RwLock::new(config::MappingConfig::default())),
            models: Arc::new(RwLock::new(vec![original_model.clone()])),
            model_directory: model_directory.clone(),
            config_path: config_path.clone(),
            connection: Arc::new(RwLock::new(device::ConnectionStatus::searching())),
            config_error: Mutex::new(Some("old error".to_owned())),
            capture_next_gpio: Arc::new(AtomicBool::new(false)),
            stop: Arc::new(AtomicBool::new(false)),
            worker: Mutex::new(None),
        };
        let mut updated_model = original_model.clone();
        updated_model.groups[0].buttons[0].label = "One".into();
        let io_maps = BTreeMap::from([(
            updated_model.id.clone(),
            BTreeMap::from([(6, updated_model.groups[0].buttons[0].id.clone())]),
        )]);
        let actions = BTreeMap::from([(
            updated_model.groups[0].buttons[0].id.clone(),
            config::ButtonAction::Paste { text: "x".into() },
        )]);

        let saved = save_workspace_inner(
            &state,
            updated_model.id.clone(),
            io_maps.clone(),
            actions.clone(),
            vec![updated_model.clone()],
        )
        .unwrap();

        let persisted_config = config::load(&config_path).unwrap();
        let persisted_model: ModelLayout =
            serde_json::from_slice(&fs::read(model_directory.join("red-phone-v1.json")).unwrap())
                .unwrap();
        assert_eq!(persisted_config.active_model, updated_model.id);
        assert_eq!(persisted_config.io_maps, io_maps);
        assert_eq!(persisted_config.actions, actions);
        assert_eq!(persisted_model, updated_model);
        assert_eq!(state.mappings.read().unwrap().clone(), persisted_config);
        assert_eq!(state.models.read().unwrap().as_slice(), &[persisted_model]);
        assert_eq!(saved.active_model, "red-phone-v1");
        assert_eq!(*state.config_error.lock().unwrap(), None);
        fs::remove_dir_all(directory).unwrap();
    }
}
