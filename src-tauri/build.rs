fn main() {
    tauri_build::build();

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS is not set");
    let target_env =
        std::env::var("CARGO_CFG_TARGET_ENV").expect("CARGO_CFG_TARGET_ENV is not set");

    if target_os == "windows" && target_env == "msvc" {
        let manifest = std::path::PathBuf::from(
            std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"),
        )
        .join("windows-test-manifest.xml");
        let manifest = manifest
            .to_str()
            .expect("Windows test manifest path is not valid UTF-8");

        println!("cargo:rerun-if-changed={manifest}");
        println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-tests=/MANIFESTINPUT:{manifest}");
    }
}
