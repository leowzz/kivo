use crate::{
    product::{ProductDefinition, generated_header, sha256_hex},
    storage::atomic_write,
    workspace::AppError,
};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

pub const PRODUCT_PROTOCOL_VERSION: u16 = 9;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProductManifest {
    pub product_version_id: String,
    pub protocol_version: u16,
    pub product_definition_sha256: String,
    pub product_definition_bytes: usize,
    pub board_profile_id: String,
    pub git_commit: String,
    pub build_id: String,
    pub firmware_file: String,
    pub firmware_size: u64,
    pub firmware_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductBuildOutput {
    pub output_directory: PathBuf,
    pub firmware_path: PathBuf,
    pub definition_path: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest: ProductManifest,
}

pub fn product_path(repo_root: &Path, product_version_id: &str) -> Result<PathBuf, AppError> {
    if !crate::product::valid_product_version_id(product_version_id) {
        return Err(AppError::new("invalid_product_version_id"));
    }
    Ok(repo_root
        .join("products")
        .join(product_version_id)
        .join("product.yaml"))
}

pub fn build_product(
    repo_root: &Path,
    product_version_id: &str,
    build_id: &str,
    log: impl FnMut(&str),
) -> Result<ProductBuildOutput, AppError> {
    build_product_cancellable(
        repo_root,
        product_version_id,
        build_id,
        &AtomicBool::new(false),
        log,
    )
}

pub fn build_product_cancellable(
    repo_root: &Path,
    product_version_id: &str,
    build_id: &str,
    cancelled: &AtomicBool,
    mut log: impl FnMut(&str),
) -> Result<ProductBuildOutput, AppError> {
    validate_build_id(build_id)?;
    if cancelled.load(Ordering::Acquire) {
        return Err(AppError::new("product_build_cancelled"));
    }
    let definition = ProductDefinition::load(&product_path(repo_root, product_version_id)?)?;
    if definition.product.product_version_id != product_version_id {
        return Err(AppError::new("product_directory_id_mismatch"));
    }
    let normalized = definition.normalize()?;
    let board = crate::hardware::board_by_id(&definition.hardware_profile.board_profile_id)
        .ok_or_else(|| AppError::new("unknown_board_profile"))?;
    let isolated_root = repo_root.join(".pio/product").join(&normalized.sha256);
    let generated_directory = isolated_root.join("generated");
    fs::create_dir_all(&generated_directory).map_err(|error| {
        AppError::new("create_product_build_directory_failed").with_detail(error.to_string())
    })?;
    let header_path = generated_directory.join("KivoProductGenerated.h");
    let header = generated_header(&normalized)?;
    atomic_write(&header_path, header.as_bytes())
        .map_err(|error| AppError::new("write_product_header_failed").with_detail(error))?;

    log(&format!(
        "Validated {} ({}, {} bytes)",
        product_version_id, normalized.sha256, normalized.byte_length
    ));
    log(&format!(
        "Building PlatformIO environment {}",
        board.firmware_environment
    ));
    let mut child = Command::new("uv")
        .current_dir(repo_root)
        .args(["run", "pio", "run", "-e", board.firmware_environment])
        .env("KIVO_FIRMWARE_BUILD_ID", build_id)
        .env("KIVO_PRODUCT_GENERATED_DIR", &generated_directory)
        .env("PLATFORMIO_BUILD_DIR", isolated_root.join("build"))
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| {
            AppError::new("product_firmware_build_failed").with_detail(error.to_string())
        })?;
    let status = loop {
        if cancelled.load(Ordering::Acquire) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AppError::new("product_build_cancelled"));
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(
                    AppError::new("product_firmware_build_failed").with_detail(error.to_string())
                );
            }
        }
    };
    if !status.success() {
        return Err(AppError::new("product_firmware_build_failed")
            .with_detail(format!("PlatformIO exited with {status}")));
    }

    let environment_build = isolated_root.join("build").join(board.firmware_environment);
    let firmware_source = firmware_artifact(&environment_build, board.firmware_environment)?;
    let output_directory = repo_root
        .join("output/products")
        .join(product_version_id)
        .join(build_id);
    fs::create_dir_all(&output_directory).map_err(|error| {
        AppError::new("create_product_output_directory_failed").with_detail(error.to_string())
    })?;
    let firmware_name = firmware_source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::new("invalid_firmware_artifact_name"))?;
    let firmware_path = output_directory.join(firmware_name);
    fs::copy(&firmware_source, &firmware_path).map_err(|error| {
        AppError::new("copy_product_firmware_failed").with_detail(error.to_string())
    })?;
    let definition_path = output_directory.join("product.json");
    atomic_write(&definition_path, normalized.json.as_bytes())
        .map_err(|error| AppError::new("write_normalized_product_failed").with_detail(error))?;
    let firmware = fs::read(&firmware_path).map_err(|error| {
        AppError::new("read_product_firmware_failed").with_detail(error.to_string())
    })?;
    let manifest = ProductManifest {
        product_version_id: product_version_id.into(),
        protocol_version: PRODUCT_PROTOCOL_VERSION,
        product_definition_sha256: normalized.sha256,
        product_definition_bytes: normalized.byte_length,
        board_profile_id: board.id.into(),
        git_commit: git_commit(repo_root),
        build_id: build_id.into(),
        firmware_file: firmware_name.into(),
        firmware_size: firmware.len() as u64,
        firmware_sha256: sha256_hex(&firmware),
    };
    let manifest_path = output_directory.join("manifest.json");
    let manifest_json = serde_json::to_vec_pretty(&manifest).map_err(|error| {
        AppError::new("serialize_product_manifest_failed").with_detail(error.to_string())
    })?;
    atomic_write(&manifest_path, &manifest_json)
        .map_err(|error| AppError::new("write_product_manifest_failed").with_detail(error))?;
    log(&format!("Published {}", output_directory.display()));
    Ok(ProductBuildOutput {
        output_directory,
        firmware_path,
        definition_path,
        manifest_path,
        manifest,
    })
}

fn validate_build_id(build_id: &str) -> Result<(), AppError> {
    (!build_id.is_empty()
        && build_id.len() <= 128
        && build_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')))
    .then_some(())
    .ok_or_else(|| AppError::new("invalid_build_id"))
}

fn firmware_artifact(build: &Path, environment: &str) -> Result<PathBuf, AppError> {
    let candidates = if environment == "rp2040" {
        vec![build.join("firmware.uf2")]
    } else {
        vec![
            build.join("firmware.factory.bin"),
            build.join("firmware.bin"),
        ]
    };
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| AppError::new("product_firmware_artifact_missing"))
}

fn git_commit(repo_root: &Path) -> String {
    Command::new("git")
        .current_dir(repo_root)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_paths_reject_traversal() {
        let root = Path::new("/repo");
        assert!(product_path(root, "../outside-r01").is_err());
        assert_eq!(
            product_path(root, "key-k1-r01").unwrap(),
            root.join("products/key-k1-r01/product.yaml")
        );
    }

    #[test]
    fn build_ids_are_safe_path_components() {
        assert!(validate_build_id("v1.2.3-dev_4").is_ok());
        assert!(validate_build_id("../escape").is_err());
        assert!(validate_build_id("has space").is_err());
    }

    #[test]
    fn cancelled_build_stops_before_reading_or_spawning() {
        let cancelled = AtomicBool::new(true);
        assert_eq!(
            build_product_cancellable(
                Path::new("/missing/repository"),
                "key-k1-r01",
                "test",
                &cancelled,
                |_| {},
            )
            .unwrap_err()
            .code,
            "product_build_cancelled"
        );
    }
}
