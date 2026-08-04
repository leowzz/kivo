# Keypad Case Size Variants Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Generate verified top and bottom STL pairs for 3x4, 4x4, and 5x4 YD-RP2040 macro-pad cases while preserving every functional dimension from the 3x3 source.

**Architecture:** A reproducible Python mesh generator loads and rotates each source STL so the Type-C edge is the fixed top datum. It repeats complete 19.05 mm switch-cell bands for the top; for the bottom it separates a rigid internal functional core, expands only the base and perimeter shell, then rejoins the unchanged core. A validator checks dimensions, sections, topology, protected features, and rendered previews before exporting binary STL artifacts.

**Tech Stack:** Python 3.13, PEP 723 scripts through `uv`, `trimesh==5.0.0`, `manifold3d==3.5.2`, `numpy==2.5.1`, `Pillow==12.3.0`, `pytest==8.4.2`, binary STL.

## Global Constraints

- Source geometry is exactly `models/3d-print/3x3keypad/pico_macro_pad_top.stl.stl` and `models/3d-print/3x3keypad/pico_macro_pad_bottom_fitted_to_usb_c.stl.stl`.
- The adjacent 3MF is measurement evidence only and is never an input to generation.
- Layout notation is `columns x rows`; the columns run along the Type-C top edge and rows extend downward.
- Key pitch is exactly `19.05 mm`; target footprints are `65.15 x 84.20`, `84.20 x 84.20`, and `103.25 x 84.20 mm`.
- Keep the Type-C/RP2040 group rigid, including its approximately `0.5 mm` source offset; never recenter or scale it internally.
- New bottom regions remain empty for hand wiring; do not duplicate supports, pockets, or ribs.
- Preserve switch, wall, base, mating, screw, and counterbore cross-sections from the approved design.
- Every output STL is one closed, consistently oriented, positive-volume two-manifold component in millimeters.
- Do not modify `pyproject.toml`, `uv.lock`, application dependencies, source STL files, or unrelated untracked files.
- Honor `models/.gitignore`; generated STL artifacts must exist at the requested paths but remain ignored and uncommitted.
- All shell commands are prefixed with `rtk`; each segment of a command chain has its own `rtk` prefix.
- Slicer and physical-fit acceptance remain Not Run.

---

## File Structure

- Create `scripts/macro_pad_variants.py`: isolated dependency declaration, source normalization, mesh operations, top/bottom generation, validation, preview rendering, and CLI.
- Create `test/test_macro_pad_variants.py`: source-contract, layout, top, bottom, validator, and CLI regression tests.
- Create six requested STL artifacts under `models/3d-print/{3x4,4x4,5x4}/`.
- Create previews only under `/tmp/kivo-macro-pad-previews`; do not commit preview images.

The generator remains in the repository because six binary artifacts cannot be reviewed or reproduced safely from opaque manual edits alone. Its dependencies are isolated in PEP 723 metadata, so Kivo itself acquires no mesh runtime dependency.

### Task 1: Lock Source Contracts And Mesh Primitives

**Files:**
- Create: `scripts/macro_pad_variants.py`
- Create: `test/test_macro_pad_variants.py`

**Interfaces:**
- Consumes: the two canonical binary STL source paths.
- Produces: `Layout`, `LAYOUTS`, `load_source(path)`, `clip_slab(mesh, axis, lower, upper)`, `union_meshes(parts)`, and `affine_axis(mesh, axis, source_interval, target_interval)`.

- [ ] **Step 1: Write failing source and layout contract tests**

```python
from pathlib import Path

import numpy as np
import pytest

from scripts import macro_pad_variants as variants


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "models/3d-print/3x3keypad"


def test_layout_contracts() -> None:
    assert variants.LAYOUTS["3x4"].footprint == pytest.approx((65.15, 84.20))
    assert variants.LAYOUTS["4x4"].growth == pytest.approx((9.525, 9.525, 19.05))
    assert variants.LAYOUTS["5x4"].footprint == pytest.approx((103.25, 84.20))


@pytest.mark.parametrize(
    ("filename", "faces", "extents"),
    [
        ("pico_macro_pad_top.stl.stl", 3398, (65.15, 65.15, 9.998)),
        (
            "pico_macro_pad_bottom_fitted_to_usb_c.stl.stl",
            3838,
            (65.148, 65.15, 15.006),
        ),
    ],
)
def test_source_mesh_contract(filename: str, faces: int, extents: tuple[float, ...]) -> None:
    mesh = variants.load_source(SOURCE / filename)
    assert len(mesh.faces) == faces
    assert mesh.extents == pytest.approx(extents, abs=0.003)
    assert np.allclose(mesh.bounds[0], 0.0, atol=1e-6)
    assert mesh.is_watertight
    assert mesh.is_winding_consistent
    assert mesh.body_count == 1
```

- [ ] **Step 2: Run the tests and verify the module is missing**

Run:

```bash
rtk uv run --isolated --with pytest==8.4.2 --with trimesh==5.0.0 --with manifold3d==3.5.2 --with numpy==2.5.1 --with Pillow==12.3.0 python -m pytest test/test_macro_pad_variants.py -q
```

Expected: FAIL during collection with `ImportError: cannot import name 'macro_pad_variants' from 'scripts'`.

- [ ] **Step 3: Add the isolated script metadata, layouts, source orientation, and Boolean helpers**

Start `scripts/macro_pad_variants.py` with these exact public contracts:

```python
# /// script
# requires-python = ">=3.13"
# dependencies = [
#   "manifold3d==3.5.2",
#   "numpy==2.5.1",
#   "Pillow==12.3.0",
#   "trimesh==5.0.0",
# ]
# ///

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

import numpy as np
import trimesh


PITCH = 19.05
SOURCE_SIZE = 65.15
CELL_START = 23.05
CELL_END = CELL_START + PITCH
EPSILON = 1e-5


@dataclass(frozen=True)
class Layout:
    name: str
    columns: int
    rows: int

    @property
    def footprint(self) -> tuple[float, float]:
        return (8.0 + self.columns * PITCH, 8.0 + self.rows * PITCH)

    @property
    def growth(self) -> tuple[float, float, float]:
        width_growth = (self.columns - 3) * PITCH
        return (width_growth / 2.0, width_growth / 2.0, (self.rows - 3) * PITCH)


LAYOUTS = {name: Layout(name, columns, 4) for name, columns in (("3x4", 3), ("4x4", 4), ("5x4", 5))}


def load_source(path: Path) -> trimesh.Trimesh:
    mesh = trimesh.load_mesh(path, file_type="stl", process=False)
    if not isinstance(mesh, trimesh.Trimesh):
        raise TypeError(f"expected one mesh in {path}")
    mesh = mesh.copy()
    bounds = mesh.bounds.copy()
    transform = np.array(
        [
            [0.0, -1.0, 0.0, bounds[1, 1]],
            [1.0, 0.0, 0.0, -bounds[0, 0]],
            [0.0, 0.0, 1.0, -bounds[0, 2]],
            [0.0, 0.0, 0.0, 1.0],
        ]
    )
    mesh.apply_transform(transform)
    mesh.remove_unreferenced_vertices()
    return mesh


def clip_slab(mesh: trimesh.Trimesh, axis: int, lower: float, upper: float) -> trimesh.Trimesh:
    bounds = mesh.bounds.copy()
    bounds[0] -= 1.0
    bounds[1] += 1.0
    bounds[0, axis] = lower
    bounds[1, axis] = upper
    extents = bounds[1] - bounds[0]
    box = trimesh.creation.box(extents=extents)
    box.apply_translation((bounds[0] + bounds[1]) / 2.0)
    result = trimesh.boolean.intersection([mesh, box], engine="manifold")
    if not isinstance(result, trimesh.Trimesh) or result.is_empty:
        raise ValueError(f"empty slab on axis {axis}: {lower}..{upper}")
    return result


def union_meshes(parts: Iterable[trimesh.Trimesh]) -> trimesh.Trimesh:
    result = trimesh.boolean.union(list(parts), engine="manifold")
    if not isinstance(result, trimesh.Trimesh) or result.is_empty:
        raise ValueError("mesh union produced no solid")
    result.remove_unreferenced_vertices()
    result.process(validate=True)
    return result


def affine_axis(
    mesh: trimesh.Trimesh,
    axis: int,
    source_interval: tuple[float, float],
    target_interval: tuple[float, float],
) -> trimesh.Trimesh:
    source_start, source_end = source_interval
    target_start, target_end = target_interval
    result = mesh.copy()
    scale = (target_end - target_start) / (source_end - source_start)
    result.vertices[:, axis] = target_start + (result.vertices[:, axis] - source_start) * scale
    return result
```

- [ ] **Step 4: Run focused tests**

Run the Step 2 command again.

Expected: PASS with one layout case and two parameterized source cases.

- [ ] **Step 5: Commit the source contract and primitives**

```bash
rtk git add scripts/macro_pad_variants.py test/test_macro_pad_variants.py && rtk git commit -m "feat: add macro pad mesh generation primitives"
```

### Task 2: Generate Top Variants By Repeating Whole Switch Cells

**Files:**
- Modify: `scripts/macro_pad_variants.py`
- Modify: `test/test_macro_pad_variants.py`

**Interfaces:**
- Consumes: `Layout`, `PITCH`, `CELL_START`, `CELL_END`, `clip_slab()`, and `union_meshes()`.
- Produces: `repeat_cell_band(mesh, axis, near_shift, far_shift, tile_offsets)` and `generate_top(source, layout)`.

- [ ] **Step 1: Add failing tests for all top layouts**

```python
@pytest.mark.parametrize("name", ["3x4", "4x4", "5x4"])
def test_generate_top_preserves_pitch_holes_and_topology(name: str) -> None:
    source = variants.load_source(SOURCE / "pico_macro_pad_top.stl.stl")
    layout = variants.LAYOUTS[name]
    mesh = variants.generate_top(source, layout)

    assert mesh.extents[:2] == pytest.approx(layout.footprint, abs=0.003)
    assert mesh.extents[2] == pytest.approx(9.998, abs=0.001)
    assert mesh.is_watertight
    assert mesh.is_winding_consistent
    assert mesh.body_count == 1
    assert mesh.euler_number == 2 - 2 * layout.columns * layout.rows

    centers = variants.expected_switch_centers(layout)
    openings = variants.switch_section_sizes(mesh, centers, z=2.7)
    reliefs = variants.switch_section_sizes(mesh, centers, z=1.0)
    assert openings == pytest.approx(np.full((layout.columns * layout.rows, 2), 14.0), abs=0.003)
    assert reliefs == pytest.approx(np.full((layout.columns * layout.rows, 2), 14.8), abs=0.003)
    assert variants.axis_pitch(centers[:, 0]) == pytest.approx(variants.PITCH, abs=0.003)
    assert variants.axis_pitch(centers[:, 1]) == pytest.approx(variants.PITCH, abs=0.003)
```

- [ ] **Step 2: Verify the tests fail on missing top-generation functions**

Run:

```bash
rtk uv run --isolated --with pytest==8.4.2 --with trimesh==5.0.0 --with manifold3d==3.5.2 --with numpy==2.5.1 --with Pillow==12.3.0 python -m pytest test/test_macro_pad_variants.py -q -k generate_top
```

Expected: FAIL with `AttributeError: module 'scripts.macro_pad_variants' has no attribute 'generate_top'`.

- [ ] **Step 3: Implement repeatable cell bands and top construction**

Add these functions, retaining the exact offset rules:

```python
def translated(mesh: trimesh.Trimesh, axis: int, distance: float) -> trimesh.Trimesh:
    result = mesh.copy()
    offset = np.zeros(3)
    offset[axis] = distance
    result.apply_translation(offset)
    return result


def repeat_cell_band(
    mesh: trimesh.Trimesh,
    axis: int,
    near_shift: float,
    far_shift: float,
    tile_offsets: tuple[float, ...],
) -> trimesh.Trimesh:
    minimum, maximum = mesh.bounds[:, axis]
    near = clip_slab(mesh, axis, minimum - 1.0, CELL_START)
    tile = clip_slab(mesh, axis, CELL_START, CELL_END)
    far = clip_slab(mesh, axis, CELL_END, maximum + 1.0)
    parts = [translated(near, axis, near_shift)]
    parts.extend(translated(tile, axis, offset) for offset in tile_offsets)
    parts.append(translated(far, axis, far_shift))
    return union_meshes(parts)


def normalize_origin(mesh: trimesh.Trimesh) -> trimesh.Trimesh:
    result = mesh.copy()
    result.apply_translation(-result.bounds[0])
    return result


def generate_top(source: trimesh.Trimesh, layout: Layout) -> trimesh.Trimesh:
    left, right, bottom = layout.growth
    x_offsets = tuple(-left + index * PITCH for index in range(layout.columns - 2))
    result = repeat_cell_band(source, 0, -left, right, x_offsets)
    y_offsets = tuple(index * PITCH for index in range(layout.rows - 2))
    result = repeat_cell_band(result, 1, 0.0, bottom, y_offsets)
    return normalize_origin(result)
```

Add direct plane-intersection helpers. The `8.0 mm` half-window is smaller than
half the `19.05 mm` pitch, so each selection contains only one switch profile:

```python
def expected_switch_centers(layout: Layout) -> np.ndarray:
    first = 4.0 + PITCH / 2.0
    return np.array(
        [
            (first + column * PITCH, first + row * PITCH)
            for row in range(layout.rows)
            for column in range(layout.columns)
        ]
    )


def switch_section_sizes(
    mesh: trimesh.Trimesh, centers: np.ndarray, z: float
) -> np.ndarray:
    lines = trimesh.intersections.mesh_plane(
        mesh, plane_normal=[0.0, 0.0, 1.0], plane_origin=[0.0, 0.0, z]
    )
    points = lines.reshape(-1, 3)[:, :2]
    sizes: list[np.ndarray] = []
    for center in centers:
        local = points[np.all(np.abs(points - center) < 8.0, axis=1)]
        if len(local) == 0:
            raise ValueError(f"missing switch section at {center.tolist()}")
        sizes.append(np.ptp(local, axis=0))
    return np.array(sizes)


def axis_pitch(values: np.ndarray) -> float:
    unique = np.unique(np.round(values, 4))
    if len(unique) < 2:
        return PITCH
    differences = np.diff(unique)
    if not np.allclose(differences, PITCH, atol=0.003):
        raise ValueError(f"invalid pitch sequence: {differences.tolist()}")
    return float(differences.mean())
```

- [ ] **Step 4: Run the top tests and inspect one temporary STL**

Run the Step 2 command.

Expected: PASS for all three layouts.

Export a temporary 4x4 top directly through the completed generation API and
verify its container type:

```bash
rtk uv run --isolated --with trimesh==5.0.0 --with manifold3d==3.5.2 --with numpy==2.5.1 --with Pillow==12.3.0 python -c 'from pathlib import Path; from scripts import macro_pad_variants as v; source=v.load_source(Path("models/3d-print/3x3keypad/pico_macro_pad_top.stl.stl")); mesh=v.generate_top(source,v.LAYOUTS["4x4"]); target=Path("/tmp/kivo-top-check/4x4/pico_macro_pad_4x4_top.stl"); target.parent.mkdir(parents=True,exist_ok=True); target.write_bytes(v.trimesh.exchange.stl.export_stl(mesh)); print(mesh.extents.tolist())'
rtk file /tmp/kivo-top-check/4x4/pico_macro_pad_4x4_top.stl
```

Expected: binary STL data, `84.20 x 84.20 x 9.998 mm` in the generator report.

- [ ] **Step 5: Commit top generation**

```bash
rtk git add scripts/macro_pad_variants.py test/test_macro_pad_variants.py && rtk git commit -m "feat: generate expanded macro pad tops"
```

### Task 3: Keep The Bottom Core Rigid While Expanding The Shell

**Files:**
- Modify: `scripts/macro_pad_variants.py`
- Modify: `test/test_macro_pad_variants.py`

**Interfaces:**
- Consumes: `clip_slab()`, `union_meshes()`, `affine_axis()`, `normalize_origin()`, and `Layout.growth`.
- Produces: `BottomParts`, `TypeCSection`, `split_bottom(source)`, `expand_piecewise(mesh, axis, breakpoints, target_breakpoints)`, `expand_bottom_parts(source, layout)`, and `generate_bottom(source, layout)`.

- [ ] **Step 1: Add failing bottom geometry and protected-feature tests**

```python
@pytest.mark.parametrize("name", ["3x4", "4x4", "5x4"])
def test_generate_bottom_moves_only_empty_shell_corridors(name: str) -> None:
    source = variants.load_source(
        SOURCE / "pico_macro_pad_bottom_fitted_to_usb_c.stl.stl"
    )
    layout = variants.LAYOUTS[name]
    mesh = variants.generate_bottom(source, layout)

    assert mesh.extents[:2] == pytest.approx(layout.footprint, abs=0.003)
    assert mesh.extents[2] == pytest.approx(15.006, abs=0.001)
    assert mesh.is_watertight
    assert mesh.is_winding_consistent
    assert mesh.body_count == 1
    assert mesh.euler_number == -8

    source_usb = variants.type_c_section(source)
    output_usb = variants.type_c_section(mesh)
    assert output_usb.size == pytest.approx(source_usb.size, abs=0.003)
    assert output_usb.center_offset == pytest.approx(source_usb.center_offset, abs=0.003)
    assert variants.screw_axes(mesh) == pytest.approx(
        variants.expected_screw_axes(layout.footprint), abs=0.01
    )


@pytest.mark.parametrize("name", ["3x4", "4x4", "5x4"])
def test_bottom_internal_core_is_never_scaled(name: str) -> None:
    source = variants.load_source(
        SOURCE / "pico_macro_pad_bottom_fitted_to_usb_c.stl.stl"
    )
    _source_shell, source_core = variants.split_bottom(source)
    parts = variants.expand_bottom_parts(source, variants.LAYOUTS[name])
    assert parts.core.extents == pytest.approx(source_core.extents, abs=1e-6)
    assert parts.core.volume == pytest.approx(source_core.volume, abs=1e-4)
```

- [ ] **Step 2: Run focused bottom tests and verify failure**

Run:

```bash
rtk uv run --isolated --with pytest==8.4.2 --with trimesh==5.0.0 --with manifold3d==3.5.2 --with numpy==2.5.1 --with Pillow==12.3.0 python -m pytest test/test_macro_pad_variants.py -q -k bottom
```

Expected: FAIL on the missing `generate_bottom()` and `split_bottom()` functions.

- [ ] **Step 3: Split the rigid core and implement piecewise shell expansion**

First separate the source into a perimeter/base shell and an interior core.
The core contains every internal support above the base skin and stays rigid;
the Type-C lead-in portion that remains in the shell also stays rigid because
it lies inside both central identity intervals.

```python
@dataclass
class BottomParts:
    shell: trimesh.Trimesh
    core: trimesh.Trimesh


@dataclass(frozen=True)
class TypeCSection:
    size: tuple[float, float]
    center_offset: float


BOTTOM_X_BREAKS = (8.0, 20.50, 44.15, 57.15)
BOTTOM_Y_BREAKS = (55.00, 57.15)
BASE_SKIN_Z = 1.12
CORE_INSET = 4.05


def bounds_box(lower: np.ndarray, upper: np.ndarray) -> trimesh.Trimesh:
    box = trimesh.creation.box(extents=upper - lower)
    box.apply_translation((lower + upper) / 2.0)
    return box


def split_bottom(source: trimesh.Trimesh) -> tuple[trimesh.Trimesh, trimesh.Trimesh]:
    lower = np.array([CORE_INSET, CORE_INSET, BASE_SKIN_Z])
    upper = np.array(
        [source.extents[0] - CORE_INSET, source.extents[1] - CORE_INSET, source.extents[2] + 1.0]
    )
    cutter = bounds_box(lower, upper)
    core = trimesh.boolean.intersection([source, cutter], engine="manifold")
    shell = trimesh.boolean.difference([source, core], engine="manifold")
    if not isinstance(core, trimesh.Trimesh) or not isinstance(shell, trimesh.Trimesh):
        raise ValueError("bottom split did not produce two solids")
    return shell, core


def expand_piecewise(
    mesh: trimesh.Trimesh,
    axis: int,
    breakpoints: tuple[float, ...],
    target_breakpoints: tuple[float, ...],
) -> trimesh.Trimesh:
    source_edges = (mesh.bounds[0, axis] - 1.0, *breakpoints, mesh.bounds[1, axis] + 1.0)
    target_edges = (
        target_breakpoints[0] - (breakpoints[0] - source_edges[0]),
        *target_breakpoints,
        target_breakpoints[-1] + (source_edges[-1] - breakpoints[-1]),
    )
    parts: list[trimesh.Trimesh] = []
    for source_pair, target_pair in zip(
        zip(source_edges, source_edges[1:]),
        zip(target_edges, target_edges[1:]),
        strict=True,
    ):
        slab = clip_slab(mesh, axis, *source_pair)
        parts.append(affine_axis(slab, axis, source_pair, target_pair))
    return union_meshes(parts)


def expand_bottom_parts(source: trimesh.Trimesh, layout: Layout) -> BottomParts:
    shell, core = split_bottom(source)
    left, right, bottom = layout.growth
    x_targets = (
        BOTTOM_X_BREAKS[0] - left,
        BOTTOM_X_BREAKS[1],
        BOTTOM_X_BREAKS[2],
        BOTTOM_X_BREAKS[3] + right,
    )
    result = expand_piecewise(shell, 0, BOTTOM_X_BREAKS, x_targets)
    y_targets = (BOTTOM_Y_BREAKS[0], BOTTOM_Y_BREAKS[1] + bottom)
    result = expand_piecewise(result, 1, BOTTOM_Y_BREAKS, y_targets)
    return BottomParts(shell=result, core=core)


def generate_bottom(source: trimesh.Trimesh, layout: Layout) -> trimesh.Trimesh:
    parts = expand_bottom_parts(source, layout)
    return normalize_origin(union_meshes([parts.shell, parts.core]))
```

Keeping `shell` as the first `expand_piecewise()` operand is the regression
boundary that prevents internal supports from being stretched.

Add direct Type-C and screw-section measurements:

```python
def type_c_section(mesh: trimesh.Trimesh) -> TypeCSection:
    lines = trimesh.intersections.mesh_plane(
        mesh, plane_normal=[0.0, 1.0, 0.0], plane_origin=[0.0, 0.05, 0.0]
    )
    points = lines.reshape(-1, 3)[:, (0, 2)]
    center_x = mesh.extents[0] / 2.0
    mouth = points[
        (np.abs(points[:, 0] - center_x) < 8.0)
        & (points[:, 1] > 0.5)
        & (points[:, 1] < 8.0)
    ]
    if len(mouth) == 0:
        raise ValueError("Type-C mouth section is missing")
    lower = mouth.min(axis=0)
    upper = mouth.max(axis=0)
    return TypeCSection(
        size=(float(upper[0] - lower[0]), float(upper[1] - lower[1])),
        center_offset=float((lower[0] + upper[0]) / 2.0 - center_x),
    )


def expected_screw_axes(footprint: tuple[float, float]) -> np.ndarray:
    width, height = footprint
    return np.array(
        [(3.8, 3.8), (width - 3.8, 3.8), (3.8, height - 3.8), (width - 3.8, height - 3.8)]
    )


def screw_axes(mesh: trimesh.Trimesh, z: float = 5.0) -> np.ndarray:
    lines = trimesh.intersections.mesh_plane(
        mesh, plane_normal=[0.0, 0.0, 1.0], plane_origin=[0.0, 0.0, z]
    )
    points = lines.reshape(-1, 3)[:, :2]
    axes: list[np.ndarray] = []
    for expected in expected_screw_axes(tuple(mesh.extents[:2])):
        local = points[np.linalg.norm(points - expected, axis=1) < 2.0]
        if len(local) == 0:
            raise ValueError(f"missing screw section near {expected.tolist()}")
        axes.append((local.min(axis=0) + local.max(axis=0)) / 2.0)
    return np.array(axes)
```

- [ ] **Step 4: Run bottom and complete regression tests**

Run the Step 2 command, then the complete Task 1 command.

Expected: all source, top, and bottom tests PASS. The bottom Euler characteristic remains `-8` for all variants, proving no duplicated pocket or screw tunnel was introduced.

- [ ] **Step 5: Commit bottom generation**

```bash
rtk git add scripts/macro_pad_variants.py test/test_macro_pad_variants.py && rtk git commit -m "feat: generate protected macro pad bottoms"
```

### Task 4: Add Artifact Validation, Deterministic Export, And Previews

**Files:**
- Modify: `scripts/macro_pad_variants.py`
- Modify: `test/test_macro_pad_variants.py`

**Interfaces:**
- Consumes: `generate_top()`, `generate_bottom()`, section helpers, and layout contracts.
- Produces: `ValidationReport`, `validate_pair(top, bottom, source_bottom, layout)`, `render_preview(mesh, path, view)`, `export_variant()`, and CLI `main()`.

- [ ] **Step 1: Add failing validator and CLI tests**

```python
def test_validate_pair_reports_the_complete_contract() -> None:
    top_source = variants.load_source(SOURCE / "pico_macro_pad_top.stl.stl")
    bottom_source = variants.load_source(
        SOURCE / "pico_macro_pad_bottom_fitted_to_usb_c.stl.stl"
    )
    layout = variants.LAYOUTS["4x4"]
    report = variants.validate_pair(
        variants.generate_top(top_source, layout),
        variants.generate_bottom(bottom_source, layout),
        bottom_source,
        layout,
    )
    assert report.layout == "4x4"
    assert report.switch_count == 16
    assert report.footprint == pytest.approx((84.20, 84.20), abs=0.003)
    assert report.watertight
    assert report.manifold
    assert report.type_c_preserved
    assert report.screws_aligned


def test_cli_writes_exact_artifact_names(tmp_path: Path) -> None:
    variants.main(
        [
            "--source-root",
            str(SOURCE),
            "--output-root",
            str(tmp_path / "models"),
            "--preview-root",
            str(tmp_path / "previews"),
        ]
    )
    for name in variants.LAYOUTS:
        directory = tmp_path / "models" / name
        assert (directory / f"pico_macro_pad_{name}_top.stl").is_file()
        assert (
            directory / f"pico_macro_pad_{name}_bottom_fitted_to_usb_c.stl"
        ).is_file()
        assert (tmp_path / "previews" / f"{name}-top.png").is_file()
        assert (tmp_path / "previews" / f"{name}-bottom.png").is_file()
        assert (tmp_path / "previews" / f"{name}-type-c.png").is_file()
```

- [ ] **Step 2: Verify the validator tests fail**

Run:

```bash
rtk uv run --isolated --with pytest==8.4.2 --with trimesh==5.0.0 --with manifold3d==3.5.2 --with numpy==2.5.1 --with Pillow==12.3.0 python -m pytest test/test_macro_pad_variants.py -q -k 'validate_pair or cli'
```

Expected: FAIL on missing `validate_pair()` and `main()`.

- [ ] **Step 3: Implement strict validation and binary STL export**

Implement the report and strict checks directly:

```python
@dataclass(frozen=True)
class ValidationReport:
    layout: str
    footprint: tuple[float, float]
    switch_count: int
    watertight: bool
    manifold: bool
    type_c_preserved: bool
    screws_aligned: bool


def assert_closed_manifold(mesh: trimesh.Trimesh, label: str) -> None:
    incidence = np.bincount(mesh.edges_unique_inverse)
    if (
        mesh.body_count != 1
        or not mesh.is_watertight
        or not mesh.is_winding_consistent
        or np.any(incidence != 2)
        or np.any(mesh.area_faces < 1e-9)
        or mesh.volume <= 0.0
    ):
        raise ValueError(f"{label} is not one positive closed two-manifold solid")


def validate_pair(
    top: trimesh.Trimesh,
    bottom: trimesh.Trimesh,
    source_bottom: trimesh.Trimesh,
    layout: Layout,
) -> ValidationReport:
    assert_closed_manifold(top, f"{layout.name} top")
    assert_closed_manifold(bottom, f"{layout.name} bottom")
    expected = np.array(layout.footprint)
    if not np.allclose(top.extents[:2], expected, atol=0.003):
        raise ValueError(f"{layout.name} top footprint drifted: {top.extents[:2]}")
    if not np.allclose(bottom.extents[:2], expected, atol=0.003):
        raise ValueError(f"{layout.name} bottom footprint drifted: {bottom.extents[:2]}")
    if not np.isclose(top.extents[2], 9.998, atol=0.001):
        raise ValueError(f"{layout.name} top Z extent drifted")
    if not np.isclose(bottom.extents[2], 15.006, atol=0.001):
        raise ValueError(f"{layout.name} bottom Z extent drifted")

    centers = expected_switch_centers(layout)
    if not np.allclose(switch_section_sizes(top, centers, 2.7), 14.0, atol=0.003):
        raise ValueError(f"{layout.name} switch openings drifted")
    if not np.allclose(switch_section_sizes(top, centers, 1.0), 14.8, atol=0.003):
        raise ValueError(f"{layout.name} switch reliefs drifted")
    axis_pitch(centers[:, 0])
    axis_pitch(centers[:, 1])

    source_usb = type_c_section(source_bottom)
    output_usb = type_c_section(bottom)
    type_c_preserved = np.allclose(output_usb.size, source_usb.size, atol=0.003) and np.isclose(
        output_usb.center_offset, source_usb.center_offset, atol=0.003
    )
    if not type_c_preserved:
        raise ValueError(f"{layout.name} Type-C section drifted")

    expected_axes = expected_screw_axes(layout.footprint)
    screws_aligned = np.allclose(screw_axes(top, z=1.0), expected_axes, atol=0.01) and np.allclose(
        screw_axes(bottom, z=5.0), expected_axes, atol=0.01
    )
    if not screws_aligned:
        raise ValueError(f"{layout.name} screw axes drifted")
    if not np.allclose(top.bounds[:, :2], bottom.bounds[:, :2], atol=0.003):
        raise ValueError(f"{layout.name} top and bottom outlines do not align")

    return ValidationReport(
        layout=layout.name,
        footprint=tuple(float(value) for value in expected),
        switch_count=len(centers),
        watertight=True,
        manifold=True,
        type_c_preserved=type_c_preserved,
        screws_aligned=screws_aligned,
    )
```

Change `screw_axes()` from Task 3 to accept its section height as an explicit
`z: float` parameter. The top is checked at `Z=1.0 mm`; the bottom is checked at
`Z=5.0 mm`.

Export through temporary files and atomic replacement:

```python
def export_stl(mesh: trimesh.Trimesh, target: Path) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    temporary = target.with_suffix(target.suffix + ".tmp")
    temporary.write_bytes(trimesh.exchange.stl.export_stl(mesh))
    temporary.replace(target)
```

- [ ] **Step 4: Implement deterministic orthographic preview rendering and CLI**

Add `argparse`, `json`, `dataclasses.asdict`, `PIL.Image`, and
`PIL.ImageDraw` imports. Render depth-sorted orthographic triangles with these
fixed camera matrices and a nonblank-pixel gate:

```python
VIEW_ROTATIONS = {
    "top": np.eye(3),
    "bottom": np.diag([1.0, -1.0, -1.0]),
    "type-c": np.array(
        [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]]
    ),
}


def render_preview(mesh: trimesh.Trimesh, target: Path, view: str) -> None:
    from PIL import Image, ImageDraw

    rotation = VIEW_ROTATIONS[view]
    triangles = mesh.triangles @ rotation.T
    projected = triangles[:, :, :2]
    lower = projected.reshape(-1, 2).min(axis=0)
    upper = projected.reshape(-1, 2).max(axis=0)
    canvas = np.array([1200.0, 900.0])
    scale = float(np.min((canvas - 96.0) / (upper - lower)))
    points = (projected - lower) * scale + 48.0
    points[:, :, 1] = canvas[1] - points[:, :, 1]

    normals = np.cross(triangles[:, 1] - triangles[:, 0], triangles[:, 2] - triangles[:, 0])
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
        draw.polygon(polygon, fill=(level, level, level), outline=(70, 70, 70))
    pixels = np.asarray(image)
    if np.count_nonzero(np.any(pixels != 255, axis=2)) < pixels.shape[0] * pixels.shape[1] * 0.05:
        raise ValueError(f"blank preview for {target}")
    target.parent.mkdir(parents=True, exist_ok=True)
    image.save(target)
```

The CLI defaults are:

```python
DEFAULT_SOURCE = Path("models/3d-print/3x3keypad")
DEFAULT_OUTPUT = Path("models/3d-print")
DEFAULT_PREVIEWS = Path("/tmp/kivo-macro-pad-previews")
```

It accepts repeatable `--layout`, optional `--only {top,bottom}`, plus `--source-root`, `--output-root`, and `--preview-root`. Without filters it generates all six STL files, runs `validate_pair()` before every export, writes nine previews, and prints one JSON report per layout.

Implement the export and CLI loop with these exact filenames and ordering:

```python
def export_variant(
    top: trimesh.Trimesh,
    bottom: trimesh.Trimesh,
    layout: Layout,
    output_root: Path,
    only: str | None,
) -> None:
    directory = output_root / layout.name
    if only in (None, "top"):
        export_stl(top, directory / f"pico_macro_pad_{layout.name}_top.stl")
    if only in (None, "bottom"):
        export_stl(
            bottom,
            directory / f"pico_macro_pad_{layout.name}_bottom_fitted_to_usb_c.stl",
        )


def main(argv: list[str] | None = None) -> int:
    import argparse
    import json
    from dataclasses import asdict

    parser = argparse.ArgumentParser()
    parser.add_argument("--source-root", type=Path, default=DEFAULT_SOURCE)
    parser.add_argument("--output-root", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--preview-root", type=Path, default=DEFAULT_PREVIEWS)
    parser.add_argument("--layout", action="append", choices=tuple(LAYOUTS))
    parser.add_argument("--only", choices=("top", "bottom"))
    arguments = parser.parse_args(argv)

    top_source = load_source(arguments.source_root / "pico_macro_pad_top.stl.stl")
    bottom_source = load_source(
        arguments.source_root / "pico_macro_pad_bottom_fitted_to_usb_c.stl.stl"
    )
    selected = arguments.layout or list(LAYOUTS)
    for name in selected:
        layout = LAYOUTS[name]
        top = generate_top(top_source, layout)
        bottom = generate_bottom(bottom_source, layout)
        report = validate_pair(top, bottom, bottom_source, layout)
        export_variant(top, bottom, layout, arguments.output_root, arguments.only)
        render_preview(top, arguments.preview_root / f"{name}-top.png", "top")
        render_preview(bottom, arguments.preview_root / f"{name}-bottom.png", "bottom")
        render_preview(bottom, arguments.preview_root / f"{name}-type-c.png", "type-c")
        print(json.dumps(asdict(report), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
```

- [ ] **Step 5: Run all mesh tests and commit validation tooling**

Run:

```bash
rtk uv run --isolated --with pytest==8.4.2 --with trimesh==5.0.0 --with manifold3d==3.5.2 --with numpy==2.5.1 --with Pillow==12.3.0 python -m pytest test/test_macro_pad_variants.py -q
```

Expected: all tests PASS.

Commit:

```bash
rtk git add scripts/macro_pad_variants.py test/test_macro_pad_variants.py && rtk git commit -m "test: validate generated macro pad meshes"
```

### Task 5: Generate And Review The Six Requested STL Artifacts

**Files:**
- Create: `models/3d-print/3x4/pico_macro_pad_3x4_top.stl`
- Create: `models/3d-print/3x4/pico_macro_pad_3x4_bottom_fitted_to_usb_c.stl`
- Create: `models/3d-print/4x4/pico_macro_pad_4x4_top.stl`
- Create: `models/3d-print/4x4/pico_macro_pad_4x4_bottom_fitted_to_usb_c.stl`
- Create: `models/3d-print/5x4/pico_macro_pad_5x4_top.stl`
- Create: `models/3d-print/5x4/pico_macro_pad_5x4_bottom_fitted_to_usb_c.stl`

**Interfaces:**
- Consumes: the completed generator and unchanged 3x3 source files.
- Produces: the six user-requested, independently validated binary STL artifacts.

- [ ] **Step 1: Record source hashes and generate every variant**

```bash
rtk shasum -a 256 models/3d-print/3x3keypad/pico_macro_pad_top.stl.stl models/3d-print/3x3keypad/pico_macro_pad_bottom_fitted_to_usb_c.stl.stl
rtk uv run --script scripts/macro_pad_variants.py
```

Expected: three JSON reports with footprints `65.15 x 84.20`, `84.20 x 84.20`, and `103.25 x 84.20`; every Boolean/topology flag is true.

- [ ] **Step 2: Verify files, source immutability, and complete regression suite**

```bash
rtk file models/3d-print/3x4/*.stl models/3d-print/4x4/*.stl models/3d-print/5x4/*.stl
rtk shasum -a 256 models/3d-print/3x3keypad/pico_macro_pad_top.stl.stl models/3d-print/3x3keypad/pico_macro_pad_bottom_fitted_to_usb_c.stl.stl
rtk uv run --isolated --with pytest==8.4.2 --with trimesh==5.0.0 --with manifold3d==3.5.2 --with numpy==2.5.1 --with Pillow==12.3.0 python -m pytest test/test_macro_pad_variants.py -q
rtk git diff --check
```

Expected: six binary STL files, source hashes unchanged from Step 1, all mesh tests PASS, and no whitespace errors.

- [ ] **Step 3: Perform visual QA on all nine previews**

Open `/tmp/kivo-macro-pad-previews/{3x4,4x4,5x4}-{top,bottom,type-c}.png` with the local image viewer. Confirm:

- the requested key counts are visible and evenly spaced;
- the Type-C opening appears once, on the fixed top edge;
- the controller pocket remains at the top center;
- new bottom regions are empty;
- all four corner screw stacks move to the new corners;
- no bridge covers a switch hole or cavity;
- top and bottom outlines match for each layout.

Expected: all nine previews pass; any failure returns to the owning generation task rather than being accepted as a slicer repair.

- [ ] **Step 4: Confirm the repository ignore policy leaves binary artifacts untracked**

```bash
rtk git check-ignore -v models/3d-print/3x4/pico_macro_pad_3x4_top.stl models/3d-print/4x4/pico_macro_pad_4x4_top.stl models/3d-print/5x4/pico_macro_pad_5x4_top.stl
rtk git status --short
```

Expected: `models/.gitignore:1:3d-print` is reported for each artifact and no
model file is staged. Do not use `git add -f`; the requested deliverable is the
workspace file set, not committed binary model history.

- [ ] **Step 5: Run final staged-scope and repository checks**

```bash
rtk git status --short
rtk git log -5 --oneline
rtk uv run --isolated --with pytest==8.4.2 --with trimesh==5.0.0 --with manifold3d==3.5.2 --with numpy==2.5.1 --with Pillow==12.3.0 python -m pytest test/test_macro_pad_variants.py -q
```

Expected: Git status is clean because both source and generated model files are
ignored; the generator and tests are committed; all six output files exist at
their requested paths; all focused tests PASS. Report slicer and physical
printing as Not Run.
