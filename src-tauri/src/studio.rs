use crate::{
    hardware::BOARD_PROFILES,
    product::{NormalizedProductDefinition, ProductDefinition},
    product_build::{ProductBuildOutput, build_product_cancellable, product_path},
    storage::atomic_write,
    workspace::AppError,
};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use tauri::Manager;

pub(super) struct StudioState {
    repo_root: RwLock<Option<PathBuf>>,
    settings_path: PathBuf,
    build_active: Arc<AtomicBool>,
    build_cancelled: Arc<AtomicBool>,
    closing: Arc<AtomicBool>,
}

#[derive(Debug, Deserialize, Serialize)]
struct StudioSettings {
    repository_root: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StudioProductSummary {
    product_version_id: String,
    display_name: String,
    board_profile_id: String,
    sha256: Option<String>,
    error: Option<AppError>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StudioBoardSummary {
    id: String,
    family_id: String,
    controller_token: String,
    display_name: String,
    safe_pins: Vec<u8>,
    supports_oled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StudioSnapshot {
    products: Vec<StudioProductSummary>,
    boards: Vec<StudioBoardSummary>,
    repo_root: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StudioBuildResult {
    output: ProductBuildOutput,
    logs: Vec<String>,
}

struct BuildGuard(Arc<AtomicBool>);

impl Drop for BuildGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn validate_repository_root(path: &Path) -> Result<PathBuf, AppError> {
    let canonical = path.canonicalize().map_err(|error| {
        AppError::new("invalid_studio_repository").with_detail(error.to_string())
    })?;
    if !canonical.join("src-tauri/Cargo.toml").is_file()
        || !canonical.join("platformio.ini").is_file()
    {
        return Err(AppError::new("invalid_studio_repository"));
    }
    Ok(canonical)
}

fn saved_repository_root(settings_path: &Path) -> Option<PathBuf> {
    let settings = fs::read(settings_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<StudioSettings>(&bytes).ok())?;
    validate_repository_root(&settings.repository_root).ok()
}

fn configured_repository_root(settings_path: &Path) -> Option<PathBuf> {
    env::var_os("KIVO_REPOSITORY_ROOT")
        .map(PathBuf::from)
        .and_then(|path| validate_repository_root(&path).ok())
        .or_else(|| saved_repository_root(settings_path))
}

impl StudioState {
    fn repository_root(&self) -> Result<PathBuf, AppError> {
        self.repo_root
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .ok_or_else(|| AppError::new("studio_repository_not_configured"))
    }
}

fn list_products(repo_root: &Path) -> Result<Vec<StudioProductSummary>, AppError> {
    let products_root = repo_root.join("products");
    fs::create_dir_all(&products_root).map_err(|error| {
        AppError::new("create_products_directory_failed").with_detail(error.to_string())
    })?;
    let mut products = Vec::new();
    for entry in fs::read_dir(&products_root).map_err(|error| {
        AppError::new("read_products_directory_failed").with_detail(error.to_string())
    })? {
        let entry = entry.map_err(|error| {
            AppError::new("read_products_directory_failed").with_detail(error.to_string())
        })?;
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let path = entry.path().join("product.yaml");
        if !path.is_file() {
            continue;
        }
        match ProductDefinition::load(&path).and_then(|definition| {
            if definition.product.product_version_id != id {
                return Err(AppError::new("product_directory_id_mismatch"));
            }
            let normalized = definition.normalize()?;
            Ok((definition, normalized))
        }) {
            Ok((definition, normalized)) => products.push(StudioProductSummary {
                product_version_id: id,
                display_name: definition.product.display_name,
                board_profile_id: definition.hardware_profile.board_profile_id,
                sha256: Some(normalized.sha256),
                error: None,
            }),
            Err(error) => products.push(StudioProductSummary {
                product_version_id: id.clone(),
                display_name: id,
                board_profile_id: String::new(),
                sha256: None,
                error: Some(error),
            }),
        }
    }
    products.sort_by(|left, right| left.product_version_id.cmp(&right.product_version_id));
    Ok(products)
}

fn snapshot(repo_root: &Path) -> Result<StudioSnapshot, AppError> {
    Ok(StudioSnapshot {
        products: list_products(repo_root)?,
        boards: BOARD_PROFILES
            .iter()
            .map(|board| StudioBoardSummary {
                id: board.id.into(),
                family_id: board.family_id.into(),
                controller_token: crate::hardware::product_id_token_for_board(board.id)
                    .expect("registered boards have a controller product token")
                    .into(),
                display_name: board.display_name.into(),
                safe_pins: board.safe_pins.to_vec(),
                supports_oled: board.supports_oled,
            })
            .collect(),
        repo_root: repo_root.to_path_buf(),
    })
}

#[tauri::command]
pub(super) fn studio_get_snapshot(
    state: tauri::State<'_, StudioState>,
) -> Result<StudioSnapshot, AppError> {
    let repo_root = state.repository_root()?;
    snapshot(&repo_root)
}

#[tauri::command]
pub(super) fn studio_load_product(
    state: tauri::State<'_, StudioState>,
    product_version_id: String,
) -> Result<ProductDefinition, AppError> {
    let repo_root = state.repository_root()?;
    let definition = ProductDefinition::load(&product_path(&repo_root, &product_version_id)?)?;
    (definition.product.product_version_id == product_version_id)
        .then_some(definition)
        .ok_or_else(|| AppError::new("product_directory_id_mismatch"))
}

#[tauri::command]
pub(super) fn studio_validate_product(
    definition: ProductDefinition,
) -> Result<NormalizedProductDefinition, AppError> {
    definition.normalize()
}

#[tauri::command]
pub(super) fn studio_save_product(
    state: tauri::State<'_, StudioState>,
    definition: ProductDefinition,
    create: bool,
) -> Result<StudioSnapshot, AppError> {
    let repo_root = state.repository_root()?;
    save_product(&repo_root, &definition, create)?;
    snapshot(&repo_root)
}

fn save_product(
    repo_root: &Path,
    definition: &ProductDefinition,
    create: bool,
) -> Result<(), AppError> {
    definition.validate()?;
    let id = &definition.product.product_version_id;
    let path = product_path(repo_root, id)?;
    if create == path.exists() {
        return Err(if create {
            AppError::new("product_already_exists").with_param("productVersionId", id)
        } else {
            AppError::new("product_not_found").with_param("productVersionId", id)
        });
    }
    let directory = path.parent().expect("product path has a directory");
    fs::create_dir_all(directory).map_err(|error| {
        AppError::new("create_product_directory_failed").with_detail(error.to_string())
    })?;
    definition.save_yaml(&path)
}

#[tauri::command]
pub(super) fn studio_copy_product(
    state: tauri::State<'_, StudioState>,
    source_product_version_id: String,
    definition: ProductDefinition,
) -> Result<StudioSnapshot, AppError> {
    let repo_root = state.repository_root()?;
    copy_product(&repo_root, &source_product_version_id, &definition)?;
    snapshot(&repo_root)
}

fn copy_product(
    repo_root: &Path,
    source_product_version_id: &str,
    definition: &ProductDefinition,
) -> Result<(), AppError> {
    let source = product_path(repo_root, source_product_version_id)?;
    if !source.is_file() {
        return Err(AppError::new("source_product_not_found"));
    }
    save_product(repo_root, definition, true)
}

#[tauri::command]
pub(super) fn studio_delete_product(
    state: tauri::State<'_, StudioState>,
    product_version_id: String,
) -> Result<StudioSnapshot, AppError> {
    let repo_root = state.repository_root()?;
    delete_product(&repo_root, &product_version_id)?;
    snapshot(&repo_root)
}

fn delete_product(repo_root: &Path, product_version_id: &str) -> Result<(), AppError> {
    let path = product_path(repo_root, product_version_id)?;
    if !path.is_file() {
        return Err(AppError::new("product_not_found"));
    }
    let directory = path.parent().expect("product path has a directory");
    fs::remove_dir_all(directory).map_err(|error| {
        AppError::new("delete_product_directory_failed").with_detail(error.to_string())
    })
}

#[tauri::command]
pub(super) async fn studio_build_product(
    state: tauri::State<'_, StudioState>,
    product_version_id: String,
) -> Result<StudioBuildResult, AppError> {
    if state
        .build_active
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(AppError::new("product_build_busy"));
    }
    let _guard = BuildGuard(Arc::clone(&state.build_active));
    state.build_cancelled.store(false, Ordering::Release);
    let repo_root = state.repository_root()?;
    let cancelled = Arc::clone(&state.build_cancelled);
    let build_id = env::var("KIVO_FIRMWARE_BUILD_ID").unwrap_or_else(|_| "dev".into());
    tauri::async_runtime::spawn_blocking(move || {
        let logs = Mutex::new(Vec::new());
        let output = build_product_cancellable(
            &repo_root,
            &product_version_id,
            &build_id,
            &cancelled,
            |line| logs.lock().unwrap().push(line.to_owned()),
        )?;
        Ok(StudioBuildResult {
            output,
            logs: logs.into_inner().unwrap(),
        })
    })
    .await
    .map_err(|error| AppError::new("product_build_task_failed").with_detail(error.to_string()))?
}

#[tauri::command]
pub(super) fn studio_select_repository(
    state: tauri::State<'_, StudioState>,
    repository_root: String,
) -> Result<StudioSnapshot, AppError> {
    let repo_root = validate_repository_root(Path::new(&repository_root))?;
    let settings = serde_json::to_vec_pretty(&StudioSettings {
        repository_root: repo_root.clone(),
    })
    .map_err(|error| AppError::new("save_studio_settings_failed").with_detail(error.to_string()))?;
    let settings_directory = state
        .settings_path
        .parent()
        .ok_or_else(|| AppError::new("save_studio_settings_failed"))?;
    fs::create_dir_all(settings_directory).map_err(|error| {
        AppError::new("save_studio_settings_failed").with_detail(error.to_string())
    })?;
    atomic_write(&state.settings_path, &settings)
        .map_err(|error| AppError::new("save_studio_settings_failed").with_detail(error))?;
    *state
        .repo_root
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(repo_root.clone());
    snapshot(&repo_root)
}

pub(super) fn setup(app: &mut tauri::App) -> Result<Option<PathBuf>, AppError> {
    let settings_path = app
        .path()
        .app_config_dir()
        .map_err(|error| {
            AppError::new("resolve_studio_settings_directory_failed").with_detail(error.to_string())
        })?
        .join("studio-settings.json");
    let repo_root = configured_repository_root(&settings_path);
    app.manage(StudioState {
        repo_root: RwLock::new(repo_root.clone()),
        settings_path,
        build_active: Arc::new(AtomicBool::new(false)),
        build_cancelled: Arc::new(AtomicBool::new(false)),
        closing: Arc::new(AtomicBool::new(false)),
    });
    Ok(repo_root)
}

pub(super) fn cancel_active_build_for_shutdown(app: &tauri::AppHandle) -> bool {
    if !app
        .state::<StudioState>()
        .build_active
        .load(Ordering::Acquire)
    {
        return false;
    }
    begin_cancelled_shutdown(app);
    true
}

fn begin_cancelled_shutdown(app: &tauri::AppHandle) {
    let state = app.state::<StudioState>();
    state.build_cancelled.store(true, Ordering::Release);
    if state
        .closing
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    let active = Arc::clone(&state.build_active);
    let app = app.clone();
    thread::spawn(move || {
        while active.load(Ordering::Acquire) {
            thread::sleep(Duration::from_millis(25));
        }
        app.exit(0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repository_fixture() -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir_all(directory.path().join("src-tauri")).unwrap();
        fs::write(directory.path().join("src-tauri/Cargo.toml"), "[package]").unwrap();
        fs::write(directory.path().join("platformio.ini"), "[platformio]").unwrap();
        directory
    }

    #[test]
    fn repository_validation_requires_kivo_build_files() {
        let invalid = tempfile::tempdir().unwrap();
        assert_eq!(
            validate_repository_root(invalid.path()).unwrap_err().code,
            "invalid_studio_repository"
        );

        let valid = repository_fixture();
        assert_eq!(
            validate_repository_root(valid.path()).unwrap(),
            valid.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn saved_repository_is_loaded_only_while_it_remains_valid() {
        let repository = repository_fixture();
        let settings_directory = tempfile::tempdir().unwrap();
        let settings_path = settings_directory.path().join("studio-settings.json");
        fs::write(
            &settings_path,
            serde_json::to_vec(&StudioSettings {
                repository_root: repository.path().to_path_buf(),
            })
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            saved_repository_root(&settings_path).unwrap(),
            repository.path().canonicalize().unwrap()
        );
        fs::remove_file(repository.path().join("platformio.ini")).unwrap();
        assert!(saved_repository_root(&settings_path).is_none());
    }

    fn yaml(id: &str, revision: u16, name: &str) -> String {
        format!(
            r#"schema_version: 1
product:
  display_name: {name}
  family_id: key
  variant_id: key-rp-k1
  hardware_revision: {revision}
  product_version_id: {id}
  capabilities: []
layout:
  id: key-rp-k1
  name: {name}
  groups:
    - id: keys
      columns: 1
      buttons:
        - {{ id: K1, label: K1 }}
hardware_profile:
  id: hardware
  name: Hardware
  board_profile_id: yd-rp2040
  debounce_ms: 30
  inputs:
    - type: direct
      id: direct
      keys: {{ K1: 0 }}
"#
        )
    }

    #[test]
    fn create_update_copy_and_list_stay_under_products() {
        let directory = tempfile::tempdir().unwrap();
        let first =
            ProductDefinition::parse_yaml(yaml("key-rp-k1-r01", 1, "Key One").as_bytes()).unwrap();
        save_product(directory.path(), &first, true).unwrap();
        let duplicate = save_product(directory.path(), &first, true).unwrap_err();
        assert_eq!(duplicate.code, "product_already_exists");
        assert_eq!(
            duplicate.params.get("productVersionId").map(String::as_str),
            Some("key-rp-k1-r01")
        );

        let mut updated = first.clone();
        updated.product.display_name = "Updated Key One".into();
        updated.layout.name = "Updated Key One".into();
        save_product(directory.path(), &updated, false).unwrap();

        let second =
            ProductDefinition::parse_yaml(yaml("key-rp-k1-r02", 2, "Key Two").as_bytes()).unwrap();
        copy_product(directory.path(), "key-rp-k1-r01", &second).unwrap();
        let products = list_products(directory.path()).unwrap();
        assert_eq!(products.len(), 2);
        assert_eq!(products[0].display_name, "Updated Key One");
        assert!(
            directory
                .path()
                .join("products/key-rp-k1-r02/product.yaml")
                .is_file()
        );
        assert!(!directory.path().join("product.yaml").exists());

        delete_product(directory.path(), "key-rp-k1-r02").unwrap();
        assert_eq!(list_products(directory.path()).unwrap().len(), 1);
        assert!(!directory.path().join("products/key-rp-k1-r02").exists());
        assert_eq!(
            delete_product(directory.path(), "key-rp-k1-r02")
                .unwrap_err()
                .code,
            "product_not_found"
        );
    }

    #[test]
    fn copy_requires_an_existing_source() {
        let directory = tempfile::tempdir().unwrap();
        let definition =
            ProductDefinition::parse_yaml(yaml("key-rp-k1-r01", 1, "Key One").as_bytes()).unwrap();
        assert_eq!(
            copy_product(directory.path(), "key-rp-k1-r02", &definition)
                .unwrap_err()
                .code,
            "source_product_not_found"
        );
    }
}
