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
