# Kivo Rename Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rename every current application, package, release, and firmware-facing `Vibe Tool` identity to `Kivo` without migrating old configuration.

**Architecture:** This is one metadata-and-copy change. Existing behavior and storage code stay unchanged; Tauri derives the new configuration directory from `cn.wleo.kivo`, and generated lockfiles are refreshed with the repository's current package tools.

**Tech Stack:** Tauri 2, Rust, React/Vite, PlatformIO Arduino, npm, uv

## Global Constraints

- User-visible product name: `Kivo`.
- Package, crate, and executable name: `kivo`.
- Application identifier: `cn.wleo.kivo`.
- Firmware USB manufacturer and product names: `Kivo` and `Kivo Keyboard`.
- Do not migrate or read configuration from `com.leose.vibetool`.
- Do not rewrite historical design and implementation documents that describe the former name.

---

### Task 1: Rename the product consistently

**Files:**
- Modify: `.github/workflows/release-windows.yml`
- Modify: `index.html`
- Modify: `package.json`
- Modify: `package-lock.json`
- Modify: `pyproject.toml`
- Modify: `uv.lock`
- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/main.cpp`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/src/config.rs`
- Modify: `src-tauri/src/device.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/model.rs`
- Modify: `src-tauri/src/tray.rs`

**Interfaces:**
- Consumes: Tauri's existing `app.path().app_config_dir()` lookup.
- Produces: application metadata `Kivo` / `kivo` / `cn.wleo.kivo` and USB identity `Kivo Keyboard`.

- [x] **Step 1: Prove the old identity is present**

Run:

```bash
rtk git grep -n -i -e 'Vibe Tool' -e 'vibe-tool' -e 'vibetool' -e 'ESP Vibe' -- ':!docs/**'
```

Expected: matches in application metadata, source, release workflow, lockfiles, and tests.

- [x] **Step 2: Apply the minimal rename**

Make these exact substitutions in current source and metadata:

```text
Vibe Tool                       -> Kivo
vibe-tool                       -> kivo
vibe_tool_lib                   -> kivo_lib
com.leose.vibetool              -> cn.wleo.kivo
ESP Vibe Text Keyboard          -> Kivo Keyboard
ESP Vibe                        -> Kivo
esp-vibe                        -> kivo
```

Do not add configuration migration logic. Keep historical files under
`docs/superpowers/` unchanged except for the new Kivo spec and plan.

- [x] **Step 3: Refresh generated lockfiles**

Run:

```bash
rtk npm install --package-lock-only
rtk uv lock
rtk cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: commands succeed; root entries in `package-lock.json`, `uv.lock`, and
`src-tauri/Cargo.lock` use `kivo`.

- [x] **Step 4: Verify no current identity was missed**

Run:

```bash
rtk git grep -n -i -e 'Vibe Tool' -e 'vibe-tool' -e 'vibetool' -e 'ESP Vibe' -- ':!docs/**'
```

Expected: no matches.

- [x] **Step 5: Run focused and full checks**

Run:

```bash
rtk npm test
rtk npm run build
rtk cargo test --manifest-path src-tauri/Cargo.toml
rtk uv run pio test -e native
rtk git diff --check
```

Expected: every command exits successfully.

- [ ] **Step 6: Build the Windows installer metadata path**

Run on Windows CI or a Windows development host:

```bash
rtk npm run tauri build -- --bundles nsis
```

Expected: the generated installer and installation directory use `Kivo`, with
no space in the product directory segment.

Not run locally: this checkout is on macOS. The equivalent macOS bundle build
produced `Kivo.app` with executable `kivo`.

- [x] **Step 7: Commit the rename**

```bash
rtk git add .github/workflows/release-windows.yml index.html package.json package-lock.json pyproject.toml uv.lock src src-tauri docs/superpowers/plans/2026-07-28-kivo-rename.md
rtk git commit -m "refactor: rename Vibe Tool to Kivo"
```
