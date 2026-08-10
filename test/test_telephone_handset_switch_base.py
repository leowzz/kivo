import hashlib
from pathlib import Path

import numpy as np
import pytest

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
    np.testing.assert_allclose(lower.sizes, [[14.798, 14.798]], atol=0.003)
    np.testing.assert_allclose(upper.sizes, [[14.0, 14.0]], atol=0.003)


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
