use crate::{
    config::{self, ButtonAction, MappingConfig},
    model::{self, ModelLayout},
    protocol::encode_hotkey,
    storage::atomic_write,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
};

pub const MODEL_SCHEMA_VERSION: u16 = 1;
pub const SETTINGS_SCHEMA_VERSION: u16 = 1;
pub const BACKUP_SCHEMA_VERSION: u16 = 1;
const MAX_IMPORT_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: String,
    pub params: BTreeMap<String, String>,
    pub detail: Option<String>,
}

impl AppError {
    fn new(code: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            params: BTreeMap::new(),
            detail: None,
        }
    }

    fn with_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }

    fn with_detail(mut self, detail: impl Into<String>) -> Self {
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
pub struct SettingsDocument {
    pub schema_version: u16,
    pub active_model: Option<String>,
    pub language: Language,
}

impl Default for SettingsDocument {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            active_model: None,
            language: Language::ZhCn,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputSource {
    Direct {
        id: String,
        keys: BTreeMap<String, u8>,
    },
    ContactMatrix {
        id: String,
        pins: Vec<u8>,
        keys: BTreeMap<String, [u8; 2]>,
    },
}

impl InputSource {
    fn id(&self) -> &str {
        match self {
            Self::Direct { id, .. } | Self::ContactMatrix { id, .. } => id,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct HardwareConfig {
    pub controller: String,
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u16,
    #[serde(default)]
    pub inputs: Vec<InputSource>,
}

fn default_debounce_ms() -> u16 {
    30
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct LegacyConfig {
    #[serde(default)]
    pub unresolved_gpio_text: BTreeMap<u8, String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ModelConfig {
    pub schema_version: u16,
    pub model: ModelLayout,
    pub hardware: HardwareConfig,
    #[serde(default)]
    pub actions: BTreeMap<String, Vec<ButtonAction>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy: Option<LegacyConfig>,
}

impl ModelConfig {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != MODEL_SCHEMA_VERSION {
            return Err(AppError::new("unsupported_model_schema"));
        }
        self.model
            .validate()
            .map_err(|detail| AppError::new("invalid_layout").with_param("detail", detail))?;
        if !valid_id(&self.model.id) {
            return Err(AppError::new("invalid_model_id"));
        }

        let mut buttons = BTreeSet::new();
        for group in &self.model.groups {
            if !valid_id(&group.id) {
                return Err(AppError::new("invalid_group_id").with_param("group", &group.id));
            }
            for button in &group.buttons {
                if !valid_id(&button.id) {
                    return Err(AppError::new("invalid_button_id").with_param("button", &button.id));
                }
                buttons.insert(button.id.as_str());
            }
        }

        if !valid_id(&self.hardware.controller) {
            return Err(AppError::new("invalid_controller"));
        }
        if !(1..=1000).contains(&self.hardware.debounce_ms) {
            return Err(AppError::new("invalid_debounce"));
        }

        let mut source_ids = BTreeSet::new();
        let mut owned_pins = BTreeSet::new();
        let mut bound_buttons = BTreeSet::new();
        for source in &self.hardware.inputs {
            if !valid_id(source.id()) || !source_ids.insert(source.id()) {
                return Err(AppError::new("invalid_input_source"));
            }
            match source {
                InputSource::Direct { keys, .. } => {
                    for (button, gpio) in keys {
                        validate_binding(button, &buttons, &mut bound_buttons)?;
                        if !owned_pins.insert(*gpio) {
                            return Err(AppError::new("gpio_used_by_multiple_sources")
                                .with_param("gpio", gpio.to_string()));
                        }
                    }
                }
                InputSource::ContactMatrix { pins, keys, .. } => {
                    let unique_pins = pins.iter().copied().collect::<BTreeSet<_>>();
                    if pins.len() < 2 || unique_pins.len() != pins.len() {
                        return Err(AppError::new("invalid_matrix_pins"));
                    }
                    for pin in pins {
                        if !owned_pins.insert(*pin) {
                            return Err(AppError::new("gpio_used_by_multiple_sources")
                                .with_param("gpio", pin.to_string()));
                        }
                    }
                    let mut pairs = BTreeSet::new();
                    let mut edges = Vec::new();
                    for (button, pair) in keys {
                        validate_binding(button, &buttons, &mut bound_buttons)?;
                        let [left, right] = *pair;
                        if left == right
                            || !unique_pins.contains(&left)
                            || !unique_pins.contains(&right)
                        {
                            return Err(AppError::new("invalid_contact_pair"));
                        }
                        let pair = normalized_pair(left, right);
                        if !pairs.insert(pair) {
                            return Err(AppError::new("duplicate_contact_pair"));
                        }
                        edges.push(pair);
                    }
                    validate_bipartite(&edges)?;
                }
            }
        }

        for (button, actions) in &self.actions {
            if !buttons.contains(button.as_str()) {
                return Err(AppError::new("unknown_action_button").with_param("button", button));
            }
            for action in actions {
                match action {
                    ButtonAction::Paste { text } if text.is_empty() => {
                        return Err(AppError::new("empty_paste_text").with_param("button", button));
                    }
                    ButtonAction::Hotkey { keys } => {
                        encode_hotkey(keys).map_err(|detail| {
                            AppError::new("invalid_hotkey")
                                .with_param("button", button)
                                .with_param("detail", detail)
                        })?;
                    }
                    ButtonAction::Paste { .. } => {}
                }
            }
        }
        Ok(())
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn validate_binding<'a>(
    button: &'a str,
    known: &BTreeSet<&str>,
    bound: &mut BTreeSet<&'a str>,
) -> Result<(), AppError> {
    if !known.contains(button) {
        return Err(AppError::new("unknown_hardware_button").with_param("button", button));
    }
    if !bound.insert(button) {
        return Err(AppError::new("button_bound_multiple_times").with_param("button", button));
    }
    Ok(())
}

fn normalized_pair(left: u8, right: u8) -> (u8, u8) {
    if left < right {
        (left, right)
    } else {
        (right, left)
    }
}

fn validate_bipartite(edges: &[(u8, u8)]) -> Result<(), AppError> {
    let mut neighbors: BTreeMap<u8, Vec<u8>> = BTreeMap::new();
    for &(left, right) in edges {
        neighbors.entry(left).or_default().push(right);
        neighbors.entry(right).or_default().push(left);
    }
    let mut colors = BTreeMap::new();
    for &start in neighbors.keys() {
        if colors.contains_key(&start) {
            continue;
        }
        colors.insert(start, false);
        let mut queue = VecDeque::from([start]);
        while let Some(pin) = queue.pop_front() {
            let color = colors[&pin];
            for neighbor in &neighbors[&pin] {
                match colors.get(neighbor) {
                    Some(neighbor_color) if *neighbor_color == color => {
                        return Err(AppError::new("matrix_not_bipartite"));
                    }
                    Some(_) => {}
                    None => {
                        colors.insert(*neighbor, !color);
                        queue.push_back(*neighbor);
                    }
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceSnapshot {
    pub settings: SettingsDocument,
    pub models: BTreeMap<String, ModelConfig>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct BackupDocument {
    pub schema_version: u16,
    pub settings: SettingsDocument,
    pub models: Vec<ModelConfig>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub model_id: String,
    pub model_name: String,
    pub button_count: usize,
    pub hardware_binding_count: usize,
    pub action_count: usize,
    pub replaces_existing: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupPreview {
    pub model_count: usize,
    pub button_count: usize,
    pub hardware_binding_count: usize,
    pub action_count: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LegacyPaths<'a> {
    pub config: Option<&'a Path>,
    pub models: Option<&'a Path>,
}

impl LegacyPaths<'_> {
    pub fn none() -> Self {
        Self::default()
    }
}

pub struct Workspace {
    config_directory: PathBuf,
    pub settings: SettingsDocument,
    pub models: BTreeMap<String, ModelConfig>,
}

impl Workspace {
    pub fn load(
        config_directory: &Path,
        bundled_models: &Path,
        legacy: LegacyPaths<'_>,
    ) -> Result<Self, AppError> {
        if config_directory.join("data/settings.yaml").exists() {
            return Self::load_existing(config_directory);
        }

        if let (Some(config_path), Some(model_directory)) = (legacy.config, legacy.models)
            && config_path.exists()
            && model_directory.exists()
        {
            let legacy_config = config::load(config_path)
                .map_err(|detail| AppError::new("load_legacy_config").with_detail(detail))?;
            let (layouts, errors) = model::load_all(model_directory);
            if !errors.is_empty() {
                return Err(AppError::new("load_legacy_models").with_detail(errors.join("; ")));
            }
            let snapshot = migrate_legacy(layouts, legacy_config)?;
            write_data_directory(
                &config_directory.join("data"),
                &snapshot.settings,
                &snapshot.models,
            )?;
            return Ok(Self {
                config_directory: config_directory.to_owned(),
                settings: snapshot.settings,
                models: snapshot.models,
            });
        }

        let models = load_bundled_models(bundled_models)?;
        Self::create(config_directory, models)
    }

    pub fn create(config_directory: &Path, models: Vec<ModelConfig>) -> Result<Self, AppError> {
        let models = models
            .into_iter()
            .map(|model| {
                model.validate()?;
                Ok((model.model.id.clone(), model))
            })
            .collect::<Result<BTreeMap<_, _>, AppError>>()?;
        let settings = SettingsDocument {
            active_model: models.keys().next().cloned(),
            ..SettingsDocument::default()
        };
        write_data_directory(&config_directory.join("data"), &settings, &models)?;
        Ok(Self {
            config_directory: config_directory.to_owned(),
            settings,
            models,
        })
    }

    pub fn load_existing(config_directory: &Path) -> Result<Self, AppError> {
        let data_directory = config_directory.join("data");
        let settings: SettingsDocument = read_yaml(&data_directory.join("settings.yaml"))?;
        if settings.schema_version != SETTINGS_SCHEMA_VERSION {
            return Err(AppError::new("unsupported_settings_schema"));
        }
        let mut models = BTreeMap::new();
        let model_directory = data_directory.join("models");
        for entry in fs::read_dir(&model_directory)
            .map_err(|error| io_error("read_models", &model_directory, error))?
        {
            let entry = entry.map_err(|error| io_error("read_models", &model_directory, error))?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("yaml") {
                continue;
            }
            let model: ModelConfig = read_yaml(&path)?;
            model.validate()?;
            let expected_name = format!("{}.yaml", model.model.id);
            if path.file_name().and_then(|value| value.to_str()) != Some(&expected_name) {
                return Err(
                    AppError::new("model_filename_mismatch").with_param("model", &model.model.id)
                );
            }
            if models.insert(model.model.id.clone(), model).is_some() {
                return Err(AppError::new("duplicate_model"));
            }
        }
        validate_settings(&settings, &models)?;
        Ok(Self {
            config_directory: config_directory.to_owned(),
            settings,
            models,
        })
    }

    pub fn snapshot(&self) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            settings: self.settings.clone(),
            models: self.models.clone(),
        }
    }

    pub fn save_model(&mut self, model: ModelConfig) -> Result<(), AppError> {
        model.validate()?;
        let path = self
            .model_directory()
            .join(format!("{}.yaml", model.model.id));
        write_yaml(&path, &model)?;
        self.models.insert(model.model.id.clone(), model);
        Ok(())
    }

    pub fn save_settings(&mut self, settings: SettingsDocument) -> Result<(), AppError> {
        validate_settings(&settings, &self.models)?;
        write_yaml(&self.data_directory().join("settings.yaml"), &settings)?;
        self.settings = settings;
        Ok(())
    }

    pub fn preview_model(&self, path: &Path) -> Result<ImportPreview, AppError> {
        let model: ModelConfig = read_yaml_limited(path)?;
        model.validate()?;
        Ok(ImportPreview {
            model_id: model.model.id.clone(),
            model_name: model.model.name.clone(),
            button_count: button_count(&model),
            hardware_binding_count: hardware_binding_count(&model),
            action_count: action_count(&model),
            replaces_existing: self.models.contains_key(&model.model.id),
        })
    }

    pub fn import_model(&mut self, path: &Path) -> Result<(), AppError> {
        let model: ModelConfig = read_yaml_limited(path)?;
        model.validate()?;
        if !self.models.is_empty() {
            return self.save_model(model);
        }

        let model_path = self
            .model_directory()
            .join(format!("{}.yaml", model.model.id));
        write_yaml(&model_path, &model)?;
        let settings = SettingsDocument {
            active_model: Some(model.model.id.clone()),
            ..self.settings.clone()
        };
        if let Err(error) = write_yaml(&self.data_directory().join("settings.yaml"), &settings) {
            let _ = fs::remove_file(model_path);
            return Err(error);
        }
        self.models.insert(model.model.id.clone(), model);
        self.settings = settings;
        Ok(())
    }

    pub fn export_model(&self, id: &str, path: &Path) -> Result<(), AppError> {
        let model = self
            .models
            .get(id)
            .ok_or_else(|| AppError::new("unknown_model").with_param("model", id))?;
        write_yaml(path, model)
    }

    pub fn delete_model(&mut self, id: &str) -> Result<(), AppError> {
        if !self.models.contains_key(id) {
            return Err(AppError::new("unknown_model").with_param("model", id));
        }
        let previous_settings = self.settings.clone();
        let mut next_settings = previous_settings.clone();
        if next_settings.active_model.as_deref() == Some(id) {
            next_settings.active_model = self.models.keys().find(|key| key.as_str() != id).cloned();
        }
        write_yaml(&self.data_directory().join("settings.yaml"), &next_settings)?;
        let path = self.model_directory().join(format!("{id}.yaml"));
        if let Err(error) = fs::remove_file(&path) {
            let _ = write_yaml(
                &self.data_directory().join("settings.yaml"),
                &previous_settings,
            );
            return Err(io_error("delete_model", &path, error));
        }
        self.models.remove(id);
        self.settings = next_settings;
        Ok(())
    }

    pub fn preview_backup(&self, path: &Path) -> Result<BackupPreview, AppError> {
        let backup = read_backup(path)?;
        Ok(BackupPreview {
            model_count: backup.models.len(),
            button_count: backup.models.values().map(button_count).sum(),
            hardware_binding_count: backup.models.values().map(hardware_binding_count).sum(),
            action_count: backup.models.values().map(action_count).sum(),
        })
    }

    pub fn export_backup(&self, path: &Path) -> Result<(), AppError> {
        write_yaml(
            path,
            &BackupDocument {
                schema_version: BACKUP_SCHEMA_VERSION,
                settings: self.settings.clone(),
                models: self.models.values().cloned().collect(),
            },
        )
    }

    pub fn restore_backup(&mut self, path: &Path) -> Result<(), AppError> {
        let snapshot = read_backup(path)?;
        let data_directory = self.data_directory();
        let next_directory = self.config_directory.join("data.next");
        let previous_directory = self.config_directory.join("data.previous");
        remove_directory_if_exists(&next_directory)?;
        remove_directory_if_exists(&previous_directory)?;
        write_data_directory(&next_directory, &snapshot.settings, &snapshot.models)?;
        fs::rename(&data_directory, &previous_directory)
            .map_err(|error| io_error("stage_restore", &data_directory, error))?;
        if let Err(error) = fs::rename(&next_directory, &data_directory) {
            let _ = fs::rename(&previous_directory, &data_directory);
            return Err(io_error("activate_restore", &next_directory, error));
        }
        self.settings = snapshot.settings;
        self.models = snapshot.models;
        let _ = fs::remove_dir_all(previous_directory);
        Ok(())
    }

    fn data_directory(&self) -> PathBuf {
        self.config_directory.join("data")
    }

    fn model_directory(&self) -> PathBuf {
        self.data_directory().join("models")
    }
}

fn load_bundled_models(directory: &Path) -> Result<Vec<ModelConfig>, AppError> {
    let mut models = Vec::new();
    let entries = fs::read_dir(directory)
        .map_err(|error| io_error("read_bundled_models", directory, error))?;
    for entry in entries {
        let entry = entry.map_err(|error| io_error("read_bundled_models", directory, error))?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("yaml") {
            continue;
        }
        let model: ModelConfig = read_yaml_limited(&path)?;
        model.validate()?;
        let expected_name = format!("{}.yaml", model.model.id);
        if path.file_name().and_then(|value| value.to_str()) != Some(&expected_name) {
            return Err(
                AppError::new("model_filename_mismatch").with_param("model", &model.model.id)
            );
        }
        models.push(model);
    }
    models.sort_by(|left, right| left.model.id.cmp(&right.model.id));
    Ok(models)
}

fn read_backup(path: &Path) -> Result<WorkspaceSnapshot, AppError> {
    let backup: BackupDocument = read_yaml_limited(path)?;
    if backup.schema_version != BACKUP_SCHEMA_VERSION {
        return Err(AppError::new("unsupported_backup_schema"));
    }
    let mut models = BTreeMap::new();
    for model in backup.models {
        model.validate()?;
        if models.insert(model.model.id.clone(), model).is_some() {
            return Err(AppError::new("duplicate_model"));
        }
    }
    validate_settings(&backup.settings, &models)?;
    Ok(WorkspaceSnapshot {
        settings: backup.settings,
        models,
    })
}

fn button_count(model: &ModelConfig) -> usize {
    model
        .model
        .groups
        .iter()
        .map(|group| group.buttons.len())
        .sum()
}

fn hardware_binding_count(model: &ModelConfig) -> usize {
    model
        .hardware
        .inputs
        .iter()
        .map(|input| match input {
            InputSource::Direct { keys, .. } => keys.len(),
            InputSource::ContactMatrix { keys, .. } => keys.len(),
        })
        .sum()
}

fn action_count(model: &ModelConfig) -> usize {
    model.actions.values().map(Vec::len).sum()
}

fn remove_directory_if_exists(path: &Path) -> Result<(), AppError> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("remove_staging_directory", path, error)),
    }
}

pub fn migrate_legacy(
    layouts: Vec<ModelLayout>,
    legacy: MappingConfig,
) -> Result<WorkspaceSnapshot, AppError> {
    let mut models = BTreeMap::new();
    for layout in layouts {
        let button_ids = layout
            .groups
            .iter()
            .flat_map(|group| &group.buttons)
            .map(|button| button.id.as_str())
            .collect::<BTreeSet<_>>();
        let direct_keys = legacy
            .io_maps
            .get(&layout.id)
            .into_iter()
            .flat_map(|mapping| mapping.iter())
            .map(|(gpio, button)| (button.clone(), *gpio))
            .collect::<BTreeMap<_, _>>();
        let inputs = if direct_keys.is_empty() {
            Vec::new()
        } else {
            vec![InputSource::Direct {
                id: "legacy-direct".into(),
                keys: direct_keys,
            }]
        };
        let actions = legacy
            .actions
            .iter()
            .filter(|(button, _)| button_ids.contains(button.as_str()))
            .map(|(button, action)| (button.clone(), vec![action.clone()]))
            .collect();
        let legacy_config = (layout.id == legacy.active_model && !legacy.legacy_buttons.is_empty())
            .then(|| LegacyConfig {
                unresolved_gpio_text: legacy.legacy_buttons.clone(),
            });
        let model = ModelConfig {
            schema_version: MODEL_SCHEMA_VERSION,
            model: layout,
            hardware: HardwareConfig {
                controller: "esp32s3".into(),
                debounce_ms: 30,
                inputs,
            },
            actions,
            legacy: legacy_config,
        };
        model.validate()?;
        models.insert(model.model.id.clone(), model);
    }
    let active_model = if models.contains_key(&legacy.active_model) {
        Some(legacy.active_model)
    } else {
        models.keys().next().cloned()
    };
    Ok(WorkspaceSnapshot {
        settings: SettingsDocument {
            schema_version: SETTINGS_SCHEMA_VERSION,
            active_model,
            language: Language::ZhCn,
        },
        models,
    })
}

fn validate_settings(
    settings: &SettingsDocument,
    models: &BTreeMap<String, ModelConfig>,
) -> Result<(), AppError> {
    if settings.schema_version != SETTINGS_SCHEMA_VERSION {
        return Err(AppError::new("unsupported_settings_schema"));
    }
    if let Some(active_model) = &settings.active_model
        && !models.contains_key(active_model)
    {
        return Err(AppError::new("unknown_active_model").with_param("model", active_model));
    }
    Ok(())
}

fn write_data_directory(
    data_directory: &Path,
    settings: &SettingsDocument,
    models: &BTreeMap<String, ModelConfig>,
) -> Result<(), AppError> {
    let model_directory = data_directory.join("models");
    fs::create_dir_all(&model_directory)
        .map_err(|error| io_error("create_data", &model_directory, error))?;
    for model in models.values() {
        write_yaml(
            &model_directory.join(format!("{}.yaml", model.model.id)),
            model,
        )?;
    }
    write_yaml(&data_directory.join("settings.yaml"), settings)
}

fn read_yaml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, AppError> {
    let contents = fs::read_to_string(path).map_err(|error| io_error("read_file", path, error))?;
    serde_yaml_ng::from_str(&contents)
        .map_err(|error| AppError::new("invalid_yaml").with_detail(error.to_string()))
}

fn read_yaml_limited<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, AppError> {
    let metadata = fs::metadata(path).map_err(|error| io_error("read_file", path, error))?;
    if metadata.len() > MAX_IMPORT_BYTES {
        return Err(
            AppError::new("file_too_large").with_param("limit", MAX_IMPORT_BYTES.to_string())
        );
    }
    read_yaml(path)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{ButtonAction, MappingConfig},
        model::{ButtonDefinition, ButtonGroup, ModelLayout},
    };
    use std::{
        collections::BTreeMap,
        fs,
        path::PathBuf,
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

    fn layout(button_ids: &[&str]) -> ModelLayout {
        ModelLayout {
            id: "red-phone-v1".into(),
            name: "红色电话机".into(),
            groups: vec![ButtonGroup {
                id: "digits".into(),
                columns: button_ids.len(),
                buttons: button_ids
                    .iter()
                    .map(|id| ButtonDefinition {
                        id: (*id).into(),
                        label: (*id).into(),
                    })
                    .collect(),
            }],
        }
    }

    fn model_config(actions: Vec<ButtonAction>) -> ModelConfig {
        ModelConfig {
            schema_version: MODEL_SCHEMA_VERSION,
            model: layout(&["A", "B", "C"]),
            hardware: HardwareConfig {
                controller: "esp32s3".into(),
                debounce_ms: 30,
                inputs: Vec::new(),
            },
            actions: BTreeMap::from([("A".into(), actions)]),
            legacy: None,
        }
    }

    #[test]
    fn model_config_round_trips_unicode_and_action_order() {
        let config = model_config(vec![
            ButtonAction::Paste {
                text: "你好\n".into(),
            },
            ButtonAction::Hotkey {
                keys: vec!["enter".into()],
            },
        ]);

        let yaml = serde_yaml_ng::to_string(&config).unwrap();
        let loaded: ModelConfig = serde_yaml_ng::from_str(&yaml).unwrap();

        assert_eq!(loaded, config);
    }

    #[test]
    fn rejects_non_bipartite_contact_graph() {
        let mut config = model_config(Vec::new());
        config.hardware.inputs = vec![InputSource::ContactMatrix {
            id: "keys".into(),
            pins: vec![1, 2, 3],
            keys: BTreeMap::from([
                ("A".into(), [1, 2]),
                ("B".into(), [2, 3]),
                ("C".into(), [3, 1]),
            ]),
        }];

        assert_eq!(config.validate().unwrap_err().code, "matrix_not_bipartite");
    }

    #[test]
    fn deleting_last_model_persists_an_empty_workspace() {
        let directory = TestDirectory::new();
        let mut workspace =
            Workspace::create(&directory.0, vec![model_config(Vec::new())]).unwrap();

        workspace.delete_model("red-phone-v1").unwrap();

        let reloaded = Workspace::load_existing(&directory.0).unwrap();
        assert!(reloaded.models.is_empty());
        assert_eq!(reloaded.settings.active_model, None);
    }

    #[test]
    fn legacy_global_action_is_copied_per_model_and_unresolved_text_survives() {
        let first = layout(&["A", "B"]);
        let mut second = layout(&["A", "C"]);
        second.id = "other-model".into();
        second.name = "Other".into();
        let legacy = MappingConfig {
            active_model: first.id.clone(),
            io_maps: BTreeMap::from([
                (first.id.clone(), BTreeMap::from([(6, "A".into())])),
                (second.id.clone(), BTreeMap::from([(7, "A".into())])),
            ]),
            actions: BTreeMap::from([(
                "A".into(),
                ButtonAction::Paste {
                    text: "你好".into(),
                },
            )]),
            legacy_buttons: BTreeMap::from([(8, "preserve".into())]),
        };

        let migrated = migrate_legacy(vec![first, second], legacy).unwrap();

        assert_eq!(migrated.models["red-phone-v1"].actions["A"].len(), 1);
        assert_eq!(migrated.models["other-model"].actions["A"].len(), 1);
        assert_eq!(
            migrated.models["red-phone-v1"]
                .legacy
                .as_ref()
                .unwrap()
                .unresolved_gpio_text[&8],
            "preserve",
        );
    }

    #[test]
    fn same_id_import_replaces_only_that_model() {
        let directory = TestDirectory::new();
        let mut workspace =
            Workspace::create(&directory.0, vec![model_config(Vec::new())]).unwrap();
        let mut replacement = model_config(vec![ButtonAction::Paste {
            text: "替换".into(),
        }]);
        replacement.model.name = "替换型号".into();
        let import_path = directory.path("incoming.kivo-model.yaml");
        fs::write(
            &import_path,
            serde_yaml_ng::to_string(&replacement).unwrap(),
        )
        .unwrap();

        let preview = workspace.preview_model(&import_path).unwrap();
        assert!(preview.replaces_existing);
        workspace.import_model(&import_path).unwrap();

        assert_eq!(workspace.models["red-phone-v1"], replacement);
    }

    #[test]
    fn backup_restore_replaces_the_complete_snapshot() {
        let directory = TestDirectory::new();
        let mut original = model_config(vec![ButtonAction::Paste {
            text: "原始".into(),
        }]);
        original.model.name = "原始型号".into();
        let mut workspace = Workspace::create(&directory.0, vec![original.clone()]).unwrap();
        let backup_path = directory.path("backup.yaml");
        workspace.export_backup(&backup_path).unwrap();
        workspace.delete_model("red-phone-v1").unwrap();

        workspace.restore_backup(&backup_path).unwrap();

        assert_eq!(
            workspace.models,
            BTreeMap::from([("red-phone-v1".into(), original)])
        );
        assert_eq!(
            workspace.settings.active_model.as_deref(),
            Some("red-phone-v1")
        );
    }

    #[test]
    fn deleted_bundled_models_are_not_reseeded() {
        let directory = TestDirectory::new();
        let config_directory = directory.path("config");
        let bundled_directory = directory.path("bundled");
        fs::create_dir_all(&bundled_directory).unwrap();
        let bundled = model_config(Vec::new());
        fs::write(
            bundled_directory.join("red-phone-v1.yaml"),
            serde_yaml_ng::to_string(&bundled).unwrap(),
        )
        .unwrap();
        let mut workspace =
            Workspace::load(&config_directory, &bundled_directory, LegacyPaths::none()).unwrap();
        workspace.delete_model("red-phone-v1").unwrap();

        let reloaded =
            Workspace::load(&config_directory, &bundled_directory, LegacyPaths::none()).unwrap();

        assert!(reloaded.models.is_empty());
    }

    #[test]
    fn previews_counts_and_exports_the_complete_model() {
        let directory = TestDirectory::new();
        let mut model = model_config(vec![
            ButtonAction::Paste {
                text: "中文".into(),
            },
            ButtonAction::Hotkey {
                keys: vec!["cmd".into(), "k".into()],
            },
        ]);
        model.hardware.inputs = vec![InputSource::Direct {
            id: "direct".into(),
            keys: BTreeMap::from([("A".into(), 6)]),
        }];
        let workspace = Workspace::create(&directory.0, vec![model.clone()]).unwrap();
        let export_path = directory.path("red-phone-v1.kivo-model.yaml");
        workspace
            .export_model("red-phone-v1", &export_path)
            .unwrap();

        let preview = workspace.preview_model(&export_path).unwrap();
        assert_eq!(
            (
                preview.button_count,
                preview.hardware_binding_count,
                preview.action_count
            ),
            (3, 1, 2)
        );
        assert!(preview.replaces_existing);
        assert_eq!(read_yaml::<ModelConfig>(&export_path).unwrap(), model);
    }

    #[test]
    fn rejects_imports_larger_than_ten_mibibytes() {
        let directory = TestDirectory::new();
        let workspace = Workspace::create(&directory.0, vec![model_config(Vec::new())]).unwrap();
        let path = directory.path("too-large.yaml");
        let file = fs::File::create(&path).unwrap();
        file.set_len(MAX_IMPORT_BYTES + 1).unwrap();

        assert_eq!(
            workspace.preview_model(&path).unwrap_err().code,
            "file_too_large"
        );
    }
}
