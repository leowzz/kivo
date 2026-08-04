# Key9 Production Profile Design

## Context

The current runtime workspace contains three Device Profiles. Physical devices actively reference `3key` and `key9`; no device references `tel001`. The bundled `models/prod/tel001.yaml` profile is therefore a legacy first-run seed rather than a working production template.

## Production Template

`models/prod/` will contain exactly one profile: `key9.yaml`.

The template is derived from the working runtime `key9` profile and retains:

- schema version 2;
- the three 3-column groups `R1`, `R2`, and `R3`;
- buttons `K1` through `K9`;
- the `hardware` Hardware Profile for `vccgnd-yd-rp2040`;
- the verified direct GPIO mapping `K1: 1` through `K9: 9`;
- the 30 ms debounce setting.

The template sets `actions: {}`. Runtime-specific actions, device records, device IDs, names, metrics, and editor selection are not copied into the repository.

`models/prod/tel001.yaml` is deleted.

## Runtime Behavior

The existing startup contract remains unchanged. A fresh workspace loads bundled profiles and writes `key9.yaml` into its own `data/profiles/` directory. An existing workspace continues to load its own persisted profiles and is not overwritten or synchronized from `models/prod/`.

The runtime directory at `/Users/leo/Library/Application Support/cn.wleo.kivo` is read-only input for this migration and is not modified.

## Code And Tests

The bundled-profile test will require exactly one valid profile named `key9`, using the RP2040 board and the verified nine-key direct mapping. Test helpers that currently include `models/prod/tel001.yaml` will include `key9.yaml` instead while retaining their local test-only IDs where required.

Verification covers:

- YAML parsing and Device Profile validation;
- complete Rust tests;
- complete frontend tests and production build;
- Rust formatting and `git diff --check`;
- confirmation that `models/prod/` contains `key9.yaml` and no `tel001.yaml`.
