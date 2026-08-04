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

from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

import manifold3d
import numpy as np
import trimesh


PITCH = 19.05
SOURCE_SIZE = 65.15
CELL_START = 23.05
CELL_END = CELL_START + PITCH
EPSILON = 1e-5
BOTTOM_X_BREAKS = (8.0, 20.50, 44.15, 57.15)
BOTTOM_Y_BREAKS = (55.00, 57.15)
BASE_SKIN_Z = 1.12
CORE_INSET = 8.0
CORE_OVERLAP = 0.2
BOOLEAN_TOLERANCE = 5e-5


@dataclass(frozen=True)
class Layout:
    name: str
    columns: int
    rows: int

    @property
    def footprint(self) -> tuple[float, float]:
        return (8.0 + self.columns * PITCH, 8.0 + self.rows * PITCH)

    @property
    def growth(self) -> tuple[float, float, float]:
        width_growth = (self.columns - 3) * PITCH
        return (width_growth / 2.0, width_growth / 2.0, (self.rows - 3) * PITCH)


@dataclass
class BottomParts:
    shell: trimesh.Trimesh
    core: trimesh.Trimesh


@dataclass(frozen=True)
class TypeCSection:
    size: tuple[float, float]
    center_offset: float


@dataclass(frozen=True)
class ValidationReport:
    layout: str
    footprint: tuple[float, float]
    switch_count: int
    watertight: bool
    manifold: bool
    type_c_preserved: bool
    screws_aligned: bool


LAYOUTS = {
    name: Layout(name, columns, 4)
    for name, columns in (("3x4", 3), ("4x4", 4), ("5x4", 5))
}

DEFAULT_SOURCE = Path("models/3d-print/3x3keypad")
DEFAULT_OUTPUT = Path("models/3d-print")
DEFAULT_PREVIEWS = Path("/tmp/kivo-macro-pad-previews")
VIEW_ROTATIONS = {
    "top": np.eye(3),
    "bottom": np.diag([1.0, -1.0, -1.0]),
    "type-c": np.array([[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]]),
}


def load_source(path: Path) -> trimesh.Trimesh:
    mesh = trimesh.load_mesh(path, file_type="stl", process=False)
    if not isinstance(mesh, trimesh.Trimesh):
        raise TypeError(f"expected one mesh in {path}")
    mesh = mesh.copy()
    bounds = mesh.bounds.copy()
    transform = np.array(
        [
            [0.0, -1.0, 0.0, bounds[1, 1]],
            [1.0, 0.0, 0.0, -bounds[0, 0]],
            [0.0, 0.0, 1.0, -bounds[0, 2]],
            [0.0, 0.0, 0.0, 1.0],
        ]
    )
    mesh.apply_transform(transform)
    mesh.merge_vertices()
    mesh.remove_unreferenced_vertices()
    return mesh


def boolean_meshes(
    meshes: Iterable[trimesh.Trimesh], operation: str
) -> trimesh.Trimesh:
    solids = [
        manifold3d.Manifold(
            manifold3d.Mesh64(
                vert_properties=np.ascontiguousarray(mesh.vertices, dtype=np.float64),
                tri_verts=np.ascontiguousarray(mesh.faces, dtype=np.uint64),
            )
        )
        for mesh in meshes
    ]
    if not solids:
        raise ValueError("Boolean operation requires at least one mesh")
    result = solids[0]
    for solid in solids[1:]:
        if operation == "union":
            result = result + solid
        elif operation == "intersection":
            result = result ^ solid
        else:
            raise ValueError(f"unsupported Boolean operation: {operation}")
    result = result.simplify(BOOLEAN_TOLERANCE)
    output = result.to_mesh64()
    return trimesh.Trimesh(
        vertices=np.array(output.vert_properties[:, :3], copy=True),
        faces=np.array(output.tri_verts, copy=True),
        process=False,
    )


def clip_slab(
    mesh: trimesh.Trimesh, axis: int, lower: float, upper: float
) -> trimesh.Trimesh:
    bounds = mesh.bounds.copy()
    bounds[0] -= 1.0
    bounds[1] += 1.0
    bounds[0, axis] = lower
    bounds[1, axis] = upper
    extents = bounds[1] - bounds[0]
    box = trimesh.creation.box(extents=extents)
    box.apply_translation((bounds[0] + bounds[1]) / 2.0)
    result = boolean_meshes([mesh, box], "intersection")
    if not isinstance(result, trimesh.Trimesh) or result.is_empty:
        raise ValueError(f"empty slab on axis {axis}: {lower}..{upper}")
    for boundary in (lower, upper):
        on_boundary = np.isclose(
            result.vertices[:, axis], boundary, rtol=0.0, atol=EPSILON
        )
        result.vertices[on_boundary, axis] = boundary
    return result


def union_meshes(parts: Iterable[trimesh.Trimesh]) -> trimesh.Trimesh:
    result = boolean_meshes(parts, "union")
    if not isinstance(result, trimesh.Trimesh) or result.is_empty:
        raise ValueError("mesh union produced no solid")
    result.remove_unreferenced_vertices()
    if not result.is_volume:
        raise ValueError("mesh union did not produce a positive closed volume")
    return result


def affine_axis(
    mesh: trimesh.Trimesh,
    axis: int,
    source_interval: tuple[float, float],
    target_interval: tuple[float, float],
) -> trimesh.Trimesh:
    source_start, source_end = source_interval
    target_start, target_end = target_interval
    result = mesh.copy()
    scale = (target_end - target_start) / (source_end - source_start)
    result.vertices[:, axis] = (
        target_start + (result.vertices[:, axis] - source_start) * scale
    )
    return result


def translated(mesh: trimesh.Trimesh, axis: int, distance: float) -> trimesh.Trimesh:
    result = mesh.copy()
    offset = np.zeros(3)
    offset[axis] = distance
    result.apply_translation(offset)
    return result


def repeat_cell_band(
    mesh: trimesh.Trimesh,
    axis: int,
    near_shift: float,
    far_shift: float,
    tile_offsets: tuple[float, ...],
) -> trimesh.Trimesh:
    minimum, maximum = mesh.bounds[:, axis]
    near = clip_slab(mesh, axis, minimum - 1.0, CELL_START)
    tile = clip_slab(mesh, axis, CELL_START, CELL_END)
    far = clip_slab(mesh, axis, CELL_END, maximum + 1.0)
    parts = [translated(near, axis, near_shift)]
    parts.extend(translated(tile, axis, offset) for offset in tile_offsets)
    parts.append(translated(far, axis, far_shift))
    return union_meshes(parts)


def normalize_origin(mesh: trimesh.Trimesh) -> trimesh.Trimesh:
    result = mesh.copy()
    result.apply_translation(-result.bounds[0])
    return result


def generate_top(source: trimesh.Trimesh, layout: Layout) -> trimesh.Trimesh:
    left, right, bottom = layout.growth
    x_offsets = tuple(-left + index * PITCH for index in range(layout.columns - 2))
    result = repeat_cell_band(source, 0, -left, right, x_offsets)
    y_offsets = tuple(index * PITCH for index in range(layout.rows - 2))
    result = repeat_cell_band(result, 1, 0.0, bottom, y_offsets)
    return normalize_origin(result)


def expected_switch_centers(layout: Layout) -> np.ndarray:
    first = 4.0 + PITCH / 2.0
    return np.array(
        [
            (first + column * PITCH, first + row * PITCH)
            for row in range(layout.rows)
            for column in range(layout.columns)
        ]
    )


def switch_section_sizes(
    mesh: trimesh.Trimesh, centers: np.ndarray, z: float
) -> np.ndarray:
    lines = trimesh.intersections.mesh_plane(
        mesh, plane_normal=[0.0, 0.0, 1.0], plane_origin=[0.0, 0.0, z]
    )
    points = lines.reshape(-1, 3)[:, :2]
    sizes: list[np.ndarray] = []
    for center in centers:
        local = points[np.all(np.abs(points - center) < 8.0, axis=1)]
        if len(local) == 0:
            raise ValueError(f"missing switch section at {center.tolist()}")
        sizes.append(np.ptp(local, axis=0))
    return np.array(sizes)


def axis_pitch(values: np.ndarray) -> float:
    unique = np.unique(np.round(values, 4))
    if len(unique) < 2:
        return PITCH
    differences = np.diff(unique)
    if not np.allclose(differences, PITCH, atol=0.003):
        raise ValueError(f"invalid pitch sequence: {differences.tolist()}")
    return float(differences.mean())


def bounds_box(lower: np.ndarray, upper: np.ndarray) -> trimesh.Trimesh:
    box = trimesh.creation.box(extents=upper - lower)
    box.apply_translation((lower + upper) / 2.0)
    return box


def split_bottom(source: trimesh.Trimesh) -> tuple[trimesh.Trimesh, trimesh.Trimesh]:
    lower = np.array([CORE_INSET, CORE_INSET, source.bounds[0, 2] - 1.0])
    upper = np.array(
        [
            source.extents[0] - CORE_INSET,
            source.extents[1] - CORE_INSET,
            source.extents[2] + 1.0,
        ]
    )
    cutter = bounds_box(lower, upper)
    core = boolean_meshes([source, cutter], "intersection")
    shell = union_meshes(
        [
            clip_slab(
                source,
                2,
                source.bounds[0, 2] - 1.0,
                BASE_SKIN_Z + CORE_OVERLAP,
            ),
            clip_slab(
                source,
                0,
                source.bounds[0, 0] - 1.0,
                CORE_INSET + CORE_OVERLAP,
            ),
            clip_slab(
                source,
                0,
                source.extents[0] - CORE_INSET - CORE_OVERLAP,
                source.bounds[1, 0] + 1.0,
            ),
            clip_slab(
                source,
                1,
                source.bounds[0, 1] - 1.0,
                CORE_INSET + CORE_OVERLAP,
            ),
            clip_slab(
                source,
                1,
                source.extents[1] - CORE_INSET - CORE_OVERLAP,
                source.bounds[1, 1] + 1.0,
            ),
        ]
    )
    if not isinstance(core, trimesh.Trimesh) or not isinstance(shell, trimesh.Trimesh):
        raise ValueError("bottom split did not produce two solids")
    return shell, core


def expand_piecewise(
    mesh: trimesh.Trimesh,
    axis: int,
    breakpoints: tuple[float, ...],
    target_breakpoints: tuple[float, ...],
) -> trimesh.Trimesh:
    if np.allclose(breakpoints, target_breakpoints, rtol=0.0, atol=EPSILON):
        return mesh.copy()
    source_edges = (
        mesh.bounds[0, axis] - 1.0,
        *breakpoints,
        mesh.bounds[1, axis] + 1.0,
    )
    target_edges = (
        target_breakpoints[0] - (breakpoints[0] - source_edges[0]),
        *target_breakpoints,
        target_breakpoints[-1] + (source_edges[-1] - breakpoints[-1]),
    )
    parts: list[trimesh.Trimesh] = []
    for source_pair, target_pair in zip(
        zip(source_edges, source_edges[1:]),
        zip(target_edges, target_edges[1:]),
        strict=True,
    ):
        slab = clip_slab(mesh, axis, *source_pair)
        parts.append(affine_axis(slab, axis, source_pair, target_pair))
    return union_meshes(parts)


def expand_bottom_parts(source: trimesh.Trimesh, layout: Layout) -> BottomParts:
    shell, core = split_bottom(source)
    left, right, bottom = layout.growth
    x_targets = (
        BOTTOM_X_BREAKS[0] - left,
        BOTTOM_X_BREAKS[1],
        BOTTOM_X_BREAKS[2],
        BOTTOM_X_BREAKS[3] + right,
    )
    result = expand_piecewise(shell, 0, BOTTOM_X_BREAKS, x_targets)
    y_targets = (BOTTOM_Y_BREAKS[0], BOTTOM_Y_BREAKS[1] + bottom)
    result = expand_piecewise(result, 1, BOTTOM_Y_BREAKS, y_targets)
    return BottomParts(shell=result, core=core)


def generate_bottom(source: trimesh.Trimesh, layout: Layout) -> trimesh.Trimesh:
    parts = expand_bottom_parts(source, layout)
    return normalize_origin(union_meshes([parts.shell, parts.core]))


def type_c_section(mesh: trimesh.Trimesh) -> TypeCSection:
    lines = trimesh.intersections.mesh_plane(
        mesh, plane_normal=[0.0, 1.0, 0.0], plane_origin=[0.0, 0.05, 0.0]
    )
    points = lines.reshape(-1, 3)[:, (0, 2)]
    center_x = mesh.extents[0] / 2.0
    mouth = points[
        (np.abs(points[:, 0] - center_x) < 8.0)
        & (points[:, 1] > 0.5)
        & (points[:, 1] < 8.0)
    ]
    if len(mouth) == 0:
        raise ValueError("Type-C mouth section is missing")
    lower = mouth.min(axis=0)
    upper = mouth.max(axis=0)
    return TypeCSection(
        size=(float(upper[0] - lower[0]), float(upper[1] - lower[1])),
        center_offset=float((lower[0] + upper[0]) / 2.0 - center_x),
    )


def expected_screw_axes(footprint: tuple[float, float]) -> np.ndarray:
    width, height = footprint
    return np.array(
        [
            (3.8, 3.8),
            (width - 3.8, 3.8),
            (3.8, height - 3.8),
            (width - 3.8, height - 3.8),
        ]
    )


def screw_axes(mesh: trimesh.Trimesh, z: float = 5.0) -> np.ndarray:
    lines = trimesh.intersections.mesh_plane(
        mesh, plane_normal=[0.0, 0.0, 1.0], plane_origin=[0.0, 0.0, z]
    )
    points = lines.reshape(-1, 3)[:, :2]
    axes: list[np.ndarray] = []
    for expected in expected_screw_axes(tuple(mesh.extents[:2])):
        local = points[np.linalg.norm(points - expected, axis=1) < 2.0]
        if len(local) == 0:
            raise ValueError(f"missing screw section near {expected.tolist()}")
        axes.append((local.min(axis=0) + local.max(axis=0)) / 2.0)
    return np.array(axes)


def assert_closed_manifold(mesh: trimesh.Trimesh, label: str) -> None:
    incidence = np.bincount(mesh.edges_unique_inverse)
    if (
        mesh.body_count != 1
        or not mesh.is_watertight
        or not mesh.is_winding_consistent
        or np.any(incidence != 2)
        or np.any(mesh.area_faces < 1e-9)
        or mesh.volume <= 0.0
    ):
        raise ValueError(f"{label} is not one positive closed two-manifold solid")


def validate_pair(
    top: trimesh.Trimesh,
    bottom: trimesh.Trimesh,
    source_bottom: trimesh.Trimesh,
    layout: Layout,
) -> ValidationReport:
    assert_closed_manifold(top, f"{layout.name} top")
    assert_closed_manifold(bottom, f"{layout.name} bottom")
    expected = np.array(layout.footprint)
    if not np.allclose(top.extents[:2], expected, atol=0.003):
        raise ValueError(f"{layout.name} top footprint drifted: {top.extents[:2]}")
    if not np.allclose(bottom.extents[:2], expected, atol=0.003):
        raise ValueError(
            f"{layout.name} bottom footprint drifted: {bottom.extents[:2]}"
        )
    if not np.isclose(top.extents[2], 9.998, atol=0.001):
        raise ValueError(f"{layout.name} top Z extent drifted")
    if not np.isclose(bottom.extents[2], 15.006, atol=0.001):
        raise ValueError(f"{layout.name} bottom Z extent drifted")

    centers = expected_switch_centers(layout)
    if not np.allclose(switch_section_sizes(top, centers, 2.7), 14.0, atol=0.003):
        raise ValueError(f"{layout.name} switch openings drifted")
    if not np.allclose(switch_section_sizes(top, centers, 1.0), 14.8, atol=0.003):
        raise ValueError(f"{layout.name} switch reliefs drifted")
    axis_pitch(centers[:, 0])
    axis_pitch(centers[:, 1])

    source_usb = type_c_section(source_bottom)
    output_usb = type_c_section(bottom)
    type_c_preserved = bool(
        np.allclose(output_usb.size, source_usb.size, atol=0.003)
        and np.isclose(output_usb.center_offset, source_usb.center_offset, atol=0.003)
    )
    if not type_c_preserved:
        raise ValueError(f"{layout.name} Type-C section drifted")

    expected_axes = expected_screw_axes(layout.footprint)
    screws_aligned = bool(
        np.allclose(screw_axes(top, z=1.0), expected_axes, atol=0.01)
        and np.allclose(screw_axes(bottom, z=5.0), expected_axes, atol=0.01)
    )
    if not screws_aligned:
        raise ValueError(f"{layout.name} screw axes drifted")
    if not np.allclose(top.bounds[:, :2], bottom.bounds[:, :2], atol=0.003):
        raise ValueError(f"{layout.name} top and bottom outlines do not align")

    return ValidationReport(
        layout=layout.name,
        footprint=tuple(float(value) for value in expected),
        switch_count=len(centers),
        watertight=True,
        manifold=True,
        type_c_preserved=type_c_preserved,
        screws_aligned=screws_aligned,
    )


def export_stl(mesh: trimesh.Trimesh, target: Path) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    temporary = target.with_suffix(target.suffix + ".tmp")
    temporary.write_bytes(trimesh.exchange.stl.export_stl(mesh))
    temporary.replace(target)


def render_preview(mesh: trimesh.Trimesh, target: Path, view: str) -> None:
    from PIL import Image, ImageDraw

    rotation = VIEW_ROTATIONS[view]
    triangles = mesh.triangles @ rotation.T
    projected = triangles[:, :, :2]
    lower = projected.reshape(-1, 2).min(axis=0)
    upper = projected.reshape(-1, 2).max(axis=0)
    canvas = np.array([1200.0, 900.0])
    scale = float(np.min((canvas - 96.0) / (upper - lower)))
    points = (projected - lower) * scale + 48.0
    points[:, :, 1] = canvas[1] - points[:, :, 1]

    normals = np.cross(
        triangles[:, 1] - triangles[:, 0], triangles[:, 2] - triangles[:, 0]
    )
    lengths = np.linalg.norm(normals, axis=1)
    normals = normals / np.maximum(lengths[:, None], 1e-12)
    light = np.array([0.3, -0.4, 0.85])
    light /= np.linalg.norm(light)
    shade = np.clip(0.55 + 0.35 * np.abs(normals @ light), 0.0, 1.0)
    depth = triangles[:, :, 2].mean(axis=1)

    image = Image.new("RGB", (1200, 900), "white")
    draw = ImageDraw.Draw(image)
    for index in np.argsort(depth):
        level = int(235 - 105 * shade[index])
        polygon = [tuple(value) for value in points[index].tolist()]
        draw.polygon(polygon, fill=(level, level, level), outline=(70, 70, 70))
    pixels = np.asarray(image)
    nonblank = np.count_nonzero(np.any(pixels != 255, axis=2))
    if nonblank < pixels.shape[0] * pixels.shape[1] * 0.05:
        raise ValueError(f"blank preview for {target}")
    target.parent.mkdir(parents=True, exist_ok=True)
    image.save(target)


def export_variant(
    top: trimesh.Trimesh,
    bottom: trimesh.Trimesh,
    layout: Layout,
    output_root: Path,
    only: str | None,
) -> None:
    directory = output_root / layout.name
    if only in (None, "top"):
        export_stl(top, directory / f"pico_macro_pad_{layout.name}_top.stl")
    if only in (None, "bottom"):
        export_stl(
            bottom,
            directory / f"pico_macro_pad_{layout.name}_bottom_fitted_to_usb_c.stl",
        )


def main(argv: list[str] | None = None) -> int:
    import argparse
    import json
    from dataclasses import asdict

    parser = argparse.ArgumentParser()
    parser.add_argument("--source-root", type=Path, default=DEFAULT_SOURCE)
    parser.add_argument("--output-root", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--preview-root", type=Path, default=DEFAULT_PREVIEWS)
    parser.add_argument("--layout", action="append", choices=tuple(LAYOUTS))
    parser.add_argument("--only", choices=("top", "bottom"))
    arguments = parser.parse_args(argv)

    top_source = load_source(arguments.source_root / "pico_macro_pad_top.stl.stl")
    bottom_source = load_source(
        arguments.source_root / "pico_macro_pad_bottom_fitted_to_usb_c.stl.stl"
    )
    selected = arguments.layout or list(LAYOUTS)
    for name in selected:
        layout = LAYOUTS[name]
        top = generate_top(top_source, layout)
        bottom = generate_bottom(bottom_source, layout)
        report = validate_pair(top, bottom, bottom_source, layout)
        export_variant(top, bottom, layout, arguments.output_root, arguments.only)
        render_preview(top, arguments.preview_root / f"{name}-top.png", "top")
        render_preview(bottom, arguments.preview_root / f"{name}-bottom.png", "bottom")
        render_preview(bottom, arguments.preview_root / f"{name}-type-c.png", "type-c")
        print(json.dumps(asdict(report), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
