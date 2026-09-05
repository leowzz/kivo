"""Run with KiCad's Python interpreter to create the UNROUTED review board.

The output path is explicit so regeneration cannot silently overwrite routing.
"""

import argparse
import json
from pathlib import Path

import pcbnew as pcb
import wx


def position(x, y):
    return pcb.VECTOR2I(pcb.FromMM(x), pcb.FromMM(y))


def generate(manifest, output, libraries):
    app = wx.App(False)
    data = json.loads(manifest.read_text())
    board = pcb.BOARD()
    board.SetCopperLayerCount(2)
    board.GetDesignSettings().SetBoardThickness(pcb.FromMM(1.6))
    board.GetTitleBlock().SetTitle("Kivo Workbench S3 r01 - SOCKETED YD ESP32-S3 - UNROUTED")
    net_names = sorted({net for part in data["parts"] for net in part["nets"].values() if net})
    nets = {}
    for name in net_names:
        net = pcb.NETINFO_ITEM(board, "/" + name)
        board.Add(net)
        nets[name] = net
    origin_x, origin_y = 50, 50

    def point(x, y):
        return position(origin_x+x, origin_y+y)

    def line(start, end, layer=pcb.Edge_Cuts, width=0.05):
        drawing = pcb.PCB_SHAPE(board)
        drawing.SetShape(pcb.SHAPE_T_SEGMENT)
        drawing.SetStart(point(*start))
        drawing.SetEnd(point(*end))
        drawing.SetLayer(layer)
        drawing.SetWidth(pcb.FromMM(width))
        board.Add(drawing)

    def text(value, x, y, layer=pcb.F_SilkS, size=1.0, angle=0):
        item = pcb.PCB_TEXT(board)
        item.SetText(value)
        item.SetPosition(point(x, y))
        item.SetTextSize(position(size, size))
        item.SetTextThickness(pcb.FromMM(0.15))
        item.SetTextAngle(pcb.EDA_ANGLE(angle, pcb.DEGREES_T))
        item.SetLayer(layer)
        board.Add(item)

    for part in data["parts"]:
        library, name = part["footprint"].split(":")
        path = manifest.parent / "Workbench.pretty" if library == "Workbench" else libraries / f"{library}.pretty"
        footprint = pcb.FootprintLoad(str(path), name)
        if footprint is None:
            raise ValueError(f"Cannot load {part['footprint']}")
        footprint.SetReference(part["ref"])
        footprint.SetValue(part["value"])
        footprint.SetPosition(point(*part["pcb"]))
        footprint.SetPath(pcb.KIID_PATH(f"/{data['sheet_uuid']}/{part['uuid']}"))
        footprint.SetSheetfile("workbench-s3-r01.kicad_sch")
        board.Add(footprint)
        if part["side"] == "B":
            footprint.Flip(footprint.GetPosition(), False)
        footprint.SetOrientationDegrees(part["angle"])
        footprint.SetDNP(part.get("dnp", False))
        footprint.Value().SetVisible(False)
        footprint.Reference().SetTextSize(position(0.8, 0.8))
        footprint.Reference().SetTextThickness(pcb.FromMM(0.12))
        if part["ref"].startswith(("C", "R")):
            footprint.Reference().SetVisible(False)
        if part["ref"] in ("J1", "J7", "J4", "J5", "J6"):
            footprint.Reference().SetVisible(False)
        for pad in footprint.Pads():
            number = pad.GetNumber()
            if not number:
                continue
            if number not in part["nets"]:
                raise ValueError(f"{part['ref']}: pad {number} not in schematic")
            name = part["nets"][number]
            if name:
                pad.SetNet(nets[name])

    width, height = data["width"], data["height"]
    for start, end in [((0,0),(width,0)), ((width,0),(width,height)), ((width,height),(0,height)), ((0,height),(0,0))]:
        line(start, end)
    for index, center in enumerate([(4,4), (4,73), (122,43), (4,131), (122.5,131)]):
        hole = pcb.FootprintLoad(str(manifest.parent / "Workbench.pretty"), "MountingHole_M3_5.6mm_Head")
        hole.SetReference(f"H{index+1}")
        hole.SetPosition(point(*center))
        hole.Value().SetVisible(False)
        hole.Reference().SetVisible(False)
        hole.SetBoardOnly(True)
        board.Add(hole)

    # Display envelope only: the module stays panel-mounted and uses a harness.
    for start, end in [((2,35),(62,35)), ((62,35),(62,70)), ((62,70),(2,70)), ((2,70),(2,35))]:
        line(start, end, pcb.Dwgs_User, 0.15)
    text("SH1106 + EC11 MODULE", 32, 50, pcb.Dwgs_User, 1.5)
    text("PANEL MOUNT / VERIFY HEIGHT", 32, 55, pcb.Dwgs_User)
    text("KIVO WORKBENCH", 31, 19, size=1.5)
    text("S3 r01 MATRIX / UNROUTED", 31, 23)
    text("J2 HARNESS: SEE README", 32, 65)
    for column, label in enumerate(["GND", "3V3"] + [f"GP{pin}" for pin in data["expansion_gpios"]]):
        text(label, 10 + column*2.54, 7, size=0.8, angle=90)
    text("J4 / 3V3 IO", 21.43, 12, size=0.8)
    text("BOOT", 66.27, 1, size=0.8)
    text("RESET", 75.27, 1, size=0.8)
    text("J7 / P2", 82, 26, size=0.8, angle=90)
    text("J1 / P1", 116, 26, size=0.8, angle=90)
    text("YD ESP32-S3", 99, 24, size=1.2)
    text("2 x 22 SOCKETS", 99, 28, size=0.9)
    text("USB-C AT REAR", 99, 32, size=0.9)
    # Measured module envelope and antenna clearance; module itself is removable.
    for start, end in [((85.03,0),(112.97,0)), ((112.97,0),(112.97,57.15)),
                       ((112.97,57.15),(108,57.15)), ((108,57.15),(108,63.39)),
                       ((108,63.39),(90,63.39)), ((90,63.39),(90,57.15)),
                       ((90,57.15),(85.03,57.15)), ((85.03,57.15),(85.03,0))]:
        line(start, end, pcb.Dwgs_User, 0.15)
    for start, end in [((89,57.7),(109,57.7)), ((109,57.7),(109,65)),
                       ((109,65),(89,65)), ((89,65),(89,57.7))]:
        line(start, end)
    keepout = pcb.ZONE(board)
    keepout.SetIsRuleArea(True)
    layers = pcb.LSET()
    layers.AddLayer(pcb.F_Cu)
    layers.AddLayer(pcb.B_Cu)
    keepout.SetLayerSet(layers)
    keepout.SetDoNotAllowTracks(True)
    keepout.SetDoNotAllowVias(True)
    keepout.SetDoNotAllowZoneFills(True)
    keepout.SetDoNotAllowPads(True)
    keepout.SetDoNotAllowFootprints(True)
    keepout.SetZoneName("ANTENNA / NO COPPER OR COMPONENTS")
    polygon = keepout.Outline()
    polygon.NewOutline()
    x1, y1, x2, y2 = data["antenna_keepout"]
    for x, y in [(x1,y1), (x2,y1), (x2,y2), (x1,y2)]:
        p = point(x, y)
        polygon.Append(p.x, p.y)
    board.Add(keepout)
    text("ANTENNA CLEARANCE", 99, 73, pcb.Dwgs_User, 0.9)
    for row, label in enumerate(["GND", "3V3", "SCL", "SDA", "OK", "PRESS", "A", "B", "BACK"]):
        text(label, 62, 35+row*2.54, size=0.8)
    for key in data["keys"]:
        text(key["key"].replace("KEY_", "K"), key["pcb_x"], key["pcb_y"]+7, size=1.0)
    text("126 x 135 mm / 1.6 mm / 2 Cu layers (proposed)", 63, -9, pcb.Dwgs_User, 1.2)
    text("NO ROUTING. NO GERBERS. ENCLOSURE REDESIGN REQUIRED.", 63, -6, pcb.Dwgs_User, 1.0)
    board.SetFileName(str(output))
    pcb.SaveBoard(str(output), board)
    print(f"Saved {len(data['parts'])} components, {len(nets)} nets, 18 bottom sockets; no tracks: {output}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--footprints", type=Path, default=Path("/Applications/KiCad/KiCad.app/Contents/SharedSupport/footprints"))
    args = parser.parse_args()
    if args.output.exists():
        parser.error("Output exists; choose a new filename to preserve existing PCB edits.")
    generate(args.manifest, args.output, args.footprints)
