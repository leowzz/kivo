use std::fs;
use tauri_plugin_log::{
    Builder, RotationStrategy, Target, TargetKind,
    log::{LevelFilter, info},
};

#[test]
fn official_plugin_bounds_rotated_runtime_logs() {
    let directory = tempfile::tempdir().unwrap();
    let app = tauri::test::mock_builder()
        .plugin(
            Builder::new()
                .level(LevelFilter::Info)
                .clear_format()
                .max_file_size(256)
                .rotation_strategy(RotationStrategy::KeepSome(3))
                .targets([Target::new(TargetKind::Folder {
                    path: directory.path().to_path_buf(),
                    file_name: Some("kivo".into()),
                })])
                .build(),
        )
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();

    for sequence in 0..100 {
        info!(target: "kivo::runtime", "{{\"timestampMs\":{sequence},\"event\":\"rotation_probe\",\"padding\":\"0123456789012345678901234567890123456789\"}}");
    }
    tauri_plugin_log::log::logger().flush();

    let files = fs::read_dir(directory.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("kivo"))
        .collect::<Vec<_>>();
    eprintln!("rotation produced {} files", files.len());
    assert!(files.len() > 1, "rotation did not produce an archived log");
    assert!(files.len() <= 3, "rotation left {} files", files.len());
    drop(app);
}
