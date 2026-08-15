from __future__ import annotations

from pathlib import Path

import numpy as np
import pytest
import trimesh

from scripts.modeling import heat_set_insert_test_plate as test_plate
from scripts.modeling import macro_pad_variants as macro


@pytest.fixture(scope="module")
def generated_plate() -> trimesh.Trimesh:
    return test_plate.generate_test_plate()


def test_plate_has_labeled_through_hole_range(
    generated_plate: trimesh.Trimesh,
) -> None:
    report = test_plate.validate_test_plate(generated_plate)

    assert report.hole_diameters == pytest.approx((4.3, 4.4, 4.5, 4.6, 4.7, 4.8))
    assert report.lead_diameter == pytest.approx(5.1)
    assert report.lead_depth == pytest.approx(0.6)
    assert report.plate_size == pytest.approx((84.0, 30.0, 8.4))
    assert report.labels == ("4.3", "4.4", "4.5", "4.6", "4.7", "4.8")
    assert report.watertight
    assert report.manifold


def test_plate_holes_are_open_at_body_and_lead_sections(
    generated_plate: trimesh.Trimesh,
) -> None:
    body = test_plate.measured_hole_diameters(
        generated_plate, test_plate.PLATE_THICKNESS / 2.0
    )
    lead = test_plate.measured_hole_diameters(
        generated_plate,
        test_plate.PLATE_THICKNESS - test_plate.LEAD_DEPTH / 2.0,
    )

    assert np.allclose(body, test_plate.HOLE_DIAMETERS, atol=0.003)
    assert np.allclose(lead, test_plate.LEAD_DIAMETER, atol=0.003)


def test_exported_plate_reloads_as_closed_manifold(
    tmp_path: Path,
    generated_plate: trimesh.Trimesh,
) -> None:
    target = tmp_path / test_plate.OUTPUT_FILENAME
    test_plate.export(generated_plate, target)

    reloaded = trimesh.load_mesh(target, file_type="stl", process=False)
    assert isinstance(reloaded, trimesh.Trimesh)
    reloaded.merge_vertices()
    reloaded.remove_unreferenced_vertices()
    macro.assert_closed_manifold(reloaded, "heat-set insert test plate")


def test_top_preview_is_nonblank(
    tmp_path: Path,
    generated_plate: trimesh.Trimesh,
) -> None:
    from PIL import Image

    target = tmp_path / "top.png"
    test_plate.render_top_preview(generated_plate, target)

    image = np.asarray(Image.open(target).convert("RGB"))
    assert image.shape == (520, 1200, 3)
    assert np.count_nonzero(np.any(image != 255, axis=2)) > image.shape[0] * 0.4
