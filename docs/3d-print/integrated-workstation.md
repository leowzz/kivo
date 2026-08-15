# Kivo Integrated Workstation

This model combines an 18-key mechanical keypad, a display PCB, either
supported controller board, and a separately printed telephone handset base
into a compact desktop workstation.

All dimensions are millimeters.

## Printed Parts

- `kivo_integrated_workstation_shell.stl`: open-top `138 x 104` chassis,
  30-degree panel support lip, and five continuous panel-to-bottom attachment
  pillars, plus two solid support walls below the 18-key field. Both high rear
  side-wall corners end in flat caps instead of sharp points.
- `kivo_integrated_workstation_sloped_panel.stl`: removable `132 x 117` key and
  display panel with a raised horizontal two-switch pod and six recessed
  fly-wire clips. It has no underside protrusions and prints flat.
- `kivo_integrated_workstation_bottom_cover.stl`: removable `138 x 104` bottom
  cover with five countersunk attachment holes, a two-level slide-in controller
  cradle, and ventilation slots.
- `telephone_handset_switch_base_workstation_mount.stl`: independent handset
  base with two blind M3 heat-set insert pockets cut directly into its existing
  right wall. They align with two round holes at the same height in the chassis
  left wall, keeping the handset base horizontal. A `12 mm` cable passage runs
  directly through both mating walls between the screws.

The generator is `scripts/modeling/integrated_workstation.py`. Run it from the
repository root with:

```bash
uv run --script scripts/modeling/integrated_workstation.py
```

## Reference Dimensions

| Item | Designed interface |
|---|---:|
| Heat-set insert supplied | `3.9` body, `4.9` knurled rings, `4.9` long |
| Heat-set insert pocket | `4.8 x 5.4` blind bore, `5.1 x 0.6` lead-in |
| M3 countersunk screw | `2.9` thread, `5.3` measured head diameter |
| Printed countersink | `3.4` through hole, `5.6 x 0.5` straight recess, then `90 degrees` |
| Bottom cover | `138 x 104`, matching the controller chassis footprint |
| Bottom cover attachment | Five countersunk M3 screws into the shared pillars |
| Handset chassis holes | Two `3.4` round clearances at `y=47.2/90.0`, `z=4.6` |
| Handset base inserts | Two `4.8 x 5.4` blind bores with `5.1 x 0.6` lead-ins |
| Handset cable passage | One aligned `12.0` round opening at `y=76.5`, `z=9.6` |
| Handset base installed bottom | `z=-2.4`, aligned with the bottom-cover underside |
| Handset base installed rear | `y=108`, flush with the chassis rear for cable access |
| Key switches | `14.8` lower relief, `14.0` upper lip |
| Key layout | `6 x 3`, `19.05` pitch |
| Key plane | `30 degrees` above horizontal |
| Rear side-wall caps | Two horizontal `4.0`-long flattened ends |
| Key-field support walls | Two longitudinal `2.8` solid walls at `x=123.45/161.55` |
| Support-wall extent | Local `y=4.0` to `62.625`, from chassis bottom to sloped panel seat |
| Central wiring corridor | `35.3` clear width between the solid support walls |
| Fly-wire retention | Six clips, one per three-key group |
| Wire clip pocket | `16.0 x 3.0 x 2.2`, with a `1.5` snap-in mouth |
| Sloped panel attachment | Five `3.4` clearances into hidden chassis inserts |
| Shared attachment pillars | Five support-free columns with independent blind insert pockets at both ends |
| Display PCB | `64.90 x 35.03`, four backside-loaded heat-set insert holes |
| Display attachment | Four `4.8` through bores with `5.1 x 0.6` backside lead-ins |
| Display header | 8 pins on PCB left half, first pin at `x=11.38`, `2.54` pitch |
| Display cable slot | `24.5 x 6.5`, aligned below the left-side header |
| Display bezel position | Flush with the panel's left edge |
| Toggle switch mounting plane | Horizontal, parallel to the workstation bottom |
| Toggle switch openings | Two vertical `12.0` through-holes in one row |
| Toggle switch body envelope | `15.0 x 29.0 x 27.0` |
| Toggle switch body cavity | One shared `39.6 x 29.6` opening from the panel underside |
| Toggle switch center pitch | `24.0` horizontally, centered on the pod |
| Toggle pod maximum roof bridge | `29.6` with no internal divider |
| Controller cradle | `28.64 x 57.65` maximum clear area |
| RP2040 reference | `22.86 x 53.34` |
| ESP32-S3 retained inner board | `27.94 x 57.15` |
| RP2040 support level | PCB underside `3.0` above the cover inner face |
| RP2040 locating slot | `23.0` clear width, `2.4` walls rising `2.5` above support |
| RP2040 top clip | `10.0` wide with `1.0` PCB overlap |
| ESP32-S3 support level | PCB underside `6.5` above the cover inner face |
| ESP32-S3 retaining lips | `12.0` long, `1.8` stem, `0.8` PCB overlap |
| ESP32-S3 retained length | `55.15` from the front stop to the USB-end relief |
| Controller USB opening | Rear wall, `37.0 x 9.5` |
| Controller USB edge | Flush with the rear wall inner face |

The display PCB sits on the removable panel in a rounded locating bezel with
`0.65` clearance on each edge. The bezel rises only `2.0` above the 30-degree
key plane. Four M3 heat-set inserts are installed from the panel's flat back
side; their `4.8` bores pass through the full `5.4` mounting thickness, with a
`5.1 x 0.6` lead-in on the back. The dimensioned reference places its eight-pin
header on the left half of the board, so a hidden `24.5 x 6.5` slot is aligned
below that header rather than centered beneath the PCB.

The 18 switches are intended for direct fly-wiring rather than a hot-swap PCB.
Each key row has a left and right recessed wire clip, so every three switches
share one clip. Feed the thin wires into the `1.5` opening one at a time; the
opening expands behind two internal lips into a `3.0` pocket that retains the
bundle. The clips stop `1.2` short of the visible panel face.

The chassis supports key presses with two narrow longitudinal walls placed in
column gaps, not a closed internal deck. Each wall is a single solid wedge from
the chassis bottom plane to the 30-degree panel seat and bonds directly into the
front lip. There are no separate rails, layers, or intermittent support ribs.
The `35.3`-wide center corridor remains open for wiring, and the removable
bottom cover stays a separate part.

The toggle-switch area rises from the lower edge of the 30-degree panel into a
horizontal mounting plane. Its two hole axes are vertical in the assembled
workstation, so the `27.0`-deep switch bodies hang straight down instead of
leaning into the rear wall. Both bodies share one continuous underside
cavity; there are no walls between neighboring switches. The outer cavity
boundary keeps `0.3` clearance around the combined body envelope. The complete
body envelopes have been checked against the rear wall, right panel rail, and
attachment bosses.

The five sloped-panel insert bosses continue to the chassis bottom as continuous
pillars. Each pillar has one blind insert pocket at the sloped end and a second
blind pocket at the bottom end, so the panel and cover can be removed
independently while reusing the same printed structure. The unused screen-side
rear pillar and both of its attachment holes are omitted.

## Hardware And Assembly

- 16 M3 heat-set brass inserts with the measured `3.9` body, `4.9` knurled
  rings, and `4.9` length: 5 for the sloped-panel attachment, 4 for the display,
  5 for the bottom cover, and 2 for the handset base. Receiving parts use a
  `4.8 x 5.4` blind bore with a shallow `5.1` lead-in; the four display bores
  pass through the panel and open from its back side.
- The user-supplied screws measure `2.9` across the thread and `5.3` across
  the head. Exterior attachment holes use a `3.4` clearance bore and a
  `5.6 x 0.5` straight recess followed by a 90-degree countersink so the screw
  heads sit 0.5 mm deeper.
- 5 M3 x 8 screws for the bottom cover. They pass through the outside cover
  and enter inserts installed from the hidden underside of the chassis.
- 5 M3 x 10 screws for the removable sloped panel. They pass through the
  panel face into inserts installed in the chassis support bosses. The insert
  openings are covered completely by the assembled panel.
- 4 M3 x 10 screws for the display PCB.
- 2 M3 screws and 2 washers for the handset base. The washers sit against the
  chassis interior so both screw loads are spread over the side wall.
- 2 panel-mount toggle switches with `12 mm` threaded bushings and
  `15 x 29 x 27 mm` bodies. Install them from the panel underside through the
  shared rectangular cavity. Their mounting axes are vertical after assembly.
- The RP2040 and ESP32-S3 reference boards have GPIO solder holes but no
  dedicated mechanical mounting holes. The RP2040 uses a `23.0 mm` U-shaped
  locating slot with one wide top clip; the ESP32-S3 uses the outer support
  level with fixed horizontal retaining lips.

1. Heat two inserts directly into the handset base's right wall. Hold the base
   horizontal with its bottom aligned to the installed bottom cover, then pass
   two M3 screws and washers through the round chassis holes into the inserts.
   Feed the handset cable through the aligned `12 mm` center passage, tighten
   both screws, and keep both washers on the chassis interior.
2. Place the controller USB end toward the wide rear connector opening and
   slide the board lengthwise into its slot until it reaches the front stop. The
   PCB's USB edge then sits directly against the rear wall inner face so its
   Type-C connector reaches the outside of the chassis without a recessed gap. The
   RP2040 runs inside the `23.0 mm` lower U-slot and under its single top clip;
   the larger ESP32-S3 uses the outer upper level and horizontal side lips.
   Slide either board back out from the rear; the retainers are fixed and should
   not be bent.
3. Heat four inserts into the display holes from the panel's flat back side
   until flush. Install all 18 key switches and the two toggle switches, then
   fasten the display PCB from the front with four M3 x 10 screws. Fly-wire each
   three-key group and press its wires individually into the nearest recessed
   clip before attaching the panel to the chassis.
4. Heat five inserts into the chassis panel bosses from the sloped mating face.
   Place the completed panel on the support lip and drive five M3 x 10 screws
   through its face into the hidden inserts.
5. After testing the keypad and display, heat five inserts into the bottom ends
   of the shared chassis pillars and fasten the bottom cover from outside.
   Future key or display changes require reprinting only the panel.

## Printing

- Print the shell upright with its bottom rim on the build plate. All five shared
  attachment pillars rise continuously from the build plate; their outer walls
  stay within 45 degrees, so the sloped insert bosses do not need support.
  The two solid key-field walls also start on the build plate and run
  continuously to the sloped panel seat. Leave the center wiring corridor free
  of support. The two rear side-wall tips are truncated into horizontal 4 mm
  caps, so there are no sharp peaks at the shell's highest corners.
- Print the handset base upright in its modeled orientation. It has no external
  attachment protrusions. The two horizontal blind insert bores may print
  slightly flat at the top; clear them gently with a `4.8 mm` drill before
  heat-setting the inserts if needed. The horizontal `12 mm` cable opening may
  also print slightly flat at its top; deburr it after printing if needed.
- Print the sloped panel separately with its large flat underside on the build
  plate. The switch steps and 2 mm screen bezel face upward and need no support.
  The raised toggle pod has one uninterrupted underside cavity. With no internal
  divider, its longest roof span is `29.6 mm`; add build-plate-only
  support inside this cavity if the printer cannot bridge that distance. The
  support remains accessible and removable from the open panel underside.
  The six wire clips are recessed into that underside: their internal walls
  open at 45 degrees and their `3.0` pocket roofs are short bridges, so do not
  add support inside them. Use elephant-foot compensation if the `1.5` mouths
  print too tight.
- Print the bottom cover flat with the controller rails facing upward.
- The RP2040 slot uses continuous `2.4 mm` walls and one `10 mm` top clip. The
  ESP32-S3 retainers use `1.8 mm` fixed stems and `12 mm` horizontal lips.
  Their short overhangs print without support; slide controllers in from the rear.
- Start with a 0.4 nozzle, 0.2 layer height, 4 perimeters, and 20 percent
  infill.
- The main shell and cover both remain `138 mm` wide; there is no separate
  hanger rail protruding from the shell. The two horizontal `3.4 mm` round holes
  normally bridge without support; clear them with a `3.4 mm` drill if their top
  surfaces print slightly flat.
- The chassis has no broad sloped roof, so it does not need support beneath the
  key deck. The removable panel rests on two side rails, a front lip, five shared
  pillars, and the two solid key-field walls. Route wiring through the open
  center corridor between the walls.
- The remaining rear panel screw is moved 3 mm toward the front and the side
  rails stop at local `y=117`, keeping every panel support at or in front of
  the chassis rear wall (`y=108` in assembled coordinates).
- The removable panel's rear edge is shortened by 1 mm and the screen is moved
  forward by the same amount. In the assembled model its rear bound is
  `y=107.923`, so the panel also remains inside the `y=108` rear wall.

The generator rejects any result that is not a single positive, watertight,
consistently wound two-manifold mesh. It measures both levels of all 18 switch
apertures directly on the flat-print panel and verifies the five panel screw
paths, five bottom-cover screw paths, five continuous support-free pillars, two
continuous solid key-field support walls,
switch-body and wiring clearance, two blind handset insert bores, two same-height
chassis holes, the aligned `12 mm` handset cable passage, bottom-cover height
compensation, inside washer clearance, and assembly clearance.
