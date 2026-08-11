# Stabilize Telephone Handset Pocket Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the short loose entry chamfer with a large insertion funnel
and lower the switch platform so the fully bottomed switch and four corner
pads support the handset together.

**Architecture:** Keep the canonical switch cell, open-bottom tower, ribs, and
wire route unchanged in concept. Derive the larger outer ring from the exact
`55 x 70` locating section plus a `2 mm`-per-side funnel and a minimum `2.4 mm`
rim wall; independently validate both funnel sections and the complete allowed
solid. Separate the switch-platform datum from the handset support datum so the
validator can prove the nominal zero-gap height chain.

**Tech Stack:** Python 3.13, PEP 723 through `uv`, `trimesh==5.0.0`,
`manifold3d==3.5.2`, `numpy==2.5.1`, `scipy==1.16.3`, `Pillow==12.3.0`,
`pytest==8.4.2`, binary STL in millimeters.

## Global Constraints

- Canonical source remains
  `models/3d-print/3x3keypad/pico_macro_pad_top.stl.stl` with SHA-256
  `ce0f7b64d06b3fc2864d29452e87fb264f70567c0f5924eab380d0748f4e9155`.
- Preserve the exact centered `19.05 x 19.05 x 3.4` source cell and stepped
  `14.798 x 14.798 x 1.998` / `14 x 14 x 1.402` switch aperture.
- Lower locating section: `55 x 70`, `R1.6`, from `Z=13.4` through `Z=24.4`.
- Funnel: `4.0` high, widening `2.0` per side to a `59 x 74`, `R3.6` mouth.
- Outer bounds: `63.8 x 78.8 x 28.4`, outer radius `R6.0`, minimum mouth wall
  `2.4`, lower locating wall `4.4`.
- Safety-pad support datum stays `Z=13.4`; switch platform moves to
  `Z=7.0..10.4`; the bottomed-trigger/recess chain is `10.4 + 5 - 2 = 13.4`.
- Tower and rear-rib tops move to `Z=7.0`; the `19.2` channel stays open.
- Rear hole stays circular diameter `4.0`, centered at `X=31.9`, `Z=5.0`, and
  must cross the complete `4.4`-thick lower rear wall.
- Keep one connected, upright, support-free, open-bottom component. Add no
  floor, roof, cover, fastener, connector, keycap, branding, or decoration.
- Do not modify the canonical STL, generated keypad variants,
  `scripts/macro_pad_variants.py`, dependency manifests, or unrelated files.
- The ignored STL remains local under `models/3d-print`; previews remain under
  `/tmp/kivo-handset-switch-base-previews`.
- Every shell command and command-chain segment starts with `rtk`.
- Slicer, printed fit, actual simultaneous pad contact, wire fit, stability, and
  sustained-load checks remain Not Run.

## File Structure

- Modify `scripts/telephone_handset_switch_base.py`: dimensions, funnel ring,
  lowered platform/tower/ribs, pad placement, rear bore, validation, previews.
- Modify `test/test_telephone_handset_switch_base.py`: finished-mesh funnel,
  datum-chain, access, validator-adversary, export, and CLI contracts.
- Update `docs/superpowers/specs/2026-08-10-telephone-handset-switch-base-design.md`:
  approved dimensions and physical acceptance boundary.
- Generate ignored
  `models/3d-print/telephone-handset-switch-base/telephone_handset_switch_base.stl`.
- Generate temporary `/tmp/kivo-handset-switch-base-previews/*.png`.

---

### Task 1: Build And Validate The Funnel-Supported Geometry

**Files:**

- Modify: `scripts/telephone_handset_switch_base.py`
- Modify: `test/test_telephone_handset_switch_base.py`

**Interfaces:**

- Consumes: canonical normalized source mesh from `load_canonical_source()`.
- Produces: revised constants, `inner_funnel_cutter()`, `build_outer_ring()`,
  `generate_base(source)`, and `validate_base(mesh, source)`.

- [ ] **Step 1: Replace the chamfer regression with a failing funnel contract**

Replace `test_generate_base_preserves_exact_inner_chamfer_slope` with:

```python
def test_generate_base_uses_exact_funnel_and_locating_sections() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)

    expected_sections = (
        (13.401, (55.0, 70.0)),
        (20.0, (55.0, 70.0)),
        (24.399, (55.0, 70.0)),
        (25.4, (56.0, 71.0)),
        (26.4, (57.0, 72.0)),
        (27.4, (58.0, 73.0)),
        (28.399, (58.999, 73.999)),
    )
    for level, expected in expected_sections:
        loops = section_loop_sizes(mesh, axis=2, level=level)
        assert any(
            np.allclose(size, expected, rtol=0.0, atol=0.003)
            for size in loops
        )
```

Update the existing outer/pocket test to require outer extents
`(63.8, 78.8, 28.4)`, lower `R1.6`, and top `R3.6` profiles.

- [ ] **Step 2: Add a failing support-height and rear-wall contract**

Add a test that measures the finished platform/source-cell bounds, pad tops,
tower/rib tops, and complete rear bore:

```python
def test_bottomed_switch_and_four_pads_share_the_support_datum() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)

    cell = base.place_source_cell(source)
    np.testing.assert_allclose(
        cell.bounds[:, 2], [7.0, 10.4], rtol=0.0, atol=0.003
    )
    assert base.PLATFORM_TOP + 5.0 - 2.0 == pytest.approx(base.PAD_TOP)
    assert base.PLATFORM_TOP == pytest.approx(10.4)
    assert base.PAD_TOP == pytest.approx(13.4)

    report = base.validate_base(mesh, source)
    assert report.pocket_depth == pytest.approx(15.0, abs=0.003)
    assert report.open_underside
    assert report.rear_wire_path
```

Extend the existing internal rear-hole annulus regression so the obstruction is
placed anywhere across `Y=74.4..78.8`; every restriction must be rejected.

- [ ] **Step 3: Run RED and confirm failures are dimensional**

```bash
rtk uv run --offline --isolated \
  --with pytest==8.4.2 \
  --with manifold3d==3.5.2 \
  --with numpy==2.5.1 \
  --with Pillow==12.3.0 \
  --with scipy==1.16.3 \
  --with trimesh==5.0.0 \
  python -m pytest test/test_telephone_handset_switch_base.py -q
```

Expected: the new/replaced tests fail because the current output remains
`59.8 x 74.8`, uses a `0.8` chamfer, and places the platform at
`Z=10.0..13.4`. Existing tests continue to pass.

- [ ] **Step 4: Derive the new binding constants**

Use dimension relationships rather than duplicate literals:

```python
INNER_WIDTH = 55.0
INNER_LENGTH = 70.0
INNER_RADIUS = 1.6
WALL = 2.4
FUNNEL_EXPANSION = 2.0
FUNNEL_DEPTH = 4.0
MOUTH_WIDTH = INNER_WIDTH + 2.0 * FUNNEL_EXPANSION
MOUTH_LENGTH = INNER_LENGTH + 2.0 * FUNNEL_EXPANSION
MOUTH_RADIUS = INNER_RADIUS + FUNNEL_EXPANSION
LOWER_INSET = WALL + FUNNEL_EXPANSION
OUTER_WIDTH = MOUTH_WIDTH + 2.0 * WALL
OUTER_LENGTH = MOUTH_LENGTH + 2.0 * WALL
OUTER_RADIUS = MOUTH_RADIUS + WALL
OUTER_HEIGHT = 28.4
FUNNEL_BOTTOM = OUTER_HEIGHT - FUNNEL_DEPTH
PAD_TOP = 13.4
BOTTOMED_TRIGGER_HEIGHT = 5.0
HANDSET_RECESS = 2.0
PLATFORM_TOP = PAD_TOP - BOTTOMED_TRIGGER_HEIGHT + HANDSET_RECESS
PLATFORM_BOTTOM = PLATFORM_TOP - PLATE_THICKNESS
```

Update dynamic probes and centers from these constants. Replace hard-coded
corner coordinates with expressions based on `OUTER_WIDTH` and `OUTER_LENGTH`.

- [ ] **Step 5: Replace the chamfer with the exact funnel**

Rename `inner_chamfer_cutter()` to `inner_funnel_cutter()`. Build a convex hull
between the exact lower section at `FUNNEL_BOTTOM` and the expanded mouth
section at `OUTER_HEIGHT`:

```python
def inner_funnel_cutter() -> trimesh.Trimesh:
    lower = rounded_rectangle_section(
        INNER_WIDTH, INNER_LENGTH, INNER_RADIUS, (CENTER_X, CENTER_Y)
    )
    mouth = lower.offset(
        FUNNEL_EXPANSION,
        join_type=manifold3d.JoinType.Round,
        circular_segments=ROUNDED_SECTION_SEGMENTS,
    )
    points = [
        [float(x), float(y), z]
        for section, z in ((lower, FUNNEL_BOTTOM), (mouth, OUTER_HEIGHT))
        for polygon in section.to_polygons()
        for x, y in polygon
    ]
    return manifold_to_mesh(manifold3d.Manifold.hull_points(points))
```

Subtract the full-height `55 x 70` lower cutter and this funnel from the new
`63.8 x 78.8`, `R6` outer prism. Build the validation mouth from
`validation_rounded_rectangle_section()` so the independent outer-ring
reference does not call `rounded_rectangle_section()`, `build_outer_ring()`,
or `generate_base()`.

- [ ] **Step 6: Lower load-bearing structures and preserve access**

Move the source platform to `Z=7.0..10.4`; tower and ribs end at `Z=7.0` plus
the existing `JOIN_OVERLAP`. Keep tower wall thickness `2.4` and clear channel
`19.2`.

Use `LOWER_INSET` for lower pocket boundaries, pad placement, pad references,
and the rear inner face. Keep pad tops at `PAD_TOP=13.4` and derive pad/gusset
bottoms from that datum.

Cut the rear bore through the complete lower wall:

```python
REAR_WALL_THICKNESS = LOWER_INSET
rear_center_y = OUTER_LENGTH - REAR_WALL_THICKNESS / 2.0
```

Use cutter height `REAR_WALL_THICKNESS + 2.0` and validation-clearance height
`REAR_WALL_THICKNESS + 2 * BOOLEAN_TOLERANCE`. Keep the hole center at
`(CENTER_X, 5.0)` in its X/Z section.

- [ ] **Step 7: Separate platform and pad validation**

Replace the shared `SUPPORT_TOP_PROBES` assumption with one platform-top probe
expected at `PLATFORM_TOP` and four pad-top probes expected at `PAD_TOP`.
Compute pocket depth from `PAD_TOP`, not `PLATFORM_TOP`.

Validate lower `55 x 70 R1.6` and funnel sections independently at multiple Z
levels, then retain the full outer-ring coverage and complete allowed-union
checks. Update open-underside probes so their upper Z stays below the new
`PLATFORM_BOTTOM`.

- [ ] **Step 8: Run GREEN and focused regression gates**

Run the Step 3 command. Expected: at least `28 passed`, with no warnings.

```bash
rtk uv run --offline --isolated --with ruff==0.12.12 \
  ruff check scripts/telephone_handset_switch_base.py \
  test/test_telephone_handset_switch_base.py
rtk uv run --offline --isolated --with ruff==0.12.12 \
  ruff format --check scripts/telephone_handset_switch_base.py \
  test/test_telephone_handset_switch_base.py
rtk git diff --check -- scripts/telephone_handset_switch_base.py \
  test/test_telephone_handset_switch_base.py
```

Expected: all exit `0`.

- [ ] **Step 9: Commit the geometry change**

```bash
rtk git add scripts/telephone_handset_switch_base.py \
  test/test_telephone_handset_switch_base.py
rtk git commit -m "fix: stabilize handset pocket support"
```

The commit must contain only the generator and its tests.

---

### Task 2: Regenerate And Inspect The Printable Artifact

**Files:**

- Modify: `scripts/telephone_handset_switch_base.py` only if preview framing
  needs adjustment for the larger footprint/lower platform.
- Modify: `test/test_telephone_handset_switch_base.py` only for a failing
  preview regression discovered during inspection.
- Generate, ignored:
  `models/3d-print/telephone-handset-switch-base/telephone_handset_switch_base.stl`
- Generate, temporary: `/tmp/kivo-handset-switch-base-previews/*.png`

**Interfaces:**

- Consumes: revised `generate_base()` and `validate_base()`.
- Produces: deterministic binary STL, sorted CLI JSON, and four `1200 x 900`
  previews.

- [ ] **Step 1: Regenerate through the reproducible CLI**

```bash
rtk uv run --offline --script scripts/telephone_handset_switch_base.py
```

Expected JSON includes outer extents `[63.8, 78.8, 28.4]`, pocket bounds
`[55.0, 70.0]`, pocket depth `15.0`, one component, watertight/two-manifold
true, open underside true, rear path true, output path, and a new SHA-256.

- [ ] **Step 2: Reload the exported STL and validate it**

Load the STL with `process=False`, merge duplicate facet vertices on a copy,
and run `validate_base(reloaded, source)`. Assert one normalized body,
watertightness, two-face edge incidence, positive volume, and the revised
outer/funnel/platform contracts.

- [ ] **Step 3: Inspect all four regenerated previews**

Open:

```text
/tmp/kivo-handset-switch-base-previews/isometric.png
/tmp/kivo-handset-switch-base-previews/top.png
/tmp/kivo-handset-switch-base-previews/side-section.png
/tmp/kivo-handset-switch-base-previews/bottom.png
```

Reject and fix blank/cropped output, a blocked stepped switch aperture, a broad
floor/roof, disconnected tower/ribs, a rear bore that stops inside the thicker
wall, missing pads/gussets, or a side section that fails to show the lowered
platform and pad datum separately.

- [ ] **Step 4: Run the full fresh verification suite**

Run the focused command from Task 1 Step 3, then:

```bash
rtk uv run --offline --isolated \
  --with pytest==8.4.2 \
  --with manifold3d==3.5.2 \
  --with numpy==2.5.1 \
  --with Pillow==12.3.0 \
  --with scipy==1.16.3 \
  --with trimesh==5.0.0 \
  python -m pytest test/test_macro_pad_variants.py -q
rtk uv run --offline --isolated --with ruff==0.12.12 \
  ruff check scripts/telephone_handset_switch_base.py \
  test/test_telephone_handset_switch_base.py
rtk uv run --offline --isolated --with ruff==0.12.12 \
  ruff format --check scripts/telephone_handset_switch_base.py \
  test/test_telephone_handset_switch_base.py
rtk git diff --check
```

Expected: focused suite at least `28 passed`, macro suite `36 passed`, all
static/diff checks exit `0`, and the generated STL remains ignored.

- [ ] **Step 5: Commit only a necessary preview fix**

If Task 2 required tracked preview/test changes, commit only the two owned
files with:

```bash
rtk git add scripts/telephone_handset_switch_base.py \
  test/test_telephone_handset_switch_base.py
rtk git commit -m "fix: frame stabilized handset previews"
```

If no tracked files changed, do not create an empty commit.

- [ ] **Step 6: Report the physical boundary**

Report mesh tests and visual preview inspection as complete. Keep slicer
toolpath, printed funnel fit, simultaneous switch bottom-out/four-pad contact,
rear-wire fit, flat-surface stability, and sustained-load behavior as Not Run.
