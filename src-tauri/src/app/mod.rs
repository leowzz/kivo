#[cfg(any(target_os = "macos", target_os = "windows"))]
use crate::tray;
use crate::{
    coordinator, device, display, hardware, metrics, paste, profile, runtime_log, usage, workspace,
};
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

mod commands;
#[cfg(test)]
use commands::*;

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

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.manage(StartupState::default());
            let result: SetupResult = (|| {
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
                #[cfg(any(target_os = "macos", target_os = "windows"))]
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

    let builder = builder.invoke_handler(tauri::generate_handler![
        get_startup_failure,
        commands::get_snapshot,
        commands::retry_candidate,
        commands::save_device_profile,
        commands::create_device_profile,
        commands::duplicate_profile_for_device,
        commands::save_settings,
        commands::save_usage_settings,
        commands::rename_device,
        commands::save_product_configuration,
        commands::select_product_configuration,
        commands::create_product_configuration,
        commands::save_runtime_assignment,
        commands::complete_device_setup,
        commands::clear_runtime_assignment,
        commands::forget_device,
        commands::get_device_metrics,
        commands::preview_device_profile_import,
        commands::import_device_profile,
        commands::export_device_profile,
        commands::delete_device_profile,
        commands::preview_backup,
        commands::export_backup,
        commands::restore_backup,
    ]);

    let app = builder
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
mod tests;
