use crate::{
    hardware::{DeviceId, board_by_id},
    metrics::{MetricsBackup, MetricsStore},
    profile::{DeviceProfile, HardwareProfile, InputSource, PROFILE_SCHEMA_VERSION},
    storage::atomic_write,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub const SETTINGS_SCHEMA_VERSION: u16 = 2;
pub const BACKUP_SCHEMA_VERSION: u16 = 2;
const MAX_IMPORT_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: String,
    pub params: BTreeMap<String, String>,
    pub detail: Option<String>,
}

impl std::fmt::Display for AppError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(detail) = &self.detail {
            write!(formatter, "{}: {detail}", self.code)
        } else {
            formatter.write_str(&self.code)
        }
    }
}

impl std::error::Error for AppError {}

impl AppError {
    pub(crate) fn new(code: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            params: BTreeMap::new(),
            detail: None,
        }
    }

    pub(crate) fn with_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }

    pub(crate) fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub enum Language {
    #[default]
    #[serde(rename = "zh-CN")]
    ZhCn,
    #[serde(rename = "en-US")]
    EnUs,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct RuntimeAssignment {
    pub device_profile_id: String,
    pub hardware_profile_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeviceRecord {
    pub device_id: DeviceId,
    pub name: String,
    pub board_profile_id: String,
    pub runtime_assignment: Option<RuntimeAssignment>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct SettingsDocument {
    pub schema_version: u16,
    pub editor_profile: Option<String>,
    pub language: Language,
    #[serde(default)]
    pub devices: BTreeMap<DeviceId, DeviceRecord>,
}

impl Default for SettingsDocument {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            editor_profile: None,
            language: Language::ZhCn,
            devices: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct EditorSettingsPatch {
    pub schema_version: u16,
    pub editor_profile: Option<String>,
    pub language: Language,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceSnapshot {
    pub settings: SettingsDocument,
    pub profiles: BTreeMap<String, DeviceProfile>,
    pub metrics: MetricsBackup,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct BackupDocument {
    pub schema_version: u16,
    pub settings: SettingsDocument,
    pub profiles: Vec<DeviceProfile>,
    pub metrics: MetricsBackup,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub profile_id: String,
    pub profile_name: String,
    pub button_count: usize,
    pub hardware_binding_count: usize,
    pub action_count: usize,
    pub replaces_existing: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupPreview {
    pub profile_count: usize,
    pub button_count: usize,
    pub hardware_binding_count: usize,
    pub action_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssignmentResolution<'a> {
    UnknownDevice,
    Unassigned {
        device: &'a DeviceRecord,
    },
    Valid {
        device: &'a DeviceRecord,
        assignment: &'a RuntimeAssignment,
        profile: &'a DeviceProfile,
        hardware: &'a HardwareProfile,
    },
    InvalidAssignment {
        device: &'a DeviceRecord,
        assignment: &'a RuntimeAssignment,
    },
}

#[derive(Debug)]
pub struct Workspace {
    config_directory: PathBuf,
    pub settings: SettingsDocument,
    pub profiles: BTreeMap<String, DeviceProfile>,
}

impl Workspace {
    pub fn load(config_directory: &Path, bundled_profiles: &Path) -> Result<Self, AppError> {
        if config_directory.join("data/settings.yaml").exists() {
            Self::load_existing(config_directory)
        } else {
            Self::create(config_directory, load_bundled_profiles(bundled_profiles)?)
        }
    }

    pub fn create(config_directory: &Path, profiles: Vec<DeviceProfile>) -> Result<Self, AppError> {
        let profiles = collect_profiles(profiles)?;
        let settings = SettingsDocument {
            editor_profile: profiles.keys().next().cloned(),
            ..SettingsDocument::default()
        };
        write_new_data_directory(config_directory, &settings, &profiles)?;
        Ok(Self {
            config_directory: config_directory.to_owned(),
            settings,
            profiles,
        })
    }

    pub fn load_existing(config_directory: &Path) -> Result<Self, AppError> {
        let data_directory = config_directory.join("data");
        let settings = read_versioned_yaml(
            &data_directory.join("settings.yaml"),
            SETTINGS_SCHEMA_VERSION,
            "unsupported_settings_schema",
            false,
        )?;
        let mut profiles = BTreeMap::new();
        let profile_directory = data_directory.join("profiles");
        for path in yaml_files(&profile_directory, "read_profiles")? {
            let profile: DeviceProfile = read_versioned_yaml(
                &path,
                PROFILE_SCHEMA_VERSION,
                "unsupported_profile_schema",
                false,
            )?;
            profile.validate()?;
            validate_profile_filename(&path, &profile)?;
            if profiles
                .insert(profile.profile.id.clone(), profile)
                .is_some()
            {
                return Err(AppError::new("duplicate_profile"));
            }
        }
        validate_settings(&settings, &profiles)?;
        Ok(Self {
            config_directory: config_directory.to_owned(),
            settings,
            profiles,
        })
    }

    pub fn save_profile(&mut self, profile: DeviceProfile) -> Result<(), AppError> {
        profile.validate()?;
        let id = profile.profile.id.clone();
        let path = self.profile_directory().join(format!("{id}.yaml"));
        write_yaml(&path, &profile)?;
        if self.profiles.is_empty() && self.settings.editor_profile.is_none() {
            let mut settings = self.settings.clone();
            settings.editor_profile = Some(id.clone());
            if let Err(error) = self.persist_settings(&settings) {
                let _ = fs::remove_file(path);
                return Err(error);
            }
            self.settings = settings;
        }
        self.profiles.insert(id, profile);
        Ok(())
    }

    pub fn save_settings(&mut self, patch: EditorSettingsPatch) -> Result<(), AppError> {
        validate_editor_settings_patch(&patch, &self.profiles)?;
        let settings = SettingsDocument {
            schema_version: patch.schema_version,
            editor_profile: patch.editor_profile,
            language: patch.language,
            devices: self.settings.devices.clone(),
        };
        self.persist_settings(&settings)?;
        self.settings = settings;
        Ok(())
    }

    pub fn preview_profile(&self, path: &Path) -> Result<ImportPreview, AppError> {
        let profile = read_profile_limited(path)?;
        profile.validate()?;
        Ok(ImportPreview {
            profile_id: profile.profile.id.clone(),
            profile_name: profile.profile.name.clone(),
            button_count: button_count(&profile),
            hardware_binding_count: hardware_binding_count(&profile),
            action_count: action_count(&profile),
            replaces_existing: self.profiles.contains_key(&profile.profile.id),
        })
    }

    pub fn import_profile(&mut self, path: &Path) -> Result<(), AppError> {
        let profile = read_profile_limited(path)?;
        profile.validate()?;
        self.save_profile(profile)
    }

    pub fn export_profile(&self, id: &str, path: &Path) -> Result<(), AppError> {
        let profile = self
            .profiles
            .get(id)
            .ok_or_else(|| AppError::new("unknown_profile").with_param("profile", id))?;
        write_yaml(path, profile)
    }

    pub fn delete_profile(&mut self, id: &str) -> Result<(), AppError> {
        if !self.profiles.contains_key(id) {
            return Err(AppError::new("unknown_profile").with_param("profile", id));
        }
        let mut settings = self.settings.clone();
        if settings.editor_profile.as_deref() == Some(id) {
            settings.editor_profile = self
                .profiles
                .keys()
                .find(|profile_id| profile_id.as_str() != id)
                .cloned();
        }
        self.persist_settings(&settings)?;
        let path = self.profile_directory().join(format!("{id}.yaml"));
        if let Err(error) = fs::remove_file(&path) {
            let _ = self.persist_settings(&self.settings);
            return Err(io_error("delete_profile", &path, error));
        }
        self.profiles.remove(id);
        self.settings = settings;
        Ok(())
    }

    pub fn enroll_device(&mut self, id: DeviceId) -> Result<&DeviceRecord, AppError> {
        if !self.settings.devices.contains_key(&id) {
            let board = board_by_id(id.board_profile_id()).ok_or_else(|| {
                AppError::new("unknown_board_profile")
                    .with_param("board_profile", id.board_profile_id())
            })?;
            let suffix = id
                .hardware_serial()
                .chars()
                .rev()
                .take(6)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>();
            let record = DeviceRecord {
                device_id: id.clone(),
                name: format!("{} · {suffix}", board.display_name),
                board_profile_id: board.id.into(),
                runtime_assignment: None,
            };
            let mut settings = self.settings.clone();
            settings.devices.insert(id.clone(), record);
            self.persist_settings(&settings)?;
            self.settings = settings;
        }
        Ok(&self.settings.devices[&id])
    }

    pub fn rename_device(&mut self, id: &DeviceId, name: String) -> Result<(), AppError> {
        if name.trim().is_empty() {
            return Err(AppError::new("invalid_device_name"));
        }
        self.update_device(id, |record| record.name = name)
    }

    pub fn set_assignment(
        &mut self,
        id: &DeviceId,
        value: RuntimeAssignment,
    ) -> Result<(), AppError> {
        let device = self.device(id)?;
        let profile = self.profiles.get(&value.device_profile_id).ok_or_else(|| {
            AppError::new("unknown_profile").with_param("profile", &value.device_profile_id)
        })?;
        let hardware = profile
            .hardware_profile(&value.hardware_profile_id)
            .ok_or_else(|| {
                AppError::new("unknown_hardware_profile")
                    .with_param("hardware_profile", &value.hardware_profile_id)
            })?;
        if hardware.board_profile_id != device.board_profile_id {
            return Err(AppError::new("assignment_board_mismatch")
                .with_param("device_board_profile", &device.board_profile_id)
                .with_param("hardware_board_profile", &hardware.board_profile_id));
        }
        self.update_device(id, |record| record.runtime_assignment = Some(value))
    }

    pub fn clear_assignment(&mut self, id: &DeviceId) -> Result<(), AppError> {
        self.update_device(id, |record| record.runtime_assignment = None)
    }

    pub fn forget_offline_device(&mut self, id: &DeviceId, online: bool) -> Result<(), AppError> {
        self.device(id)?;
        if online {
            return Err(AppError::new("device_online"));
        }
        let mut settings = self.settings.clone();
        settings.devices.remove(id);
        self.persist_settings(&settings)?;
        self.settings = settings;
        Ok(())
    }

    pub fn assignment_resolution(&self, id: &DeviceId) -> AssignmentResolution<'_> {
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

    pub fn preview_backup(&self, path: &Path) -> Result<BackupPreview, AppError> {
        let snapshot = read_backup(path)?;
        Ok(BackupPreview {
            profile_count: snapshot.profiles.len(),
            button_count: snapshot.profiles.values().map(button_count).sum(),
            hardware_binding_count: snapshot.profiles.values().map(hardware_binding_count).sum(),
            action_count: snapshot.profiles.values().map(action_count).sum(),
        })
    }

    pub fn export_backup(&self, path: &Path, metrics: &MetricsStore) -> Result<(), AppError> {
        let metrics = metrics
            .backup()
            .map_err(|error| metrics_error("read_metrics_backup", error))?;
        write_yaml(
            path,
            &BackupDocument {
                schema_version: BACKUP_SCHEMA_VERSION,
                settings: self.settings.clone(),
                profiles: self.profiles.values().cloned().collect(),
                metrics,
            },
        )
    }

    pub fn restore_backup(&mut self, path: &Path, metrics: &MetricsStore) -> Result<(), AppError> {
        self.restore_backup_with_reopen(path, metrics, |store, path| {
            store
                .reopen(path)
                .map_err(|error| metrics_error("reopen_metrics", error))
        })
    }

    fn restore_backup_with_reopen(
        &mut self,
        path: &Path,
        metrics: &MetricsStore,
        reopen_metrics: impl FnOnce(&MetricsStore, &Path) -> Result<(), AppError>,
    ) -> Result<(), AppError> {
        let snapshot = read_backup(path)?;
        let data_directory = self.data_directory();
        let next_directory = self.config_directory.join("data.next");
        let previous_directory = self.config_directory.join("data.previous");
        remove_directory_if_exists(&next_directory)?;
        remove_directory_if_exists(&previous_directory)?;
        if let Err(error) =
            write_data_directory(&next_directory, &snapshot.settings, &snapshot.profiles)
        {
            let _ = fs::remove_dir_all(&next_directory);
            return Err(error);
        }
        let staged_metrics_path = next_directory.join("metrics.sqlite3");
        let staged_metrics = match MetricsStore::open(&staged_metrics_path) {
            Ok(metrics) => metrics,
            Err(error) => {
                let _ = fs::remove_dir_all(&next_directory);
                return Err(metrics_error("stage_metrics", error));
            }
        };
        if let Err(error) = staged_metrics.replace_from_backup(&snapshot.metrics) {
            drop(staged_metrics);
            let _ = fs::remove_dir_all(&next_directory);
            return Err(metrics_error("stage_metrics", error));
        }
        drop(staged_metrics);

        metrics.close();
        if let Err(error) = fs::rename(&data_directory, &previous_directory) {
            let _ = metrics.reopen(&data_directory.join("metrics.sqlite3"));
            let _ = fs::remove_dir_all(&next_directory);
            return Err(io_error("stage_restore", &data_directory, error));
        }
        if let Err(error) = fs::rename(&next_directory, &data_directory) {
            fs::rename(&previous_directory, &data_directory).map_err(|rollback| {
                io_error("rollback_previous_data", &previous_directory, rollback)
            })?;
            metrics
                .reopen(&data_directory.join("metrics.sqlite3"))
                .map_err(|reopen| metrics_error("reopen_rollback_metrics", reopen))?;
            remove_directory_if_exists(&next_directory)?;
            return Err(io_error("activate_restore", &next_directory, error));
        }

        let active_metrics_path = data_directory.join("metrics.sqlite3");
        if let Err(error) = reopen_metrics(metrics, &active_metrics_path) {
            rollback_data_swap(
                &data_directory,
                &next_directory,
                &previous_directory,
                metrics,
            )?;
            return Err(error);
        }
        let restored = match Self::load_existing(&self.config_directory) {
            Ok(restored) => restored,
            Err(error) => {
                metrics.close();
                rollback_data_swap(
                    &data_directory,
                    &next_directory,
                    &previous_directory,
                    metrics,
                )?;
                return Err(error);
            }
        };
        self.settings = restored.settings;
        self.profiles = restored.profiles;
        let _ = fs::remove_dir_all(&previous_directory);
        Ok(())
    }

    fn update_device(
        &mut self,
        id: &DeviceId,
        update: impl FnOnce(&mut DeviceRecord),
    ) -> Result<(), AppError> {
        self.device(id)?;
        let mut settings = self.settings.clone();
        update(settings.devices.get_mut(id).expect("device was checked"));
        self.persist_settings(&settings)?;
        self.settings = settings;
        Ok(())
    }

    fn device(&self, id: &DeviceId) -> Result<&DeviceRecord, AppError> {
        self.settings
            .devices
            .get(id)
            .ok_or_else(|| AppError::new("unknown_device").with_param("device_id", id.as_str()))
    }

    fn persist_settings(&self, settings: &SettingsDocument) -> Result<(), AppError> {
        write_yaml(&self.data_directory().join("settings.yaml"), settings)
    }

    fn data_directory(&self) -> PathBuf {
        self.config_directory.join("data")
    }

    fn profile_directory(&self) -> PathBuf {
        self.data_directory().join("profiles")
    }
}

fn collect_profiles(
    values: Vec<DeviceProfile>,
) -> Result<BTreeMap<String, DeviceProfile>, AppError> {
    let mut profiles = BTreeMap::new();
    for profile in values {
        profile.validate()?;
        if profiles
            .insert(profile.profile.id.clone(), profile)
            .is_some()
        {
            return Err(AppError::new("duplicate_profile"));
        }
    }
    Ok(profiles)
}

fn load_bundled_profiles(directory: &Path) -> Result<Vec<DeviceProfile>, AppError> {
    let mut profiles = Vec::new();
    for path in yaml_files(directory, "read_bundled_profiles")? {
        let profile: DeviceProfile = read_versioned_yaml(
            &path,
            PROFILE_SCHEMA_VERSION,
            "unsupported_profile_schema",
            true,
        )?;
        profile.validate()?;
        validate_profile_filename(&path, &profile)?;
        profiles.push(profile);
    }
    profiles.sort_by(|left, right| left.profile.id.cmp(&right.profile.id));
    Ok(profiles)
}

fn validate_settings(
    settings: &SettingsDocument,
    profiles: &BTreeMap<String, DeviceProfile>,
) -> Result<(), AppError> {
    if settings.schema_version != SETTINGS_SCHEMA_VERSION {
        return Err(AppError::new("unsupported_settings_schema"));
    }
    if let Some(editor_profile) = &settings.editor_profile
        && !profiles.contains_key(editor_profile)
    {
        return Err(AppError::new("unknown_editor_profile").with_param("profile", editor_profile));
    }
    for (id, device) in &settings.devices {
        if id != &device.device_id {
            return Err(AppError::new("device_id_mismatch").with_param("device_id", id.as_str()));
        }
        let board = board_by_id(&device.board_profile_id).ok_or_else(|| {
            AppError::new("unknown_board_profile")
                .with_param("board_profile", &device.board_profile_id)
        })?;
        if id.board_profile_id() != board.id {
            return Err(AppError::new("device_board_mismatch").with_param("device_id", id.as_str()));
        }
        if device.name.trim().is_empty() {
            return Err(AppError::new("invalid_device_name").with_param("device_id", id.as_str()));
        }
    }
    Ok(())
}

fn validate_editor_settings_patch(
    patch: &EditorSettingsPatch,
    profiles: &BTreeMap<String, DeviceProfile>,
) -> Result<(), AppError> {
    if patch.schema_version != SETTINGS_SCHEMA_VERSION {
        return Err(AppError::new("unsupported_settings_schema"));
    }
    if let Some(editor_profile) = &patch.editor_profile
        && !profiles.contains_key(editor_profile)
    {
        return Err(AppError::new("unknown_editor_profile").with_param("profile", editor_profile));
    }
    Ok(())
}

fn read_backup(path: &Path) -> Result<WorkspaceSnapshot, AppError> {
    let backup: BackupDocument = read_versioned_yaml(
        path,
        BACKUP_SCHEMA_VERSION,
        "unsupported_backup_schema",
        false,
    )?;
    let profiles = collect_profiles(backup.profiles)?;
    validate_settings(&backup.settings, &profiles)?;
    backup
        .metrics
        .validate()
        .map_err(|error| metrics_error("invalid_backup_metrics", error))?;
    Ok(WorkspaceSnapshot {
        settings: backup.settings,
        profiles,
        metrics: backup.metrics,
    })
}

fn button_count(profile: &DeviceProfile) -> usize {
    profile
        .profile
        .groups
        .iter()
        .map(|group| group.buttons.len())
        .sum()
}

fn hardware_binding_count(profile: &DeviceProfile) -> usize {
    profile
        .hardware_profiles
        .iter()
        .flat_map(|hardware| &hardware.inputs)
        .map(|input| match input {
            InputSource::Direct { keys, .. } => keys.len(),
            InputSource::ContactMatrix { keys, .. } => keys.len(),
        })
        .sum()
}

fn action_count(profile: &DeviceProfile) -> usize {
    profile.actions.values().map(Vec::len).sum()
}

fn validate_profile_filename(path: &Path, profile: &DeviceProfile) -> Result<(), AppError> {
    let expected = format!("{}.yaml", profile.profile.id);
    if path.file_name().and_then(|value| value.to_str()) != Some(&expected) {
        return Err(
            AppError::new("profile_filename_mismatch").with_param("profile", &profile.profile.id)
        );
    }
    Ok(())
}

fn yaml_files(directory: &Path, code: &str) -> Result<Vec<PathBuf>, AppError> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(directory).map_err(|error| io_error(code, directory, error))? {
        let entry = entry.map_err(|error| io_error(code, directory, error))?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) == Some("yaml") {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn remove_directory_if_exists(path: &Path) -> Result<(), AppError> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("remove_staging_directory", path, error)),
    }
}

fn rollback_data_swap(
    data_directory: &Path,
    next_directory: &Path,
    previous_directory: &Path,
    metrics: &MetricsStore,
) -> Result<(), AppError> {
    metrics.close();
    fs::rename(data_directory, next_directory)
        .map_err(|error| io_error("rollback_restored_data", data_directory, error))?;
    if let Err(error) = fs::rename(previous_directory, data_directory) {
        let _ = fs::rename(next_directory, data_directory);
        let _ = metrics.reopen(&data_directory.join("metrics.sqlite3"));
        return Err(io_error(
            "rollback_previous_data",
            previous_directory,
            error,
        ));
    }
    metrics
        .reopen(&data_directory.join("metrics.sqlite3"))
        .map_err(|error| metrics_error("reopen_rollback_metrics", error))?;
    remove_directory_if_exists(next_directory)
}

fn write_data_directory(
    data_directory: &Path,
    settings: &SettingsDocument,
    profiles: &BTreeMap<String, DeviceProfile>,
) -> Result<(), AppError> {
    let profile_directory = data_directory.join("profiles");
    fs::create_dir_all(&profile_directory)
        .map_err(|error| io_error("create_data", &profile_directory, error))?;
    for profile in profiles.values() {
        write_yaml(
            &profile_directory.join(format!("{}.yaml", profile.profile.id)),
            profile,
        )?;
    }
    write_yaml(&data_directory.join("settings.yaml"), settings)
}

fn write_new_data_directory(
    config_directory: &Path,
    settings: &SettingsDocument,
    profiles: &BTreeMap<String, DeviceProfile>,
) -> Result<(), AppError> {
    let next_directory = config_directory.join("data.next");
    remove_directory_if_exists(&next_directory)?;
    write_data_directory(&next_directory, settings, profiles)?;
    let data_directory = config_directory.join("data");
    fs::rename(&next_directory, &data_directory)
        .map_err(|error| io_error("activate_data", &data_directory, error))
}

#[derive(Deserialize)]
struct SchemaHeader {
    schema_version: u16,
}

fn read_profile_limited(path: &Path) -> Result<DeviceProfile, AppError> {
    read_versioned_yaml(
        path,
        PROFILE_SCHEMA_VERSION,
        "unsupported_profile_schema",
        true,
    )
}

fn read_versioned_yaml<T: for<'de> Deserialize<'de>>(
    path: &Path,
    expected: u16,
    unsupported_code: &str,
    limited: bool,
) -> Result<T, AppError> {
    if limited {
        let metadata = fs::metadata(path).map_err(|error| io_error("read_file", path, error))?;
        if metadata.len() > MAX_IMPORT_BYTES {
            return Err(
                AppError::new("file_too_large").with_param("limit", MAX_IMPORT_BYTES.to_string())
            );
        }
    }
    let contents = fs::read_to_string(path).map_err(|error| io_error("read_file", path, error))?;
    let header: SchemaHeader = serde_yaml_ng::from_str(&contents)
        .map_err(|error| AppError::new("invalid_yaml").with_detail(error.to_string()))?;
    if header.schema_version != expected {
        return Err(AppError::new(unsupported_code));
    }
    serde_yaml_ng::from_str(&contents)
        .map_err(|error| AppError::new("invalid_yaml").with_detail(error.to_string()))
}

fn write_yaml(path: &Path, value: &impl Serialize) -> Result<(), AppError> {
    let yaml = serde_yaml_ng::to_string(value)
        .map_err(|error| AppError::new("serialize_yaml").with_detail(error.to_string()))?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| io_error("create_directory", parent, error))?;
    atomic_write(path, yaml.as_bytes())
        .map_err(|detail| AppError::new("write_file").with_detail(detail))
}

fn io_error(code: &str, path: &Path, error: std::io::Error) -> AppError {
    AppError::new(code)
        .with_param("path", path.display().to_string())
        .with_detail(error.to_string())
}

fn metrics_error(code: &str, error: rusqlite::Error) -> AppError {
    AppError::new(code).with_detail(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        metrics::{MetricAttribution, MetricsStore},
        model::{ButtonDefinition, ButtonGroup, ModelLayout},
        profile::ButtonAction,
    };
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "kivo-workspace-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed),
            ));
            fs::create_dir(&path).unwrap();
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

    fn layout() -> ModelLayout {
        ModelLayout {
            id: "red-phone-v1".into(),
            name: "红色电话机".into(),
            groups: vec![ButtonGroup {
                id: "digits".into(),
                columns: 3,
                buttons: ["A", "B", "C"]
                    .into_iter()
                    .map(|id| ButtonDefinition {
                        id: id.into(),
                        label: id.into(),
                    })
                    .collect(),
            }],
        }
    }

    fn hardware(id: &str, board_profile_id: &str) -> HardwareProfile {
        HardwareProfile {
            id: id.into(),
            name: id.into(),
            board_profile_id: board_profile_id.into(),
            debounce_ms: 30,
            inputs: Vec::new(),
        }
    }

    fn device_profile() -> DeviceProfile {
        DeviceProfile {
            schema_version: PROFILE_SCHEMA_VERSION,
            profile: layout(),
            hardware_profiles: vec![
                hardware("esp-primary", "luatos-esp32s3-aio"),
                hardware("esp-alternate", "luatos-esp32s3-aio"),
                hardware("rp-primary", "vccgnd-yd-rp2040"),
            ],
            actions: BTreeMap::from([(
                "A".into(),
                vec![
                    ButtonAction::Paste {
                        text: "你好\n".into(),
                    },
                    ButtonAction::Hotkey {
                        keys: vec!["enter".into()],
                    },
                ],
            )]),
        }
    }

    fn workspace(directory: &TestDirectory) -> Workspace {
        Workspace::create(&directory.0, vec![device_profile()]).unwrap()
    }

    #[test]
    fn device_profile_round_trips_multiple_hardware_profiles_for_both_boards() {
        let profile = device_profile();
        profile.validate().unwrap();
        let yaml = serde_yaml_ng::to_string(&profile).unwrap();
        let loaded: DeviceProfile = serde_yaml_ng::from_str(&yaml).unwrap();
        assert_eq!(loaded, profile);
        assert_eq!(loaded.compatible_hardware("luatos-esp32s3-aio").len(), 2);
        assert_eq!(loaded.compatible_hardware("vccgnd-yd-rp2040").len(), 1);
    }

    #[test]
    fn profile_validation_rejects_duplicate_hardware_ids_and_unsafe_pins() {
        let mut duplicate = device_profile();
        duplicate.hardware_profiles[1].id = "esp-primary".into();
        assert_eq!(
            duplicate.validate().unwrap_err().code,
            "duplicate_hardware_profile"
        );
        let mut unsafe_pin = device_profile();
        unsafe_pin.hardware_profiles[2].inputs = vec![InputSource::Direct {
            id: "direct".into(),
            keys: BTreeMap::from([("A".into(), 23)]),
        }];
        assert_eq!(unsafe_pin.validate().unwrap_err().code, "unsupported_gpio");
    }

    #[test]
    fn profile_validation_rejects_invalid_group_and_button_ids() {
        let mut invalid_group = device_profile();
        invalid_group.profile.groups[0].id = "bad group".into();
        assert_eq!(
            invalid_group.validate().unwrap_err().code,
            "invalid_group_id"
        );

        let mut invalid_button = device_profile();
        invalid_button.profile.groups[0].buttons[0].id = "bad button".into();
        assert_eq!(
            invalid_button.validate().unwrap_err().code,
            "invalid_button_id"
        );
    }

    #[test]
    fn profile_validation_keeps_matrix_and_action_rules() {
        let mut profile = device_profile();
        profile.hardware_profiles[0].inputs = vec![InputSource::ContactMatrix {
            id: "keys".into(),
            pins: vec![1, 2, 3],
            keys: BTreeMap::from([
                ("A".into(), [1, 2]),
                ("B".into(), [2, 3]),
                ("C".into(), [3, 1]),
            ]),
        }];
        assert_eq!(profile.validate().unwrap_err().code, "matrix_not_bipartite");
    }

    #[test]
    fn schema_v1_profiles_settings_and_backups_are_rejected() {
        let directory = TestDirectory::new();
        let mut profile = device_profile();
        profile.schema_version = 1;
        assert_eq!(
            profile.validate().unwrap_err().code,
            "unsupported_profile_schema"
        );
        let old_profile_path = directory.path("old-profile.yaml");
        fs::write(
            &old_profile_path,
            "schema_version: 1\nmodel: { id: old, name: Old, groups: [] }\nhardware: {}\n",
        )
        .unwrap();
        let workspace = workspace(&directory);
        assert_eq!(
            workspace
                .preview_profile(&old_profile_path)
                .unwrap_err()
                .code,
            "unsupported_profile_schema"
        );
        let config_directory = directory.path("config");
        let data_directory = config_directory.join("data");
        fs::create_dir_all(data_directory.join("profiles")).unwrap();
        fs::write(
            data_directory.join("settings.yaml"),
            "schema_version: 1\nactive_model: red-phone-v1\nlanguage: zh-CN\n",
        )
        .unwrap();
        assert_eq!(
            Workspace::load_existing(&config_directory)
                .unwrap_err()
                .code,
            "unsupported_settings_schema"
        );
        let backup_path = directory.path("backup.yaml");
        fs::write(&backup_path, "schema_version: 1\nmodels: []\n").unwrap();
        assert_eq!(
            workspace.preview_backup(&backup_path).unwrap_err().code,
            "unsupported_backup_schema"
        );
    }

    #[test]
    fn settings_reject_mismatched_malformed_and_unknown_board_device_ids() {
        let id = DeviceId::new("luatos-esp32s3-aio", "ABCDEF123456").unwrap();
        let other = DeviceId::new("luatos-esp32s3-aio", "654321FEDCBA").unwrap();
        let settings = SettingsDocument {
            devices: BTreeMap::from([(
                id,
                DeviceRecord {
                    device_id: other,
                    name: "Desk".into(),
                    board_profile_id: "luatos-esp32s3-aio".into(),
                    runtime_assignment: None,
                },
            )]),
            ..SettingsDocument::default()
        };
        assert_eq!(
            validate_settings(&settings, &BTreeMap::new())
                .unwrap_err()
                .code,
            "device_id_mismatch"
        );
        let mut unknown = SettingsDocument::default();
        let id = DeviceId::new("luatos-esp32s3-aio", "ABCDEF123456").unwrap();
        unknown.devices.insert(
            id.clone(),
            DeviceRecord {
                device_id: id,
                name: "Desk".into(),
                board_profile_id: "unknown-board".into(),
                runtime_assignment: None,
            },
        );
        assert_eq!(
            validate_settings(&unknown, &BTreeMap::new())
                .unwrap_err()
                .code,
            "unknown_board_profile"
        );
        assert!(serde_yaml_ng::from_str::<SettingsDocument>(
            "schema_version: 2\neditor_profile: null\nlanguage: zh-CN\ndevices:\n  malformed:\n    device_id: malformed\n    name: Desk\n    board_profile_id: luatos-esp32s3-aio\n    runtime_assignment: null\n"
        ).is_err());
    }

    #[test]
    fn enrollment_is_idempotent_and_persists_a_default_name() {
        let directory = TestDirectory::new();
        let mut workspace = workspace(&directory);
        let id = DeviceId::new("vccgnd-yd-rp2040", "E0C9125B0D9B").unwrap();
        let first = workspace.enroll_device(id.clone()).unwrap().clone();
        let second = workspace.enroll_device(id.clone()).unwrap().clone();
        assert_eq!(first, second);
        assert_eq!(first.name, "VCC-GND YD-RP2040 · 5B0D9B");
        assert_eq!(
            Workspace::load_existing(&directory.0)
                .unwrap()
                .settings
                .devices[&id],
            first
        );
    }

    #[test]
    fn assignments_require_exact_board_equality() {
        let directory = TestDirectory::new();
        let mut workspace = workspace(&directory);
        let id = DeviceId::new("vccgnd-yd-rp2040", "E0C9125B0D9B").unwrap();
        workspace.enroll_device(id.clone()).unwrap();
        assert_eq!(
            workspace
                .set_assignment(
                    &id,
                    RuntimeAssignment {
                        device_profile_id: "red-phone-v1".into(),
                        hardware_profile_id: "esp-primary".into(),
                    }
                )
                .unwrap_err()
                .code,
            "assignment_board_mismatch"
        );
        let assignment = RuntimeAssignment {
            device_profile_id: "red-phone-v1".into(),
            hardware_profile_id: "rp-primary".into(),
        };
        workspace.set_assignment(&id, assignment.clone()).unwrap();
        assert!(
            matches!(workspace.assignment_resolution(&id), AssignmentResolution::Valid { hardware, .. } if hardware.id == "rp-primary")
        );
        assert_eq!(
            workspace.settings.devices[&id].runtime_assignment,
            Some(assignment)
        );
    }

    #[test]
    fn editor_settings_patch_ignores_stale_device_mutations() {
        let directory = TestDirectory::new();
        let mut workspace = workspace(&directory);
        let removed_id = DeviceId::new("luatos-esp32s3-aio", "ABCDEF123456").unwrap();
        let changed_id = DeviceId::new("luatos-esp32s3-aio", "654321FEDCBA").unwrap();
        let added_id = DeviceId::new("luatos-esp32s3-aio", "ADDED123456").unwrap();
        let assignment = RuntimeAssignment {
            device_profile_id: "red-phone-v1".into(),
            hardware_profile_id: "esp-primary".into(),
        };
        workspace.enroll_device(removed_id.clone()).unwrap();
        workspace.enroll_device(changed_id.clone()).unwrap();
        workspace
            .set_assignment(&changed_id, assignment.clone())
            .unwrap();
        workspace
            .rename_device(&changed_id, "Authoritative".into())
            .unwrap();
        let authoritative_devices = workspace.settings.devices.clone();

        let mut stale = workspace.settings.clone();
        stale.language = Language::EnUs;
        stale.devices.remove(&removed_id);
        stale.devices.get_mut(&changed_id).unwrap().name = "Stale rename".into();
        stale
            .devices
            .get_mut(&changed_id)
            .unwrap()
            .runtime_assignment = None;
        stale.devices.insert(
            added_id.clone(),
            DeviceRecord {
                device_id: added_id,
                name: "Stale addition".into(),
                board_profile_id: "luatos-esp32s3-aio".into(),
                runtime_assignment: Some(assignment),
            },
        );
        let patch: EditorSettingsPatch =
            serde_json::from_value(serde_json::to_value(stale).unwrap()).unwrap();

        workspace.save_settings(patch).unwrap();

        assert_eq!(workspace.settings.language, Language::EnUs);
        assert_eq!(workspace.settings.devices, authoritative_devices);
        assert_eq!(
            Workspace::load_existing(&directory.0)
                .unwrap()
                .settings
                .devices,
            authoritative_devices
        );
    }

    #[test]
    fn invalid_editor_settings_patch_rolls_back_memory_and_disk() {
        let directory = TestDirectory::new();
        let mut workspace = workspace(&directory);
        let id = DeviceId::new("luatos-esp32s3-aio", "ABCDEF123456").unwrap();
        workspace.enroll_device(id).unwrap();
        let settings_before = workspace.settings.clone();
        let settings_path = directory.path("data/settings.yaml");
        let disk_before = fs::read(&settings_path).unwrap();

        let error = workspace
            .save_settings(EditorSettingsPatch {
                schema_version: SETTINGS_SCHEMA_VERSION,
                editor_profile: Some("missing-profile".into()),
                language: Language::EnUs,
            })
            .unwrap_err();

        assert_eq!(error.code, "unknown_editor_profile");
        assert_eq!(workspace.settings, settings_before);
        assert_eq!(fs::read(settings_path).unwrap(), disk_before);
    }

    #[test]
    fn editor_settings_patch_rejects_schema_v1() {
        let directory = TestDirectory::new();
        let mut workspace = workspace(&directory);

        let error = workspace
            .save_settings(EditorSettingsPatch {
                schema_version: 1,
                editor_profile: Some("red-phone-v1".into()),
                language: Language::EnUs,
            })
            .unwrap_err();

        assert_eq!(error.code, "unsupported_settings_schema");
    }

    #[test]
    fn broken_assignments_are_retained_without_compatible_fallback() {
        let directory = TestDirectory::new();
        let mut workspace = workspace(&directory);
        let id = DeviceId::new("luatos-esp32s3-aio", "ABCDEF123456").unwrap();
        workspace.enroll_device(id.clone()).unwrap();
        let assignment = RuntimeAssignment {
            device_profile_id: "red-phone-v1".into(),
            hardware_profile_id: "esp-primary".into(),
        };
        workspace.set_assignment(&id, assignment.clone()).unwrap();
        let mut edited = device_profile();
        edited.hardware_profiles.remove(0);
        workspace.save_profile(edited).unwrap();
        assert!(
            matches!(workspace.assignment_resolution(&id), AssignmentResolution::InvalidAssignment { assignment: retained, .. } if retained == &assignment)
        );
        assert_eq!(
            workspace.settings.devices[&id].runtime_assignment.as_ref(),
            Some(&assignment)
        );
        let mut incompatible = device_profile();
        incompatible.hardware_profiles[0].board_profile_id = "vccgnd-yd-rp2040".into();
        workspace.save_profile(incompatible).unwrap();
        assert!(matches!(
            workspace.assignment_resolution(&id),
            AssignmentResolution::InvalidAssignment { .. }
        ));
    }

    #[test]
    fn clear_rename_and_forget_are_durable_transactions() {
        let directory = TestDirectory::new();
        let mut workspace = workspace(&directory);
        let id = DeviceId::new("luatos-esp32s3-aio", "ABCDEF123456").unwrap();
        workspace.enroll_device(id.clone()).unwrap();
        workspace
            .set_assignment(
                &id,
                RuntimeAssignment {
                    device_profile_id: "red-phone-v1".into(),
                    hardware_profile_id: "esp-primary".into(),
                },
            )
            .unwrap();
        workspace.rename_device(&id, "Front desk".into()).unwrap();
        workspace.clear_assignment(&id).unwrap();
        assert_eq!(
            workspace.forget_offline_device(&id, true).unwrap_err().code,
            "device_online"
        );
        workspace.forget_offline_device(&id, false).unwrap();
        assert!(!workspace.settings.devices.contains_key(&id));
        assert!(workspace.profiles.contains_key("red-phone-v1"));
        let reloaded = Workspace::load_existing(&directory.0).unwrap();
        assert!(!reloaded.settings.devices.contains_key(&id));
        assert!(reloaded.profiles.contains_key("red-phone-v1"));
    }

    #[test]
    fn preview_export_and_button_lookup_use_complete_profiles() {
        let directory = TestDirectory::new();
        let workspace = workspace(&directory);
        let path = directory.path("red-phone-v1.yaml");
        workspace.export_profile("red-phone-v1", &path).unwrap();
        let preview = workspace.preview_profile(&path).unwrap();
        assert_eq!(
            (
                preview.button_count,
                preview.hardware_binding_count,
                preview.action_count
            ),
            (3, 0, 2)
        );
        assert!(preview.replaces_existing);
        let mut profile = device_profile();
        profile.hardware_profiles[0].inputs = vec![InputSource::Direct {
            id: "direct".into(),
            keys: BTreeMap::from([("A".into(), 6)]),
        }];
        profile.hardware_profiles[1].inputs = vec![InputSource::Direct {
            id: "direct".into(),
            keys: BTreeMap::from([("B".into(), 6)]),
        }];
        assert_eq!(
            profile.button_for(
                "esp-primary",
                &crate::protocol::PhysicalInput::Direct { gpio: 6 }
            ),
            Some("A")
        );
        assert_eq!(
            profile.button_for(
                "esp-alternate",
                &crate::protocol::PhysicalInput::Direct { gpio: 6 }
            ),
            Some("B")
        );
    }

    #[test]
    fn failed_metrics_reopen_rolls_back_settings_profiles_and_metrics() {
        let directory = TestDirectory::new();
        let source_directory = directory.path("source");
        let target_directory = directory.path("target");
        let mut source = Workspace::create(&source_directory, vec![device_profile()]).unwrap();
        source
            .save_settings(EditorSettingsPatch {
                schema_version: SETTINGS_SCHEMA_VERSION,
                editor_profile: Some("red-phone-v1".into()),
                language: Language::EnUs,
            })
            .unwrap();
        let source_metrics =
            MetricsStore::open(&source_directory.join("data/metrics.sqlite3")).unwrap();
        let device = DeviceId::new("luatos-esp32s3-aio", "AAAAAAAAAAAA").unwrap();
        let attribution = MetricAttribution {
            device_id: device.clone(),
            device_name: "Backup desk".into(),
            device_profile_id: "red-phone-v1".into(),
            hardware_profile_id: "esp-primary".into(),
        };
        source_metrics
            .record_button_press(&attribution, "A", 1_720_086_400_000)
            .unwrap();
        let backup_path = directory.path("backup.yaml");
        source.export_backup(&backup_path, &source_metrics).unwrap();

        let mut target = Workspace::create(&target_directory, vec![device_profile()]).unwrap();
        let target_metrics =
            MetricsStore::open(&target_directory.join("data/metrics.sqlite3")).unwrap();
        let original_attribution = MetricAttribution {
            device_name: "Original desk".into(),
            ..attribution
        };
        target_metrics
            .record_button_press(&original_attribution, "B", 1_720_086_400_001)
            .unwrap();
        let original_settings = target.settings.clone();
        let original_metrics = target_metrics.backup().unwrap();

        let error = target
            .restore_backup_with_reopen(&backup_path, &target_metrics, |_metrics, _path| {
                Err(AppError::new("injected_metrics_reopen_failure"))
            })
            .unwrap_err();

        assert_eq!(error.code, "injected_metrics_reopen_failure");
        assert_eq!(target.settings, original_settings);
        assert_eq!(target_metrics.backup().unwrap(), original_metrics);
        assert_eq!(
            Workspace::load_existing(&target_directory)
                .unwrap()
                .settings,
            original_settings
        );
        assert!(!target_directory.join("data.previous").exists());
        assert!(!target_directory.join("data.next").exists());
    }

    #[test]
    fn full_backup_can_reopen_an_export_larger_than_the_profile_import_limit() {
        let directory = TestDirectory::new();
        let workspace = workspace(&directory);
        let metrics = MetricsStore::open(&directory.path("data/metrics.sqlite3")).unwrap();
        let attribution = MetricAttribution {
            device_id: DeviceId::new("luatos-esp32s3-aio", "AAAAAAAAAAAA").unwrap(),
            device_name: "D".repeat(MAX_IMPORT_BYTES as usize + 1),
            device_profile_id: "red-phone-v1".into(),
            hardware_profile_id: "esp-primary".into(),
        };
        metrics
            .record_button_press(&attribution, "A", 1_720_086_400_000)
            .unwrap();
        let backup_path = directory.path("large-backup.yaml");
        workspace.export_backup(&backup_path, &metrics).unwrap();
        assert!(fs::metadata(&backup_path).unwrap().len() > MAX_IMPORT_BYTES);

        let preview = workspace.preview_backup(&backup_path).unwrap();

        assert_eq!(preview.profile_count, 1);
    }

    #[test]
    fn bundled_loader_accepts_only_yaml_v2_profiles() {
        let directory = TestDirectory::new();
        fs::write(directory.path("ignored.json"), "{}").unwrap();
        fs::write(
            directory.path("red-phone-v1.yaml"),
            serde_yaml_ng::to_string(&device_profile()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            load_bundled_profiles(&directory.0).unwrap(),
            vec![device_profile()]
        );
    }

    #[test]
    fn bundled_product_profile_is_valid_v2_yaml() {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../models/prod");
        let profiles = load_bundled_profiles(&directory).unwrap();
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].profile.id, "tel001");
        assert_eq!(profiles[0].hardware_profiles.len(), 1);
        assert_eq!(
            profiles[0].hardware_profiles[0].board_profile_id,
            "luatos-esp32s3-aio"
        );
    }

    #[test]
    fn rejects_imports_larger_than_ten_mibibytes() {
        let directory = TestDirectory::new();
        let workspace = workspace(&directory);
        let path = directory.path("too-large.yaml");
        fs::File::create(&path)
            .unwrap()
            .set_len(MAX_IMPORT_BYTES + 1)
            .unwrap();
        assert_eq!(
            workspace.preview_profile(&path).unwrap_err().code,
            "file_too_large"
        );
    }
}
