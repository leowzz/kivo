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
BOTTOM_X_BREAKS = (8.0, 20.50, 44.15, 57.15)
BOTTOM_Y_BREAKS = (55.00, 57.15)
BASE_SKIN_Z = 1.12
CORE_INSET = 8.0
CORE_OVERLAP = 0.2


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


@dataclass
class BottomParts:
    shell: trimesh.Trimesh
    core: trimesh.Trimesh


@dataclass(frozen=True)
class TypeCSection:
    size: tuple[float, float]
    center_offset: float


LAYOUTS = {
    name: Layout(name, columns, 4)
    for name, columns in (("3x4", 3), ("4x4", 4), ("5x4", 5))
}


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
    mesh.merge_vertices()
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
    for boundary in (lower, upper):
        on_boundary = np.isclose(
            result.vertices[:, axis], boundary, rtol=0.0, atol=EPSILON
        )
        result.vertices[on_boundary, axis] = boundary
    return result


def union_meshes(parts: Iterable[trimesh.Trimesh]) -> trimesh.Trimesh:
    result = trimesh.boolean.union(list(parts), engine="manifold")
    if not isinstance(result, trimesh.Trimesh) or result.is_empty:
        raise ValueError("mesh union produced no solid")
    result.remove_unreferenced_vertices()
    if not result.is_volume:
        raise ValueError("mesh union did not produce a positive closed volume")
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


def bounds_box(lower: np.ndarray, upper: np.ndarray) -> trimesh.Trimesh:
    box = trimesh.creation.box(extents=upper - lower)
    box.apply_translation((lower + upper) / 2.0)
    return box


def split_bottom(source: trimesh.Trimesh) -> tuple[trimesh.Trimesh, trimesh.Trimesh]:
    lower = np.array([CORE_INSET, CORE_INSET, source.bounds[0, 2] - 1.0])
    upper = np.array(
        [
            source.extents[0] - CORE_INSET,
            source.extents[1] - CORE_INSET,
            source.extents[2] + 1.0,
        ]
    )
    cutter = bounds_box(lower, upper)
    core = trimesh.boolean.intersection([source, cutter], engine="manifold")
    shell = union_meshes(
        [
            clip_slab(
                source,
                2,
                source.bounds[0, 2] - 1.0,
                BASE_SKIN_Z + CORE_OVERLAP,
            ),
            clip_slab(
                source,
                0,
                source.bounds[0, 0] - 1.0,
                CORE_INSET + CORE_OVERLAP,
            ),
            clip_slab(
                source,
                0,
                source.extents[0] - CORE_INSET - CORE_OVERLAP,
                source.bounds[1, 0] + 1.0,
            ),
            clip_slab(
                source,
                1,
                source.bounds[0, 1] - 1.0,
                CORE_INSET + CORE_OVERLAP,
            ),
            clip_slab(
                source,
                1,
                source.extents[1] - CORE_INSET - CORE_OVERLAP,
                source.bounds[1, 1] + 1.0,
            ),
        ]
    )
    if not isinstance(core, trimesh.Trimesh) or not isinstance(
        shell, trimesh.Trimesh
    ):
        raise ValueError("bottom split did not produce two solids")
    return shell, core


def expand_piecewise(
    mesh: trimesh.Trimesh,
    axis: int,
    breakpoints: tuple[float, ...],
    target_breakpoints: tuple[float, ...],
) -> trimesh.Trimesh:
    if np.allclose(breakpoints, target_breakpoints, rtol=0.0, atol=EPSILON):
        return mesh.copy()
    source_edges = (
        mesh.bounds[0, axis] - 1.0,
        *breakpoints,
        mesh.bounds[1, axis] + 1.0,
    )
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
        [
            (3.8, 3.8),
            (width - 3.8, 3.8),
            (3.8, height - 3.8),
            (width - 3.8, height - 3.8),
        ]
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
