import hashlib
import json
import warnings
from pathlib import Path

import manifold3d
import numpy as np
import pytest
import trimesh
from PIL import Image

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
    np.testing.assert_allclose(lower.sizes, [[14.798, 14.798]], rtol=0.0, atol=0.003)
    np.testing.assert_allclose(upper.sizes, [[14.0, 14.0]], rtol=0.0, atol=0.003)


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

    np.testing.assert_allclose(
        outer.bounds, [[0.0, 0.0, 1.0], [20.0, 30.0, 6.0]], atol=1e-7
    )
    assert result.is_watertight
    assert result.is_winding_consistent
    assert result.volume < outer.volume
    assert base.region_volume(
        result, np.array([8.5, 13.5, 1.5]), np.array([11.5, 16.5, 5.5])
    ) == pytest.approx(0.0, abs=1e-6)


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


def contour_with_points(
    contours: list[np.ndarray], *expected_points: tuple[float, float]
) -> np.ndarray:
    for contour in contours:
        if all(
            np.any(np.all(np.isclose(contour, point, rtol=0.0, atol=0.003), axis=1))
            for point in expected_points
        ):
            return contour
    raise AssertionError(f"section is missing points: {expected_points}")


def contour_point(contour: np.ndarray, expected: tuple[float, float]) -> np.ndarray:
    matches = np.all(np.isclose(contour, expected, rtol=0.0, atol=0.003), axis=1)
    assert np.any(matches)
    return contour[np.flatnonzero(matches)[0]]


def tangent_endpoint_section(
    width: float,
    length: float,
    radius: float,
    center: tuple[float, float],
) -> manifold3d.CrossSection:
    x0 = center[0] - width / 2.0
    x1 = center[0] + width / 2.0
    y0 = center[1] - length / 2.0
    y1 = center[1] + length / 2.0
    points = [
        (x0 + radius, y0),
        (x1 - radius, y0),
        (x1, y0 + radius),
        (x1, y1 - radius),
        (x1 - radius, y1),
        (x0 + radius, y1),
        (x0, y1 - radius),
        (x0, y0 + radius),
    ]
    return manifold3d.CrossSection([points])


def build_profile_variant(
    source: trimesh.Trimesh, *, chord_outer: bool, chord_inner: bool
) -> trimesh.Trimesh:
    mesh = base.generate_base(source)
    if chord_outer:
        outer_limit = base.manifold_to_mesh(
            tangent_endpoint_section(
                base.OUTER_WIDTH,
                base.OUTER_LENGTH,
                base.OUTER_RADIUS,
                (base.CENTER_X, base.CENTER_Y),
            )
            .extrude(base.OUTER_HEIGHT + 0.2)
            .translate((0.0, 0.0, -0.1))
        )
        return macro.boolean_meshes([mesh, outer_limit], "intersection")

    if chord_inner:
        outer = base.rounded_rectangle_section(
            base.OUTER_WIDTH,
            base.OUTER_LENGTH,
            base.OUTER_RADIUS,
            (base.CENTER_X, base.CENTER_Y),
        )
        inner = tangent_endpoint_section(
            base.INNER_WIDTH,
            base.INNER_LENGTH,
            base.INNER_RADIUS,
            (base.CENTER_X, base.CENTER_Y),
        )
        ring = base.manifold_to_mesh((outer - inner).extrude(base.OUTER_HEIGHT))
        parts = [
            ring,
            base.build_switch_platform(source),
            base.build_tower_and_ribs(),
        ]
        parts.extend(
            base.build_safety_pad(x_side, y_side)
            for x_side in (-1, 1)
            for y_side in (-1, 1)
        )
        joined = macro.union_meshes(parts)
        result = base.subtract_meshes(joined, [base.rear_hole_cutter()])
        result.remove_unreferenced_vertices()
        return result

    raise ValueError("one profile must be chorded")


def test_generate_base_uses_exact_funnel_and_locating_sections() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)

    expected_sections = (
        (13.401, (55.0, 70.0)),
        (20.0, (55.0, 70.0)),
        (24.399, (55.0, 70.0)),
        (25.4, (56.0, 71.0)),
        (26.4, (57.0, 72.0)),
        (27.4, (58.0, 73.0)),
        (28.399, (58.999, 73.999)),
    )
    for level, expected in expected_sections:
        loops = section_loop_sizes(mesh, axis=2, level=level)
        assert any(np.allclose(size, expected, rtol=0.0, atol=0.003) for size in loops)


def test_generate_base_preserves_outer_pocket_and_switch_dimensions() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)

    macro.assert_closed_manifold(mesh, "telephone handset switch base")
    assert mesh.bounds[0] == pytest.approx((0.0, 0.0, 0.0), abs=0.003)
    assert mesh.extents == pytest.approx((63.8, 78.8, 28.4), abs=0.003)

    loops = section_loop_sizes(mesh, axis=2, level=20.0)
    assert any(np.allclose(size, (63.8, 78.8), atol=0.003) for size in loops)
    assert any(np.allclose(size, (55.0, 70.0), atol=0.003) for size in loops)
    base.require_rounded_rectangle_loop(
        base.measured_section_loops(mesh, axis=2, level=20.0),
        np.array([[4.4, 4.4], [59.4, 74.4]]),
        1.6,
        "R1.6 lower locating profile",
        0.003,
    )
    base.require_rounded_rectangle_loop(
        base.measured_section_loops(mesh, axis=2, level=28.399),
        np.array([[2.4005, 2.4005], [61.3995, 76.3995]]),
        3.5995,
        "R3.6 top funnel profile",
        0.003,
    )

    platform_loops = section_loop_sizes(mesh, axis=2, level=9.7)
    assert any(np.allclose(size, (24.0, 24.0), atol=0.003) for size in platform_loops)

    lower = macro.measure_switch_section(mesh, z=8.0, nominal_size=14.8)
    upper = macro.measure_switch_section(mesh, z=9.7, nominal_size=14.0)
    np.testing.assert_allclose(
        lower.centers, [[base.CENTER_X, base.CENTER_Y]], rtol=0.0, atol=0.003
    )
    np.testing.assert_allclose(
        upper.centers, [[base.CENTER_X, base.CENTER_Y]], rtol=0.0, atol=0.003
    )
    np.testing.assert_allclose(lower.sizes, [[14.798, 14.798]], rtol=0.0, atol=0.003)
    np.testing.assert_allclose(upper.sizes, [[14.0, 14.0]], rtol=0.0, atol=0.003)

    rear = section_loop_sizes(mesh, axis=1, level=76.6)
    assert any(np.allclose(size, (4.0, 4.0), atol=0.01) for size in rear)


def test_bottomed_switch_and_four_pads_share_the_support_datum() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)

    cell = base.place_source_cell(source)
    np.testing.assert_allclose(cell.bounds[:, 2], [7.0, 10.4], rtol=0.0, atol=0.003)
    assert base.PLATFORM_TOP + 5.0 - 2.0 == pytest.approx(base.PAD_TOP)
    assert base.PLATFORM_TOP == pytest.approx(10.4)
    assert base.PAD_TOP == pytest.approx(13.4)

    report = base.validate_base(mesh, source)
    assert report.pocket_depth == pytest.approx(15.0, abs=0.003)
    assert report.open_underside
    assert report.rear_wire_path


def test_finished_mesh_locks_literal_safety_pad_geometry() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)

    top = mesh.section(plane_origin=[0.0, 0.0, 12.0], plane_normal=[0.0, 0.0, 1.0])
    assert top is not None
    top_contours = [
        np.asarray(entity.discrete(top.vertices))[:, :2]
        for entity in top.entities
        if entity.closed
    ]
    pad_contour = contour_with_points(
        top_contours, (14.4, 14.4), (14.4, 4.4), (4.4, 14.4)
    )
    pad_corner = contour_point(pad_contour, (14.4, 14.4))
    np.testing.assert_allclose(
        pad_corner - contour_point(pad_contour, (14.4, 4.4)),
        [0.0, 10.0],
        rtol=0.0,
        atol=0.003,
    )
    np.testing.assert_allclose(
        pad_corner - contour_point(pad_contour, (4.4, 14.4)),
        [10.0, 0.0],
        rtol=0.0,
        atol=0.003,
    )

    outer_face = base.vertical_section_contours(
        mesh, np.array([0.0, 10.0, 0.0]), np.array([1.0, 0.0, 0.0])
    )
    face_contour = contour_with_points(outer_face, (14.4, 11.0), (14.4, 13.4))
    np.testing.assert_allclose(
        contour_point(face_contour, (14.4, 13.4))
        - contour_point(face_contour, (14.4, 11.0)),
        [0.0, 2.4],
        rtol=0.0,
        atol=0.003,
    )

    gusset = base.vertical_section_contours(
        mesh, np.array([0.0, 5.6, 0.0]), np.array([1.0, 0.0, 0.0])
    )
    gusset_contour = contour_with_points(gusset, (6.8, 3.4), (14.4, 11.0))
    gusset_run = contour_point(gusset_contour, (14.4, 11.0)) - contour_point(
        gusset_contour, (6.8, 3.4)
    )
    np.testing.assert_allclose(gusset_run, [7.6, 7.6], rtol=0.0, atol=0.003)
    assert gusset_run[1] / gusset_run[0] == pytest.approx(1.0, abs=0.0004)


def test_open_bottom_wire_path_and_required_supports() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)

    open_probes = (
        *base.OPEN_UNDERSIDE_PROBES,
        base.SWITCH_CHANNEL_PROBE,
        base.REAR_WIRE_PROBE,
        *base.OUTER_CORNER_PROBES,
    )
    for lower, upper in open_probes:
        assert base.region_volume(
            mesh, np.array(lower), np.array(upper)
        ) == pytest.approx(0.0, abs=1e-6)

    for _, feature in base.required_feature_references():
        assert base.intersection_volume([mesh, feature]) >= feature.volume - 0.03


def test_validate_base_reports_every_mesh_contract() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)

    report = base.validate_base(mesh, source)

    assert report.outer_extents == pytest.approx((63.8, 78.8, 28.4), abs=0.003)
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
        np.array([base.CENTER_X - 1.0, 29.7, 1.0]),
        np.array([base.CENTER_X + 1.0, 31.7, base.PLATFORM_BOTTOM - 0.1]),
    )
    blocked_underside = macro.union_meshes([mesh, underside_block])
    with pytest.raises(ValueError, match="open underside"):
        base.validate_base(blocked_underside, source)

    wire_block = base.box_from_bounds(
        np.array([base.CENTER_X - 2.1, 75.0, 3.8]),
        np.array([base.CENTER_X + 2.1, 75.5, 6.2]),
    )
    blocked_wire = macro.union_meshes([mesh, wire_block])
    with pytest.raises(ValueError, match="rear wire path"):
        base.validate_base(blocked_wire, source)


def test_validator_rejects_protected_switch_cell_drift() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)
    aperture_notch = base.box_from_bounds(
        np.array([base.CENTER_X + 6.8, base.CENTER_Y - 1.0, 9.4]),
        np.array([base.CENTER_X + 8.0, base.CENTER_Y + 1.0, 10.5]),
    )
    drifted = base.subtract_meshes(mesh, [aperture_notch])

    with pytest.raises(ValueError, match="source switch cell"):
        base.validate_base(drifted, source)


def test_validator_measures_datum_corners_and_every_required_feature() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)

    ledge = base.box_from_bounds(
        np.array([base.LOWER_INSET - 0.1, 30.0, 14.0]),
        np.array([10.0, 40.0, 14.5]),
    )
    with pytest.raises(ValueError, match="pocket floor datum"):
        base.validate_base(macro.union_meshes([mesh, ledge]), source)

    square_corner = base.box_from_bounds(
        np.array([0.0, 0.0, 11.2]), np.array([3.0, 3.0, 13.2])
    )
    with pytest.raises(ValueError, match="R6 outer corner"):
        base.validate_base(macro.union_meshes([mesh, square_corner]), source)

    front_wall_cutter = base.box_from_bounds(
        np.array([22.4, 27.3, -0.1]), np.array([41.4, 29.85, 7.1])
    )
    missing_front_wall = base.subtract_meshes(mesh, [front_wall_cutter])
    with pytest.raises(
        ValueError, match="required platform, tower, rib, pad, or gusset"
    ):
        base.validate_base(missing_front_wall, source)


def test_validator_rejects_bottom_plate_below_existing_probes() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)
    bottom_plate = base.box_from_bounds(
        np.array([base.WALL, base.WALL, 0.0]),
        np.array([base.OUTER_WIDTH - base.WALL, base.OUTER_LENGTH - base.WALL, 0.5]),
    )
    blocked_bottom = macro.union_meshes([mesh, bottom_plate])

    with pytest.raises(ValueError, match="open underside"):
        base.validate_base(blocked_bottom, source)


def test_validator_rejects_non_r6_finished_ring_profile() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)
    r3_outer = base.rounded_prism(
        base.OUTER_WIDTH,
        base.OUTER_LENGTH,
        3.0,
        z_min=0.0,
        height=base.OUTER_HEIGHT,
        center=(base.CENTER_X, base.CENTER_Y),
    )
    r6_outer = base.rounded_prism(
        base.OUTER_WIDTH,
        base.OUTER_LENGTH,
        base.OUTER_RADIUS,
        z_min=-0.1,
        height=base.OUTER_HEIGHT + 0.2,
        center=(base.CENTER_X, base.CENTER_Y),
    )
    corner_fill = base.subtract_meshes(r3_outer, [r6_outer])
    r3_profile = macro.union_meshes([mesh, corner_fill])
    for legacy_probe in base.OUTER_CORNER_PROBES:
        assert base.probe_volume(r3_profile, legacy_probe) == pytest.approx(
            0.0, abs=1e-6
        )

    with pytest.raises(ValueError, match="R6 outer corner"):
        base.validate_base(r3_profile, source)


def test_validator_rejects_19_191_mm_switch_channel() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)
    channel_intrusion = base.box_from_bounds(
        np.array([22.29, 29.81, 1.0]), np.array([22.309, 74.39, 6.9])
    )
    narrowed_channel = macro.union_meshes([mesh, channel_intrusion])

    with pytest.raises(ValueError, match="19.2 channel"):
        base.validate_base(narrowed_channel, source)


def test_validator_rejects_partial_required_solid() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)
    partial_front_wall_cutter = base.box_from_bounds(
        np.array([23.3, 27.3, -0.1]), np.array([41.4, 29.85, 7.1])
    )
    front_wall_stub = base.subtract_meshes(mesh, [partial_front_wall_cutter])
    assert front_wall_stub.volume < mesh.volume

    with pytest.raises(
        ValueError, match="required platform, tower, rib, pad, or gusset"
    ):
        base.validate_base(front_wall_stub, source)


def test_validation_report_uses_measured_pocket_loop() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)
    sub_tolerance_inner_strip = base.box_from_bounds(
        np.array([4.399, 4.4, 19.98]), np.array([4.4025, 74.4, 20.02])
    )
    measured_variant = macro.union_meshes([mesh, sub_tolerance_inner_strip])

    report = base.validate_base(measured_variant, source)

    assert report.pocket_bounds == pytest.approx((54.9975, 70.0), abs=0.0002)


def test_validator_rejects_r5_9_finished_ring_profile() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)
    r5_9_outer = base.rounded_prism(
        base.OUTER_WIDTH,
        base.OUTER_LENGTH,
        5.9,
        z_min=0.0,
        height=base.OUTER_HEIGHT,
        center=(base.CENTER_X, base.CENTER_Y),
    )
    r6_outer = base.rounded_prism(
        base.OUTER_WIDTH,
        base.OUTER_LENGTH,
        base.OUTER_RADIUS,
        z_min=-0.1,
        height=base.OUTER_HEIGHT + 0.2,
        center=(base.CENTER_X, base.CENTER_Y),
    )
    corner_fill = base.subtract_meshes(r5_9_outer, [r6_outer])
    r5_9_profile = macro.union_meshes([mesh, corner_fill])
    for legacy_probe in base.OUTER_CORNER_PROBES:
        assert base.probe_volume(r5_9_profile, legacy_probe) == pytest.approx(
            0.0, abs=1e-6
        )

    with pytest.raises(ValueError, match="R6 outer corner"):
        base.validate_base(r5_9_profile, source)


def test_validator_rejects_missing_front_wall_side_strips() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)
    left_strip_cutter = base.box_from_bounds(
        np.array([22.29, 27.3, -0.1]), np.array([23.0, 29.85, 7.1])
    )
    right_strip_cutter = base.box_from_bounds(
        np.array([40.8, 27.3, -0.1]), np.array([41.51, 29.85, 7.1])
    )
    stripped_front_wall = base.subtract_meshes(
        mesh, [left_strip_cutter, right_strip_cutter]
    )
    assert stripped_front_wall.volume < mesh.volume

    with pytest.raises(
        ValueError, match="required platform, tower, rib, pad, or gusset"
    ):
        base.validate_base(stripped_front_wall, source)


def test_validator_rejects_square_rear_wire_hole() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)
    hole_fill = base.box_from_bounds(
        np.array([29.7, 74.3, 2.8]), np.array([34.1, 78.8, 7.2])
    )
    filled_hole = macro.union_meshes([mesh, hole_fill])
    square_cutter = base.box_from_bounds(
        np.array([29.9, 74.2, 3.0]), np.array([33.9, 78.9, 7.0])
    )
    square_hole = base.subtract_meshes(filled_hole, [square_cutter])
    rear_loops = section_loop_sizes(square_hole, axis=1, level=76.6)
    assert any(
        np.allclose(size, (4.0, 4.0), rtol=0.0, atol=0.003) for size in rear_loops
    )

    with pytest.raises(ValueError, match="rear wire hole"):
        base.validate_base(square_hole, source)


@pytest.mark.parametrize(
    ("chord_outer", "chord_inner", "error_pattern"),
    (
        (True, False, "R6 outer"),
        (False, True, "R1.6 inner"),
    ),
)
def test_validator_rejects_tangent_endpoint_chord_profiles(
    chord_outer: bool, chord_inner: bool, error_pattern: str
) -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    chorded = build_profile_variant(
        source, chord_outer=chord_outer, chord_inner=chord_inner
    )

    with pytest.raises(ValueError, match=error_pattern):
        base.validate_base(chorded, source)


def test_validator_rejects_inscribed_diamond_rear_wire_hole() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)
    hole_fill = base.box_from_bounds(
        np.array([29.7, 74.3, 2.8]), np.array([34.1, 78.8, 7.2])
    )
    filled_hole = macro.union_meshes([mesh, hole_fill])
    diamond_cutter = trimesh.creation.cylinder(
        radius=base.WIRE_HOLE_DIAMETER / 2.0,
        height=base.REAR_WALL_THICKNESS + 2.0,
        sections=4,
    )
    diamond_cutter.apply_transform(
        trimesh.transformations.rotation_matrix(np.pi / 2.0, [1.0, 0.0, 0.0])
    )
    diamond_cutter.apply_translation(
        [base.CENTER_X, base.OUTER_LENGTH - base.REAR_WALL_THICKNESS / 2.0, 5.0]
    )
    diamond_hole = base.subtract_meshes(filled_hole, [diamond_cutter])

    with pytest.raises(ValueError, match="rear wire hole profile"):
        base.validate_base(diamond_hole, source)


def test_validator_rejects_partial_lower_floor_outside_legacy_probes() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)
    partial_floor = base.box_from_bounds(
        np.array([base.LOWER_INSET, base.LOWER_INSET, 0.0]),
        np.array([base.OUTER_WIDTH - base.WALL, 24.9, 0.5]),
    )
    obstructed = macro.union_meshes([mesh, partial_floor])

    with pytest.raises(ValueError, match="unexpected lower material"):
        base.validate_base(obstructed, source)


def test_validator_rejects_broad_roof_above_lower_access_region() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)
    roof = base.box_from_bounds(
        np.array([base.LOWER_INSET, base.LOWER_INSET, 7.1]),
        np.array([base.OUTER_WIDTH - base.LOWER_INSET, 26.9, 7.6]),
    )
    obstructed = macro.union_meshes([mesh, roof])

    with pytest.raises(ValueError, match="unexpected .* material"):
        base.validate_base(obstructed, source)


def test_validator_rejects_partial_outer_front_wall_notch() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)
    notch = base.box_from_bounds(
        np.array([20.0, -0.1, 1.0]), np.array([30.0, 2.5, 9.0])
    )
    missing_front_wall = base.subtract_meshes(mesh, [notch])

    with pytest.raises(ValueError, match="outer wall is missing"):
        base.validate_base(missing_front_wall, source)


@pytest.mark.parametrize("rear_y", (75.0, 76.0, 78.2))
def test_validator_rejects_internal_rear_hole_annulus(rear_y: float) -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)
    outer = trimesh.creation.cylinder(
        radius=base.WIRE_HOLE_DIAMETER / 2.0,
        height=1.2,
        sections=base.WIRE_HOLE_SEGMENTS,
    )
    inner = trimesh.creation.cylinder(
        radius=1.2,
        height=1.4,
        sections=base.WIRE_HOLE_SEGMENTS,
    )
    rotation = trimesh.transformations.rotation_matrix(np.pi / 2.0, [1.0, 0.0, 0.0])
    for cylinder in (outer, inner):
        cylinder.apply_transform(rotation)
        cylinder.apply_translation([base.CENTER_X, rear_y, 5.0])
    annulus = base.subtract_meshes(outer, [inner])
    restricted_hole = macro.union_meshes([mesh, annulus])

    with pytest.raises(ValueError, match=r"rear wire (path|hole clearance)"):
        base.validate_base(restricted_hole, source)


def test_export_is_deterministic_and_reload_validates(tmp_path: Path) -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    first = tmp_path / "first.stl"
    second = tmp_path / "second.stl"

    base.export_base(base.generate_base(source), first)
    base.export_base(base.generate_base(source), second)

    assert first.read_bytes() == second.read_bytes()
    reloaded = trimesh.load_mesh(first, file_type="stl", process=False)
    assert isinstance(reloaded, trimesh.Trimesh)
    with warnings.catch_warnings():
        warnings.simplefilter("error", RuntimeWarning)
        base.validate_base(reloaded, source)


def test_side_section_contours_expose_platform_and_pad_datums() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)

    sections = base.side_section_contours(mesh)

    assert set(sections) == {"centerline", "diagonal"}
    for contours, datum in (
        (sections["centerline"], base.PLATFORM_TOP),
        (sections["diagonal"], base.PAD_TOP),
    ):
        assert any(
            np.any(
                np.isclose(contour[:-1, 1], datum, atol=0.003)
                & np.isclose(contour[1:, 1], datum, atol=0.003)
            )
            for contour in contours
        )


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
            assert image.size == (1200, 900)
            pixels = np.asarray(image.convert("RGB"))
        ink = np.any(pixels != 255, axis=2)
        nonblank = np.count_nonzero(ink)
        assert nonblank >= pixels.shape[0] * pixels.shape[1] * 0.05
        if path.name == "side-section.png":
            rows, columns = np.nonzero(ink)
            framed = ink[
                rows.min() : rows.max() + 1,
                columns.min() : columns.max() + 1,
            ]
            assert np.count_nonzero(~framed) >= framed.size * 0.15
