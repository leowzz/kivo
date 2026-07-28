use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, path::Path};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ButtonDefinition {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ButtonGroup {
    pub id: String,
    pub columns: usize,
    pub buttons: Vec<ButtonDefinition>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ModelLayout {
    pub id: String,
    pub name: String,
    pub groups: Vec<ButtonGroup>,
}

impl ModelLayout {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() || self.name.trim().is_empty() {
            return Err("model id and name are required".into());
        }
        if !self
            .id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err("model id must use ASCII letters, digits, hyphens, or underscores".into());
        }
        let mut groups = BTreeSet::new();
        let mut buttons = BTreeSet::new();
        for group in &self.groups {
            if group.id.trim().is_empty() || !groups.insert(group.id.as_str()) {
                return Err(format!("invalid or duplicate group {}", group.id));
            }
            if group.columns == 0 || group.buttons.is_empty() {
                return Err(format!("group {} must have columns and buttons", group.id));
            }
            for button in &group.buttons {
                if button.id.trim().is_empty() || button.label.trim().is_empty() {
                    return Err("button id and label are required".into());
                }
                if !buttons.insert(button.id.as_str()) {
                    return Err(format!("duplicate button {}", button.id));
                }
            }
        }
        Ok(())
    }
}

pub fn load_all(directory: &Path) -> (Vec<ModelLayout>, Vec<String>) {
    let mut layouts = Vec::new();
    let mut errors = Vec::new();
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push(format!("read {}: {error}", directory.display()));
            return (layouts, errors);
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                errors.push(format!("read {}: {error}", directory.display()));
                continue;
            }
        };
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let result = fs::read_to_string(&path)
            .map_err(|error| error.to_string())
            .and_then(|contents| {
                serde_json::from_str::<ModelLayout>(&contents).map_err(|error| error.to_string())
            })
            .and_then(|layout| {
                layout.validate()?;
                Ok(layout)
            });
        match result {
            Ok(layout) => layouts.push(layout),
            Err(error) => errors.push(format!("load {}: {error}", path.display())),
        }
    }
    layouts.sort_by(|left, right| left.name.cmp(&right.name));
    (layouts, errors)
}

pub fn sync_bundled(source: &Path, destination: &Path) -> Vec<String> {
    let entries = match fs::read_dir(source) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => return vec![format!("read {}: {error}", source.display())],
    };
    let mut errors = Vec::new();
    let mut paths = Vec::new();
    for entry in entries {
        match entry {
            Ok(entry) => paths.push(entry.path()),
            Err(error) => errors.push(format!("read {}: {error}", source.display())),
        }
    }
    paths.sort();
    for path in paths {
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let result = fs::read_to_string(&path)
            .map_err(|error| error.to_string())
            .and_then(|contents| {
                serde_json::from_str::<ModelLayout>(&contents).map_err(|error| error.to_string())
            })
            .and_then(|layout| {
                layout.validate()?;
                let filename = path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("");
                let expected = format!("{}.json", layout.id);
                if filename != expected {
                    return Err(format!(
                        "filename {filename} must match model id {}",
                        layout.id
                    ));
                }
                save(destination, &layout)
            });
        if let Err(error) = result {
            errors.push(format!("sync {}: {error}", path.display()));
        }
    }
    errors
}

pub fn save(directory: &Path, layout: &ModelLayout) -> Result<(), String> {
    layout.validate()?;
    fs::create_dir_all(directory)
        .map_err(|error| format!("create {}: {error}", directory.display()))?;
    let contents =
        serde_json::to_vec_pretty(layout).map_err(|error| format!("serialize model: {error}"))?;
    crate::storage::atomic_write(&directory.join(format!("{}.json", layout.id)), &contents)
}

pub fn seed_default(directory: &Path) -> Result<(), String> {
    fs::create_dir_all(directory)
        .map_err(|error| format!("create {}: {error}", directory.display()))?;
    let path = directory.join("red-phone-v1.json");
    if !path.exists() {
        crate::storage::atomic_write(&path, include_bytes!("../../models/red-phone-v1.json"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
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
                "kivo-model-{}-{}-{}",
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
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn default_red_phone_has_one_back_out_button() {
        let model: ModelLayout =
            serde_json::from_str(include_str!("../../models/red-phone-v1.json")).unwrap();
        let ids = model
            .groups
            .iter()
            .flat_map(|group| &group.buttons)
            .map(|button| button.id.as_str())
            .collect::<Vec<_>>();
        assert!(ids.contains(&"BACK_OUT"));
        assert!(!ids.contains(&"BACK"));
        assert!(!ids.contains(&"OUT"));
    }

    #[test]
    fn rejects_duplicate_button_ids() {
        let model = ModelLayout {
            id: "test".into(),
            name: "Test".into(),
            groups: vec![ButtonGroup {
                id: "row".into(),
                columns: 2,
                buttons: vec![
                    ButtonDefinition {
                        id: "A".into(),
                        label: "A".into(),
                    },
                    ButtonDefinition {
                        id: "A".into(),
                        label: "Again".into(),
                    },
                ],
            }],
        };
        assert!(model.validate().unwrap_err().contains("duplicate button A"));
    }

    #[test]
    fn rejects_separator_bearing_model_ids() {
        let mut model: ModelLayout =
            serde_json::from_str(include_str!("../../models/red-phone-v1.json")).unwrap();

        for id in ["../escape", "/tmp/escape", r"folder\escape"] {
            model.id = id.into();
            assert!(model.validate().is_err(), "{id} must be rejected");
        }
    }

    #[test]
    fn load_all_keeps_valid_layouts_and_reports_invalid_files() {
        let directory = TestDirectory::new();
        fs::write(
            directory.0.join("z.json"),
            r#"{"id":"z","name":"Zulu","groups":[{"id":"g","columns":1,"buttons":[{"id":"Z","label":"Z"}]}]}"#,
        )
        .unwrap();
        fs::write(
            directory.0.join("a.json"),
            r#"{"id":"a","name":"Alpha","groups":[{"id":"g","columns":1,"buttons":[{"id":"A","label":"A"}]}]}"#,
        )
        .unwrap();
        fs::write(directory.0.join("invalid.json"), "not JSON").unwrap();

        let (layouts, errors) = load_all(&directory.0);

        assert_eq!(
            layouts
                .iter()
                .map(|layout| &layout.name)
                .collect::<Vec<_>>(),
            ["Alpha", "Zulu"]
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("invalid.json"));
    }

    #[test]
    fn save_validates_before_writing_pretty_json() {
        let directory = TestDirectory::new();
        let layout = ModelLayout {
            id: "test-layout".into(),
            name: "Test Layout".into(),
            groups: vec![ButtonGroup {
                id: "row".into(),
                columns: 1,
                buttons: vec![ButtonDefinition {
                    id: "A".into(),
                    label: "A".into(),
                }],
            }],
        };

        save(&directory.0, &layout).unwrap();

        let saved = fs::read_to_string(directory.0.join("test-layout.json")).unwrap();
        assert!(saved.contains("\n  \"id\": \"test-layout\""));
        assert_eq!(serde_json::from_str::<ModelLayout>(&saved).unwrap(), layout);
    }

    #[test]
    fn seed_default_only_writes_when_absent() {
        let directory = TestDirectory::new();
        let path = directory.0.join("red-phone-v1.json");

        seed_default(&directory.0).unwrap();
        let default = fs::read_to_string(&path).unwrap();
        fs::write(&path, "preserve this").unwrap();
        seed_default(&directory.0).unwrap();

        assert!(default.contains("BACK_OUT"));
        assert_eq!(fs::read_to_string(path).unwrap(), "preserve this");
    }

    #[test]
    fn bundled_models_overwrite_matching_files_and_preserve_runtime_only_models() {
        let directory = TestDirectory::new();
        let bundled = directory.0.join("bundled");
        let runtime = directory.0.join("runtime");
        fs::create_dir_all(&bundled).unwrap();
        fs::create_dir_all(&runtime).unwrap();
        fs::write(
            bundled.join("phone.json"),
            r#"{"id":"phone","name":"Packaged","groups":[{"id":"g","columns":1,"buttons":[{"id":"A","label":"New"}]}]}"#,
        )
        .unwrap();
        fs::write(
            runtime.join("phone.json"),
            r#"{"id":"phone","name":"User edit","groups":[{"id":"g","columns":1,"buttons":[{"id":"A","label":"Old"}]}]}"#,
        )
        .unwrap();
        fs::write(
            runtime.join("custom.json"),
            r#"{"id":"custom","name":"Custom","groups":[{"id":"g","columns":1,"buttons":[{"id":"C","label":"C"}]}]}"#,
        )
        .unwrap();

        let errors = sync_bundled(&bundled, &runtime);

        assert!(errors.is_empty());
        let packaged: ModelLayout =
            serde_json::from_slice(&fs::read(runtime.join("phone.json")).unwrap()).unwrap();
        assert_eq!(packaged.name, "Packaged");
        assert!(runtime.join("custom.json").exists());
    }

    #[test]
    fn invalid_bundled_model_does_not_replace_runtime_and_other_models_continue() {
        let directory = TestDirectory::new();
        let bundled = directory.0.join("bundled");
        let runtime = directory.0.join("runtime");
        fs::create_dir_all(&bundled).unwrap();
        fs::create_dir_all(&runtime).unwrap();
        let existing = r#"{"id":"broken","name":"Existing","groups":[{"id":"g","columns":1,"buttons":[{"id":"A","label":"A"}]}]}"#;
        fs::write(runtime.join("broken.json"), existing).unwrap();
        fs::write(bundled.join("broken.json"), "not JSON").unwrap();
        fs::write(
            bundled.join("valid.json"),
            r#"{"id":"valid","name":"Valid","groups":[{"id":"g","columns":1,"buttons":[{"id":"V","label":"V"}]}]}"#,
        )
        .unwrap();

        let errors = sync_bundled(&bundled, &runtime);

        assert_eq!(errors.len(), 1);
        assert_eq!(
            fs::read_to_string(runtime.join("broken.json")).unwrap(),
            existing
        );
        assert!(runtime.join("valid.json").exists());
    }

    #[test]
    fn bundled_model_filename_must_match_its_id() {
        let directory = TestDirectory::new();
        let bundled = directory.0.join("bundled");
        let runtime = directory.0.join("runtime");
        fs::create_dir_all(&bundled).unwrap();
        fs::write(
            bundled.join("wrong.json"),
            r#"{"id":"actual","name":"Actual","groups":[{"id":"g","columns":1,"buttons":[{"id":"A","label":"A"}]}]}"#,
        )
        .unwrap();

        let errors = sync_bundled(&bundled, &runtime);

        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("filename wrong.json must match model id actual"));
        assert!(!runtime.join("actual.json").exists());
    }

    #[test]
    fn missing_bundled_model_directory_is_a_noop() {
        let directory = TestDirectory::new();

        assert!(
            sync_bundled(&directory.0.join("missing"), &directory.0.join("runtime")).is_empty()
        );
    }
}
