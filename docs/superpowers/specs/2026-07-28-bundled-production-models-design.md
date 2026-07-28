# Bundled Production Models Design

## Goal

Package every `models/prod/*.json` keypad layout with the Tauri application and make those layouts available in the GUI model selector after installation.

## Build Layout

`src-tauri/tauri.conf.json` maps `../models/prod/*.json` into the application resource directory at `models/`. This uses Tauri's native resource mapping and keeps the packaged files inspectable without a custom build script or generated Rust source.

`models/prod/` is the production catalog. Files elsewhere under `models/` remain development inputs and are not automatically shipped by this mechanism.

## Startup Synchronization

Before loading the user model catalog, startup resolves the packaged resource directory and synchronizes its `models/*.json` files into the existing application configuration directory:

`<app-config>/models/`

For each packaged JSON file:

1. Read and deserialize it as `ModelLayout`.
2. Validate it with the existing `ModelLayout::validate()` rules.
3. Require the source filename to equal `<model-id>.json`.
4. Atomically write it to the runtime model directory using that filename.

Packaged models overwrite runtime models with the same ID on every startup. Missing packaged models are added. Runtime JSON files whose IDs are not present in the package are preserved, so custom models are not deleted.

After synchronization, the existing `load_all()` path loads the complete runtime catalog for the GUI. Development runs without packaged resources retain the current default-model seeding behavior.

## Failure Behavior

An unreadable, malformed, or invalid packaged model must not overwrite an existing runtime file. The application continues startup, preserves the last valid runtime copy, and surfaces the synchronization error through the existing configuration error field.

Errors in one packaged model do not prevent other valid packaged models from synchronizing.

## Tests

Focused Rust tests cover:

- copying a missing packaged model;
- overwriting a same-ID runtime model;
- preserving runtime-only models;
- rejecting an invalid or mismatched-filename packaged model without replacing the valid runtime file;
- continuing to synchronize other valid models after one failure.

Packaging verification builds the Tauri app and confirms that `models/prod/*.json` appears under the bundled resource `models/` directory.

## Non-Goals

- Runtime image recognition or model discovery from hardware.
- Deleting custom/runtime-only models.
- Synchronizing development files outside `models/prod/`.
- Changing GPIO maps or button actions during model synchronization.
