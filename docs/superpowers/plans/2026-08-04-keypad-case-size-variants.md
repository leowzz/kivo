# Keypad Case Size Variants Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Generate verified top and bottom STL pairs for 3x4, 4x4, and 5x4 YD-RP2040 macro-pad cases while preserving every functional dimension from the 3x3 source.

**Architecture:** A reproducible Python mesh generator loads and rotates each source STL so the Type-C edge is the fixed top datum. It repeats complete 19.05 mm switch-cell bands for the top and applies piecewise affine expansion only through measured empty shell corridors for the bottom; a validator checks dimensions, sections, topology, protected features, and rendered previews before exporting binary STL artifacts.

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

    openings = variants.square_section_centers(mesh, z=2.7, side=14.0)
    reliefs = variants.square_section_centers(mesh, z=1.0, side=14.8)
    assert openings.shape == (layout.columns * layout.rows, 2)
    assert reliefs.shape == openings.shape
    assert variants.axis_pitch(openings[:, 0]) == pytest.approx(PITCH, abs=0.003)
    assert variants.axis_pitch(openings[:, 1]) == pytest.approx(PITCH, abs=0.003)
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

Implement `square_section_centers()` by taking `mesh.section()` at the requested Z plane, projecting each closed path to XY, and retaining loops whose X and Y extents both match `side` within `0.03 mm`. Implement `axis_pitch()` by sorting unique rounded coordinates, taking adjacent differences, and requiring every difference to agree within `0.003 mm`.

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

### Task 3: Expand The Bottom Through Empty Corridors

**Files:**
- Modify: `scripts/macro_pad_variants.py`
- Modify: `test/test_macro_pad_variants.py`

**Interfaces:**
- Consumes: `clip_slab()`, `union_meshes()`, `affine_axis()`, `normalize_origin()`, and `Layout.growth`.
- Produces: `expand_piecewise(mesh, axis, breakpoints, target_breakpoints)` and `generate_bottom(source, layout)`.

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


def test_bottom_stretch_zones_do_not_cross_controller_features() -> None:
    source = variants.load_source(
        SOURCE / "pico_macro_pad_bottom_fitted_to_usb_c.stl.stl"
    )
    variants.assert_empty_bridge_corridors(source)
```

- [ ] **Step 2: Run focused bottom tests and verify failure**

Run:

```bash
rtk uv run --isolated --with pytest==8.4.2 --with trimesh==5.0.0 --with manifold3d==3.5.2 --with numpy==2.5.1 --with Pillow==12.3.0 python -m pytest test/test_macro_pad_variants.py -q -k bottom
```

Expected: FAIL on the missing `generate_bottom` and corridor-check functions.

- [ ] **Step 3: Implement protected-zone assertions and piecewise expansion**

Use these measured boundaries in the Type-C-top coordinate frame:

```python
BOTTOM_X_BREAKS = (8.0, 20.50, 44.15, 57.15)
BOTTOM_Y_BREAKS = (55.00, 57.15)
BASE_SKIN_Z = 1.12
WALL_INSET = 4.0


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


def generate_bottom(source: trimesh.Trimesh, layout: Layout) -> trimesh.Trimesh:
    assert_empty_bridge_corridors(source)
    left, right, bottom = layout.growth
    x_targets = (
        BOTTOM_X_BREAKS[0] - left,
        BOTTOM_X_BREAKS[1],
        BOTTOM_X_BREAKS[2],
        BOTTOM_X_BREAKS[3] + right,
    )
    result = expand_piecewise(source, 0, BOTTOM_X_BREAKS, x_targets)
    y_targets = (BOTTOM_Y_BREAKS[0], BOTTOM_Y_BREAKS[1] + bottom)
    result = expand_piecewise(result, 1, BOTTOM_Y_BREAKS, y_targets)
    return normalize_origin(result)
```

`assert_empty_bridge_corridors()` must reject a source when a face centroid in either affine stretch interval is above the `1.12 mm` base skin and more than `4.0 mm` inside the perimeter. Allow only the known straight perimeter wall/tongue faces in those intervals. This protects the controller pocket, local ribs, Type-C lead-in, and screw stacks from accidental stretching if the source STL changes later.

Implement `type_c_section()` from the boundary loop on the normalized `Y=0` wall, returning its width, height, and X offset from the enclosure center. Implement `screw_axes()` from the four circular section loops at `Z=5.0 mm`; sort results by Y then X.

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
- Produces: `ValidationReport`, `validate_pair(top, bottom, layout)`, `render_preview(mesh, path, view)`, `export_variant()`, and CLI `main()`.

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

Add a frozen `ValidationReport` with fields `layout`, `footprint`, `switch_count`, `watertight`, `manifold`, `type_c_preserved`, and `screws_aligned`. `validate_pair()` must raise `ValueError` immediately when any approved contract fails; it returns the report only after all checks pass.

Use `mesh.edges_unique_inverse` with `numpy.bincount()` to require exactly two incident faces per unique edge. Reject any face whose area is below `1e-9 mm2`, any non-positive signed volume, mismatched top/bottom XY bounds, unexpected Z extent, section dimension drift greater than `0.03 mm`, or pitch drift greater than `0.003 mm`.

Export through temporary files and atomic replacement:

```python
def export_stl(mesh: trimesh.Trimesh, target: Path) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    temporary = target.with_suffix(target.suffix + ".tmp")
    temporary.write_bytes(trimesh.exchange.stl.export_stl(mesh))
    temporary.replace(target)
```

- [ ] **Step 4: Implement deterministic orthographic preview rendering and CLI**

Use Pillow to draw depth-sorted projected triangles onto an `1200 x 900` white canvas. Support `top`, `bottom`, and `type-c` camera matrices; derive flat face shading from the transformed normal dot a fixed light vector. Fit projected bounds with a 48-pixel margin and reject a preview when fewer than 5% of pixels differ from the white background.

The CLI defaults are:

```python
DEFAULT_SOURCE = Path("models/3d-print/3x3keypad")
DEFAULT_OUTPUT = Path("models/3d-print")
DEFAULT_PREVIEWS = Path("/tmp/kivo-macro-pad-previews")
```

It accepts repeatable `--layout`, optional `--only {top,bottom}`, plus `--source-root`, `--output-root`, and `--preview-root`. Without filters it generates all six STL files, runs `validate_pair()` before every export, writes nine previews, and prints one JSON report per layout.

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

- [ ] **Step 4: Commit only the requested artifacts**

```bash
rtk git add models/3d-print/3x4 models/3d-print/4x4 models/3d-print/5x4 && rtk git commit -m "feat: add expanded RP2040 macro pad cases"
```

- [ ] **Step 5: Run final staged-scope and repository checks**

```bash
rtk git status --short
rtk git log -5 --oneline
rtk uv run --isolated --with pytest==8.4.2 --with trimesh==5.0.0 --with manifold3d==3.5.2 --with numpy==2.5.1 --with Pillow==12.3.0 python -m pytest test/test_macro_pad_variants.py -q
```

Expected: only the pre-existing untracked `models/3d-print/3x3keypad` source directory remains outside commits; the generator, tests, and six outputs are committed; all focused tests PASS. Report slicer and physical printing as Not Run.
