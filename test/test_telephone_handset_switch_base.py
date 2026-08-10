import hashlib
from pathlib import Path

import numpy as np
import pytest
import trimesh

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


def test_generate_base_preserves_exact_inner_chamfer_slope() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)

    for level, expected in (
        (27.8, (55.4, 70.4)),
        (28.2, (56.2, 71.2)),
    ):
        loops = section_loop_sizes(mesh, axis=2, level=level)
        assert any(np.allclose(size, expected, rtol=0.0, atol=0.003) for size in loops)


def test_generate_base_preserves_outer_pocket_and_switch_dimensions() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)

    macro.assert_closed_manifold(mesh, "telephone handset switch base")
    assert mesh.bounds[0] == pytest.approx((0.0, 0.0, 0.0), abs=0.003)
    assert mesh.extents == pytest.approx(
        (base.OUTER_WIDTH, base.OUTER_LENGTH, base.OUTER_HEIGHT), abs=0.003
    )

    loops = section_loop_sizes(mesh, axis=2, level=20.0)
    assert any(np.allclose(size, (59.8, 74.8), atol=0.003) for size in loops)
    assert any(np.allclose(size, (55.0, 70.0), atol=0.003) for size in loops)

    platform_loops = section_loop_sizes(mesh, axis=2, level=12.7)
    assert any(np.allclose(size, (24.0, 24.0), atol=0.003) for size in platform_loops)

    lower = macro.measure_switch_section(mesh, z=11.0, nominal_size=14.8)
    upper = macro.measure_switch_section(mesh, z=12.7, nominal_size=14.0)
    np.testing.assert_allclose(
        lower.centers, [[base.CENTER_X, base.CENTER_Y]], rtol=0.0, atol=0.003
    )
    np.testing.assert_allclose(
        upper.centers, [[base.CENTER_X, base.CENTER_Y]], rtol=0.0, atol=0.003
    )
    np.testing.assert_allclose(lower.sizes, [[14.798, 14.798]], rtol=0.0, atol=0.003)
    np.testing.assert_allclose(upper.sizes, [[14.0, 14.0]], rtol=0.0, atol=0.003)

    rear = section_loop_sizes(mesh, axis=1, level=73.6)
    assert any(np.allclose(size, (4.0, 4.0), atol=0.01) for size in rear)


def test_open_bottom_wire_path_and_required_supports() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)

    open_probes = (
        ([8.0, 25.0, 1.0], [15.0, 35.0, 9.0]),
        ([44.8, 25.0, 1.0], [51.8, 35.0, 9.0]),
        ([8.0, 48.0, 1.0], [15.0, 58.0, 9.0]),
        ([44.8, 48.0, 1.0], [51.8, 58.0, 9.0]),
        ([20.31, 28.0, 1.0], [39.49, 72.0, 9.0]),
        ([28.9, 49.4, 4.5], [30.9, 75.8, 5.5]),
        ([0.0, 0.0, 11.2], [0.5, 0.5, 13.2]),
        ([59.3, 0.0, 11.2], [59.8, 0.5, 13.2]),
        ([0.0, 74.3, 11.2], [0.5, 74.8, 13.2]),
        ([59.3, 74.3, 11.2], [59.8, 74.8, 13.2]),
    )
    for lower, upper in open_probes:
        assert base.region_volume(
            mesh, np.array(lower), np.array(upper)
        ) == pytest.approx(0.0, abs=1e-6)

    required_solids = (
        ([18.0, 30.0, 1.0], [20.2, 45.0, 9.0]),
        ([39.6, 30.0, 1.0], [41.8, 45.0, 9.0]),
        ([21.0, 25.5, 1.0], [38.8, 27.7, 9.0]),
        ([18.0, 52.0, 1.0], [20.2, 70.0, 9.0]),
        ([39.6, 52.0, 1.0], [41.8, 70.0, 9.0]),
        ([40.0, 36.0, 10.2], [41.5, 38.8, 13.2]),
        ([3.0, 3.0, 11.2], [12.0, 12.0, 13.2]),
        ([47.8, 3.0, 11.2], [56.8, 12.0, 13.2]),
        ([3.0, 62.8, 11.2], [12.0, 71.8, 13.2]),
        ([47.8, 62.8, 11.2], [56.8, 71.8, 13.2]),
        ([5.5, 5.5, 6.5], [7.5, 7.5, 7.5]),
        ([52.3, 5.5, 6.5], [54.3, 7.5, 7.5]),
        ([5.5, 67.3, 6.5], [7.5, 69.3, 7.5]),
        ([52.3, 67.3, 6.5], [54.3, 69.3, 7.5]),
    )
    for lower, upper in required_solids:
        assert base.region_volume(mesh, np.array(lower), np.array(upper)) > 0.5


def test_validate_base_reports_every_mesh_contract() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)

    report = base.validate_base(mesh, source)

    assert report.outer_extents == pytest.approx((59.8, 74.8, 28.4), abs=0.003)
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
        np.array([28.9, 27.7, 1.0]), np.array([30.9, 30.0, 9.0])
    )
    blocked_underside = macro.union_meshes([mesh, underside_block])
    with pytest.raises(ValueError, match="open underside"):
        base.validate_base(blocked_underside, source)

    wire_block = base.box_from_bounds(
        np.array([27.8, 72.0, 3.8]), np.array([32.0, 72.5, 6.2])
    )
    blocked_wire = macro.union_meshes([mesh, wire_block])
    with pytest.raises(ValueError, match="rear wire path"):
        base.validate_base(blocked_wire, source)


def test_validator_rejects_protected_switch_cell_drift() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)
    aperture_notch = base.box_from_bounds(
        np.array([base.CENTER_X + 6.8, base.CENTER_Y - 1.0, 12.4]),
        np.array([base.CENTER_X + 8.0, base.CENTER_Y + 1.0, 13.5]),
    )
    drifted = base.subtract_meshes(mesh, [aperture_notch])

    with pytest.raises(ValueError, match="source switch cell"):
        base.validate_base(drifted, source)


def test_validator_measures_datum_corners_and_every_required_feature() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)

    ledge = base.box_from_bounds(
        np.array([2.3, 30.0, 14.0]), np.array([10.0, 40.0, 14.5])
    )
    with pytest.raises(ValueError, match="pocket floor datum"):
        base.validate_base(macro.union_meshes([mesh, ledge]), source)

    square_corner = base.box_from_bounds(
        np.array([0.0, 0.0, 11.2]), np.array([3.0, 3.0, 13.2])
    )
    with pytest.raises(ValueError, match="R4 outer corner"):
        base.validate_base(macro.union_meshes([mesh, square_corner]), source)

    front_wall_cutter = base.box_from_bounds(
        np.array([20.2, 25.3, -0.1]), np.array([39.6, 27.85, 10.1])
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


def test_validator_rejects_non_r4_finished_ring_profile() -> None:
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
    r4_outer = base.rounded_prism(
        base.OUTER_WIDTH,
        base.OUTER_LENGTH,
        base.OUTER_RADIUS,
        z_min=-0.1,
        height=base.OUTER_HEIGHT + 0.2,
        center=(base.CENTER_X, base.CENTER_Y),
    )
    corner_fill = base.subtract_meshes(r3_outer, [r4_outer])
    r3_profile = macro.union_meshes([mesh, corner_fill])
    for legacy_probe in base.OUTER_CORNER_PROBES:
        assert base.probe_volume(r3_profile, legacy_probe) == pytest.approx(
            0.0, abs=1e-6
        )

    with pytest.raises(ValueError, match="R4 outer corner"):
        base.validate_base(r3_profile, source)


def test_validator_rejects_19_191_mm_switch_channel() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)
    channel_intrusion = base.box_from_bounds(
        np.array([20.29, 28.0, 1.0]), np.array([20.309, 72.0, 9.0])
    )
    narrowed_channel = macro.union_meshes([mesh, channel_intrusion])

    with pytest.raises(ValueError, match="19.2 channel"):
        base.validate_base(narrowed_channel, source)


def test_validator_rejects_partial_required_solid() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)
    partial_front_wall_cutter = base.box_from_bounds(
        np.array([21.1, 25.3, -0.1]), np.array([39.6, 27.85, 10.1])
    )
    front_wall_stub = base.subtract_meshes(mesh, [partial_front_wall_cutter])
    assert base.probe_volume(front_wall_stub, base.REQUIRED_SOLID_PROBES[2]) > 0.5

    with pytest.raises(
        ValueError, match="required platform, tower, rib, pad, or gusset"
    ):
        base.validate_base(front_wall_stub, source)


def test_validation_report_uses_measured_pocket_loop() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)
    sub_tolerance_inner_strip = base.box_from_bounds(
        np.array([2.399, 2.4, 19.5]), np.array([2.4025, 72.4, 20.5])
    )
    measured_variant = macro.union_meshes([mesh, sub_tolerance_inner_strip])

    report = base.validate_base(measured_variant, source)

    assert report.pocket_bounds == pytest.approx((54.9975, 70.0), abs=0.0002)


def test_validator_rejects_r3_9_finished_ring_profile() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)
    r3_9_outer = base.rounded_prism(
        base.OUTER_WIDTH,
        base.OUTER_LENGTH,
        3.9,
        z_min=0.0,
        height=base.OUTER_HEIGHT,
        center=(base.CENTER_X, base.CENTER_Y),
    )
    r4_outer = base.rounded_prism(
        base.OUTER_WIDTH,
        base.OUTER_LENGTH,
        base.OUTER_RADIUS,
        z_min=-0.1,
        height=base.OUTER_HEIGHT + 0.2,
        center=(base.CENTER_X, base.CENTER_Y),
    )
    corner_fill = base.subtract_meshes(r3_9_outer, [r4_outer])
    r3_9_profile = macro.union_meshes([mesh, corner_fill])
    for legacy_probe in base.OUTER_CORNER_PROBES:
        assert base.probe_volume(r3_9_profile, legacy_probe) == pytest.approx(
            0.0, abs=1e-6
        )

    with pytest.raises(ValueError, match="R4 outer corner"):
        base.validate_base(r3_9_profile, source)


def test_validator_rejects_missing_front_wall_side_strips() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)
    left_strip_cutter = base.box_from_bounds(
        np.array([20.29, 25.3, -0.1]), np.array([21.0, 27.9, 10.1])
    )
    right_strip_cutter = base.box_from_bounds(
        np.array([38.8, 25.3, -0.1]), np.array([39.51, 27.9, 10.1])
    )
    stripped_front_wall = base.subtract_meshes(
        mesh, [left_strip_cutter, right_strip_cutter]
    )
    front_probe = base.REQUIRED_SOLID_PROBES[2]
    expected_probe_volume = float(np.prod(np.subtract(front_probe[1], front_probe[0])))
    assert base.probe_volume(stripped_front_wall, front_probe) == pytest.approx(
        expected_probe_volume, abs=0.01
    )

    with pytest.raises(
        ValueError, match="required platform, tower, rib, pad, or gusset"
    ):
        base.validate_base(stripped_front_wall, source)


def test_validator_rejects_square_rear_wire_hole() -> None:
    source = base.load_canonical_source(SOURCE_ROOT)
    mesh = base.generate_base(source)
    hole_fill = base.box_from_bounds(
        np.array([27.7, 72.3, 2.8]), np.array([32.1, 74.8, 7.2])
    )
    filled_hole = macro.union_meshes([mesh, hole_fill])
    square_cutter = base.box_from_bounds(
        np.array([27.9, 72.2, 3.0]), np.array([31.9, 74.9, 7.0])
    )
    square_hole = base.subtract_meshes(filled_hole, [square_cutter])
    rear_loops = section_loop_sizes(square_hole, axis=1, level=73.6)
    assert any(
        np.allclose(size, (4.0, 4.0), rtol=0.0, atol=0.003) for size in rear_loops
    )

    with pytest.raises(ValueError, match="rear wire hole"):
        base.validate_base(square_hole, source)
