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
from dataclasses import asdict, dataclass
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
PLATFORM_SIZE = 24.0
PAD_SIZE = 10.0
PAD_THICKNESS = 2.4
PAD_TOP = 13.4
BOTTOMED_TRIGGER_HEIGHT = 5.0
HANDSET_RECESS = 2.0
PLATFORM_TOP = PAD_TOP - BOTTOMED_TRIGGER_HEIGHT + HANDSET_RECESS
PLATFORM_BOTTOM = PLATFORM_TOP - PLATE_THICKNESS
WIRE_HOLE_DIAMETER = 4.0
CENTER_X = OUTER_WIDTH / 2.0
CENTER_Y = OUTER_LENGTH / 2.0
REAR_WALL_THICKNESS = LOWER_INSET
BOOLEAN_TOLERANCE = 5e-5
PROFILE_TOLERANCE = 0.003
PROTECTED_VOLUME_TOLERANCE = 0.02
RING_SECTION_LEVEL = 20.0
REQUIRED_SOLID_VOLUME_TOLERANCE = 0.03


def circle_segments_for_sagitta(radius: float, tolerance: float) -> int:
    required = int(np.ceil(np.pi / np.arccos(1.0 - tolerance / radius)))
    return 4 * int(np.ceil(required / 4.0))


ROUNDED_SECTION_SEGMENTS = circle_segments_for_sagitta(OUTER_RADIUS, PROFILE_TOLERANCE)
WIRE_HOLE_SEGMENTS = circle_segments_for_sagitta(
    WIRE_HOLE_DIAMETER / 2.0, PROFILE_TOLERANCE
)

OPEN_UNDERSIDE_PROBES = (
    ((8.0, 25.0, -0.1), (15.0, 35.0, PLATFORM_BOTTOM - 0.1)),
    (
        (OUTER_WIDTH - 15.0, 25.0, -0.1),
        (OUTER_WIDTH - 8.0, 35.0, PLATFORM_BOTTOM - 0.1),
    ),
    ((8.0, 48.0, -0.1), (15.0, 58.0, PLATFORM_BOTTOM - 0.1)),
    (
        (OUTER_WIDTH - 15.0, 48.0, -0.1),
        (OUTER_WIDTH - 8.0, 58.0, PLATFORM_BOTTOM - 0.1),
    ),
)
SWITCH_CHANNEL_PROBE = (
    (
        CENTER_X - PLATFORM_SIZE / 2.0 + WALL + 0.001,
        CENTER_Y - PLATFORM_SIZE / 2.0 + WALL + 0.001,
        -0.1,
    ),
    (
        CENTER_X + PLATFORM_SIZE / 2.0 - WALL - 0.001,
        OUTER_LENGTH - LOWER_INSET - 0.001,
        PLATFORM_BOTTOM - 0.1,
    ),
)
REAR_WIRE_PROBE = (
    (CENTER_X - 1.0, 49.4, 4.5),
    (CENTER_X + 1.0, OUTER_LENGTH + 1.0, 5.5),
)
OUTER_CORNER_PROBES = (
    ((0.0, 0.0, 11.2), (0.5, 0.5, 13.2)),
    ((OUTER_WIDTH - 0.5, 0.0, 11.2), (OUTER_WIDTH, 0.5, 13.2)),
    ((0.0, OUTER_LENGTH - 0.5, 11.2), (0.5, OUTER_LENGTH, 13.2)),
    (
        (OUTER_WIDTH - 0.5, OUTER_LENGTH - 0.5, 11.2),
        (OUTER_WIDTH, OUTER_LENGTH, 13.2),
    ),
)
PLATFORM_TOP_PROBE = (
    (CENTER_X + CELL_SIZE / 2.0 + 0.25, CENTER_Y - 1.0, -0.1),
    (CENTER_X + PLATFORM_SIZE / 2.0 - 0.25, CENTER_Y + 1.0, OUTER_HEIGHT + 1.0),
)
PAD_TOP_PROBES = (
    (
        (LOWER_INSET + 1.5, LOWER_INSET + 1.5, -0.1),
        (LOWER_INSET + 3.5, LOWER_INSET + 3.5, OUTER_HEIGHT + 1.0),
    ),
    (
        (OUTER_WIDTH - LOWER_INSET - 3.5, LOWER_INSET + 1.5, -0.1),
        (OUTER_WIDTH - LOWER_INSET - 1.5, LOWER_INSET + 3.5, OUTER_HEIGHT + 1.0),
    ),
    (
        (LOWER_INSET + 1.5, OUTER_LENGTH - LOWER_INSET - 3.5, -0.1),
        (LOWER_INSET + 3.5, OUTER_LENGTH - LOWER_INSET - 1.5, OUTER_HEIGHT + 1.0),
    ),
    (
        (OUTER_WIDTH - LOWER_INSET - 3.5, OUTER_LENGTH - LOWER_INSET - 3.5, -0.1),
        (
            OUTER_WIDTH - LOWER_INSET - 1.5,
            OUTER_LENGTH - LOWER_INSET - 1.5,
            OUTER_HEIGHT + 1.0,
        ),
    ),
)

DEFAULT_SOURCE_ROOT = Path("models/3d-print/3x3keypad")
DEFAULT_OUTPUT_ROOT = Path("models/3d-print/telephone-handset-switch-base")
DEFAULT_PREVIEW_ROOT = Path("/tmp/kivo-handset-switch-base-previews")
OUTPUT_FILENAME = "telephone_handset_switch_base.stl"
VIEW_ROTATIONS = {
    "top": np.eye(3),
    "bottom": np.diag([1.0, -1.0, -1.0]),
    "side-section": np.array([[0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [-1.0, 0.0, 0.0]]),
    "isometric": np.array(
        [
            [0.70710678, -0.70710678, 0.0],
            [0.40824829, 0.40824829, -0.81649658],
            [0.57735027, 0.57735027, 0.57735027],
        ]
    ),
}


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
        circular_segments=ROUNDED_SECTION_SEGMENTS,
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
    if region.is_empty:
        return 0.0
    triangles = region.triangles
    signed_volume = (
        np.einsum(
            "ij,ij->i",
            triangles[:, 0],
            np.cross(triangles[:, 1], triangles[:, 2]),
        ).sum()
        / 6.0
    )
    return abs(float(signed_volume))


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


def inner_funnel_cutter() -> trimesh.Trimesh:
    lower_section = rounded_rectangle_section(
        INNER_WIDTH, INNER_LENGTH, INNER_RADIUS, (CENTER_X, CENTER_Y)
    )
    mouth_section = lower_section.offset(
        FUNNEL_EXPANSION,
        join_type=manifold3d.JoinType.Round,
        circular_segments=ROUNDED_SECTION_SEGMENTS,
    )
    points = [
        [float(x), float(y), z]
        for section, z in (
            (lower_section, FUNNEL_BOTTOM),
            (mouth_section, OUTER_HEIGHT),
        )
        for polygon in section.to_polygons()
        for x, y in polygon
    ]
    return manifold_to_mesh(manifold3d.Manifold.hull_points(points))


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
    return subtract_meshes(outer, [inner, inner_funnel_cutter()])


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
    inner_rear = OUTER_LENGTH - LOWER_INSET
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
    x_pad, x_exposed, x_foot = side_bounds(
        x_side, LOWER_INSET, OUTER_WIDTH - LOWER_INSET
    )
    y_pad, y_exposed, y_foot = side_bounds(
        y_side, LOWER_INSET, OUTER_LENGTH - LOWER_INSET
    )
    pad_bottom = PAD_TOP - PAD_THICKNESS
    gusset_bottom = pad_bottom - (PAD_SIZE - WALL)

    pad = box_from_bounds(
        np.array([x_pad[0], y_pad[0], pad_bottom]),
        np.array([x_pad[1], y_pad[1], PAD_TOP]),
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
        height=REAR_WALL_THICKNESS + 2.0,
        sections=WIRE_HOLE_SEGMENTS,
    )
    cutter.apply_transform(
        trimesh.transformations.rotation_matrix(np.pi / 2.0, [1.0, 0.0, 0.0])
    )
    cutter.apply_translation([CENTER_X, OUTER_LENGTH - REAR_WALL_THICKNESS / 2.0, 5.0])
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
    result.remove_unreferenced_vertices()
    return result


def measured_section_loops(
    mesh: trimesh.Trimesh, axis: int, level: float
) -> list[np.ndarray]:
    origin = np.zeros(3)
    normal = np.zeros(3)
    origin[axis] = level
    normal[axis] = 1.0
    section = mesh.section(plane_origin=origin, plane_normal=normal)
    if section is None:
        raise ValueError(f"missing section on axis {axis} at {level}")
    dimensions = [index for index in range(3) if index != axis]
    loops: list[np.ndarray] = []
    for entity in section.entities:
        if not entity.closed:
            continue
        points = entity.discrete(section.vertices)
        loops.append(points[:, dimensions])
    return loops


def measured_section_loop_sizes(
    mesh: trimesh.Trimesh, axis: int, level: float
) -> np.ndarray:
    return np.array(
        [np.ptp(points, axis=0) for points in measured_section_loops(mesh, axis, level)]
    )


def turning_vertices(points: np.ndarray) -> np.ndarray:
    if np.allclose(points[0], points[-1], rtol=0.0, atol=BOOLEAN_TOLERANCE):
        points = points[:-1]
    incoming = points - np.roll(points, 1, axis=0)
    outgoing = np.roll(points, -1, axis=0) - points
    cross = np.abs(incoming[:, 0] * outgoing[:, 1] - incoming[:, 1] * outgoing[:, 0])
    scale = np.linalg.norm(incoming, axis=1) * np.linalg.norm(outgoing, axis=1)
    return points[cross > BOOLEAN_TOLERANCE * scale]


def arc_chord_height_is_valid(
    angles: np.ndarray,
    start: float,
    end: float,
    radius: float,
    tolerance: float,
) -> bool:
    if len(angles) == 0:
        return False
    angles = np.sort(angles)
    angular_tolerance = tolerance / radius
    if angles[0] < start - angular_tolerance or angles[-1] > end + angular_tolerance:
        return False
    angles = np.clip(angles, start, end)
    covered = np.concatenate(([start], angles, [end]))
    gaps = np.diff(covered)
    if np.any(gaps < -angular_tolerance):
        return False
    sagittas = radius * (1.0 - np.cos(np.maximum(gaps, 0.0) / 2.0))
    return bool(np.all(sagittas <= tolerance))


def rounded_rectangle_arcs_are_valid(
    vertices: np.ndarray,
    nearest_core: np.ndarray,
    expected_bounds: np.ndarray,
    radius: float,
    tolerance: float,
) -> bool:
    core_lower = expected_bounds[0] + radius
    core_upper = expected_bounds[1] - radius
    assigned = np.zeros(len(vertices), dtype=bool)
    for signs in ((-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)):
        signs_array = np.array(signs)
        center = np.where(signs_array < 0.0, core_lower, core_upper)
        corner = np.all(
            np.isclose(nearest_core, center, rtol=0.0, atol=tolerance), axis=1
        )
        local = (vertices[corner] - center) * signs_array
        if np.any(local < -tolerance):
            return False
        angles = np.arctan2(local[:, 1], local[:, 0])
        if not arc_chord_height_is_valid(angles, 0.0, np.pi / 2.0, radius, tolerance):
            return False
        assigned |= corner
    return bool(np.all(assigned))


def require_rounded_rectangle_loop(
    loops: list[np.ndarray],
    expected_bounds: np.ndarray,
    radius: float,
    label: str,
    tolerance: float,
) -> np.ndarray:
    for points in loops:
        vertices = turning_vertices(points)
        bounds = np.array([vertices.min(axis=0), vertices.max(axis=0)])
        if not np.allclose(bounds, expected_bounds, rtol=0.0, atol=tolerance):
            continue
        core_lower = expected_bounds[0] + radius
        core_upper = expected_bounds[1] - radius
        nearest_core = np.clip(vertices, core_lower, core_upper)
        radii = np.linalg.norm(vertices - nearest_core, axis=1)
        if np.allclose(
            radii, radius, rtol=0.0, atol=tolerance
        ) and rounded_rectangle_arcs_are_valid(
            vertices, nearest_core, expected_bounds, radius, tolerance
        ):
            return bounds[1] - bounds[0]
    raise ValueError(f"{label} drifted")


def require_circular_loop(
    loops: list[np.ndarray],
    center: np.ndarray,
    radius: float,
    label: str,
    tolerance: float,
) -> None:
    expected_bounds = np.array([center - radius, center + radius])
    for points in loops:
        vertices = turning_vertices(points)
        bounds = np.array([vertices.min(axis=0), vertices.max(axis=0)])
        if not np.allclose(bounds, expected_bounds, rtol=0.0, atol=tolerance):
            continue
        radii = np.linalg.norm(vertices - center, axis=1)
        if not np.allclose(radii, radius, rtol=0.0, atol=tolerance):
            continue
        relative = vertices - center
        angles = np.sort(
            np.mod(np.arctan2(relative[:, 1], relative[:, 0]), 2.0 * np.pi)
        )
        wrapped = np.concatenate((angles, [angles[0] + 2.0 * np.pi]))
        gaps = np.diff(wrapped)
        sagittas = radius * (1.0 - np.cos(gaps / 2.0))
        if np.all(sagittas <= tolerance):
            return
    raise ValueError(f"{label} drifted")


def require_loop_size(
    sizes: np.ndarray, expected: tuple[float, float], label: str, tolerance: float
) -> np.ndarray:
    for size in sizes:
        if np.allclose(size, expected, rtol=0.0, atol=tolerance):
            return size
    raise ValueError(f"{label} drifted: {sizes.tolist()}")


def protected_cell_mismatch(mesh: trimesh.Trimesh, source: trimesh.Trimesh) -> float:
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


def validation_rounded_rectangle_section(
    width: float,
    length: float,
    radius: float,
    circular_segments: int = ROUNDED_SECTION_SEGMENTS,
) -> manifold3d.CrossSection:
    x0 = CENTER_X - width / 2.0
    x1 = CENTER_X + width / 2.0
    y0 = CENTER_Y - length / 2.0
    y1 = CENTER_Y + length / 2.0
    quarter_segments = circular_segments // 4
    corners = (
        ((x1 - radius, y0 + radius), -np.pi / 2.0, 0.0),
        ((x1 - radius, y1 - radius), 0.0, np.pi / 2.0),
        ((x0 + radius, y1 - radius), np.pi / 2.0, np.pi),
        ((x0 + radius, y0 + radius), np.pi, 3.0 * np.pi / 2.0),
    )
    points: list[tuple[float, float]] = []
    for center, start, end in corners:
        for angle in np.linspace(start, end, quarter_segments + 1):
            points.append(
                (
                    center[0] + radius * float(np.cos(angle)),
                    center[1] + radius * float(np.sin(angle)),
                )
            )
    return manifold3d.CrossSection([points])


def validation_rear_hole_clearance() -> trimesh.Trimesh:
    clearance = trimesh.creation.cylinder(
        radius=WIRE_HOLE_DIAMETER / 2.0,
        height=REAR_WALL_THICKNESS + 2.0 * BOOLEAN_TOLERANCE,
        sections=WIRE_HOLE_SEGMENTS,
    )
    clearance.apply_transform(
        trimesh.transformations.rotation_matrix(np.pi / 2.0, [1.0, 0.0, 0.0])
    )
    clearance.apply_translation(
        [CENTER_X, OUTER_LENGTH - REAR_WALL_THICKNESS / 2.0, 5.0]
    )
    return clearance


def required_outer_ring_reference(
    rear_clearance: trimesh.Trimesh,
) -> trimesh.Trimesh:
    outer_section = validation_rounded_rectangle_section(
        OUTER_WIDTH, OUTER_LENGTH, OUTER_RADIUS
    )
    inner_section = validation_rounded_rectangle_section(
        INNER_WIDTH, INNER_LENGTH, INNER_RADIUS
    )
    mouth_section = validation_rounded_rectangle_section(
        MOUTH_WIDTH,
        MOUTH_LENGTH,
        MOUTH_RADIUS,
        circular_segments=2 * ROUNDED_SECTION_SEGMENTS,
    )
    outer = manifold_to_mesh(outer_section.extrude(OUTER_HEIGHT))
    inner = manifold_to_mesh(
        inner_section.extrude(OUTER_HEIGHT + 2.0).translate((0.0, 0.0, -1.0))
    )
    funnel_points = [
        [float(x), float(y), z]
        for section, z in (
            (inner_section, FUNNEL_BOTTOM),
            (mouth_section, OUTER_HEIGHT),
        )
        for polygon in section.to_polygons()
        for x, y in polygon
    ]
    funnel = manifold_to_mesh(manifold3d.Manifold.hull_points(funnel_points))
    return subtract_meshes(outer, [inner, funnel, rear_clearance])


def intersection_volume(meshes: Iterable[trimesh.Trimesh]) -> float:
    intersection = macro.boolean_meshes(meshes, "intersection")
    if intersection.is_empty:
        return 0.0
    triangles = intersection.triangles
    signed_volume = (
        np.einsum(
            "ij,ij->i",
            triangles[:, 0],
            np.cross(triangles[:, 1], triangles[:, 2]),
        ).sum()
        / 6.0
    )
    return abs(float(signed_volume))


def required_feature_references() -> list[tuple[str, trimesh.Trimesh]]:
    x0 = CENTER_X - PLATFORM_SIZE / 2.0
    x1 = CENTER_X + PLATFORM_SIZE / 2.0
    y0 = CENTER_Y - PLATFORM_SIZE / 2.0
    y1 = CENTER_Y + PLATFORM_SIZE / 2.0
    z1 = PLATFORM_BOTTOM + JOIN_OVERLAP
    rear = OUTER_LENGTH - LOWER_INSET

    platform = box_from_bounds(
        np.array([x0, y0, PLATFORM_BOTTOM]),
        np.array([x1, y1, PLATFORM_TOP]),
    )
    protected_cell = box_from_bounds(
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
    references = [
        ("switch platform", subtract_meshes(platform, [protected_cell])),
        (
            "left tower",
            box_from_bounds(np.array([x0, y0, 0.0]), np.array([x0 + WALL, y1, z1])),
        ),
        (
            "right tower",
            box_from_bounds(np.array([x1 - WALL, y0, 0.0]), np.array([x1, y1, z1])),
        ),
        (
            "front wall",
            box_from_bounds(
                np.array([x0 + WALL, y0, 0.0]),
                np.array([x1 - WALL, y0 + WALL, z1]),
            ),
        ),
        (
            "left rear rib",
            box_from_bounds(
                np.array([x0, y1 - JOIN_OVERLAP, 0.0]),
                np.array([x0 + WALL, rear + JOIN_OVERLAP, z1]),
            ),
        ),
        (
            "right rear rib",
            box_from_bounds(
                np.array([x1 - WALL, y1 - JOIN_OVERLAP, 0.0]),
                np.array([x1, rear + JOIN_OVERLAP, z1]),
            ),
        ),
    ]

    def reference_ranges(
        side: int, lower: float, upper: float
    ) -> tuple[tuple[float, float], tuple[float, float], tuple[float, float]]:
        if side < 0:
            return (
                (lower - JOIN_OVERLAP, lower + PAD_SIZE),
                (lower, lower + PAD_SIZE),
                (lower, lower + WALL),
            )
        return (
            (upper - PAD_SIZE, upper + JOIN_OVERLAP),
            (upper - PAD_SIZE, upper),
            (upper - WALL, upper),
        )

    pad_bottom = PAD_TOP - PAD_THICKNESS
    gusset_bottom = pad_bottom - (PAD_SIZE - WALL)
    for x_side in (-1, 1):
        x_pad, x_exposed, x_foot = reference_ranges(
            x_side, LOWER_INSET, OUTER_WIDTH - LOWER_INSET
        )
        for y_side in (-1, 1):
            y_pad, y_exposed, y_foot = reference_ranges(
                y_side, LOWER_INSET, OUTER_LENGTH - LOWER_INSET
            )
            suffix = f"{x_side},{y_side}"
            references.append(
                (
                    f"safety pad {suffix}",
                    box_from_bounds(
                        np.array([x_pad[0], y_pad[0], pad_bottom]),
                        np.array([x_pad[1], y_pad[1], PAD_TOP]),
                    ),
                )
            )
            references.append(
                (
                    f"safety foot {suffix}",
                    box_from_bounds(
                        np.array([x_foot[0], y_foot[0], 0.0]),
                        np.array([x_foot[1], y_foot[1], gusset_bottom]),
                    ),
                )
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
            references.append((f"safety gusset {suffix}", hull_points(points)))
    return references


def validate_outer_ring_coverage(
    mesh: trimesh.Trimesh, reference: trimesh.Trimesh
) -> None:
    shared_volume = intersection_volume([mesh, reference])
    missing_volume = max(0.0, float(reference.volume - shared_volume))
    if missing_volume > REQUIRED_SOLID_VOLUME_TOLERANCE:
        raise ValueError(f"outer wall is missing: volume={missing_volume}")


def validation_source_cell_reference(source: trimesh.Trimesh) -> trimesh.Trimesh:
    cell = macro.clip_slab(source, 0, CELL_START, CELL_END)
    cell = macro.clip_slab(cell, 1, CELL_START, CELL_END)
    target_lower = np.array(
        [
            CENTER_X - CELL_SIZE / 2.0,
            CENTER_Y - CELL_SIZE / 2.0,
            PLATFORM_BOTTOM,
        ]
    )
    cell.apply_translation(target_lower - cell.bounds[0])
    return cell


def validate_unexpected_material(
    mesh: trimesh.Trimesh,
    source: trimesh.Trimesh,
    outer_ring: trimesh.Trimesh,
    feature_references: list[tuple[str, trimesh.Trimesh]],
) -> None:
    allowed = macro.union_meshes(
        [
            outer_ring,
            validation_source_cell_reference(source),
            *(reference for _, reference in feature_references),
        ]
    )
    lower_bounds = box_from_bounds(
        np.array([0.0, 0.0, -BOOLEAN_TOLERANCE]),
        np.array([OUTER_WIDTH, OUTER_LENGTH, PLATFORM_BOTTOM - BOOLEAN_TOLERANCE]),
    )
    actual_lower = macro.boolean_meshes([mesh, lower_bounds], "intersection")
    allowed_lower = macro.boolean_meshes([allowed, lower_bounds], "intersection")
    shared_volume = intersection_volume([actual_lower, allowed_lower])
    unexpected_volume = max(0.0, float(actual_lower.volume - shared_volume))
    if unexpected_volume > REQUIRED_SOLID_VOLUME_TOLERANCE:
        raise ValueError(
            "unexpected lower material obstructs open underside: "
            f"volume={unexpected_volume}"
        )

    shared_volume = intersection_volume([mesh, allowed])
    unexpected_volume = max(0.0, float(mesh.volume - shared_volume))
    if unexpected_volume > REQUIRED_SOLID_VOLUME_TOLERANCE:
        raise ValueError(f"unexpected model material: volume={unexpected_volume}")


def funnel_section_dimensions(level: float) -> tuple[float, float, float]:
    expansion = (level - FUNNEL_BOTTOM) / FUNNEL_DEPTH * FUNNEL_EXPANSION
    return (
        INNER_WIDTH + 2.0 * expansion,
        INNER_LENGTH + 2.0 * expansion,
        INNER_RADIUS + expansion,
    )


def validate_base(mesh: trimesh.Trimesh, source: trimesh.Trimesh) -> ValidationReport:
    if not mesh.is_watertight:
        mesh = mesh.copy()
        mesh.merge_vertices()
        mesh.remove_unreferenced_vertices()
    macro.assert_closed_manifold(mesh, "telephone handset switch base")
    if not np.allclose(mesh.bounds[0], (0.0, 0.0, 0.0), atol=0.003):
        raise ValueError(f"outer origin drifted: {mesh.bounds[0].tolist()}")
    expected_extents = np.array([OUTER_WIDTH, OUTER_LENGTH, OUTER_HEIGHT])
    if not np.allclose(mesh.extents, expected_extents, atol=0.003):
        raise ValueError(f"outer extents drifted: {mesh.extents.tolist()}")

    mismatch = protected_cell_mismatch(mesh, source)
    if mismatch > PROTECTED_VOLUME_TOLERANCE:
        raise ValueError(f"source switch cell drifted: mismatch={mismatch}")

    pocket_loops = measured_section_loops(mesh, axis=2, level=RING_SECTION_LEVEL)
    require_rounded_rectangle_loop(
        pocket_loops,
        np.array([[0.0, 0.0], [OUTER_WIDTH, OUTER_LENGTH]]),
        OUTER_RADIUS,
        "R6 outer corner ring profile",
        PROFILE_TOLERANCE,
    )
    measured_pocket_bounds = require_rounded_rectangle_loop(
        pocket_loops,
        np.array(
            [
                [LOWER_INSET, LOWER_INSET],
                [OUTER_WIDTH - LOWER_INSET, OUTER_LENGTH - LOWER_INSET],
            ]
        ),
        INNER_RADIUS,
        "R1.6 inner pocket ring profile",
        PROFILE_TOLERANCE,
    )
    for level in (PAD_TOP + 0.001, RING_SECTION_LEVEL, FUNNEL_BOTTOM - 0.001):
        require_rounded_rectangle_loop(
            measured_section_loops(mesh, axis=2, level=level),
            np.array(
                [
                    [LOWER_INSET, LOWER_INSET],
                    [OUTER_WIDTH - LOWER_INSET, OUTER_LENGTH - LOWER_INSET],
                ]
            ),
            INNER_RADIUS,
            "R1.6 lower locating section",
            PROFILE_TOLERANCE,
        )
    for level in (
        FUNNEL_BOTTOM + 1.0,
        FUNNEL_BOTTOM + 2.0,
        FUNNEL_BOTTOM + 3.0,
        OUTER_HEIGHT - 0.001,
    ):
        width, length, radius = funnel_section_dimensions(level)
        require_rounded_rectangle_loop(
            measured_section_loops(mesh, axis=2, level=level),
            np.array(
                [
                    [CENTER_X - width / 2.0, CENTER_Y - length / 2.0],
                    [CENTER_X + width / 2.0, CENTER_Y + length / 2.0],
                ]
            ),
            radius,
            "funnel section",
            PROFILE_TOLERANCE,
        )

    platform_loops = measured_section_loop_sizes(mesh, axis=2, level=PLATFORM_TOP - 0.7)
    require_loop_size(platform_loops, (24.0, 24.0), "switch platform", 0.003)

    support_top = measured_pocket_floor_top(mesh)
    if not np.isclose(support_top, PAD_TOP, atol=0.003):
        raise ValueError(f"pocket floor datum drifted: {support_top}")
    platform_top = float(probe_bounds(mesh, PLATFORM_TOP_PROBE)[1, 2])
    if not np.isclose(platform_top, PLATFORM_TOP, atol=0.003):
        raise ValueError(f"platform top drifted: {platform_top}")
    for probe in PAD_TOP_PROBES:
        pad_top = float(probe_bounds(mesh, probe)[1, 2])
        if not np.isclose(pad_top, PAD_TOP, atol=0.003):
            raise ValueError(f"safety-pad top drifted: {probe}")
    pocket_depth = float(mesh.bounds[1, 2] - PAD_TOP)
    if not np.isclose(pocket_depth, 15.0, atol=0.003):
        raise ValueError(f"pocket depth drifted: {pocket_depth}")

    lower = macro.measure_switch_section(
        mesh, z=PLATFORM_BOTTOM + 1.0, nominal_size=14.8
    )
    upper = macro.measure_switch_section(mesh, z=PLATFORM_TOP - 0.7, nominal_size=14.0)
    expected_center = np.array([[CENTER_X, CENTER_Y]])
    if not np.allclose(lower.centers, expected_center, atol=0.003):
        raise ValueError("lower switch relief center drifted")
    if not np.allclose(upper.centers, expected_center, atol=0.003):
        raise ValueError("upper switch aperture center drifted")
    if not np.allclose(lower.sizes, [[14.798, 14.798]], atol=0.003):
        raise ValueError("lower switch relief drifted")
    if not np.allclose(upper.sizes, [[14.0, 14.0]], atol=0.003):
        raise ValueError("upper switch aperture drifted")

    rear_loops = measured_section_loops(
        mesh, axis=1, level=OUTER_LENGTH - REAR_WALL_THICKNESS / 2.0
    )
    require_circular_loop(
        rear_loops,
        np.array([CENTER_X, 5.0]),
        WIRE_HOLE_DIAMETER / 2.0,
        "rear wire hole profile",
        PROFILE_TOLERANCE,
    )

    for probe in OPEN_UNDERSIDE_PROBES:
        if probe_volume(mesh, probe) >= 1e-6:
            raise ValueError(f"open underside is obstructed: {probe}")
    if probe_volume(mesh, SWITCH_CHANNEL_PROBE) >= 1e-6:
        raise ValueError("open underside 19.2 channel is obstructed")
    if probe_volume(mesh, REAR_WIRE_PROBE) >= 1e-6:
        raise ValueError("rear wire path is obstructed")
    for probe in OUTER_CORNER_PROBES:
        if probe_volume(mesh, probe) >= 1e-6:
            raise ValueError(f"R6 outer corner is filled: {probe}")

    rear_clearance = validation_rear_hole_clearance()
    obstructed_hole_volume = intersection_volume([mesh, rear_clearance])
    if obstructed_hole_volume > REQUIRED_SOLID_VOLUME_TOLERANCE:
        raise ValueError(
            f"rear wire hole clearance is obstructed: volume={obstructed_hole_volume}"
        )

    outer_ring_reference = required_outer_ring_reference(rear_clearance)
    validate_outer_ring_coverage(mesh, outer_ring_reference)
    feature_references = required_feature_references()
    validate_unexpected_material(mesh, source, outer_ring_reference, feature_references)
    for label, reference in feature_references:
        shared = macro.boolean_meshes([mesh, reference], "intersection")
        missing_volume = max(0.0, float(reference.volume - shared.volume))
        if missing_volume > REQUIRED_SOLID_VOLUME_TOLERANCE:
            raise ValueError(
                "required platform, tower, rib, pad, or gusset is missing: "
                f"{label}, volume={missing_volume}"
            )

    incidence = np.bincount(mesh.edges_unique_inverse)
    return ValidationReport(
        outer_extents=tuple(float(value) for value in mesh.extents),
        pocket_bounds=tuple(float(value) for value in measured_pocket_bounds),
        pocket_depth=float(pocket_depth),
        protected_mismatch_volume=float(mismatch),
        connected_components=int(mesh.body_count),
        watertight=bool(mesh.is_watertight),
        two_manifold=bool(np.all(incidence == 2)),
        open_underside=True,
        rear_wire_path=True,
    )


def export_base(mesh: trimesh.Trimesh, target: Path) -> None:
    macro.export_stl(mesh, target)


def mesh_for_preview(mesh: trimesh.Trimesh, view: str) -> trimesh.Trimesh:
    if view == "side-section":
        return macro.clip_slab(mesh, 0, CENTER_X, OUTER_WIDTH + 1.0)
    return mesh


def render_side_section_preview(mesh: trimesh.Trimesh, target: Path) -> None:
    from PIL import Image, ImageDraw

    section = mesh_to_manifold(mesh).rotate((0.0, -90.0, 0.0)).slice(CENTER_X)
    contours = [np.asarray(polygon) for polygon in section.to_polygons()]
    if not contours:
        raise ValueError("empty side-section preview")

    projected = [
        np.column_stack((contour[:, 1], -contour[:, 0])) for contour in contours
    ]
    stacked = np.vstack(projected)
    lower = stacked.min(axis=0)
    upper = stacked.max(axis=0)
    canvas = np.array([1200.0, 900.0])
    scale = float(np.min((canvas - 96.0) / (upper - lower)))
    rendered_size = (upper - lower) * scale
    offset = (canvas - rendered_size) / 2.0

    image = Image.new("RGB", (1200, 900), "white")
    draw = ImageDraw.Draw(image)
    ordered = sorted(
        projected,
        key=lambda contour: abs(
            float(
                np.sum(
                    contour[:, 0] * np.roll(contour[:, 1], -1)
                    - contour[:, 1] * np.roll(contour[:, 0], -1)
                )
            )
        ),
        reverse=True,
    )
    for contour in ordered:
        signed_area = float(
            np.sum(
                contour[:, 0] * np.roll(contour[:, 1], -1)
                - contour[:, 1] * np.roll(contour[:, 0], -1)
            )
        )
        points = (contour - lower) * scale + offset
        points[:, 1] = canvas[1] - points[:, 1]
        fill = (165, 165, 165) if signed_area > 0.0 else (255, 255, 255)
        polygon = [tuple(value) for value in points.tolist()]
        draw.polygon(
            polygon,
            fill=fill,
        )
        draw.line(
            polygon + [polygon[0]],
            fill=(70, 70, 70),
            width=14,
            joint="curve",
        )
    save_nonblank_preview(image, target)


def save_nonblank_preview(image: object, target: Path) -> None:
    pixels = np.asarray(image)
    nonblank = np.count_nonzero(np.any(pixels != 255, axis=2))
    if nonblank < pixels.shape[0] * pixels.shape[1] * 0.05:
        raise ValueError(f"blank preview for {target}")
    target.parent.mkdir(parents=True, exist_ok=True)
    image.save(target)


def render_preview(mesh: trimesh.Trimesh, target: Path, view: str) -> None:
    from PIL import Image, ImageDraw

    if view not in VIEW_ROTATIONS:
        raise ValueError(f"unsupported preview view: {view}")
    if view == "side-section":
        render_side_section_preview(mesh, target)
        return
    rendered_mesh = mesh_for_preview(mesh, view)
    triangles = rendered_mesh.triangles
    triangles = triangles @ VIEW_ROTATIONS[view].T
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
    save_nonblank_preview(image, target)


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
