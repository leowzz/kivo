use std::{env, error::Error, fs, path::Path, process::Command};
use tauri_plugin_log::{
    Builder, RotationStrategy, Target, TargetKind,
    log::{LevelFilter, info},
};

const CHILD_DIRECTORY_ENV: &str = "KIVO_RUNTIME_LOG_ROTATION_CHILD_DIRECTORY";
const REPORT_FILES_ENV: &str = "KIVO_RUNTIME_LOG_ROTATION_REPORT_FILES";
const LOG_TARGET: &str = "kivo::runtime";
const PRE_ROTATION_SENTINEL: &str = "pre_rotation_sentinel";
const POST_ROTATION_SENTINEL: &str = "post_rotation_sentinel";

#[test]
fn official_plugin_bounds_rotated_runtime_logs() -> Result<(), Box<dyn Error>> {
    if let Some(directory) = env::var_os(CHILD_DIRECTORY_ENV) {
        return write_rotating_logs(Path::new(&directory));
    }

    let directory = tempfile::tempdir()?;
    for second in 0..5 {
        fs::write(
            directory
                .path()
                .join(format!("kivo_2020-01-01_00-00-0{second}.log")),
            format!("historical seed {second}\n"),
        )?;
    }

    let output = Command::new(env::current_exe()?)
        .env(CHILD_DIRECTORY_ENV, directory.path())
        .args([
            "--exact",
            "official_plugin_bounds_rotated_runtime_logs",
            "--nocapture",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "rotation child failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let mut all_names = Vec::new();
    for entry in fs::read_dir(directory.path())? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            all_names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    all_names.sort();
    let log_names = all_names
        .iter()
        .filter(|name| {
            name.as_str() == "kivo.log" || (name.starts_with("kivo_") && name.ends_with(".log"))
        })
        .cloned()
        .collect::<Vec<_>>();

    assert_eq!(
        log_names.len(),
        4,
        "KeepSome(3) must leave three archives plus active; observed {all_names:?}"
    );
    assert!(
        !all_names.iter().any(|name| name.ends_with(".bak")),
        "one normal rotation must not create .bak files: {all_names:?}"
    );

    let archive_names = log_names
        .iter()
        .filter(|name| name.as_str() != "kivo.log")
        .collect::<Vec<_>>();
    assert_eq!(archive_names.len(), 3);
    let mut sentinel_archives = Vec::new();
    for name in archive_names {
        let contents = fs::read_to_string(directory.path().join(name))?;
        if contents.contains(PRE_ROTATION_SENTINEL) {
            sentinel_archives.push(name);
        }
    }
    assert_eq!(
        sentinel_archives.len(),
        1,
        "pre-rotation sentinel must appear in one archive: {log_names:?}"
    );

    let active = fs::read_to_string(directory.path().join("kivo.log"))?;
    assert!(active.contains(POST_ROTATION_SENTINEL));
    assert!(!active.contains(PRE_ROTATION_SENTINEL));

    if env::var_os(REPORT_FILES_ENV).is_some() {
        println!("rotation files: {log_names:?}");
    }
    Ok(())
}

fn write_rotating_logs(directory: &Path) -> Result<(), Box<dyn Error>> {
    let _app = tauri::test::mock_builder()
        .plugin(
            Builder::new()
                .level(LevelFilter::Info)
                .clear_format()
                .max_file_size(256)
                .rotation_strategy(RotationStrategy::KeepSome(3))
                .targets([Target::new(TargetKind::Folder {
                    path: directory.to_path_buf(),
                    file_name: Some("kivo".into()),
                })
                .filter(|metadata| metadata.target() == LOG_TARGET)])
                .build(),
        )
        .build(tauri::test::mock_context(tauri::test::noop_assets()))?;

    let padding = "0123456789".repeat(32);
    info!(target: LOG_TARGET, "{{\"event\":\"{PRE_ROTATION_SENTINEL}\",\"padding\":\"{padding}\"}}");
    info!(target: LOG_TARGET, "{{\"event\":\"{POST_ROTATION_SENTINEL}\",\"padding\":\"{padding}\"}}");
    tauri_plugin_log::log::logger().flush();
    Ok(())
}
