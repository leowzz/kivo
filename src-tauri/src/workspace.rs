use crate::{
    hardware::{
        DeviceId, HardwareRegistry, board_by_id, canonical_board_profile_id, compiled_registry,
    },
    metrics::{MetricsBackup, MetricsStore},
    product::ProductDefinition,
    profile::{
        ButtonAction, CreateDeviceProfileRequest, DeviceProfile, HardwareProfile, InputSource,
        PROFILE_SCHEMA_VERSION, TriggerActions, TriggerSettings, blank_device_profile,
    },
    storage::atomic_write,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub const SETTINGS_SCHEMA_VERSION: u16 = 3;
pub const BACKUP_SCHEMA_VERSION: u16 = 2;
pub const USER_BACKUP_SCHEMA_VERSION: u16 = 1;
const LEGACY_SCHEMA_VERSION: u16 = 1;
const PREVIOUS_SETTINGS_SCHEMA_VERSION: u16 = 2;
const PREVIOUS_PROFILE_SCHEMA_VERSION: u16 = 2;
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
pub struct DuplicateProfileForDeviceRequest {
    pub device_id: DeviceId,
    pub source_profile: DeviceProfile,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeviceRecord {
    pub device_id: DeviceId,
    pub name: String,
    pub board_profile_id: String,
    pub runtime_assignment: Option<RuntimeAssignment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product_config: Option<ProductDeviceConfig>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProductDeviceConfig {
    pub product_version_id: String,
    #[serde(default)]
    pub trigger_settings: TriggerSettings,
    #[serde(default)]
    pub actions: BTreeMap<String, TriggerActions>,
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

#[derive(Deserialize)]
struct LegacySettingsDocument {
    active_model: Option<String>,
    language: Language,
}

#[derive(Deserialize)]
struct LegacyHardwareConfig {
    controller: String,
    #[serde(default = "default_legacy_debounce_ms")]
    debounce_ms: u16,
    #[serde(default)]
    inputs: Vec<InputSource>,
}

fn default_legacy_debounce_ms() -> u16 {
    30
}

#[derive(Deserialize)]
struct LegacyModelConfig {
    schema_version: u16,
    model: crate::model::ModelLayout,
    hardware: LegacyHardwareConfig,
    #[serde(default)]
    actions: BTreeMap<String, Vec<ButtonAction>>,
}

#[derive(Deserialize)]
struct SchemaV2DeviceProfile {
    schema_version: u16,
    profile: crate::model::ModelLayout,
    #[serde(default)]
    hardware_profiles: Vec<HardwareProfile>,
    #[serde(default)]
    actions: BTreeMap<String, Vec<ButtonAction>>,
}

struct ReadProfile {
    profile: DeviceProfile,
    migrated: bool,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupKind {
    ProductDevices,
    Full,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UserBackupDevice {
    pub device_id: DeviceId,
    pub product_version_id: String,
    #[serde(default)]
    pub trigger_settings: TriggerSettings,
    #[serde(default)]
    pub actions: BTreeMap<String, TriggerActions>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UserBackupDocument {
    pub schema_version: u16,
    pub kind: BackupKind,
    #[serde(default)]
    pub devices: Vec<UserBackupDevice>,
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
    pub kind: BackupKind,
    pub profile_count: usize,
    pub button_count: usize,
    pub hardware_binding_count: usize,
    pub action_count: usize,
    pub device_count: usize,
    pub assignment_count: usize,
    pub metric_row_count: usize,
    pub activity_count: usize,
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

trait RestoreOperations {
    fn rename(&mut self, from: &Path, to: &Path) -> std::io::Result<()>;
    fn remove_dir_all(&mut self, path: &Path) -> std::io::Result<()>;
    fn reopen_metrics(
        &mut self,
        metrics: &MetricsStore,
        path: &Path,
    ) -> Result<(), rusqlite::Error>;
}

struct SystemRestoreOperations;

impl RestoreOperations for SystemRestoreOperations {
    fn rename(&mut self, from: &Path, to: &Path) -> std::io::Result<()> {
        fs::rename(from, to)
    }

    fn remove_dir_all(&mut self, path: &Path) -> std::io::Result<()> {
        fs::remove_dir_all(path)
    }

    fn reopen_metrics(
        &mut self,
        metrics: &MetricsStore,
        path: &Path,
    ) -> Result<(), rusqlite::Error> {
        metrics.reopen(path)
    }
}

trait DataGenerationOperations {
    fn rename(&mut self, from: &Path, to: &Path) -> std::io::Result<()>;

    fn remove_dir_all(&mut self, path: &Path) -> std::io::Result<()>;

    fn close_metrics(&mut self, metrics: &MetricsStore);

    fn reopen_metrics(
        &mut self,
        metrics: &MetricsStore,
        path: &Path,
    ) -> Result<(), rusqlite::Error>;
}

struct SystemDataGenerationOperations;

impl DataGenerationOperations for SystemDataGenerationOperations {
    fn rename(&mut self, from: &Path, to: &Path) -> std::io::Result<()> {
        fs::rename(from, to)
    }

    fn remove_dir_all(&mut self, path: &Path) -> std::io::Result<()> {
        fs::remove_dir_all(path)
    }

    fn close_metrics(&mut self, metrics: &MetricsStore) {
        metrics.close();
    }

    fn reopen_metrics(
        &mut self,
        metrics: &MetricsStore,
        path: &Path,
    ) -> Result<(), rusqlite::Error> {
        metrics.reopen(path)
    }
}

impl Workspace {
    pub fn load(config_directory: &Path, bundled_profiles: &Path) -> Result<Self, AppError> {
        Self::recover_interrupted_schema_v1_migration(config_directory)?;
        Self::recover_interrupted_data_generation(config_directory)?;
        let settings_path = config_directory.join("data/settings.yaml");
        if settings_path.exists() {
            if read_schema_header(&settings_path)?.schema_version == LEGACY_SCHEMA_VERSION {
                Self::migrate_schema_v1(config_directory)
            } else {
                Self::load_existing(config_directory)
            }
        } else {
            Self::create(config_directory, load_bundled_profiles(bundled_profiles)?)
        }
    }

    fn recover_interrupted_schema_v1_migration(config_directory: &Path) -> Result<(), AppError> {
        let data_directory = config_directory.join("data");
        let backup_directory = config_directory.join("data.v1.backup");
        if data_directory.exists() || !backup_directory.exists() {
            return Ok(());
        }
        let next_directory = config_directory.join("data.next");
        if next_directory.exists()
            && Self::load_data_directory(config_directory, &next_directory).is_ok()
            && fs::rename(&next_directory, &data_directory).is_ok()
        {
            return Ok(());
        }
        fs::rename(&backup_directory, &data_directory)
            .map_err(|error| io_error("recover_schema_v1_data", &backup_directory, error))
    }

    fn recover_interrupted_data_generation(config_directory: &Path) -> Result<(), AppError> {
        let data_directory = config_directory.join("data");
        let next_directory = config_directory.join("data.next");
        let previous_directory = config_directory.join("data.previous");
        if data_directory.exists() {
            remove_directory_if_exists(&next_directory)?;
            remove_directory_if_exists(&previous_directory)?;
            return Ok(());
        }

        match (previous_directory.exists(), next_directory.exists()) {
            (false, false) => Ok(()),
            (true, false) => fs::rename(&previous_directory, &data_directory)
                .map_err(|error| io_error("recover_previous_data", &previous_directory, error)),
            (false, true) => {
                if Self::load_data_directory(config_directory, &next_directory).is_err() {
                    return Err(AppError::new("recover_data_generation_failed"));
                }
                fs::rename(&next_directory, &data_directory)
                    .map_err(|error| io_error("recover_next_data", &next_directory, error))
            }
            (true, true) => {
                if Self::load_data_directory(config_directory, &next_directory).is_ok() {
                    fs::rename(&next_directory, &data_directory)
                        .map_err(|error| io_error("recover_next_data", &next_directory, error))?;
                    remove_directory_if_exists(&previous_directory)
                } else {
                    fs::rename(&previous_directory, &data_directory).map_err(|error| {
                        io_error("recover_previous_data", &previous_directory, error)
                    })?;
                    remove_directory_if_exists(&next_directory)
                }
            }
        }
    }

    fn migrate_schema_v1(config_directory: &Path) -> Result<Self, AppError> {
        let data_directory = config_directory.join("data");
        let legacy_settings: LegacySettingsDocument = read_versioned_yaml(
            &data_directory.join("settings.yaml"),
            LEGACY_SCHEMA_VERSION,
            "unsupported_settings_schema",
            false,
        )?;
        let mut profiles = BTreeMap::new();
        for path in yaml_files(&data_directory.join("models"), "read_models")? {
            let legacy: LegacyModelConfig = read_versioned_yaml(
                &path,
                LEGACY_SCHEMA_VERSION,
                "unsupported_model_schema",
                false,
            )?;
            let profile = migrate_schema_v1_model(legacy)?;
            validate_profile_filename(&path, &profile)?;
            if profiles
                .insert(profile.profile.id.clone(), profile)
                .is_some()
            {
                return Err(AppError::new("duplicate_profile"));
            }
        }
        let settings = SettingsDocument {
            schema_version: SETTINGS_SCHEMA_VERSION,
            editor_profile: legacy_settings.active_model,
            language: legacy_settings.language,
            devices: BTreeMap::new(),
        };
        validate_settings(&settings, &profiles)?;
        activate_schema_v1_migration(config_directory, &settings, &profiles)?;
        Ok(Self {
            config_directory: config_directory.to_owned(),
            settings,
            profiles,
        })
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
        Self::load_data_directory(config_directory, &data_directory)
    }

    fn load_data_directory(
        config_directory: &Path,
        data_directory: &Path,
    ) -> Result<Self, AppError> {
        let settings_path = data_directory.join("settings.yaml");
        let settings_schema_version = read_schema_header(&settings_path)?.schema_version;
        let mut settings: SettingsDocument = match settings_schema_version {
            SETTINGS_SCHEMA_VERSION => read_versioned_yaml(
                &settings_path,
                SETTINGS_SCHEMA_VERSION,
                "unsupported_settings_schema",
                false,
            )?,
            PREVIOUS_SETTINGS_SCHEMA_VERSION => {
                let contents = read_yaml_contents(&settings_path, false)?;
                let mut migrated: SettingsDocument =
                    serde_yaml_ng::from_str(&contents).map_err(|error| {
                        AppError::new("invalid_yaml").with_detail(error.to_string())
                    })?;
                migrated.schema_version = SETTINGS_SCHEMA_VERSION;
                migrated
            }
            _ => return Err(AppError::new("unsupported_settings_schema")),
        };
        let settings_migrated = canonicalize_settings_board_ids(&mut settings)
            || settings_schema_version == PREVIOUS_SETTINGS_SCHEMA_VERSION;
        let mut profiles = BTreeMap::new();
        let mut migrated_profiles = Vec::new();
        let profile_directory = data_directory.join("profiles");
        for path in yaml_files(&profile_directory, "read_profiles")? {
            let read = read_profile(&path, false, true)?;
            let profile = read.profile;
            profile.validate()?;
            validate_profile_filename(&path, &profile)?;
            if read.migrated {
                migrated_profiles.push((path, profile.profile.id.clone()));
            }
            if profiles
                .insert(profile.profile.id.clone(), profile)
                .is_some()
            {
                return Err(AppError::new("duplicate_profile"));
            }
        }
        validate_settings(&settings, &profiles)?;
        if settings_migrated {
            write_yaml(&settings_path, &settings)?;
        }
        for (path, id) in migrated_profiles {
            write_yaml(&path, &profiles[&id])?;
        }
        Ok(Self {
            config_directory: config_directory.to_owned(),
            settings,
            profiles,
        })
    }

    pub fn save_profile(&mut self, mut profile: DeviceProfile) -> Result<(), AppError> {
        canonicalize_profile_board_ids(&mut profile);
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

    pub fn create_profile(
        &mut self,
        request: CreateDeviceProfileRequest,
    ) -> Result<&DeviceProfile, AppError> {
        let (name, fallback, mut profile) = match request {
            CreateDeviceProfileRequest::Clone {
                name,
                source_profile_id,
            } => {
                let source = self.profiles.get(&source_profile_id).ok_or_else(|| {
                    AppError::new("unknown_profile").with_param("profile", source_profile_id)
                })?;
                (name, source.profile.id.clone(), source.clone())
            }
            CreateDeviceProfileRequest::Blank {
                name,
                board_profile_id,
            } => {
                let board = board_by_id(&board_profile_id).ok_or_else(|| {
                    AppError::new("unknown_board_profile")
                        .with_param("board_profile", &board_profile_id)
                })?;
                let profile =
                    blank_device_profile(String::new(), name.clone(), board.id.to_owned());
                (name, board.id.to_owned(), profile)
            }
        };
        let name = name.trim().to_owned();
        if name.is_empty() {
            return Err(AppError::new("invalid_profile_name"));
        }
        let id = next_profile_id(&self.profiles, &name, &fallback);
        profile.profile.id = id.clone();
        profile.profile.name = name;
        profile.validate()?;

        let path = self.profile_directory().join(format!("{id}.yaml"));
        if path.exists() || self.profiles.contains_key(&id) {
            return Err(AppError::new("profile_already_exists").with_param("profile", id));
        }
        write_yaml(&path, &profile)?;
        let mut settings = self.settings.clone();
        settings.editor_profile = Some(id.clone());
        if let Err(error) = self.persist_settings(&settings) {
            let _ = fs::remove_file(path);
            return Err(error);
        }
        self.settings = settings;
        self.profiles.insert(id.clone(), profile);
        Ok(&self.profiles[&id])
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

    #[cfg(test)]
    pub fn enroll_device(&mut self, id: DeviceId) -> Result<&DeviceRecord, AppError> {
        self.enroll_device_with_registry(compiled_registry(), id)
    }

    pub(crate) fn enroll_device_with_registry(
        &mut self,
        registry: HardwareRegistry<'_>,
        id: DeviceId,
    ) -> Result<&DeviceRecord, AppError> {
        if !self.settings.devices.contains_key(&id) {
            let board = registry.board_by_id(id.board_profile_id()).ok_or_else(|| {
                AppError::new("unknown_board_profile")
                    .with_param("board_profile", id.board_profile_id())
            })?;
            let suffix = device_serial_suffix(&id);
            let record = DeviceRecord {
                device_id: id.clone(),
                name: format!("{} · {suffix}", board.display_name),
                board_profile_id: board.id.into(),
                runtime_assignment: None,
                product_config: None,
            };
            let mut settings = self.settings.clone();
            settings.devices.insert(id.clone(), record);
            self.persist_settings(&settings)?;
            self.settings = settings;
        }
        Ok(&self.settings.devices[&id])
    }

    pub(crate) fn enroll_product_device_with_registry(
        &mut self,
        registry: HardwareRegistry<'_>,
        id: DeviceId,
        definition: &ProductDefinition,
    ) -> Result<&DeviceRecord, AppError> {
        if !crate::hardware::board_profile_ids_match(
            &definition.hardware_profile.board_profile_id,
            id.board_profile_id(),
        ) {
            return Err(AppError::new("product_board_profile_mismatch"));
        }
        self.enroll_device_with_registry(registry, id.clone())?;
        let current_config = self.settings.devices[&id].product_config.as_ref();
        if let Some(config) = current_config {
            if config.product_version_id != definition.product.product_version_id {
                return Err(AppError::new("product_version_id_mismatch"));
            }
            definition
                .as_runtime_profile(config.trigger_settings.clone(), config.actions.clone())
                .validate()?;
        } else {
            let mut settings = self.settings.clone();
            settings
                .devices
                .get_mut(&id)
                .expect("device was enrolled")
                .product_config = Some(ProductDeviceConfig {
                product_version_id: definition.product.product_version_id.clone(),
                trigger_settings: TriggerSettings::default(),
                actions: BTreeMap::new(),
            });
            self.persist_settings(&settings)?;
            self.settings = settings;
        }
        Ok(&self.settings.devices[&id])
    }

    pub fn save_product_device_config(
        &mut self,
        id: &DeviceId,
        definition: &ProductDefinition,
        config: ProductDeviceConfig,
    ) -> Result<(), AppError> {
        if config.product_version_id != definition.product.product_version_id {
            return Err(AppError::new("product_version_id_mismatch"));
        }
        definition
            .as_runtime_profile(config.trigger_settings.clone(), config.actions.clone())
            .validate()?;
        self.update_device(id, |record| record.product_config = Some(config))
    }

    pub fn copy_product_device_config(
        &mut self,
        source_id: &DeviceId,
        target_id: &DeviceId,
        definition: &ProductDefinition,
    ) -> Result<(), AppError> {
        let config = self
            .device(source_id)?
            .product_config
            .clone()
            .ok_or_else(|| AppError::new("source_product_config_missing"))?;
        self.save_product_device_config(target_id, definition, config)
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
        self.validate_assignment(id, &value)?;
        self.update_device(id, |record| record.runtime_assignment = Some(value))
    }

    pub fn duplicate_profile_for_device(
        &mut self,
        request: DuplicateProfileForDeviceRequest,
    ) -> Result<DeviceProfile, AppError> {
        self.duplicate_profile_for_device_internal(request, None)
    }

    pub fn duplicate_profile_for_device_with_metrics(
        &mut self,
        request: DuplicateProfileForDeviceRequest,
        metrics: &MetricsStore,
    ) -> Result<DeviceProfile, AppError> {
        self.duplicate_profile_for_device_internal(request, Some(metrics))
    }

    fn duplicate_profile_for_device_internal(
        &mut self,
        request: DuplicateProfileForDeviceRequest,
        metrics: Option<&MetricsStore>,
    ) -> Result<DeviceProfile, AppError> {
        let name = request.name.trim().to_owned();
        if name.is_empty() {
            return Err(AppError::new("invalid_profile_name"));
        }
        let source_id = request.source_profile.profile.id.clone();
        if !self.profiles.contains_key(&source_id) {
            return Err(AppError::new("unknown_profile").with_param("profile", &source_id));
        }
        let device = self.device(&request.device_id)?;
        let mut profile = request.source_profile;
        profile.profile.name = name.clone();
        profile.profile.id = next_profile_id(&self.profiles, &name, &source_id);

        let current_assignment = device.runtime_assignment.clone();
        let preserved_index = current_assignment.as_ref().and_then(|assignment| {
            (assignment.device_profile_id == source_id).then(|| {
                profile.hardware_profiles.iter().position(|hardware| {
                    hardware.id == assignment.hardware_profile_id
                        && hardware.board_profile_id == device.board_profile_id
                })
            })
        });
        let selected_index = preserved_index
            .flatten()
            .or_else(|| unique_compatible_hardware_index(&profile, &device.board_profile_id))
            .ok_or_else(|| AppError::new("hardware_resolution_required"))?;

        profile.validate()?;
        let old_hardware_id = profile.hardware_profiles[selected_index].id.clone();
        let mut used_hardware_ids = BTreeSet::new();
        let mut selected_hardware_id = None;
        for hardware in &mut profile.hardware_profiles {
            let original_id = hardware.id.clone();
            let cloned_id = next_hardware_id(&used_hardware_ids, &original_id);
            hardware.id = cloned_id.clone();
            used_hardware_ids.insert(cloned_id.clone());
            if original_id == old_hardware_id {
                selected_hardware_id = Some(cloned_id);
            }
        }
        let selected_hardware_id = selected_hardware_id.expect("selected hardware was cloned");
        let selected_hardware = &profile.hardware_profiles[selected_index];
        if selected_hardware.board_profile_id != device.board_profile_id {
            return Err(AppError::new("assignment_board_mismatch")
                .with_param("device_board_profile", &device.board_profile_id)
                .with_param(
                    "hardware_board_profile",
                    &selected_hardware.board_profile_id,
                ));
        }

        profile.validate()?;
        let profile_id = profile.profile.id.clone();
        let profile_path = self.profile_directory().join(format!("{profile_id}.yaml"));
        if profile_path.exists() || self.profiles.contains_key(&profile_id) {
            return Err(AppError::new("profile_already_exists").with_param("profile", profile_id));
        }
        let mut settings = self.settings.clone();
        settings
            .devices
            .get_mut(&request.device_id)
            .expect("device was validated")
            .runtime_assignment = Some(RuntimeAssignment {
            device_profile_id: profile_id.clone(),
            hardware_profile_id: selected_hardware_id,
        });

        let mut staged_profiles = self.profiles.clone();
        staged_profiles.insert(profile_id.clone(), profile.clone());
        validate_settings(&settings, &staged_profiles)?;
        let next_directory = self.stage_data_generation(&settings, &staged_profiles)?;
        self.activate_staged_data_generation(&next_directory, metrics)?;
        self.profiles.insert(profile_id, profile.clone());
        self.settings = settings;
        Ok(profile)
    }

    fn validate_assignment(
        &self,
        id: &DeviceId,
        value: &RuntimeAssignment,
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
        Ok(())
    }

    pub fn complete_device_setup(
        &mut self,
        id: &DeviceId,
        name: String,
        assignment: RuntimeAssignment,
    ) -> Result<(), AppError> {
        let name = name.trim().to_owned();
        if name.is_empty() {
            return Err(AppError::new("invalid_device_name"));
        }
        self.validate_assignment(id, &assignment)?;
        let mut settings = self.settings.clone();
        let record = settings.devices.get_mut(id).expect("device was validated");
        record.name = name;
        record.runtime_assignment = Some(assignment);
        self.persist_settings(&settings)?;
        self.settings = settings;
        Ok(())
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
        match read_compatible_backup(path)? {
            CompatibleBackup::ProductDevices(backup) => Ok(BackupPreview {
                kind: BackupKind::ProductDevices,
                profile_count: 0,
                button_count: 0,
                hardware_binding_count: 0,
                action_count: backup
                    .devices
                    .iter()
                    .flat_map(|device| device.actions.values())
                    .map(TriggerActions::action_count)
                    .sum(),
                device_count: backup.devices.len(),
                assignment_count: 0,
                metric_row_count: 0,
                activity_count: 0,
            }),
            CompatibleBackup::Full(snapshot) => Ok(BackupPreview {
                kind: BackupKind::Full,
                profile_count: snapshot.profiles.len(),
                button_count: snapshot.profiles.values().map(button_count).sum(),
                hardware_binding_count: snapshot
                    .profiles
                    .values()
                    .map(hardware_binding_count)
                    .sum(),
                action_count: snapshot.profiles.values().map(action_count).sum(),
                device_count: snapshot.settings.devices.len(),
                assignment_count: snapshot
                    .settings
                    .devices
                    .values()
                    .filter(|device| device.runtime_assignment.is_some())
                    .count(),
                metric_row_count: snapshot.metrics.button_metrics.len()
                    + snapshot.metrics.button_metric_days.len(),
                activity_count: snapshot.metrics.activity_logs.len(),
            }),
        }
    }

    pub fn export_user_backup(&self, path: &Path) -> Result<(), AppError> {
        let devices = self
            .settings
            .devices
            .values()
            .filter_map(|device| {
                device
                    .product_config
                    .as_ref()
                    .map(|config| UserBackupDevice {
                        device_id: device.device_id.clone(),
                        product_version_id: config.product_version_id.clone(),
                        trigger_settings: config.trigger_settings.clone(),
                        actions: config.actions.clone(),
                    })
            })
            .collect();
        write_yaml(
            path,
            &UserBackupDocument {
                schema_version: USER_BACKUP_SCHEMA_VERSION,
                kind: BackupKind::ProductDevices,
                devices,
            },
        )
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
        self.restore_backup_with_operations(path, metrics, &mut SystemRestoreOperations)
    }

    pub fn restore_compatible_backup(
        &mut self,
        path: &Path,
        metrics: Option<&MetricsStore>,
    ) -> Result<(), AppError> {
        match read_compatible_backup(path)? {
            CompatibleBackup::ProductDevices(backup) => self.restore_user_backup(backup),
            CompatibleBackup::Full(_) => self.restore_backup(
                path,
                metrics.ok_or_else(|| AppError::new("metrics_unavailable"))?,
            ),
        }
    }

    fn restore_user_backup(&mut self, backup: UserBackupDocument) -> Result<(), AppError> {
        let mut settings = self.settings.clone();
        for backup_device in backup.devices {
            let config = ProductDeviceConfig {
                product_version_id: backup_device.product_version_id,
                trigger_settings: backup_device.trigger_settings,
                actions: backup_device.actions,
            };
            if let Some(device) = settings.devices.get_mut(&backup_device.device_id) {
                if device
                    .product_config
                    .as_ref()
                    .is_some_and(|current| current.product_version_id != config.product_version_id)
                {
                    continue;
                }
                device.product_config = Some(config);
                continue;
            }

            let board = board_by_id(backup_device.device_id.board_profile_id())
                .ok_or_else(|| AppError::new("unknown_board_profile"))?;
            let suffix = device_serial_suffix(&backup_device.device_id);
            settings.devices.insert(
                backup_device.device_id.clone(),
                DeviceRecord {
                    device_id: backup_device.device_id,
                    name: format!("{} · {suffix}", board.display_name),
                    board_profile_id: board.id.into(),
                    runtime_assignment: None,
                    product_config: Some(config),
                },
            );
        }
        validate_settings(&settings, &self.profiles)?;
        self.persist_settings(&settings)?;
        self.settings = settings;
        Ok(())
    }

    fn restore_backup_with_operations(
        &mut self,
        path: &Path,
        metrics: &MetricsStore,
        operations: &mut impl RestoreOperations,
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
        if let Err(error) = operations.rename(&data_directory, &previous_directory) {
            let primary = io_error("stage_restore", &data_directory, error);
            let recovery = recover_original_generation(
                RestoreGenerationState::OriginalActive,
                &data_directory,
                &next_directory,
                &previous_directory,
                metrics,
                operations,
            );
            return Err(restore_error(primary, recovery));
        }
        if let Err(error) = operations.rename(&next_directory, &data_directory) {
            let primary = io_error("activate_restore", &next_directory, error);
            let recovery = recover_original_generation(
                RestoreGenerationState::OriginalPrevious,
                &data_directory,
                &next_directory,
                &previous_directory,
                metrics,
                operations,
            );
            return Err(restore_error(primary, recovery));
        }

        let active_metrics_path = data_directory.join("metrics.sqlite3");
        if let Err(error) = operations.reopen_metrics(metrics, &active_metrics_path) {
            let primary = metrics_error("reopen_metrics", error);
            let recovery = recover_original_generation(
                RestoreGenerationState::RestoredActive,
                &data_directory,
                &next_directory,
                &previous_directory,
                metrics,
                operations,
            );
            return Err(restore_error(primary, recovery));
        }
        let restored = match Self::load_existing(&self.config_directory) {
            Ok(restored) => restored,
            Err(error) => {
                let recovery = recover_original_generation(
                    RestoreGenerationState::RestoredActive,
                    &data_directory,
                    &next_directory,
                    &previous_directory,
                    metrics,
                    operations,
                );
                return Err(restore_error(error, recovery));
            }
        };
        self.settings = restored.settings;
        self.profiles = restored.profiles;
        let _ = operations.remove_dir_all(&previous_directory);
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

    fn stage_data_generation(
        &self,
        settings: &SettingsDocument,
        profiles: &BTreeMap<String, DeviceProfile>,
    ) -> Result<PathBuf, AppError> {
        Self::recover_interrupted_data_generation(&self.config_directory)?;
        let next_directory = self.config_directory.join("data.next");
        remove_directory_if_exists(&next_directory)?;
        if let Err(error) = write_data_directory(&next_directory, settings, profiles) {
            let _ = fs::remove_dir_all(&next_directory);
            return Err(error);
        }

        let source_metrics = self.data_directory().join("metrics.sqlite3");
        if source_metrics.exists() {
            let staged_metrics = next_directory.join("metrics.sqlite3");
            if let Err(error) = fs::hard_link(&source_metrics, &staged_metrics) {
                let _ = fs::remove_dir_all(&next_directory);
                return Err(io_error("stage_metrics", &staged_metrics, error));
            }
        }
        Ok(next_directory)
    }

    fn activate_staged_data_generation(
        &self,
        next_directory: &Path,
        metrics: Option<&MetricsStore>,
    ) -> Result<(), AppError> {
        let mut operations = SystemDataGenerationOperations;
        self.activate_staged_data_generation_with_operations(
            next_directory,
            metrics,
            &mut operations,
        )
    }

    fn activate_staged_data_generation_with_operations(
        &self,
        next_directory: &Path,
        metrics: Option<&MetricsStore>,
        operations: &mut impl DataGenerationOperations,
    ) -> Result<(), AppError> {
        let data_directory = self.data_directory();
        let previous_directory = self.config_directory.join("data.previous");
        match operations.remove_dir_all(&previous_directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(io_error(
                    "remove_staging_directory",
                    &previous_directory,
                    error,
                ));
            }
        }
        if let Some(metrics) = metrics {
            operations.close_metrics(metrics);
        }
        if let Err(error) = operations.rename(&data_directory, &previous_directory) {
            let reopen = metrics.and_then(|metrics| {
                operations
                    .reopen_metrics(metrics, &data_directory.join("metrics.sqlite3"))
                    .err()
            });
            let _ = operations.remove_dir_all(next_directory);
            return match reopen {
                Some(reopen) => Err(AppError::new("duplicate_profile_rollback_failed")
                    .with_detail(format!(
                        "stage duplicate data: {error}; reopen original metrics: {reopen}"
                    ))),
                None => Err(io_error("stage_duplicate_data", &data_directory, error)),
            };
        }
        if let Err(error) = operations.rename(next_directory, &data_directory) {
            let rollback = operations.rename(&previous_directory, &data_directory);
            let reopen = metrics.and_then(|metrics| {
                operations
                    .reopen_metrics(metrics, &data_directory.join("metrics.sqlite3"))
                    .err()
            });
            let _ = operations.remove_dir_all(next_directory);
            return match (rollback, reopen) {
                (Ok(()), None) => Err(io_error("activate_duplicate_data", next_directory, error)),
                (rollback, reopen) => Err(AppError::new("duplicate_profile_rollback_failed")
                    .with_detail(format!(
                        "activate duplicate data: {error}; restore previous data: {rollback:?}; reopen original metrics: {reopen:?}"
                    ))),
            };
        }
        if let Some(metrics) = metrics
            && let Err(error) =
                operations.reopen_metrics(metrics, &data_directory.join("metrics.sqlite3"))
        {
            let rollback_new = operations.rename(&data_directory, next_directory);
            let rollback_old = operations.rename(&previous_directory, &data_directory);
            let reopen = operations
                .reopen_metrics(metrics, &data_directory.join("metrics.sqlite3"))
                .err();
            let _ = operations.remove_dir_all(next_directory);
            if rollback_new.is_err() || rollback_old.is_err() || reopen.is_some() {
                return Err(AppError::new("duplicate_profile_rollback_failed").with_detail(
                    format!(
                        "reopen active metrics: {error}; restore new data: {rollback_new:?}; restore original data: {rollback_old:?}; reopen original metrics: {reopen:?}"
                    ),
                ));
            }
            return Err(metrics_error("reopen_metrics", error));
        }
        let _ = operations.remove_dir_all(&previous_directory);
        Ok(())
    }

    fn data_directory(&self) -> PathBuf {
        self.config_directory.join("data")
    }

    fn profile_directory(&self) -> PathBuf {
        self.data_directory().join("profiles")
    }
}

fn ascii_slug(value: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() {
            if separator && !slug.is_empty() {
                slug.push('-');
            }
            slug.push((byte as char).to_ascii_lowercase());
            separator = false;
        } else if !slug.is_empty() {
            separator = true;
        }
    }
    slug
}

fn next_profile_id(
    profiles: &BTreeMap<String, DeviceProfile>,
    name: &str,
    fallback: &str,
) -> String {
    let base = [ascii_slug(name), ascii_slug(fallback), "profile".into()]
        .into_iter()
        .find(|value| !value.is_empty())
        .unwrap();
    if !profiles.contains_key(&base) {
        return base;
    }
    (2..)
        .map(|suffix| format!("{base}-{suffix}"))
        .find(|candidate| !profiles.contains_key(candidate))
        .expect("profile ID suffix exhausted")
}

fn next_hardware_id(used: &BTreeSet<String>, original: &str) -> String {
    let base = format!("{}-copy", ascii_slug(original));
    let base = if base == "-copy" {
        "hardware-copy".to_owned()
    } else {
        base
    };
    if !used.contains(&base) {
        return base;
    }
    (2..)
        .map(|suffix| format!("{base}-{suffix}"))
        .find(|candidate| !used.contains(candidate))
        .expect("hardware ID suffix exhausted")
}

fn unique_compatible_hardware_index(
    profile: &DeviceProfile,
    board_profile_id: &str,
) -> Option<usize> {
    let candidates = profile
        .hardware_profiles
        .iter()
        .enumerate()
        .filter(|(_, hardware)| hardware.board_profile_id == board_profile_id)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    (candidates.len() == 1).then_some(candidates[0])
}

fn collect_profiles(
    values: Vec<DeviceProfile>,
) -> Result<BTreeMap<String, DeviceProfile>, AppError> {
    let mut profiles = BTreeMap::new();
    for mut profile in values {
        canonicalize_profile_board_ids(&mut profile);
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
        let profile = read_profile(&path, true, false)?.profile;
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
    validate_settings_with_registry(compiled_registry(), settings, profiles)
}

fn validate_settings_with_registry(
    registry: HardwareRegistry<'_>,
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
        let board = registry
            .board_by_id(&device.board_profile_id)
            .ok_or_else(|| {
                AppError::new("unknown_board_profile")
                    .with_param("board_profile", &device.board_profile_id)
            })?;
        if id.board_profile_id() != board.id {
            return Err(AppError::new("device_board_mismatch").with_param("device_id", id.as_str()));
        }
        if device.name.trim().is_empty() {
            return Err(AppError::new("invalid_device_name").with_param("device_id", id.as_str()));
        }
        if let Some(config) = &device.product_config
            && !crate::product::valid_product_version_id(&config.product_version_id)
        {
            return Err(
                AppError::new("invalid_product_version_id").with_param("device_id", id.as_str())
            );
        }
    }
    Ok(())
}

fn device_serial_suffix(id: &DeviceId) -> String {
    id.hardware_serial()
        .chars()
        .rev()
        .take(6)
        .collect::<String>()
        .chars()
        .rev()
        .collect()
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
    let mut backup: BackupDocument = read_versioned_yaml(
        path,
        BACKUP_SCHEMA_VERSION,
        "unsupported_backup_schema",
        false,
    )?;
    if backup.settings.schema_version == PREVIOUS_SETTINGS_SCHEMA_VERSION {
        backup.settings.schema_version = SETTINGS_SCHEMA_VERSION;
    }
    canonicalize_settings_board_ids(&mut backup.settings);
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

enum CompatibleBackup {
    ProductDevices(UserBackupDocument),
    Full(WorkspaceSnapshot),
}

#[derive(Deserialize)]
struct BackupHeader {
    schema_version: u16,
    #[serde(default)]
    kind: Option<BackupKind>,
}

fn read_compatible_backup(path: &Path) -> Result<CompatibleBackup, AppError> {
    let contents = read_yaml_contents(path, false)?;
    let header: BackupHeader = serde_yaml_ng::from_str(&contents)
        .map_err(|error| AppError::new("invalid_yaml").with_detail(error.to_string()))?;
    if header.kind == Some(BackupKind::ProductDevices) {
        if header.schema_version != USER_BACKUP_SCHEMA_VERSION {
            return Err(AppError::new("unsupported_backup_schema"));
        }
        let backup: UserBackupDocument = serde_yaml_ng::from_str(&contents)
            .map_err(|error| AppError::new("invalid_yaml").with_detail(error.to_string()))?;
        validate_user_backup(&backup)?;
        Ok(CompatibleBackup::ProductDevices(backup))
    } else {
        read_backup(path).map(CompatibleBackup::Full)
    }
}

fn validate_user_backup(backup: &UserBackupDocument) -> Result<(), AppError> {
    if backup.schema_version != USER_BACKUP_SCHEMA_VERSION
        || backup.kind != BackupKind::ProductDevices
    {
        return Err(AppError::new("unsupported_backup_schema"));
    }
    let mut ids = BTreeSet::new();
    for device in &backup.devices {
        if !ids.insert(&device.device_id) {
            return Err(AppError::new("duplicate_backup_device")
                .with_param("device_id", device.device_id.as_str()));
        }
        if !crate::product::valid_product_version_id(&device.product_version_id) {
            return Err(AppError::new("invalid_product_version_id")
                .with_param("device_id", device.device_id.as_str()));
        }
    }
    Ok(())
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
            InputSource::FeatureSwitch { buttons, .. } => buttons.len(),
        })
        .sum()
}

fn action_count(profile: &DeviceProfile) -> usize {
    profile
        .actions
        .values()
        .map(TriggerActions::action_count)
        .sum()
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

#[derive(Clone, Copy)]
enum RestoreGenerationState {
    OriginalActive,
    OriginalPrevious,
    RestoredActive,
}

struct RestoreRecovery {
    errors: Vec<String>,
    fatal_state: Option<&'static str>,
}

fn recover_original_generation(
    state: RestoreGenerationState,
    data_directory: &Path,
    next_directory: &Path,
    previous_directory: &Path,
    metrics: &MetricsStore,
    operations: &mut impl RestoreOperations,
) -> RestoreRecovery {
    let mut errors = Vec::new();
    metrics.close();

    if matches!(state, RestoreGenerationState::RestoredActive)
        && !retry_rename(
            operations,
            data_directory,
            next_directory,
            "quarantine_restored_data",
            &mut errors,
        )
        && !retry_remove(
            operations,
            data_directory,
            "remove_restored_data",
            &mut errors,
        )
    {
        return RestoreRecovery {
            errors,
            fatal_state: Some("restored_active_original_previous_metrics_closed"),
        };
    }

    if matches!(
        state,
        RestoreGenerationState::OriginalPrevious | RestoreGenerationState::RestoredActive
    ) && !retry_rename(
        operations,
        previous_directory,
        data_directory,
        "restore_previous_data",
        &mut errors,
    ) {
        return RestoreRecovery {
            errors,
            fatal_state: Some("original_previous_data_missing_metrics_closed"),
        };
    }

    if !retry_reopen_metrics(
        operations,
        metrics,
        &data_directory.join("metrics.sqlite3"),
        &mut errors,
    ) {
        let _ = retry_remove(
            operations,
            next_directory,
            "cleanup_failed_generation",
            &mut errors,
        );
        return RestoreRecovery {
            errors,
            fatal_state: Some("original_active_metrics_closed"),
        };
    }

    let _ = retry_remove(
        operations,
        next_directory,
        "cleanup_failed_generation",
        &mut errors,
    );
    let _ = retry_remove(
        operations,
        previous_directory,
        "cleanup_previous_generation",
        &mut errors,
    );
    RestoreRecovery {
        errors,
        fatal_state: None,
    }
}

fn retry_rename(
    operations: &mut impl RestoreOperations,
    from: &Path,
    to: &Path,
    label: &str,
    errors: &mut Vec<String>,
) -> bool {
    for _ in 0..2 {
        match operations.rename(from, to) {
            Ok(()) => return true,
            Err(error) => errors.push(format!("{label}: {error}")),
        }
    }
    false
}

fn retry_remove(
    operations: &mut impl RestoreOperations,
    path: &Path,
    label: &str,
    errors: &mut Vec<String>,
) -> bool {
    for _ in 0..2 {
        match operations.remove_dir_all(path) {
            Ok(()) => return true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return true,
            Err(error) => errors.push(format!("{label}: {error}")),
        }
    }
    false
}

fn retry_reopen_metrics(
    operations: &mut impl RestoreOperations,
    metrics: &MetricsStore,
    path: &Path,
    errors: &mut Vec<String>,
) -> bool {
    for _ in 0..2 {
        match operations.reopen_metrics(metrics, path) {
            Ok(()) => return true,
            Err(error) => errors.push(format!("reopen_original_metrics: {error}")),
        }
    }
    false
}

fn restore_error(mut primary: AppError, recovery: RestoreRecovery) -> AppError {
    if let Some(state) = recovery.fatal_state {
        let primary_code = primary.code.clone();
        let primary_detail = primary.to_string();
        return AppError::new("restore_rollback_failed")
            .with_param("primary_code", primary_code)
            .with_param("state", state)
            .with_param("rollback_errors", recovery.errors.join(" | "))
            .with_detail(primary_detail);
    }
    if !recovery.errors.is_empty() {
        primary
            .params
            .insert("rollback_errors".into(), recovery.errors.join(" | "));
    }
    primary
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

fn read_schema_header(path: &Path) -> Result<SchemaHeader, AppError> {
    let contents = fs::read_to_string(path).map_err(|error| io_error("read_file", path, error))?;
    serde_yaml_ng::from_str(&contents)
        .map_err(|error| AppError::new("invalid_yaml").with_detail(error.to_string()))
}

fn migrate_schema_v1_model(legacy: LegacyModelConfig) -> Result<DeviceProfile, AppError> {
    if legacy.schema_version != LEGACY_SCHEMA_VERSION {
        return Err(AppError::new("unsupported_model_schema"));
    }
    let board_profile_id = match legacy.hardware.controller.as_str() {
        "esp32s3" => crate::hardware::YD_ESP32_S3_BOARD_ID,
        controller => {
            return Err(
                AppError::new("unsupported_legacy_controller").with_param("controller", controller)
            );
        }
    };
    let board = board_by_id(board_profile_id).ok_or_else(|| {
        AppError::new("unknown_board_profile").with_param("board_profile", board_profile_id)
    })?;
    let profile = DeviceProfile {
        schema_version: PROFILE_SCHEMA_VERSION,
        profile: legacy.model,
        trigger_settings: TriggerSettings::default(),
        hardware_profiles: vec![HardwareProfile {
            id: board.id.into(),
            name: board.display_name.into(),
            board_profile_id: board.id.into(),
            debounce_ms: legacy.hardware.debounce_ms,
            ssd1306: None,
            inputs: legacy.hardware.inputs,
        }],
        actions: legacy
            .actions
            .into_iter()
            .map(|(button, actions)| (button, TriggerActions::press(actions)))
            .collect(),
    };
    profile.validate()?;
    Ok(profile)
}

fn activate_schema_v1_migration(
    config_directory: &Path,
    settings: &SettingsDocument,
    profiles: &BTreeMap<String, DeviceProfile>,
) -> Result<(), AppError> {
    let data_directory = config_directory.join("data");
    let next_directory = config_directory.join("data.next");
    let backup_directory = config_directory.join("data.v1.backup");
    if backup_directory.exists() {
        return Err(io_error(
            "migration_backup_exists",
            &backup_directory,
            std::io::Error::new(std::io::ErrorKind::AlreadyExists, "backup already exists"),
        ));
    }
    remove_directory_if_exists(&next_directory)?;
    write_data_directory(&next_directory, settings, profiles)?;
    if let Err(error) = fs::rename(&data_directory, &backup_directory) {
        let _ = fs::remove_dir_all(&next_directory);
        return Err(io_error("backup_schema_v1_data", &data_directory, error));
    }
    if let Err(error) = fs::rename(&next_directory, &data_directory) {
        let primary = io_error("activate_migrated_data", &data_directory, error);
        return match fs::rename(&backup_directory, &data_directory) {
            Ok(()) => Err(primary),
            Err(rollback) => Err(AppError::new("migration_rollback_failed")
                .with_param("backup", backup_directory.display().to_string())
                .with_detail(format!("{primary}; rollback: {rollback}"))),
        };
    }
    Ok(())
}

fn read_profile_limited(path: &Path) -> Result<DeviceProfile, AppError> {
    read_profile(path, true, true).map(|read| read.profile)
}

fn read_profile(
    path: &Path,
    limited: bool,
    allow_schema_v2: bool,
) -> Result<ReadProfile, AppError> {
    let contents = read_yaml_contents(path, limited)?;
    let header: SchemaHeader = serde_yaml_ng::from_str(&contents)
        .map_err(|error| AppError::new("invalid_yaml").with_detail(error.to_string()))?;
    let mut read = match header.schema_version {
        PROFILE_SCHEMA_VERSION => serde_yaml_ng::from_str(&contents)
            .map(|profile| ReadProfile {
                profile,
                migrated: false,
            })
            .map_err(|error| AppError::new("invalid_yaml").with_detail(error.to_string())),
        PREVIOUS_PROFILE_SCHEMA_VERSION if allow_schema_v2 => {
            let profile: SchemaV2DeviceProfile = serde_yaml_ng::from_str(&contents)
                .map_err(|error| AppError::new("invalid_yaml").with_detail(error.to_string()))?;
            Ok(ReadProfile {
                profile: migrate_schema_v2_profile(profile),
                migrated: true,
            })
        }
        _ => Err(AppError::new("unsupported_profile_schema")),
    }?;
    read.migrated |= canonicalize_profile_board_ids(&mut read.profile);
    Ok(read)
}

fn canonicalize_settings_board_ids(settings: &mut SettingsDocument) -> bool {
    let mut changed = false;
    for device in settings.devices.values_mut() {
        let canonical = canonical_board_profile_id(&device.board_profile_id).to_owned();
        if canonical != device.board_profile_id {
            device.board_profile_id = canonical;
            changed = true;
        }
    }
    changed
}

fn canonicalize_profile_board_ids(profile: &mut DeviceProfile) -> bool {
    let mut changed = false;
    for hardware in &mut profile.hardware_profiles {
        let canonical = canonical_board_profile_id(&hardware.board_profile_id).to_owned();
        if canonical != hardware.board_profile_id {
            hardware.board_profile_id = canonical;
            changed = true;
        }
    }
    changed
}

fn migrate_schema_v2_profile(legacy: SchemaV2DeviceProfile) -> DeviceProfile {
    debug_assert_eq!(legacy.schema_version, PREVIOUS_PROFILE_SCHEMA_VERSION);
    DeviceProfile {
        schema_version: PROFILE_SCHEMA_VERSION,
        profile: legacy.profile,
        trigger_settings: TriggerSettings::default(),
        hardware_profiles: legacy.hardware_profiles,
        actions: legacy
            .actions
            .into_iter()
            .map(|(button, actions)| (button, TriggerActions::press(actions)))
            .collect(),
    }
}

fn read_versioned_yaml<T: for<'de> Deserialize<'de>>(
    path: &Path,
    expected: u16,
    unsupported_code: &str,
    limited: bool,
) -> Result<T, AppError> {
    let contents = read_yaml_contents(path, limited)?;
    let header: SchemaHeader = serde_yaml_ng::from_str(&contents)
        .map_err(|error| AppError::new("invalid_yaml").with_detail(error.to_string()))?;
    if header.schema_version != expected {
        return Err(AppError::new(unsupported_code));
    }
    serde_yaml_ng::from_str(&contents)
        .map_err(|error| AppError::new("invalid_yaml").with_detail(error.to_string()))
}

fn read_yaml_contents(path: &Path, limited: bool) -> Result<String, AppError> {
    if limited {
        let metadata = fs::metadata(path).map_err(|error| io_error("read_file", path, error))?;
        if metadata.len() > MAX_IMPORT_BYTES {
            return Err(
                AppError::new("file_too_large").with_param("limit", MAX_IMPORT_BYTES.to_string())
            );
        }
    }
    fs::read_to_string(path).map_err(|error| io_error("read_file", path, error))
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
        product::{PRODUCT_DEFINITION_SCHEMA_VERSION, ProductDefinition, ProductIdentity},
        profile::ButtonAction,
    };
    use std::{
        collections::VecDeque,
        fs::OpenOptions,
        io::Write,
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
            ssd1306: None,
            inputs: Vec::new(),
        }
    }

    fn device_profile() -> DeviceProfile {
        DeviceProfile {
            schema_version: PROFILE_SCHEMA_VERSION,
            profile: layout(),
            trigger_settings: TriggerSettings::default(),
            hardware_profiles: vec![
                hardware("esp-primary", crate::hardware::YD_ESP32_S3_BOARD_ID),
                hardware("esp-alternate", crate::hardware::YD_ESP32_S3_BOARD_ID),
                hardware("rp-primary", crate::hardware::YD_RP2040_BOARD_ID),
            ],
            actions: BTreeMap::from([(
                "A".into(),
                TriggerActions::press(vec![
                    ButtonAction::Paste {
                        text: "你好\n".into(),
                    },
                    ButtonAction::Hotkey {
                        keys: vec!["enter".into()],
                    },
                ]),
            )]),
        }
    }

    fn workspace(directory: &TestDirectory) -> Workspace {
        Workspace::create(&directory.0, vec![device_profile()]).unwrap()
    }

    fn product_definition(family_id: &str) -> ProductDefinition {
        ProductDefinition {
            schema_version: PRODUCT_DEFINITION_SCHEMA_VERSION,
            product: ProductIdentity {
                display_name: "Kivo Key 3".into(),
                family_id: family_id.into(),
                variant_id: format!("{family_id}-rp-k3"),
                hardware_revision: 1,
                product_version_id: format!("{family_id}-rp-k3-r01"),
                capabilities: Vec::new(),
            },
            layout: layout(),
            hardware_profile: HardwareProfile {
                id: "hardware".into(),
                name: "Hardware".into(),
                board_profile_id: crate::hardware::YD_RP2040_BOARD_ID.into(),
                debounce_ms: 30,
                ssd1306: None,
                inputs: vec![InputSource::Direct {
                    id: "direct".into(),
                    keys: BTreeMap::from([("A".into(), 0), ("B".into(), 1), ("C".into(), 2)]),
                }],
            },
        }
    }

    struct MetricsAwareGenerationOperations {
        metrics_closed: bool,
        reopened_paths: Vec<PathBuf>,
    }

    impl MetricsAwareGenerationOperations {
        fn new() -> Self {
            Self {
                metrics_closed: false,
                reopened_paths: Vec::new(),
            }
        }
    }

    impl DataGenerationOperations for MetricsAwareGenerationOperations {
        fn rename(&mut self, from: &Path, to: &Path) -> std::io::Result<()> {
            if from.file_name().and_then(|name| name.to_str()) == Some("data")
                && !self.metrics_closed
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "open metrics blocks data generation rename",
                ));
            }
            fs::rename(from, to)
        }

        fn remove_dir_all(&mut self, path: &Path) -> std::io::Result<()> {
            fs::remove_dir_all(path)
        }

        fn close_metrics(&mut self, metrics: &MetricsStore) {
            metrics.close();
            self.metrics_closed = true;
        }

        fn reopen_metrics(
            &mut self,
            metrics: &MetricsStore,
            path: &Path,
        ) -> Result<(), rusqlite::Error> {
            let result = metrics.reopen(path);
            if result.is_ok() {
                self.reopened_paths.push(path.to_owned());
            }
            result
        }
    }

    #[test]
    fn duplicate_generation_closes_metrics_before_swapping_and_reopens_active_path() {
        let directory = TestDirectory::new();
        let workspace = workspace(&directory);
        let metrics_path = directory.path("data/metrics.sqlite3");
        let metrics = MetricsStore::open(&metrics_path).unwrap();
        let next_directory = workspace
            .stage_data_generation(&workspace.settings, &workspace.profiles)
            .unwrap();
        let mut operations = MetricsAwareGenerationOperations::new();

        workspace
            .activate_staged_data_generation_with_operations(
                &next_directory,
                Some(&metrics),
                &mut operations,
            )
            .unwrap();

        assert!(operations.metrics_closed);
        assert_eq!(
            operations.reopened_paths,
            vec![directory.path("data/metrics.sqlite3")]
        );
        metrics
            .record_button_press(
                &MetricAttribution {
                    device_id: DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "GENERATION")
                        .unwrap(),
                    device_name: "Generation test".into(),
                    device_profile_id: "red-phone-v1".into(),
                    hardware_profile_id: "esp-primary".into(),
                },
                "A",
                1_720_086_400_000,
            )
            .unwrap();
    }

    #[test]
    fn creates_a_deep_cloned_profile_with_a_unique_stable_id() {
        let directory = TestDirectory::new();
        let mut workspace = workspace(&directory);
        let original = workspace.profiles["red-phone-v1"].clone();

        let created = workspace
            .create_profile(CreateDeviceProfileRequest::Clone {
                name: "Red Phone".into(),
                source_profile_id: "red-phone-v1".into(),
            })
            .unwrap()
            .clone();

        assert_eq!(created.profile.id, "red-phone");
        assert_eq!(created.profile.name, "Red Phone");
        assert_eq!(created.profile.groups, original.profile.groups);
        assert_eq!(created.actions, original.actions);
        assert_eq!(created.hardware_profiles, original.hardware_profiles);
        assert_eq!(
            workspace.settings.editor_profile.as_deref(),
            Some("red-phone")
        );
        assert_eq!(workspace.profiles["red-phone-v1"], original);

        let second = workspace
            .create_profile(CreateDeviceProfileRequest::Clone {
                name: "Red Phone".into(),
                source_profile_id: "red-phone-v1".into(),
            })
            .unwrap();
        assert_eq!(second.profile.id, "red-phone-2");
    }

    #[test]
    fn creates_a_valid_blank_profile_for_the_exact_board() {
        let directory = TestDirectory::new();
        let mut workspace = workspace(&directory);

        let created = workspace
            .create_profile(CreateDeviceProfileRequest::Blank {
                name: "新键盘".into(),
                board_profile_id: crate::hardware::YD_RP2040_BOARD_ID.into(),
            })
            .unwrap();

        created.validate().unwrap();
        assert_eq!(created.profile.id, "yd-rp2040");
        assert_eq!(created.profile.name, "新键盘");
        assert!(created.profile.groups.is_empty());
        assert!(created.actions.is_empty());
        assert_eq!(created.hardware_profiles.len(), 1);
        assert_eq!(created.hardware_profiles[0].id, "hardware");
        assert_eq!(
            created.hardware_profiles[0].board_profile_id,
            crate::hardware::YD_RP2040_BOARD_ID
        );
        assert!(created.hardware_profiles[0].inputs.is_empty());
    }

    #[test]
    fn profile_creation_rejects_bad_sources_and_never_overwrites() {
        let directory = TestDirectory::new();
        let mut workspace = workspace(&directory);
        let before = workspace.profiles.clone();

        assert_eq!(
            workspace
                .create_profile(CreateDeviceProfileRequest::Clone {
                    name: "Copy".into(),
                    source_profile_id: "missing".into(),
                })
                .unwrap_err()
                .code,
            "unknown_profile"
        );
        assert_eq!(
            workspace
                .create_profile(CreateDeviceProfileRequest::Blank {
                    name: " ".into(),
                    board_profile_id: crate::hardware::YD_RP2040_BOARD_ID.into(),
                })
                .unwrap_err()
                .code,
            "invalid_profile_name"
        );
        assert_eq!(workspace.profiles, before);
    }

    #[test]
    fn complete_device_setup_persists_name_and_assignment_together() {
        let directory = TestDirectory::new();
        let mut workspace = workspace(&directory);
        let id = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "SETUP-A").unwrap();
        workspace.enroll_device(id.clone()).unwrap();
        let assignment = RuntimeAssignment {
            device_profile_id: "red-phone-v1".into(),
            hardware_profile_id: "esp-primary".into(),
        };

        workspace
            .complete_device_setup(&id, "Front desk".into(), assignment.clone())
            .unwrap();

        let record = &workspace.settings.devices[&id];
        assert_eq!(record.name, "Front desk");
        assert_eq!(record.runtime_assignment, Some(assignment));
        let reloaded = Workspace::load_existing(&directory.0).unwrap();
        assert_eq!(reloaded.settings.devices[&id], *record);
    }

    #[test]
    fn complete_device_setup_rolls_back_both_fields_when_assignment_is_invalid() {
        let directory = TestDirectory::new();
        let mut workspace = workspace(&directory);
        let id = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "SETUP-B").unwrap();
        workspace.enroll_device(id.clone()).unwrap();
        let before = workspace.settings.clone();
        let disk_before = fs::read(directory.path("data/settings.yaml")).unwrap();

        let error = workspace
            .complete_device_setup(
                &id,
                "Partially written".into(),
                RuntimeAssignment {
                    device_profile_id: "red-phone-v1".into(),
                    hardware_profile_id: "missing".into(),
                },
            )
            .unwrap_err();

        assert_eq!(error.code, "unknown_hardware_profile");
        assert_eq!(workspace.settings, before);
        assert_eq!(
            fs::read(directory.path("data/settings.yaml")).unwrap(),
            disk_before
        );
    }

    #[test]
    fn duplicate_and_assign_is_atomic_and_generates_unique_ids() {
        let directory = TestDirectory::new();
        let mut workspace = workspace(&directory);
        let device_a = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "SHARED-A").unwrap();
        let device_b = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "SHARED-B").unwrap();
        workspace.enroll_device(device_a.clone()).unwrap();
        workspace.enroll_device(device_b.clone()).unwrap();
        let assignment = RuntimeAssignment {
            device_profile_id: "red-phone-v1".into(),
            hardware_profile_id: "esp-primary".into(),
        };
        workspace
            .set_assignment(&device_a, assignment.clone())
            .unwrap();
        workspace
            .set_assignment(&device_b, assignment.clone())
            .unwrap();

        let mut edited = workspace.profiles["red-phone-v1"].clone();
        edited.trigger_settings.long_press_ms = 700;
        let cloned = workspace
            .duplicate_profile_for_device(DuplicateProfileForDeviceRequest {
                device_id: device_a.clone(),
                source_profile: edited,
                name: "Phone copy".into(),
            })
            .unwrap();

        assert_ne!(cloned.profile.id, "red-phone-v1");
        assert_ne!(cloned.hardware_profiles[0].id, "esp-primary");
        assert_eq!(cloned.trigger_settings.long_press_ms, 700);
        assert_eq!(
            workspace.settings.devices[&device_b]
                .runtime_assignment
                .as_ref()
                .unwrap()
                .device_profile_id,
            "red-phone-v1"
        );
        assert_eq!(
            workspace.settings.devices[&device_a]
                .runtime_assignment
                .as_ref()
                .unwrap()
                .device_profile_id,
            cloned.profile.id
        );
        assert_eq!(
            workspace.profiles["red-phone-v1"]
                .trigger_settings
                .long_press_ms,
            500
        );
    }

    #[test]
    fn duplicate_generation_keeps_the_open_metrics_store_on_the_active_inode() {
        let directory = TestDirectory::new();
        let mut workspace = workspace(&directory);
        let device =
            DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "METRICS-GENERATION").unwrap();
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
        let metrics_path = directory.path("data/metrics.sqlite3");
        let metrics = MetricsStore::open(&metrics_path).unwrap();
        let attribution = MetricAttribution {
            device_id: device.clone(),
            device_name: "Metrics desk".into(),
            device_profile_id: "red-phone-v1".into(),
            hardware_profile_id: "esp-primary".into(),
        };
        metrics
            .record_button_press(&attribution, "A", 1_720_086_400_000)
            .unwrap();

        workspace
            .duplicate_profile_for_device_with_metrics(
                DuplicateProfileForDeviceRequest {
                    device_id: device,
                    source_profile: workspace.profiles["red-phone-v1"].clone(),
                    name: "Metrics copy".into(),
                },
                &metrics,
            )
            .unwrap();
        metrics
            .record_button_press(&attribution, "A", 1_720_086_400_001)
            .unwrap();
        drop(metrics);

        let reopened = MetricsStore::open(&metrics_path).unwrap();
        let backup = reopened.backup().unwrap();
        assert_eq!(backup.button_metrics.len(), 1);
        assert_eq!(backup.button_metrics[0].total_presses, 2);
    }

    #[test]
    fn duplicate_and_assign_write_failure_rolls_back_profile_and_assignment() {
        let directory = TestDirectory::new();
        let mut workspace = workspace(&directory);
        let device = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "FAILURE").unwrap();
        workspace.enroll_device(device.clone()).unwrap();
        let assignment = RuntimeAssignment {
            device_profile_id: "red-phone-v1".into(),
            hardware_profile_id: "esp-primary".into(),
        };
        workspace
            .set_assignment(&device, assignment.clone())
            .unwrap();
        let profiles_before = workspace.profiles.clone();
        let settings_before = workspace.settings.clone();
        let settings_bytes_before = fs::read(directory.path("data/settings.yaml")).unwrap();
        fs::write(
            directory.path("data.next"),
            b"staging generation is unavailable",
        )
        .unwrap();

        let error = workspace
            .duplicate_profile_for_device(DuplicateProfileForDeviceRequest {
                device_id: device,
                source_profile: workspace.profiles["red-phone-v1"].clone(),
                name: "Failure copy".into(),
            })
            .unwrap_err();

        assert_eq!(error.code, "remove_staging_directory");
        assert_eq!(workspace.profiles, profiles_before);
        assert_eq!(workspace.settings, settings_before);
        assert_eq!(
            fs::read(directory.path("data/settings.yaml")).unwrap(),
            settings_bytes_before
        );
        assert_eq!(
            fs::read_dir(directory.path("data/profiles"))
                .unwrap()
                .count(),
            profiles_before.len()
        );
    }

    #[test]
    fn duplicate_and_assign_does_not_guess_same_hardware_id_across_profiles() {
        let directory = TestDirectory::new();
        let mut workspace = workspace(&directory);
        let device = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "CROSS-PROFILE").unwrap();
        workspace.enroll_device(device.clone()).unwrap();
        let other = workspace
            .create_profile(CreateDeviceProfileRequest::Clone {
                name: "Other profile".into(),
                source_profile_id: "red-phone-v1".into(),
            })
            .unwrap()
            .profile
            .id
            .clone();
        let assignment = RuntimeAssignment {
            device_profile_id: other,
            hardware_profile_id: "esp-primary".into(),
        };
        workspace
            .set_assignment(&device, assignment.clone())
            .unwrap();
        let profiles_before = workspace.profiles.clone();

        let error = workspace
            .duplicate_profile_for_device(DuplicateProfileForDeviceRequest {
                device_id: device.clone(),
                source_profile: workspace.profiles["red-phone-v1"].clone(),
                name: "No guessed mapping".into(),
            })
            .unwrap_err();

        assert_eq!(error.code, "hardware_resolution_required");
        assert_eq!(workspace.profiles, profiles_before);
        assert_eq!(
            workspace.settings.devices[&device]
                .runtime_assignment
                .as_ref()
                .unwrap(),
            &assignment
        );
    }

    #[test]
    fn duplicate_and_assign_profile_write_failure_leaves_workspace_unchanged() {
        let directory = TestDirectory::new();
        let mut workspace = workspace(&directory);
        let device =
            DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "PROFILE-FAILURE").unwrap();
        workspace.enroll_device(device.clone()).unwrap();
        let assignment = RuntimeAssignment {
            device_profile_id: "red-phone-v1".into(),
            hardware_profile_id: "esp-primary".into(),
        };
        workspace.set_assignment(&device, assignment).unwrap();
        let profiles_before = workspace.profiles.clone();
        let settings_before = fs::read(directory.path("data/settings.yaml")).unwrap();
        fs::write(
            directory.path("data.next"),
            b"staging generation is unavailable",
        )
        .unwrap();

        let error = workspace
            .duplicate_profile_for_device(DuplicateProfileForDeviceRequest {
                device_id: device,
                source_profile: workspace.profiles["red-phone-v1"].clone(),
                name: "Profile write failure".into(),
            })
            .unwrap_err();

        assert_eq!(error.code, "remove_staging_directory");
        assert_eq!(workspace.profiles, profiles_before);
        assert_eq!(
            fs::read(directory.path("data/settings.yaml")).unwrap(),
            settings_before
        );
    }

    #[test]
    fn complete_device_setup_allows_multiple_devices_to_share_one_profile() {
        let directory = TestDirectory::new();
        let mut workspace = workspace(&directory);
        let a = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "SHARED-A").unwrap();
        let b = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "SHARED-B").unwrap();
        workspace.enroll_device(a.clone()).unwrap();
        workspace.enroll_device(b.clone()).unwrap();
        let assignment = RuntimeAssignment {
            device_profile_id: "red-phone-v1".into(),
            hardware_profile_id: "esp-primary".into(),
        };

        workspace
            .complete_device_setup(&a, "Shared A".into(), assignment.clone())
            .unwrap();
        workspace
            .complete_device_setup(&b, "Shared B".into(), assignment.clone())
            .unwrap();

        assert_eq!(
            workspace.settings.devices[&a].runtime_assignment,
            Some(assignment.clone())
        );
        assert_eq!(
            workspace.settings.devices[&b].runtime_assignment,
            Some(assignment)
        );
    }

    #[derive(Default)]
    struct InjectedRestoreOperations {
        rename_failures: BTreeMap<(String, String), usize>,
        reopen_failures: VecDeque<bool>,
        open_path: Option<PathBuf>,
    }

    impl InjectedRestoreOperations {
        fn fail_rename(mut self, from: &str, to: &str, count: usize) -> Self {
            self.rename_failures.insert((from.into(), to.into()), count);
            self
        }

        fn fail_reopens(mut self, failures: impl IntoIterator<Item = bool>) -> Self {
            self.reopen_failures.extend(failures);
            self
        }

        fn deny_renaming_ancestor_of(mut self, path: PathBuf) -> Self {
            self.open_path = Some(path);
            self
        }
    }

    impl RestoreOperations for InjectedRestoreOperations {
        fn rename(&mut self, from: &Path, to: &Path) -> std::io::Result<()> {
            let key = (
                from.file_name().unwrap().to_string_lossy().into_owned(),
                to.file_name().unwrap().to_string_lossy().into_owned(),
            );
            if let Some(remaining) = self.rename_failures.get_mut(&key)
                && *remaining > 0
            {
                *remaining -= 1;
                return Err(std::io::Error::other("injected rename failure"));
            }
            if self
                .open_path
                .as_ref()
                .is_some_and(|path| path.starts_with(from))
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "open file blocks ancestor rename",
                ));
            }
            fs::rename(from, to)
        }

        fn remove_dir_all(&mut self, path: &Path) -> std::io::Result<()> {
            fs::remove_dir_all(path)
        }

        fn reopen_metrics(
            &mut self,
            metrics: &MetricsStore,
            path: &Path,
        ) -> Result<(), rusqlite::Error> {
            if self.reopen_failures.pop_front() == Some(true) {
                return Err(rusqlite::Error::InvalidQuery);
            }
            metrics.reopen(path)
        }
    }

    fn restore_fixture(
        directory: &TestDirectory,
    ) -> (
        PathBuf,
        Workspace,
        MetricsStore,
        SettingsDocument,
        MetricsBackup,
    ) {
        let source_directory = directory.path("source-operations");
        let target_directory = directory.path("target-operations");
        let mut second_profile = device_profile();
        second_profile.profile.id = "blue-phone-v1".into();
        second_profile.profile.name = "Blue phone".into();
        let mut source =
            Workspace::create(&source_directory, vec![device_profile(), second_profile]).unwrap();
        source
            .save_settings(EditorSettingsPatch {
                schema_version: SETTINGS_SCHEMA_VERSION,
                editor_profile: Some("red-phone-v1".into()),
                language: Language::EnUs,
            })
            .unwrap();
        let source_metrics =
            MetricsStore::open(&source_directory.join("data/metrics.sqlite3")).unwrap();
        let device = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "AAAAAAAAAAAA").unwrap();
        let source_attribution = MetricAttribution {
            device_id: device.clone(),
            device_name: "Backup desk".into(),
            device_profile_id: "red-phone-v1".into(),
            hardware_profile_id: "esp-primary".into(),
        };
        source_metrics
            .record_button_press(&source_attribution, "A", 1_720_086_400_000)
            .unwrap();
        source.enroll_device(device.clone()).unwrap();
        source
            .set_assignment(
                &device,
                RuntimeAssignment {
                    device_profile_id: "red-phone-v1".into(),
                    hardware_profile_id: "esp-primary".into(),
                },
            )
            .unwrap();
        let rp2040 = DeviceId::new(crate::hardware::YD_RP2040_BOARD_ID, "BBBBBBBBBBBB").unwrap();
        source.enroll_device(rp2040.clone()).unwrap();
        source
            .set_assignment(
                &rp2040,
                RuntimeAssignment {
                    device_profile_id: "blue-phone-v1".into(),
                    hardware_profile_id: "rp-primary".into(),
                },
            )
            .unwrap();
        let backup_path = directory.path("operations-backup.yaml");
        source.export_backup(&backup_path, &source_metrics).unwrap();

        let target = Workspace::create(&target_directory, vec![device_profile()]).unwrap();
        let target_metrics =
            MetricsStore::open(&target_directory.join("data/metrics.sqlite3")).unwrap();
        let original_attribution = MetricAttribution {
            device_name: "Original desk".into(),
            ..source_attribution
        };
        target_metrics
            .record_button_press(&original_attribution, "B", 1_720_086_400_001)
            .unwrap();
        let original_settings = target.settings.clone();
        let original_metrics = target_metrics.backup().unwrap();
        (
            backup_path,
            target,
            target_metrics,
            original_settings,
            original_metrics,
        )
    }

    #[test]
    fn device_profile_round_trips_multiple_hardware_profiles_for_both_boards() {
        let profile = device_profile();
        profile.validate().unwrap();
        let yaml = serde_yaml_ng::to_string(&profile).unwrap();
        let loaded: DeviceProfile = serde_yaml_ng::from_str(&yaml).unwrap();
        assert_eq!(loaded, profile);
        assert_eq!(
            loaded
                .compatible_hardware(crate::hardware::YD_ESP32_S3_BOARD_ID)
                .len(),
            2
        );
        assert_eq!(
            loaded
                .compatible_hardware(crate::hardware::YD_RP2040_BOARD_ID)
                .len(),
            1
        );
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
            keys: BTreeMap::from([("A".into(), 24)]),
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
    fn load_migrates_schema_v2_actions_to_press_without_reordering() {
        let directory = TestDirectory::new();
        let config_directory = directory.path("config");
        let data_directory = config_directory.join("data");
        fs::create_dir_all(data_directory.join("profiles")).unwrap();
        fs::write(
            data_directory.join("settings.yaml"),
            "schema_version: 2\neditor_profile: phone\nlanguage: en-US\ndevices: {}\n",
        )
        .unwrap();
        fs::write(
            data_directory.join("profiles/phone.yaml"),
            r#"schema_version: 2
profile:
  id: phone
  name: Phone
  groups:
    - id: keys
      columns: 1
      buttons:
        - { id: HANDSET, label: HANDSET }
hardware_profiles: []
actions:
  HANDSET:
    - { type: open, target: Phone.app }
    - { type: media, command: play_pause }
"#,
        )
        .unwrap();

        let workspace = Workspace::load_existing(&config_directory).unwrap();
        let actions = &workspace.profiles["phone"].actions["HANDSET"];

        assert_eq!(workspace.profiles["phone"].schema_version, 3);
        assert!(matches!(actions.press[0], ButtonAction::Open { .. }));
        assert!(matches!(actions.press[1], ButtonAction::Media { .. }));

        let persisted = fs::read_to_string(data_directory.join("profiles/phone.yaml")).unwrap();
        assert!(persisted.starts_with("schema_version: 3\n"));
        let persisted: DeviceProfile = serde_yaml_ng::from_str(&persisted).unwrap();
        assert!(matches!(
            persisted.actions["HANDSET"].press[0],
            ButtonAction::Open { .. }
        ));
        assert!(matches!(
            persisted.actions["HANDSET"].press[1],
            ButtonAction::Media { .. }
        ));
    }

    #[test]
    fn load_migrates_legacy_board_ids_without_losing_assignments_or_actions() {
        let directory = TestDirectory::new();
        let mut original = workspace(&directory);
        let id = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "ABCDEF123456").unwrap();
        original.enroll_device(id.clone()).unwrap();
        let assignment = RuntimeAssignment {
            device_profile_id: "red-phone-v1".into(),
            hardware_profile_id: "esp-primary".into(),
        };
        original.set_assignment(&id, assignment.clone()).unwrap();
        drop(original);

        let settings_path = directory.path("data/settings.yaml");
        let legacy_settings = fs::read_to_string(&settings_path)
            .unwrap()
            .replace(
                "11:yd-esp32-s3ABCDEF123456",
                "18:luatos-esp32s3-aioABCDEF123456",
            )
            .replace("yd-esp32-s3", "luatos-esp32s3-aio");
        fs::write(&settings_path, legacy_settings).unwrap();

        let profile_path = directory.path("data/profiles/red-phone-v1.yaml");
        let legacy_profile = fs::read_to_string(&profile_path)
            .unwrap()
            .replace("yd-esp32-s3", "luatos-esp32s3-aio")
            .replace("yd-rp2040", "vccgnd-yd-rp2040");
        fs::write(&profile_path, legacy_profile).unwrap();

        let migrated = Workspace::load_existing(&directory.0).unwrap();
        assert_eq!(
            migrated.settings.devices[&id].board_profile_id,
            crate::hardware::YD_ESP32_S3_BOARD_ID
        );
        assert_eq!(
            migrated.settings.devices[&id].runtime_assignment,
            Some(assignment)
        );
        assert_eq!(
            migrated.profiles["red-phone-v1"].actions,
            device_profile().actions
        );
        assert!(
            migrated.profiles["red-phone-v1"]
                .hardware_profiles
                .iter()
                .all(|hardware| !hardware.board_profile_id.contains("luatos")
                    && !hardware.board_profile_id.contains("vccgnd"))
        );

        let persisted_settings = fs::read_to_string(settings_path).unwrap();
        let persisted_profile = fs::read_to_string(profile_path).unwrap();
        assert!(persisted_settings.contains("11:yd-esp32-s3ABCDEF123456"));
        assert!(persisted_settings.contains("board_profile_id: yd-esp32-s3"));
        assert!(persisted_profile.contains("board_profile_id: yd-esp32-s3"));
        assert!(persisted_profile.contains("board_profile_id: yd-rp2040"));
    }

    #[test]
    fn preview_and_import_migrate_schema_v2_profiles_without_changing_the_id() {
        let directory = TestDirectory::new();
        let mut workspace = workspace(&directory);
        let import_path = directory.path("phone.yaml");
        fs::write(
            &import_path,
            r#"schema_version: 2
profile:
  id: phone
  name: Phone
  groups:
    - id: keys
      columns: 1
      buttons:
        - { id: HANDSET, label: HANDSET }
hardware_profiles: []
actions:
  HANDSET:
    - { type: delay, duration_ms: 10 }
"#,
        )
        .unwrap();

        let preview = workspace.preview_profile(&import_path).unwrap();
        assert_eq!(preview.profile_id, "phone");
        assert_eq!(preview.action_count, 1);

        workspace.import_profile(&import_path).unwrap();
        let profile = &workspace.profiles["phone"];
        assert_eq!(profile.schema_version, PROFILE_SCHEMA_VERSION);
        assert_eq!(profile.actions["HANDSET"].press.len(), 1);
    }

    #[test]
    fn load_migrates_schema_v1_workspace_without_losing_model_data() {
        let directory = TestDirectory::new();
        let config_directory = directory.path("config");
        let data_directory = config_directory.join("data");
        fs::create_dir_all(data_directory.join("models")).unwrap();
        fs::write(
            data_directory.join("settings.yaml"),
            "schema_version: 1\nactive_model: pad_06\nlanguage: zh-CN\n",
        )
        .unwrap();
        fs::write(
            data_directory.join("models/pad_06.yaml"),
            r#"schema_version: 1
model:
  id: pad_06
  name: PAD_06
  groups:
    - id: digits
      columns: 1
      buttons:
        - { id: DIGIT_1, label: "1" }
hardware:
  controller: esp32s3
  debounce_ms: 45
  inputs:
    - type: direct
      id: legacy-direct
      keys:
        DIGIT_1: 1
actions:
  DIGIT_1:
    - type: paste
      text: migrated
legacy:
  unresolved_gpio_text:
    7: preserved-in-backup
"#,
        )
        .unwrap();

        let workspace = Workspace::load(&config_directory, &directory.path("bundled")).unwrap();

        assert_eq!(workspace.settings.schema_version, SETTINGS_SCHEMA_VERSION);
        assert_eq!(workspace.settings.editor_profile.as_deref(), Some("pad_06"));
        assert_eq!(workspace.settings.language, Language::ZhCn);
        assert!(workspace.settings.devices.is_empty());
        let profile = &workspace.profiles["pad_06"];
        assert_eq!(profile.schema_version, PROFILE_SCHEMA_VERSION);
        assert_eq!(
            profile.actions["DIGIT_1"].press[0],
            ButtonAction::Paste {
                text: "migrated".into(),
            }
        );
        assert_eq!(profile.hardware_profiles.len(), 1);
        assert_eq!(
            profile.hardware_profiles[0].board_profile_id,
            crate::hardware::YD_ESP32_S3_BOARD_ID
        );
        assert_eq!(profile.hardware_profiles[0].debounce_ms, 45);
        assert_eq!(
            profile.hardware_profiles[0].inputs,
            vec![InputSource::Direct {
                id: "legacy-direct".into(),
                keys: BTreeMap::from([("DIGIT_1".into(), 1)]),
            }]
        );
        assert!(config_directory.join("data/profiles/pad_06.yaml").exists());
        let backup =
            fs::read_to_string(config_directory.join("data.v1.backup/models/pad_06.yaml")).unwrap();
        assert!(backup.contains("unresolved_gpio_text"));

        assert_eq!(
            Workspace::load(&config_directory, &directory.path("bundled"))
                .unwrap()
                .settings,
            workspace.settings
        );
    }

    #[test]
    fn load_completes_an_interrupted_schema_v1_activation() {
        let directory = TestDirectory::new();
        let config_directory = directory.path("config");
        let data_directory = config_directory.join("data");
        fs::create_dir_all(data_directory.join("models")).unwrap();
        fs::write(
            data_directory.join("settings.yaml"),
            "schema_version: 1\nactive_model: pad_06\nlanguage: en-US\n",
        )
        .unwrap();
        fs::write(
            data_directory.join("models/pad_06.yaml"),
            r#"schema_version: 1
model:
  id: pad_06
  name: PAD_06
  groups: []
hardware:
  controller: esp32s3
  inputs: []
actions: {}
"#,
        )
        .unwrap();
        let bundled = directory.path("bundled");
        Workspace::load(&config_directory, &bundled).unwrap();
        fs::rename(
            config_directory.join("data"),
            config_directory.join("data.next"),
        )
        .unwrap();

        let recovered = Workspace::load(&config_directory, &bundled).unwrap();

        assert_eq!(recovered.settings.editor_profile.as_deref(), Some("pad_06"));
        assert_eq!(recovered.settings.language, Language::EnUs);
        assert!(config_directory.join("data/settings.yaml").exists());
        assert!(
            config_directory
                .join("data.v1.backup/settings.yaml")
                .exists()
        );
        assert!(!config_directory.join("data.next").exists());
    }

    #[test]
    fn load_recovers_valid_next_generation_when_active_data_is_missing() {
        let directory = TestDirectory::new();
        let config_directory = directory.path("config");
        let workspace = Workspace::create(&config_directory, vec![device_profile()]).unwrap();
        let mut next_settings = workspace.settings.clone();
        next_settings.language = Language::EnUs;
        let next_directory = config_directory.join("data.next");
        write_data_directory(&next_directory, &next_settings, &workspace.profiles).unwrap();
        fs::rename(
            config_directory.join("data"),
            config_directory.join("data.previous"),
        )
        .unwrap();

        let recovered = Workspace::load(&config_directory, &directory.path("bundled")).unwrap();

        assert_eq!(recovered.settings.language, Language::EnUs);
        assert!(!config_directory.join("data.previous").exists());
        assert!(!config_directory.join("data.next").exists());
    }

    #[test]
    fn load_discards_stale_next_generation_when_active_data_exists() {
        let directory = TestDirectory::new();
        let config_directory = directory.path("config");
        let workspace = Workspace::create(&config_directory, vec![device_profile()]).unwrap();
        let next_directory = config_directory.join("data.next");
        write_data_directory(&next_directory, &workspace.settings, &workspace.profiles).unwrap();

        let recovered = Workspace::load(&config_directory, &directory.path("bundled")).unwrap();

        assert_eq!(recovered.settings.language, Language::ZhCn);
        assert!(!config_directory.join("data.next").exists());
        assert!(!config_directory.join("data.previous").exists());
    }

    #[test]
    fn settings_reject_mismatched_malformed_and_unknown_board_device_ids() {
        let id = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "ABCDEF123456").unwrap();
        let other = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "654321FEDCBA").unwrap();
        let settings = SettingsDocument {
            devices: BTreeMap::from([(
                id,
                DeviceRecord {
                    device_id: other,
                    name: "Desk".into(),
                    board_profile_id: crate::hardware::YD_ESP32_S3_BOARD_ID.into(),
                    runtime_assignment: None,
                    product_config: None,
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
        let id = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "ABCDEF123456").unwrap();
        unknown.devices.insert(
            id.clone(),
            DeviceRecord {
                device_id: id,
                name: "Desk".into(),
                board_profile_id: "unknown-board".into(),
                runtime_assignment: None,
                product_config: None,
            },
        );
        assert_eq!(
            validate_settings(&unknown, &BTreeMap::new())
                .unwrap_err()
                .code,
            "unknown_board_profile"
        );
        assert!(serde_yaml_ng::from_str::<SettingsDocument>(
            "schema_version: 2\neditor_profile: null\nlanguage: zh-CN\ndevices:\n  malformed:\n    device_id: malformed\n    name: Desk\n    board_profile_id: yd-esp32-s3\n    runtime_assignment: null\n"
        ).is_err());
    }

    #[test]
    fn enrollment_is_idempotent_and_persists_a_default_name() {
        let directory = TestDirectory::new();
        let mut workspace = workspace(&directory);
        let id = DeviceId::new(crate::hardware::YD_RP2040_BOARD_ID, "E0C9125B0D9B").unwrap();
        let first = workspace.enroll_device(id.clone()).unwrap().clone();
        let second = workspace.enroll_device(id.clone()).unwrap().clone();
        assert_eq!(first, second);
        assert_eq!(first.name, "YD-RP2040 · 5B0D9B");
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
        let id = DeviceId::new(crate::hardware::YD_RP2040_BOARD_ID, "E0C9125B0D9B").unwrap();
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
        let removed_id =
            DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "ABCDEF123456").unwrap();
        let changed_id =
            DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "654321FEDCBA").unwrap();
        let added_id = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "ADDED123456").unwrap();
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
                board_profile_id: crate::hardware::YD_ESP32_S3_BOARD_ID.into(),
                runtime_assignment: Some(assignment),
                product_config: None,
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
        let id = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "ABCDEF123456").unwrap();
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
        let id = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "ABCDEF123456").unwrap();
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
        incompatible.hardware_profiles[0].board_profile_id =
            crate::hardware::YD_RP2040_BOARD_ID.into();
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
        let id = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "ABCDEF123456").unwrap();
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
        let mut reloaded = Workspace::load_existing(&directory.0).unwrap();
        assert!(!reloaded.settings.devices.contains_key(&id));
        assert!(reloaded.profiles.contains_key("red-phone-v1"));
        let reenrolled = reloaded.enroll_device(id.clone()).unwrap().clone();
        assert_eq!(id.as_str(), "11:yd-esp32-s3ABCDEF123456");
        assert_eq!(reenrolled.name, "YD-ESP32-S3 · 123456");
        assert_eq!(reenrolled.runtime_assignment, None);
        assert_eq!(
            Workspace::load_existing(&directory.0)
                .unwrap()
                .settings
                .devices[&id],
            reenrolled
        );
    }

    #[test]
    fn full_backup_restore_switches_devices_assignments_and_metrics_together() {
        let directory = TestDirectory::new();
        let (backup, mut target, metrics, original_settings, original_metrics) =
            restore_fixture(&directory);
        assert!(original_settings.devices.is_empty());

        target.restore_backup(&backup, &metrics).unwrap();

        let id = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "AAAAAAAAAAAA").unwrap();
        let rp2040 = DeviceId::new(crate::hardware::YD_RP2040_BOARD_ID, "BBBBBBBBBBBB").unwrap();
        assert_eq!(id.as_str(), "11:yd-esp32-s3AAAAAAAAAAAA");
        assert_eq!(rp2040.as_str(), "9:yd-rp2040BBBBBBBBBBBB");
        assert_eq!(target.profiles.len(), 2);
        assert_eq!(target.settings.devices.len(), 2);
        assert_eq!(
            target.settings.devices[&id].runtime_assignment,
            Some(RuntimeAssignment {
                device_profile_id: "red-phone-v1".into(),
                hardware_profile_id: "esp-primary".into(),
            })
        );
        assert_eq!(
            target.settings.devices[&rp2040].runtime_assignment,
            Some(RuntimeAssignment {
                device_profile_id: "blue-phone-v1".into(),
                hardware_profile_id: "rp-primary".into(),
            })
        );
        assert_eq!(target.settings.language, Language::EnUs);
        let restored_metrics = metrics.backup().unwrap();
        assert_ne!(restored_metrics, original_metrics);
        assert_eq!(restored_metrics.button_metrics.len(), 1);
        assert_eq!(restored_metrics.button_metrics[0].device_id, id);
        assert_eq!(restored_metrics.button_metrics[0].button_id, "A");
        assert_eq!(restored_metrics.activity_logs.len(), 1);
        assert_eq!(restored_metrics.activity_logs[0].device_id, id);
        assert_eq!(
            restored_metrics.activity_logs[0].button_id.as_deref(),
            Some("A")
        );
        assert_eq!(restored_metrics.activity_logs[0].device_name, "Backup desk");
        assert_eq!(
            restored_metrics.activity_logs[0].device_profile_id,
            "red-phone-v1"
        );
        assert_eq!(
            restored_metrics.activity_logs[0].hardware_profile_id,
            "esp-primary"
        );
    }

    #[test]
    fn restore_keeps_open_runtime_log_writes_at_the_active_log_path() {
        let directory = TestDirectory::new();
        let (backup, mut target, metrics, _, _) = restore_fixture(&directory);
        let log_path = crate::runtime_log::log_directory(&target.config_directory).join("kivo.log");
        fs::create_dir_all(log_path.parent().unwrap()).unwrap();
        let mut log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .unwrap();
        log.write_all(b"before restore\n").unwrap();
        log.flush().unwrap();

        let mut operations =
            InjectedRestoreOperations::default().deny_renaming_ancestor_of(log_path.clone());

        target
            .restore_backup_with_operations(&backup, &metrics, &mut operations)
            .unwrap();

        log.write_all(b"after restore\n").unwrap();
        log.flush().unwrap();

        assert_eq!(
            fs::read(&log_path).unwrap(),
            b"before restore\nafter restore\n"
        );
        assert!(!target.config_directory.join("data.previous").exists());
    }

    #[test]
    fn failed_restore_rollback_keeps_open_runtime_log_writes_at_the_active_log_path() {
        let directory = TestDirectory::new();
        let (backup, mut target, metrics, _, _) = restore_fixture(&directory);
        let log_path = crate::runtime_log::log_directory(&target.config_directory).join("kivo.log");
        fs::create_dir_all(log_path.parent().unwrap()).unwrap();
        let mut log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .unwrap();
        log.write_all(b"before rollback\n").unwrap();
        log.flush().unwrap();
        let mut operations = InjectedRestoreOperations::default()
            .deny_renaming_ancestor_of(log_path.clone())
            .fail_rename("data.next", "data", 1);

        let error = target
            .restore_backup_with_operations(&backup, &metrics, &mut operations)
            .unwrap_err();

        assert_eq!(error.code, "activate_restore");
        log.write_all(b"after rollback\n").unwrap();
        log.flush().unwrap();
        assert_eq!(
            fs::read(&log_path).unwrap(),
            b"before rollback\nafter rollback\n"
        );
        assert!(!target.config_directory.join("data.previous").exists());
        assert!(!target.config_directory.join("data.next").exists());
    }

    #[test]
    fn restore_without_a_runtime_log_directory_succeeds() {
        let directory = TestDirectory::new();
        let (backup, mut target, metrics, _, _) = restore_fixture(&directory);
        let log_directory = crate::runtime_log::log_directory(&target.config_directory);
        assert!(!log_directory.exists());

        target.restore_backup(&backup, &metrics).unwrap();

        assert_eq!(target.settings.language, Language::EnUs);
        assert_eq!(target.profiles.len(), 2);
        assert!(!log_directory.exists());
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
        let device = DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "AAAAAAAAAAAA").unwrap();
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

        let mut operations = InjectedRestoreOperations::default().fail_reopens([true, false]);
        let error = target
            .restore_backup_with_operations(&backup_path, &target_metrics, &mut operations)
            .unwrap_err();

        assert_eq!(error.code, "reopen_metrics");
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
    fn rollback_recovers_after_secondary_quarantine_and_reopen_failures() {
        let directory = TestDirectory::new();
        let (backup, mut target, metrics, original_settings, original_metrics) =
            restore_fixture(&directory);
        let target_directory = target.config_directory.clone();
        let mut operations = InjectedRestoreOperations::default()
            .fail_rename("data", "data.next", 1)
            .fail_reopens([true, true, false]);

        let error = target
            .restore_backup_with_operations(&backup, &metrics, &mut operations)
            .unwrap_err();

        assert_eq!(error.code, "reopen_metrics");
        assert!(error.params.contains_key("rollback_errors"));
        assert_eq!(target.settings, original_settings);
        assert_eq!(metrics.backup().unwrap(), original_metrics);
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
    fn activation_rollback_retries_a_secondary_previous_generation_rename_failure() {
        let directory = TestDirectory::new();
        let (backup, mut target, metrics, original_settings, original_metrics) =
            restore_fixture(&directory);
        let target_directory = target.config_directory.clone();
        let mut operations = InjectedRestoreOperations::default()
            .fail_rename("data.next", "data", 1)
            .fail_rename("data.previous", "data", 1);

        let error = target
            .restore_backup_with_operations(&backup, &metrics, &mut operations)
            .unwrap_err();

        assert_eq!(error.code, "activate_restore");
        assert!(error.params.contains_key("rollback_errors"));
        assert_eq!(target.settings, original_settings);
        assert_eq!(metrics.backup().unwrap(), original_metrics);
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
    fn persistent_rollback_reopen_failure_reports_deterministic_fatal_state() {
        let directory = TestDirectory::new();
        let (backup, mut target, metrics, original_settings, _original_metrics) =
            restore_fixture(&directory);
        let target_directory = target.config_directory.clone();
        let mut operations =
            InjectedRestoreOperations::default().fail_reopens([true, true, true, true]);

        let error = target
            .restore_backup_with_operations(&backup, &metrics, &mut operations)
            .unwrap_err();

        assert_eq!(error.code, "restore_rollback_failed");
        assert_eq!(
            error.params.get("primary_code").map(String::as_str),
            Some("reopen_metrics")
        );
        assert_eq!(
            error.params.get("state").map(String::as_str),
            Some("original_active_metrics_closed")
        );
        assert_eq!(target.settings, original_settings);
        assert!(metrics.backup().is_err());
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
            device_id: DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "AAAAAAAAAAAA")
                .unwrap(),
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
    fn full_backup_preview_counts_devices_assignments_metric_rows_and_activity() {
        let directory = TestDirectory::new();
        let mut workspace = workspace(&directory);
        let assigned =
            DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "AAAAAAAAAAAA").unwrap();
        let unassigned =
            DeviceId::new(crate::hardware::YD_RP2040_BOARD_ID, "BBBBBBBBBBBB").unwrap();
        workspace.enroll_device(assigned.clone()).unwrap();
        workspace.enroll_device(unassigned).unwrap();
        workspace
            .set_assignment(
                &assigned,
                RuntimeAssignment {
                    device_profile_id: "red-phone-v1".into(),
                    hardware_profile_id: "esp-primary".into(),
                },
            )
            .unwrap();
        let metrics = MetricsStore::open(&directory.path("data/metrics.sqlite3")).unwrap();
        metrics
            .record_button_press(
                &MetricAttribution {
                    device_id: assigned,
                    device_name: "Backup desk".into(),
                    device_profile_id: "red-phone-v1".into(),
                    hardware_profile_id: "esp-primary".into(),
                },
                "A",
                1_720_086_400_000,
            )
            .unwrap();
        let backup_path = directory.path("counted-backup.yaml");
        workspace.export_backup(&backup_path, &metrics).unwrap();

        let preview = workspace.preview_backup(&backup_path).unwrap();

        assert_eq!(preview.device_count, 2);
        assert_eq!(preview.assignment_count, 1);
        assert_eq!(preview.metric_row_count, 2);
        assert_eq!(preview.activity_count, 1);
    }

    #[test]
    fn bundled_loader_accepts_only_yaml_v3_profiles() {
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
    fn bundled_product_profile_is_valid_v3_yaml() {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../models/prod");
        let profiles = load_bundled_profiles(&directory).unwrap();
        assert_eq!(profiles.len(), 1);
        let profile = &profiles[0];
        assert_eq!(profile.profile.id, "key9");
        assert_eq!(profile.hardware_profiles.len(), 1);
        let hardware = &profile.hardware_profiles[0];
        assert_eq!(hardware.id, "hardware");
        assert_eq!(
            hardware.board_profile_id,
            crate::hardware::YD_RP2040_BOARD_ID
        );
        assert_eq!(
            hardware.inputs,
            vec![InputSource::Direct {
                id: "direct-1".into(),
                keys: BTreeMap::from([
                    ("K1".into(), 1),
                    ("K2".into(), 2),
                    ("K3".into(), 3),
                    ("K4".into(), 4),
                    ("K5".into(), 5),
                    ("K6".into(), 6),
                    ("K7".into(), 7),
                    ("K8".into(), 8),
                    ("K9".into(), 9),
                ]),
            }]
        );
        assert!(profile.actions.is_empty());
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

    #[test]
    fn user_backup_restores_offline_product_devices_and_skips_mismatches() {
        let source_directory = TestDirectory::new();
        let mut source = workspace(&source_directory);
        let device = DeviceId::new(crate::hardware::YD_RP2040_BOARD_ID, "ABCDEF123456").unwrap();
        let definition = product_definition("key");
        source
            .enroll_product_device_with_registry(compiled_registry(), device.clone(), &definition)
            .unwrap();
        source
            .save_product_device_config(
                &device,
                &definition,
                ProductDeviceConfig {
                    product_version_id: "key-rp-k3-r01".into(),
                    trigger_settings: TriggerSettings {
                        long_press_ms: 725,
                        double_press_ms: 260,
                    },
                    actions: BTreeMap::from([(
                        "A".into(),
                        TriggerActions::press(vec![ButtonAction::Delay { duration_ms: 25 }]),
                    )]),
                },
            )
            .unwrap();
        let backup_path = source_directory.path("user-backup.yaml");
        source.export_user_backup(&backup_path).unwrap();
        let preview = source.preview_backup(&backup_path).unwrap();
        assert_eq!(preview.kind, BackupKind::ProductDevices);
        assert_eq!(preview.device_count, 1);
        assert_eq!(preview.action_count, 1);

        let target_directory = TestDirectory::new();
        let mut target = workspace(&target_directory);
        target
            .restore_compatible_backup(&backup_path, None)
            .unwrap();
        let restored = target
            .device(&device)
            .unwrap()
            .product_config
            .as_ref()
            .unwrap();
        assert_eq!(restored.product_version_id, "key-rp-k3-r01");
        assert_eq!(restored.trigger_settings.long_press_ms, 725);

        let mismatch_directory = TestDirectory::new();
        let mut mismatch = workspace(&mismatch_directory);
        let other_definition = product_definition("alt");
        mismatch
            .enroll_product_device_with_registry(
                compiled_registry(),
                device.clone(),
                &other_definition,
            )
            .unwrap();
        mismatch
            .restore_compatible_backup(&backup_path, None)
            .unwrap();
        assert_eq!(
            mismatch
                .device(&device)
                .unwrap()
                .product_config
                .as_ref()
                .unwrap()
                .product_version_id,
            "alt-rp-k3-r01"
        );
    }

    #[test]
    fn restored_unknown_product_button_is_rejected_on_connection() {
        let directory = TestDirectory::new();
        let mut workspace = workspace(&directory);
        let device = DeviceId::new(crate::hardware::YD_RP2040_BOARD_ID, "ABCDEF123456").unwrap();
        let backup_path = directory.path("unknown-button.yaml");
        write_yaml(
            &backup_path,
            &UserBackupDocument {
                schema_version: USER_BACKUP_SCHEMA_VERSION,
                kind: BackupKind::ProductDevices,
                devices: vec![UserBackupDevice {
                    device_id: device.clone(),
                    product_version_id: "key-rp-k3-r01".into(),
                    trigger_settings: TriggerSettings::default(),
                    actions: BTreeMap::from([(
                        "UNKNOWN".into(),
                        TriggerActions::press(vec![ButtonAction::Delay { duration_ms: 10 }]),
                    )]),
                }],
            },
        )
        .unwrap();
        workspace
            .restore_compatible_backup(&backup_path, None)
            .unwrap();

        assert_eq!(
            workspace
                .enroll_product_device_with_registry(
                    compiled_registry(),
                    device,
                    &product_definition("key"),
                )
                .unwrap_err()
                .code,
            "unknown_action_button"
        );
    }

    #[test]
    fn settings_schema_v2_is_migrated_to_v3() {
        let directory = TestDirectory::new();
        let workspace = workspace(&directory);
        drop(workspace);
        let settings_path = directory.path("data/settings.yaml");
        let mut settings = fs::read_to_string(&settings_path).unwrap();
        settings = settings.replacen("schema_version: 3", "schema_version: 2", 1);
        fs::write(&settings_path, settings).unwrap();

        let migrated = Workspace::load_existing(&directory.0).unwrap();
        assert_eq!(migrated.settings.schema_version, SETTINGS_SCHEMA_VERSION);
        assert!(
            fs::read_to_string(settings_path)
                .unwrap()
                .starts_with("schema_version: 3\n")
        );
    }

    #[test]
    fn same_product_devices_keep_independent_actions_until_explicit_copy() {
        let directory = TestDirectory::new();
        let mut workspace = workspace(&directory);
        let definition = product_definition("key");
        let first = DeviceId::new(crate::hardware::YD_RP2040_BOARD_ID, "FIRST123456").unwrap();
        let second = DeviceId::new(crate::hardware::YD_RP2040_BOARD_ID, "SECOND123456").unwrap();
        for device in [&first, &second] {
            workspace
                .enroll_product_device_with_registry(
                    compiled_registry(),
                    device.clone(),
                    &definition,
                )
                .unwrap();
        }
        let first_config = ProductDeviceConfig {
            product_version_id: "key-rp-k3-r01".into(),
            trigger_settings: TriggerSettings::default(),
            actions: BTreeMap::from([(
                "A".into(),
                TriggerActions::press(vec![ButtonAction::Delay { duration_ms: 25 }]),
            )]),
        };
        workspace
            .save_product_device_config(&first, &definition, first_config.clone())
            .unwrap();
        assert!(
            workspace
                .device(&second)
                .unwrap()
                .product_config
                .as_ref()
                .unwrap()
                .actions
                .is_empty()
        );

        workspace
            .copy_product_device_config(&first, &second, &definition)
            .unwrap();
        assert_eq!(
            workspace.device(&second).unwrap().product_config.as_ref(),
            Some(&first_config)
        );
    }
}
