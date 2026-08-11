# Narrow Telephone Handset Throat Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Narrow the handset's support-datum locating throat to `40 x 55` while
retaining the existing `59 x 74` insertion mouth and extending the funnel over
the complete `15.0` pocket depth.

**Architecture:** Keep the approved outer footprint, switch-height chain,
canonical switch cell, four pads, open-bottom tower, ribs, and rear wire route.
Replace the short scalar-offset funnel with an explicit ruled hull between two
rounded rectangles, then independently validate interpolated width, length,
and corner radius at multiple heights. Derive the thicker lower ring and rear
wall from the new throat instead of duplicating dimensions.

**Tech Stack:** Python 3.13, PEP 723 through `uv`, `trimesh==5.0.0`,
`manifold3d==3.5.2`, `numpy==2.5.1`, `scipy==1.16.3`, `Pillow==12.3.0`,
`pytest==8.4.2`, binary STL in millimeters.

## Global Constraints

- Canonical source remains
  `models/3d-print/3x3keypad/pico_macro_pad_top.stl.stl` with SHA-256
  `ce0f7b64d06b3fc2864d29452e87fb264f70567c0f5924eab380d0748f4e9155`.
- Preserve the exact centered `19.05 x 19.05 x 3.4` source cell and stepped
  `14.798 x 14.798 x 1.998` / `14 x 14 x 1.402` switch aperture.
- Locating throat: `40 x 55`, `R1.6`, at support datum `Z=13.4`.
- Funnel mouth: `59 x 74`, `R3.6`, at `Z=28.4`; the ruled funnel spans the
  complete `15.0` depth and expands `9.5` per side/end.
- Outer bounds remain `63.8 x 78.8 x 28.4`, outer radius `R6.0`, mouth wall
  `2.4`, and lower throat/rear wall `11.9`.
- Safety-pad tops stay `Z=13.4`; switch platform stays `Z=7.0..10.4`; the
  bottomed-trigger/recess chain stays `10.4 + 5 - 2 = 13.4`.
- Tower and rear-rib nominal tops stay `Z=7.0`; the `19.2` channel stays open.
- Rear hole stays circular diameter `4.0`, centered at `X=31.9`, `Z=5.0`, and
  must cross the complete `11.9`-thick rear wall.
- Keep one connected, upright, support-free, open-bottom component. Add no
  floor, roof, cover, fastener, connector, keycap, branding, or decoration.
- Keep mouth `R3.6` and outer `R6.0`; do not derive either radius by applying
  the `9.5` throat expansion as a uniform offset.
- Do not modify the canonical STL, keypad variants,
  `scripts/macro_pad_variants.py`, dependency manifests, or unrelated files.
- The ignored STL remains local under `models/3d-print`; previews remain under
  `/tmp/kivo-handset-switch-base-previews`.
- Every shell command and command-chain segment starts with `rtk`.
- Slicer, printed fit, actual wedge height, simultaneous pad contact, wire fit,
  stability, and sustained-load checks remain Not Run.

## File Structure

- Modify `scripts/telephone_handset_switch_base.py`: throat/funnel constants,
  explicit mouth profile, interpolation, probes, lower ring, pads/ribs, rear
  bore, validation, and previews.
- Modify `test/test_telephone_handset_switch_base.py`: finished-mesh throat,
  interpolated funnel, lower-support, rear-wall, pad, export, and CLI contracts.
- Update `docs/superpowers/specs/2026-08-10-telephone-handset-switch-base-design.md`:
  approved `40 x 55` full-depth-funnel contract.
- Generate ignored
  `models/3d-print/telephone-handset-switch-base/telephone_handset_switch_base.stl`.
- Generate temporary `/tmp/kivo-handset-switch-base-previews/*.png`.

---

### Task 1: Build And Validate The 40 x 55 Full-Depth Funnel

**Files:**

- Modify: `scripts/telephone_handset_switch_base.py`
- Modify: `test/test_telephone_handset_switch_base.py`

**Interfaces:**

- Consumes: canonical normalized source mesh from `load_canonical_source()`.
- Produces: revised dimensional constants, `inner_funnel_cutter()`,
  `funnel_section_dimensions(level)`, `generate_base(source)`, and
  `validate_base(mesh, source)`.

- [ ] **Step 1: Replace the old short-funnel regression with a failing full-depth contract**

Use finished-mesh sections at the throat, quarter points, and mouth:

```python
def test_generate_base_uses_exact_full_depth_funnel() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)

    expected_sections = (
        (13.401, (40.001, 55.001)),
        (17.15, (44.75, 59.75)),
        (20.9, (49.5, 64.5)),
        (24.65, (54.25, 69.25)),
        (28.399, (58.999, 73.999)),
    )
    for level, expected in expected_sections:
        loops = section_loop_sizes(mesh, axis=2, level=level)
        assert any(
            np.allclose(size, expected, rtol=0.0, atol=0.003)
            for size in loops
        )
```

At `Z=13.401`, `20.9`, and `28.399`, additionally call
`require_rounded_rectangle_loop()` with literal expected radii `1.600133`,
`2.6`, and `3.599867`. Update the outer/pocket test to keep the existing outer
and mouth bounds while reporting a `40 x 55` throat.

- [ ] **Step 2: Add failing lower-ring, rear-wall, pad, and access contracts**

Update the literal finished-pad regression to measure these new coordinates:

```python
# Z=12 pad footprint
(21.9, 21.9), (21.9, 11.9), (11.9, 21.9)
# Y=17.0 outer pad face
(21.9, 11.0), (21.9, 13.4)
# Y=13.1 foot-to-pad gusset
(14.3, 3.4), (21.9, 11.0)
```

The measured runs remain literal `10.0`, `2.4`, and `[7.6, 7.6]`; do not derive
them from production constants. Move the rear circular-section assertion to
`Y=72.85`. Parameterize rear annulus obstructions wholly inside
`Y=66.9..78.8`, using centers `67.5`, `72.85`, and `78.2`, and accept only
`rear wire path` or `rear wire hole clearance` errors.

Add assertions that the generated lower interior begins at literal inset
`11.9`, the rear ribs reach literal `Y=66.9`, and open-underside probes occupy
the four gaps between the pads and centered `24 x 24` platform without
intersecting the thick lower ring.

- [ ] **Step 3: Run RED and confirm dimensional failures**

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

Expected: the new/replaced tests fail because the current output still uses a
`55 x 70` throat, a `4.0`-high funnel, `4.4` lower inset/rear wall, old pad
coordinates, and the old rear-hole section.

- [ ] **Step 4: Derive the new binding constants**

Keep mouth and outer values explicit while deriving throat expansion and lower
wall thickness:

```python
INNER_WIDTH = 40.0
INNER_LENGTH = 55.0
INNER_RADIUS = 1.6
MOUTH_WIDTH = 59.0
MOUTH_LENGTH = 74.0
MOUTH_RADIUS = 3.6
WALL = 2.4
FUNNEL_EXPANSION = (MOUTH_WIDTH - INNER_WIDTH) / 2.0
assert FUNNEL_EXPANSION == (MOUTH_LENGTH - INNER_LENGTH) / 2.0
OUTER_WIDTH = MOUTH_WIDTH + 2.0 * WALL
OUTER_LENGTH = MOUTH_LENGTH + 2.0 * WALL
OUTER_RADIUS = 6.0
OUTER_HEIGHT = 28.4
PAD_TOP = 13.4
FUNNEL_BOTTOM = PAD_TOP
FUNNEL_DEPTH = OUTER_HEIGHT - FUNNEL_BOTTOM
LOWER_INSET = (OUTER_WIDTH - INNER_WIDTH) / 2.0
assert LOWER_INSET == (OUTER_LENGTH - INNER_LENGTH) / 2.0
REAR_WALL_THICKNESS = LOWER_INSET
RING_SECTION_LEVEL = FUNNEL_BOTTOM + 0.001
```

Keep `PLATFORM_TOP=10.4`, `PLATFORM_BOTTOM=7.0`, centers `(31.9, 39.4)`, and
all canonical aperture values unchanged.

- [ ] **Step 5: Build an explicit mouth rather than a uniform offset**

Construct both production profiles explicitly and hull them:

```python
def inner_funnel_cutter() -> trimesh.Trimesh:
    throat = rounded_rectangle_section(
        INNER_WIDTH, INNER_LENGTH, INNER_RADIUS, (CENTER_X, CENTER_Y)
    )
    mouth = rounded_rectangle_section(
        MOUTH_WIDTH, MOUTH_LENGTH, MOUTH_RADIUS, (CENTER_X, CENTER_Y)
    )
    points = [
        [float(x), float(y), z]
        for section, z in ((throat, FUNNEL_BOTTOM), (mouth, OUTER_HEIGHT))
        for polygon in section.to_polygons()
        for x, y in polygon
    ]
    return manifold_to_mesh(manifold3d.Manifold.hull_points(points))
```

Subtract the full-height `40 x 55` throat cutter plus the ruled funnel from the
unchanged `63.8 x 78.8`, `R6` outer prism. The independent ring reference must
construct both profiles through `validation_rounded_rectangle_section()` and
must not call the production rounded-profile, ring, funnel, or base builders.

- [ ] **Step 6: Interpolate width, length, and radius independently**

Use one interpolation fraction but separate dimension/radius deltas:

```python
def funnel_section_dimensions(level: float) -> tuple[float, float, float]:
    fraction = (level - FUNNEL_BOTTOM) / FUNNEL_DEPTH
    return (
        INNER_WIDTH + fraction * (MOUTH_WIDTH - INNER_WIDTH),
        INNER_LENGTH + fraction * (MOUTH_LENGTH - INNER_LENGTH),
        INNER_RADIUS + fraction * (MOUTH_RADIUS - INNER_RADIUS),
    )
```

Validate the throat and at least three interior funnel levels plus the mouth.
Measure and validate the throat at `RING_SECTION_LEVEL`, where the `0.001`
rise remains inside the `0.003` absolute mesh tolerance. After the measured
check passes, report nominal `(INNER_WIDTH, INNER_LENGTH)` so the CLI contract
remains exactly `[40.0, 55.0]` instead of exposing the epsilon probe offset.

- [ ] **Step 7: Move lower structures and preserve open access**

Continue using `LOWER_INSET` for both axes, which now equals `11.9`. This moves
the pads to the throat corners, moves the rear inner face/rib endpoint to
`Y=66.9`, and makes the rear wall/hole path `11.9` thick.

Derive open-underside probes from the real gaps:

```python
side_open_x = (
    LOWER_INSET + 0.5,
    CENTER_X - PLATFORM_SIZE / 2.0 - 0.5,
)
front_open_y = (
    LOWER_INSET + PAD_SIZE + 0.5,
    CENTER_Y - PLATFORM_SIZE / 2.0 - 0.5,
)
rear_open_y = (
    CENTER_Y + PLATFORM_SIZE / 2.0 + 0.5,
    OUTER_LENGTH - LOWER_INSET - PAD_SIZE - 0.5,
)
```

Mirror `side_open_x` for the right side. Keep the central switch channel and
rear-wire probe open through the new rib/rear-wall geometry. Cut and validate
the rear bore with height `REAR_WALL_THICKNESS + 2.0`.

- [ ] **Step 8: Retain complete allowed-volume and adversarial validation**

Update the independent allowed union for the explicit throat/mouth hull, pad
locations, rib endpoint, and full rear bore. Keep the complete-model surplus
comparison and all existing floor, roof, wall-notch, protected-cell, channel,
profile, and hole mutation tests. Do not loosen
`REQUIRED_SOLID_VOLUME_TOLERANCE=0.03` unless a measured legal reference delta
exceeds it; if it does, record legal and destructive mutation volumes before
choosing the smallest justified tolerance.

- [ ] **Step 9: Run GREEN and focused regression gates**

Run the Step 3 command. Expected: at least `32 passed`, with no warnings.

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

- [ ] **Step 10: Commit the geometry change**

```bash
rtk git add scripts/telephone_handset_switch_base.py \
  test/test_telephone_handset_switch_base.py
rtk git commit -m "fix: narrow handset locating throat"
```

The commit must contain only the generator and its tests.

---

### Task 2: Regenerate And Inspect The Narrow-Throat Artifact

**Files:**

- Modify: `scripts/telephone_handset_switch_base.py` only if preview framing
  needs adjustment for the longer full-depth slopes.
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
`[40.0, 55.0]`, pocket depth `15.0`, one component, watertight/two-manifold
true, open underside true, rear path true, output path, and a new SHA-256.

- [ ] **Step 2: Reload the exported STL and validate it**

Load the STL with `process=False`, merge duplicate facet vertices on a copy,
and run `validate_base(reloaded, source)`. Assert one normalized body,
watertightness, two-face edge incidence, positive volume, and the revised
throat/funnel/platform/rear-wall contracts.

- [ ] **Step 3: Inspect all four regenerated previews**

Open:

```text
/tmp/kivo-handset-switch-base-previews/isometric.png
/tmp/kivo-handset-switch-base-previews/top.png
/tmp/kivo-handset-switch-base-previews/side-section.png
/tmp/kivo-handset-switch-base-previews/bottom.png
```

Reject and fix blank/cropped output, a blocked stepped aperture, a broad
floor/roof, disconnected tower/ribs, a rear bore that stops inside the thicker
wall, missing pads/gussets, or a side section that does not visibly show the
full-depth slopes plus the separate `Z=10.4` platform and `Z=13.4` pad datums.

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

Expected: focused suite at least `32 passed`, macro suite `36 passed`, all
static/diff checks exit `0`, and the generated STL remains ignored.

- [ ] **Step 5: Commit only a necessary preview fix**

If Task 2 required tracked preview/test changes, commit only the two owned
files with:

```bash
rtk git add scripts/telephone_handset_switch_base.py \
  test/test_telephone_handset_switch_base.py
rtk git commit -m "fix: frame narrow handset funnel previews"
```

If no tracked files changed, do not create an empty commit.

- [ ] **Step 6: Report the physical boundary**

Report mesh tests and visual preview inspection as complete. Keep slicer
toolpath, printed funnel fit, actual handset wedge height, simultaneous switch
bottom-out/four-pad contact, rear-wire fit, flat-surface stability, and
sustained-load behavior as Not Run.
