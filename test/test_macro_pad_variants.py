import hashlib
from pathlib import Path

import numpy as np
import pytest
import trimesh

from scripts import macro_pad_variants as variants


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "models/3d-print/3x3keypad"


def test_layout_contracts() -> None:
    assert variants.LAYOUTS["3x4"].footprint == pytest.approx((65.15, 84.20))
    assert variants.LAYOUTS["4x3"].footprint == pytest.approx((84.20, 65.15))
    assert variants.LAYOUTS["4x3"].growth == pytest.approx((9.525, 9.525, 0.0))
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
def test_source_mesh_contract(
    filename: str, faces: int, extents: tuple[float, ...]
) -> None:
    mesh = variants.load_source(SOURCE / filename)
    assert len(mesh.faces) == faces
    assert mesh.extents == pytest.approx(extents, abs=0.003)
    assert np.allclose(mesh.bounds[0], 0.0, atol=1e-6)
    assert mesh.is_watertight
    assert mesh.is_winding_consistent
    assert mesh.body_count == 1


def test_source_hash_contract() -> None:
    for filename, expected in variants.SOURCE_HASHES.items():
        assert hashlib.sha256((SOURCE / filename).read_bytes()).hexdigest() == expected


def test_load_source_rejects_changed_canonical_mesh(tmp_path: Path) -> None:
    filename = "pico_macro_pad_top.stl.stl"
    changed = tmp_path / filename
    changed.write_bytes((SOURCE / filename).read_bytes() + b"changed")

    with pytest.raises(ValueError, match="source hash mismatch"):
        variants.load_source(changed)


@pytest.mark.parametrize("name", ["3x4", "4x3", "4x4", "5x4"])
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

    expected_centers = variants.expected_switch_centers(layout)
    actual_centers = variants.switch_section_centers(mesh, z=2.7, nominal_size=14.0)
    openings = variants.switch_section_sizes(mesh, z=2.7, nominal_size=14.0)
    reliefs = variants.switch_section_sizes(mesh, z=1.0, nominal_size=14.8)
    assert len(actual_centers) == layout.columns * layout.rows
    assert actual_centers == pytest.approx(expected_centers, abs=0.003)
    assert openings == pytest.approx(
        np.full((layout.columns * layout.rows, 2), 14.0), abs=0.003
    )
    assert reliefs == pytest.approx(
        np.full((layout.columns * layout.rows, 2), 14.8), abs=0.003
    )
    assert variants.axis_pitch(actual_centers[:, 0]) == pytest.approx(
        variants.PITCH, abs=0.003
    )
    assert variants.axis_pitch(actual_centers[:, 1]) == pytest.approx(
        variants.PITCH, abs=0.003
    )
    assert variants.screw_section_sizes(mesh, z=1.0, window=3.4) == pytest.approx(
        np.full((4, 2), 4.6), abs=0.003
    )
    assert variants.screw_section_sizes(mesh, z=2.0, window=2.0) == pytest.approx(
        np.full((4, 2), 2.95), abs=0.003
    )


def test_validator_rejects_a_shifted_switch_opening() -> None:
    top_source = variants.load_source(SOURCE / "pico_macro_pad_top.stl.stl")
    bottom_source = variants.load_source(
        SOURCE / "pico_macro_pad_bottom_fitted_to_usb_c.stl.stl"
    )
    layout = variants.LAYOUTS["3x4"]
    top = variants.generate_top(top_source, layout)
    bottom = variants.generate_bottom(bottom_source, layout)
    center = variants.expected_switch_centers(layout)[4]
    opening_vertices = np.all(np.abs(top.vertices[:, :2] - center) < 8.0, axis=1)
    top.vertices[opening_vertices, 0] += 0.5

    measured = variants.switch_section_centers(top, z=2.7, nominal_size=14.0)
    assert measured[4, 0] == pytest.approx(center[0] + 0.5, abs=0.003)
    with pytest.raises(ValueError, match="switch grid shape drifted"):
        variants.validate_pair(top, bottom, top_source, bottom_source, layout)


def test_axis_pitch_rejects_a_single_coordinate_level() -> None:
    with pytest.raises(ValueError, match="at least two measured coordinate levels"):
        variants.axis_pitch(np.array([13.525, 13.525]))


@pytest.mark.parametrize("name", ["3x4", "4x3", "4x4", "5x4"])
def test_generate_bottom_preserves_protected_features(name: str) -> None:
    source = variants.load_source(
        SOURCE / "pico_macro_pad_bottom_fitted_to_usb_c.stl.stl"
    )
    layout = variants.LAYOUTS[name]
    mesh = variants.generate_bottom(source, layout)

    assert mesh.extents[:2] == pytest.approx(layout.footprint, abs=0.003)
    assert mesh.extents[2] == pytest.approx(15.006, abs=0.001)
    assert mesh.is_watertight
    assert mesh.is_winding_consistent
    assert mesh.body_count == 1
    assert mesh.euler_number == -8

    source_usb = variants.type_c_section(source)
    output_usb = variants.type_c_section(mesh)
    assert output_usb.size == pytest.approx(source_usb.size, abs=0.003)
    assert output_usb.center_offset == pytest.approx(
        source_usb.center_offset, abs=0.003
    )
    assert variants.screw_axes(mesh) == pytest.approx(
        variants.expected_screw_axes(layout.footprint), abs=0.01
    )
    assert variants.screw_section_sizes(mesh, z=1.0, window=3.4) == pytest.approx(
        variants.screw_section_sizes(source, z=1.0, window=3.4), abs=0.003
    )
    assert variants.screw_section_sizes(mesh, z=5.0, window=2.0) == pytest.approx(
        variants.screw_section_sizes(source, z=5.0, window=2.0), abs=0.003
    )


@pytest.mark.parametrize("name", ["3x4", "4x3", "4x4", "5x4"])
def test_bottom_internal_core_is_never_scaled(name: str) -> None:
    source = variants.load_source(
        SOURCE / "pico_macro_pad_bottom_fitted_to_usb_c.stl.stl"
    )
    _source_shell, source_core = variants.split_bottom(source)
    parts = variants.expand_bottom_parts(source, variants.LAYOUTS[name])
    assert parts.core.extents == pytest.approx(source_core.extents, abs=1e-6)
    assert parts.core.volume == pytest.approx(source_core.volume, abs=1e-4)


@pytest.mark.parametrize("name", ["3x4", "4x3", "4x4", "5x4"])
def test_bottom_growth_corridors_are_empty(name: str) -> None:
    source = variants.load_source(
        SOURCE / "pico_macro_pad_bottom_fitted_to_usb_c.stl.stl"
    )
    layout = variants.LAYOUTS[name]
    mesh = variants.generate_bottom(source, layout)
    corridors = variants.growth_corridor_boxes(mesh, source, layout)
    expected_corridors = {"3x4": 1, "4x3": 2, "4x4": 3, "5x4": 3}
    assert len(corridors) == expected_corridors[name]
    for corridor in corridors:
        overlap = variants.boolean_meshes([mesh, corridor], "intersection")
        assert overlap.is_empty or overlap.volume < 1e-6


def test_validator_rejects_a_bottom_floor_hole() -> None:
    top_source = variants.load_source(SOURCE / "pico_macro_pad_top.stl.stl")
    bottom_source = variants.load_source(
        SOURCE / "pico_macro_pad_bottom_fitted_to_usb_c.stl.stl"
    )
    layout = variants.LAYOUTS["5x4"]
    top = variants.generate_top(top_source, layout)
    bottom = variants.generate_bottom(bottom_source, layout)
    cutter = variants.bounds_box(
        np.array([10.0, 60.0, -0.1]), np.array([14.0, 64.0, 2.0])
    )
    damaged = trimesh.boolean.difference([bottom, cutter], engine="manifold")
    assert isinstance(damaged, trimesh.Trimesh)
    assert damaged.euler_number == -10

    with pytest.raises(ValueError, match="bottom tunnel topology drifted"):
        variants.validate_pair(top, damaged, top_source, bottom_source, layout)


@pytest.mark.parametrize(
    ("lower", "upper", "message"),
    [
        (
            (10.0, 74.0, 1.0),
            (14.0, 75.0, 3.0),
            "generated bottom geometry drifted",
        ),
        (
            (98.0, 25.0, 1.0),
            (100.0, 29.0, 3.0),
            "protected geometry drifted",
        ),
    ],
)
def test_validator_rejects_added_bottom_ribs(
    lower: tuple[float, ...], upper: tuple[float, ...], message: str
) -> None:
    top_source = variants.load_source(SOURCE / "pico_macro_pad_top.stl.stl")
    bottom_source = variants.load_source(
        SOURCE / "pico_macro_pad_bottom_fitted_to_usb_c.stl.stl"
    )
    layout = variants.LAYOUTS["5x4"]
    top = variants.generate_top(top_source, layout)
    bottom = variants.generate_bottom(bottom_source, layout)
    rib = variants.bounds_box(np.array(lower), np.array(upper))
    damaged = variants.union_meshes([bottom, rib])
    assert damaged.euler_number == bottom.euler_number

    with pytest.raises(ValueError, match=message):
        variants.validate_pair(top, damaged, top_source, bottom_source, layout)


def test_validator_rejects_the_same_generator_regression(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    top_source = variants.load_source(SOURCE / "pico_macro_pad_top.stl.stl")
    bottom_source = variants.load_source(
        SOURCE / "pico_macro_pad_bottom_fitted_to_usb_c.stl.stl"
    )
    layout = variants.LAYOUTS["5x4"]
    top = variants.generate_top(top_source, layout)
    bottom = variants.generate_bottom(bottom_source, layout)
    rib = variants.bounds_box(np.array([10.0, 74.0, 1.0]), np.array([14.0, 75.0, 3.0]))
    damaged = variants.union_meshes([bottom, rib])
    monkeypatch.setattr(
        variants, "generate_bottom", lambda _source, _layout: damaged.copy()
    )

    with pytest.raises(ValueError, match="generated bottom geometry drifted"):
        variants.validate_pair(top, damaged, top_source, bottom_source, layout)


def test_validator_rejects_a_small_controller_bump() -> None:
    top_source = variants.load_source(SOURCE / "pico_macro_pad_top.stl.stl")
    bottom_source = variants.load_source(
        SOURCE / "pico_macro_pad_bottom_fitted_to_usb_c.stl.stl"
    )
    layout = variants.LAYOUTS["5x4"]
    top = variants.generate_top(top_source, layout)
    bottom = variants.generate_bottom(bottom_source, layout)
    bump = variants.bounds_box(np.array([49.0, 10.0, 1.0]), np.array([49.2, 10.2, 2.0]))
    damaged = variants.union_meshes([bottom, bump])

    with pytest.raises(ValueError, match="protected geometry drifted"):
        variants.validate_pair(top, damaged, top_source, bottom_source, layout)


@pytest.mark.parametrize("name", ["3x4", "4x3", "4x4", "5x4"])
def test_protected_geometry_matches_source(name: str) -> None:
    top_source = variants.load_source(SOURCE / "pico_macro_pad_top.stl.stl")
    bottom_source = variants.load_source(
        SOURCE / "pico_macro_pad_bottom_fitted_to_usb_c.stl.stl"
    )
    layout = variants.LAYOUTS[name]
    top = variants.generate_top(top_source, layout)
    bottom = variants.generate_bottom(bottom_source, layout)

    mismatches = variants.protected_region_mismatches(
        top, bottom, top_source, bottom_source, layout
    )
    assert set(mismatches) == {
        "top-switch-cell",
        "top-front-mating-wall",
        "top-left-mating-wall",
        "top-rear-mating-wall",
        "top-right-mating-wall",
        "bottom-controller-group",
        "bottom-left-mating-wall",
        "bottom-rear-mating-wall",
        "bottom-right-mating-wall",
        "bottom-base-skin",
    }
    for label, mismatch in mismatches.items():
        assert mismatch <= variants.protected_volume_tolerance(label)


def test_validate_pair_reports_the_complete_contract() -> None:
    top_source = variants.load_source(SOURCE / "pico_macro_pad_top.stl.stl")
    bottom_source = variants.load_source(
        SOURCE / "pico_macro_pad_bottom_fitted_to_usb_c.stl.stl"
    )
    layout = variants.LAYOUTS["4x4"]
    report = variants.validate_pair(
        variants.generate_top(top_source, layout),
        variants.generate_bottom(bottom_source, layout),
        top_source,
        bottom_source,
        layout,
    )
    assert report.layout == "4x4"
    assert report.switch_count == 16
    assert report.footprint == pytest.approx((84.20, 84.20), abs=0.003)
    assert report.watertight
    assert report.manifold
    assert report.type_c_preserved
    assert report.screws_aligned
    assert report.protected_geometry_preserved
    assert report.growth_corridors_empty
    assert report.generated_geometry_matched


def test_cli_writes_exact_artifact_names(tmp_path: Path) -> None:
    result = variants.main(
        [
            "--source-root",
            str(SOURCE),
            "--output-root",
            str(tmp_path / "models"),
            "--preview-root",
            str(tmp_path / "previews"),
        ]
    )
    assert result == 0
    source_bottom = variants.load_source(
        SOURCE / "pico_macro_pad_bottom_fitted_to_usb_c.stl.stl"
    )
    source_top = variants.load_source(SOURCE / "pico_macro_pad_top.stl.stl")
    for name in variants.LAYOUTS:
        directory = tmp_path / "models" / name
        top_path = directory / f"pico_macro_pad_{name}_top.stl"
        bottom_path = directory / f"pico_macro_pad_{name}_bottom_fitted_to_usb_c.stl"
        assert top_path.is_file()
        assert bottom_path.is_file()
        assert (tmp_path / "previews" / f"{name}-top.png").is_file()
        assert (tmp_path / "previews" / f"{name}-bottom.png").is_file()
        assert (tmp_path / "previews" / f"{name}-interior.png").is_file()
        assert (tmp_path / "previews" / f"{name}-type-c.png").is_file()

        top = trimesh.load_mesh(top_path, file_type="stl", process=False)
        bottom = trimesh.load_mesh(bottom_path, file_type="stl", process=False)
        assert isinstance(top, trimesh.Trimesh)
        assert isinstance(bottom, trimesh.Trimesh)
        top.merge_vertices()
        bottom.merge_vertices()
        report = variants.validate_pair(
            top, bottom, source_top, source_bottom, variants.LAYOUTS[name]
        )
        assert report.layout == name


def test_binary_stl_export_is_deterministic(tmp_path: Path) -> None:
    source = variants.load_source(SOURCE / "pico_macro_pad_top.stl.stl")
    mesh = variants.generate_top(source, variants.LAYOUTS["3x4"])
    first = tmp_path / "first.stl"
    second = tmp_path / "second.stl"
    variants.export_stl(mesh, first)
    variants.export_stl(mesh, second)

    data = first.read_bytes()
    assert data == second.read_bytes()
    assert len(data) == 84 + 50 * len(variants.prepare_stl_mesh(mesh).faces)


def test_cli_filters_layout_and_part(tmp_path: Path) -> None:
    output = tmp_path / "models"
    result = variants.main(
        [
            "--source-root",
            str(SOURCE),
            "--output-root",
            str(output),
            "--preview-root",
            str(tmp_path / "previews"),
            "--layout",
            "4x4",
            "--only",
            "top",
        ]
    )
    assert result == 0
    assert (output / "4x4/pico_macro_pad_4x4_top.stl").is_file()
    assert not (output / "4x4/pico_macro_pad_4x4_bottom_fitted_to_usb_c.stl").exists()
    assert not (output / "3x4").exists()
    assert not (output / "5x4").exists()
