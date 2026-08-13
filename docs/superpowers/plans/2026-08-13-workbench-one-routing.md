# Workbench One P0 Routing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce a complete four-layer P0 copper-routing preview for Workbench One without claiming fabrication-ready USB or EMC performance.

**Architecture:** Keep the existing functional placement and enable the local tscircuit autorouter at board scope. Use L1/L4 primarily for signals, GND-dominant copper on L2, and 3V3-dominant copper on L3. Apply a 0.25 mm default signal width, 0.20 mm clearance, 0.50 mm primary power traces, and native differential-pair constraints for the two point-to-point downstream USB links. Treat any missing route or PCB routing error as a failed build.

**Tech Stack:** tscircuit 0.0.2308, React 19, TypeScript 5.9, Vitest 4

## Global Constraints

- Preserve the 180 mm by 105 mm board, component positions, four M3 holes, and repaired schematic layout.
- Use four copper layers with inner GND/3V3 pours and explicit plane vias.
- Do not claim controlled impedance, validated return paths, EMI compliance, or fabrication readiness.
- Keep generated `dist/` and `.tscircuit/` artifacts out of git.

---

### Task 1: Routing Acceptance Test

**Files:**
- Modify: `hardware/workbench-one/tests/workbench-one.test.tsx`

**Interfaces:**
- Consumes: `renderCircuit(): Promise<AnyCircuitElement[]>`
- Produces: assertions for PCB copper, routing errors, and USB differential-pair declarations

- [x] **Step 1: Add failing routing assertions**

Assert that routed output contains `pcb_trace` elements, contains no error whose type includes `autorouting`, `trace_missing`, `trace_error`, or `clearance_error`, records three USB links, and declares two valid point-to-point differential-pair elements.

- [x] **Step 2: Verify the test fails**

Run: `rtk npm test -- --run tests/workbench-one.test.tsx`

Expected: FAIL because the board currently has zero `pcb_trace` elements and no differential-pair declarations.

### Task 2: Routing Configuration

**Files:**
- Modify: `hardware/workbench-one/src/WorkbenchOne.tsx`
- Modify: `hardware/workbench-one/src/UsbAndPower.tsx`
- Modify: `hardware/workbench-one/src/Controller.tsx`
- Modify: `hardware/workbench-one/src/Audio.tsx`
- Modify: `hardware/workbench-one/src/ControlPanel.tsx`
- Modify: `hardware/workbench-one/src/KeyMatrix.tsx`
- Modify: `hardware/workbench-one/package.json`

**Interfaces:**
- Consumes: existing named nets and component port selectors
- Produces: routed Circuit JSON and preview artifacts

- [x] **Step 1: Enable board routing**

Remove `routingDisabled`, set `defaultTraceWidth="0.25mm"`, `minTraceWidth="0.20mm"`, `traceClearance="0.20mm"`, `autorouter="auto_local"`, and `autorouterEffortLevel="10x"` on the board.

- [x] **Step 2: Declare USB differential pairs**

Add MCU and audio `<differentialpair>` elements with `maxLengthSkew="0.5mm"`, `pcbTraceGap="0.2mm"`, and the appropriate point-to-point port selectors. Keep the upstream Type-C connection as a multi-terminal USB link because its duplicate connector pins and shunt ESD branch are not a valid native point-to-point differential-pair object.

- [x] **Step 3: Widen primary power traces**

Set `thickness="0.5mm"` on source traces that directly carry `VBUS_RAW`, `VBUS`, `V3V3`, or GND supply branches. Keep data and control traces at the 0.25 mm board default.

- [x] **Step 4: Remove forced routing-disabled CLI flags**

Update `build` and `build:preview` scripts so generated artifacts include copper routing.

### Task 3: Route And Repair

**Files:**
- Modify as required: `hardware/workbench-one/src/*.tsx`

**Interfaces:**
- Consumes: autorouter diagnostics and Circuit JSON
- Produces: zero routing failures

- [x] **Step 1: Run routed build with diagnostics**

Run: `rtk proxy npx tsci build src/main.tsx --disable-parts-engine --autorouter-timeout 2m --autorouter-dump-srj failed`

- [x] **Step 2: Inspect failures**

Count `pcb_trace`, `pcb_via`, `pcb_autorouting_error`, `pcb_trace_missing_error`, `pcb_trace_error`, and clearance-error elements in `dist/src/main/circuit.json`.

- [x] **Step 3: Repair only failed connections**

Use `routingPhaseIndex`, `pcbRouteHints`, or explicit `pcbPath` only for connections the local autorouter cannot complete. Do not hand-route already successful networks.

### Task 4: Final Verification

**Files:**
- Verify: `hardware/workbench-one/dist/src/main/pcb.png`

**Interfaces:**
- Consumes: final routed source
- Produces: verified P0 routing preview

- [x] **Step 1: Run automated checks**

Run tests, typecheck, netlist, schematic placement, PCB placement, and routed build. Expected: all exit zero; PCB and schematic checks report no actionable overlaps or routing errors.

- [x] **Step 2: Inspect generated routing**

Verify the PCB PNG and interactive viewer are nonblank, all four layers contain routed copper, the key matrix remains readable, USB pairs stay in the logic area, and no route crosses a mounting hole or component keepout.

- [x] **Step 3: Check the worktree**

Run `rtk git diff --check` and `rtk git status --short`. Expected: only Workbench One source, tests, and routing docs are modified.
