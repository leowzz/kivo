# Device Registry Extension Contract Verification

Date: 2026-07-31

Base and tested HEAD: `9ba9f3de5530eb126d88d56fbeef96cd8caf128d` plus the unstaged Task 8 working tree.

## Contract Under Test

The tests inject a borrowed `HardwareRegistry<'_>` containing the two compiled production boards, a test-only second RP2040 board, and a test-only ESP32-C3 family and board. The synthetic entries are compiled only for Rust tests and do not extend `CONTROLLER_FAMILIES` or `BOARD_PROFILES` in production builds.

The extension-flow fixture exercises USB classification, canonical Device ID creation from Board Profile ID plus non-empty hardware serial, protocol-v3 hello validation, durable enrollment, board-compatible assignment, immutable runtime snapshot and metric attribution, structured `Reconfigure`, and transition to `Ready`. It compares serialized object keys and command shape with a compiled board from the corresponding existing family flow.

No physical hardware was connected or validated by this task.

## RED Evidence

Test: `hardware::tests::injected_registry_supports_synthetic_extensions`

Command:

```text
rtk cargo test --manifest-path src-tauri/Cargo.toml injected_registry_supports_synthetic_extensions -- --exact
```

Result: exit 101. Compilation failed with `E0433` because `HardwareRegistry` was undeclared at `src/hardware.rs:350`. This proves the registry-injection API did not exist before production changes.

Tests:

- `coordinator::tests::second_rp2040_board_traverses_injected_registry_domain_flow`
- `coordinator::tests::esp32c3_board_traverses_injected_registry_domain_flow`

Command:

```text
rtk cargo test --manifest-path src-tauri/Cargo.toml traverses_injected_registry_domain_flow
```

Result: exit 101. Compilation failed because `crate::hardware::HardwareRegistry` and `RuntimeCoordinator::new_with_registry` did not exist. This proves both end-to-end tests required the new injection seam rather than passing through compiled production constants.

## GREEN Evidence

Command:

```text
rtk cargo test --manifest-path src-tauri/Cargo.toml hardware::tests::injected_registry_supports_synthetic_extensions -- --exact
```

Result: exit 0, `1 passed, 105 filtered out`.

Command:

```text
rtk cargo test --manifest-path src-tauri/Cargo.toml traverses_injected_registry_domain_flow
```

Result: exit 0, `2 passed, 104 filtered out`.

The registered test names were confirmed with `cargo test -- --list`:

- `hardware::tests::injected_registry_supports_synthetic_extensions`
- `coordinator::tests::second_rp2040_board_traverses_injected_registry_domain_flow`
- `coordinator::tests::esp32c3_board_traverses_injected_registry_domain_flow`

## Full Verification

```text
rtk cargo test --manifest-path src-tauri/Cargo.toml
```

Exit 0: `106 passed (3 suites, 2.97s)`.

```text
rtk cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Exit 0: `cargo clippy: No issues found`.

```text
rtk proxy rustfmt --edition 2024 --check src-tauri/src/hardware.rs src-tauri/src/coordinator.rs src-tauri/src/device.rs src-tauri/src/workspace.rs src-tauri/src/lib.rs
```

Exit 0 with no output. This scoped check covers every changed Rust file.

```text
rtk git diff --check
```

Exit 0 with no diagnostics.

## Acceptance Search Gate

The exact required command was run against these files:

- `src-tauri/src/coordinator.rs`
- `src-tauri/src/device.rs`
- `src-tauri/src/workspace.rs`
- `src-tauri/src/lib.rs`

```text
rtk proxy rg -n '"esp32s3"|"rp2040"|"esp32c3"|"luatos-esp32s3-aio"|"vccgnd-yd-rp2040"' src-tauri/src/coordinator.rs src-tauri/src/device.rs src-tauri/src/workspace.rs src-tauri/src/lib.rs
```

RED result: exit 0 with 71 matches: coordinator 21, device 8, workspace 23, and lib 19.

Controller resolution made this exact four-file search authoritative, including test modules. Production family and board IDs now have named constants in `hardware.rs`; synthetic IDs and their registry fixture remain under `#[cfg(test)]`. All 71 searched literals were replaced with those constants or registry-derived values, including the mechanical test-only changes in `device.rs` and `lib.rs`.

GREEN result: exit 1 with zero output and zero matches.

## Evidence Boundary

The automated evidence proves a shared backend domain flow for injected registry definitions and preserves all existing Rust tests. It does not prove USB behavior, firmware behavior, or GPIO safety on a physical second RP2040 or ESP32-C3 device.
