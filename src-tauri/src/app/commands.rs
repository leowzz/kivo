use super::*;

pub(super) fn snapshot(state: &AppState) -> Result<AppSnapshot, AppError> {
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

pub(super) fn mutate_workspace(
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

pub(super) fn mutate_workspace_with_operation_barrier(
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

pub(super) fn require_addressable_identity(
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

pub(super) fn save_profile_inner(
    state: &AppState,
    profile: DeviceProfile,
) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, move |workspace, _| workspace.save_profile(profile))
}

pub(super) fn create_device_profile_inner(
    state: &AppState,
    request: CreateDeviceProfileRequest,
) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, move |workspace, _| {
        workspace.create_profile(request).map(|_| ())
    })
}

pub(super) fn retry_candidate_inner(
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

pub(super) fn save_settings_inner(
    state: &AppState,
    settings: EditorSettingsPatch,
) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, move |workspace, _| workspace.save_settings(settings))
}

pub(super) fn save_usage_settings_inner(
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

pub(super) fn import_profile_inner(state: &AppState, path: &Path) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, |workspace, _| workspace.import_profile(path))
}

pub(super) fn export_profile_inner(
    state: &AppState,
    id: &str,
    path: &Path,
) -> Result<AppSnapshot, AppError> {
    state
        .workspace
        .read()
        .map_err(|_| state_error("workspace_unavailable"))?
        .export_profile(id, path)?;
    snapshot(state)
}

pub(super) fn delete_profile_inner(state: &AppState, id: &str) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, |workspace, _| workspace.delete_profile(id))
}

pub(super) fn rename_device_inner(
    state: &AppState,
    device_id: &hardware::DeviceId,
    name: String,
) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, move |workspace, coordinator| {
        require_addressable_identity(coordinator, device_id)?;
        workspace.rename_device(device_id, name)
    })
}

pub(super) fn save_product_configuration_inner(
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

pub(super) fn select_product_configuration_inner(
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

pub(super) fn create_product_configuration_inner(
    state: &AppState,
    request: CreateProductConfigurationRequest,
) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, move |workspace, _| {
        workspace.create_product_configuration(request)
    })
}

pub(super) fn save_runtime_assignment_inner(
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

pub(super) fn validate_online_assignment_protocol(
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

pub(super) fn validate_online_firmware_protocol(
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

pub(super) fn validate_setup_eligibility(
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

pub(super) fn require_setup_device(
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

pub(super) fn complete_device_setup_inner(
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

pub(super) fn duplicate_profile_for_device_inner(
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

pub(super) fn clear_runtime_assignment_inner(
    state: &AppState,
    device_id: &hardware::DeviceId,
) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, |workspace, coordinator| {
        require_addressable_identity(coordinator, device_id)?;
        workspace.clear_assignment(device_id)
    })
}

pub(super) fn forget_device_inner(
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

pub(super) fn get_device_metrics_inner(
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

pub(super) fn restore_backup_inner(state: &AppState, path: &Path) -> Result<AppSnapshot, AppError> {
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

pub(super) fn export_backup_inner(state: &AppState, path: &Path) -> Result<AppSnapshot, AppError> {
    let workspace = state
        .workspace
        .read()
        .map_err(|_| state_error("workspace_unavailable"))?;
    workspace.export_user_backup(path)?;
    drop(workspace);
    snapshot(state)
}

pub(super) fn device_operation_context(device_id: &hardware::DeviceId) -> serde_json::Value {
    serde_json::json!({"deviceId": device_id})
}

pub(super) fn profile_operation_context(device_profile_id: &str) -> serde_json::Value {
    serde_json::json!({"deviceProfileId": device_profile_id})
}

pub(super) fn assignment_operation_context(
    device_id: &hardware::DeviceId,
    assignment: &RuntimeAssignment,
) -> serde_json::Value {
    serde_json::json!({
        "deviceId": device_id,
        "deviceProfileId": assignment.device_profile_id,
        "hardwareProfileId": assignment.hardware_profile_id,
    })
}

pub(super) fn create_profile_operation_context(
    request: &CreateDeviceProfileRequest,
) -> serde_json::Value {
    match request {
        CreateDeviceProfileRequest::Clone {
            source_profile_id, ..
        } => serde_json::json!({"kind": "clone", "sourceProfileId": source_profile_id}),
        CreateDeviceProfileRequest::Blank {
            board_profile_id, ..
        } => serde_json::json!({"kind": "blank", "boardProfileId": board_profile_id}),
    }
}

pub(super) fn settings_operation_context(settings: &EditorSettingsPatch) -> serde_json::Value {
    serde_json::json!({
        "schemaVersion": settings.schema_version,
        "editorProfile": settings.editor_profile,
        "language": settings.language,
    })
}

#[tauri::command]
pub(super) fn get_snapshot(state: tauri::State<'_, AppState>) -> Result<AppSnapshot, AppError> {
    snapshot(&state)
}

#[tauri::command]
pub(super) fn retry_candidate(
    state: tauri::State<'_, AppState>,
    device_id: hardware::DeviceId,
) -> Result<AppSnapshot, AppError> {
    let context = device_operation_context(&device_id);
    runtime_log::operation(now_ms(), "device_candidate_retry", context, || {
        retry_candidate_inner(&state, &device_id)
    })
}

#[tauri::command]
pub(super) fn save_device_profile(
    state: tauri::State<'_, AppState>,
    profile: DeviceProfile,
) -> Result<AppSnapshot, AppError> {
    let context = profile_operation_context(&profile.profile.id);
    runtime_log::operation(now_ms(), "device_profile_saved", context, || {
        save_profile_inner(&state, profile)
    })
}

#[tauri::command]
pub(super) fn create_device_profile(
    state: tauri::State<'_, AppState>,
    request: CreateDeviceProfileRequest,
) -> Result<AppSnapshot, AppError> {
    let context = create_profile_operation_context(&request);
    runtime_log::operation(now_ms(), "device_profile_created", context, || {
        create_device_profile_inner(&state, request)
    })
}

#[tauri::command]
pub(super) fn duplicate_profile_for_device(
    state: tauri::State<'_, AppState>,
    request: DuplicateProfileForDeviceRequest,
) -> Result<AppSnapshot, AppError> {
    let context = device_operation_context(&request.device_id);
    runtime_log::operation(now_ms(), "profile_duplicated_for_device", context, || {
        duplicate_profile_for_device_inner(&state, request)
    })
}

#[tauri::command]
pub(super) fn save_settings(
    state: tauri::State<'_, AppState>,
    settings: EditorSettingsPatch,
) -> Result<AppSnapshot, AppError> {
    let context = settings_operation_context(&settings);
    runtime_log::operation(now_ms(), "settings_saved", context, || {
        save_settings_inner(&state, settings)
    })
}

#[tauri::command]
pub(super) fn save_usage_settings(
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
pub(super) fn rename_device(
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
pub(super) fn save_product_configuration(
    state: tauri::State<'_, AppState>,
    device_id: hardware::DeviceId,
    config: ProductConfigurationProfile,
) -> Result<AppSnapshot, AppError> {
    save_product_configuration_inner(&state, &device_id, config)
}

#[tauri::command]
pub(super) fn select_product_configuration(
    state: tauri::State<'_, AppState>,
    device_id: hardware::DeviceId,
    configuration_id: String,
) -> Result<AppSnapshot, AppError> {
    select_product_configuration_inner(&state, &device_id, &configuration_id)
}

#[tauri::command]
pub(super) fn create_product_configuration(
    state: tauri::State<'_, AppState>,
    request: CreateProductConfigurationRequest,
) -> Result<AppSnapshot, AppError> {
    create_product_configuration_inner(&state, request)
}

#[tauri::command]
pub(super) fn save_runtime_assignment(
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
pub(super) fn complete_device_setup(
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
pub(super) fn clear_runtime_assignment(
    state: tauri::State<'_, AppState>,
    device_id: hardware::DeviceId,
) -> Result<AppSnapshot, AppError> {
    let context = device_operation_context(&device_id);
    runtime_log::operation(now_ms(), "runtime_assignment_cleared", context, || {
        clear_runtime_assignment_inner(&state, &device_id)
    })
}

#[tauri::command]
pub(super) fn forget_device(
    state: tauri::State<'_, AppState>,
    device_id: hardware::DeviceId,
) -> Result<AppSnapshot, AppError> {
    let context = device_operation_context(&device_id);
    runtime_log::operation(now_ms(), "device_forgotten", context, || {
        forget_device_inner(&state, &device_id)
    })
}

#[tauri::command]
pub(super) fn get_device_metrics(
    state: tauri::State<'_, AppState>,
    device_id: hardware::DeviceId,
) -> Result<HomeMetricsSnapshot, AppError> {
    get_device_metrics_inner(&state, &device_id)
}

#[tauri::command]
pub(super) fn preview_device_profile_import(
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
pub(super) fn import_device_profile(
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
pub(super) fn export_device_profile(
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
pub(super) fn delete_device_profile(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<AppSnapshot, AppError> {
    let context = profile_operation_context(&id);
    runtime_log::operation(now_ms(), "device_profile_deleted", context, || {
        delete_profile_inner(&state, &id)
    })
}

#[tauri::command]
pub(super) fn preview_backup(
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
pub(super) fn export_backup(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<AppSnapshot, AppError> {
    runtime_log::operation(now_ms(), "backup_exported", serde_json::json!({}), || {
        export_backup_inner(&state, Path::new(&path))
    })
}

#[tauri::command]
pub(super) fn restore_backup(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<AppSnapshot, AppError> {
    runtime_log::operation(now_ms(), "backup_restored", serde_json::json!({}), || {
        restore_backup_inner(&state, Path::new(&path))
    })
}
