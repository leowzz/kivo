# Kivo Rename Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rename the packaged application and device branding from Vibe Tool to Kivo.

**Architecture:** Replace existing product metadata and visible strings in place. Keep the existing VID/PID device match so previously flashed firmware remains connectable; do not migrate the old application configuration directory.

**Tech Stack:** Tauri 2, Rust, React/TypeScript, PlatformIO/Arduino, GitHub Actions

## Global Constraints

- User-visible product name: `Kivo`.
- Package, crate, library, and executable names: `kivo` / `kivo_lib`.
- Application identifier: `cn.wleo.kivo`.
- Firmware manufacturer and product names: `Kivo` and `Kivo Keyboard`.
- Do not migrate or read `com.leose.vibetool` configuration.
- Keep USB matching based on VID `0x303a` and PID `0x4002`.
- Do not rewrite historical design and implementation-plan documents.

---

### Task 1: Rename Kivo Across Packaging and Runtime Surfaces

**Files:**
- Modify: `package.json`
- Modify: `package-lock.json`
- Modify: `index.html`
- Modify: `.github/workflows/release-windows.yml`
- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/main.cpp`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/tray.rs`
- Modify: `src-tauri/src/config.rs`
- Modify: `src-tauri/src/model.rs`

**Interfaces:**
- Consumes: existing Tauri product metadata, Rust crate target, React headings, tray labels, firmware USB descriptors, and release workflow metadata.
- Produces: a `Kivo` installer/application, `kivo` executable and package names, `kivo_lib` Rust library target, `cn.wleo.kivo` platform identity, and `Kivo Keyboard` USB display name.

- [ ] **Step 1: Rename package and application metadata**

Set npm package names to `kivo`; Cargo package, library, and lockfile entries to `kivo` and `kivo_lib`; Tauri `productName`, `identifier`, and window title to `Kivo`, `cn.wleo.kivo`, and `Kivo`; and the Rust entry point to `kivo_lib::run()`.

- [ ] **Step 2: Rename visible application and release text**

Replace the HTML title, React heading, tray `Open`/`Quit` labels and tooltip, Rust startup error, and GitHub release name with `Kivo`. Rename test-only temporary prefixes and the frontend fixture path from `vibe-tool` to `kivo`.

- [ ] **Step 3: Rename firmware USB descriptors**

Set:

```cpp
USB.manufacturerName("Kivo");
USB.productName("Kivo Keyboard");
```

Do not change `USB.vid(0x303A)` or `USB.pid(0x4002)`; existing firmware remains discoverable by the desktop app.

- [ ] **Step 4: Verify no old branding remains in active source**

Run:

```bash
rtk proxy rg -n -i "vibe[_ -]?tool|vibetool|ESP Vibe" \
  --glob '!docs/superpowers/**' --glob '!docs/rp2040-compatibility-assessment.md' \
  --glob '!target/**' .
```

Expected: no matches.

- [ ] **Step 5: Run focused and full checks**

Run:

```bash
rtk npm test
rtk npm run build
rtk cargo test --manifest-path src-tauri/Cargo.toml
rtk uv run pio test -e native
rtk npm run tauri build -- --bundles app
rtk git diff --check
```

Expected: every command exits `0`; the bundle is named `Kivo.app` on macOS and uses `Kivo` as the Windows MSI product name.

- [ ] **Step 6: Commit the implementation**

```bash
rtk git add package.json package-lock.json index.html .github/workflows/release-windows.yml src src-tauri
rtk git commit -m "refactor: rename app to Kivo"
```
