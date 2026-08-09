use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use tauri_plugin_log::{
    Builder, RotationStrategy, Target, TargetKind,
    log::{self, LevelFilter},
};

pub(crate) const LOG_TARGET: &str = "kivo::runtime";
pub(crate) const MAX_FILE_SIZE: u128 = 10 * 1024 * 1024;
pub(crate) const RETAINED_FILES: usize = 5;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RuntimeLogLevel {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeLogEntry {
    timestamp_ms: u64,
    level: RuntimeLogLevel,
    event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
    context: Value,
}

impl RuntimeLogEntry {
    pub(crate) fn new(
        timestamp_ms: u64,
        level: RuntimeLogLevel,
        event: impl Into<String>,
        context: Value,
    ) -> Self {
        Self {
            timestamp_ms,
            level,
            event: event.into(),
            result: None,
            detail: None,
            context,
        }
    }

    pub(crate) fn with_result(mut self, result: impl Into<String>) -> Self {
        self.result = Some(result.into());
        self
    }

    pub(crate) fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

pub(crate) fn log_directory(config_directory: &Path) -> PathBuf {
    config_directory.join("data/log")
}

pub(crate) fn serialize_entry(entry: &RuntimeLogEntry) -> serde_json::Result<String> {
    serde_json::to_string(entry)
}

pub(crate) fn emit(entry: RuntimeLogEntry) {
    let line = match serialize_entry(&entry) {
        Ok(line) => line,
        Err(error) => {
            eprintln!("failed to serialize runtime log entry: {error}");
            return;
        }
    };

    match entry.level {
        RuntimeLogLevel::Info => log::info!(target: LOG_TARGET, "{line}"),
        RuntimeLogLevel::Warning => log::warn!(target: LOG_TARGET, "{line}"),
        RuntimeLogLevel::Error => log::error!(target: LOG_TARGET, "{line}"),
    }
}

pub(crate) fn install<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    config_directory: &Path,
) -> tauri::Result<()> {
    app.plugin(
        Builder::new()
            .level(LevelFilter::Info)
            .clear_format()
            .max_file_size(MAX_FILE_SIZE)
            .rotation_strategy(RotationStrategy::KeepSome(RETAINED_FILES))
            .targets([
                Target::new(TargetKind::Folder {
                    path: log_directory(config_directory),
                    file_name: Some("kivo".into()),
                })
                .filter(|metadata| metadata.target() == LOG_TARGET),
                Target::new(TargetKind::Stderr).filter(|metadata| metadata.target() == LOG_TARGET),
            ])
            .build(),
    )
}

#[cfg(test)]
mod tests {
    use super::{RuntimeLogEntry, RuntimeLogLevel, log_directory, serialize_entry};
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn places_runtime_logs_under_the_application_data_directory() {
        assert_eq!(
            log_directory(Path::new("/tmp/kivo")),
            Path::new("/tmp/kivo/data/log")
        );
    }

    #[test]
    fn serializes_runtime_log_entries_as_single_json_lines() {
        let entry = RuntimeLogEntry::new(
            1_722_355_200_000,
            RuntimeLogLevel::Info,
            "application_started",
            json!({"version": "0.1.0"}),
        );

        let line = serialize_entry(&entry).expect("entry serializes");
        let value: serde_json::Value = serde_json::from_str(&line).expect("line is JSON");

        assert!(!line.contains('\n'));
        assert_eq!(value["timestampMs"], 1_722_355_200_000_u64);
        assert_eq!(value["level"], "info");
        assert_eq!(value["event"], "application_started");
        assert_eq!(value["context"]["version"], "0.1.0");
    }
}
