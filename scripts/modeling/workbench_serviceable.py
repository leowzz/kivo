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
"""Low-angle, hand-wired Workbench; independent of the old PCB drafts.

Coordinates: X right, Y away from the user, Z up. The lid and its fittings
are designed in an untilted local plane; printable files all rest on Z=0.
Component meshes are clearance references, NOT exact vendor CAD.
"""

from __future__ import annotations

import argparse
import base64
import csv
import hashlib
import json
import zlib
import zipfile
from pathlib import Path

import numpy as np
import trimesh
from PIL import Image, ImageDraw, ImageFont

if __package__:
    from scripts.modeling import integrated_workstation as old
    from scripts.modeling import workbench_perfboard_wiring as board_wiring
else:
    import integrated_workstation as old
    import workbench_perfboard_wiring as board_wiring

box, cylinder, union, subtract = old.box, old.cylinder, old.union, old.subtract
rounded = old.rounded_prism

WIDTH, DEPTH = 148.0, 132.0
ANGLE = np.deg2rad(12.0)
LENGTH = DEPTH / np.cos(ANGLE)
LID_Z, LID_T, BASE_T = 26.0, 6.6, 2.6
PITCH, CAPTURE_LIP = 19.05, 1.5
KEY_X = WIDTH / 2 + (np.arange(6) - 2.5) * PITCH
# Electrical numbering is rear to front, left to right.
KEY_Y = np.array([57.1, 38.05, 19.0])
TOP_SCREWS = [(x, y) for x in (8.0, WIDTH - 8) for y in (8.0, LENGTH - 8)]
DECK = old.rotation_transform(ANGLE, (0, 0, LID_Z))
BOTTOM_SCREWS = [(x, y * np.cos(ANGLE)) for x, y in TOP_SCREWS]
DISPLAY_XY = np.array([67.0, 88.0])
DISPLAY_SIZE = np.array([64.90, 35.03])
DISPLAY_HOLES = DISPLAY_XY + old.SCREEN_BOARD_HOLES
OPTION_X, OPTION_Y = 26.0, 109.0
OPTION_SCREWS = [(OPTION_X, OPTION_Y - 21.0), (OPTION_X, OPTION_Y + 21.0)]
BOARD_T, BOARD_Z = 1.6, 8.0
BOARD_CENTER_X = 67.0
KEYCAP_REST_BOTTOM, KEYCAP_TRAVEL = 13.5, 4.0


def placed(mesh: trimesh.Trimesh, transform=DECK) -> trimesh.Trimesh:
    result = mesh.copy()
    result.apply_transform(transform)
    return result


def moved(mesh: trimesh.Trimesh, offset) -> trimesh.Trimesh:
    result = mesh.copy()
    result.apply_translation(offset)
    return result


def holes(points, diameter, bottom, top):
    return [cylinder(diameter / 2, top - bottom, (x, y, (bottom + top) / 2))
            for x, y in points]


def insert_holes(points, surface, direction):
    return [part for point in points
            for part in old.heat_set_insert_cutters(point, surface, direction)]


def ring(width, length, radius, z, thickness, center, border):
    return subtract(rounded(width, length, radius, z, thickness, center),
                    [rounded(width - border * 2, length - border * 2,
                             max(0.5, radius - border), z - 1,
                             thickness + 2, center)])


def build_lid(aperture=14.0):
    plate = rounded(WIDTH, LENGTH, 10, 0, LID_T, (WIDTH / 2, LENGTH / 2))
    # An exposed, replaceable display module; no guessed OLED/EC11 cutouts.
    bezel = ring(72.9, 43.03, 5, LID_T - 0.2, 2.2,
                 tuple(DISPLAY_XY + DISPLAY_SIZE / 2), 3.35)
    result = union([plate, bezel])
    cutters = []
    for y in KEY_Y:
        for x in KEY_X:
            cutters.extend([
                box((x - 7.4, y - 7.4, -1), (x + 7.4, y + 7.4, LID_T - CAPTURE_LIP)),
                box((x - aperture / 2, y - aperture / 2, LID_T - CAPTURE_LIP - .02),
                    (x + aperture / 2, y + aperture / 2, 12)),
            ])
    cutters += holes(TOP_SCREWS, 3.4, -1, 12)
    # Display fasteners install in blind insert pockets from the top.
    cutters += insert_holes(DISPLAY_HOLES, LID_T, -1)
    cutters += insert_holes(OPTION_SCREWS, LID_T, -1)
    # 24.5 mm accommodates the reference's nine contacts, not the old 8-pin count.
    cutters.append(box((DISPLAY_XY[0] + 9.5, DISPLAY_XY[1] + 28.4, -1),
                       (DISPLAY_XY[0] + 34.0, DISPLAY_XY[1] + 35.7, 10)))
    cutters.append(rounded(20.6, 32.6, 2, -1, 12, (OPTION_X, OPTION_Y)))
    # Two slots per cable tie. Three ties per row support six loose wire pairs.
    for y in KEY_Y:
        for x in (45.425, 83.525, 121.625):
            for dx in (-2.8, 2.8):
                cutters.append(box((x + dx - .8, y - 10.7, -1),
                                   (x + dx + .8, y - 9.35, 12)))
    return subtract(result, cutters)


def build_option(toggle=False):
    cap = rounded(28, 48, 3, 0, 2.4, (0, 0))
    cutters = holes([(0, -21), (0, 21)], 3.4, -1, 4)
    if toggle:
        cutters += holes([(0, 0)], 12, -1, 4)
    return subtract(cap, cutters)


def build_shell():
    # A rounded open ring, trimmed to the lid's underside plane.
    raw = ring(WIDTH, DEPTH, 10, BASE_T, 72 - BASE_T,
               (WIDTH / 2, DEPTH / 2), 3)
    posts = [cylinder(6.0, 70 - BASE_T, (x, y, (70 + BASE_T) / 2))
             for x, y in BOTTOM_SCREWS]
    result = union([raw, *posts])
    above_deck = placed(box((-10, -20, 0), (WIDTH + 10, LENGTH + 20, 100)))
    cutters = [above_deck]
    cutters += [placed(part) for part in insert_holes(TOP_SCREWS, 0, -1)]
    cutters += insert_holes(BOTTOM_SCREWS, BASE_T, 1)
    # Large service opening accepts a socket-mounted board and variable USB height.
    cutters.append(box((15, DEPTH - 4, 11), (56, DEPTH + 1, 31)))
    return subtract(result, cutters)


def board_bounds(width, depth):
    return (BOARD_CENTER_X - width / 2, DEPTH - 4 - depth,
            BOARD_CENTER_X + width / 2, DEPTH - 4)


def clamp_centers(width, depth):
    x0, y0, x1, y1 = board_bounds(width, depth)
    return [(x, y) for x in (x0 - 4.3, x1 + 4.3) for y in (y0 + 6, y1 - 6)]


def build_base(width, depth):
    plate = rounded(WIDTH, DEPTH, 10, 0, BASE_T, (WIDTH / 2, DEPTH / 2))
    x0, _, x1, _ = board_bounds(width, depth)
    feet = []
    for x, y in clamp_centers(width, depth):
        feet.append(cylinder(3.8, BOARD_Z + BOARD_T - BASE_T,
                             (x, y, (BASE_T + BOARD_Z + BOARD_T) / 2)))
        lo, hi = ((x, x0 + 2) if x < x0 else (x1 - 2, x))
        feet.append(box((lo, y - 3.0, BASE_T - .1), (hi, y + 3, BOARD_Z)))
    # Bottom cable ties; all knots remain inside, away from the desk.
    anchors = [box((108, y - 3, BASE_T - .1), (132, y + 3, 5.5))
               for y in (72, 98)]
    result = union([plate, *feet, *anchors])
    cutters = holes(BOTTOM_SCREWS, 3.4, -1, 12)
    cutters += [old.countersink_cutter(point, 0, 1) for point in BOTTOM_SCREWS]
    cutters += insert_holes(clamp_centers(width, depth), BOARD_Z + BOARD_T, -1)
    for y in (72, 98):
        cutters.append(box((112, y - 4, BASE_T + .8), (128, y + 4, 4.6)))
    # Four shallow foot recesses. They do not conceal the screws.
    cutters += holes([(x, y) for x in (19, 129) for y in (18, 113)], 11, -1, .8)
    return subtract(result, cutters)


def build_board_clamp():
    # Hole at zero; the long end overlaps the PCB by 2 mm. Mirror for right edge.
    return subtract(rounded(10.3, 7, 1.5, 0, 2.4, (1.15, 0)),
                    holes([(0, 0)], 3.4, -1, 4))


def build_coupon():
    plate = rounded(66, 29, 3, 0, LID_T, (33, 14.5))
    cutters = []
    for x, size in zip((13, 33, 53), (14.0, 14.1, 14.2), strict=True):
        cutters += [box((x - 7.4, 14.5 - 7.4, -1),
                        (x + 7.4, 14.5 + 7.4, LID_T - CAPTURE_LIP)),
                    box((x - size / 2, 14.5 - size / 2, LID_T - CAPTURE_LIP - .02),
                        (x + size / 2, 14.5 + size / 2, 9))]
    return subtract(plate, cutters)


def wire(points, radius=.45):
    """Polyline tubes are visual routing references, not printable components."""
    parts = []
    for a, b in zip(points[:-1], points[1:]):
        a, b = np.asarray(a), np.asarray(b)
        vector = b - a
        if np.linalg.norm(vector) < 1e-9:
            continue
        transform = trimesh.geometry.align_vectors((0, 0, 1), vector)
        segment = trimesh.creation.cylinder(radius, np.linalg.norm(vector), sections=8)
        segment.apply_transform(transform)
        segment.apply_translation((a + b) / 2)
        parts.append(segment)
    return trimesh.util.concatenate(parts)


def components(width, depth):
    refs = []
    # Colors identify assembly parts, not a proposed electronics BOM.
    for row, y in enumerate(KEY_Y):
        for col, x in enumerate(KEY_X):
            label = f"K{row * 6 + col + 1:02}"
            refs.append((label + " body", placed(box((x - 6.9, y - 6.9, -8.0),
                                                       (x + 6.9, y + 6.9, 12.5))), "#282c31"))
            refs.append((label + " flange", placed(box((x - 7.8, y - 7.8, LID_T),
                                                         (x + 7.8, y + 7.8, LID_T + .7))), "#3c4249"))
            color = "#c4c9ce" if col < 5 else "#f99c55"
            refs.append((label + " cap", placed(rounded(17.6, 17.6, 2.0, KEYCAP_REST_BOTTOM, 9,
                                                         (x, y))), color))
    dx, dy = DISPLAY_XY
    refs.append(("Display PCB envelope", placed(box((dx, dy, LID_T),
                 (dx + DISPLAY_SIZE[0], dy + DISPLAY_SIZE[1], LID_T + 1.6))), "#273735"))
    refs.append(("OLED envelope", placed(box((dx + 5.5, dy + 6.0, LID_T + 1.6),
                  (dx + 38.5, dy + 27, LID_T + 4.6))), "#111a23"))
    # Drawing coordinates are measured from the module's top-left corner.
    refs.append(("Encoder envelope", placed(cylinder(8, 13,
                  (dx + 53.29, dy + 17.43, LID_T + 8.1))), "#e3e3df"))
    for yy in (dy + 5.3, dy + 27.8):
        refs.append(("Module button envelope", placed(box((dx + 50.0, yy - 2.8, LID_T + 1.6),
                     (dx + 56.4, yy + 2.8, LID_T + 5.0))), "#f99c55"))
    # One left-hand panel toggle; its dimensions are mechanical references.
    refs.append(("Toggle body envelope", placed(box(
        (OPTION_X - 7.5, OPTION_Y - 14.5, LID_T - 27),
        (OPTION_X + 7.5, OPTION_Y + 14.5, LID_T))), "#554737"))
    refs.append(("Toggle bushing envelope", placed(cylinder(
        5.8, 6.8, (OPTION_X, OPTION_Y, LID_T + 3.4))), "#acb3b8"))
    nut = trimesh.creation.cylinder(radius=9.0, height=2.2, sections=6)
    nut.apply_translation((OPTION_X, OPTION_Y, LID_T + 3.5))
    refs.append(("Toggle mounting nut envelope", placed(nut), "#c7cbd0"))
    pivot = np.array([OPTION_X, OPTION_Y, LID_T + 6.8])
    tip = pivot + np.array([0, -5.0, 19.0])
    refs.append(("Toggle lever envelope", placed(wire([pivot, tip], 2.1)), "#c7cbd0"))
    handle = trimesh.creation.icosphere(subdivisions=2, radius=2.7)
    handle.apply_translation(tip)
    refs.append(("Toggle lever tip envelope", placed(handle), "#f99c55"))
    x0, y0, x1, y1 = board_bounds(width, depth)
    refs.append(("Universal perfboard envelope", box((x0, y0, BOARD_Z),
                 (x1, y1, BOARD_Z + BOARD_T)), "#8d643c"))
    detailed = width == 80 and depth == 60
    def grid_xy(x, y):
        return np.array([x0 + (width - 30 * 2.54) / 2 + (x-1)*2.54,
                         y1 - (depth - 22 * 2.54) / 2 - (y-1)*2.54])
    # Existing measured RP2040 board outline, on female sockets; pin pitch TBD.
    if detailed:
        pin_left=grid_xy(4,2); pin_right=grid_xy(11,2); last=grid_xy(4,21)
        rp_bounds=(pin_left[0]-2.54,last[1]-2.54,pin_right[0]+2.54,pin_left[1]+2.54)
    else:
        rp_bounds=(32,y1-53.34,54.86,y1)
    rx0,ry0,rx1,ry1=rp_bounds
    refs.append(("RP2040 envelope", box((rx0,ry0,18.1),(rx1,ry1,19.7)), "#315e66"))
    # Reference drawing: 17.78 mm between rows, not the 22.86 mm board width.
    for xx in (rx0+1.27,rx1-3.81):
        refs.append(("MCU socket envelope",box((xx,ry0+1.27,9.6),(xx+2.54,ry1-1.27,18.1)),"#20252b"))
    usb_x=(rx0+rx1)/2
    refs.append(("USB envelope",box((usb_x-5,ry1-1,19.7),(usb_x+5,ry1+3.8,23.7)),"#aab2b9"))
    if detailed:
        ports=board_wiring.contacts()
        for name in ("J1","J2","J3","J4","J5"):
            pins=[grid_xy(c.x,c.y) for c in ports if c.connector==name]
            lower=np.min(pins,axis=0)-1.27; upper=np.max(pins,axis=0)+1.27
            refs.append((f"{name} header envelope",box((*lower,9.6),(*upper,18.1)),"#262c35"))
        wires=board_wiring.wiring(ports)
        pads={}
        for i,w in enumerate(wires):
            level=BOARD_Z-.4 if w.bare else BOARD_Z-1.1-(i%3)*.25
            points=[[*grid_xy(x,y),level] for x,y in w.points]
            if not w.bare:
                points=[[*grid_xy(*w.points[0]),BOARD_Z-.4],*points,
                        [*grid_xy(*w.points[-1]),BOARD_Z-.4]]
            refs.append((f"Perfboard wire {i+1} {w.net}",wire(points,.26 if not w.bare else .38),board_wiring.COLORS[w.group]))
            pad_points=[w.points[0],w.points[-1]]
            if w.bare:
                (ax,ay),(bx,by)=w.points
                pad_points=[(x,y) for x in range(int(ax),int(bx)+1) for y in range(int(ay),int(by)+1)]
            for point in pad_points:
                pads[tuple(point)]=grid_xy(*point)
        solder=trimesh.util.concatenate([cylinder(.65,.6,(*xy,BOARD_Z-.25)) for xy in pads.values()])
        refs.append(("Perfboard solder pads",solder,"#bfc5c8"))
    else:
        for row in range(3):
            yy=y0+12.7+row*12.7
            refs.append((f"J{row+1} 2x7 header envelope",box((65,yy-2.5,9.6),(84,yy+2.5,18.1)),"#262c35"))
    # Three color-coded harnesses, open service loops kept in the right-hand bay.
    for row, color in enumerate(("#d79452", "#78a5a3", "#9396c8")):
        yy = float(KEY_Y[row])
        start = trimesh.transform_points([[130, yy, -4]], DECK)[0]
        end=[*grid_xy(23,6+row*5),17] if detailed else [84,y0+12.7+row*12.7,17]
        refs.append((f"J{row+1} service loop", wire([
            start, (135, 63 + row * 8, 20), (135, 102 + row * 4, 17),
            (122, 118 + row * 2, 15), (105, 107, 15),
            end,
        ], 1.3), color))
    return refs


def intersection(a, b):
    return abs(float((old.mesh_to_manifold(a) ^ old.mesh_to_manifold(b)).volume()))


def validate(parts, assembly, width, depth):
    report = {"status": "digital geometry checks passed; physical fit unverified",
              "key_count": 18, "key_pitch_mm": PITCH, "angle_degrees": 12,
              "footprint_mm": [WIDTH, DEPTH], "perfboard_mm": [width, depth],
              "layout": {"display_pcb_origin_mm": DISPLAY_XY.tolist(),
                         "left_toggle_center_mm": [OPTION_X, OPTION_Y],
                         "retention_frames": 0, "dip_switches": 0},
              "parts": {}, "checks": []}
    for name, mesh in parts.items():
        old.macro.assert_closed_manifold(mesh, name)
        if len(mesh.split()) != 1:
            raise ValueError(f"{name} is not one connected printable part")
        report["parts"][name] = {"watertight": bool(mesh.is_watertight),
                                  "volume_mm3": round(float(mesh.volume), 3),
                                  "bounds_mm": mesh.bounds.round(4).tolist()}
    report["checks"].append(f"all {len(parts)} print designs are single connected positive manifolds")
    # Measure actual planar mesh sections, independently of the cutters.
    for height, aperture in ((1, 14.8), (LID_T - .7, 14.0)):
        section = parts["lid"].section([0, 0, 1], [0, 0, height])
        squares = []
        for entity in section.entities:
            if entity.closed:
                points = np.asarray(entity.discrete(section.vertices))[:, :2]
                if np.allclose(np.ptp(points, axis=0), aperture, atol=.015):
                    squares.append((points.min(axis=0) + points.max(axis=0)) / 2)
        expected = sorted((float(x), float(y)) for y in KEY_Y for x in KEY_X)
        if len(squares) != 18 or not np.allclose(sorted(map(tuple, squares)), expected):
            raise ValueError(f"switch section {height} drifted: {len(squares)} apertures")
    report["checks"].append("18 switch apertures measured at both 14.0 and 14.8 mm sections")
    fixed = {name: mesh for name, mesh, _ in assembly
             if name in ("shell", "lid", "base", "toggle_cap") or name.startswith("board clamp")}
    names = list(fixed)
    for i, name in enumerate(names):
        for other in names[i + 1:]:
            volume = intersection(fixed[name], fixed[other])
            if volume > .04:
                raise ValueError(f"printed parts collide: {name} / {other}: {volume:.3f} mm3")
    report["checks"].append("assembled printed parts have no volumetric interference")
    # Conservative switch bodies beneath the plate. Electrical pins need extra space.
    for y in KEY_Y:
        for x in KEY_X:
            keepout = placed(box((x - 7, y - 7, -10.5), (x + 7, y + 7, -.05)))
            for name in ("shell", "base"):
                if intersection(fixed[name], keepout) > .04:
                    raise ValueError(f"{name} blocks switch/wiring clearance at {x}, {y}")
    report["checks"].append("18 underside body/pin envelopes clear shell and base")
    # Test the entire simplified keycap sweep, not only its released position.
    for y in KEY_Y:
        for x in KEY_X:
            swept_cap = placed(rounded(17.6, 17.6, 2.0,
                               KEYCAP_REST_BOTTOM - KEYCAP_TRAVEL,
                               9 + KEYCAP_TRAVEL, (x, y)))
            for name, mesh in fixed.items():
                if intersection(mesh, swept_cap) > .04:
                    raise ValueError(f"{name} blocks keycap travel at {x}, {y}")
    report["checks"].append("18 proxy keycap swept volumes clear printed parts over 4 mm travel")
    report["keycap_travel_reference"] = {
        "travel_mm": KEYCAP_TRAVEL,
        "full_press_skirt_above_plate_mm": round(KEYCAP_REST_BOTTOM - KEYCAP_TRAVEL - LID_T, 3),
        "actual_switch_and_keycap_fit_unverified": True,
    }
    # Screw and display connector access are tested as complete cylinders/volumes.
    for point in TOP_SCREWS:
        probe = placed(cylinder(1.65, LID_T + 2, (*point, LID_T / 2)))
        if intersection(fixed["lid"], probe) > .04:
            raise ValueError("lid screw access blocked")
    board = box((*board_bounds(width, depth)[:2], BOARD_Z),
                (*board_bounds(width, depth)[2:], BOARD_Z + BOARD_T))
    if intersection(fixed["base"], board) > .04:
        raise ValueError("perfboard intersects supports")
    report["checks"].append("lid screw paths and undrilled perfboard clearance pass")
    toggle_body = placed(box((OPTION_X - 7.8, OPTION_Y - 14.8, LID_T - 32),
                             (OPTION_X + 7.8, OPTION_Y + 14.8, LID_T - .05)))
    for name in ("shell", "base", "lid"):
        if intersection(fixed[name], toggle_body) > .04:
            raise ValueError(f"left toggle body/terminal allowance intersects {name}")
    report["checks"].append("left toggle body plus 5 mm terminal allowance clears printed parts")
    for name, mesh, _ in components(width, depth):
        if any(label in name for label in ("perfboard", "RP2040", "socket", "USB", "header")):
            if intersection(mesh, toggle_body) > .04:
                raise ValueError(f"left toggle terminal allowance intersects {name}")
    for point in TOP_SCREWS:
        head = placed(cylinder(3.0, 2.5, (*point, LID_T + 1.25)))
        for name in ("lid", "toggle_cap"):
            if intersection(fixed[name], head) > .04:
                raise ValueError(f"{name} blocks lid screw head access at {point}")
    report["checks"].append("left toggle clears controller electronics; relocated display clears lid screw heads")
    report["physical_fit_required"] = [
        "actual switch clips and keycap full-travel clearance",
        "printed aperture and heat-set insert fit using coupons",
        "actual perfboard dimensions, copper isolation and MCU socket row spacing",
        "display connector labels, power voltage, header height and rear solder joints",
        "USB plug shell access, strain relief, open-lid harness reach",
    ]
    return report


def assembly_parts(parts, width, depth):
    items = [("shell", parts["shell"], "#333a43"),
             ("lid", placed(parts["lid"]), "#555f6c"),
             ("base", parts["base"], "#2a3038"),
             ("toggle_cap", placed(moved(parts["toggle_cap"], (OPTION_X, OPTION_Y, LID_T))), "#343d48")]
    for i, (x, y) in enumerate(clamp_centers(width, depth)):
        clamp = parts["board_clamp"].copy()
        if x > BOARD_CENTER_X:
            clamp.apply_transform(trimesh.transformations.rotation_matrix(np.pi, (0, 0, 1)))
        items.append((f"board clamp {i+1}", moved(clamp, (x, y, BOARD_Z + BOARD_T + .2)), "#77818c"))
    return items


def render(items, target, view="iso"):
    """Deterministic orthographic CPU render; no display/GPU dependency."""
    rotations = {
        "iso": np.array([[.83, .55, 0], [-.30, .45, .84], [.462, -.697, .55]]),
        "top": np.array([[1, 0, 0], [0, 1, 0], [0, 0, 1]]),
        "under": np.array([[1, 0, 0], [0, -.65, -.76], [0, .76, -.65]]),
    }
    rotation = rotations[view]
    triangles, colors = [], []
    for _, mesh, color in items:
        triangles.extend(mesh.triangles @ rotation.T)
        rgb = [int(color[i:i+2], 16) for i in (1, 3, 5)]
        colors.extend([rgb] * len(mesh.faces))
    triangles, colors = np.asarray(triangles), np.asarray(colors)
    lower, upper = triangles[:, :, :2].reshape(-1, 2).min(axis=0), triangles[:, :, :2].reshape(-1, 2).max(axis=0)
    canvas = np.array([1600, 1200])
    scale = min((canvas - 160) / (upper - lower))
    points = (triangles[:, :, :2] - lower) * scale + (canvas - (upper - lower) * scale) / 2
    points[:, :, 1] = canvas[1] - points[:, :, 1]
    normals = np.cross(triangles[:, 1] - triangles[:, 0], triangles[:, 2] - triangles[:, 0])
    normals /= np.maximum(np.linalg.norm(normals, axis=1, keepdims=True), 1e-9)
    shade = np.clip(.68 + .32 * np.abs(normals @ np.array([.25, -.35, .90])), 0, 1)
    image = Image.new("RGB", tuple(canvas), "#edf0f2")
    pixels = np.asarray(image).copy()
    depth_buffer = np.full((canvas[1], canvas[0]), -np.inf, dtype=np.float32)
    # A depth buffer is essential for openings and exploded parts; sorting
    # whole triangles by average depth incorrectly paints internal faces on top.
    for i in range(len(triangles)):
        if normals[i, 2] <= 1e-8:
            continue
        p = points[i]
        lo = np.maximum(np.floor(p.min(axis=0)).astype(int), 0)
        hi = np.minimum(np.ceil(p.max(axis=0)).astype(int), canvas - 1)
        if np.any(hi < lo):
            continue
        xx, yy = np.meshgrid(np.arange(lo[0], hi[0] + 1) + .5,
                             np.arange(lo[1], hi[1] + 1) + .5)
        a, b, c = p
        denominator = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1])
        if abs(denominator) < 1e-10:
            continue
        wa = ((b[1] - c[1]) * (xx - c[0]) + (c[0] - b[0]) * (yy - c[1])) / denominator
        wb = ((c[1] - a[1]) * (xx - c[0]) + (a[0] - c[0]) * (yy - c[1])) / denominator
        wc = 1 - wa - wb
        zz = wa * triangles[i, 0, 2] + wb * triangles[i, 1, 2] + wc * triangles[i, 2, 2]
        region = np.s_[lo[1]:hi[1]+1, lo[0]:hi[0]+1]
        mask = (wa >= -1e-6) & (wb >= -1e-6) & (wc >= -1e-6) & (zz > depth_buffer[region])
        depth_buffer[region][mask] = zz[mask]
        pixels[region][mask] = (colors[i] * shade[i]).astype(np.uint8)
    image = Image.fromarray(pixels)
    target.parent.mkdir(parents=True, exist_ok=True)
    image.save(target)


def export_scene(items, target):
    scene = trimesh.Scene()
    for index, (name, mesh, color) in enumerate(items):
        geometry = mesh.copy()
        # glTF/GLB uses metres; printable STLs and design coordinates use mm.
        geometry.apply_scale(.001)
        geometry.units = "m"
        geometry.visual.face_colors = trimesh.visual.color.hex_to_rgba(color)
        scene.add_geometry(geometry, node_name=name, geom_name=f"part-{index}")
    target.write_bytes(scene.export(file_type="glb"))


def export_viewer(items, target):
    payload = []
    for name, mesh, color in items:
        payload.append({"name": name, "color": color,
                        "v": base64.b64encode(np.asarray(mesh.vertices, dtype="<f4").tobytes()).decode(),
                        "f": base64.b64encode(np.asarray(mesh.faces, dtype="<u4").tobytes()).decode()})
    packed = base64.b64encode(zlib.compress(json.dumps(payload).encode(), 9)).decode()
    template = Path(__file__).with_name("workbench_viewer.html").read_text()
    target.write_text(template.replace("__MODEL_DATA__", packed))


def write_wiring(target):
    with target.open("w", newline="") as stream:
        writer = csv.writer(stream)
        writer.writerow(["connector", "contact", "key", "switch_contact", "RP2040_net", "view"])
        for row in range(3):
            for col in range(6):
                key = row * 6 + col + 1
                for contact, net, switch_side in ((col * 2 + 1, f"GPIO{key}", "A"),
                                                  (col * 2 + 2, "GND", "B")):
                    writer.writerow([f"J{row+1}", contact, f"K{key}", switch_side, net,
                                     "hub mating face; marked arrow is 1; odd row=signals"])
            writer.writerow([f"J{row+1}", 13, "", "", "NC / pin removed AND matching cavity blocked", "index end"])
            writer.writerow([f"J{row+1}", 14, "", "", "NC / guide pin present", "index end"])


def write_matrix_wiring(target):
    with target.open("w", newline="") as stream:
        writer = csv.writer(stream)
        writer.writerow(["key", "connector", "signal_contact", "return_contact",
                         "column", "S3_column_GPIO", "diode", "stripe_to_row", "S3_row_GPIO"])
        for row, gpio in enumerate((1, 2, 4)):
            for col in range(6):
                key = row * 6 + col + 1
                writer.writerow([f"K{key}", f"J{row+1}", col * 2 + 1, col * 2 + 2,
                                 f"C{col+1}", col + 5, f"D{key}", f"R{row+1}", gpio])


def render_wiring(target):
    """Assembly schematic, not an inferred pin map of the purchased module."""
    image = Image.new("RGB", (1600, 1120), "#f4f5f3")
    draw = ImageDraw.Draw(image)
    font_path = next((str(path) for path in map(Path, [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Medium.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ]) if path.exists()), None)
    def text(x, y, content, size=24, color="#293540"):
        font = ImageFont.truetype(font_path, size) if font_path else ImageFont.load_default(size=size)
        draw.text((x, y), content, fill=color, font=font)
    def line(points, color="#697989", width=5):
        draw.line(points, fill=color, width=width, joint="curve")
    def panel(rect, title):
        draw.rounded_rectangle(rect, radius=18, fill="white", outline="#dde2df", width=2)
        text(rect[0]+26, rect[1]+18, title, 28)
    text(42, 28, "接线的关键：每颗轴独立回线，软线留余量并固定线皮", 34)
    panel((35, 100, 975, 635), "RP2040 直连：18 颗同理，图示其中 3 颗")
    panel((1000, 100, 1565, 635), "单颗轴：局部软线余量")
    panel((35, 660, 800, 1085), "每排一个 2×7 插头 · 看洞洞板插合面")
    panel((825, 660, 1565, 1085), "S3 矩阵升级 · 二极管固定在洞洞板")
    draw.rounded_rectangle((470, 190, 920, 568), radius=16, fill="#eadfcf")
    text(500, 205, "洞洞板", 25)
    text(500, 245, "固定的 GND 汇流排", 23)
    line([(600, 294), (600, 523)], "#39454e", 12)
    for i in range(3):
        yy = 275 + i * 100
        draw.rounded_rectangle((80, yy-15, 220, yy+57), radius=8, fill="#e9edef", outline="#8a969e", width=2)
        text(112, yy-5, f"K{i+1}", 25)
        line([(220, yy), (410, yy), (410, yy-5), (790, yy-5)], "#d58d48", 5)
        line([(220, yy+36), (380, yy+36), (380, yy+46), (600, yy+46)], "#39454e", 5)
        draw.ellipse((593, yy+39, 607, yy+53), fill="#39454e")
        text(800, yy-20, f"GPIO{i+1}", 22)
    line([(600, 523), (825, 523)], "#39454e", 7)
    text(835, 501, "GND", 22)
    text(80, 575, "轴与轴之间没有短地线；公共连接只在固定板上发生。", 23)
    draw.rounded_rectangle((1055, 205, 1220, 310), radius=10, fill="#cdd5d9")
    text(1097, 223, "轴壳", 25)
    line([(1080, 310), (1080, 402)], "#d58d48", 6)
    line([(1190, 310), (1190, 393)], "#39454e", 6)
    draw.arc((1080, 310, 1320, 495), 0, 180, fill="#d58d48", width=6)
    draw.arc((1190, 310, 1360, 475), 0, 180, fill="#39454e", width=6)
    line([(1320, 402), (1320, 350), (1505, 350)], "#d58d48", 6)
    line([(1360, 392), (1360, 385), (1505, 385)], "#39454e", 6)
    draw.rounded_rectangle((1410, 331, 1445, 403), radius=7, fill="#83a8a0")
    text(1380, 425, "松扎带", 22)
    text(1040, 514, "局部多留 25–35 mm 软线", 23)
    text(1040, 553, "整排线束另多留 80–120 mm", 23)
    text(1040, 590, "先断电、拔插头，再抬盖。", 22)
    for col in range(7):
        xx = 105 + col * 94
        for row in range(2):
            yy = 790 + row * 66
            number = col * 2 + row + 1
            draw.rectangle((xx-14, yy-14, xx+14, yy+14), fill="#38434d" if number < 13 else "#d9dddf")
            text(xx-13, yy-48, str(number), 21)
        text(xx-20, 891, f"K{col+1}" if col < 6 else "定位", 21)
    text(76, 945, "上排：该排 6 颗键的信号 / 下排：各自 GND", 22)
    text(76, 984, "13 脚缺公针且封同号母孔；14 脚不接电。", 22)
    text(76, 1023, "J1=K1–6，J2=K7–12，J3=K13–18", 22)
    text(866, 738, "列输入", 23)
    text(1064, 738, "独立双线 / 轴", 23)
    text(1350, 738, "二极管 → 行", 23)
    line([(875, 837), (1070, 837)], "#b88044", 5)
    draw.ellipse((1062, 829, 1078, 845), outline="#38434d", width=3)
    draw.ellipse((1152, 829, 1168, 845), outline="#38434d", width=3)
    line([(1070, 825), (1143, 798)], "#38434d", 5)
    line([(1160, 837), (1280, 837)], "#39454e", 5)
    draw.polygon([(1280, 809), (1280, 865), (1330, 837)], fill="#83a8a0")
    line([(1330, 807), (1330, 867)], "#293540", 6)
    line([(1330, 837), (1500, 837)], "#83a8a0", 5)
    text(1347, 886, "色环朝行线", 23)
    text(866, 945, "3 行 + 6 列 + 2 屏幕 + 5 控制 = 16 GPIO", 22)
    text(866, 984, "矩阵版返回端不再接 GND；需更换底板和配置。", 22)
    text(866, 1023, "18 颗直插 1N4148，无须定制 PCB。", 22)
    image.save(target)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-root", type=Path, default=Path("output/modeling/workbench-serviceable"))
    parser.add_argument("--perfboard-width", type=float, default=80)
    parser.add_argument("--perfboard-depth", type=float, default=60)
    args = parser.parse_args()
    if not 70 <= args.perfboard_width <= 88 or not 58 <= args.perfboard_depth <= 64:
        parser.error("supported perfboard envelope: width 70..88, depth 58..64 mm")
    out = args.output_root
    out.mkdir(parents=True, exist_ok=True)
    width, depth = args.perfboard_width, args.perfboard_depth
    parts = {"shell": build_shell(), "lid": build_lid(), "base": build_base(width, depth),
             "blank": build_option(),
             "toggle_cap": build_option(True), "board_clamp": build_board_clamp(),
             "switch_coupon": build_coupon()}
    assembled = assembly_parts(parts, width, depth)
    report = validate(parts, assembled, width, depth)
    for name, mesh in parts.items():
        # The shell is translated only for printing. Its assembled origin is BASE_T.
        printable = moved(mesh, (0, 0, -mesh.bounds[0, 2]))
        path = out / f"workbench_{name}.stl"
        old.export(printable, path)
        report["parts"][name]["sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
    refs = components(width, depth)
    render(assembled + refs, out / "assembled.png")
    render(assembled + refs, out / "top.png", "top")
    opened = [item for item in assembled if item[0] == "base" or item[0].startswith("board clamp")]
    render(opened + [item for item in refs if any(word in item[0] for word in
           ("perfboard", "Perfboard", "RP2040", "socket", "USB", "header", "loop"))], out / "wiring-bay.png")
    underside = [("lid", placed(parts["lid"]), "#555f6c")]
    render(underside + [item for item in refs if "body" in item[0] or "loop" in item[0]],
           out / "lid-underside.png", "under")
    exploded = []
    for name, mesh, color in assembled + refs:
        offset = (0, 0, 0)
        if name == "lid" or name == "toggle_cap" or name.startswith("K") or any(
            word in name for word in ("Display", "OLED", "Encoder", "Module", "Toggle")):
            offset = (0, 0, 40)
        if name == "base" or name.startswith("board clamp") or any(
            word in name for word in ("perfboard", "Perfboard", "RP2040", "socket", "USB", "header")):
            offset = (0, 0, -24)
        if "loop" not in name:
            exploded.append((name, moved(mesh, offset), color))
    render(exploded, out / "exploded.png")
    export_scene(assembled + refs, out / "workbench_assembly.glb")
    export_scene(exploded, out / "workbench_exploded.glb")
    export_viewer(assembled + refs, out / "workbench-3d.html")
    write_wiring(out / "rp2040-harness.csv")
    write_matrix_wiring(out / "s3-matrix-harness-draft.csv")
    wiring_files=board_wiring.generate(out,announce=False) if width==80 and depth==60 else []
    render_wiring(out / "wiring-topology.png")
    (out / "verification.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    document = Path(__file__).resolve().parents[2] / "docs/3d-print/workbench-serviceable.md"
    (out / "README.md").write_text(document.read_text())
    filenames = [*(f"workbench_{name}.stl" for name in parts),
                 "workbench_assembly.glb", "workbench_exploded.glb", "workbench-3d.html",
                 "assembled.png", "exploded.png", "top.png", "wiring-bay.png",
                 "lid-underside.png", "wiring-topology.png", "rp2040-harness.csv",
                 "s3-matrix-harness-draft.csv", "verification.json", "README.md",*wiring_files]
    with zipfile.ZipFile(out / "workbench-serviceable.zip", "w", zipfile.ZIP_DEFLATED) as package:
        for filename in filenames:
            package.write(out / filename, filename)
    # These were generated by the previous design and are no longer printable parts.
    for filename in ("workbench_retainer.stl", "workbench_capture_coupon.stl"):
        (out / filename).unlink(missing_ok=True)
    print(json.dumps({"output": str(out.resolve()), "checks": report["checks"],
                      "print_designs": len(parts), "assembly_print_quantity": 8,
                      "fit_coupons": 1}, ensure_ascii=False))


if __name__ == "__main__":
    main()
