use crate::{
    hardware::BOARD_PROFILES,
    product::{NormalizedProductDefinition, ProductDefinition},
    product_build::{ProductBuildOutput, build_product_cancellable, product_path},
    workspace::AppError,
};
use serde::Serialize;
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use tauri::Manager;

struct StudioState {
    repo_root: PathBuf,
    build_active: Arc<AtomicBool>,
    build_cancelled: Arc<AtomicBool>,
    closing: Arc<AtomicBool>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct StudioProductSummary {
    product_version_id: String,
    display_name: String,
    board_profile_id: String,
    sha256: Option<String>,
    error: Option<AppError>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct StudioBoardSummary {
    id: String,
    family_id: String,
    display_name: String,
    safe_pins: Vec<u8>,
    supports_oled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct StudioSnapshot {
    products: Vec<StudioProductSummary>,
    boards: Vec<StudioBoardSummary>,
    repo_root: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct StudioBuildResult {
    output: ProductBuildOutput,
    logs: Vec<String>,
}

struct BuildGuard(Arc<AtomicBool>);

impl Drop for BuildGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
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

fn snapshot(state: &StudioState) -> Result<StudioSnapshot, AppError> {
    Ok(StudioSnapshot {
        products: list_products(&state.repo_root)?,
        boards: BOARD_PROFILES
            .iter()
            .map(|board| StudioBoardSummary {
                id: board.id.into(),
                family_id: board.family_id.into(),
                display_name: board.display_name.into(),
                safe_pins: board.safe_pins.to_vec(),
                supports_oled: board.supports_oled,
            })
            .collect(),
        repo_root: state.repo_root.clone(),
    })
}

#[tauri::command]
fn studio_get_snapshot(state: tauri::State<'_, StudioState>) -> Result<StudioSnapshot, AppError> {
    snapshot(&state)
}

#[tauri::command]
fn studio_load_product(
    state: tauri::State<'_, StudioState>,
    product_version_id: String,
) -> Result<ProductDefinition, AppError> {
    let definition =
        ProductDefinition::load(&product_path(&state.repo_root, &product_version_id)?)?;
    (definition.product.product_version_id == product_version_id)
        .then_some(definition)
        .ok_or_else(|| AppError::new("product_directory_id_mismatch"))
}

#[tauri::command]
fn studio_validate_product(
    definition: ProductDefinition,
) -> Result<NormalizedProductDefinition, AppError> {
    definition.normalize()
}

#[tauri::command]
fn studio_save_product(
    state: tauri::State<'_, StudioState>,
    definition: ProductDefinition,
    create: bool,
) -> Result<StudioSnapshot, AppError> {
    save_product(&state.repo_root, &definition, create)?;
    snapshot(&state)
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
        return Err(AppError::new(if create {
            "product_already_exists"
        } else {
            "product_not_found"
        }));
    }
    let directory = path.parent().expect("product path has a directory");
    fs::create_dir_all(directory).map_err(|error| {
        AppError::new("create_product_directory_failed").with_detail(error.to_string())
    })?;
    definition.save_yaml(&path)
}

#[tauri::command]
fn studio_copy_product(
    state: tauri::State<'_, StudioState>,
    source_product_version_id: String,
    definition: ProductDefinition,
) -> Result<StudioSnapshot, AppError> {
    copy_product(&state.repo_root, &source_product_version_id, &definition)?;
    snapshot(&state)
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
async fn studio_build_product(
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
    let repo_root = state.repo_root.clone();
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

pub fn run() {
    let repo_root = env::var_os("KIVO_REPOSITORY_ROOT")
        .map(PathBuf::from)
        .and_then(|path| path.canonicalize().ok())
        .filter(|path| path.join("src-tauri/Cargo.toml").is_file())
        .expect("KIVO_REPOSITORY_ROOT must point to the Kivo repository");
    tauri::Builder::default()
        .setup(move |app| {
            app.manage(StudioState {
                repo_root,
                build_active: Arc::new(AtomicBool::new(false)),
                build_cancelled: Arc::new(AtomicBool::new(false)),
                closing: Arc::new(AtomicBool::new(false)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            studio_get_snapshot,
            studio_load_product,
            studio_validate_product,
            studio_save_product,
            studio_copy_product,
            studio_build_product,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Kivo Product Studio")
        .run(|app, event| match event {
            tauri::RunEvent::WindowEvent {
                event: tauri::WindowEvent::CloseRequested { api, .. },
                ..
            } if app
                .state::<StudioState>()
                .build_active
                .load(Ordering::Acquire) =>
            {
                api.prevent_close();
                begin_cancelled_shutdown(app);
            }
            tauri::RunEvent::ExitRequested { api, .. }
                if app
                    .state::<StudioState>()
                    .build_active
                    .load(Ordering::Acquire) =>
            {
                api.prevent_exit();
                begin_cancelled_shutdown(app);
            }
            _ => {}
        });
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

    fn yaml(id: &str, revision: u16, name: &str) -> String {
        format!(
            r#"schema_version: 1
product:
  display_name: {name}
  family_id: key
  variant_id: key-k1
  hardware_revision: {revision}
  product_version_id: {id}
  capabilities: []
layout:
  id: key-k1
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
            ProductDefinition::parse_yaml(yaml("key-k1-r01", 1, "Key One").as_bytes()).unwrap();
        save_product(directory.path(), &first, true).unwrap();
        assert_eq!(
            save_product(directory.path(), &first, true)
                .unwrap_err()
                .code,
            "product_already_exists"
        );

        let mut updated = first.clone();
        updated.product.display_name = "Updated Key One".into();
        updated.layout.name = "Updated Key One".into();
        save_product(directory.path(), &updated, false).unwrap();

        let second =
            ProductDefinition::parse_yaml(yaml("key-k1-r02", 2, "Key Two").as_bytes()).unwrap();
        copy_product(directory.path(), "key-k1-r01", &second).unwrap();
        let products = list_products(directory.path()).unwrap();
        assert_eq!(products.len(), 2);
        assert_eq!(products[0].display_name, "Updated Key One");
        assert!(
            directory
                .path()
                .join("products/key-k1-r02/product.yaml")
                .is_file()
        );
        assert!(!directory.path().join("product.yaml").exists());
    }

    #[test]
    fn copy_requires_an_existing_source() {
        let directory = tempfile::tempdir().unwrap();
        let definition =
            ProductDefinition::parse_yaml(yaml("key-k1-r01", 1, "Key One").as_bytes()).unwrap();
        assert_eq!(
            copy_product(directory.path(), "key-k1-r02", &definition)
                .unwrap_err()
                .code,
            "source_product_not_found"
        );
    }
}
