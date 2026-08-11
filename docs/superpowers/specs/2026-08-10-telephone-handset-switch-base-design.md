# Telephone Handset Switch Base Design

Date: 2026-08-10
Revised: 2026-08-11
Status: Approved

## Goal

Create a one-piece FDM-printable base that holds the center of a telephone
handset and fully bottoms out one mechanical keyboard switch when the handset
is placed in the base. A wide funnel mouth guides insertion into an exact
`55 x 70` locating section. At full switch travel, the handset's adjacent
bearing surface reaches all four corner pads so the normal resting load is
shared and the former single-point rocking is removed.

All dimensions in this document are millimeters.

## Canonical Switch Geometry

Use only this original upper-cover STL as the switch-aperture source:

```text
models/3d-print/3x3keypad/pico_macro_pad_top.stl.stl
```

Its required SHA-256 is:

```text
ce0f7b64d06b3fc2864d29452e87fb264f70567c0f5924eab380d0748f4e9155
```

Do not derive the switch aperture from any generated 3x4, 4x3, 4x4, or 5x4
variant. Do not scale the source mesh.

The source switch plate has this measured stepped cross-section:

| Region from plate underside | Square opening | Height |
|---|---:|---:|
| Lower clip relief | `14.798 x 14.798` | `1.998` |
| Upper retaining lip | `14.000 x 14.000` | `1.402` |
| Total plate | | `3.400` |

The upper `1.402` region is the effective thickness captured by the switch
clips. The full `3.400` plate thickness is structural and must not be treated
as the clip thickness.

Normalize the source STL using the existing
`scripts/macro_pad_variants.py::load_source()` transform. The exact central
switch cell occupies `X=23.05..42.10` and `Y=23.05..42.10` in that normalized
mesh. Preserve its local geometry exactly inside a centered `19.05 x 19.05`
protected region.

## Coordinate System

- `X` is the handset-pocket width.
- `Y` is the handset-pocket length.
- The rear wire-exit edge is positive `Y`.
- `Z=0` is the build-plate and table-contact plane.
- Positive `Z` points upward toward the handset.

The complete model has nominal bounds:

```text
X: 0.0 .. 63.8
Y: 0.0 .. 78.8
Z: 0.0 .. 28.4
```

## Handset Pocket

The locating pocket has exact `55 x 70` wall-to-wall section extents from the
`Z=13.4` support datum through `Z=24.4`. These are internal dimensions, not
outer dimensions. The `R1.6` lower inner corners round the four section
corners, so `55 x 70` describes the section bounding box rather than a
sharp-cornered rectangular prism.

The upper `4.0` of pocket depth is a straight-sided funnel:

- funnel bottom at `Z=24.4`: `55 x 70`, `R1.6`;
- funnel mouth at `Z=28.4`: `59 x 74`, `R3.6`;
- widening per side: `2.0` over `4.0` vertical height;
- straight locating height below the funnel: `11.0`;
- minimum wall thickness at the mouth: `2.4`.

The outer footprint is `63.8 x 78.8` with vertical `R6.0` outer corners. The
wall is `2.4` thick at the funnel mouth and `4.4` thick around the lower
locating section. The safety-pad tops define the support datum at `Z=13.4`.
The rim top is `Z=28.4`, producing exactly `15.0` pocket depth. There is no
continuous `55 x 70` floor.

## Open-Bottom Structure

Use the selected open-bottom structure, not a removable cover or an enclosed
wire tunnel.

### Outer Ring

The perimeter wall continues from `Z=0` to the rim at `Z=28.4`. Its outer
profile is vertical while its inner profile follows the lower locating section
and upper funnel. It forms a closed outer ring while leaving the underside
interior open and accessible. All bottom edges are coplanar at `Z=0`.

### Central Platform And Tower

- Center a `24 x 24 x 3.4` switch platform in the pocket.
- Its underside is `Z=7.0`; its top is `Z=10.4`.
- Preserve the canonical source cell throughout the centered
  `19.05 x 19.05` protected region.
- Added material may exist only outside that protected region to reach the
  `24 x 24` platform footprint.
- Support the platform with a U-shaped tower from `Z=0` to `Z=7.0`.
- Tower outer footprint: `24 x 24`.
- Tower wall thickness: `2.4`.
- Omit the rear, positive-`Y` tower wall so the switch pins and wires remain
  accessible from below and behind.

The tower and the outer ring must touch the table plane together. The switch
load therefore transfers directly through the tower instead of bending a broad
unsupported floor.

### Rear Guide Ribs

Continue the tower's two side walls toward the inner face of the rear outer
wall as two straight guide ribs:

- rib thickness: `2.4`;
- rib height: `7.0`;
- clear space between ribs: `19.2`;
- rib bottoms: `Z=0`;
- rib tops: `Z=7.0`.

The ribs make the tower and outer ring one rigid body and keep the two flying
wires directed toward the rear exit. The channel remains open from below.

### Safety Pads

Place one `10 x 10` safety pad at each inner pocket corner. Each pad:

- has its top at `Z=13.4`;
- is `2.4` thick at the outer-wall attachment;
- uses a `45 degree` underside gusset back to the adjacent walls;
- does not project into the central switch protected region.

The pads are normal handset supports after the switch reaches full travel.
They prevent the handset from rocking around the center switch while keeping
the center trigger fully depressed.

## Switch Travel And Handset Height

Use this nominal dimension chain for the physical arrangement:

- switch body above mounting datum: `5`;
- unpressed trigger region above body: `4`;
- assumed switch full travel: `4`;
- handset center recess relative to its adjacent bearing surface: `2`.

The `4`-mm full-travel value assumes the stated `4`-mm trigger protrusion can be
fully consumed. At nominal full travel, the trigger top is `5` above the
platform. With the platform top at `Z=10.4`, the trigger contact is `Z=15.4`.
The handset center is recessed `2` above its adjacent bearing surface, placing
that bearing surface exactly at the `Z=13.4` corner-pad datum:

```text
10.4 mm platform + 5 mm bottomed trigger - 2 mm handset recess = 13.4 mm
```

No keycap or separate actuator extension is part of this design. Physical
acceptance must confirm that the switch reaches full travel at the same moment
the handset reaches the four pads. If the actual switch or handset recess
differs from the nominal `5`/`2` chain, adjust the platform height before
printing another final part; do not enlarge the `55 x 70` locating section.

## Rear Wire Exit

Cut one circular hole through the rear outer wall:

- diameter: `4.0`;
- axis: parallel to `Y`;
- horizontal center: `X=31.9`;
- vertical center: `Z=5.0`.

The hole carries two flying wires from the switch pins. Do not add a connector,
strain-relief clamp, or bottom-open cable notch.

## Assembly

1. Print the base upright with `Z=0` on the build plate.
2. Snap the switch into the stepped aperture from above.
3. Feed two wires through the rear `4.0` hole into the open underside.
4. Solder the wires to the two exposed switch pins from below.
5. Route both wires between the rear guide ribs.
6. Place the handset and verify the switch reaches full travel as all four
   safety pads begin carrying the normal resting load.

The open underside deliberately keeps the pins and solder joints serviceable.

## Printability

- Produce one connected printable component.
- Use the upright orientation described above.
- The outer ring, U-shaped tower, and guide ribs all start at `Z=0`.
- The safety pads use `45 degree` gussets.
- Do not create a broad roof over the open underside.
- Preserve the source switch aperture without support material inside it.
- The minimum `2.4` mouth-wall thickness aligns with six nominal `0.4`-mm
  extrusion widths; the lower locating wall is `4.4` thick.
- Nominal slicing context is a `0.4` nozzle and `0.2` layer height.

The design must not require a second printed part, screws, threaded inserts,
or slicer-generated support structures.

## Deliverables

Keep the implementation reproducible and isolated from Kivo runtime
dependencies:

```text
scripts/telephone_handset_switch_base.py
test/test_telephone_handset_switch_base.py
models/3d-print/telephone-handset-switch-base/
  telephone_handset_switch_base.stl
```

The generator should follow the repository's existing PEP 723
`trimesh`/`manifold3d` pattern. The generated STL remains ignored under
`models/3d-print`; the generator and tests are tracked. Generate previews only
under `/tmp/kivo-handset-switch-base-previews`.

Do not generate G-code or a new 3MF project.

## Automated Verification

The implementation must verify all of the following before exporting:

- canonical source hash matches;
- one connected, positive-volume, consistently wound, watertight two-manifold
  output mesh;
- exact `63.8 x 78.8 x 28.4` outer extents;
- exact `55 x 70` locating section through `Z=24.4`;
- exact `4.0`-high funnel ending at a `59 x 74` mouth;
- exact `15.0` pocket depth;
- switch protected region matches the normalized source cell;
- lower `14.798 x 14.798` relief and upper `14.000 x 14.000` aperture remain
  centered and retain their measured heights;
- central platform, U-tower, guide-rib, safety-pad, and rear-hole dimensions;
- open underside access to the switch-pin region;
- no broad floor or roof bridges the open underside;
- rear wire hole reaches the guide channel and exterior;
- no degenerate triangles, boundary edges, or non-two-manifold edges;
- deterministic binary STL output for the pinned toolchain.

Render nonblank isometric, top, side-section, and bottom previews. Inspect the
previews for blocked switch openings, accidental floors, disconnected ribs,
missing wire access, and incoherent overlaps.

## Physical Acceptance Boundary

Mesh validation is not physical acceptance. Completion reporting must keep
these items separate:

- slicer import and repair status;
- support-free toolpath inspection;
- printed switch snap-fit;
- full switch travel under the actual handset;
- simultaneous full switch travel and four-pad contact;
- rear wire-hole fit;
- base stability on a flat surface;
- sustained-load behavior of the selected switch and filament.

No slicer is currently installed in the workspace environment. Until a part is
sliced and printed, all physical checks remain Not Run.

## Non-Goals

- A bottom cover or enclosed electronics compartment.
- Fasteners, magnets, rubber feet, or adhesive pockets.
- A connector or cable strain relief.
- A keycap or actuator extension.
- Decorative text, branding, or surface ornament.
- Editing the canonical source STL or generated keypad variants.
- Claiming physical fit from mesh checks alone.
