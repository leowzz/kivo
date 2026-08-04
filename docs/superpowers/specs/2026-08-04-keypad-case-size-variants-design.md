# Keypad Case Size Variants Design

Date: 2026-08-04
Status: Approved

## Goal

Generate watertight top and bottom STL pairs for 3x4, 4x4, and 5x4
macro-pad layouts from the existing 3x3 YD-RP2040 case. Preserve the source
switch geometry, Type-C opening, controller clearance, wall thicknesses,
fastener geometry, and top-to-bottom fit instead of scaling the source meshes.

The source files are:

- `models/3d-print/3x3keypad/pico_macro_pad_top.stl.stl`
- `models/3d-print/3x3keypad/pico_macro_pad_bottom_fitted_to_usb_c.stl.stl`

The adjacent 3MF is reference material only. The two exported STL files are the
canonical input geometry.

## Orientation And Layout Notation

The Type-C edge is the top of every layout. In `columns x rows`, the first
number is the number of columns along the Type-C edge and the second number is
the number of rows extending away from it.

Use a normalized XY frame with the Type-C edge fixed at the top:

- increasing X moves from left to right;
- increasing Y moves down, away from the Type-C edge;
- increasing Z follows the existing STL height direction.

The original key pitch is `19.05 mm`. The source enclosure is nominally
`65.15 x 65.15 mm`, following this dimension rule:

```text
width  = 8.00 + columns * 19.05 mm
height = 8.00 + rows    * 19.05 mm
```

The target footprints and growth relative to the 3x3 source are:

| Layout | Footprint | Left growth | Right growth | Bottom growth |
|---|---:|---:|---:|---:|
| 3x4 | `65.15 x 84.20 mm` | `0` | `0` | `19.05 mm` |
| 4x4 | `84.20 x 84.20 mm` | `9.525 mm` | `9.525 mm` | `19.05 mm` |
| 5x4 | `103.25 x 84.20 mm` | `19.05 mm` | `19.05 mm` | `19.05 mm` |

The Type-C edge and controller feature group stay fixed. Width is added
symmetrically so the keys and controller remain centered as a group. Height is
added only at the bottom.

## Source Geometry Contract

The generated models retain these measured source dimensions:

- key pitch: `19.05 mm` in both directions;
- switch opening: `14.00 x 14.00 mm`;
- lower switch relief: `14.80 x 14.80 mm`;
- switch plate thickness: `3.40 mm`;
- nominal peripheral wall thickness: `4.00 mm`;
- bottom base skin: `1.12 mm` where no local support feature is present;
- bottom mating tongue: `1.60 mm` wide and `1.00 mm` high;
- top mating recess: `2.402 mm` wide and `1.998 mm` deep;
- screw bore: nominal `2.95 mm` diameter;
- bottom counterbore: nominal `5.60 mm` diameter;
- screw axis offset: `3.80 mm` from both adjacent outer edges.

The Type-C mouth, its lead-in, the RP2040 clearance pocket, and the nearby
support geometry form one protected feature group. Preserve that group exactly,
including the source Type-C center's approximately `0.5 mm` offset from the
enclosure centerline. Do not silently recenter or rescale it.

## Geometry Construction

Use mesh strip insertion rather than global scaling or a complete visual
reconstruction.

### Top

Cut only through feature-free planes at the boundaries of a complete
`19.05 mm` switch-cell band. Insert copies of the band to create the required
rows and columns, then translate the far perimeter sections by the inserted
distance.

This operation must preserve the key-hole and lower-relief cross-sections. Move
the four corner screw receivers to the new corners while keeping their size,
depth, and `3.80 mm` edge offsets. Extend the straight mating recess sections
without changing their cross-section or corner terminations.

### Bottom

Treat the Type-C/RP2040 feature group as rigid and keep it attached to the
fixed top edge. Do not duplicate or stretch the controller pocket, Type-C mouth,
lead-in, end stop, or their local supports.

Extend the base skin, empty internal cavity, side walls, and mating tongue into
the new left, right, and bottom regions. The added regions remain empty for
hand wiring. Do not introduce duplicated controller supports or ribs. Translate
the corner wall sections and four screw stacks to the new corners, preserving
their measured profiles and edge offsets.

The top recess and bottom tongue must be extended by the same distances so the
two generated halves retain their original clearance and alignment.

## Output

Generate binary STL files in millimeters with the source print orientation:

```text
models/3d-print/3x4/
  pico_macro_pad_3x4_top.stl
  pico_macro_pad_3x4_bottom_fitted_to_usb_c.stl

models/3d-print/4x4/
  pico_macro_pad_4x4_top.stl
  pico_macro_pad_4x4_bottom_fitted_to_usb_c.stl

models/3d-print/5x4/
  pico_macro_pad_5x4_top.stl
  pico_macro_pad_5x4_bottom_fitted_to_usb_c.stl
```

Do not generate 3MF projects, G-code, or unrelated model variants.

## Verification

Automated geometry validation must check every generated pair:

- exact target XY footprint and unchanged source Z extent;
- exactly `columns * rows` switch tunnels;
- `19.05 mm` center pitch in X and Y;
- unchanged switch-opening and relief dimensions;
- four aligned screw axes at `3.80 mm` from the new corners;
- unchanged screw-bore and counterbore diameters;
- unchanged Type-C mouth and controller-pocket cross-sections;
- unchanged wall, base-skin, tongue, and recess cross-sections;
- matching top and bottom footprint and mating-edge path;
- one connected component per STL;
- positive enclosed volume;
- no degenerate triangles, boundary edges, or non-manifold edges;
- consistent face orientation.

Create rendered previews from at least the top, bottom, and Type-C-side views to
confirm that no surfaces overlap, disappear, or bridge across a switch opening.

No slicer or CAD application is installed in the current environment. Slicer
repair checks, toolpath inspection, print tolerances, switch fit, screw fit,
controller fit, and physical top-to-bottom assembly are explicitly Not Run
until the generated files are opened in a slicer or printed.

## Implementation Boundary

Use isolated mesh tooling for STL parsing, robust Boolean operations, and
topology inspection. Do not add runtime dependencies to the Kivo application.
A generation helper may be kept only when it materially improves reproducibility;
otherwise the requested STL artifacts remain the only product files.

Preserve all unrelated and pre-existing untracked files under
`models/3d-print/3x3keypad`.

## Non-Goals

- Redesigning the switch spacing, key apertures, external styling, or corner
  radii.
- Moving or recentering the Type-C opening or RP2040 pocket.
- Adding a PCB, plate electronics, wiring channels, duplicated supports, or
  extra fasteners.
- Changing top-to-bottom clearances or print tolerances.
- Scaling the original STL meshes.
- Producing sliced printer output or claiming physical-fit acceptance.
