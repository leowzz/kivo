# Workbench One P0 PCB Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build an isolated tscircuit project that renders and structurally verifies the Workbench One P0 voice-and-macro console PCB.

**Architecture:** A declarative TypeScript circuit composes small subsystem components for input, display, USB/power, controller, and audio. A separate structural test renders Circuit JSON and asserts the board-level contracts without coupling the hardware package to the Kivo desktop application's dependency graph.

**Tech Stack:** tscircuit, TypeScript 5, React 19, Vitest, Node 24

## Global Constraints

- Treat this as a visual/electrical proof-of-concept, not a fabrication-ready release.
- Preserve display connector order exactly as `CON/SDA/SCL/PSH/TRA/TRB/BAK/GND/3V3`.
- Use one ESP32-S3 controller, one two-port USB 2.0 hub, and one CM108B audio codec.
- Use an 18-key `3 x 6` diode-isolated matrix.
- Use active-low, pulled-up inputs for three toggle switches and one hook switch.
- Keep the package under `hardware/workbench-one` and do not change root application dependencies.
- Prefix every shell command with `rtk`.

---

### Task 1: Isolated Circuit Package And Structural Contract

**Files:**
- Create: `hardware/workbench-one/package.json`
- Create: `hardware/workbench-one/tsconfig.json`
- Create: `hardware/workbench-one/vitest.config.ts`
- Create: `hardware/workbench-one/tests/workbench-one.test.tsx`

**Interfaces:**
- Consumes: the design contracts in `docs/superpowers/specs/2026-08-13-workbench-one-pcb-design.md`.
- Produces: package scripts `test`, `typecheck`, and `dev`; a failing structural test that imports `WorkbenchOne`.

- [ ] **Step 1: Add package metadata and exact scripts**

Create an ESM package with `tscircuit`, React, TypeScript, and Vitest dependencies. Configure `jsx: react-jsx`, strict type checking, and no emit.

- [ ] **Step 2: Write the structural test before circuit code**

Render `<WorkbenchOne />` through tscircuit's core API. Assert one `180 x 105` board, 18 `SW_K*` components, 18 `D_K*` components, display pin labels, four mounting holes, and the named USB/controller/audio components.

- [ ] **Step 3: Run the test and verify RED**

Run: `rtk proxy zsh -lc 'source /Users/leo/.nvm/nvm.sh; nvm use 24.18.0 >/dev/null; cd hardware/workbench-one; npm install; npm test'`

Expected: FAIL because `src/WorkbenchOne.tsx` does not exist.

### Task 2: Input Surface And Display Interface

**Files:**
- Create: `hardware/workbench-one/src/KeyMatrix.tsx`
- Create: `hardware/workbench-one/src/ControlPanel.tsx`
- Create: `hardware/workbench-one/src/footprints.ts`

**Interfaces:**
- Produces: `KeyMatrix(): JSX.Element`, `ControlPanel(): JSX.Element`, shared representative footprints, and named nets `ROW0..ROW2`, `COL0..COL5`, `CON`, `SDA`, `SCL`, `PSH`, `TRA`, `TRB`, `BAK`, `MODE1..MODE3`, and `HOOK`.

- [ ] **Step 1: Add focused source tests**

Extend the structural test with assertions for the 6 x 3 key coordinate grid, one diode per key, the exact nine-pin display ordering, three toggle components, hook connector, and I2C/input pull-ups.

- [ ] **Step 2: Run the test and verify RED**

Expected: FAIL because the input components and display interface are absent.

- [ ] **Step 3: Implement the minimal input surface**

Generate 18 switches and 18 1N4148W diodes from one immutable layout table. Add the display header, pull-ups, three toggle switches, and hook connector as small independent React components.

- [ ] **Step 4: Run the focused test and verify GREEN**

Expected: all input and display assertions PASS.

### Task 3: USB, Controller, Audio, And Power Topology

**Files:**
- Create: `hardware/workbench-one/src/UsbAndPower.tsx`
- Create: `hardware/workbench-one/src/Controller.tsx`
- Create: `hardware/workbench-one/src/Audio.tsx`

**Interfaces:**
- Consumes: input/display nets from Task 2.
- Produces: USB nets `USB_UP_DP/DM`, `USB_MCU_DP/DM`, `USB_AUDIO_DP/DM`; power nets `VBUS`, `+3V3`, `GND`; named components `J_USB`, `U_HUB`, `U_MCU`, and `U_AUDIO`.

- [ ] **Step 1: Add topology assertions**

Assert the named USB-C connector, hub, ESP32-S3, and CM108B exist; the hub pin labels expose one upstream and two downstream pairs; power protection, 3.3 V regulation, crystals, decoupling, and handset connector are present.

- [ ] **Step 2: Run the test and verify RED**

Expected: FAIL because the USB, controller, audio, and power components are absent.

- [ ] **Step 3: Implement representative subsystem circuits**

Create compact black-box IC components with explicit labelled ports and representative footprints. Connect the USB upstream pair to the hub, one downstream pair to native ESP32-S3 USB, the other to CM108B, and expose the handset microphone/receiver connector.

- [ ] **Step 4: Run the focused test and verify GREEN**

Expected: all topology assertions PASS.

### Task 4: Board Assembly, Labels, And Preview

**Files:**
- Create: `hardware/workbench-one/src/WorkbenchOne.tsx`
- Create: `hardware/workbench-one/src/main.tsx`
- Create: `hardware/workbench-one/README.md`

**Interfaces:**
- Consumes: `KeyMatrix`, `ControlPanel`, `UsbAndPower`, `Controller`, and `Audio`.
- Produces: default tscircuit entry point and documented local preview commands.

- [ ] **Step 1: Add board-level layout assertions**

Assert a `180 x 105` board, four M3 holes, zone labels, board title, and no fatal render errors.

- [ ] **Step 2: Run the test and verify RED**

Expected: FAIL because the assembled board and labels are absent.

- [ ] **Step 3: Assemble the board**

Place the control/display strip above the key matrix, group logic around the upper edge, add four mounting holes, and add restrained silkscreen labels and an antenna keepout marker.

- [ ] **Step 4: Document preview and limitations**

Document Node 24, `npm install`, `npm run dev`, `npm test`, `npm run typecheck`, exact module pin order, and the non-fabrication limitations from the design.

- [ ] **Step 5: Run tests and typecheck**

Run: `rtk proxy zsh -lc 'source /Users/leo/.nvm/nvm.sh; nvm use 24.18.0 >/dev/null; cd hardware/workbench-one; npm test && npm run typecheck'`

Expected: PASS with no TypeScript errors.

### Task 5: Visual Verification And Repository Gate

**Files:**
- Modify only files under `hardware/workbench-one` if visual defects are found.

**Interfaces:**
- Consumes: the assembled tscircuit board.
- Produces: a running local preview and captured evidence of a nonblank, coherent PCB view.

- [ ] **Step 1: Start the tscircuit development server**

Run: `rtk proxy zsh -lc 'source /Users/leo/.nvm/nvm.sh; nvm use 24.18.0 >/dev/null; cd hardware/workbench-one; npm run dev -- --host 127.0.0.1'`

Expected: a local preview URL and a server that remains running for inspection.

- [ ] **Step 2: Inspect the PCB view**

Open the preview, select PCB view, and confirm the board is nonblank, all 18 key positions fit, the top controls do not overlap, labels are legible, and the logic cluster stays out of the key field.

- [ ] **Step 3: Run the complete focused gate**

Run tests, typecheck, `rtk git diff --check`, and inspect `rtk git status --short`.

Expected: all focused checks PASS; only intended documentation and hardware project files are changed.
