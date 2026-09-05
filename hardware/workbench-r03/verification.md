# Draft Verification - 2026-09-05

Toolchain: KiCad CLI and pcbnew Python API 10.0.6 on macOS. Sources are the
current r02 product YAML and integrated-workstation modeling constants.

| Check | Result |
| --- | --- |
| KiCad schematic load, SVG export and XML netlist export | Pass |
| Electrical Rules Check, including warnings | 0 violations |
| Exported schematic pins versus actual PCB pads | 214 connected pins, 53 nets match |
| 18 direct key inputs and bottom-side sockets | Pass |
| BOOT/QSPI chip select, SWD, USB, core supplies and J2 mapping | Pass |
| J4 expansion GPIO0/23/24/25/29 versus MCU pads | Pass; all five spare GPIOs, plus GND and 3.3 V |
| J5 BOOT / J6 RESET versus onboard buttons | Pass; BOOT through R7 to QSPI_SS, RESET to RUN |
| J4/J5/J6 holes and positions | Pass; plated through-hole, 2.54 mm pitch, 1.0 mm drills |
| Key pitch and CSV positions | 19.05 mm; all 18 placements match |
| Board size, thickness and copper count | 126 x 105 mm; 1.6 mm; 4 layers |
| Top and bottom 3D previews | Rendered and visually inspected; socket models present |
| Placement courtyard / silkscreen checks | No remaining violations of these types |
| PCB DRC | **Not passed: 4 hole-clearance errors and 164 unconnected items** |
| Routing | **Not started: 0 tracks and no copper zones** |
| Actual module voltage / physical connector order | **Not verified** |
| Enclosure fit / assembled hardware / USB impedance | **Not verified** |
| Manufacturing package | **Not produced** |

The four DRC hole-clearance errors are within the standard GCT USB4105
footprint: the NPTH locating holes are approximately 0.1944 mm from nearby
ground pads, below the draft project's 0.25 mm requirement. Resolve this by
checking the connector drawing and PCB manufacturer's supported geometry,
or by selecting another connector. The rule has not been weakened or excluded
to make the report pass.

The expansion-header change adds no DRC violations other than the six expected
additional unconnected items in this unrouted board. The four hole-clearance
errors are identical to the pre-change baseline. J4 starts at (10, 3) mm from
the board's rear-left corner; J5 starts at (97.5, 4) mm and J6 at (104, 4) mm.
All three run left to right in the component-side view with USB at the top.
Their pin order, electrical nets, through-hole attributes, drills and spacing
are checked by `scripts/hardware/verify_workbench.py`. Existing component
placements, board outline and project rules are preserved, apart from the
requested replacement and relocation of J4.

GPIO24/25 remain unavailable in the current YD-RP2040 firmware whitelist;
custom-board firmware support and connector/button clearance in the revised
enclosure still need validation.

The unconnected items are expected because this is a placement draft; they
must all be resolved before fabrication. ERC verifies electrical connectivity
rules, not analog correctness, component procurement, real-world USB behavior
or physical fit.

Generated reports and visual previews are under the ignored directory
`output/hardware/workbench-r03/`: `erc.json`, `drc.json`, `netlist.xml`,
`board-top.png` and `board-bottom.png`.

Re-run the relevant checks from the repository root:

```sh
/Applications/KiCad/KiCad.app/Contents/MacOS/kicad-cli sch erc \
  --exit-code-violations --format json -o /tmp/workbench-erc.json \
  hardware/workbench-r03/workbench-r03.kicad_sch

/Applications/KiCad/KiCad.app/Contents/MacOS/kicad-cli sch export netlist \
  --format kicadxml -o /tmp/workbench-netlist.xml \
  hardware/workbench-r03/workbench-r03.kicad_sch

/Applications/KiCad/KiCad.app/Contents/Frameworks/Python.framework/Versions/3.9/bin/python3.9 \
  scripts/hardware/verify_workbench.py hardware/workbench-r03 /tmp/workbench-netlist.xml

/Applications/KiCad/KiCad.app/Contents/MacOS/kicad-cli pcb drc \
  --exit-code-violations --format json -o /tmp/workbench-drc.json \
  hardware/workbench-r03/workbench-r03.kicad_pcb
```

The last command is expected to return a nonzero status for this draft. The
focused verification script also deliberately checks that this saved first
draft is unrouted; update that assertion when routing begins.
