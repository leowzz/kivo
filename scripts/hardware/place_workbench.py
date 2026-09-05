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
    board.SetCopperLayerCount(4)
    board.GetDesignSettings().SetBoardThickness(pcb.FromMM(1.6))
    board.GetTitleBlock().SetTitle("Kivo Workbench r03 - UNROUTED REVIEW DRAFT")
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

    def text(value, x, y, layer=pcb.F_SilkS, size=1.0):
        item = pcb.PCB_TEXT(board)
        item.SetText(value)
        item.SetPosition(point(x, y))
        item.SetTextSize(position(size, size))
        item.SetTextThickness(pcb.FromMM(0.15))
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
        footprint.SetSheetfile("workbench-r03.kicad_sch")
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
        if part["ref"] == "U1":
            footprint.Reference().SetPosition(point(99, 36))
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
    for index, center in enumerate([(4,4), (4,43), (122,43), (4,101), (122.5,101)]):
        hole = pcb.FootprintLoad(str(manifest.parent / "Workbench.pretty"), "MountingHole_M3_5.6mm_Head")
        hole.SetReference(f"H{index+1}")
        hole.SetPosition(point(*center))
        hole.Value().SetVisible(False)
        hole.Reference().SetVisible(False)
        hole.SetBoardOnly(True)
        board.Add(hole)

    # Display envelope only: the module stays panel-mounted and uses a harness.
    for start, end in [((2,5),(67,5)), ((67,5),(67,40)), ((67,40),(2,40)), ((2,40),(2,5))]:
        line(start, end, pcb.Dwgs_User, 0.15)
    text("SH1106 + EC11 MODULE", 34, 20, pcb.Dwgs_User, 1.5)
    text("PANEL MOUNT / VERIFY HEIGHT", 34, 25, pcb.Dwgs_User)
    text("KIVO WORKBENCH", 38, 10, size=1.5)
    text("r03 DRAFT - UNROUTED", 38, 14)
    text("J2 HARNESS: SEE README", 41, 35)
    for row, label in enumerate(["GND", "3V3", "SCL", "SDA", "OK", "PRESS", "A", "B", "BACK"]):
        text(label, 67.0, 16+row*2.54, size=0.8)
    for key in data["keys"]:
        text(key["key"].replace("KEY_", "K"), key["pcb_x"], key["pcb_y"]+7, size=1.0)
    text("126 x 105 mm / 1.6 mm / 4 Cu layers (proposed)", 63, -9, pcb.Dwgs_User, 1.2)
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
