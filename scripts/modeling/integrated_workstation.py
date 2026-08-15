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
    from scripts.modeling import macro_pad_variants as macro
    from scripts.modeling import telephone_handset_switch_base as handset
else:
    import macro_pad_variants as macro
    import telephone_handset_switch_base as handset


# All dimensions are millimeters. X runs left-to-right, Y runs front-to-back,
# and Z points up from the desk.
KEY_COLUMNS = 6
KEY_ROWS = 3
KEY_PITCH = 19.05
KEY_PLATE_THICKNESS = 3.4
KEY_STEP_HEIGHT = 2.0
LOWER_SWITCH_APERTURE = 14.8
UPPER_SWITCH_APERTURE = 14.0
KEY_ANGLE_DEGREES = 30.0
KEY_ANGLE = np.deg2rad(KEY_ANGLE_DEGREES)

WEDGE_X0 = 72.0
WEDGE_X1 = 210.0
WEDGE_Y0 = 4.0
WEDGE_Y1 = 108.0
WEDGE_FRONT_HEIGHT = 16.0
WEDGE_WALL = 3.0

KEY_X0 = 85.35
KEY_Y0 = 12.0
PANEL_X0 = 75.0
PANEL_X1 = 207.0
PANEL_Y0 = 3.0
PANEL_Y1 = 120.0
PANEL_CLEARANCE = 0.3
PANEL_OPENING_REAR_OVERCUT = 3.0
PANEL_LIP_DEPTH = 2.4
PANEL_SIDE_SUPPORT_Y1 = 117.0
PANEL_SCREW_CENTERS = np.array(
    [
        [79.0, 12.0],
        [203.0, 12.0],
        [79.0, 62.0],
        [203.0, 62.0],
        [79.0, 109.0],
        [203.0, 109.0],
    ]
)
PANEL_HOLE_DIAMETER = 3.4

# User-measured M3 heat-set insert and countersunk screw dimensions.
# The slightly oversized lead-in centers the hot insert before it reaches the
# interference-fit bore. All insert holes are blind and open only at a hidden
# mating surface.
HEAT_SET_INSERT_NARROW_DIAMETER = 3.9
HEAT_SET_INSERT_WIDE_DIAMETER = 4.9
HEAT_SET_INSERT_LENGTH = 4.9
HEAT_SET_INSERT_HOLE_DIAMETER = 4.0
HEAT_SET_INSERT_LEAD_DIAMETER = 5.1
HEAT_SET_INSERT_LEAD_DEPTH = 0.6
HEAT_SET_INSERT_DEPTH_CLEARANCE = 0.5
HEAT_SET_INSERT_HOLE_DEPTH = HEAT_SET_INSERT_LENGTH + HEAT_SET_INSERT_DEPTH_CLEARANCE
HEAT_SET_INSERT_BLIND_FLOOR = 1.2
HEAT_SET_INSERT_CUTTER_OVERSHOOT = 1.0
M3_SCREW_THREAD_DIAMETER = 2.9
M3_SCREW_HEAD_DIAMETER = 5.3
M3_SCREW_HEAD_CLEARANCE_DIAMETER = 5.6
M3_SCREW_COUNTERSINK_ANGLE_DEGREES = 90.0
M3_SCREW_COUNTERSINK_CUTTER_OVERSHOOT = 0.1
PANEL_INSERT_BOSS_RADIUS = 5.5
PANEL_INSERT_BOSS_DEPTH = HEAT_SET_INSERT_HOLE_DEPTH + HEAT_SET_INSERT_BLIND_FLOOR

SCREEN_BOARD_WIDTH = 64.90
SCREEN_BOARD_HEIGHT = 35.03
SCREEN_BEZEL_WIDTH = 76.0
SCREEN_BEZEL_HEIGHT = 45.0
SCREEN_BEZEL_X0 = 142.5 - SCREEN_BEZEL_WIDTH / 2.0
SCREEN_BEZEL_Y0 = 73.0
SCREEN_BEZEL_RAISE = 2.0
SCREEN_RECESS_CLEARANCE = 0.65
SCREEN_BOARD_ORIGIN = np.array(
    [
        SCREEN_BEZEL_X0 + (SCREEN_BEZEL_WIDTH - SCREEN_BOARD_WIDTH) / 2.0,
        SCREEN_BEZEL_Y0 + (SCREEN_BEZEL_HEIGHT - SCREEN_BOARD_HEIGHT) / 2.0,
    ]
)
SCREEN_INSERT_THROUGH_DIAMETER = HEAT_SET_INSERT_HOLE_DIAMETER
SCREEN_INSERT_MATERIAL_DEPTH = KEY_PLATE_THICKNESS + SCREEN_BEZEL_RAISE
SCREEN_BOARD_HOLES = np.array(
    [
        [2.95, 2.97],
        [61.93, 3.15],
        [2.87, SCREEN_BOARD_HEIGHT - 2.85],
        [SCREEN_BOARD_WIDTH - 3.00, SCREEN_BOARD_HEIGHT - 2.90],
    ]
)
SCREEN_HEADER_PIN_COUNT = 8
SCREEN_HEADER_FIRST_PIN_X = 11.38
SCREEN_HEADER_PIN_PITCH = 2.54
SCREEN_HEADER_PIN_Y_FROM_TOP = 1.93
SCREEN_HEADER_PIN_CENTERS = np.array(
    [
        [
            SCREEN_HEADER_FIRST_PIN_X + pin * SCREEN_HEADER_PIN_PITCH,
            SCREEN_BOARD_HEIGHT - SCREEN_HEADER_PIN_Y_FROM_TOP,
        ]
        for pin in range(SCREEN_HEADER_PIN_COUNT)
    ]
)
SCREEN_CABLE_SLOT_LOCAL_X0 = 9.5
SCREEN_CABLE_SLOT_LOCAL_Y0 = 29.0
SCREEN_CABLE_SLOT_WIDTH = 24.5
SCREEN_CABLE_SLOT_HEIGHT = 6.5

# Six recessed clips retain fly-wire bundles without adding protrusions to the
# panel's print face. Each slot groups three neighboring switches. The mouth
# expands into the pocket at 45 degrees and the 3 mm roof is a short bridge, so
# the panel remains support-free when printed with its underside on the bed.
WIRE_CLIP_LENGTH = 16.0
WIRE_CLIP_MOUTH_WIDTH = 1.5
WIRE_CLIP_POCKET_WIDTH = 3.0
WIRE_CLIP_MOUTH_DEPTH = 0.6
WIRE_CLIP_TRANSITION_DEPTH = 0.75
WIRE_CLIP_POCKET_DEPTH = 2.2
WIRE_CLIP_FRONT_SKIN = KEY_PLATE_THICKNESS - WIRE_CLIP_POCKET_DEPTH
WIRE_CLIP_X_CENTERS = np.array([KEY_X0 + 1.5 * KEY_PITCH, KEY_X0 + 4.5 * KEY_PITCH])
WIRE_CLIP_Y_CENTERS = np.array(
    [
        KEY_Y0 + KEY_PITCH,
        KEY_Y0 + 2.0 * KEY_PITCH,
        (KEY_Y0 + 2.5 * KEY_PITCH + LOWER_SWITCH_APERTURE / 2.0 + SCREEN_BEZEL_Y0)
        / 2.0,
    ]
)
WIRE_CLIP_CENTERS = np.array(
    [[x, y] for y in WIRE_CLIP_Y_CENTERS for x in WIRE_CLIP_X_CENTERS]
)

SHELL_SCREW_CENTERS = np.array(
    [[77.0, 13.0], [205.0, 13.0], [77.0, 99.0], [205.0, 99.0]]
)
COVER_SCREW_CENTERS = SHELL_SCREW_CENTERS.copy()
SHELL_BOSS_RADIUS = 5.0
SHELL_BOSS_HEIGHT = 12.0

# Two upward T-rails on the chassis slide into downward-opening slots added to
# the handset cradle. All rail and housing geometry starts on the print bed.
HANDSET_HANGER_LOCAL_Y_CENTERS = np.array([18.0, 60.8])
HANDSET_BASE_RIGHT_X = 64.5
HANDSET_MOUNT_ORIGIN = np.array(
    [
        HANDSET_BASE_RIGHT_X - handset.OUTER_WIDTH,
        (WEDGE_Y0 + WEDGE_Y1 - handset.OUTER_LENGTH) / 2.0,
        0.0,
    ]
)
HANDSET_HANGER_CLEARANCE = 0.3
HANDSET_HANGER_OUTER_X0 = 64.2
HANDSET_HANGER_OUTER_X1 = 71.0
HANDSET_HANGER_OUTER_HALF_WIDTH = 7.5
HANDSET_HANGER_HEIGHT = 20.2
HANDSET_HANGER_SLOT_MAIN_TOP = 14.0
HANDSET_HANGER_SLOT_ROOF_TOP = 19.8
HANDSET_HANGER_SLOT_ROOF_HALF_WIDTH = 0.05
HANDSET_HANGER_ENTRY_HEIGHT = 2.0
HANDSET_HANGER_ENTRY_EXTRA_CLEARANCE = 0.6
HANDSET_HANGER_HEAD_X0 = 66.2
HANDSET_HANGER_HEAD_X1 = 69.4
HANDSET_HANGER_HEAD_HALF_WIDTH = 5.5
HANDSET_HANGER_NECK_X0 = 69.2
HANDSET_HANGER_NECK_X1 = WEDGE_X0 + 0.2
HANDSET_HANGER_NECK_HALF_WIDTH = 3.5
HANDSET_HANGER_RAIL_HEIGHT = 13.6
HANDSET_HANGER_SLOT_HEAD_X0 = HANDSET_HANGER_HEAD_X0 - HANDSET_HANGER_CLEARANCE
HANDSET_HANGER_SLOT_HEAD_X1 = HANDSET_HANGER_HEAD_X1 + HANDSET_HANGER_CLEARANCE
HANDSET_HANGER_SLOT_HEAD_HALF_WIDTH = (
    HANDSET_HANGER_HEAD_HALF_WIDTH + HANDSET_HANGER_CLEARANCE
)
HANDSET_HANGER_SLOT_NECK_X0 = HANDSET_HANGER_NECK_X0 - HANDSET_HANGER_CLEARANCE
HANDSET_HANGER_SLOT_NECK_X1 = WEDGE_X0 + 1.0
HANDSET_HANGER_SLOT_NECK_HALF_WIDTH = (
    HANDSET_HANGER_NECK_HALF_WIDTH + HANDSET_HANGER_CLEARANCE
)

COVER_LENGTH = WEDGE_Y1 - WEDGE_Y0
COVER_WIDTH = WEDGE_X1 - WEDGE_X0
COVER_CENTER = ((WEDGE_X0 + WEDGE_X1) / 2.0, (WEDGE_Y0 + WEDGE_Y1) / 2.0)
COVER_THICKNESS = 2.4
COVER_HOLE_DIAMETER = 3.4
RP2040_BOARD_WIDTH = 22.86
RP2040_BOARD_LENGTH = 53.34
ESP32_S3_BOARD_WIDTH = 27.94
ESP32_S3_BOARD_LENGTH = 63.39
CONTROLLER_CENTER_X = 141.5
CONTROLLER_SIDE_CLEARANCE = 0.35
CONTROLLER_END_CLEARANCE = 0.5
CONTROLLER_CLEAR_WIDTH = ESP32_S3_BOARD_WIDTH + 2.0 * CONTROLLER_SIDE_CLEARANCE
CONTROLLER_CLEAR_LENGTH = ESP32_S3_BOARD_LENGTH + CONTROLLER_END_CLEARANCE
CONTROLLER_X0 = CONTROLLER_CENTER_X - CONTROLLER_CLEAR_WIDTH / 2.0
CONTROLLER_X1 = CONTROLLER_X0 + CONTROLLER_CLEAR_WIDTH
CONTROLLER_REAR_CLEARANCE = 5.0
CONTROLLER_Y1 = WEDGE_Y1 - WEDGE_WALL - CONTROLLER_REAR_CLEARANCE
CONTROLLER_Y0 = CONTROLLER_Y1 - CONTROLLER_CLEAR_LENGTH
CONTROLLER_PCB_THICKNESS = 1.6
CONTROLLER_RP2040_RAISE = 3.0
CONTROLLER_ESP32_S3_RAISE = 6.5
CONTROLLER_SUPPORT_RAIL_WIDTH = 1.8
CONTROLLER_SNAP_STEM_THICKNESS = 1.0
CONTROLLER_SNAP_LENGTH = 5.0
CONTROLLER_SNAP_OVERLAP = 0.3
CONTROLLER_SNAP_CLEARANCE = 0.15
CONTROLLER_SNAP_RISE = 1.0
CONTROLLER_USB_OPENING_X0 = 123.0
CONTROLLER_USB_OPENING_X1 = 160.0
CONTROLLER_USB_OPENING_Y0 = WEDGE_Y1 - WEDGE_WALL - 2.0
CONTROLLER_USB_OPENING_Y1 = WEDGE_Y1 + 1.0
CONTROLLER_USB_OPENING_Z0 = 2.0
CONTROLLER_USB_OPENING_Z1 = 11.5

DEFAULT_OUTPUT_ROOT = Path("models/3d-print/integrated-workstation")
DEFAULT_PREVIEW_ROOT = Path("/tmp/kivo-integrated-workstation-previews")
SHELL_FILENAME = "kivo_integrated_workstation_shell.stl"
PANEL_FILENAME = "kivo_integrated_workstation_sloped_panel.stl"
COVER_FILENAME = "kivo_integrated_workstation_bottom_cover.stl"
HANDSET_MOUNT_FILENAME = "telephone_handset_switch_base_workstation_mount.stl"

BOOLEAN_TOLERANCE = 5e-5
CIRCLE_SEGMENTS = 64
FRONT_ISOMETRIC_ROTATION = np.array(
    [
        [0.70710678, 0.70710678, 0.0],
        [-0.40824829, 0.40824829, 0.81649658],
        [0.57735027, -0.57735027, 0.57735027],
    ]
)
FRONT_ROTATION = np.array([[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]])


@dataclass(frozen=True)
class ValidationReport:
    shell_extents: tuple[float, float, float]
    panel_extents: tuple[float, float, float]
    cover_extents: tuple[float, float, float]
    handset_mount_extents: tuple[float, float, float]
    key_count: int
    key_layout: tuple[int, int]
    key_pitch: float
    key_plane_degrees: float
    wire_clip_count: int
    controller_bay: tuple[float, float]
    controller_support_levels: tuple[float, float]
    screen_board: tuple[float, float]
    screen_plane_degrees: float
    panel_screw_count: int
    bottom_cover_screw_count: int
    handset_hanger_count: int
    shell_watertight: bool
    panel_watertight: bool
    cover_watertight: bool
    handset_mount_watertight: bool


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


def box(lower: Iterable[float], upper: Iterable[float]) -> trimesh.Trimesh:
    lower_array = np.asarray(tuple(lower), dtype=float)
    upper_array = np.asarray(tuple(upper), dtype=float)
    result = trimesh.creation.box(extents=upper_array - lower_array)
    result.apply_translation((lower_array + upper_array) / 2.0)
    return result


def cylinder(
    radius: float,
    height: float,
    center: Iterable[float],
    axis: int = 2,
) -> trimesh.Trimesh:
    result = trimesh.creation.cylinder(
        radius=radius,
        height=height,
        sections=CIRCLE_SEGMENTS,
    )
    if axis == 0:
        result.apply_transform(
            trimesh.transformations.rotation_matrix(np.pi / 2.0, [0.0, 1.0, 0.0])
        )
    elif axis == 1:
        result.apply_transform(
            trimesh.transformations.rotation_matrix(np.pi / 2.0, [1.0, 0.0, 0.0])
        )
    elif axis != 2:
        raise ValueError(f"unsupported cylinder axis: {axis}")
    result.apply_translation(np.asarray(tuple(center), dtype=float))
    return result


def heat_set_insert_cutters(
    center: Iterable[float],
    insertion_surface_z: float,
    inward_direction: int,
) -> list[trimesh.Trimesh]:
    if inward_direction not in (-1, 1):
        raise ValueError("heat-set insert direction must be -1 or 1")
    center_xy = tuple(center)
    if len(center_xy) != 2:
        raise ValueError("heat-set insert center must contain x and y")

    outer_z = insertion_surface_z - inward_direction * HEAT_SET_INSERT_CUTTER_OVERSHOOT
    body_inner_z = insertion_surface_z + inward_direction * HEAT_SET_INSERT_HOLE_DEPTH
    lead_inner_z = insertion_surface_z + inward_direction * HEAT_SET_INSERT_LEAD_DEPTH

    def cutter(diameter: float, z_a: float, z_b: float) -> trimesh.Trimesh:
        z_min, z_max = sorted((z_a, z_b))
        return cylinder(
            diameter / 2.0,
            z_max - z_min,
            (center_xy[0], center_xy[1], (z_min + z_max) / 2.0),
        )

    return [
        cutter(HEAT_SET_INSERT_HOLE_DIAMETER, outer_z, body_inner_z),
        cutter(HEAT_SET_INSERT_LEAD_DIAMETER, outer_z, lead_inner_z),
    ]


def countersink_cutter(
    center: Iterable[float],
    exterior_surface_z: float,
    inward_direction: int,
) -> trimesh.Trimesh:
    if inward_direction not in (-1, 1):
        raise ValueError("countersink direction must be -1 or 1")
    center_xy = tuple(center)
    if len(center_xy) != 2:
        raise ValueError("countersink center must contain x and y")

    half_angle = np.deg2rad(M3_SCREW_COUNTERSINK_ANGLE_DEGREES / 2.0)
    radial_per_depth = np.tan(half_angle)
    opening_radius = M3_SCREW_HEAD_CLEARANCE_DIAMETER / 2.0
    cutter_radius = (
        opening_radius + M3_SCREW_COUNTERSINK_CUTTER_OVERSHOOT * radial_per_depth
    )
    cutter_height = cutter_radius / radial_per_depth
    result = trimesh.creation.cone(
        radius=cutter_radius,
        height=cutter_height,
        sections=CIRCLE_SEGMENTS,
    )
    if inward_direction == -1:
        result.apply_transform(
            trimesh.transformations.rotation_matrix(np.pi, [1.0, 0.0, 0.0])
        )
    result.apply_translation(
        (
            center_xy[0],
            center_xy[1],
            exterior_surface_z
            - inward_direction * M3_SCREW_COUNTERSINK_CUTTER_OVERSHOOT,
        )
    )
    return result


def subtract(
    base: trimesh.Trimesh, cutters: Iterable[trimesh.Trimesh]
) -> trimesh.Trimesh:
    solid = mesh_to_manifold(base)
    for cutter in cutters:
        solid -= mesh_to_manifold(cutter)
    result = manifold_to_mesh(solid)
    if result.is_empty or result.volume <= 0.0:
        raise ValueError("mesh subtraction produced no positive solid")
    return result


def union(parts: Iterable[trimesh.Trimesh]) -> trimesh.Trimesh:
    return macro.union_meshes(parts)


def rounded_prism(
    width: float,
    length: float,
    radius: float,
    z_min: float,
    height: float,
    center: tuple[float, float],
) -> trimesh.Trimesh:
    core = manifold3d.CrossSection.square(
        (width - 2.0 * radius, length - 2.0 * radius), center=True
    )
    section = core.offset(
        radius,
        join_type=manifold3d.JoinType.Round,
        circular_segments=CIRCLE_SEGMENTS,
    ).translate(center)
    return manifold_to_mesh(section.extrude(height).translate((0.0, 0.0, z_min)))


def hull(points: Iterable[Iterable[float]]) -> trimesh.Trimesh:
    return manifold_to_mesh(
        manifold3d.Manifold.hull_points([list(point) for point in points])
    )


def rotation_transform(angle: float, translation: Iterable[float]) -> np.ndarray:
    transform = trimesh.transformations.rotation_matrix(angle, [1.0, 0.0, 0.0])
    transform[:3, 3] = np.asarray(tuple(translation), dtype=float)
    return transform


def wedge_top(y: float) -> float:
    return WEDGE_FRONT_HEIGHT + np.tan(KEY_ANGLE) * (y - WEDGE_Y0)


DECK_TRANSFORM = rotation_transform(
    KEY_ANGLE,
    (0.0, WEDGE_Y0, WEDGE_FRONT_HEIGHT - KEY_PLATE_THICKNESS / np.cos(KEY_ANGLE)),
)


def wedge_prism(
    x0: float,
    x1: float,
    y0: float,
    y1: float,
    bottom: float,
    top_offset: float,
) -> trimesh.Trimesh:
    points = [
        [x, y, z]
        for x in (x0, x1)
        for y in (y0, y1)
        for z in (bottom, wedge_top(y) + top_offset)
    ]
    return hull(points)


def switch_cutter_local(
    column: int, row: int, size: float, z0: float, z1: float
) -> trimesh.Trimesh:
    center_x = KEY_X0 + (column + 0.5) * KEY_PITCH
    center_y = KEY_Y0 + (row + 0.5) * KEY_PITCH
    return box(
        (center_x - size / 2.0, center_y - size / 2.0, z0),
        (center_x + size / 2.0, center_y + size / 2.0, z1),
    )


def build_wedge_shell() -> trimesh.Trimesh:
    outer = wedge_prism(
        WEDGE_X0, WEDGE_X1, WEDGE_Y0, WEDGE_Y1, bottom=0.0, top_offset=0.0
    )
    inner = wedge_prism(
        WEDGE_X0 + WEDGE_WALL,
        WEDGE_X1 - WEDGE_WALL,
        WEDGE_Y0 + WEDGE_WALL,
        WEDGE_Y1 - WEDGE_WALL,
        bottom=-1.0,
        top_offset=-KEY_PLATE_THICKNESS / np.cos(KEY_ANGLE),
    )
    return subtract(outer, [inner])


def build_panel_screen_parts() -> list[trimesh.Trimesh]:
    bezel_outer = rounded_prism(
        SCREEN_BEZEL_WIDTH,
        SCREEN_BEZEL_HEIGHT,
        radius=4.0,
        z_min=KEY_PLATE_THICKNESS - 0.02,
        height=SCREEN_BEZEL_RAISE + 0.02,
        center=(
            SCREEN_BEZEL_X0 + SCREEN_BEZEL_WIDTH / 2.0,
            SCREEN_BEZEL_Y0 + SCREEN_BEZEL_HEIGHT / 2.0,
        ),
    )
    bezel_inner = rounded_prism(
        SCREEN_BOARD_WIDTH + 2.0 * SCREEN_RECESS_CLEARANCE,
        SCREEN_BOARD_HEIGHT + 2.0 * SCREEN_RECESS_CLEARANCE,
        radius=2.5,
        z_min=KEY_PLATE_THICKNESS - 0.1,
        height=SCREEN_BEZEL_RAISE + 0.3,
        center=(
            SCREEN_BEZEL_X0 + SCREEN_BEZEL_WIDTH / 2.0,
            SCREEN_BEZEL_Y0 + SCREEN_BEZEL_HEIGHT / 2.0,
        ),
    )
    bezel = subtract(bezel_outer, [bezel_inner])
    collars = [
        cylinder(
            4.8,
            SCREEN_BEZEL_RAISE + 0.02,
            (
                hole[0],
                hole[1],
                KEY_PLATE_THICKNESS + SCREEN_BEZEL_RAISE / 2.0 - 0.01,
            ),
            axis=2,
        )
        for hole in SCREEN_BOARD_HOLES + SCREEN_BOARD_ORIGIN
    ]
    return [bezel, *collars]


def panel_screen_cutters() -> list[trimesh.Trimesh]:
    cable_x0 = SCREEN_BOARD_ORIGIN[0] + SCREEN_CABLE_SLOT_LOCAL_X0
    cable_y0 = SCREEN_BOARD_ORIGIN[1] + SCREEN_CABLE_SLOT_LOCAL_Y0
    cutters = [
        box(
            (cable_x0, cable_y0, -1.0),
            (
                cable_x0 + SCREEN_CABLE_SLOT_WIDTH,
                cable_y0 + SCREEN_CABLE_SLOT_HEIGHT,
                KEY_PLATE_THICKNESS + 1.0,
            ),
        )
    ]
    for hole in SCREEN_BOARD_HOLES + SCREEN_BOARD_ORIGIN:
        # The display inserts are installed from the panel's flat underside.
        # Their 4 mm bores continue through the screen collars so the PCB-side
        # screws can enter the inserts without a trapped plastic floor.
        cutters.extend(
            [
                cylinder(
                    SCREEN_INSERT_THROUGH_DIAMETER / 2.0,
                    SCREEN_INSERT_MATERIAL_DEPTH
                    + 2.0 * HEAT_SET_INSERT_CUTTER_OVERSHOOT,
                    (
                        hole[0],
                        hole[1],
                        SCREEN_INSERT_MATERIAL_DEPTH / 2.0,
                    ),
                    axis=2,
                ),
                cylinder(
                    HEAT_SET_INSERT_LEAD_DIAMETER / 2.0,
                    HEAT_SET_INSERT_LEAD_DEPTH + HEAT_SET_INSERT_CUTTER_OVERSHOOT,
                    (
                        hole[0],
                        hole[1],
                        (HEAT_SET_INSERT_LEAD_DEPTH - HEAT_SET_INSERT_CUTTER_OVERSHOOT)
                        / 2.0,
                    ),
                    axis=2,
                ),
            ]
        )
    return cutters


def wire_clip_cutters(center: Iterable[float]) -> list[trimesh.Trimesh]:
    center_xy = tuple(center)
    if len(center_xy) != 2:
        raise ValueError("wire clip center must contain x and y")
    center_x, center_y = center_xy
    x0 = center_x - WIRE_CLIP_LENGTH / 2.0
    x1 = center_x + WIRE_CLIP_LENGTH / 2.0
    transition_z1 = WIRE_CLIP_MOUTH_DEPTH + WIRE_CLIP_TRANSITION_DEPTH

    mouth = box(
        (x0, center_y - WIRE_CLIP_MOUTH_WIDTH / 2.0, -1.0),
        (x1, center_y + WIRE_CLIP_MOUTH_WIDTH / 2.0, WIRE_CLIP_MOUTH_DEPTH),
    )
    transition = hull(
        [
            [x, center_y + side * width / 2.0, z]
            for x in (x0, x1)
            for width, z in (
                (WIRE_CLIP_MOUTH_WIDTH, WIRE_CLIP_MOUTH_DEPTH - 0.02),
                (WIRE_CLIP_POCKET_WIDTH, transition_z1 + 0.02),
            )
            for side in (-1.0, 1.0)
        ]
    )
    pocket = box(
        (x0, center_y - WIRE_CLIP_POCKET_WIDTH / 2.0, transition_z1),
        (x1, center_y + WIRE_CLIP_POCKET_WIDTH / 2.0, WIRE_CLIP_POCKET_DEPTH),
    )
    return [mouth, transition, pocket]


def panel_attachment_cutters() -> list[trimesh.Trimesh]:
    through_holes = [
        cylinder(
            PANEL_HOLE_DIAMETER / 2.0,
            KEY_PLATE_THICKNESS + 2.0,
            (center[0], center[1], KEY_PLATE_THICKNESS / 2.0),
        )
        for center in PANEL_SCREW_CENTERS
    ]
    countersinks = [
        countersink_cutter(center, KEY_PLATE_THICKNESS, -1)
        for center in PANEL_SCREW_CENTERS
    ]
    return [*through_holes, *countersinks]


def generate_sloped_panel() -> trimesh.Trimesh:
    plate = rounded_prism(
        PANEL_X1 - PANEL_X0,
        PANEL_Y1 - PANEL_Y0,
        radius=3.0,
        z_min=0.0,
        height=KEY_PLATE_THICKNESS,
        center=((PANEL_X0 + PANEL_X1) / 2.0, (PANEL_Y0 + PANEL_Y1) / 2.0),
    )
    combined = union([plate, *build_panel_screen_parts()])
    cutters: list[trimesh.Trimesh] = []
    for row in range(KEY_ROWS):
        for column in range(KEY_COLUMNS):
            cutters.append(
                switch_cutter_local(
                    column,
                    row,
                    LOWER_SWITCH_APERTURE,
                    -1.0,
                    KEY_STEP_HEIGHT + 0.001,
                )
            )
            cutters.append(
                switch_cutter_local(
                    column,
                    row,
                    UPPER_SWITCH_APERTURE,
                    KEY_STEP_HEIGHT - 0.001,
                    KEY_PLATE_THICKNESS + 1.0,
                )
            )
    cutters.extend(panel_screen_cutters())
    cutters.extend(
        cutter for center in WIRE_CLIP_CENTERS for cutter in wire_clip_cutters(center)
    )
    cutters.extend(panel_attachment_cutters())
    result = subtract(combined, cutters)
    result.apply_translation([-PANEL_X0, -PANEL_Y0, 0.0])
    result.merge_vertices()
    result.remove_unreferenced_vertices()
    return result


def panel_opening_cutter() -> trimesh.Trimesh:
    cutter = box(
        (
            PANEL_X0 - PANEL_CLEARANCE,
            PANEL_Y0 - PANEL_CLEARANCE,
            -0.05,
        ),
        (
            PANEL_X1 + PANEL_CLEARANCE,
            PANEL_Y1 + PANEL_OPENING_REAR_OVERCUT,
            KEY_PLATE_THICKNESS + SCREEN_BEZEL_RAISE + 1.0,
        ),
    )
    cutter.apply_transform(DECK_TRANSFORM)
    return cutter


def build_panel_support_parts() -> list[trimesh.Trimesh]:
    parts = [
        box(
            (WEDGE_X0, 0.0, -PANEL_LIP_DEPTH),
            (82.0, PANEL_SIDE_SUPPORT_Y1, 0.0),
        ),
        box(
            (200.0, 0.0, -PANEL_LIP_DEPTH),
            (WEDGE_X1, PANEL_SIDE_SUPPORT_Y1, 0.0),
        ),
        box((82.0, 0.0, -PANEL_LIP_DEPTH), (200.0, 9.0, 0.0)),
    ]
    parts.extend(
        cylinder(
            PANEL_INSERT_BOSS_RADIUS,
            PANEL_INSERT_BOSS_DEPTH,
            (center[0], center[1], -PANEL_INSERT_BOSS_DEPTH / 2.0),
        )
        for center in PANEL_SCREW_CENTERS
    )
    for part in parts:
        part.apply_transform(DECK_TRANSFORM)
    return parts


def panel_insert_cutters() -> list[trimesh.Trimesh]:
    cutters = [
        cutter
        for center in PANEL_SCREW_CENTERS
        for cutter in heat_set_insert_cutters(center, 0.0, -1)
    ]
    for cutter in cutters:
        cutter.apply_transform(DECK_TRANSFORM)
    return cutters


def build_shell_bosses() -> list[trimesh.Trimesh]:
    return [
        cylinder(
            SHELL_BOSS_RADIUS,
            SHELL_BOSS_HEIGHT,
            (center[0], center[1], SHELL_BOSS_HEIGHT / 2.0),
        )
        for center in SHELL_SCREW_CENTERS
    ]


def build_handset_hanger_rails() -> list[trimesh.Trimesh]:
    parts: list[trimesh.Trimesh] = []
    for local_y in HANDSET_HANGER_LOCAL_Y_CENTERS:
        world_y = HANDSET_MOUNT_ORIGIN[1] + local_y
        parts.extend(
            [
                box(
                    (
                        HANDSET_HANGER_HEAD_X0,
                        world_y - HANDSET_HANGER_HEAD_HALF_WIDTH,
                        0.0,
                    ),
                    (
                        HANDSET_HANGER_HEAD_X1,
                        world_y + HANDSET_HANGER_HEAD_HALF_WIDTH,
                        HANDSET_HANGER_RAIL_HEIGHT,
                    ),
                ),
                box(
                    (
                        HANDSET_HANGER_NECK_X0,
                        world_y - HANDSET_HANGER_NECK_HALF_WIDTH,
                        0.0,
                    ),
                    (
                        HANDSET_HANGER_NECK_X1,
                        world_y + HANDSET_HANGER_NECK_HALF_WIDTH,
                        HANDSET_HANGER_RAIL_HEIGHT,
                    ),
                ),
            ]
        )
    return parts


def handset_hanger_housings() -> list[trimesh.Trimesh]:
    x0 = HANDSET_HANGER_OUTER_X0 - HANDSET_MOUNT_ORIGIN[0]
    x1 = HANDSET_HANGER_OUTER_X1 - HANDSET_MOUNT_ORIGIN[0]
    return [
        box(
            (x0, center_y - HANDSET_HANGER_OUTER_HALF_WIDTH, 0.0),
            (
                x1,
                center_y + HANDSET_HANGER_OUTER_HALF_WIDTH,
                HANDSET_HANGER_HEIGHT,
            ),
        )
        for center_y in HANDSET_HANGER_LOCAL_Y_CENTERS
    ]


def tapered_slot_cutter(
    x0: float,
    x1: float,
    center_y: float,
    lower_half_width: float,
    upper_half_width: float,
    z0: float,
    z1: float,
    lower_x_extra: float = 0.0,
) -> trimesh.Trimesh:
    return hull(
        [
            [x, y, z]
            for z, x_extra, half_width in (
                (z0, lower_x_extra, lower_half_width),
                (z1, 0.0, upper_half_width),
            )
            for x in (x0 - x_extra, x1 + x_extra)
            for y in (center_y - half_width, center_y + half_width)
        ]
    )


def handset_hanger_slot_cutters(center_y: float) -> list[trimesh.Trimesh]:
    origin_x = HANDSET_MOUNT_ORIGIN[0]
    head_x0 = HANDSET_HANGER_SLOT_HEAD_X0 - origin_x
    head_x1 = HANDSET_HANGER_SLOT_HEAD_X1 - origin_x
    neck_x0 = HANDSET_HANGER_SLOT_NECK_X0 - origin_x
    neck_x1 = HANDSET_HANGER_SLOT_NECK_X1 - origin_x
    entry_extra = HANDSET_HANGER_ENTRY_EXTRA_CLEARANCE
    roof_z0 = HANDSET_HANGER_SLOT_MAIN_TOP - 0.02

    return [
        box(
            (head_x0, center_y - HANDSET_HANGER_SLOT_HEAD_HALF_WIDTH, -1.0),
            (
                head_x1,
                center_y + HANDSET_HANGER_SLOT_HEAD_HALF_WIDTH,
                HANDSET_HANGER_SLOT_MAIN_TOP,
            ),
        ),
        box(
            (neck_x0, center_y - HANDSET_HANGER_SLOT_NECK_HALF_WIDTH, -1.0),
            (
                neck_x1,
                center_y + HANDSET_HANGER_SLOT_NECK_HALF_WIDTH,
                HANDSET_HANGER_SLOT_MAIN_TOP,
            ),
        ),
        tapered_slot_cutter(
            head_x0,
            head_x1,
            center_y,
            HANDSET_HANGER_SLOT_HEAD_HALF_WIDTH + entry_extra,
            HANDSET_HANGER_SLOT_HEAD_HALF_WIDTH,
            -0.5,
            HANDSET_HANGER_ENTRY_HEIGHT,
            lower_x_extra=entry_extra,
        ),
        tapered_slot_cutter(
            neck_x0,
            neck_x1,
            center_y,
            HANDSET_HANGER_SLOT_NECK_HALF_WIDTH + entry_extra,
            HANDSET_HANGER_SLOT_NECK_HALF_WIDTH,
            -0.5,
            HANDSET_HANGER_ENTRY_HEIGHT,
            lower_x_extra=entry_extra,
        ),
        tapered_slot_cutter(
            head_x0,
            head_x1,
            center_y,
            HANDSET_HANGER_SLOT_HEAD_HALF_WIDTH,
            HANDSET_HANGER_SLOT_ROOF_HALF_WIDTH,
            roof_z0,
            HANDSET_HANGER_SLOT_ROOF_TOP,
        ),
        tapered_slot_cutter(
            neck_x0,
            neck_x1,
            center_y,
            HANDSET_HANGER_SLOT_NECK_HALF_WIDTH,
            HANDSET_HANGER_SLOT_ROOF_HALF_WIDTH,
            roof_z0,
            HANDSET_HANGER_SLOT_ROOF_TOP,
        ),
    ]


def shell_cutters() -> list[trimesh.Trimesh]:
    cutters = [
        # The rear opening accepts the RP2040 single USB-C connector or the
        # ESP32-S3 board's two adjacent USB-C connectors.
        box(
            (
                CONTROLLER_USB_OPENING_X0,
                CONTROLLER_USB_OPENING_Y0,
                CONTROLLER_USB_OPENING_Z0,
            ),
            (
                CONTROLLER_USB_OPENING_X1,
                CONTROLLER_USB_OPENING_Y1,
                CONTROLLER_USB_OPENING_Z1,
            ),
        )
    ]
    cutters.extend(panel_insert_cutters())
    cutters.extend(
        cutter
        for center in COVER_SCREW_CENTERS
        for cutter in heat_set_insert_cutters(center, 0.0, 1)
    )
    return cutters


def generate_shell() -> trimesh.Trimesh:
    open_wedge = subtract(build_wedge_shell(), [panel_opening_cutter()])
    combined = union(
        [
            open_wedge,
            *build_panel_support_parts(),
            *build_shell_bosses(),
            *build_handset_hanger_rails(),
        ]
    )
    result = subtract(combined, shell_cutters())
    result.merge_vertices()
    result.remove_unreferenced_vertices()
    return result


def generate_handset_mount() -> trimesh.Trimesh:
    base = handset.generate_base()
    joined = union([base, *handset_hanger_housings()])
    cutters = [
        cutter
        for center_y in HANDSET_HANGER_LOCAL_Y_CENTERS
        for cutter in handset_hanger_slot_cutters(center_y)
    ]
    result = subtract(joined, cutters)
    result.merge_vertices()
    result.remove_unreferenced_vertices()
    return result


def controller_board_bounds(
    width: float, length: float
) -> tuple[float, float, float, float]:
    x0 = CONTROLLER_CENTER_X - width / 2.0
    return x0, CONTROLLER_Y1 - length, x0 + width, CONTROLLER_Y1


def build_controller_snap_tabs(
    width: float, length: float, support_raise: float
) -> list[trimesh.Trimesh]:
    board_x0, board_y0, board_x1, _ = controller_board_bounds(width, length)
    board_top_z = COVER_THICKNESS + support_raise + CONTROLLER_PCB_THICKNESS
    tab_center_y = board_y0 + length * 0.55
    tab_y0 = tab_center_y - CONTROLLER_SNAP_LENGTH / 2.0
    tab_y1 = tab_center_y + CONTROLLER_SNAP_LENGTH / 2.0
    stem_z0 = COVER_THICKNESS - 0.02
    stem_z1 = board_top_z + CONTROLLER_SNAP_RISE

    parts: list[trimesh.Trimesh] = []
    for side, board_edge in ((-1, board_x0), (1, board_x1)):
        stem_inner_x = board_edge + side * CONTROLLER_SIDE_CLEARANCE
        stem_outer_x = stem_inner_x + side * CONTROLLER_SNAP_STEM_THICKNESS
        parts.append(
            box(
                (
                    min(stem_inner_x, stem_outer_x) - 0.05,
                    tab_y0,
                    stem_z0,
                ),
                (
                    max(stem_inner_x, stem_outer_x) + 0.05,
                    tab_y1,
                    stem_z1,
                ),
            )
        )

        nub_tip_x = board_edge - side * CONTROLLER_SNAP_OVERLAP
        nub_base_x = stem_inner_x - side * 0.05
        nub_bottom_z = board_top_z + CONTROLLER_SNAP_CLEARANCE
        nub_points = [
            [x, y, z]
            for y in (tab_y0, tab_y1)
            for x, z in (
                (nub_base_x, nub_bottom_z),
                (nub_tip_x, nub_bottom_z),
                (nub_base_x, stem_z1),
            )
        ]
        parts.append(hull(nub_points))
    return parts


def build_controller_support_level(
    width: float, length: float, support_raise: float
) -> list[trimesh.Trimesh]:
    board_x0, board_y0, board_x1, board_y1 = controller_board_bounds(width, length)
    support_z = COVER_THICKNESS + support_raise
    overlap_z = COVER_THICKNESS - 0.02
    mounts: list[trimesh.Trimesh] = [
        box(
            (
                board_x0 - CONTROLLER_SIDE_CLEARANCE,
                board_y0 - CONTROLLER_END_CLEARANCE,
                overlap_z,
            ),
            (
                board_x0 + CONTROLLER_SUPPORT_RAIL_WIDTH,
                board_y1 - 2.0,
                support_z,
            ),
        ),
        box(
            (
                board_x1 - CONTROLLER_SUPPORT_RAIL_WIDTH,
                board_y0 - CONTROLLER_END_CLEARANCE,
                overlap_z,
            ),
            (
                board_x1 + CONTROLLER_SIDE_CLEARANCE,
                board_y1 - 2.0,
                support_z,
            ),
        ),
        box(
            (
                board_x0,
                board_y0 - CONTROLLER_END_CLEARANCE,
                overlap_z,
            ),
            (
                board_x1,
                board_y0,
                support_z + CONTROLLER_PCB_THICKNESS / 2.0,
            ),
        ),
    ]
    mounts.extend(build_controller_snap_tabs(width, length, support_raise))
    return mounts


def build_controller_mounts() -> list[trimesh.Trimesh]:
    return [
        *build_controller_support_level(
            RP2040_BOARD_WIDTH,
            RP2040_BOARD_LENGTH,
            CONTROLLER_RP2040_RAISE,
        ),
        *build_controller_support_level(
            ESP32_S3_BOARD_WIDTH,
            ESP32_S3_BOARD_LENGTH,
            CONTROLLER_ESP32_S3_RAISE,
        ),
    ]


def cover_cutters() -> list[trimesh.Trimesh]:
    cutters: list[trimesh.Trimesh] = []
    cutters.extend(
        cylinder(
            COVER_HOLE_DIAMETER / 2.0,
            COVER_THICKNESS + 2.0,
            (center[0], center[1], COVER_THICKNESS / 2.0),
        )
        for center in COVER_SCREW_CENTERS
    )
    cutters.extend(countersink_cutter(center, 0.0, 1) for center in COVER_SCREW_CENTERS)
    for y0 in (26.0, 38.0, 50.0, 62.0, 74.0):
        cutters.append(box((92.0, y0, -1.0), (114.0, y0 + 2.4, COVER_THICKNESS + 1.0)))
    return cutters


def generate_cover() -> trimesh.Trimesh:
    main_plate = rounded_prism(
        COVER_WIDTH,
        COVER_LENGTH,
        radius=4.0,
        z_min=0.0,
        height=COVER_THICKNESS,
        center=COVER_CENTER,
    )
    combined = union([main_plate, *build_controller_mounts()])
    result = subtract(combined, cover_cutters())
    result.merge_vertices()
    result.remove_unreferenced_vertices()
    return result


def expected_switch_centers() -> np.ndarray:
    return np.array(
        [
            [
                KEY_X0 + (column + 0.5) * KEY_PITCH - PANEL_X0,
                KEY_Y0 + (row + 0.5) * KEY_PITCH - PANEL_Y0,
            ]
            for row in range(KEY_ROWS)
            for column in range(KEY_COLUMNS)
        ]
    )


def validate_switch_geometry(panel: trimesh.Trimesh) -> None:
    lower = macro.measure_switch_section(panel, 1.0, LOWER_SWITCH_APERTURE)
    upper = macro.measure_switch_section(panel, 2.7, UPPER_SWITCH_APERTURE)
    expected = expected_switch_centers()
    if len(lower.centers) != KEY_COLUMNS * KEY_ROWS:
        raise ValueError(f"lower switch count drifted: {len(lower.centers)}")
    if len(upper.centers) != KEY_COLUMNS * KEY_ROWS:
        raise ValueError(f"upper switch count drifted: {len(upper.centers)}")
    if not np.allclose(lower.centers, expected, rtol=0.0, atol=0.003):
        raise ValueError("lower switch centers drifted")
    if not np.allclose(upper.centers, expected, rtol=0.0, atol=0.003):
        raise ValueError("upper switch centers drifted")
    if not np.allclose(lower.sizes, LOWER_SWITCH_APERTURE, rtol=0.0, atol=0.003):
        raise ValueError("lower switch apertures drifted")
    if not np.allclose(upper.sizes, UPPER_SWITCH_APERTURE, rtol=0.0, atol=0.003):
        raise ValueError("upper switch apertures drifted")
    if not np.allclose(np.diff(lower.x_levels), KEY_PITCH, atol=0.003):
        raise ValueError("switch column pitch drifted")
    if not np.allclose(np.diff(lower.y_levels), KEY_PITCH, atol=0.003):
        raise ValueError("switch row pitch drifted")


def validate_screen_header_access(panel: trimesh.Trimesh) -> None:
    slot_x1 = SCREEN_CABLE_SLOT_LOCAL_X0 + SCREEN_CABLE_SLOT_WIDTH
    slot_y1 = SCREEN_CABLE_SLOT_LOCAL_Y0 + SCREEN_CABLE_SLOT_HEIGHT
    if SCREEN_HEADER_PIN_CENTERS[:, 0].max() >= SCREEN_BOARD_WIDTH / 2.0:
        raise ValueError("screen header is not on the PCB's left half")
    if not np.all(
        (SCREEN_HEADER_PIN_CENTERS[:, 0] > SCREEN_CABLE_SLOT_LOCAL_X0)
        & (SCREEN_HEADER_PIN_CENTERS[:, 0] < slot_x1)
        & (SCREEN_HEADER_PIN_CENTERS[:, 1] > SCREEN_CABLE_SLOT_LOCAL_Y0)
        & (SCREEN_HEADER_PIN_CENTERS[:, 1] < slot_y1)
    ):
        raise ValueError("screen cable slot does not cover all header pins")

    for pin in SCREEN_HEADER_PIN_CENTERS:
        panel_pin = SCREEN_BOARD_ORIGIN + pin - np.array([PANEL_X0, PANEL_Y0])
        probe = cylinder(
            0.5,
            KEY_PLATE_THICKNESS + 0.4,
            (panel_pin[0], panel_pin[1], KEY_PLATE_THICKNESS / 2.0),
        )
        if intersection_volume(panel, probe) > 0.01:
            raise ValueError(f"screen header pin access is blocked: {pin}")


def validate_screen_insert_holes(panel: trimesh.Trimesh) -> None:
    if SCREEN_INSERT_MATERIAL_DEPTH < HEAT_SET_INSERT_LENGTH:
        raise ValueError("screen mounting collars are too shallow for the inserts")

    for screen_center in SCREEN_BOARD_HOLES + SCREEN_BOARD_ORIGIN:
        panel_center = screen_center - np.array([PANEL_X0, PANEL_Y0])
        through_probe = cylinder(
            SCREEN_INSERT_THROUGH_DIAMETER / 2.0 - 0.05,
            SCREEN_INSERT_MATERIAL_DEPTH + 0.2,
            (
                panel_center[0],
                panel_center[1],
                SCREEN_INSERT_MATERIAL_DEPTH / 2.0,
            ),
        )
        if intersection_volume(panel, through_probe) > 0.01:
            raise ValueError(f"screen insert through hole is blocked: {screen_center}")

        lead_probe = cylinder(
            HEAT_SET_INSERT_LEAD_DIAMETER / 2.0 - 0.05,
            HEAT_SET_INSERT_LEAD_DEPTH - 0.1,
            (
                panel_center[0],
                panel_center[1],
                HEAT_SET_INSERT_LEAD_DEPTH / 2.0,
            ),
        )
        if intersection_volume(panel, lead_probe) > 0.01:
            raise ValueError(f"screen insert lead-in is blocked: {screen_center}")


def validate_wire_clips(panel: trimesh.Trimesh) -> None:
    side_expansion = (WIRE_CLIP_POCKET_WIDTH - WIRE_CLIP_MOUTH_WIDTH) / 2.0
    if side_expansion > WIRE_CLIP_TRANSITION_DEPTH + 1e-9:
        raise ValueError("wire clip transition exceeds a support-free 45 degree slope")
    if WIRE_CLIP_FRONT_SKIN < 1.0:
        raise ValueError("wire clips leave too little material on the panel face")

    for design_center in WIRE_CLIP_CENTERS:
        center = design_center - np.array([PANEL_X0, PANEL_Y0])
        mouth_probe = box(
            (
                center[0] - WIRE_CLIP_LENGTH / 2.0 + 0.1,
                center[1] - WIRE_CLIP_MOUTH_WIDTH / 2.0 + 0.05,
                0.05,
            ),
            (
                center[0] + WIRE_CLIP_LENGTH / 2.0 - 0.1,
                center[1] + WIRE_CLIP_MOUTH_WIDTH / 2.0 - 0.05,
                WIRE_CLIP_MOUTH_DEPTH - 0.05,
            ),
        )
        if intersection_volume(panel, mouth_probe) > 0.01:
            raise ValueError(f"wire clip mouth is blocked: {design_center}")

        pocket_probe = box(
            (
                center[0] - WIRE_CLIP_LENGTH / 2.0 + 0.1,
                center[1] - WIRE_CLIP_POCKET_WIDTH / 2.0 + 0.05,
                WIRE_CLIP_MOUTH_DEPTH + WIRE_CLIP_TRANSITION_DEPTH + 0.05,
            ),
            (
                center[0] + WIRE_CLIP_LENGTH / 2.0 - 0.1,
                center[1] + WIRE_CLIP_POCKET_WIDTH / 2.0 - 0.05,
                WIRE_CLIP_POCKET_DEPTH - 0.05,
            ),
        )
        if intersection_volume(panel, pocket_probe) > 0.01:
            raise ValueError(f"wire clip pocket is blocked: {design_center}")

        skin_probe = box(
            (
                center[0] - WIRE_CLIP_LENGTH / 2.0 + 0.2,
                center[1] - WIRE_CLIP_POCKET_WIDTH / 2.0 + 0.1,
                WIRE_CLIP_POCKET_DEPTH + 0.1,
            ),
            (
                center[0] + WIRE_CLIP_LENGTH / 2.0 - 0.2,
                center[1] + WIRE_CLIP_POCKET_WIDTH / 2.0 - 0.1,
                KEY_PLATE_THICKNESS - 0.1,
            ),
        )
        if intersection_volume(panel, skin_probe) < skin_probe.volume - 0.02:
            raise ValueError(
                f"wire clip breaks through the panel face: {design_center}"
            )


def validate_controller_connector_opening(shell: trimesh.Trimesh) -> None:
    rear_probe = box(
        (
            CONTROLLER_USB_OPENING_X0 + 1.0,
            WEDGE_Y1 - WEDGE_WALL + 0.1,
            CONTROLLER_USB_OPENING_Z0 + 0.5,
        ),
        (
            CONTROLLER_USB_OPENING_X1 - 1.0,
            WEDGE_Y1 - 0.1,
            CONTROLLER_USB_OPENING_Z1 - 0.5,
        ),
    )
    if intersection_volume(shell, rear_probe) > 0.01:
        raise ValueError("rear Type-C opening is blocked")

    front_probe = box(
        (
            CONTROLLER_USB_OPENING_X0 + 1.0,
            WEDGE_Y0 + 0.1,
            CONTROLLER_USB_OPENING_Z0 + 0.5,
        ),
        (
            CONTROLLER_USB_OPENING_X1 - 1.0,
            WEDGE_Y0 + WEDGE_WALL - 0.1,
            CONTROLLER_USB_OPENING_Z1 - 0.5,
        ),
    )
    if intersection_volume(shell, front_probe) < front_probe.volume - 0.05:
        raise ValueError("front wall is not closed at the former Type-C opening")

    connector_gap = CONTROLLER_USB_OPENING_Y0 - CONTROLLER_Y1
    if not 2.0 <= connector_gap <= 5.0:
        raise ValueError(f"controller-to-rear-opening gap drifted: {connector_gap}")


def validate_controller_cradle(cover: trimesh.Trimesh) -> None:
    lower_snap_top = (
        COVER_THICKNESS
        + CONTROLLER_RP2040_RAISE
        + CONTROLLER_PCB_THICKNESS
        + CONTROLLER_SNAP_RISE
    )
    upper_board_bottom = COVER_THICKNESS + CONTROLLER_ESP32_S3_RAISE
    if lower_snap_top >= upper_board_bottom:
        raise ValueError("RP2040 snap tabs collide with the upper ESP32-S3 tier")

    for label, width, length, support_raise in (
        (
            "RP2040",
            RP2040_BOARD_WIDTH,
            RP2040_BOARD_LENGTH,
            CONTROLLER_RP2040_RAISE,
        ),
        (
            "ESP32-S3",
            ESP32_S3_BOARD_WIDTH,
            ESP32_S3_BOARD_LENGTH,
            CONTROLLER_ESP32_S3_RAISE,
        ),
    ):
        x0, y0, x1, y1 = controller_board_bounds(width, length)
        board_probe = box(
            (
                x0 + 0.1,
                y0 + 0.1,
                COVER_THICKNESS + support_raise + 0.05,
            ),
            (
                x1 - 0.1,
                y1 - 0.1,
                COVER_THICKNESS + support_raise + CONTROLLER_PCB_THICKNESS - 0.05,
            ),
        )
        if intersection_volume(cover, board_probe) > 0.02:
            raise ValueError(f"{label} board volume is blocked in the snap cradle")

    # The former cable-tie slots must now be closed by the solid cover plate.
    for slot_center_y in (60.0, 83.0):
        closed_slot_probe = box(
            (123.0, slot_center_y - 1.0, 0.2),
            (160.0, slot_center_y + 1.0, COVER_THICKNESS - 0.2),
        )
        if intersection_volume(cover, closed_slot_probe) < (
            closed_slot_probe.volume - 0.05
        ):
            raise ValueError(f"former cable-tie slot remains open at y={slot_center_y}")


def intersection_volume(mesh: trimesh.Trimesh, probe: trimesh.Trimesh) -> float:
    intersection = macro.boolean_meshes([mesh, probe], "intersection")
    return 0.0 if intersection.is_empty else float(intersection.volume)


def validate_countersink_opening(
    mesh: trimesh.Trimesh,
    center: Iterable[float],
    exterior_surface_z: float,
    inward_direction: int,
    label: str,
) -> None:
    center_xy = tuple(center)
    probe_depth = 0.08
    probe = cylinder(
        M3_SCREW_HEAD_DIAMETER / 2.0 - 0.03,
        probe_depth,
        (
            center_xy[0],
            center_xy[1],
            exterior_surface_z + inward_direction * probe_depth / 2.0,
        ),
    )
    if intersection_volume(mesh, probe) > 0.01:
        raise ValueError(f"{label} countersink is blocked: {center_xy}")


def place_sloped_panel(panel: trimesh.Trimesh) -> trimesh.Trimesh:
    placed = panel.copy()
    placed.apply_translation([PANEL_X0, PANEL_Y0, 0.0])
    placed.apply_transform(DECK_TRANSFORM)
    return placed


def validate_panel_attachment(shell: trimesh.Trimesh, panel: trimesh.Trimesh) -> None:
    if not np.isclose(panel.bounds[0, 2], 0.0, atol=0.003):
        raise ValueError("sloped panel does not have a flat print underside")
    for center in PANEL_SCREW_CENTERS:
        panel_center = center - np.array([PANEL_X0, PANEL_Y0])
        panel_probe = cylinder(
            PANEL_HOLE_DIAMETER / 2.0 - 0.05,
            KEY_PLATE_THICKNESS,
            (panel_center[0], panel_center[1], KEY_PLATE_THICKNESS / 2.0),
        )
        if intersection_volume(panel, panel_probe) > 0.01:
            raise ValueError(f"panel attachment hole is blocked: {center}")
        validate_countersink_opening(
            panel,
            panel_center,
            KEY_PLATE_THICKNESS,
            -1,
            "sloped panel",
        )

        insert_probe = cylinder(
            HEAT_SET_INSERT_HOLE_DIAMETER / 2.0 - 0.05,
            HEAT_SET_INSERT_HOLE_DEPTH - 0.2,
            (center[0], center[1], -HEAT_SET_INSERT_HOLE_DEPTH / 2.0),
        )
        insert_probe.apply_transform(DECK_TRANSFORM)
        if intersection_volume(shell, insert_probe) > 0.01:
            raise ValueError(f"panel insert hole is blocked: {center}")

        lead_probe = cylinder(
            HEAT_SET_INSERT_LEAD_DIAMETER / 2.0 - 0.05,
            HEAT_SET_INSERT_LEAD_DEPTH - 0.1,
            (center[0], center[1], -HEAT_SET_INSERT_LEAD_DEPTH / 2.0),
        )
        lead_probe.apply_transform(DECK_TRANSFORM)
        if intersection_volume(shell, lead_probe) > 0.01:
            raise ValueError(f"panel insert lead-in is blocked: {center}")

        floor_probe = cylinder(
            1.0,
            HEAT_SET_INSERT_BLIND_FLOOR / 2.0,
            (
                center[0],
                center[1],
                -HEAT_SET_INSERT_HOLE_DEPTH - HEAT_SET_INSERT_BLIND_FLOOR / 4.0,
            ),
        )
        floor_probe.apply_transform(DECK_TRANSFORM)
        if intersection_volume(shell, floor_probe) < 0.5:
            raise ValueError(f"panel insert hole is not blind: {center}")

    collision = macro.boolean_meshes([shell, place_sloped_panel(panel)], "intersection")
    if not collision.is_empty and collision.volume > 0.05:
        raise ValueError(f"sloped panel assembly collision: {collision.volume}")

    rear_strip_probe = box(
        (90.0, PANEL_Y1 + PANEL_CLEARANCE + 0.05, 0.1),
        (192.0, PANEL_Y1 + 1.2, KEY_PLATE_THICKNESS - 0.1),
    )
    rear_strip_probe.apply_transform(DECK_TRANSFORM)
    if intersection_volume(shell, rear_strip_probe) > 0.01:
        raise ValueError("panel opening leaves an unsupported rear roof strip")
    if shell.bounds[1, 1] > WEDGE_Y1 + 0.003:
        raise ValueError(
            f"panel supports protrude behind chassis: y={shell.bounds[1, 1]}"
        )
    placed_panel = place_sloped_panel(panel)
    if placed_panel.bounds[1, 1] > WEDGE_Y1 + 0.003:
        raise ValueError(
            f"sloped panel protrudes behind chassis: y={placed_panel.bounds[1, 1]}"
        )


def validate_bottom_cover_attachment(
    shell: trimesh.Trimesh, cover: trimesh.Trimesh
) -> None:
    expected_bounds = np.array(
        [[WEDGE_X0, WEDGE_Y0], [WEDGE_X1, WEDGE_Y1]], dtype=float
    )
    if not np.allclose(cover.bounds[:, :2], expected_bounds, atol=0.003):
        raise ValueError(f"bottom cover footprint drifted: {cover.bounds[:, :2]}")

    for center in COVER_SCREW_CENTERS:
        cover_probe = cylinder(
            COVER_HOLE_DIAMETER / 2.0 - 0.05,
            COVER_THICKNESS,
            (center[0], center[1], COVER_THICKNESS / 2.0),
        )
        if intersection_volume(cover, cover_probe) > 0.01:
            raise ValueError(f"bottom cover screw hole is blocked: {center}")
        validate_countersink_opening(
            cover,
            center,
            0.0,
            1,
            "bottom cover",
        )

        insert_probe = cylinder(
            HEAT_SET_INSERT_HOLE_DIAMETER / 2.0 - 0.05,
            HEAT_SET_INSERT_HOLE_DEPTH - 0.2,
            (center[0], center[1], HEAT_SET_INSERT_HOLE_DEPTH / 2.0),
        )
        if intersection_volume(shell, insert_probe) > 0.01:
            raise ValueError(f"bottom cover insert hole is blocked: {center}")

        lead_probe = cylinder(
            HEAT_SET_INSERT_LEAD_DIAMETER / 2.0 - 0.05,
            HEAT_SET_INSERT_LEAD_DEPTH - 0.1,
            (center[0], center[1], HEAT_SET_INSERT_LEAD_DEPTH / 2.0),
        )
        if intersection_volume(shell, lead_probe) > 0.01:
            raise ValueError(f"bottom cover insert lead-in is blocked: {center}")

        floor_probe = cylinder(
            1.0,
            HEAT_SET_INSERT_BLIND_FLOOR / 2.0,
            (
                center[0],
                center[1],
                HEAT_SET_INSERT_HOLE_DEPTH + HEAT_SET_INSERT_BLIND_FLOOR / 4.0,
            ),
        )
        if intersection_volume(shell, floor_probe) < 0.5:
            raise ValueError(f"bottom cover insert hole is not blind: {center}")


def place_handset_mount(handset_mount: trimesh.Trimesh) -> trimesh.Trimesh:
    placed = handset_mount.copy()
    placed.apply_translation(HANDSET_MOUNT_ORIGIN)
    return placed


def validate_handset_mount_attachment(
    shell: trimesh.Trimesh, handset_mount: trimesh.Trimesh
) -> None:
    roof_rise = HANDSET_HANGER_SLOT_ROOF_TOP - HANDSET_HANGER_SLOT_MAIN_TOP
    roof_run = HANDSET_HANGER_SLOT_HEAD_HALF_WIDTH - HANDSET_HANGER_SLOT_ROOF_HALF_WIDTH
    if roof_run > roof_rise + 1e-9:
        raise ValueError(
            "handset hanger slot roof exceeds a support-free 45 degree slope"
        )
    if HANDSET_HANGER_SLOT_MAIN_TOP - HANDSET_HANGER_RAIL_HEIGHT < 0.3:
        raise ValueError("handset hanger lacks vertical insertion clearance")
    if HANDSET_HANGER_HEAD_HALF_WIDTH <= HANDSET_HANGER_SLOT_NECK_HALF_WIDTH:
        raise ValueError("handset hanger head cannot retain the side slot")

    placed = place_handset_mount(handset_mount)
    collision = macro.boolean_meshes([shell, placed], "intersection")
    if not collision.is_empty and collision.volume > 0.03:
        raise ValueError(
            f"handset side mount collides with chassis: {collision.volume}"
        )

    local_head_x0 = HANDSET_HANGER_HEAD_X0 - HANDSET_MOUNT_ORIGIN[0]
    local_head_x1 = HANDSET_HANGER_HEAD_X1 - HANDSET_MOUNT_ORIGIN[0]
    local_neck_x0 = HANDSET_HANGER_NECK_X0 - HANDSET_MOUNT_ORIGIN[0]
    local_neck_x1 = HANDSET_HANGER_NECK_X1 - HANDSET_MOUNT_ORIGIN[0]
    for local_y in HANDSET_HANGER_LOCAL_Y_CENTERS:
        world_y = HANDSET_MOUNT_ORIGIN[1] + local_y
        head_probe = box(
            (
                local_head_x0 + 0.05,
                local_y - HANDSET_HANGER_HEAD_HALF_WIDTH + 0.05,
                0.1,
            ),
            (
                local_head_x1 - 0.05,
                local_y + HANDSET_HANGER_HEAD_HALF_WIDTH - 0.05,
                HANDSET_HANGER_RAIL_HEIGHT - 0.1,
            ),
        )
        neck_probe = box(
            (
                local_neck_x0 + 0.05,
                local_y - HANDSET_HANGER_NECK_HALF_WIDTH + 0.05,
                0.1,
            ),
            (
                local_neck_x1 - 0.05,
                local_y + HANDSET_HANGER_NECK_HALF_WIDTH - 0.05,
                HANDSET_HANGER_RAIL_HEIGHT - 0.1,
            ),
        )
        if intersection_volume(handset_mount, head_probe) > 0.01:
            raise ValueError(f"handset downward slot blocks rail head: {local_y}")
        if intersection_volume(handset_mount, neck_probe) > 0.01:
            raise ValueError(f"handset downward slot blocks rail neck: {local_y}")

        world_head_probe = head_probe.copy()
        world_head_probe.apply_translation(HANDSET_MOUNT_ORIGIN)
        world_neck_probe = neck_probe.copy()
        world_neck_probe.apply_translation(HANDSET_MOUNT_ORIGIN)
        if (
            intersection_volume(shell, world_head_probe)
            < world_head_probe.volume - 0.05
        ):
            raise ValueError(f"chassis upward rail head is incomplete: {world_y}")
        if (
            intersection_volume(shell, world_neck_probe)
            < world_neck_probe.volume - 0.05
        ):
            raise ValueError(f"chassis upward rail neck is incomplete: {world_y}")

        cap_probe = box(
            (
                HANDSET_HANGER_SLOT_HEAD_X0 - HANDSET_MOUNT_ORIGIN[0] + 0.2,
                local_y - 0.2,
                HANDSET_HANGER_SLOT_ROOF_TOP + 0.1,
            ),
            (
                HANDSET_HANGER_SLOT_HEAD_X1 - HANDSET_MOUNT_ORIGIN[0] - 0.2,
                local_y + 0.2,
                HANDSET_HANGER_HEIGHT - 0.1,
            ),
        )
        if intersection_volume(handset_mount, cap_probe) < cap_probe.volume - 0.02:
            raise ValueError(
                f"handset downward slot lacks a closed upper stop: {local_y}"
            )


def validate_models(
    shell: trimesh.Trimesh,
    panel: trimesh.Trimesh,
    cover: trimesh.Trimesh,
    handset_mount: trimesh.Trimesh,
) -> ValidationReport:
    macro.assert_closed_manifold(shell, "integrated workstation shell")
    macro.assert_closed_manifold(panel, "integrated workstation sloped panel")
    macro.assert_closed_manifold(cover, "integrated workstation bottom cover")
    macro.assert_closed_manifold(handset_mount, "workstation handset side mount")
    validate_switch_geometry(panel)
    validate_screen_header_access(panel)
    validate_screen_insert_holes(panel)
    validate_wire_clips(panel)
    validate_controller_connector_opening(shell)
    validate_controller_cradle(cover)
    validate_panel_attachment(shell, panel)
    validate_bottom_cover_attachment(shell, cover)
    validate_handset_mount_attachment(shell, handset_mount)

    normal = DECK_TRANSFORM[:3, :3] @ np.array([0.0, 0.0, 1.0])
    plane_angle = float(np.rad2deg(np.arccos(np.dot(normal, [0.0, 0.0, 1.0]))))
    if not np.isclose(plane_angle, KEY_ANGLE_DEGREES, atol=1e-9):
        raise ValueError(f"key plane angle drifted: {plane_angle}")
    if not np.allclose(
        [CONTROLLER_X1 - CONTROLLER_X0, CONTROLLER_Y1 - CONTROLLER_Y0],
        [CONTROLLER_CLEAR_WIDTH, CONTROLLER_CLEAR_LENGTH],
        atol=1e-9,
    ):
        raise ValueError("controller bay drifted")

    return ValidationReport(
        shell_extents=tuple(float(value) for value in shell.extents),
        panel_extents=tuple(float(value) for value in panel.extents),
        cover_extents=tuple(float(value) for value in cover.extents),
        handset_mount_extents=tuple(float(value) for value in handset_mount.extents),
        key_count=KEY_COLUMNS * KEY_ROWS,
        key_layout=(KEY_COLUMNS, KEY_ROWS),
        key_pitch=KEY_PITCH,
        key_plane_degrees=plane_angle,
        wire_clip_count=len(WIRE_CLIP_CENTERS),
        controller_bay=(CONTROLLER_CLEAR_WIDTH, CONTROLLER_CLEAR_LENGTH),
        controller_support_levels=(
            CONTROLLER_RP2040_RAISE,
            CONTROLLER_ESP32_S3_RAISE,
        ),
        screen_board=(SCREEN_BOARD_WIDTH, SCREEN_BOARD_HEIGHT),
        screen_plane_degrees=plane_angle,
        panel_screw_count=len(PANEL_SCREW_CENTERS),
        bottom_cover_screw_count=len(COVER_SCREW_CENTERS),
        handset_hanger_count=len(HANDSET_HANGER_LOCAL_Y_CENTERS),
        shell_watertight=bool(shell.is_watertight),
        panel_watertight=bool(panel.is_watertight),
        cover_watertight=bool(cover.is_watertight),
        handset_mount_watertight=bool(handset_mount.is_watertight),
    )


def fit_check_mesh(
    shell: trimesh.Trimesh,
    panel: trimesh.Trimesh,
    cover: trimesh.Trimesh,
    handset_mount: trimesh.Trimesh,
) -> trimesh.Trimesh:
    parts = [shell.copy(), place_sloped_panel(panel)]
    placed_cover = cover.copy()
    placed_cover.apply_translation([0.0, 0.0, -COVER_THICKNESS])
    parts.append(placed_cover)
    parts.append(place_handset_mount(handset_mount))
    return trimesh.util.concatenate(parts)


def exploded_fit_mesh(
    shell: trimesh.Trimesh,
    panel: trimesh.Trimesh,
    handset_mount: trimesh.Trimesh,
) -> trimesh.Trimesh:
    lifted_panel = place_sloped_panel(panel)
    panel_normal = DECK_TRANSFORM[:3, :3] @ np.array([0.0, 0.0, 1.0])
    lifted_panel.apply_translation(panel_normal * 24.0)
    separated_handset = place_handset_mount(handset_mount)
    separated_handset.apply_translation([-18.0, 0.0, 0.0])
    return trimesh.util.concatenate([shell.copy(), lifted_panel, separated_handset])


def render_previews(
    shell: trimesh.Trimesh,
    panel: trimesh.Trimesh,
    cover: trimesh.Trimesh,
    handset_mount: trimesh.Trimesh,
    preview_root: Path,
) -> None:
    handset.VIEW_ROTATIONS["front-isometric"] = FRONT_ISOMETRIC_ROTATION
    handset.VIEW_ROTATIONS["front"] = FRONT_ROTATION
    handset.render_preview(
        shell, preview_root / "shell-front-isometric.png", "front-isometric"
    )
    handset.render_preview(shell, preview_root / "shell-front.png", "front")
    handset.render_preview(shell, preview_root / "shell-isometric.png", "isometric")
    handset.render_preview(shell, preview_root / "shell-top.png", "top")
    handset.render_preview(shell, preview_root / "shell-bottom.png", "bottom")
    handset.render_preview(
        panel,
        preview_root / "sloped-panel-isometric.png",
        "isometric",
    )
    handset.render_preview(panel, preview_root / "sloped-panel-top.png", "top")
    handset.render_preview(cover, preview_root / "cover-isometric.png", "isometric")
    handset.render_preview(cover, preview_root / "cover-top.png", "top")
    handset.render_preview(
        handset_mount,
        preview_root / "handset-mount-isometric.png",
        "isometric",
    )
    handset.render_preview(
        fit_check_mesh(shell, panel, cover, handset_mount),
        preview_root / "fit-check-front-isometric.png",
        "front-isometric",
    )
    handset.render_preview(
        exploded_fit_mesh(shell, panel, handset_mount),
        preview_root / "exploded-front-isometric.png",
        "front-isometric",
    )


def export(mesh: trimesh.Trimesh, target: Path) -> None:
    macro.export_stl(mesh, target)


def main(argv: list[str] | None = None) -> int:
    import argparse
    import json

    parser = argparse.ArgumentParser()
    parser.add_argument("--output-root", type=Path, default=DEFAULT_OUTPUT_ROOT)
    parser.add_argument("--preview-root", type=Path, default=DEFAULT_PREVIEW_ROOT)
    arguments = parser.parse_args(argv)

    shell = generate_shell()
    panel = generate_sloped_panel()
    cover = generate_cover()
    handset_mount = generate_handset_mount()
    report = validate_models(shell, panel, cover, handset_mount)

    shell_target = arguments.output_root / SHELL_FILENAME
    panel_target = arguments.output_root / PANEL_FILENAME
    cover_target = arguments.output_root / COVER_FILENAME
    handset_mount_target = arguments.output_root / HANDSET_MOUNT_FILENAME
    export(shell, shell_target)
    export(panel, panel_target)
    export(cover, cover_target)
    export(handset_mount, handset_mount_target)
    render_previews(shell, panel, cover, handset_mount, arguments.preview_root)

    payload = asdict(report)
    payload["shell_path"] = str(shell_target)
    payload["panel_path"] = str(panel_target)
    payload["cover_path"] = str(cover_target)
    payload["handset_mount_path"] = str(handset_mount_target)
    payload["shell_sha256"] = hashlib.sha256(shell_target.read_bytes()).hexdigest()
    payload["panel_sha256"] = hashlib.sha256(panel_target.read_bytes()).hexdigest()
    payload["cover_sha256"] = hashlib.sha256(cover_target.read_bytes()).hexdigest()
    payload["handset_mount_sha256"] = hashlib.sha256(
        handset_mount_target.read_bytes()
    ).hexdigest()
    payload["preview_root"] = str(arguments.preview_root)
    print(json.dumps(payload, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
