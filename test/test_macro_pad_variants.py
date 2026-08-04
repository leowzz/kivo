from pathlib import Path

import numpy as np
import pytest
import trimesh

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


@pytest.mark.parametrize("name", ["3x4", "4x4", "5x4"])
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


@pytest.mark.parametrize("name", ["3x4", "4x4", "5x4"])
def test_bottom_internal_core_is_never_scaled(name: str) -> None:
    source = variants.load_source(
        SOURCE / "pico_macro_pad_bottom_fitted_to_usb_c.stl.stl"
    )
    _source_shell, source_core = variants.split_bottom(source)
    parts = variants.expand_bottom_parts(source, variants.LAYOUTS[name])
    assert parts.core.extents == pytest.approx(source_core.extents, abs=1e-6)
    assert parts.core.volume == pytest.approx(source_core.volume, abs=1e-4)


@pytest.mark.parametrize("name", ["3x4", "4x4", "5x4"])
def test_bottom_growth_corridors_are_empty(name: str) -> None:
    source = variants.load_source(
        SOURCE / "pico_macro_pad_bottom_fitted_to_usb_c.stl.stl"
    )
    layout = variants.LAYOUTS[name]
    mesh = variants.generate_bottom(source, layout)
    points = trimesh.intersections.mesh_plane(
        mesh, plane_normal=[0.0, 0.0, 1.0], plane_origin=[0.0, 0.0, 5.0]
    ).reshape(-1, 3)[:, :2]
    left, right, _bottom = layout.growth
    width, height = mesh.extents[:2]
    margin = 0.5
    corridors: list[tuple[np.ndarray, np.ndarray]] = []

    if left > 0.0:
        corridors.extend(
            [
                (
                    np.array(
                        [variants.CORE_INSET + margin, variants.CORE_INSET + margin]
                    ),
                    np.array(
                        [
                            variants.CORE_INSET + left - margin,
                            source.extents[1] - variants.CORE_INSET - margin,
                        ]
                    ),
                ),
                (
                    np.array(
                        [
                            width - variants.CORE_INSET - right + margin,
                            variants.CORE_INSET + margin,
                        ]
                    ),
                    np.array(
                        [
                            width - variants.CORE_INSET - margin,
                            source.extents[1] - variants.CORE_INSET - margin,
                        ]
                    ),
                ),
            ]
        )

    corridors.append(
        (
            np.array(
                [
                    variants.CORE_INSET + margin,
                    source.extents[1] - variants.CORE_INSET + margin,
                ]
            ),
            np.array(
                [
                    width - variants.CORE_INSET - margin,
                    height - variants.CORE_INSET - margin,
                ]
            ),
        )
    )
    for lower, upper in corridors:
        inside = np.all((points > lower) & (points < upper), axis=1)
        assert not np.any(inside)


def test_validate_pair_reports_the_complete_contract() -> None:
    top_source = variants.load_source(SOURCE / "pico_macro_pad_top.stl.stl")
    bottom_source = variants.load_source(
        SOURCE / "pico_macro_pad_bottom_fitted_to_usb_c.stl.stl"
    )
    layout = variants.LAYOUTS["4x4"]
    report = variants.validate_pair(
        variants.generate_top(top_source, layout),
        variants.generate_bottom(bottom_source, layout),
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
    for name in variants.LAYOUTS:
        directory = tmp_path / "models" / name
        top_path = directory / f"pico_macro_pad_{name}_top.stl"
        bottom_path = directory / f"pico_macro_pad_{name}_bottom_fitted_to_usb_c.stl"
        assert top_path.is_file()
        assert bottom_path.is_file()
        assert (tmp_path / "previews" / f"{name}-top.png").is_file()
        assert (tmp_path / "previews" / f"{name}-bottom.png").is_file()
        assert (tmp_path / "previews" / f"{name}-type-c.png").is_file()

        top = trimesh.load_mesh(top_path, file_type="stl", process=False)
        bottom = trimesh.load_mesh(bottom_path, file_type="stl", process=False)
        assert isinstance(top, trimesh.Trimesh)
        assert isinstance(bottom, trimesh.Trimesh)
        top.merge_vertices()
        bottom.merge_vertices()
        report = variants.validate_pair(
            top, bottom, source_bottom, variants.LAYOUTS[name]
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
