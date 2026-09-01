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
from dataclasses import asdict, dataclass
from pathlib import Path

import numpy as np
import trimesh

if __package__:
    from scripts.modeling import integrated_workstation as geometry
    from scripts.modeling import macro_pad_variants as macro
    from scripts.modeling import telephone_handset_switch_base as preview
else:
    import integrated_workstation as geometry
    import macro_pad_variants as macro
    import telephone_handset_switch_base as preview


HOLE_DIAMETERS = (4.3, 4.4, 4.5, 4.6, 4.7, 4.8)
PLATE_WIDTH = 84.0
PLATE_LENGTH = 30.0
PLATE_THICKNESS = 8.0
PLATE_CORNER_RADIUS = 3.0
HOLE_CENTER_Y = 18.0
HOLE_FIRST_CENTER_X = 9.5
HOLE_PITCH = 13.0
HOLE_CENTERS = np.array(
    [
        [HOLE_FIRST_CENTER_X + index * HOLE_PITCH, HOLE_CENTER_Y]
        for index in range(len(HOLE_DIAMETERS))
    ]
)
LEAD_DIAMETER = 5.1
LEAD_DEPTH = 0.6
CUTTER_OVERSHOOT = 0.2
LABEL_CENTER_Y = 6.0
LABEL_HEIGHT = 0.4
LABEL_OVERLAP = 0.02
LABEL_DIGIT_WIDTH = 2.5
LABEL_DIGIT_HEIGHT = 4.0
LABEL_STROKE = 0.45
LABEL_DIGIT_OFFSET = 1.9
LABEL_DOT_RADIUS = 0.3
LABEL_DOT_Y_OFFSET = -1.7

DEFAULT_OUTPUT_ROOT = Path("models/3d-print/heat-set-insert-test-plate")
DEFAULT_PREVIEW_ROOT = Path("/tmp/kivo-heat-set-insert-test-plate-previews")
OUTPUT_FILENAME = "m3_heat_set_insert_hole_test_plate_4.3-4.8mm.stl"

SEGMENTS_BY_DIGIT = {
    "0": frozenset("abcdef"),
    "1": frozenset("bc"),
    "2": frozenset("abged"),
    "3": frozenset("abgcd"),
    "4": frozenset("fgbc"),
    "5": frozenset("afgcd"),
    "6": frozenset("afgecd"),
    "7": frozenset("abc"),
    "8": frozenset("abcdefg"),
    "9": frozenset("abfgcd"),
}


@dataclass(frozen=True)
class ValidationReport:
    hole_diameters: tuple[float, ...]
    lead_diameter: float
    lead_depth: float
    plate_size: tuple[float, float, float]
    labels: tuple[str, ...]
    watertight: bool
    manifold: bool


def label_digit_parts(digit: str, center: tuple[float, float]) -> list[trimesh.Trimesh]:
    if digit not in SEGMENTS_BY_DIGIT:
        raise ValueError(f"unsupported label digit: {digit}")

    center_x, center_y = center
    width = LABEL_DIGIT_WIDTH
    height = LABEL_DIGIT_HEIGHT
    stroke = LABEL_STROKE
    z_min = PLATE_THICKNESS - LABEL_OVERLAP
    z_max = PLATE_THICKNESS + LABEL_HEIGHT
    segment_bounds = {
        "a": ((-width / 2.0, height / 2.0 - stroke), (width / 2.0, height / 2.0)),
        "b": ((width / 2.0 - stroke, stroke / 2.0), (width / 2.0, height / 2.0)),
        "c": ((width / 2.0 - stroke, -height / 2.0), (width / 2.0, -stroke / 2.0)),
        "d": ((-width / 2.0, -height / 2.0), (width / 2.0, -height / 2.0 + stroke)),
        "e": ((-width / 2.0, -height / 2.0), (-width / 2.0 + stroke, -stroke / 2.0)),
        "f": ((-width / 2.0, stroke / 2.0), (-width / 2.0 + stroke, height / 2.0)),
        "g": ((-width / 2.0, -stroke / 2.0), (width / 2.0, stroke / 2.0)),
    }
    return [
        geometry.box(
            (center_x + lower[0], center_y + lower[1], z_min),
            (center_x + upper[0], center_y + upper[1], z_max),
        )
        for name, (lower, upper) in segment_bounds.items()
        if name in SEGMENTS_BY_DIGIT[digit]
    ]


def diameter_label_parts(diameter: float, center_x: float) -> list[trimesh.Trimesh]:
    label = f"{diameter:.1f}"
    whole, fraction = label.split(".")
    parts = [
        *label_digit_parts(
            whole,
            (center_x - LABEL_DIGIT_OFFSET, LABEL_CENTER_Y),
        ),
        *label_digit_parts(
            fraction,
            (center_x + LABEL_DIGIT_OFFSET, LABEL_CENTER_Y),
        ),
    ]
    dot_height = LABEL_HEIGHT + LABEL_OVERLAP
    parts.append(
        geometry.cylinder(
            LABEL_DOT_RADIUS,
            dot_height,
            (
                center_x,
                LABEL_CENTER_Y + LABEL_DOT_Y_OFFSET,
                PLATE_THICKNESS + (LABEL_HEIGHT - LABEL_OVERLAP) / 2.0,
            ),
        )
    )
    return parts


def generate_test_plate() -> trimesh.Trimesh:
    plate = geometry.rounded_prism(
        PLATE_WIDTH,
        PLATE_LENGTH,
        PLATE_CORNER_RADIUS,
        0.0,
        PLATE_THICKNESS,
        (PLATE_WIDTH / 2.0, PLATE_LENGTH / 2.0),
    )
    cutters: list[trimesh.Trimesh] = []
    for diameter, center in zip(HOLE_DIAMETERS, HOLE_CENTERS, strict=True):
        cutters.append(
            geometry.cylinder(
                diameter / 2.0,
                PLATE_THICKNESS + 2.0 * CUTTER_OVERSHOOT,
                (center[0], center[1], PLATE_THICKNESS / 2.0),
            )
        )
        cutters.append(
            geometry.cylinder(
                LEAD_DIAMETER / 2.0,
                LEAD_DEPTH + CUTTER_OVERSHOOT,
                (
                    center[0],
                    center[1],
                    PLATE_THICKNESS - (LEAD_DEPTH - CUTTER_OVERSHOOT) / 2.0,
                ),
            )
        )
    drilled_plate = geometry.subtract(plate, cutters)

    labels = [
        part
        for diameter, center in zip(HOLE_DIAMETERS, HOLE_CENTERS, strict=True)
        for part in diameter_label_parts(diameter, float(center[0]))
    ]
    return geometry.union([drilled_plate, *labels])


def measured_hole_diameters(mesh: trimesh.Trimesh, z: float) -> np.ndarray:
    lines = trimesh.intersections.mesh_plane(
        mesh,
        plane_normal=[0.0, 0.0, 1.0],
        plane_origin=[0.0, 0.0, z],
    )
    points = lines.reshape(-1, 3)[:, :2]
    measured = []
    for center in HOLE_CENTERS:
        distances = np.linalg.norm(points - center, axis=1)
        local = distances[distances < LEAD_DIAMETER / 2.0 + 0.2]
        if len(local) == 0:
            raise ValueError(f"missing hole section at {center.tolist()}")
        measured.append(float(local.max() * 2.0))
    return np.array(measured)


def validate_test_plate(mesh: trimesh.Trimesh) -> ValidationReport:
    macro.assert_closed_manifold(mesh, "heat-set insert test plate")
    expected_extents = (PLATE_WIDTH, PLATE_LENGTH, PLATE_THICKNESS + LABEL_HEIGHT)
    if not np.allclose(mesh.extents, expected_extents, atol=0.003):
        raise ValueError(f"test plate extents drifted: {mesh.extents.tolist()}")

    body_sections = measured_hole_diameters(mesh, PLATE_THICKNESS / 2.0)
    if not np.allclose(body_sections, HOLE_DIAMETERS, atol=0.003):
        raise ValueError(f"test hole diameters drifted: {body_sections.tolist()}")
    lead_sections = measured_hole_diameters(mesh, PLATE_THICKNESS - LEAD_DEPTH / 2.0)
    if not np.allclose(lead_sections, LEAD_DIAMETER, atol=0.003):
        raise ValueError(f"test hole lead-ins drifted: {lead_sections.tolist()}")

    for diameter, center in zip(HOLE_DIAMETERS, HOLE_CENTERS, strict=True):
        through_probe = geometry.cylinder(
            diameter / 2.0 - 0.05,
            PLATE_THICKNESS + 0.2,
            (center[0], center[1], PLATE_THICKNESS / 2.0),
        )
        if geometry.intersection_volume(mesh, through_probe) > 0.001:
            raise ValueError(f"test hole is blocked: {diameter:.1f} mm")

    return ValidationReport(
        hole_diameters=HOLE_DIAMETERS,
        lead_diameter=LEAD_DIAMETER,
        lead_depth=LEAD_DEPTH,
        plate_size=expected_extents,
        labels=tuple(f"{diameter:.1f}" for diameter in HOLE_DIAMETERS),
        watertight=bool(mesh.is_watertight),
        manifold=bool(mesh.is_winding_consistent),
    )


def export(mesh: trimesh.Trimesh, target: Path) -> None:
    macro.export_stl(mesh, target)


def render_top_preview(mesh: trimesh.Trimesh, target: Path) -> None:
    from PIL import Image, ImageDraw

    if not np.allclose(
        mesh.extents[:2], (PLATE_WIDTH, PLATE_LENGTH), atol=0.003
    ):
        raise ValueError("cannot preview a test plate with unexpected XY extents")

    canvas_width = 1200
    canvas_height = 520
    margin = 48.0
    scale = min(
        (canvas_width - 2.0 * margin) / PLATE_WIDTH,
        (canvas_height - 2.0 * margin) / PLATE_LENGTH,
    )
    rendered_width = PLATE_WIDTH * scale
    rendered_height = PLATE_LENGTH * scale
    offset_x = (canvas_width - rendered_width) / 2.0
    offset_y = (canvas_height - rendered_height) / 2.0

    def point(x: float, y: float) -> tuple[float, float]:
        return (offset_x + x * scale, offset_y + (PLATE_LENGTH - y) * scale)

    image = Image.new("RGB", (canvas_width, canvas_height), "white")
    draw = ImageDraw.Draw(image)
    plate_bounds = [point(0.0, PLATE_LENGTH), point(PLATE_WIDTH, 0.0)]
    draw.rounded_rectangle(
        plate_bounds,
        radius=PLATE_CORNER_RADIUS * scale,
        fill=(205, 208, 210),
        outline=(70, 74, 77),
        width=3,
    )

    for diameter, center in zip(HOLE_DIAMETERS, HOLE_CENTERS, strict=True):
        center_x, center_y = point(float(center[0]), float(center[1]))
        lead_radius = LEAD_DIAMETER * scale / 2.0
        body_radius = diameter * scale / 2.0
        draw.ellipse(
            [
                (center_x - lead_radius, center_y - lead_radius),
                (center_x + lead_radius, center_y + lead_radius),
            ],
            fill=(170, 174, 177),
            outline=(55, 59, 62),
            width=2,
        )
        draw.ellipse(
            [
                (center_x - body_radius, center_y - body_radius),
                (center_x + body_radius, center_y + body_radius),
            ],
            fill="white",
            outline=(90, 94, 97),
            width=2,
        )

    segment_lines = {
        "a": ((-0.5, 0.5), (0.5, 0.5)),
        "b": ((0.5, 0.5), (0.5, 0.0)),
        "c": ((0.5, 0.0), (0.5, -0.5)),
        "d": ((-0.5, -0.5), (0.5, -0.5)),
        "e": ((-0.5, 0.0), (-0.5, -0.5)),
        "f": ((-0.5, 0.5), (-0.5, 0.0)),
        "g": ((-0.5, 0.0), (0.5, 0.0)),
    }
    label_color = (34, 37, 39)
    stroke_width = max(2, round(LABEL_STROKE * scale))
    for diameter, center in zip(HOLE_DIAMETERS, HOLE_CENTERS, strict=True):
        label = f"{diameter:.1f}"
        for digit, digit_offset in zip(
            (label[0], label[2]),
            (-LABEL_DIGIT_OFFSET, LABEL_DIGIT_OFFSET),
            strict=True,
        ):
            digit_center_x = float(center[0]) + digit_offset
            for segment in SEGMENTS_BY_DIGIT[digit]:
                line_start, line_end = segment_lines[segment]
                draw.line(
                    [
                        point(
                            digit_center_x + line_start[0] * LABEL_DIGIT_WIDTH,
                            LABEL_CENTER_Y + line_start[1] * LABEL_DIGIT_HEIGHT,
                        ),
                        point(
                            digit_center_x + line_end[0] * LABEL_DIGIT_WIDTH,
                            LABEL_CENTER_Y + line_end[1] * LABEL_DIGIT_HEIGHT,
                        ),
                    ],
                    fill=label_color,
                    width=stroke_width,
                )
        dot_x, dot_y = point(
            float(center[0]),
            LABEL_CENTER_Y + LABEL_DOT_Y_OFFSET,
        )
        dot_radius = LABEL_DOT_RADIUS * scale
        draw.ellipse(
            [
                (dot_x - dot_radius, dot_y - dot_radius),
                (dot_x + dot_radius, dot_y + dot_radius),
            ],
            fill=label_color,
        )

    target.parent.mkdir(parents=True, exist_ok=True)
    image.save(target)


def main(argv: list[str] | None = None) -> int:
    import argparse
    import json

    parser = argparse.ArgumentParser()
    parser.add_argument("--output-root", type=Path, default=DEFAULT_OUTPUT_ROOT)
    parser.add_argument("--preview-root", type=Path, default=DEFAULT_PREVIEW_ROOT)
    arguments = parser.parse_args(argv)

    mesh = generate_test_plate()
    report = validate_test_plate(mesh)
    target = arguments.output_root / OUTPUT_FILENAME
    export(mesh, target)
    for view in ("isometric", "bottom"):
        preview.render_preview(mesh, arguments.preview_root / f"{view}.png", view)
    render_top_preview(mesh, arguments.preview_root / "top.png")

    payload = asdict(report)
    payload["stl_path"] = str(target)
    payload["stl_sha256"] = hashlib.sha256(target.read_bytes()).hexdigest()
    payload["preview_root"] = str(arguments.preview_root)
    print(json.dumps(payload, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
