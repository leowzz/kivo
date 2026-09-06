use super::{StudioState, gpio::GpioState};
use crate::{error::AppError, hardware::DeviceId};
use serde::Deserialize;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
};
use tauri::ipc::Channel;

#[derive(Default)]
struct OperationStatus {
    active: bool,
    closed: bool,
}

#[derive(Default)]
pub(super) struct FirmwareState(Arc<Mutex<OperationStatus>>);

struct OperationGuard(Arc<Mutex<OperationStatus>>);

impl Drop for OperationGuard {
    fn drop(&mut self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).active = false;
    }
}

impl FirmwareState {
    fn reserve(&self) -> Result<OperationGuard, AppError> {
        let mut status = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if status.active || status.closed {
            return Err(AppError::new("studio_firmware_busy"));
        }
        status.active = true;
        Ok(OperationGuard(Arc::clone(&self.0)))
    }

    pub(super) fn prevent_shutdown(&self) -> bool {
        let mut status = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if status.active {
            return true;
        }
        status.closed = true;
        false
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum FirmwareOperation {
    Inspect,
    Backup,
    Flash,
    InstallTest,
    Status,
}

impl FirmwareOperation {
    fn argument(self) -> &'static str {
        match self {
            Self::Inspect => "inspect",
            Self::Backup => "backup",
            Self::Flash => "flash",
            Self::InstallTest => "install_test",
            Self::Status => "status",
        }
    }
}

fn run_operation(
    repo: &Path,
    operation: FirmwareOperation,
    device_id: &DeviceId,
    path: Option<&Path>,
    sha256: Option<&str>,
    progress: Channel<Value>,
) -> Result<Value, AppError> {
    let mut command = Command::new("uv");
    command
        .current_dir(repo)
        .args([
            "run",
            "python",
            "-u",
            "-m",
            "scripts.studio_firmware",
            operation.argument(),
        ])
        .args([
            "--board",
            device_id.board_profile_id(),
            "--serial",
            device_id.hardware_serial(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if let Some(path) = path {
        command.arg("--path").arg(path);
    }
    if let Some(sha256) = sha256 {
        command.args(["--sha256", sha256]);
    }
    let failed = |detail: String| AppError::new("studio_firmware_failed").with_detail(detail);
    let mut child = command.spawn().map_err(|error| failed(error.to_string()))?;
    let stdout = child.stdout.take().expect("piped firmware output");
    let mut result = None;
    let mut failure = None;
    for line in BufReader::new(stdout).lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(failed(error.to_string()));
            }
        };
        let Ok(event) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        match event.get("phase").and_then(Value::as_str) {
            Some("complete") => result = Some(event.clone()),
            Some("error") => {
                failure = event
                    .get("message")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            }
            _ => {}
        }
        let _ = progress.send(event);
    }
    let status = child.wait().map_err(|error| failed(error.to_string()))?;
    if !status.success() || failure.is_some() {
        return Err(failed(
            failure.unwrap_or_else(|| format!("Firmware tool exited with {status}")),
        ));
    }
    result.ok_or_else(|| failed("Firmware tool returned no result".into()))
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(super) async fn studio_firmware_operation(
    state: tauri::State<'_, StudioState>,
    firmware: tauri::State<'_, FirmwareState>,
    gpio: tauri::State<'_, GpioState>,
    operation: FirmwareOperation,
    device_id: DeviceId,
    path: Option<PathBuf>,
    sha256: Option<String>,
    progress: Channel<Value>,
) -> Result<Value, AppError> {
    let guard = if matches!(operation, FirmwareOperation::Status) {
        None
    } else {
        Some(firmware.reserve()?)
    };
    let repo = state.repository_root()?;
    let gpio = (*gpio).clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        let run = || {
            run_operation(
                &repo,
                operation,
                &device_id,
                path.as_deref(),
                sha256.as_deref(),
                progress,
            )
        };
        if matches!(operation, FirmwareOperation::Status) {
            run()
        } else {
            gpio.exclusive_access(run)
        }
    })
    .await
    .map_err(|error| AppError::new("studio_firmware_failed").with_detail(error.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firmware_reservation_blocks_competing_operations_and_shutdown() {
        let state = FirmwareState::default();
        let first = state.reserve().unwrap();
        assert!(state.reserve().is_err());
        assert!(state.prevent_shutdown());
        drop(first);
        let retry = state.reserve().unwrap();
        drop(retry);
        assert!(!state.prevent_shutdown());
        assert!(state.reserve().is_err());
    }
}
