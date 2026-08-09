# Windows Runtime Log Test Manifest Design

**Date:** 2026-08-09

## Goal

Allow the `runtime_log_rotation` integration-test executable to start on
Windows while preserving its existing official-plugin rotation coverage and
leaving Kivo's production executable metadata unchanged.

## Root Cause

The test uses `tauri::test::mock_builder()`, which links Windows Common
Controls v6 entry points into the integration-test executable. `tauri-build`
already supplies the required Common Controls v6 application manifest, but its
resource linker directive is scoped to normal binary targets. Cargo therefore
does not embed that manifest into `tests/runtime_log_rotation.rs`. Windows
rejects the resulting executable during loader startup with
`STATUS_ENTRYPOINT_NOT_FOUND` (`0xc0000139`), before the Rust test harness can
run.

## Design

Add a dedicated XML manifest under `src-tauri` containing the same Common
Controls v6 dependency declared by Tauri's standard Windows manifest. On an
MSVC Windows target, `build.rs` will emit Cargo's `rustc-link-arg-tests`
directives for `/MANIFEST:EMBED` and `/MANIFESTINPUT:<path>`. Cargo applies
those arguments only to integration-test targets, including
`runtime_log_rotation`.

Keep the existing `tauri_build::build()` call. Its normal application resource
generation remains authoritative for Kivo binaries, icons, version metadata,
and packaging. Do not replace the application manifest globally, post-process
executables in CI, or disable the rotation test on Windows.

The build script will register the manifest with `rerun-if-changed`. It will
read Cargo's target OS and target environment variables at runtime rather than
using the build script host's compile-time `cfg`, so the behavior remains
correct when cross-compiling. The MSVC condition prevents MSVC linker flags
from reaching GNU Windows targets.

## Failure Behavior

The manifest path is derived from `CARGO_MANIFEST_DIR`, avoiding dependence on
the caller's working directory. Missing Cargo target variables or a non-UTF-8
manifest path will fail the build with a direct diagnostic instead of silently
producing another unloadable test executable.

## Testing

The existing Windows CI failure is the red regression case: the test executable
currently exits with `0xc0000139` before reporting its test count. After the
change, Windows CI must start and pass `runtime_log_rotation` and complete the
full Rust test suite.

Locally, run the targeted integration test, the complete Rust suite, formatting,
and Clippy on the host platform. Inspect verbose Cargo build output for an MSVC
Windows test target to confirm the manifest arguments are scoped to test
artifacts. Production packaging remains covered by the existing Windows NSIS
build in the same workflow.
