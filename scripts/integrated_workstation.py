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
    from scripts import telephone_handset_switch_base as handset
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
PANEL_Y1 = 121.0
PANEL_CLEARANCE = 0.3
PANEL_OPENING_REAR_OVERCUT = 2.0
PANEL_LIP_DEPTH = 2.4
PANEL_SCREW_CENTERS = np.array(
    [
        [79.0, 12.0],
        [203.0, 12.0],
        [79.0, 62.0],
        [203.0, 62.0],
        [79.0, 112.0],
        [203.0, 112.0],
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

TRAY_WIDTH = 76.0
TRAY_LENGTH = 92.0
TRAY_CENTER = (38.0, 50.0)
TRAY_BOTTOM_Y = 4.0
TRAY_HEIGHT = 12.0
TRAY_ROOF_UNDERSIDE = 9.2
HANDSET_WIDTH = 63.8
HANDSET_LENGTH = 78.8
HANDSET_CLEARANCE = 0.6
HANDSET_POCKET_WIDTH = HANDSET_WIDTH + 2.0 * HANDSET_CLEARANCE
HANDSET_POCKET_LENGTH = HANDSET_LENGTH + 2.0 * HANDSET_CLEARANCE
HANDSET_POCKET_FLOOR = 10.8
HANDSET_SOURCE_MODEL = Path(
    "models/3d-print/telephone-handset-switch-base/telephone_handset_switch_base.stl"
)
HANDSET_SCREW_LOCAL_CENTERS = np.array(
    [[8.0, 8.0], [55.8, 8.0], [8.0, 70.8], [55.8, 70.8]]
)
HANDSET_TRAY_HOLE_DIAMETER = 3.4

SCREEN_BOARD_WIDTH = 64.90
SCREEN_BOARD_HEIGHT = 35.03
SCREEN_BEZEL_WIDTH = 76.0
SCREEN_BEZEL_HEIGHT = 45.0
SCREEN_BEZEL_X0 = 142.5 - SCREEN_BEZEL_WIDTH / 2.0
SCREEN_BEZEL_Y0 = 74.0
SCREEN_BEZEL_RAISE = 2.0
SCREEN_RECESS_CLEARANCE = 0.65
SCREEN_BOARD_ORIGIN = np.array(
    [
        SCREEN_BEZEL_X0 + (SCREEN_BEZEL_WIDTH - SCREEN_BOARD_WIDTH) / 2.0,
        SCREEN_BEZEL_Y0 + (SCREEN_BEZEL_HEIGHT - SCREEN_BOARD_HEIGHT) / 2.0,
    ]
)
SCREEN_HOLE_DIAMETER = 3.4
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

SHELL_SCREW_CENTERS = np.array(
    [[77.0, 13.0], [205.0, 13.0], [77.0, 99.0], [205.0, 99.0]]
)
SHELL_BOSS_RADIUS = 5.0
SHELL_BOSS_HEIGHT = 12.0

COVER_WIDTH = WEDGE_X1 - WEDGE_X0
COVER_LENGTH = WEDGE_Y1 - WEDGE_Y0
COVER_CENTER = (141.0, 56.0)
COVER_THICKNESS = 2.4
COVER_HOLE_DIAMETER = 3.4
CONTROLLER_CLEAR_WIDTH = 29.0
CONTROLLER_CLEAR_LENGTH = 58.0
CONTROLLER_X0 = 127.0
CONTROLLER_X1 = CONTROLLER_X0 + CONTROLLER_CLEAR_WIDTH
CONTROLLER_Y0 = 12.0
CONTROLLER_Y1 = CONTROLLER_Y0 + CONTROLLER_CLEAR_LENGTH

DEFAULT_OUTPUT_ROOT = Path("models/3d-print/integrated-workstation")
DEFAULT_PREVIEW_ROOT = Path("/tmp/kivo-integrated-workstation-previews")
SHELL_FILENAME = "kivo_integrated_workstation_shell.stl"
PANEL_FILENAME = "kivo_integrated_workstation_sloped_panel.stl"
COVER_FILENAME = "kivo_integrated_workstation_bottom_cover.stl"
HANDSET_BASE_FILENAME = "telephone_handset_switch_base_workstation_mount.stl"

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
    key_count: int
    key_layout: tuple[int, int]
    key_pitch: float
    key_plane_degrees: float
    handset_pocket: tuple[float, float]
    handset_clearance_per_side: float
    handset_screw_count: int
    controller_bay: tuple[float, float]
    screen_board: tuple[float, float]
    screen_plane_degrees: float
    panel_screw_count: int
    shell_watertight: bool
    panel_watertight: bool
    cover_watertight: bool
    handset_base_watertight: bool


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


def handset_base_origin() -> np.ndarray:
    return np.array(
        [
            TRAY_CENTER[0] - HANDSET_WIDTH / 2.0,
            TRAY_CENTER[1] - HANDSET_LENGTH / 2.0,
        ]
    )


def handset_screw_world_centers() -> np.ndarray:
    return HANDSET_SCREW_LOCAL_CENTERS + handset_base_origin()


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


def build_handset_tray() -> trimesh.Trimesh:
    outer = rounded_prism(
        TRAY_WIDTH,
        TRAY_LENGTH,
        radius=7.0,
        z_min=0.0,
        height=TRAY_HEIGHT,
        center=TRAY_CENTER,
    )
    underside = rounded_prism(
        TRAY_WIDTH - 6.0,
        TRAY_LENGTH - 6.0,
        radius=5.0,
        z_min=-1.0,
        height=TRAY_ROOF_UNDERSIDE + 1.0,
        center=TRAY_CENTER,
    )
    pocket = rounded_prism(
        HANDSET_POCKET_WIDTH,
        HANDSET_POCKET_LENGTH,
        radius=6.6,
        z_min=HANDSET_POCKET_FLOOR,
        height=TRAY_HEIGHT - HANDSET_POCKET_FLOOR + 1.0,
        center=TRAY_CENTER,
    )
    access = rounded_prism(
        42.0,
        57.0,
        radius=2.2,
        z_min=TRAY_ROOF_UNDERSIDE - 0.5,
        height=TRAY_HEIGHT - TRAY_ROOF_UNDERSIDE + 1.5,
        center=TRAY_CENTER,
    )
    rear_cable_chute = box((34.0, 76.0, 8.5), (42.0, 97.0, 13.0))
    return subtract(outer, [underside, pocket, access, rear_cable_chute])


def load_handset_source() -> trimesh.Trimesh:
    base = trimesh.load_mesh(HANDSET_SOURCE_MODEL, file_type="stl", process=False)
    if not isinstance(base, trimesh.Trimesh):
        raise TypeError(f"expected one handset mesh in {HANDSET_SOURCE_MODEL}")
    base.merge_vertices()
    base.remove_unreferenced_vertices()
    macro.assert_closed_manifold(base, "source handset base")
    return base


def generate_handset_base() -> trimesh.Trimesh:
    insert_cutters = [
        cutter
        for center in HANDSET_SCREW_LOCAL_CENTERS
        for cutter in heat_set_insert_cutters(center, 0.0, 1)
    ]
    source = load_handset_source()
    for center in HANDSET_SCREW_LOCAL_CENTERS:
        keepout = cylinder(
            HEAT_SET_INSERT_LEAD_DIAMETER / 2.0 + 1.2,
            HEAT_SET_INSERT_HOLE_DEPTH,
            (center[0], center[1], HEAT_SET_INSERT_HOLE_DEPTH / 2.0),
        )
        source_material = intersection_volume(source, keepout)
        if source_material < 65.0:
            raise ValueError(
                f"handset insert hole lacks surrounding material: {center}, "
                f"volume={source_material}"
            )
    result = subtract(source, insert_cutters)
    result.merge_vertices()
    result.remove_unreferenced_vertices()
    return result


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
        cutters.append(
            cylinder(
                SCREEN_HOLE_DIAMETER / 2.0,
                KEY_PLATE_THICKNESS + SCREEN_BEZEL_RAISE + 2.0,
                (
                    hole[0],
                    hole[1],
                    (KEY_PLATE_THICKNESS + SCREEN_BEZEL_RAISE) / 2.0,
                ),
                axis=2,
            )
        )
    return cutters


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
        box((WEDGE_X0, 0.0, -PANEL_LIP_DEPTH), (82.0, 120.0, 0.0)),
        box((200.0, 0.0, -PANEL_LIP_DEPTH), (WEDGE_X1, 120.0, 0.0)),
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


def shell_cutters() -> list[trimesh.Trimesh]:
    cutters = [
        # Wide front opening accepts the RP2040 single USB-C connector or the
        # ESP32-S3 board's two adjacent USB-C connectors.
        box((123.0, 2.0, 2.0), (160.0, 9.0, 11.5)),
        # Cable path from the handset tray to the controller chamber.
        cylinder(3.0, 40.0, (72.0, 50.0, 5.0), axis=0),
    ]
    cutters.extend(panel_insert_cutters())
    cutters.extend(
        cylinder(
            HANDSET_TRAY_HOLE_DIAMETER / 2.0,
            5.0,
            (center[0], center[1], 10.5),
            axis=2,
        )
        for center in handset_screw_world_centers()
    )
    cutters.extend(
        countersink_cutter(center, TRAY_ROOF_UNDERSIDE, 1)
        for center in handset_screw_world_centers()
    )
    cutters.extend(
        cutter
        for center in SHELL_SCREW_CENTERS
        for cutter in heat_set_insert_cutters(center, 0.0, 1)
    )
    return cutters


def generate_shell() -> trimesh.Trimesh:
    open_wedge = subtract(build_wedge_shell(), [panel_opening_cutter()])
    combined = union(
        [
            open_wedge,
            build_handset_tray(),
            *build_panel_support_parts(),
            *build_shell_bosses(),
        ]
    )
    result = subtract(combined, shell_cutters())
    result.merge_vertices()
    result.remove_unreferenced_vertices()
    return result


def build_controller_mounts() -> list[trimesh.Trimesh]:
    rail_height = 3.0
    z1 = COVER_THICKNESS + rail_height
    overlap_z = COVER_THICKNESS - 0.02
    mounts = [
        box(
            (CONTROLLER_X0 - 2.0, CONTROLLER_Y0, overlap_z),
            (CONTROLLER_X0, CONTROLLER_Y1 + 2.0, z1),
        ),
        box(
            (CONTROLLER_X1, CONTROLLER_Y0, overlap_z),
            (CONTROLLER_X1 + 2.0, CONTROLLER_Y1 + 2.0, z1),
        ),
        box(
            (CONTROLLER_X0, CONTROLLER_Y1, overlap_z),
            (CONTROLLER_X1, CONTROLLER_Y1 + 2.0, z1),
        ),
    ]
    pad_height = COVER_THICKNESS + 2.6
    for x0 in (CONTROLLER_X0 + 0.5, CONTROLLER_X1 - 5.5):
        for y0 in (CONTROLLER_Y0 + 2.0, CONTROLLER_Y1 - 6.0):
            mounts.append(box((x0, y0, overlap_z), (x0 + 5.0, y0 + 5.0, pad_height)))
    return mounts


def cover_cutters() -> list[trimesh.Trimesh]:
    cutters = [
        box((122.0, 28.5, -1.0), (161.0, 31.5, COVER_THICKNESS + 1.0)),
        box((122.0, 51.5, -1.0), (161.0, 54.5, COVER_THICKNESS + 1.0)),
    ]
    cutters.extend(
        cylinder(
            COVER_HOLE_DIAMETER / 2.0,
            COVER_THICKNESS + 2.0,
            (center[0], center[1], COVER_THICKNESS / 2.0),
        )
        for center in SHELL_SCREW_CENTERS
    )
    cutters.extend(countersink_cutter(center, 0.0, 1) for center in SHELL_SCREW_CENTERS)
    for y0 in (26.0, 38.0, 50.0, 62.0, 74.0):
        cutters.append(box((92.0, y0, -1.0), (114.0, y0 + 2.4, COVER_THICKNESS + 1.0)))
    return cutters


def generate_cover() -> trimesh.Trimesh:
    plate = rounded_prism(
        COVER_WIDTH,
        COVER_LENGTH,
        radius=4.0,
        z_min=0.0,
        height=COVER_THICKNESS,
        center=COVER_CENTER,
    )
    combined = union([plate, *build_controller_mounts()])
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


def place_handset_base(base: trimesh.Trimesh) -> trimesh.Trimesh:
    base = base.copy()
    origin = handset_base_origin()
    base.apply_translation([origin[0], origin[1], HANDSET_POCKET_FLOOR + 0.02])
    return base


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


def validate_handset_fit(shell: trimesh.Trimesh, handset_base: trimesh.Trimesh) -> None:
    base = place_handset_base(handset_base)
    collision = macro.boolean_meshes([shell, base], "intersection")
    if not collision.is_empty and collision.volume > 0.03:
        raise ValueError(f"handset pocket collision: {collision.volume}")


def validate_handset_screw_holes(
    shell: trimesh.Trimesh, handset_base: trimesh.Trimesh
) -> None:
    for local_center, world_center in zip(
        HANDSET_SCREW_LOCAL_CENTERS,
        handset_screw_world_centers(),
        strict=True,
    ):
        tray_probe = cylinder(
            HANDSET_TRAY_HOLE_DIAMETER / 2.0 - 0.05,
            3.0,
            (world_center[0], world_center[1], 10.5),
        )
        if intersection_volume(shell, tray_probe) > 0.01:
            raise ValueError(f"handset tray screw hole is blocked: {world_center}")
        validate_countersink_opening(
            shell,
            world_center,
            TRAY_ROOF_UNDERSIDE,
            1,
            "handset tray",
        )
        base_probe = cylinder(
            HEAT_SET_INSERT_HOLE_DIAMETER / 2.0 - 0.05,
            HEAT_SET_INSERT_HOLE_DEPTH - 0.2,
            (
                local_center[0],
                local_center[1],
                HEAT_SET_INSERT_HOLE_DEPTH / 2.0,
            ),
        )
        if intersection_volume(handset_base, base_probe) > 0.01:
            raise ValueError(f"handset base insert hole is blocked: {local_center}")
        lead_probe = cylinder(
            HEAT_SET_INSERT_LEAD_DIAMETER / 2.0 - 0.05,
            HEAT_SET_INSERT_LEAD_DEPTH - 0.1,
            (
                local_center[0],
                local_center[1],
                HEAT_SET_INSERT_LEAD_DEPTH / 2.0,
            ),
        )
        if intersection_volume(handset_base, lead_probe) > 0.01:
            raise ValueError(f"handset base insert lead-in is blocked: {local_center}")
        floor_probe = cylinder(
            1.0,
            HEAT_SET_INSERT_BLIND_FLOOR / 2.0,
            (
                local_center[0],
                local_center[1],
                HEAT_SET_INSERT_HOLE_DEPTH + HEAT_SET_INSERT_BLIND_FLOOR / 4.0,
            ),
        )
        if intersection_volume(handset_base, floor_probe) < 0.5:
            raise ValueError(f"handset base insert hole is not blind: {local_center}")


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


def validate_bottom_cover_attachment(
    shell: trimesh.Trimesh, cover: trimesh.Trimesh
) -> None:
    for center in SHELL_SCREW_CENTERS:
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


def validate_models(
    shell: trimesh.Trimesh,
    panel: trimesh.Trimesh,
    cover: trimesh.Trimesh,
    handset_base: trimesh.Trimesh,
) -> ValidationReport:
    macro.assert_closed_manifold(shell, "integrated workstation shell")
    macro.assert_closed_manifold(panel, "integrated workstation sloped panel")
    macro.assert_closed_manifold(cover, "integrated workstation bottom cover")
    macro.assert_closed_manifold(handset_base, "workstation handset base")
    validate_switch_geometry(panel)
    validate_screen_header_access(panel)
    validate_panel_attachment(shell, panel)
    validate_handset_fit(shell, handset_base)
    validate_handset_screw_holes(shell, handset_base)
    validate_bottom_cover_attachment(shell, cover)

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
        key_count=KEY_COLUMNS * KEY_ROWS,
        key_layout=(KEY_COLUMNS, KEY_ROWS),
        key_pitch=KEY_PITCH,
        key_plane_degrees=plane_angle,
        handset_pocket=(HANDSET_POCKET_WIDTH, HANDSET_POCKET_LENGTH),
        handset_clearance_per_side=HANDSET_CLEARANCE,
        handset_screw_count=len(HANDSET_SCREW_LOCAL_CENTERS),
        controller_bay=(CONTROLLER_CLEAR_WIDTH, CONTROLLER_CLEAR_LENGTH),
        screen_board=(SCREEN_BOARD_WIDTH, SCREEN_BOARD_HEIGHT),
        screen_plane_degrees=plane_angle,
        panel_screw_count=len(PANEL_SCREW_CENTERS),
        shell_watertight=bool(shell.is_watertight),
        panel_watertight=bool(panel.is_watertight),
        cover_watertight=bool(cover.is_watertight),
        handset_base_watertight=bool(handset_base.is_watertight),
    )


def fit_check_mesh(
    shell: trimesh.Trimesh,
    panel: trimesh.Trimesh,
    cover: trimesh.Trimesh,
    handset_base: trimesh.Trimesh,
) -> trimesh.Trimesh:
    parts = [shell.copy(), place_sloped_panel(panel)]
    placed_cover = cover.copy()
    placed_cover.apply_translation([0.0, 0.0, -COVER_THICKNESS])
    parts.append(placed_cover)
    parts.append(place_handset_base(handset_base))
    return trimesh.util.concatenate(parts)


def exploded_fit_mesh(
    shell: trimesh.Trimesh,
    panel: trimesh.Trimesh,
    handset_base: trimesh.Trimesh,
) -> trimesh.Trimesh:
    lifted_panel = place_sloped_panel(panel)
    panel_normal = DECK_TRANSFORM[:3, :3] @ np.array([0.0, 0.0, 1.0])
    lifted_panel.apply_translation(panel_normal * 24.0)
    return trimesh.util.concatenate(
        [shell.copy(), lifted_panel, place_handset_base(handset_base)]
    )


def render_previews(
    shell: trimesh.Trimesh,
    panel: trimesh.Trimesh,
    cover: trimesh.Trimesh,
    handset_base: trimesh.Trimesh,
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
    handset.render_preview(
        handset_base,
        preview_root / "handset-base-bottom.png",
        "bottom",
    )
    handset.render_preview(
        fit_check_mesh(shell, panel, cover, handset_base),
        preview_root / "fit-check-front-isometric.png",
        "front-isometric",
    )
    handset.render_preview(
        exploded_fit_mesh(shell, panel, handset_base),
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
    handset_base = generate_handset_base()
    report = validate_models(shell, panel, cover, handset_base)

    shell_target = arguments.output_root / SHELL_FILENAME
    panel_target = arguments.output_root / PANEL_FILENAME
    cover_target = arguments.output_root / COVER_FILENAME
    handset_base_target = arguments.output_root / HANDSET_BASE_FILENAME
    export(shell, shell_target)
    export(panel, panel_target)
    export(cover, cover_target)
    export(handset_base, handset_base_target)
    render_previews(shell, panel, cover, handset_base, arguments.preview_root)

    payload = asdict(report)
    payload["shell_path"] = str(shell_target)
    payload["panel_path"] = str(panel_target)
    payload["cover_path"] = str(cover_target)
    payload["handset_base_path"] = str(handset_base_target)
    payload["shell_sha256"] = hashlib.sha256(shell_target.read_bytes()).hexdigest()
    payload["panel_sha256"] = hashlib.sha256(panel_target.read_bytes()).hexdigest()
    payload["cover_sha256"] = hashlib.sha256(cover_target.read_bytes()).hexdigest()
    payload["handset_base_sha256"] = hashlib.sha256(
        handset_base_target.read_bytes()
    ).hexdigest()
    payload["preview_root"] = str(arguments.preview_root)
    print(json.dumps(payload, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
