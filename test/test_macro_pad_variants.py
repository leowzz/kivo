from pathlib import Path

import numpy as np
import pytest

from scripts import macro_pad_variants as variants


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "models/3d-print/3x3keypad"


def test_layout_contracts() -> None:
    assert variants.LAYOUTS["3x4"].footprint == pytest.approx((65.15, 84.20))
    assert variants.LAYOUTS["4x4"].growth == pytest.approx((9.525, 9.525, 19.05))
    assert variants.LAYOUTS["5x4"].footprint == pytest.approx((103.25, 84.20))


@pytest.mark.parametrize(
    ("filename", "faces", "extents"),
    [
        ("pico_macro_pad_top.stl.stl", 3398, (65.15, 65.15, 9.998)),
        (
            "pico_macro_pad_bottom_fitted_to_usb_c.stl.stl",
            3838,
            (65.148, 65.15, 15.006),
        ),
    ],
)
def test_source_mesh_contract(filename: str, faces: int, extents: tuple[float, ...]) -> None:
    mesh = variants.load_source(SOURCE / filename)
    assert len(mesh.faces) == faces
    assert mesh.extents == pytest.approx(extents, abs=0.003)
    assert np.allclose(mesh.bounds[0], 0.0, atol=1e-6)
    assert mesh.is_watertight
    assert mesh.is_winding_consistent
    assert mesh.body_count == 1


@pytest.mark.parametrize("name", ["3x4", "4x4", "5x4"])
def test_generate_top_preserves_pitch_holes_and_topology(name: str) -> None:
    source = variants.load_source(SOURCE / "pico_macro_pad_top.stl.stl")
    layout = variants.LAYOUTS[name]
    mesh = variants.generate_top(source, layout)

    assert mesh.extents[:2] == pytest.approx(layout.footprint, abs=0.003)
    assert mesh.extents[2] == pytest.approx(9.998, abs=0.001)
    assert mesh.is_watertight
    assert mesh.is_winding_consistent
    assert mesh.body_count == 1
    assert mesh.euler_number == 2 - 2 * layout.columns * layout.rows

    centers = variants.expected_switch_centers(layout)
    openings = variants.switch_section_sizes(mesh, centers, z=2.7)
    reliefs = variants.switch_section_sizes(mesh, centers, z=1.0)
    assert openings == pytest.approx(
        np.full((layout.columns * layout.rows, 2), 14.0), abs=0.003
    )
    assert reliefs == pytest.approx(
        np.full((layout.columns * layout.rows, 2), 14.8), abs=0.003
    )
    assert variants.axis_pitch(centers[:, 0]) == pytest.approx(
        variants.PITCH, abs=0.003
    )
    assert variants.axis_pitch(centers[:, 1]) == pytest.approx(
        variants.PITCH, abs=0.003
    )
