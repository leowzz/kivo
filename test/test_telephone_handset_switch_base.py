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
