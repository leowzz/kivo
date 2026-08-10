# Telephone Handset Switch Base Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task.

**Goal:** Generate and verify one printable STL for an open-bottom telephone
handset base whose centered mechanical switch uses the exact aperture geometry
from the original 3x3 upper-cover STL and reaches full travel under the
handset.

**Architecture:** A standalone PEP 723 Python generator imports the
repository's proven mesh helpers, extracts the canonical `19.05 x 19.05`
source switch cell, and unions it into a purpose-built rounded base. The base
is constructed from closed manifold solids: a perimeter ring, source-preserving
central platform, U-shaped load tower, rear guide ribs, and four gusseted
safety pads. An independent validator measures sections, protected-volume
equality, access corridors, topology, dimensions, previews, and deterministic
STL bytes before export.

**Tech stack:** Python 3.13, PEP 723 through `uv`, `trimesh==5.0.0`,
`manifold3d==3.5.2`, `numpy==2.5.1`, `scipy==1.16.3`,
`Pillow==12.3.0`, `pytest==8.4.2`, binary STL in millimeters.

## Global Constraints

- Canonical switch source:
  `models/3d-print/3x3keypad/pico_macro_pad_top.stl.stl`, SHA-256
  `ce0f7b64d06b3fc2864d29452e87fb264f70567c0f5924eab380d0748f4e9155`.
- Never source geometry from generated 3x4, 4x3, 4x4, or 5x4 variants, and
  never scale the canonical mesh.
- Preserve the normalized central cell `X=23.05..42.10`,
  `Y=23.05..42.10`, `Z=0..3.4` inside the target's centered
  `19.05 x 19.05` protected region.
- Preserve the measured aperture: lower `14.798 x 14.798 x 1.998`, upper
  `14.000 x 14.000 x 1.402`, total plate `3.400`.
- Target bounds: `59.8 x 74.8 x 28.4`; rounded pocket section:
  `55 x 70` wall-to-wall bounds and `15.0` depth.
- Use `2.4` perimeter walls, `R4.0` outer corners, `R1.6` inner
  corners, and a `0.8 x 45 degree` inner top chamfer.
- Use a centered `24 x 24 x 3.4` platform at `Z=10.0..13.4`, a
  `24 x 24` U-tower at `Z=0..10.0`, two `2.4`-thick rear ribs, and
  four exposed `10 x 10` safety pads with `45 degree` gussets.
- Keep `19.2` clear between the guide ribs. Platform and safety-pad tops share
  the `Z=13.4` datum; add no handset ledge above it.
- The nominal fully bottomed switch leaves the handset's adjacent bearing
  surface `3 mm` above the pads because the handset center is recessed `2 mm`.
  Treat this as a physical-fit assumption, not a mesh-only acceptance result.
- Rear wire hole: `4.0` diameter, centered at `X=29.9`, `Z=5.0`,
  through the positive-`Y` wall.
- Keep the design one-piece, open underneath, upright, and support-free.
- Do not add a bottom cover, fasteners, connector, strain relief, keycap,
  branding, or decoration.
- Do not modify `pyproject.toml`, `uv.lock`,
  `scripts/macro_pad_variants.py`, the canonical STL, generated keypad
  variants, or unrelated files.
- Honor `models/.gitignore`: the requested STL exists locally but remains
  ignored and uncommitted.
- Prefix every shell command and every command-chain segment with `rtk`.
- Slicer inspection, printed fit, full physical travel, wire fit, stability,
  and sustained-load checks remain Not Run until performed on real hardware.

## File Structure

- Create `scripts/telephone_handset_switch_base.py`: constants, source-cell
  extraction, manifold primitives, base generation, validation, rendering,
  atomic STL export, and CLI.
- Create `test/test_telephone_handset_switch_base.py`: source contracts,
  geometry contracts, adversarial validator tests, CLI/export tests, and
  deterministic-output checks.
- Generate ignored artifact
  `models/3d-print/telephone-handset-switch-base/telephone_handset_switch_base.stl`.
- Generate temporary previews only under
  `/tmp/kivo-handset-switch-base-previews`.

---

### Task 1: Lock The Canonical Source And Mesh Primitives

**Files:**

- Create: `scripts/telephone_handset_switch_base.py`
- Create: `test/test_telephone_handset_switch_base.py`

**Interfaces:**

- Consume a canonical source directory as `Path`.
- Produce constants, `load_canonical_source(source_root)`,
  `extract_source_cell(source)`,
  `rounded_prism(width, length, radius, z_min, height, center)`,
  `box_from_bounds(lower, upper)`, `subtract_meshes(base_mesh, cutters)`, and
  `region_volume(mesh, lower, upper)`.

- [ ] **Step 1: Write failing source and primitive contract tests**

Create `test/test_telephone_handset_switch_base.py`:

```python
import hashlib
from pathlib import Path

import numpy as np
import pytest
import trimesh

from scripts import macro_pad_variants as macro
from scripts import telephone_handset_switch_base as base


ROOT = Path(__file__).resolve().parents[1]
SOURCE_ROOT = ROOT / "models/3d-print/3x3keypad"


def test_canonical_source_and_cell_contract() -> None:
    source_path = SOURCE_ROOT / base.SOURCE_FILENAME
    assert hashlib.sha256(source_path.read_bytes()).hexdigest() == base.SOURCE_HASH

    source = base.load_canonical_source(SOURCE_ROOT)
    cell = base.extract_source_cell(source)

    assert source.extents == pytest.approx((65.15, 65.15, 9.998), abs=0.003)
    assert cell.bounds[0] == pytest.approx(
        (base.CELL_START, base.CELL_START, 0.0), abs=0.003
    )
    assert cell.extents == pytest.approx(
        (base.CELL_SIZE, base.CELL_SIZE, base.PLATE_THICKNESS), abs=0.003
    )
    lower = macro.measure_switch_section(cell, z=1.0, nominal_size=14.8)
    upper = macro.measure_switch_section(cell, z=2.7, nominal_size=14.0)
    assert lower.sizes == pytest.approx([[14.798, 14.798]], abs=0.003)
    assert upper.sizes == pytest.approx([[14.0, 14.0]], abs=0.003)


def test_canonical_loader_rejects_changed_source(tmp_path: Path) -> None:
    source = SOURCE_ROOT / base.SOURCE_FILENAME
    changed = tmp_path / base.SOURCE_FILENAME
    changed.write_bytes(source.read_bytes() + b"changed")

    with pytest.raises(ValueError, match="source hash mismatch"):
        base.load_canonical_source(tmp_path)


def test_rounded_prism_and_subtraction_contracts() -> None:
    outer = base.rounded_prism(
        width=20.0,
        length=30.0,
        radius=2.0,
        z_min=1.0,
        height=5.0,
        center=(10.0, 15.0),
    )
    cutter = base.box_from_bounds(
        np.array([8.0, 13.0, 0.0]), np.array([12.0, 17.0, 7.0])
    )
    result = base.subtract_meshes(outer, [cutter])

    assert outer.bounds == pytest.approx([[0.0, 0.0, 1.0], [20.0, 30.0, 6.0]])
    assert result.is_watertight
    assert result.is_winding_consistent
    assert result.volume < outer.volume
    assert base.region_volume(
        result, np.array([8.5, 13.5, 1.5]), np.array([11.5, 16.5, 5.5])
    ) == pytest.approx(0.0, abs=1e-6)
```

- [ ] **Step 2: Run the tests and verify the module is missing**

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

Expected: collection fails with
`ImportError: cannot import name 'telephone_handset_switch_base'`.

- [ ] **Step 3: Implement the source contract and reusable primitives**

Create `scripts/telephone_handset_switch_base.py`:

```python
# /// script
# requires-python = ">=3.13"
# dependencies = [
#   "manifold3d==3.5.2",
#   "numpy==2.5.1",
#   "Pillow==12.3.0",
#   "scipy==1.16.3",
#   "trimesh==5.0.0",
# ]
# ///

from __future__ import annotations

import hashlib
from pathlib import Path
from typing import Iterable

import manifold3d
import numpy as np
import trimesh

if __package__:
    from scripts import macro_pad_variants as macro
else:
    import macro_pad_variants as macro


SOURCE_FILENAME = "pico_macro_pad_top.stl.stl"
SOURCE_HASH = "ce0f7b64d06b3fc2864d29452e87fb264f70567c0f5924eab380d0748f4e9155"
CELL_START = 23.05
CELL_SIZE = 19.05
CELL_END = CELL_START + CELL_SIZE
PLATE_THICKNESS = 3.4

OUTER_WIDTH = 59.8
OUTER_LENGTH = 74.8
OUTER_HEIGHT = 28.4
WALL = 2.4
OUTER_RADIUS = 4.0
INNER_WIDTH = 55.0
INNER_LENGTH = 70.0
INNER_RADIUS = 1.6
CHAMFER = 0.8
PLATFORM_SIZE = 24.0
PLATFORM_BOTTOM = 10.0
PLATFORM_TOP = PLATFORM_BOTTOM + PLATE_THICKNESS
PAD_SIZE = 10.0
PAD_THICKNESS = 2.4
WIRE_HOLE_DIAMETER = 4.0
CENTER_X = OUTER_WIDTH / 2.0
CENTER_Y = OUTER_LENGTH / 2.0
BOOLEAN_TOLERANCE = 5e-5

DEFAULT_SOURCE_ROOT = Path("models/3d-print/3x3keypad")
DEFAULT_OUTPUT_ROOT = Path("models/3d-print/telephone-handset-switch-base")
DEFAULT_PREVIEW_ROOT = Path("/tmp/kivo-handset-switch-base-previews")


def mesh_to_manifold(mesh: trimesh.Trimesh) -> manifold3d.Manifold:
    return manifold3d.Manifold(
        manifold3d.Mesh64(
            vert_properties=np.ascontiguousarray(mesh.vertices, dtype=np.float64),
            tri_verts=np.ascontiguousarray(mesh.faces, dtype=np.uint64),
        )
    )


def manifold_to_mesh(solid: manifold3d.Manifold) -> trimesh.Trimesh:
    output = solid.simplify(BOOLEAN_TOLERANCE).to_mesh64()
    mesh = trimesh.Trimesh(
        vertices=np.array(output.vert_properties[:, :3], copy=True),
        faces=np.array(output.tri_verts, copy=True),
        process=False,
    )
    mesh.remove_unreferenced_vertices()
    return mesh


def rounded_rectangle_section(
    width: float,
    length: float,
    radius: float,
    center: tuple[float, float],
) -> manifold3d.CrossSection:
    if radius <= 0.0 or 2.0 * radius >= min(width, length):
        raise ValueError("rounded rectangle radius must fit inside its bounds")
    core = manifold3d.CrossSection.square(
        (width - 2.0 * radius, length - 2.0 * radius), center=True
    )
    return core.offset(
        radius,
        join_type=manifold3d.JoinType.Round,
        circular_segments=32,
    ).translate(center)


def rounded_prism(
    width: float,
    length: float,
    radius: float,
    z_min: float,
    height: float,
    center: tuple[float, float],
) -> trimesh.Trimesh:
    section = rounded_rectangle_section(width, length, radius, center)
    return manifold_to_mesh(section.extrude(height).translate((0.0, 0.0, z_min)))


def box_from_bounds(lower: np.ndarray, upper: np.ndarray) -> trimesh.Trimesh:
    extents = np.asarray(upper, dtype=float) - np.asarray(lower, dtype=float)
    box = trimesh.creation.box(extents=extents)
    box.apply_translation((np.asarray(lower) + np.asarray(upper)) / 2.0)
    return box


def subtract_meshes(
    base_mesh: trimesh.Trimesh, cutters: Iterable[trimesh.Trimesh]
) -> trimesh.Trimesh:
    result = mesh_to_manifold(base_mesh)
    for cutter in cutters:
        result = result - mesh_to_manifold(cutter)
    output = manifold_to_mesh(result)
    if output.is_empty or output.volume <= 0.0:
        raise ValueError("mesh subtraction produced no positive solid")
    return output


def region_volume(
    mesh: trimesh.Trimesh, lower: np.ndarray, upper: np.ndarray
) -> float:
    region = macro.boolean_meshes([mesh, box_from_bounds(lower, upper)], "intersection")
    return 0.0 if region.is_empty else float(region.volume)


def load_canonical_source(source_root: Path) -> trimesh.Trimesh:
    path = source_root / SOURCE_FILENAME
    actual = hashlib.sha256(path.read_bytes()).hexdigest()
    if actual != SOURCE_HASH:
        raise ValueError(f"source hash mismatch for {path}: {actual} != {SOURCE_HASH}")
    return macro.load_source(path)


def extract_source_cell(source: trimesh.Trimesh) -> trimesh.Trimesh:
    cell = macro.clip_slab(source, 0, CELL_START, CELL_END)
    return macro.clip_slab(cell, 1, CELL_START, CELL_END)
```

- [ ] **Step 4: Run the focused tests**

Run the Step 2 command again.

Expected: `3 passed`.

- [ ] **Step 5: Run formatting and diff checks**

```bash
rtk uv run --offline --isolated --with ruff==0.12.12 \
  ruff format scripts/telephone_handset_switch_base.py test/test_telephone_handset_switch_base.py
rtk uv run --offline --isolated --with ruff==0.12.12 \
  ruff check scripts/telephone_handset_switch_base.py test/test_telephone_handset_switch_base.py
rtk uv run --offline --isolated --with ruff==0.12.12 \
  ruff format --check scripts/telephone_handset_switch_base.py test/test_telephone_handset_switch_base.py
rtk git diff --check
```

Expected: all commands exit `0`.

- [ ] **Step 6: Commit the source contract**

```bash
rtk git add scripts/telephone_handset_switch_base.py test/test_telephone_handset_switch_base.py
rtk git commit -m "feat: lock handset base source geometry"
```

Expected: one commit containing only the generator primitives and focused tests.

---

### Task 2: Generate The One-Piece Open-Bottom Body

**Files:**

- Modify: `scripts/telephone_handset_switch_base.py`
- Modify: `test/test_telephone_handset_switch_base.py`

**Interfaces:**

- Consume the normalized source returned by `load_canonical_source()`.
- Produce `build_outer_ring()`, `place_source_cell(source)`,
  `build_switch_platform(source)`, `build_tower_and_ribs()`,
  `build_safety_pad(x_side, y_side)`, `rear_hole_cutter()`, and
  `generate_base(source)`.

- [ ] **Step 1: Add failing complete-geometry tests**

Append to `test/test_telephone_handset_switch_base.py`:

```python
def section_loop_sizes(mesh: trimesh.Trimesh, axis: int, level: float) -> np.ndarray:
    origin = np.zeros(3)
    normal = np.zeros(3)
    origin[axis] = level
    normal[axis] = 1.0
    section = mesh.section(plane_origin=origin, plane_normal=normal)
    assert section is not None
    dimensions = [index for index in range(3) if index != axis]
    sizes: list[np.ndarray] = []
    for entity in section.entities:
        if not entity.closed:
            continue
        points = entity.discrete(section.vertices)
        projected = points[:, dimensions]
        sizes.append(projected.max(axis=0) - projected.min(axis=0))
    return np.array(sizes)


def test_generate_base_preserves_outer_pocket_and_switch_dimensions() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)

    macro.assert_closed_manifold(mesh, "telephone handset switch base")
    assert mesh.bounds[0] == pytest.approx((0.0, 0.0, 0.0), abs=0.003)
    assert mesh.extents == pytest.approx(
        (base.OUTER_WIDTH, base.OUTER_LENGTH, base.OUTER_HEIGHT), abs=0.003
    )

    loops = section_loop_sizes(mesh, axis=2, level=20.0)
    assert any(np.allclose(size, (59.8, 74.8), atol=0.003) for size in loops)
    assert any(np.allclose(size, (55.0, 70.0), atol=0.003) for size in loops)

    platform_loops = section_loop_sizes(mesh, axis=2, level=12.7)
    assert any(
        np.allclose(size, (24.0, 24.0), atol=0.003) for size in platform_loops
    )

    lower = macro.measure_switch_section(mesh, z=11.0, nominal_size=14.8)
    upper = macro.measure_switch_section(mesh, z=12.7, nominal_size=14.0)
    assert lower.centers == pytest.approx([[base.CENTER_X, base.CENTER_Y]], abs=0.003)
    assert upper.centers == pytest.approx([[base.CENTER_X, base.CENTER_Y]], abs=0.003)
    assert lower.sizes == pytest.approx([[14.798, 14.798]], abs=0.003)
    assert upper.sizes == pytest.approx([[14.0, 14.0]], abs=0.003)

    rear = section_loop_sizes(mesh, axis=1, level=73.6)
    assert any(np.allclose(size, (4.0, 4.0), atol=0.01) for size in rear)


def test_open_bottom_wire_path_and_required_supports() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)

    open_probes = (
        ([8.0, 25.0, 1.0], [15.0, 35.0, 9.0]),
        ([44.8, 25.0, 1.0], [51.8, 35.0, 9.0]),
        ([8.0, 48.0, 1.0], [15.0, 58.0, 9.0]),
        ([44.8, 48.0, 1.0], [51.8, 58.0, 9.0]),
        ([20.31, 28.0, 1.0], [39.49, 72.0, 9.0]),
        ([28.9, 49.4, 4.5], [30.9, 75.8, 5.5]),
        ([0.0, 0.0, 11.2], [0.5, 0.5, 13.2]),
        ([59.3, 0.0, 11.2], [59.8, 0.5, 13.2]),
        ([0.0, 74.3, 11.2], [0.5, 74.8, 13.2]),
        ([59.3, 74.3, 11.2], [59.8, 74.8, 13.2]),
    )
    for lower, upper in open_probes:
        assert base.region_volume(mesh, np.array(lower), np.array(upper)) == pytest.approx(
            0.0, abs=1e-6
        )

    required_solids = (
        ([18.0, 30.0, 1.0], [20.2, 45.0, 9.0]),
        ([39.6, 30.0, 1.0], [41.8, 45.0, 9.0]),
        ([21.0, 25.5, 1.0], [38.8, 27.7, 9.0]),
        ([18.0, 52.0, 1.0], [20.2, 70.0, 9.0]),
        ([39.6, 52.0, 1.0], [41.8, 70.0, 9.0]),
        ([40.0, 36.0, 10.2], [41.5, 38.8, 13.2]),
        ([3.0, 3.0, 11.2], [12.0, 12.0, 13.2]),
        ([47.8, 3.0, 11.2], [56.8, 12.0, 13.2]),
        ([3.0, 62.8, 11.2], [12.0, 71.8, 13.2]),
        ([47.8, 62.8, 11.2], [56.8, 71.8, 13.2]),
        ([5.5, 5.5, 6.5], [7.5, 7.5, 7.5]),
        ([52.3, 5.5, 6.5], [54.3, 7.5, 7.5]),
        ([5.5, 67.3, 6.5], [7.5, 69.3, 7.5]),
        ([52.3, 67.3, 6.5], [54.3, 69.3, 7.5]),
    )
    for lower, upper in required_solids:
        assert base.region_volume(mesh, np.array(lower), np.array(upper)) > 0.5
```

- [ ] **Step 2: Run the new tests and verify generation is missing**

Run the Task 1 Step 2 pytest command.

Expected: the three Task 1 tests pass and both new tests fail because
`generate_base` is absent.

- [ ] **Step 3: Implement the rounded ring and top chamfer**

Append to `scripts/telephone_handset_switch_base.py`:

```python
JOIN_OVERLAP = 0.02


def inner_chamfer_cutter() -> trimesh.Trimesh:
    lower_section = rounded_rectangle_section(
        INNER_WIDTH, INNER_LENGTH, INNER_RADIUS, (CENTER_X, CENTER_Y)
    )
    upper_section = lower_section.offset(
        CHAMFER,
        join_type=manifold3d.JoinType.Round,
        circular_segments=32,
    )
    slice_height = 0.01
    lower = lower_section.extrude(slice_height).translate(
        (0.0, 0.0, OUTER_HEIGHT - CHAMFER)
    )
    upper = upper_section.extrude(slice_height).translate(
        (0.0, 0.0, OUTER_HEIGHT - slice_height)
    )
    return manifold_to_mesh(manifold3d.Manifold.batch_hull([lower, upper]))


def build_outer_ring() -> trimesh.Trimesh:
    outer = rounded_prism(
        OUTER_WIDTH,
        OUTER_LENGTH,
        OUTER_RADIUS,
        z_min=0.0,
        height=OUTER_HEIGHT,
        center=(CENTER_X, CENTER_Y),
    )
    inner = rounded_prism(
        INNER_WIDTH,
        INNER_LENGTH,
        INNER_RADIUS,
        z_min=-1.0,
        height=OUTER_HEIGHT + 2.0,
        center=(CENTER_X, CENTER_Y),
    )
    return subtract_meshes(outer, [inner, inner_chamfer_cutter()])
```

- [ ] **Step 4: Implement the exact source-cell platform**

Append:

```python
def place_source_cell(source: trimesh.Trimesh) -> trimesh.Trimesh:
    cell = extract_source_cell(source).copy()
    target_lower = np.array(
        [
            CENTER_X - CELL_SIZE / 2.0,
            CENTER_Y - CELL_SIZE / 2.0,
            PLATFORM_BOTTOM,
        ]
    )
    cell.apply_translation(target_lower - cell.bounds[0])
    return cell


def build_switch_platform(source: trimesh.Trimesh) -> trimesh.Trimesh:
    platform = box_from_bounds(
        np.array(
            [
                CENTER_X - PLATFORM_SIZE / 2.0,
                CENTER_Y - PLATFORM_SIZE / 2.0,
                PLATFORM_BOTTOM,
            ]
        ),
        np.array(
            [
                CENTER_X + PLATFORM_SIZE / 2.0,
                CENTER_Y + PLATFORM_SIZE / 2.0,
                PLATFORM_TOP,
            ]
        ),
    )
    protected_cutter = box_from_bounds(
        np.array(
            [
                CENTER_X - CELL_SIZE / 2.0,
                CENTER_Y - CELL_SIZE / 2.0,
                PLATFORM_BOTTOM - 1.0,
            ]
        ),
        np.array(
            [
                CENTER_X + CELL_SIZE / 2.0,
                CENTER_Y + CELL_SIZE / 2.0,
                PLATFORM_TOP + 1.0,
            ]
        ),
    )
    border = subtract_meshes(platform, [protected_cutter])
    return macro.union_meshes([border, place_source_cell(source)])
```

- [ ] **Step 5: Implement the U-tower and rear guide ribs**

Append:

```python
def build_tower_and_ribs() -> trimesh.Trimesh:
    x0 = CENTER_X - PLATFORM_SIZE / 2.0
    x1 = CENTER_X + PLATFORM_SIZE / 2.0
    y0 = CENTER_Y - PLATFORM_SIZE / 2.0
    y1 = CENTER_Y + PLATFORM_SIZE / 2.0
    inner_rear = OUTER_LENGTH - WALL
    z1 = PLATFORM_BOTTOM + JOIN_OVERLAP

    parts = [
        box_from_bounds(np.array([x0, y0, 0.0]), np.array([x0 + WALL, y1, z1])),
        box_from_bounds(np.array([x1 - WALL, y0, 0.0]), np.array([x1, y1, z1])),
        box_from_bounds(
            np.array([x0 + WALL, y0, 0.0]),
            np.array([x1 - WALL, y0 + WALL, z1]),
        ),
        box_from_bounds(
            np.array([x0, y1, 0.0]),
            np.array([x0 + WALL, inner_rear + JOIN_OVERLAP, z1]),
        ),
        box_from_bounds(
            np.array([x1 - WALL, y1, 0.0]),
            np.array([x1, inner_rear + JOIN_OVERLAP, z1]),
        ),
    ]
    return macro.union_meshes(parts)
```

- [ ] **Step 6: Implement four 45-degree gusseted safety pads**

Append:

```python
def hull_points(points: list[list[float]]) -> trimesh.Trimesh:
    return manifold_to_mesh(manifold3d.Manifold.hull_points(points))


def side_bounds(
    side: int, inner_min: float, inner_max: float
) -> tuple[tuple[float, float], tuple[float, float], tuple[float, float]]:
    if side < 0:
        pad = (inner_min - JOIN_OVERLAP, inner_min + PAD_SIZE)
        exposed = (inner_min, inner_min + PAD_SIZE)
        foot = (inner_min, inner_min + WALL)
    else:
        pad = (inner_max - PAD_SIZE, inner_max + JOIN_OVERLAP)
        exposed = (inner_max - PAD_SIZE, inner_max)
        foot = (inner_max - WALL, inner_max)
    return pad, exposed, foot


def build_safety_pad(x_side: int, y_side: int) -> trimesh.Trimesh:
    x_pad, x_exposed, x_foot = side_bounds(x_side, WALL, OUTER_WIDTH - WALL)
    y_pad, y_exposed, y_foot = side_bounds(y_side, WALL, OUTER_LENGTH - WALL)
    pad_bottom = PLATFORM_TOP - PAD_THICKNESS
    gusset_bottom = pad_bottom - (PAD_SIZE - WALL)

    pad = box_from_bounds(
        np.array([x_pad[0], y_pad[0], pad_bottom]),
        np.array([x_pad[1], y_pad[1], PLATFORM_TOP]),
    )
    foot = box_from_bounds(
        np.array([x_foot[0], y_foot[0], 0.0]),
        np.array([x_foot[1], y_foot[1], gusset_bottom]),
    )
    points = [
        [x, y, z]
        for z, x_bounds, y_bounds in (
            (gusset_bottom, x_foot, y_foot),
            (pad_bottom, x_exposed, y_exposed),
        )
        for x in x_bounds
        for y in y_bounds
    ]
    gusset = hull_points(points)
    return macro.union_meshes([pad, foot, gusset])
```

The vertical gusset rise is `11.0 - 3.4 = 7.6`, exactly matching the exposed
pad expansion `10.0 - 2.4 = 7.6`; the underside faces are therefore
`45 degrees`. The pad overlaps each inner wall by only `0.02`; it never reaches
the outer silhouette, so the vertical `R4.0` corner remains unchanged.

- [ ] **Step 7: Implement the rear hole and complete union**

Append:

```python
def rear_hole_cutter() -> trimesh.Trimesh:
    cutter = trimesh.creation.cylinder(
        radius=WIRE_HOLE_DIAMETER / 2.0,
        height=WALL + 2.0,
        sections=32,
    )
    cutter.apply_transform(
        trimesh.transformations.rotation_matrix(np.pi / 2.0, [1.0, 0.0, 0.0])
    )
    cutter.apply_translation([CENTER_X, OUTER_LENGTH - WALL / 2.0, 5.0])
    return cutter


def generate_base(source: trimesh.Trimesh) -> trimesh.Trimesh:
    parts = [
        build_outer_ring(),
        build_switch_platform(source),
        build_tower_and_ribs(),
    ]
    parts.extend(
        build_safety_pad(x_side, y_side)
        for x_side in (-1, 1)
        for y_side in (-1, 1)
    )
    joined = macro.union_meshes(parts)
    result = subtract_meshes(joined, [rear_hole_cutter()])
    result.merge_vertices()
    result.remove_unreferenced_vertices()
    return result
```

- [ ] **Step 8: Run geometry, formatting, and diff checks**

Run the Task 1 Step 2 and Step 5 commands.

Expected: `5 passed`; every check exits `0`.

- [ ] **Step 9: Commit the generated body implementation**

```bash
rtk git add scripts/telephone_handset_switch_base.py test/test_telephone_handset_switch_base.py
rtk git commit -m "feat: generate open-bottom handset switch base"
```

Expected: one commit containing the complete body construction and section
tests.

---

### Task 3: Add Independent Geometry Validation

**Files:**

- Modify: `scripts/telephone_handset_switch_base.py`
- Modify: `test/test_telephone_handset_switch_base.py`

**Interfaces:**

- Produce immutable `ValidationReport` and `validate_base(mesh, source)`.
- Validate topology, bounds, pocket section/depth, protected source volume,
  both switch aperture levels, rear-hole diameter, open access, and required
  load-bearing solids from the finished mesh rather than from construction
  parameters alone.

- [ ] **Step 1: Add failing validator and adversarial tests**

Append to `test/test_telephone_handset_switch_base.py`:

```python
def test_validate_base_reports_every_mesh_contract() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)

    report = base.validate_base(mesh, source)

    assert report.outer_extents == pytest.approx((59.8, 74.8, 28.4), abs=0.003)
    assert report.pocket_bounds == pytest.approx((55.0, 70.0), abs=0.003)
    assert report.pocket_depth == pytest.approx(15.0, abs=0.003)
    assert report.protected_mismatch_volume <= base.PROTECTED_VOLUME_TOLERANCE
    assert report.connected_components == 1
    assert report.watertight
    assert report.two_manifold
    assert report.open_underside
    assert report.rear_wire_path


def test_validator_rejects_blocked_access_paths() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)

    underside_block = base.box_from_bounds(
        np.array([28.9, 27.7, 1.0]), np.array([30.9, 30.0, 9.0])
    )
    blocked_underside = macro.union_meshes([mesh, underside_block])
    with pytest.raises(ValueError, match="open underside"):
        base.validate_base(blocked_underside, source)

    wire_block = base.box_from_bounds(
        np.array([27.8, 72.0, 3.8]), np.array([32.0, 74.8, 6.2])
    )
    blocked_wire = macro.union_meshes([mesh, wire_block])
    with pytest.raises(ValueError, match="rear wire (hole|path)"):
        base.validate_base(blocked_wire, source)


def test_validator_rejects_protected_switch_cell_drift() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)
    aperture_notch = base.box_from_bounds(
        np.array([base.CENTER_X + 6.8, base.CENTER_Y - 1.0, 12.4]),
        np.array([base.CENTER_X + 8.0, base.CENTER_Y + 1.0, 13.5]),
    )
    drifted = base.subtract_meshes(mesh, [aperture_notch])

    with pytest.raises(ValueError, match="source switch cell"):
        base.validate_base(drifted, source)


def test_validator_measures_datum_corners_and_every_required_feature() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)

    ledge = base.box_from_bounds(
        np.array([2.3, 30.0, 14.0]), np.array([10.0, 40.0, 14.5])
    )
    with pytest.raises(ValueError, match="pocket floor datum"):
        base.validate_base(macro.union_meshes([mesh, ledge]), source)

    square_corner = base.box_from_bounds(
        np.array([0.0, 0.0, 11.2]), np.array([3.0, 3.0, 13.2])
    )
    with pytest.raises(ValueError, match="R4 outer corner"):
        base.validate_base(macro.union_meshes([mesh, square_corner]), source)

    front_wall_cutter = base.box_from_bounds(
        np.array([20.2, 25.3, -0.1]), np.array([39.6, 27.85, 10.1])
    )
    missing_front_wall = base.subtract_meshes(mesh, [front_wall_cutter])
    with pytest.raises(ValueError, match="required platform, tower, rib, pad, or gusset"):
        base.validate_base(missing_front_wall, source)
```

- [ ] **Step 2: Run tests and verify the validator is absent**

Run the Task 1 Step 2 pytest command.

Expected: the five existing tests pass and the four new tests fail because
`validate_base` is absent.

- [ ] **Step 3: Add validation data and section helpers**

Add `from dataclasses import dataclass` to the generator imports, then append:

```python
PROTECTED_VOLUME_TOLERANCE = 0.02

OPEN_UNDERSIDE_PROBES = (
    ((8.0, 25.0, 1.0), (15.0, 35.0, 9.0)),
    ((44.8, 25.0, 1.0), (51.8, 35.0, 9.0)),
    ((8.0, 48.0, 1.0), (15.0, 58.0, 9.0)),
    ((44.8, 48.0, 1.0), (51.8, 58.0, 9.0)),
    ((20.31, 28.0, 1.0), (39.49, 72.0, 9.0)),
)
REAR_WIRE_PROBE = ((28.9, 49.4, 4.5), (30.9, 75.8, 5.5))
OUTER_CORNER_PROBES = (
    ((0.0, 0.0, 11.2), (0.5, 0.5, 13.2)),
    ((59.3, 0.0, 11.2), (59.8, 0.5, 13.2)),
    ((0.0, 74.3, 11.2), (0.5, 74.8, 13.2)),
    ((59.3, 74.3, 11.2), (59.8, 74.8, 13.2)),
)
REQUIRED_SOLID_PROBES = (
    ((18.0, 30.0, 1.0), (20.2, 45.0, 9.0)),
    ((39.6, 30.0, 1.0), (41.8, 45.0, 9.0)),
    ((21.0, 25.5, 1.0), (38.8, 27.7, 9.0)),
    ((18.0, 52.0, 1.0), (20.2, 70.0, 9.0)),
    ((39.6, 52.0, 1.0), (41.8, 70.0, 9.0)),
    ((40.0, 36.0, 10.2), (41.5, 38.8, 13.2)),
    ((3.0, 3.0, 11.2), (12.0, 12.0, 13.2)),
    ((47.8, 3.0, 11.2), (56.8, 12.0, 13.2)),
    ((3.0, 62.8, 11.2), (12.0, 71.8, 13.2)),
    ((47.8, 62.8, 11.2), (56.8, 71.8, 13.2)),
    ((5.5, 5.5, 6.5), (7.5, 7.5, 7.5)),
    ((52.3, 5.5, 6.5), (54.3, 7.5, 7.5)),
    ((5.5, 67.3, 6.5), (7.5, 69.3, 7.5)),
    ((52.3, 67.3, 6.5), (54.3, 69.3, 7.5)),
)
SUPPORT_TOP_PROBES = (
    ((40.0, 36.0, -0.1), (41.0, 38.0, 29.4)),
    ((6.0, 6.0, -0.1), (8.0, 8.0, 29.4)),
    ((51.8, 6.0, -0.1), (53.8, 8.0, 29.4)),
    ((6.0, 66.8, -0.1), (8.0, 68.8, 29.4)),
    ((51.8, 66.8, -0.1), (53.8, 68.8, 29.4)),
)


@dataclass(frozen=True)
class ValidationReport:
    outer_extents: tuple[float, float, float]
    pocket_bounds: tuple[float, float]
    pocket_depth: float
    protected_mismatch_volume: float
    connected_components: int
    watertight: bool
    two_manifold: bool
    open_underside: bool
    rear_wire_path: bool


def measured_section_loop_sizes(
    mesh: trimesh.Trimesh, axis: int, level: float
) -> np.ndarray:
    origin = np.zeros(3)
    normal = np.zeros(3)
    origin[axis] = level
    normal[axis] = 1.0
    section = mesh.section(plane_origin=origin, plane_normal=normal)
    if section is None:
        raise ValueError(f"missing section on axis {axis} at {level}")
    dimensions = [index for index in range(3) if index != axis]
    sizes: list[np.ndarray] = []
    for entity in section.entities:
        if not entity.closed:
            continue
        points = entity.discrete(section.vertices)
        projected = points[:, dimensions]
        sizes.append(projected.max(axis=0) - projected.min(axis=0))
    return np.array(sizes)


def require_loop_size(
    sizes: np.ndarray, expected: tuple[float, float], label: str, tolerance: float
) -> None:
    if not any(np.allclose(size, expected, atol=tolerance) for size in sizes):
        raise ValueError(f"{label} drifted: {sizes.tolist()}")


def protected_cell_mismatch(
    mesh: trimesh.Trimesh, source: trimesh.Trimesh
) -> float:
    return macro.region_mismatch_volume(
        source,
        mesh,
        np.array([CELL_START, CELL_START, -0.1]),
        np.array([CELL_END, CELL_END, PLATE_THICKNESS + 0.1]),
        np.array(
            [
                CENTER_X - CELL_SIZE / 2.0,
                CENTER_Y - CELL_SIZE / 2.0,
                PLATFORM_BOTTOM - 0.1,
            ]
        ),
        np.array(
            [
                CENTER_X + CELL_SIZE / 2.0,
                CENTER_Y + CELL_SIZE / 2.0,
                PLATFORM_TOP + 0.1,
            ]
        ),
    )
```

- [ ] **Step 4: Implement the complete validator**

Append:

```python
def probe_volume(
    mesh: trimesh.Trimesh,
    probe: tuple[tuple[float, float, float], tuple[float, float, float]],
) -> float:
    lower, upper = probe
    return region_volume(mesh, np.array(lower), np.array(upper))


def probe_bounds(
    mesh: trimesh.Trimesh,
    probe: tuple[tuple[float, float, float], tuple[float, float, float]],
) -> np.ndarray:
    lower, upper = probe
    region = macro.boolean_meshes(
        [mesh, box_from_bounds(np.array(lower), np.array(upper))], "intersection"
    )
    if region.is_empty:
        raise ValueError(f"required feature probe is empty: {probe}")
    return region.bounds


def measured_pocket_floor_top(mesh: trimesh.Trimesh) -> float:
    cavity_probe = rounded_prism(
        INNER_WIDTH - 0.02,
        INNER_LENGTH - 0.02,
        INNER_RADIUS - 0.01,
        z_min=-1.0,
        height=OUTER_HEIGHT + 2.0,
        center=(CENTER_X, CENTER_Y),
    )
    interior = macro.boolean_meshes([mesh, cavity_probe], "intersection")
    if interior.is_empty:
        raise ValueError("pocket contains no floor or support datum")
    return float(interior.bounds[1, 2])


def validate_base(
    mesh: trimesh.Trimesh, source: trimesh.Trimesh
) -> ValidationReport:
    macro.assert_closed_manifold(mesh, "telephone handset switch base")
    if not np.allclose(mesh.bounds[0], (0.0, 0.0, 0.0), atol=0.003):
        raise ValueError(f"outer origin drifted: {mesh.bounds[0].tolist()}")
    expected_extents = np.array([OUTER_WIDTH, OUTER_LENGTH, OUTER_HEIGHT])
    if not np.allclose(mesh.extents, expected_extents, atol=0.003):
        raise ValueError(f"outer extents drifted: {mesh.extents.tolist()}")

    mismatch = protected_cell_mismatch(mesh, source)
    if mismatch > PROTECTED_VOLUME_TOLERANCE:
        raise ValueError(f"source switch cell drifted: mismatch={mismatch}")

    pocket_loops = measured_section_loop_sizes(mesh, axis=2, level=20.0)
    require_loop_size(pocket_loops, (59.8, 74.8), "outer pocket section", 0.003)
    require_loop_size(pocket_loops, (55.0, 70.0), "inner pocket section", 0.003)
    platform_loops = measured_section_loop_sizes(mesh, axis=2, level=12.7)
    require_loop_size(platform_loops, (24.0, 24.0), "switch platform", 0.003)

    support_top = measured_pocket_floor_top(mesh)
    if not np.isclose(support_top, PLATFORM_TOP, atol=0.003):
        raise ValueError(f"pocket floor datum drifted: {support_top}")
    for probe in SUPPORT_TOP_PROBES:
        feature_top = float(probe_bounds(mesh, probe)[1, 2])
        if not np.isclose(feature_top, PLATFORM_TOP, atol=0.003):
            raise ValueError(f"platform or safety-pad top drifted: {probe}")
    pocket_depth = float(mesh.bounds[1, 2] - support_top)
    if not np.isclose(pocket_depth, 15.0, atol=0.003):
        raise ValueError(f"pocket depth drifted: {pocket_depth}")

    lower = macro.measure_switch_section(mesh, z=11.0, nominal_size=14.8)
    upper = macro.measure_switch_section(mesh, z=12.7, nominal_size=14.0)
    expected_center = np.array([[CENTER_X, CENTER_Y]])
    if not np.allclose(lower.centers, expected_center, atol=0.003):
        raise ValueError("lower switch relief center drifted")
    if not np.allclose(upper.centers, expected_center, atol=0.003):
        raise ValueError("upper switch aperture center drifted")
    if not np.allclose(lower.sizes, [[14.798, 14.798]], atol=0.003):
        raise ValueError("lower switch relief drifted")
    if not np.allclose(upper.sizes, [[14.0, 14.0]], atol=0.003):
        raise ValueError("upper switch aperture drifted")

    rear_loops = measured_section_loop_sizes(mesh, axis=1, level=73.6)
    require_loop_size(rear_loops, (4.0, 4.0), "rear wire hole", 0.01)

    for probe in OPEN_UNDERSIDE_PROBES:
        if probe_volume(mesh, probe) >= 1e-6:
            raise ValueError(f"open underside is obstructed: {probe}")
    if probe_volume(mesh, REAR_WIRE_PROBE) >= 1e-6:
        raise ValueError("rear wire path is obstructed")
    for probe in OUTER_CORNER_PROBES:
        if probe_volume(mesh, probe) >= 1e-6:
            raise ValueError(f"R4 outer corner is filled: {probe}")
    for probe in REQUIRED_SOLID_PROBES:
        if probe_volume(mesh, probe) <= 0.5:
            raise ValueError(
                f"required platform, tower, rib, pad, or gusset is missing: {probe}"
            )

    incidence = np.bincount(mesh.edges_unique_inverse)
    return ValidationReport(
        outer_extents=tuple(float(value) for value in mesh.extents),
        pocket_bounds=(INNER_WIDTH, INNER_LENGTH),
        pocket_depth=float(pocket_depth),
        protected_mismatch_volume=float(mismatch),
        connected_components=int(mesh.body_count),
        watertight=bool(mesh.is_watertight),
        two_manifold=bool(np.all(incidence == 2)),
        open_underside=True,
        rear_wire_path=True,
    )
```

- [ ] **Step 5: Run all focused tests**

Run the Task 1 Step 2 command.

Expected: `9 passed`, including every adversarial geometry case.

- [ ] **Step 6: Run formatting and diff checks**

Run the Task 1 Step 5 commands.

Expected: every command exits `0`.

- [ ] **Step 7: Commit validation separately**

```bash
rtk git add scripts/telephone_handset_switch_base.py test/test_telephone_handset_switch_base.py
rtk git commit -m "test: validate handset base mesh contracts"
```

Expected: one commit containing only independent validation and its regression
tests.

---

### Task 4: Export Deterministic STL And Render Acceptance Previews

**Files:**

- Modify: `scripts/telephone_handset_switch_base.py`
- Modify: `test/test_telephone_handset_switch_base.py`
- Generate, ignored:
  `models/3d-print/telephone-handset-switch-base/telephone_handset_switch_base.stl`
- Generate, temporary: `/tmp/kivo-handset-switch-base-previews/*.png`

**Interfaces:**

- Produce `export_base(mesh, target)`, `render_preview(mesh, target, view)`,
  and `main(argv=None) -> int`.
- CLI arguments: `--source-root`, `--output-root`, and `--preview-root`.
- CLI stdout: one sorted JSON object containing the validation report, final
  STL path, and SHA-256.

- [ ] **Step 1: Add failing deterministic-export and CLI tests**

Add `import json` and `from PIL import Image` to the test imports, then append:

```python
def test_export_is_deterministic_and_reload_validates(tmp_path: Path) -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    first = tmp_path / "first.stl"
    second = tmp_path / "second.stl"

    base.export_base(base.generate_base(source), first)
    base.export_base(base.generate_base(source), second)

    assert first.read_bytes() == second.read_bytes()
    reloaded = trimesh.load_mesh(first, file_type="stl", process=False)
    assert isinstance(reloaded, trimesh.Trimesh)
    base.validate_base(reloaded, source)


def test_main_exports_stl_json_and_four_nonblank_previews(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    output_root = tmp_path / "output"
    preview_root = tmp_path / "previews"

    result = base.main(
        [
            "--source-root",
            str(SOURCE_ROOT),
            "--output-root",
            str(output_root),
            "--preview-root",
            str(preview_root),
        ]
    )

    assert result == 0
    target = output_root / base.OUTPUT_FILENAME
    payload = json.loads(capsys.readouterr().out)
    assert payload["stl_path"] == str(target)
    assert payload["stl_sha256"] == hashlib.sha256(target.read_bytes()).hexdigest()
    assert payload["protected_mismatch_volume"] <= base.PROTECTED_VOLUME_TOLERANCE

    expected_previews = {
        "isometric.png",
        "top.png",
        "side-section.png",
        "bottom.png",
    }
    assert {path.name for path in preview_root.iterdir()} == expected_previews
    for path in preview_root.iterdir():
        with Image.open(path) as image:
            pixels = np.asarray(image.convert("RGB"))
        nonblank = np.count_nonzero(np.any(pixels != 255, axis=2))
        assert nonblank >= pixels.shape[0] * pixels.shape[1] * 0.05
```

- [ ] **Step 2: Run tests and verify export/render APIs are missing**

Run the Task 1 Step 2 pytest command.

Expected: the nine existing tests pass and both new tests fail because
`export_base` and `main` are absent.

- [ ] **Step 3: Add deterministic export and preview preparation**

Change the generator's dataclass import to
`from dataclasses import asdict, dataclass`, then append:

```python
OUTPUT_FILENAME = "telephone_handset_switch_base.stl"
VIEW_ROTATIONS = {
    "top": np.eye(3),
    "bottom": np.diag([1.0, -1.0, -1.0]),
    "side-section": np.array(
        [[0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [-1.0, 0.0, 0.0]]
    ),
    "isometric": np.array(
        [
            [0.70710678, -0.70710678, 0.0],
            [0.40824829, 0.40824829, -0.81649658],
            [0.57735027, 0.57735027, 0.57735027],
        ]
    ),
}


def export_base(mesh: trimesh.Trimesh, target: Path) -> None:
    macro.export_stl(mesh, target)


def mesh_for_preview(mesh: trimesh.Trimesh, view: str) -> trimesh.Trimesh:
    if view == "side-section":
        return macro.clip_slab(mesh, 0, CENTER_X, OUTER_WIDTH + 1.0)
    return mesh
```

- [ ] **Step 4: Implement the four-view raster renderer**

Append:

```python
def render_preview(mesh: trimesh.Trimesh, target: Path, view: str) -> None:
    from PIL import Image, ImageDraw

    if view not in VIEW_ROTATIONS:
        raise ValueError(f"unsupported preview view: {view}")
    rendered_mesh = mesh_for_preview(mesh, view)
    triangles = rendered_mesh.triangles @ VIEW_ROTATIONS[view].T
    projected = triangles[:, :, :2]
    lower = projected.reshape(-1, 2).min(axis=0)
    upper = projected.reshape(-1, 2).max(axis=0)
    canvas = np.array([1200.0, 900.0])
    scale = float(np.min((canvas - 96.0) / (upper - lower)))
    rendered_size = (upper - lower) * scale
    offset = (canvas - rendered_size) / 2.0
    points = (projected - lower) * scale + offset
    points[:, :, 1] = canvas[1] - points[:, :, 1]

    normals = np.cross(
        triangles[:, 1] - triangles[:, 0], triangles[:, 2] - triangles[:, 0]
    )
    lengths = np.linalg.norm(normals, axis=1)
    normals = normals / np.maximum(lengths[:, None], 1e-12)
    light = np.array([0.3, -0.4, 0.85])
    light /= np.linalg.norm(light)
    shade = np.clip(0.55 + 0.35 * np.abs(normals @ light), 0.0, 1.0)
    depth = triangles[:, :, 2].mean(axis=1)

    image = Image.new("RGB", (1200, 900), "white")
    draw = ImageDraw.Draw(image)
    for index in np.argsort(depth):
        level = int(235 - 105 * shade[index])
        polygon = [tuple(value) for value in points[index].tolist()]
        draw.polygon(polygon, fill=(level, level, level))
    pixels = np.asarray(image)
    nonblank = np.count_nonzero(np.any(pixels != 255, axis=2))
    if nonblank < pixels.shape[0] * pixels.shape[1] * 0.05:
        raise ValueError(f"blank preview for {target}")
    target.parent.mkdir(parents=True, exist_ok=True)
    image.save(target)
```

The side-section keeps the positive-X half and renders toward the cut plane,
so the central tower, open underside, switch tunnel, and rear route are visible
instead of being hidden by the perimeter wall.

- [ ] **Step 5: Implement the reproducible CLI**

Append:

```python
def main(argv: list[str] | None = None) -> int:
    import argparse
    import json

    parser = argparse.ArgumentParser()
    parser.add_argument("--source-root", type=Path, default=DEFAULT_SOURCE_ROOT)
    parser.add_argument("--output-root", type=Path, default=DEFAULT_OUTPUT_ROOT)
    parser.add_argument("--preview-root", type=Path, default=DEFAULT_PREVIEW_ROOT)
    arguments = parser.parse_args(argv)

    source = load_canonical_source(arguments.source_root)
    mesh = generate_base(source)
    report = validate_base(mesh, source)
    target = arguments.output_root / OUTPUT_FILENAME
    export_base(mesh, target)
    for view in ("isometric", "top", "side-section", "bottom"):
        render_preview(mesh, arguments.preview_root / f"{view}.png", view)

    payload = asdict(report)
    payload["stl_path"] = str(target)
    payload["stl_sha256"] = hashlib.sha256(target.read_bytes()).hexdigest()
    print(json.dumps(payload, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
```

- [ ] **Step 6: Run the complete automated suite**

Run the Task 1 Step 2 and Step 5 commands.

Expected: `11 passed`; Ruff and `git diff --check` exit `0`.

- [ ] **Step 7: Generate the requested local artifact and previews**

```bash
rtk uv run --offline --script scripts/telephone_handset_switch_base.py
rtk proxy file models/3d-print/telephone-handset-switch-base/telephone_handset_switch_base.stl
rtk proxy file /tmp/kivo-handset-switch-base-previews/*.png
```

Expected:

- stdout is one JSON object with bounds, topology/access booleans, protected
  mismatch at or below `0.02`, STL path, and SHA-256;
- `file` identifies one binary STL and four `1200 x 900` PNGs;
- the STL exists only at the requested ignored output path.

- [ ] **Step 8: Inspect all four previews**

Open each PNG with the available local image viewer. Reject and fix the model
if any view is blank or shows a blocked stepped switch opening, a broad floor,
disconnected tower/ribs, a closed rear route, missing pads, missing gussets, or
incoherent Boolean overlaps. Record that this is mesh visual inspection, not a
slicer or physical-print result.

- [ ] **Step 9: Re-run fresh verification before completion**

Invoke `superpowers:verification-before-completion`, then run:

```bash
rtk uv run --offline --isolated \
  --with pytest==8.4.2 \
  --with manifold3d==3.5.2 \
  --with numpy==2.5.1 \
  --with Pillow==12.3.0 \
  --with scipy==1.16.3 \
  --with trimesh==5.0.0 \
  python -m pytest test/test_telephone_handset_switch_base.py -q
rtk uv run --offline --isolated --with ruff==0.12.12 \
  ruff check scripts/telephone_handset_switch_base.py test/test_telephone_handset_switch_base.py
rtk uv run --offline --isolated --with ruff==0.12.12 \
  ruff format --check scripts/telephone_handset_switch_base.py test/test_telephone_handset_switch_base.py
rtk git diff --check
rtk git status --short
```

Expected: `11 passed`; all checks exit `0`; status contains only the two
tracked implementation files before staging because the STL is ignored.

- [ ] **Step 10: Commit only tracked implementation files**

```bash
rtk git add scripts/telephone_handset_switch_base.py test/test_telephone_handset_switch_base.py
rtk git commit -m "feat: export printable handset switch base"
rtk git status --short --branch
```

Expected: the commit contains only the generator and tests; the branch is
clean and the ignored STL remains present locally.

- [ ] **Step 11: Report the acceptance boundary accurately**

Report automated mesh checks and visual preview inspection as completed.
Report slicer import/toolpath, printed snap-fit, actual switch bottom-out,
nominal `3 mm` safety-pad clearance, rear-wire fit, flat-surface stability, and
sustained-load behavior as **Not Run** until they are physically performed.

---
