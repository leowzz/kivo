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
            np.array([x0, y1 - JOIN_OVERLAP, 0.0]),
            np.array([x0 + WALL, inner_rear + JOIN_OVERLAP, z1]),
        ),
        box_from_bounds(
            np.array([x1 - WALL, y1 - JOIN_OVERLAP, 0.0]),
            np.array([x1, inner_rear + JOIN_OVERLAP, z1]),
        ),
    ]
    return macro.union_meshes(parts)


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
        build_safety_pad(x_side, y_side) for x_side in (-1, 1) for y_side in (-1, 1)
    )
    joined = macro.union_meshes(parts)
    result = subtract_meshes(joined, [rear_hole_cutter()])
    result.merge_vertices()
    result.remove_unreferenced_vertices()
    return result
