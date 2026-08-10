# Task 9 Report: End-to-End Codex Display Service

## Scope

Implemented from base `6e2f4e7` with changes limited to the Task 9 display
service, built-in registry, coordinator/Tauri wiring, README documentation, and
focused tests in the owned Rust modules.

## TDD Evidence

The first focused RED was the required missing public boundary:

```text
rtk cargo test --manifest-path src-tauri/Cargo.toml display::service
error[E0432]: unresolved import `super::DisplayService`
no `DisplayService` in `display::service`
```

Further focused RED cycles proved the remaining integration contracts before
their implementations:

- two source-validation tests failed because mismatched provider items leaked
  into the semantic snapshot;
- `UnavailableDisplayProvider` was an unresolved import;
- `registry_with_codex_provider` was absent for the forced-init fallback;
- `RuntimeCoordinator::with_paste_and_renderers` was absent;
- `newest_display_snapshot` was absent.

Final focused results:

```text
display::service                                      7 passed
display::tests::built_in_registry                     2 passed
display::tests::source_initialization_failure         1 passed
coordinator::tests::injected_renderer_registry        1 passed
tests::display_snapshot_drain                         1 passed
```

## Implementation

- `DisplayService` polls every 100 ms, validates both update and item sources,
  updates `DisplayHub`, compares the complete semantic snapshot, and sends only
  changes. Shared-stop and receiver-disconnect paths both drop providers.
- Provider status logs are transition-only and contain only `providerId`,
  health, a static error code, and item count. No title, cwd, detail, task text,
  message text, or tool content reaches this log boundary.
- `UnavailableDisplayProvider` returns the stable `codex_source_init` code.
  The built-in registry contains exactly one provider: either the working Codex
  provider or the unavailable Codex provider.
- Tauri resolves the fallback as `home_dir/.codex` and cursors under
  `app_data_dir/display/codex-cursors-v1.json`. The owned registry boundary
  adapts the existing source constructor so the fallback is not doubled to
  `.codex/.codex`; App Server `codexHome` remains preferred by the source.
- Tauri creates one shared renderer registry and injects the same `Arc` into the
  coordinator and every worker. The coordinator loop drains the semantic
  channel before its 5 ms sleep and fans out only the newest queued snapshot.
- `AppState` owns the display JoinHandle. Exit sets the shared stop flag, joins
  display first, joins coordinator second, then shuts down paste and logging.
  `Option::take` makes repeated exit delivery harmless.
- README documents screens, data minimization, protocol 3-6 behavior, the V1
  SSD1306 128x32 rotation-0 panel, unchanged profile YAML, and physical checks.

## Automated Gate

The exact requested gate passed:

```text
rtk env PATH=/Users/leo/work/kivo/.superpowers/sdd/bin:/Users/leo/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin:$PATH make test
```

Evidence:

- release/Python suites: 33 + 32 + 32 passed;
- PlatformIO native: 89/89 passed;
- Rust: 360 unit tests passed, 1 ignored by design, plus both integration tests;
- Clippy: `--all-targets -- -D warnings` passed;
- frontend: 211/211 passed;
- production frontend build passed.

Firmware builds also passed:

- `rtk direnv exec . make build-rp2040`: 22,912/262,144 bytes RAM (8.7%),
  133,176/16,773,120 bytes flash (0.8%).
- `rtk direnv exec . make build-esp32s3`: 35,644/327,680 bytes RAM (10.9%),
  348,169/3,342,336 bytes flash (10.4%).
- `rtk git diff --check`: passed.

## Final Checklist Review

- Built-in registries are closed and exact: `codex` and
  `ssd1306_128x32_mono`; no discovery, manifests, dynamic loading, or IPC.
- Production Provider/service code has no Renderer or firmware dependency;
  workers render only after receiving a semantic `Arc<DisplaySnapshot>`.
- App Server requests remain read-only `thread/list` with
  `useStateDbOnly: true`; no mutation method is present.
- Existing full-gate tests cover protocol-6 display silence, per-device
  renderer selection, acknowledged scene bases, staged firmware commits,
  local-critical precedence, and 64-byte dirty-tile service after key scans.
- Source-init failure is covered end to end through unavailable provider,
  service snapshot, and the exact `CODEX OFFLINE` rendered screen.
- Shutdown is bounded by the existing one-second App Server response deadline
  plus the 100 ms service interval; no detached display or coordinator thread
  remains after Exit joins.

## Physical Acceptance

**Not Run.** No upload, OLED visual check, logic-analyzer capture, or sustained
key-press test was performed in Task 9. The parent task owns the identified
physical device and post-review upload. Automated tests and firmware builds are
not reported as physical acceptance.
