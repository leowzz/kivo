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
from collections.abc import Iterable
from pathlib import Path

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


def region_volume(mesh: trimesh.Trimesh, lower: np.ndarray, upper: np.ndarray) -> float:
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
