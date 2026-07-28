use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io::ErrorKind, path::Path};

pub const SUPPORTED_GPIOS: [u8; 17] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 13, 14, 15, 16, 17, 18];

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct MappingConfig {
    pub buttons: BTreeMap<u8, String>,
}

#[derive(Deserialize, Serialize)]
struct ConfigDocument {
    #[serde(default)]
    buttons: BTreeMap<u8, String>,
}

impl MappingConfig {
    pub fn from_buttons(buttons: BTreeMap<u8, String>) -> Result<Self, String> {
        if let Some(gpio) = buttons.keys().find(|gpio| !SUPPORTED_GPIOS.contains(gpio)) {
            return Err(format!("unsupported GPIO{gpio}"));
        }
        Ok(Self { buttons })
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
    MappingConfig::from_buttons(document.buttons)
}

pub fn save(path: &Path, config: &MappingConfig) -> Result<(), String> {
    MappingConfig::from_buttons(config.buttons.clone())?;
    let document = ConfigDocument {
        buttons: config
            .buttons
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
                "vibe-tool-{}-{}-{}",
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

    fn buttons(values: &[(u8, &str)]) -> BTreeMap<u8, String> {
        values
            .iter()
            .map(|(gpio, text)| (*gpio, (*text).to_owned()))
            .collect()
    }

    #[test]
    fn loads_unicode_and_multiline_mappings() {
        let directory = TestDirectory::new();
        let path = directory.path("config.yaml");
        fs::write(
            &path,
            "buttons:\n  0: GPIO0 文本\n  6: |-\n    你好\n    second\n",
        )
        .unwrap();

        let config = load(&path).unwrap();

        assert_eq!(config.buttons[&0], "GPIO0 文本");
        assert_eq!(config.buttons[&6], "你好\nsecond");
    }

    #[test]
    fn rejects_unsupported_gpio() {
        let directory = TestDirectory::new();
        let path = directory.path("config.yaml");
        fs::write(&path, "buttons:\n  10: unsafe\n").unwrap();

        assert!(load(&path).unwrap_err().contains("GPIO10"));
    }

    #[test]
    fn missing_file_loads_empty_mappings() {
        let directory = TestDirectory::new();

        assert_eq!(
            load(&directory.path("missing.yaml")).unwrap(),
            MappingConfig::default()
        );
    }

    #[test]
    fn saves_unicode_and_omits_empty_values() {
        let directory = TestDirectory::new();
        let path = directory.path("config.yaml");
        let config = MappingConfig::from_buttons(buttons(&[(6, "你好\nsecond"), (7, "")])).unwrap();

        save(&path, &config).unwrap();

        let saved = fs::read_to_string(&path).unwrap();
        assert_eq!(
            load(&path).unwrap().buttons,
            buttons(&[(6, "你好\nsecond")])
        );
        assert!(!saved.contains("  7:"));
    }

    #[test]
    fn failed_temporary_write_preserves_existing_file() {
        let directory = TestDirectory::new();
        let path = directory.path("config.yaml");
        fs::write(&path, "buttons:\n  6: old\n").unwrap();
        fs::create_dir(directory.path(".config.yaml.tmp")).unwrap();
        let config = MappingConfig::from_buttons(buttons(&[(6, "new")])).unwrap();

        assert!(save(&path, &config).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "buttons:\n  6: old\n");
    }

    #[test]
    fn supported_gpio_set_matches_firmware() {
        assert_eq!(
            SUPPORTED_GPIOS,
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 13, 14, 15, 16, 17, 18]
        );
    }
}
