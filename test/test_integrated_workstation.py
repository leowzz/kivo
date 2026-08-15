from __future__ import annotations

import hashlib
from pathlib import Path

import numpy as np
import pytest
import trimesh

from scripts.modeling import integrated_workstation as workstation
from scripts.modeling import macro_pad_variants as macro

GeneratedModels = tuple[
    trimesh.Trimesh, trimesh.Trimesh, trimesh.Trimesh, trimesh.Trimesh
]


@pytest.fixture(scope="module")
def generated_models() -> GeneratedModels:
    return (
        workstation.generate_shell(),
        workstation.generate_sloped_panel(),
        workstation.generate_cover(),
        workstation.generate_handset_mount(),
    )


@pytest.fixture(scope="module")
def controller_cradle_module() -> trimesh.Trimesh:
    return workstation.generate_controller_cradle_module()


def test_generated_models_validate(
    generated_models: GeneratedModels,
) -> None:
    shell, panel, cover, handset_mount = generated_models
    report = workstation.validate_models(shell, panel, cover, handset_mount)

    assert report.key_count == 18
    assert report.key_layout == (6, 3)
    assert report.key_pitch == 19.05
    assert report.key_plane_degrees == pytest.approx(30.0)
    assert report.key_support_rail_count == 2
    assert report.key_support_wall_count == 2
    assert report.key_support_center_corridor_width == pytest.approx(35.3)
    assert report.wire_clip_count == 6
    assert report.controller_bay == pytest.approx((28.64, 57.65))
    assert report.controller_support_levels == (3.0, 6.5)
    assert report.screen_board == (64.9, 35.03)
    assert report.screen_plane_degrees == pytest.approx(30.0)
    assert report.toggle_hole_count == 2
    assert report.toggle_plane_degrees == pytest.approx(0.0)
    assert report.panel_screw_count == 6
    assert report.bottom_cover_screw_count == 6
    assert report.handset_mount_insert_count == 2
    assert report.handset_side_hole_count == 2
    assert report.handset_cable_hole_count == 1
    assert report.shell_watertight
    assert report.panel_watertight
    assert report.cover_watertight
    assert report.handset_mount_watertight


def test_switch_apertures_preserve_canonical_stepped_geometry(
    generated_models: GeneratedModels,
) -> None:
    _, panel, _, _ = generated_models

    lower = macro.measure_switch_section(
        panel, z=1.0, nominal_size=workstation.LOWER_SWITCH_APERTURE
    )
    upper = macro.measure_switch_section(
        panel, z=2.7, nominal_size=workstation.UPPER_SWITCH_APERTURE
    )

    assert lower.centers.shape == (18, 2)
    assert upper.centers.shape == (18, 2)
    assert np.allclose(lower.sizes, 14.8, atol=0.003)
    assert np.allclose(upper.sizes, 14.0, atol=0.003)
    assert np.allclose(np.diff(lower.x_levels), 19.05, atol=0.003)
    assert np.allclose(np.diff(lower.y_levels), 19.05, atol=0.003)


def test_shell_has_two_solid_key_field_support_walls(
    generated_models: GeneratedModels,
) -> None:
    shell, _, cover, _ = generated_models

    assert workstation.KEY_SUPPORT_RAIL_X_CENTERS == pytest.approx((123.45, 161.55))
    assert workstation.KEY_SUPPORT_RAIL_COUNT == 2
    assert workstation.KEY_SUPPORT_WALL_COUNT == 2
    assert workstation.KEY_SUPPORT_RAIL_WIDTH == pytest.approx(2.8)
    assert workstation.KEY_SUPPORT_WALL_Y0 == pytest.approx(4.0)
    assert workstation.KEY_SUPPORT_WALL_Y1 == pytest.approx(62.625)
    assert workstation.KEY_SUPPORT_CENTER_CORRIDOR_WIDTH == pytest.approx(35.3)
    walls = workstation.build_key_field_support_walls()
    assert len(walls) == 2
    assert not hasattr(workstation, "KEY_SUPPORT_LEG_COUNT")
    for wall in walls:
        assert wall.bounds[0, 2] == pytest.approx(0.0, abs=0.003)

    placed_cover = cover.copy()
    placed_cover.apply_translation([0.0, 0.0, -workstation.COVER_THICKNESS])
    combined_walls = workstation.union(walls)
    assert workstation.intersection_volume(combined_walls, placed_cover) < 0.01
    workstation.validate_key_field_support_walls(shell)


def test_controller_bay_accepts_both_reference_boards() -> None:
    bay = np.array(
        [workstation.CONTROLLER_CLEAR_WIDTH, workstation.CONTROLLER_CLEAR_LENGTH]
    )
    rp2040 = np.array([workstation.RP2040_BOARD_WIDTH, workstation.RP2040_BOARD_LENGTH])
    esp32_s3 = np.array(
        [workstation.ESP32_S3_BOARD_WIDTH, workstation.ESP32_S3_BOARD_LENGTH]
    )

    assert np.all(bay > rp2040)
    assert np.all(bay > esp32_s3)
    assert bay - esp32_s3 == pytest.approx((0.7, 0.5))
    assert workstation.ESP32_S3_INNER_BOARD_LENGTH == pytest.approx(57.15)
    assert workstation.ESP32_S3_BOARD_LENGTH == pytest.approx(57.15)
    assert (
        workstation.ESP32_S3_BOARD_LENGTH - workstation.CONTROLLER_USB_END_RELIEF
    ) == pytest.approx(55.15)


def test_controller_uses_23_mm_rp2040_slot_and_esp_retaining_lips(
    generated_models: GeneratedModels,
) -> None:
    _, _, cover, _ = generated_models

    assert workstation.CONTROLLER_RP2040_RAISE == 3.0
    assert workstation.CONTROLLER_ESP32_S3_RAISE == 6.5
    assert workstation.RP2040_SLOT_CLEAR_WIDTH == 23.0
    assert workstation.RP2040_SLOT_CLEAR_WIDTH - workstation.RP2040_BOARD_WIDTH == (
        pytest.approx(0.14)
    )
    assert workstation.RP2040_SLOT_WALL_THICKNESS == 2.4
    assert workstation.RP2040_SLOT_WALL_HEIGHT == 2.5
    assert workstation.RP2040_TOP_CLIP_WIDTH == 10.0
    assert workstation.RP2040_TOP_CLIP_OVERLAP == 1.0
    assert workstation.RP2040_TOP_CLIP_CLEARANCE == 0.2
    assert workstation.CONTROLLER_RETAINER_STEM_THICKNESS == 1.8
    assert workstation.CONTROLLER_RETAINER_LENGTH == 12.0
    assert workstation.CONTROLLER_RETAINER_OVERLAP == 0.8
    assert workstation.CONTROLLER_RETAINER_CLEARANCE == 0.2
    assert workstation.CONTROLLER_RETAINER_LIP_THICKNESS == 0.8
    assert not hasattr(workstation, "CONTROLLER_SNAP_STEM_THICKNESS")
    assert not hasattr(workstation, "CONTROLLER_TIE_SLOT_CENTER_OFFSETS")
    workstation.validate_controller_cradle(cover)


def test_standalone_controller_cradle_module_reuses_cover_mount_geometry(
    controller_cradle_module: trimesh.Trimesh,
) -> None:
    workstation.validate_controller_cradle_module(controller_cradle_module)

    assert controller_cradle_module.bounds[0] == pytest.approx((0.0, 0.0, 0.0))
    assert controller_cradle_module.extents == pytest.approx(
        (37.0, 63.65, 11.5), abs=0.003
    )
    placed = workstation.place_controller_cradle_module(controller_cradle_module)
    for mount in workstation.build_controller_mounts():
        assert workstation.intersection_volume(placed, mount) == pytest.approx(
            mount.volume, abs=0.02
        )


def test_type_c_opening_and_controller_are_at_the_rear(
    generated_models: GeneratedModels,
) -> None:
    shell, _, _, _ = generated_models

    assert workstation.CONTROLLER_Y1 == pytest.approx(
        workstation.WEDGE_Y1 - workstation.WEDGE_WALL
    )
    assert workstation.CONTROLLER_Y1 == pytest.approx(105.0)
    assert workstation.CONTROLLER_USB_OPENING_Y0 <= workstation.CONTROLLER_Y1
    assert (
        workstation.CONTROLLER_USB_OPENING_Y0
        > (workstation.WEDGE_Y0 + workstation.WEDGE_Y1) / 2.0
    )
    workstation.validate_controller_connector_opening(shell)


def test_screen_header_slot_is_on_left_and_covers_all_eight_pins(
    generated_models: GeneratedModels,
) -> None:
    _, panel, _, _ = generated_models

    assert workstation.SCREEN_HEADER_PIN_COUNT == 8
    assert workstation.SCREEN_HEADER_FIRST_PIN_X == 11.38
    assert workstation.SCREEN_HEADER_PIN_PITCH == 2.54
    assert workstation.SCREEN_HEADER_PIN_Y_FROM_TOP == 1.93
    assert workstation.SCREEN_HEADER_PIN_CENTERS[:, 0].max() < (
        workstation.SCREEN_BOARD_WIDTH / 2.0
    )
    workstation.validate_screen_header_access(panel)


def test_screen_has_four_backside_heat_set_insert_through_holes(
    generated_models: GeneratedModels,
) -> None:
    _, panel, _, _ = generated_models

    assert workstation.SCREEN_BOARD_HOLES.shape == (4, 2)
    assert workstation.SCREEN_INSERT_THROUGH_DIAMETER == 4.8
    assert workstation.SCREEN_INSERT_MATERIAL_DEPTH == pytest.approx(5.4)
    assert workstation.SCREEN_INSERT_MATERIAL_DEPTH >= (
        workstation.HEAT_SET_INSERT_LENGTH
    )
    workstation.validate_screen_insert_holes(panel)


def test_panel_has_two_evenly_spaced_toggle_switches_on_horizontal_platform(
    generated_models: GeneratedModels,
) -> None:
    shell, panel, _, _ = generated_models

    assert workstation.SCREEN_BEZEL_X0 == workstation.PANEL_X0
    assert workstation.SCREEN_BEZEL_CENTER_X == pytest.approx(113.0)
    assert workstation.TOGGLE_SWITCH_COUNT == 2
    assert workstation.TOGGLE_SWITCH_CENTERS.shape == (2, 2)
    assert workstation.TOGGLE_SWITCH_HOLE_DIAMETER == 12.0
    assert workstation.TOGGLE_SWITCH_BODY_WIDTH == 15.0
    assert workstation.TOGGLE_SWITCH_BODY_LENGTH == 29.0
    assert workstation.TOGGLE_SWITCH_BODY_DEPTH == 27.0
    assert workstation.TOGGLE_SWITCH_MOUNTING_PLATE_THICKNESS == pytest.approx(3.4)
    assert (
        workstation.TOGGLE_SWITCH_CAVITY_X1 - workstation.TOGGLE_SWITCH_CAVITY_X0
        == (pytest.approx(39.6))
    )
    assert (
        workstation.TOGGLE_SWITCH_CAVITY_Y1 - workstation.TOGGLE_SWITCH_CAVITY_Y0
        == (pytest.approx(29.6))
    )
    assert workstation.TOGGLE_SWITCH_MAX_BRIDGE == pytest.approx(29.6)
    assert np.allclose(
        workstation.TOGGLE_SWITCH_CENTERS[:, 1],
        workstation.TOGGLE_SWITCH_CENTER_Y,
    )
    assert np.allclose(
        np.diff(workstation.TOGGLE_SWITCH_CENTERS[:, 0]),
        workstation.TOGGLE_SWITCH_CENTER_PITCH,
    )
    assert (
        workstation.TOGGLE_SWITCH_CENTER_PITCH - workstation.TOGGLE_SWITCH_BODY_WIDTH
        >= 1.0
    )
    assert workstation.TOGGLE_SWITCH_CENTER_PITCH == pytest.approx(24.0)
    assert workstation.TOGGLE_SWITCH_CENTERS[:, 0] == pytest.approx((163.0, 187.0))
    assert np.mean(workstation.TOGGLE_SWITCH_CENTERS[:, 0]) == pytest.approx(
        (workstation.TOGGLE_SWITCH_PLATFORM_X0 + workstation.TOGGLE_SWITCH_PLATFORM_X1)
        / 2.0
    )
    workstation.validate_toggle_switch_holes(panel, shell)


def test_panel_has_six_support_free_recessed_fly_wire_clips(
    generated_models: GeneratedModels,
) -> None:
    _, panel, _, _ = generated_models

    assert workstation.WIRE_CLIP_CENTERS.shape == (6, 2)
    assert workstation.WIRE_CLIP_MOUTH_WIDTH == 1.5
    assert workstation.WIRE_CLIP_POCKET_WIDTH == 3.0
    assert workstation.WIRE_CLIP_FRONT_SKIN == pytest.approx(1.2)
    assert (
        workstation.WIRE_CLIP_POCKET_WIDTH - workstation.WIRE_CLIP_MOUTH_WIDTH
    ) / 2.0 <= workstation.WIRE_CLIP_TRANSITION_DEPTH
    workstation.validate_wire_clips(panel)


def test_user_measured_heat_set_insert_holes_are_hidden_and_blind(
    generated_models: GeneratedModels,
) -> None:
    shell, _, cover, _ = generated_models

    assert workstation.HEAT_SET_INSERT_NARROW_DIAMETER == 3.9
    assert workstation.HEAT_SET_INSERT_WIDE_DIAMETER == 4.9
    assert workstation.HEAT_SET_INSERT_LENGTH == 4.9
    assert workstation.HEAT_SET_INSERT_HOLE_DIAMETER == 4.8
    assert workstation.HEAT_SET_INSERT_LEAD_DIAMETER == 5.1
    assert workstation.HEAT_SET_INSERT_HOLE_DEPTH == 5.4
    assert workstation.M3_SCREW_THREAD_DIAMETER == 2.9
    assert workstation.M3_SCREW_HEAD_DIAMETER == 5.3
    assert workstation.M3_SCREW_HEAD_CLEARANCE_DIAMETER == 5.6
    assert workstation.M3_SCREW_HEAD_RECESS_DEPTH == 0.5
    workstation.validate_bottom_cover_attachment(shell, cover)


def test_bottom_cover_matches_controller_chassis_footprint(
    generated_models: GeneratedModels,
) -> None:
    shell, _, cover, _ = generated_models

    assert shell.bounds[0, 0] == pytest.approx(workstation.WEDGE_X0, abs=0.003)
    assert cover.bounds[0, 0] == pytest.approx(workstation.WEDGE_X0, abs=0.003)
    assert cover.bounds[1, 0] == pytest.approx(workstation.WEDGE_X1, abs=0.003)
    assert cover.bounds[0, 1] == pytest.approx(workstation.WEDGE_Y0, abs=0.003)
    assert cover.bounds[1, 1] == pytest.approx(workstation.WEDGE_Y1, abs=0.003)
    assert workstation.COVER_SCREW_CENTERS.shape == (6, 2)
    assert np.array_equal(
        workstation.COVER_SCREW_CENTERS,
        workstation.SHARED_ATTACHMENT_BOTTOM_CENTERS,
    )
    assert np.array_equal(
        np.unique(workstation.COVER_SCREW_CENTERS[:, 1]),
        np.array([13.0, 62.0, 99.0]),
    )
    assert workstation.SHARED_ATTACHMENT_BOTTOM_CENTERS[
        workstation.SCREEN_SIDE_REAR_ATTACHMENT_INDEX
    ] == pytest.approx((170.0, 99.0))
    assert not hasattr(workstation, "HANDSET_POCKET_WIDTH")
    workstation.validate_bottom_cover_attachment(shell, cover)


def test_handset_base_sits_flat_and_flush_to_rear_on_two_same_height_holes(
    generated_models: GeneratedModels,
) -> None:
    shell, _, cover, handset_mount = generated_models

    assert workstation.HANDSET_MOUNT_INSERT_LOCAL_CENTERS.shape == (2, 2)
    assert workstation.HANDSET_MOUNT_INSERT_SURFACE_X == workstation.handset.OUTER_WIDTH
    assert np.allclose(
        workstation.HANDSET_SIDE_HOLE_CENTERS,
        np.array([[47.2, 4.6], [90.0, 4.6]]),
    )
    assert np.ptp(workstation.HANDSET_SIDE_HOLE_CENTERS[:, 1]) == pytest.approx(0.0)
    assert workstation.HANDSET_SIDE_HOLE_DIAMETER == 3.4
    assert workstation.HANDSET_CABLE_HOLE_DIAMETER == 12.0
    assert workstation.HANDSET_CABLE_HOLE_LOCAL_CENTER == pytest.approx((47.3, 12.0))
    assert workstation.HANDSET_CABLE_HOLE_CENTER == pytest.approx((76.5, 9.6))
    assert np.allclose(
        workstation.HANDSET_CABLE_HOLE_CENTER,
        workstation.HANDSET_MOUNT_ORIGIN[1:]
        + workstation.HANDSET_CABLE_HOLE_LOCAL_CENTER,
    )
    assert workstation.HANDSET_MOUNT_INSERT_BLIND_FLOOR == pytest.approx(1.2)
    assert workstation.HANDSET_MOUNT_ORIGIN[2] == pytest.approx(
        -workstation.COVER_THICKNESS
    )
    placed_mount = workstation.place_handset_mount(handset_mount)
    placed_cover = cover.copy()
    placed_cover.apply_translation([0.0, 0.0, -workstation.COVER_THICKNESS])
    assert placed_mount.bounds[0, 2] == pytest.approx(
        placed_cover.bounds[0, 2], abs=0.003
    )
    assert placed_mount.bounds[1, 0] == pytest.approx(workstation.WEDGE_X0, abs=0.003)
    assert placed_mount.bounds[1, 1] == pytest.approx(workstation.WEDGE_Y1, abs=0.003)
    assert not hasattr(workstation, "HANDSET_SIDE_ARC_RADIUS")
    assert not hasattr(workstation, "HANDSET_MOUNT_BOSS_LOCAL_X0")
    assert shell.bounds[0, 0] == pytest.approx(workstation.WEDGE_X0, abs=0.003)
    assert handset_mount.extents == pytest.approx((63.8, 78.8, 33.4), abs=0.003)
    workstation.validate_handset_mount_attachment(shell, handset_mount)


def test_sloped_panel_is_flat_printable_and_has_six_aligned_screws(
    generated_models: GeneratedModels,
) -> None:
    shell, panel, _, _ = generated_models

    workstation.validate_panel_attachment(shell, panel)
    assert panel.bounds[0, 2] == pytest.approx(0.0, abs=0.003)
    assert workstation.PANEL_SCREW_CENTERS.shape == (6, 2)
    screen_side_center = workstation.PANEL_SCREW_CENTERS[
        workstation.SCREEN_SIDE_REAR_ATTACHMENT_INDEX
    ]
    assert screen_side_center == pytest.approx((158.0, 109.0))
    screen_hardware_centers = (
        workstation.SCREEN_BOARD_HOLES + workstation.SCREEN_BOARD_ORIGIN
    )
    assert np.linalg.norm(
        screen_hardware_centers - screen_side_center, axis=1
    ).min() > (
        workstation.SCREEN_INSERT_COLLAR_RADIUS
        + workstation.M3_SCREW_HEAD_CLEARANCE_DIAMETER / 2.0
    )


def test_panel_bosses_continue_to_bottom_cover_as_six_support_free_pillars(
    generated_models: GeneratedModels,
) -> None:
    shell, _, _, _ = generated_models

    assert (
        workstation.PANEL_SCREW_CENTERS.shape
        == workstation.SHARED_ATTACHMENT_BOTTOM_CENTERS.shape
        == (6, 2)
    )
    assert workstation.SHARED_ATTACHMENT_BASE_HEIGHT == pytest.approx(
        workstation.PANEL_INSERT_BOSS_DEPTH
    )
    for panel_center, bottom_center in zip(
        workstation.PANEL_SCREW_CENTERS,
        workstation.SHARED_ATTACHMENT_BOTTOM_CENTERS,
        strict=True,
    ):
        pillar = workstation.build_shared_attachment_pillar(panel_center, bottom_center)
        assert pillar.bounds[0, 2] == pytest.approx(0.0, abs=0.003)
    workstation.validate_shared_attachment_pillars(shell)


def test_shell_has_no_long_unsupported_rear_panel_bridge(
    generated_models: GeneratedModels,
) -> None:
    shell, panel, _, _ = generated_models

    workstation.validate_panel_attachment(shell, panel)


def test_rear_panel_screws_and_rails_do_not_protrude_behind_chassis(
    generated_models: GeneratedModels,
) -> None:
    shell, panel, _, _ = generated_models

    assert np.all(workstation.PANEL_SCREW_CENTERS[-2:, 1] == 109.0)
    assert workstation.PANEL_SIDE_SUPPORT_Y1 == 117.0
    assert shell.bounds[1, 1] == pytest.approx(workstation.WEDGE_Y1, abs=0.003)
    assert workstation.place_sloped_panel(panel).bounds[1, 1] <= (
        workstation.WEDGE_Y1 + 0.003
    )


def test_exported_stls_reload_as_closed_manifolds(
    tmp_path: Path,
    generated_models: GeneratedModels,
    controller_cradle_module: trimesh.Trimesh,
) -> None:
    shell, panel, cover, handset_mount = generated_models
    targets = {
        "shell": (shell, tmp_path / workstation.SHELL_FILENAME),
        "panel": (panel, tmp_path / workstation.PANEL_FILENAME),
        "cover": (cover, tmp_path / workstation.COVER_FILENAME),
        "controller_cradle_module": (
            controller_cradle_module,
            tmp_path / workstation.CONTROLLER_CRADLE_MODULE_FILENAME,
        ),
        "handset_mount": (
            handset_mount,
            tmp_path / workstation.HANDSET_MOUNT_FILENAME,
        ),
    }

    hashes: dict[str, str] = {}
    for label, (mesh, target) in targets.items():
        workstation.export(mesh, target)
        hashes[label] = hashlib.sha256(target.read_bytes()).hexdigest()
        reloaded = trimesh.load_mesh(target, file_type="stl", process=False)
        assert isinstance(reloaded, trimesh.Trimesh)
        reloaded.merge_vertices()
        reloaded.remove_unreferenced_vertices()
        macro.assert_closed_manifold(reloaded, label)

    assert len(set(hashes.values())) == len(hashes)
