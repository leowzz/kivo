#[cfg(test)]
use crate::model::ModelLayout;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io::ErrorKind, path::Path};

pub const SUPPORTED_GPIOS: [u8; 17] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 13, 14, 15, 16, 17, 18];

pub type IoMaps = BTreeMap<String, BTreeMap<u8, String>>;

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ButtonAction {
    Paste { text: String },
    Hotkey { keys: Vec<String> },
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct MappingConfig {
    pub active_model: String,
    pub io_maps: IoMaps,
    pub actions: BTreeMap<String, ButtonAction>,
    #[serde(skip)]
    pub legacy_buttons: BTreeMap<u8, String>,
}

#[derive(Default, Deserialize, Serialize)]
struct ConfigDocument {
    #[serde(default)]
    active_model: String,
    #[serde(default)]
    io_maps: IoMaps,
    #[serde(default)]
    actions: BTreeMap<String, ButtonAction>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    buttons: BTreeMap<u8, String>,
}

impl MappingConfig {
    #[cfg(test)]
    pub fn resolved_button(&self, gpio: u8) -> Option<&str> {
        self.io_maps
            .get(&self.active_model)?
            .get(&gpio)
            .map(String::as_str)
    }

    #[cfg(test)]
    pub fn resolved_action(&self, gpio: u8) -> Option<ButtonAction> {
        self.resolved_button(gpio)
            .and_then(|button| self.actions.get(button))
            .cloned()
            .or_else(|| {
                self.legacy_buttons
                    .get(&gpio)
                    .filter(|text| !text.is_empty())
                    .cloned()
                    .map(|text| ButtonAction::Paste { text })
            })
    }

    pub fn migrate_legacy(&mut self) {
        let Some(io_map) = self.io_maps.get(&self.active_model) else {
            return;
        };
        let migrations = self
            .legacy_buttons
            .iter()
            .filter_map(|(gpio, text)| {
                (!text.is_empty())
                    .then(|| {
                        io_map
                            .get(gpio)
                            .map(|button| (*gpio, button.clone(), text.clone()))
                    })
                    .flatten()
            })
            .collect::<Vec<_>>();
        for (gpio, button, text) in migrations {
            self.actions
                .entry(button)
                .or_insert(ButtonAction::Paste { text });
            self.legacy_buttons.remove(&gpio);
        }
    }

    fn validate_contents(&self) -> Result<(), String> {
        if let Some(gpio) = self
            .legacy_buttons
            .keys()
            .find(|gpio| !SUPPORTED_GPIOS.contains(gpio))
        {
            return Err(format!("unsupported GPIO{gpio}"));
        }
        for (button, action) in &self.actions {
            match action {
                ButtonAction::Paste { text } if text.trim().is_empty() => {
                    return Err(format!("button {button} has empty paste text"));
                }
                ButtonAction::Hotkey { keys } => {
                    crate::protocol::encode_hotkey(keys)
                        .map_err(|error| format!("button {button} has invalid hotkey: {error}"))?;
                }
                ButtonAction::Paste { .. } => {}
            }
        }
        for (model_id, io_map) in &self.io_maps {
            let mut assigned = std::collections::BTreeSet::new();
            for (gpio, button) in io_map {
                if !SUPPORTED_GPIOS.contains(gpio) {
                    return Err(format!("unsupported GPIO{gpio}"));
                }
                if !assigned.insert(button.as_str()) {
                    return Err(format!("duplicate button {button} for model {model_id}"));
                }
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn validate(&self, models: &[ModelLayout]) -> Result<(), String> {
        self.validate_contents()?;
        if self.active_model.trim().is_empty() {
            return Err("active model is required".into());
        }
        if !models.iter().any(|model| model.id == self.active_model) {
            return Err(format!("unknown active model {}", self.active_model));
        }
        for (model_id, io_map) in &self.io_maps {
            let model = models
                .iter()
                .find(|model| model.id == *model_id)
                .ok_or_else(|| format!("unknown model {model_id}"))?;
            let buttons = model
                .groups
                .iter()
                .flat_map(|group| &group.buttons)
                .map(|button| button.id.as_str())
                .collect::<std::collections::BTreeSet<_>>();
            if let Some(button) = io_map
                .values()
                .find(|button| !buttons.contains(button.as_str()))
            {
                return Err(format!("unknown button {button} for model {model_id}"));
            }
        }
        Ok(())
    }
}

pub fn load(path: &Path) -> Result<MappingConfig, String> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(MappingConfig::default()),
        Err(error) => return Err(format!("read {}: {error}", path.display())),
    };
    let document: ConfigDocument =
        serde_yaml_ng::from_str(&contents).map_err(|error| format!("invalid YAML: {error}"))?;
    let mut config = MappingConfig {
        active_model: document.active_model,
        io_maps: document.io_maps,
        actions: document.actions,
        legacy_buttons: document.buttons,
    };
    config.migrate_legacy();
    config.validate_contents()?;
    Ok(config)
}

#[cfg(test)]
pub fn save(path: &Path, config: &MappingConfig) -> Result<(), String> {
    let mut config = config.clone();
    config.migrate_legacy();
    config.validate_contents()?;
    let document = ConfigDocument {
        active_model: config.active_model,
        io_maps: config.io_maps,
        actions: config.actions,
        buttons: config
            .legacy_buttons
            .iter()
            .filter(|(_, text)| !text.is_empty())
            .map(|(gpio, text)| (*gpio, text.clone()))
            .collect(),
    };
    let yaml = serde_yaml_ng::to_string(&document)
        .map_err(|error| format!("serialize mappings: {error}"))?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| format!("create {}: {error}", parent.display()))?;
    crate::storage::atomic_write(path, yaml.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ButtonDefinition, ButtonGroup, ModelLayout};
    use std::{
        collections::BTreeMap,
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let name = format!(
                "kivo-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed)
            );
            let path = std::env::temp_dir().join(name);
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

    fn model() -> ModelLayout {
        ModelLayout {
            id: "red-phone-v1".into(),
            name: "Red Phone v1".into(),
            groups: vec![ButtonGroup {
                id: "digits".into(),
                columns: 2,
                buttons: vec![
                    ButtonDefinition {
                        id: "DIGIT_1".into(),
                        label: "1".into(),
                    },
                    ButtonDefinition {
                        id: "DIGIT_2".into(),
                        label: "2".into(),
                    },
                ],
            }],
        }
    }

    #[test]
    fn resolves_model_gpio_to_global_action() {
        let config = MappingConfig {
            active_model: "red-phone-v1".into(),
            io_maps: BTreeMap::from([(
                "red-phone-v1".into(),
                BTreeMap::from([(6, "DIGIT_2".into())]),
            )]),
            actions: BTreeMap::from([(
                "DIGIT_2".into(),
                ButtonAction::Hotkey {
                    keys: vec!["cmd".into(), "shift".into(), "k".into()],
                },
            )]),
            legacy_buttons: BTreeMap::new(),
        };

        assert!(matches!(
            config.resolved_action(6),
            Some(ButtonAction::Hotkey { .. })
        ));
    }

    #[test]
    fn explicit_global_action_wins_over_legacy_gpio_text() {
        let config = MappingConfig {
            active_model: "red-phone-v1".into(),
            io_maps: BTreeMap::from([(
                "red-phone-v1".into(),
                BTreeMap::from([(6, "DIGIT_2".into())]),
            )]),
            actions: BTreeMap::from([(
                "DIGIT_2".into(),
                ButtonAction::Paste { text: "new".into() },
            )]),
            legacy_buttons: BTreeMap::from([(6, "old".into())]),
        };

        assert_eq!(
            config.resolved_action(6),
            Some(ButtonAction::Paste { text: "new".into() })
        );
    }

    #[test]
    fn migrates_only_legacy_gpio_entries_known_to_active_model() {
        let mut config = MappingConfig {
            active_model: "red-phone-v1".into(),
            io_maps: BTreeMap::from([(
                "red-phone-v1".into(),
                BTreeMap::from([(6, "DIGIT_2".into())]),
            )]),
            actions: BTreeMap::new(),
            legacy_buttons: BTreeMap::from([(6, "hello".into()), (7, "keep".into())]),
        };

        config.migrate_legacy();

        assert_eq!(
            config.actions["DIGIT_2"],
            ButtonAction::Paste {
                text: "hello".into()
            }
        );
        assert_eq!(config.legacy_buttons, BTreeMap::from([(7, "keep".into())]));
    }

    #[test]
    fn loading_hybrid_config_migrates_resolvable_legacy_entries() {
        let directory = TestDirectory::new();
        let path = directory.path("config.yaml");
        fs::write(
            &path,
            "active_model: red-phone-v1\nio_maps:\n  red-phone-v1:\n    6: DIGIT_2\nbuttons:\n  6: hello\n  7: keep\n",
        )
        .unwrap();

        let config = load(&path).unwrap();

        assert_eq!(
            config.actions.get("DIGIT_2"),
            Some(&ButtonAction::Paste {
                text: "hello".into()
            })
        );
        assert_eq!(config.legacy_buttons, BTreeMap::from([(7, "keep".into())]));
    }

    #[test]
    fn loading_hybrid_config_rejects_whitespace_only_migrated_paste() {
        let directory = TestDirectory::new();
        let path = directory.path("config.yaml");
        fs::write(
            &path,
            "active_model: red-phone-v1\nio_maps:\n  red-phone-v1:\n    6: DIGIT_2\nbuttons:\n  6: \" \"\n",
        )
        .unwrap();

        assert!(load(&path).is_err());
    }

    #[test]
    fn unresolved_legacy_entries_survive_save_reload() {
        let directory = TestDirectory::new();
        let path = directory.path("config.yaml");
        let config = MappingConfig {
            active_model: "red-phone-v1".into(),
            io_maps: BTreeMap::from([(
                "red-phone-v1".into(),
                BTreeMap::from([(6, "DIGIT_2".into())]),
            )]),
            actions: BTreeMap::new(),
            legacy_buttons: BTreeMap::from([(7, "keep".into())]),
        };

        save(&path, &config).unwrap();

        assert_eq!(load(&path).unwrap().legacy_buttons, config.legacy_buttons);
    }

    #[test]
    fn rejects_unsupported_gpio() {
        let config = MappingConfig {
            active_model: "red-phone-v1".into(),
            io_maps: BTreeMap::from([(
                "red-phone-v1".into(),
                BTreeMap::from([(10, "DIGIT_2".into())]),
            )]),
            actions: BTreeMap::new(),
            legacy_buttons: BTreeMap::new(),
        };

        assert!(config.validate(&[model()]).unwrap_err().contains("GPIO10"));
    }

    #[test]
    fn rejects_duplicate_button_assignments_within_a_model() {
        let config = MappingConfig {
            active_model: "red-phone-v1".into(),
            io_maps: BTreeMap::from([(
                "red-phone-v1".into(),
                BTreeMap::from([(6, "DIGIT_2".into()), (7, "DIGIT_2".into())]),
            )]),
            actions: BTreeMap::new(),
            legacy_buttons: BTreeMap::new(),
        };

        assert!(config.validate(&[model()]).unwrap_err().contains("DIGIT_2"));
    }

    #[test]
    fn rejects_empty_paste_text() {
        let config = MappingConfig {
            active_model: "red-phone-v1".into(),
            actions: BTreeMap::from([("DIGIT_2".into(), ButtonAction::Paste { text: " ".into() })]),
            ..MappingConfig::default()
        };

        assert!(config.validate(&[model()]).unwrap_err().contains("paste"));
    }

    #[test]
    fn rejects_malformed_hotkeys() {
        let config = MappingConfig {
            active_model: "red-phone-v1".into(),
            actions: BTreeMap::from([(
                "DIGIT_2".into(),
                ButtonAction::Hotkey {
                    keys: vec!["cmd".into(), "cmd".into(), "k".into()],
                },
            )]),
            ..MappingConfig::default()
        };

        assert!(config.validate(&[model()]).is_err());
    }

    #[test]
    fn rejects_empty_active_model() {
        let error = MappingConfig::default().validate(&[model()]).unwrap_err();

        assert!(error.contains("active model"));
    }

    #[test]
    fn rejects_unknown_active_model() {
        let config = MappingConfig {
            active_model: "missing".into(),
            ..MappingConfig::default()
        };

        assert!(config.validate(&[model()]).unwrap_err().contains("missing"));
    }
}
