"""Create the unrouted breakaway panel or an individual board with KiCad Python."""

import argparse
import json
from pathlib import Path

import pcbnew as pcb
import wx


def position(x, y):
    return pcb.VECTOR2I(pcb.FromMM(x), pcb.FromMM(y))


def generate(manifest, output, libraries, view="panel"):
    app = wx.App(False)
    data = json.loads(manifest.read_text())
    board = pcb.BOARD()
    board.SetCopperLayerCount(2)
    board.GetDesignSettings().SetBoardThickness(pcb.FromMM(1.6))
    board.GetTitleBlock().SetTitle(f"Workbench S3 r01 - {view.upper()} - UNROUTED")
    parts = [part for part in data["parts"] if view == "panel" or part["section"] == view]
    nets = {}
    for name in sorted({net for part in parts for net in part["nets"].values() if net}):
        net = pcb.NETINFO_ITEM(board, "/" + name)
        board.Add(net)
        nets[name] = net

    def point(x, y, section=None):
        if view == "panel" and section == "lower":
            x, y = data["width"]-x, data["height"]-y
        return position(50+x, 50+y)

    def line(start, end, layer=pcb.Edge_Cuts, width=0.05, section=None):
        item = pcb.PCB_SHAPE(board)
        item.SetShape(pcb.SHAPE_T_SEGMENT)
        item.SetStart(point(*start, section))
        item.SetEnd(point(*end, section))
        item.SetLayer(layer)
        item.SetWidth(pcb.FromMM(width))
        board.Add(item)

    def arc(start, mid, end):
        item = pcb.PCB_SHAPE(board)
        item.SetShape(pcb.SHAPE_T_ARC)
        item.SetArcGeometry(point(*start), point(*mid), point(*end))
        item.SetLayer(pcb.Edge_Cuts)
        item.SetWidth(pcb.FromMM(0.05))
        board.Add(item)

    def rectangle(bounds, section=None, layer=pcb.Edge_Cuts):
        x1, y1, x2, y2 = bounds
        corners = [(x1,y1), (x2,y1), (x2,y2), (x1,y2)]
        for index, start in enumerate(corners):
            line(start, corners[(index+1) % 4], layer=layer, section=section)

    def text(value, x, y, section=None, layer=pcb.F_SilkS, size=0.9, angle=0):
        item = pcb.PCB_TEXT(board)
        item.SetText(value)
        item.SetPosition(point(x, y, section))
        item.SetTextSize(position(size, size))
        item.SetTextThickness(pcb.FromMM(0.15))
        if view == "panel" and section == "lower":
            angle += 180
        item.SetTextAngle(pcb.EDA_ANGLE(angle, pcb.DEGREES_T))
        item.SetLayer(layer)
        item.SetMirrored(layer == pcb.B_SilkS)
        board.Add(item)

    def rule_area(name, bounds, section=None, forbid_parts=True):
        item = pcb.ZONE(board)
        item.SetIsRuleArea(True)
        layers = pcb.LSET()
        layers.AddLayer(pcb.F_Cu)
        layers.AddLayer(pcb.B_Cu)
        item.SetLayerSet(layers)
        item.SetDoNotAllowTracks(True)
        item.SetDoNotAllowVias(True)
        item.SetDoNotAllowZoneFills(True)
        item.SetDoNotAllowPads(forbid_parts)
        item.SetDoNotAllowFootprints(forbid_parts)
        item.SetZoneName(name)
        polygon = item.Outline()
        polygon.NewOutline()
        x1, y1, x2, y2 = bounds
        for x, y in [(x1,y1), (x2,y1), (x2,y2), (x1,y2)]:
            p = point(x, y, section)
            polygon.Append(p.x, p.y)
        board.Add(item)

    for part in parts:
        library, name = part["footprint"].split(":")
        path = manifest.parent / "Workbench.pretty" if library == "Workbench" else libraries / f"{library}.pretty"
        footprint = pcb.FootprintLoad(str(path), name)
        if footprint is None:
            raise ValueError(f"Cannot load {part['footprint']}")
        footprint.SetReference(part["ref"])
        footprint.SetValue(part["value"])
        footprint.SetPosition(point(*part["local_pcb"], part["section"]))
        footprint.SetPath(pcb.KIID_PATH(f"/{data['sheet_uuid']}/{part['uuid']}"))
        footprint.SetSheetfile("workbench-s3-r01.kicad_sch")
        board.Add(footprint)
        if part["side"] == "B":
            footprint.Flip(footprint.GetPosition(), False)
        rotation = 180 if view == "panel" and part["section"] == "lower" else 0
        footprint.SetOrientationDegrees(part["angle"] + rotation)
        footprint.SetDNP(part.get("dnp", False))
        footprint.Value().SetVisible(False)
        footprint.Reference().SetTextSize(position(0.8, 0.8))
        footprint.Reference().SetTextThickness(pcb.FromMM(0.12))
        if part["ref"].startswith(("C", "R", "J")):
            footprint.Reference().SetVisible(False)
        for pad in footprint.Pads():
            number = pad.GetNumber()
            if not number:
                continue
            if number not in part["nets"]:
                raise ValueError(f"{part['ref']}: pad {number} not in schematic")
            if part["nets"][number]:
                pad.SetNet(nets[part["nets"][number]])

    if view == "panel":
        # Two routed internal slots and open side notches leave three 5 mm necks.
        path = [(0,0),(126,0),(126,98),(110,98)]
        for start, end in zip(path, path[1:]):
            line(start, end)
        arc((110,98), (108.5,99.5), (110,101))
        path = [(110,101),(126,101),(126,187),(0,187),(0,101),(16,101)]
        for start, end in zip(path, path[1:]):
            line(start, end)
        arc((16,101), (17.5,99.5), (16,98))
        for start, end in [((16,98),(0,98)), ((0,98),(0,0))]:
            line(start, end)
        for left, right in [(22.5,60.5), (65.5,103.5)]:
            line((left+1.5,98), (right-1.5,98))
            arc((right-1.5,98), (right,99.5), (right-1.5,101))
            line((right-1.5,101), (left+1.5,101))
            arc((left+1.5,101), (left,99.5), (left+1.5,98))
        for index, x in enumerate(data["panel"]["tab_centers"]):
            for row, y in enumerate(data["panel"]["mouse_bite_rows"]):
                item = pcb.FootprintLoad(str(manifest.parent / "Workbench.pretty"), "MouseBites_6x0.5_P0.8")
                item.SetReference(f"MB{index*2+row+1}")
                item.SetPosition(point(x, y))
                item.Value().SetVisible(False)
                item.Reference().SetVisible(False)
                item.SetBoardOnly(True)
                board.Add(item)
        rule_area("BREAKAWAY / NO COPPER / MECHANICAL HOLES ONLY",
                  data["panel"]["copper_keepout"], forbid_parts=False)
    else:
        width, height = data["boards"][view]["size"]
        rectangle([0,0,width,height])

    for section, settings in data["boards"].items():
        if view != "panel" and view != section:
            continue
        for index, center in enumerate(settings["mounting_holes"], 1):
            hole = pcb.FootprintLoad(str(manifest.parent / "Workbench.pretty"), "MountingHole_M3_5.6mm_Head")
            hole.SetReference(f"H_{section[0].upper()}{index}")
            hole.SetPosition(point(*center, section))
            hole.Value().SetVisible(False)
            hole.Reference().SetVisible(False)
            hole.SetBoardOnly(True)
            board.Add(hole)

    if view in ("panel", "upper"):
        rectangle([2,2,67,37], "upper", pcb.Dwgs_User)
        text("SH1106 + EC11 / PANEL MOUNT", 34, 18, "upper", pcb.Dwgs_User)
        text("UPPER / 3x6 MATRIX", 30, 9, "upper", size=1.2)
        text("S3 r01 / UNROUTED", 30, 13, "upper")
        text("J2 DISPLAY / 3V3", 28, 30, "upper")
        text("J9 IDC20 / TO LOWER J8", 101, 24, "upper", pcb.B_SilkS, size=0.8)
        for ref, x in [("C1",89), ("C2",94), ("R1",101), ("R2",106)]:
            text(ref, x, 31, "upper", size=0.8)
        text("R1/R2 DNP", 103.5, 34, "upper", size=0.8)
        for row, label in enumerate(["GND", "3V3", "SCL", "SDA", "OK", "PRESS", "A", "B", "BACK"]):
            text(label, 68, 5+row*2.54, "upper", size=0.8)
        for key in data["keys"]:
            text(key["key"].replace("KEY_", "K"), key["pcb_x"], key["pcb_y"]+7, "upper")
        text("TRIM TAB STUBS BEFORE ASSEMBLY", 63, 97, "upper", pcb.Dwgs_User, size=0.8)

    if view in ("panel", "lower"):
        text("LOWER / YD ESP32-S3", 34, 45, "lower", size=1.2)
        text("HORIZONTAL / UNROUTED", 34, 49, "lower")
        text("J8 IDC20 / TO UPPER J9", 58, 33, "lower", size=0.8)
        for column, label in enumerate(["GND", "3V3"] + [f"GP{pin}" for pin in data["expansion_gpios"]]):
            text(label, 10+column*2.54, 7, "lower", size=0.8, angle=90)
        text("J4 / 3V3 IO", 21.43, 12, "lower", size=0.8)
        text("BOOT", 66.27, 1, "lower", size=0.8)
        text("RESET", 75.27, 1, "lower", size=0.8)
        text("R3", 72, 15, "lower", size=0.8)
        text("J7 / P2", 82, 26, "lower", size=0.8, angle=90)
        text("J1 / P1", 116, 26, "lower", size=0.8, angle=90)
        text("2 x 22 SOCKETS", 99, 28, "lower")
        text("USB-C AT REAR", 99, 32, "lower")
        outline = [(85.03,0),(112.97,0),(112.97,57.15),(108,57.15),
                   (108,63.39),(90,63.39),(90,57.15),(85.03,57.15),(85.03,0)]
        for start, end in zip(outline, outline[1:]):
            line(start, end, pcb.Dwgs_User, 0.15, "lower")
        rectangle([89,57.7,109,65], "lower")
        rule_area("ANTENNA / NO COPPER OR COMPONENTS", data["antenna_keepout"], "lower")
        text("ANTENNA CLEARANCE", 99, 73, "lower", pcb.Dwgs_User)

    text(f"{view.upper()} / 1.6 mm / 2 Cu / NOT FOR FABRICATION", 63, -6, layer=pcb.Dwgs_User)
    board.SetFileName(str(output))
    pcb.SaveBoard(str(output), board)
    print(f"Saved {view}: {len(parts)} parts, {len(nets)} nets, no tracks: {output}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--view", choices=["panel", "upper", "lower"], default="panel")
    parser.add_argument("--footprints", type=Path, default=Path("/Applications/KiCad/KiCad.app/Contents/SharedSupport/footprints"))
    args = parser.parse_args()
    if args.output.exists():
        parser.error("Output exists; choose a new filename to preserve existing PCB edits.")
    generate(args.manifest, args.output, args.footprints, args.view)
