"""Prepare/import local Specctra routing and fill the two independent ground planes."""

import argparse
import copy
import json
import math
from pathlib import Path
import shutil

import pcbnew as pcb
import wx


POWER_NETS = {"/+3V3", "/GND", "/UP_3V3", "/UP_GND"}


def point(x, y):
    return pcb.VECTOR2I(pcb.FromMM(x+50), pcb.FromMM(y+50))


def track(board, net, points, layer=pcb.F_Cu, width=0.25):
    for start, end in zip(points, points[1:]):
        item = pcb.PCB_TRACK(board)
        item.SetStart(point(*start))
        item.SetEnd(point(*end))
        item.SetWidth(pcb.FromMM(width))
        item.SetLayer(layer)
        item.SetNet(board.FindNet(net))
        item.SetLocked(True)
        board.Add(item)


def configure_project(source, output):
    data = json.loads(source.with_suffix(".kicad_pro").read_text())
    settings = data["net_settings"]
    default = next(c for c in settings["classes"] if c["name"] == "Default")
    default.update(track_width=0.25, clearance=0.2, via_diameter=0.6, via_drill=0.3)
    power = copy.deepcopy(default)
    power.update(name="Power", priority=0, track_width=0.5)
    settings["classes"] = [c for c in settings["classes"] if c["name"] != "Power"]+[power]
    settings["netclass_patterns"] = [p for p in settings["netclass_patterns"]
                                    if p["pattern"] not in POWER_NETS]
    settings["netclass_patterns"] += [dict(netclass="Power", pattern=n) for n in sorted(POWER_NETS)]
    design = data["board"]["design_settings"]
    design["rules"]["min_clearance"] = 0.2
    design["track_widths"] = [0.25, 0.4, 0.5]
    design["via_dimensions"] = [dict(diameter=0.6, drill=0.3)]
    output.with_suffix(".kicad_pro").write_text(json.dumps(data, indent=2)+"\n")


def prepare(board, source, output):
    assert not board.GetTracks(), "Prepare only an unrouted placement; preserve existing routes"
    configure_project(source, output)
    classes = {}
    for name, width in [("Default",0.25),("Power",0.5)]:
        nc = pcb.NETCLASS(name)
        nc.SetTrackWidth(pcb.FromMM(width))
        nc.SetClearance(pcb.FromMM(0.2))
        nc.SetViaDiameter(pcb.FromMM(0.6))
        nc.SetViaDrill(pcb.FromMM(0.3))
        classes[name] = nc
    settings = board.GetDesignSettings().m_NetSettings
    settings.SetDefaultNetclass(classes["Default"])
    settings.SetNetclass("Power", classes["Power"])
    for name in POWER_NETS:
        settings.SetNetclassPatternAssignment(name, "Power")
    settings.ClearAllCaches()
    settings.RecomputeEffectiveNetclasses()
    cap = next(f for f in board.GetFootprints() if f.GetReference() == "C3")
    cap.SetPosition(point(90.5,23.905))
    cap.SetOrientationDegrees(180)
    for drawing in board.GetDrawings():
        if isinstance(drawing, pcb.PCB_TEXT) and drawing.GetText() == "C3":
            drawing.SetPosition(point(90.5,26.905))
    # The local decoupling loop stays on the component layer, with no vias.
    track(board, "/UP_3V3", [(91.5375,23.905),(94.35,23.905)], width=0.4)
    track(board, "/UP_GND", [(89.4625,23.905),(89.4625,24.4),
                             (90.2375,25.175),(94.35,25.175)], width=0.4)
    # Preserve room for 5.6 mm metal screw heads on both board faces.
    for footprint in board.GetFootprints():
        if not footprint.GetReference().startswith("H_"):
            continue
        zone = pcb.ZONE(board)
        zone.SetIsRuleArea(True)
        zone.SetZoneName("SCREW / "+footprint.GetReference())
        layers = pcb.LSET()
        layers.AddLayer(pcb.F_Cu)
        layers.AddLayer(pcb.B_Cu)
        zone.SetLayerSet(layers)
        zone.SetDoNotAllowTracks(True)
        zone.SetDoNotAllowVias(True)
        zone.SetDoNotAllowZoneFills(True)
        zone.SetDoNotAllowPads(False)
        zone.SetDoNotAllowFootprints(False)
        polygon = zone.Outline()
        polygon.NewOutline()
        center = footprint.GetPosition()
        for i in range(64):
            angle = 2*math.pi*i/64
            polygon.Append(center.x+pcb.FromMM(3*math.cos(angle)),
                           center.y+pcb.FromMM(3*math.sin(angle)))
        board.Add(zone)
    def lower(net, points, layer, width):
        track(board,net,[(126-x,187-y) for x,y in points],layer,width)
    # A 0.4 mm section passes between the 2.54 mm socket pads; main branches use 0.5 mm.
    lower("/+3V3",[(111.7,55.25),(111.7,52.71)],pcb.F_Cu,0.5)
    lower("/+3V3",[(111.7,55.25),(109,52.55),(109,18.42),(84,18.42),
                    (74.42,28),(49.5,28),(49.5,25)],pcb.B_Cu,0.4)
    lower("/+3V3",[(49.5,25),(62.5,12),(71,12)],pcb.F_Cu,0.5)
    lower("/+3V3",[(12.54,3),(12.54,5),(19.54,12),(71,12)],pcb.F_Cu,0.5)
    pcb.SaveBoard(str(output), board)
    # KiCad DSN omits edge clearance. A 0.3 mm router-only guard plus its
    # 0.2 mm clearance enforces the project's 0.5 mm copper setback.
    edge_guards = []
    for bounds in [(0,0,126,0.3),(0,186.7,126,187),(0,0,0.3,187),(125.7,0,126,187)]:
        zone = pcb.ZONE(board)
        zone.SetIsRuleArea(True)
        zone.SetLayerSet(pcb.LSET.AllCuMask(2))
        zone.SetDoNotAllowTracks(True)
        zone.SetDoNotAllowVias(True)
        zone.SetDoNotAllowZoneFills(True)
        zone.SetDoNotAllowPads(False)
        zone.SetDoNotAllowFootprints(False)
        polygon = zone.Outline()
        polygon.NewOutline()
        x1,y1,x2,y2 = bounds
        for xy in [(x1,y1),(x2,y1),(x2,y2),(x1,y2)]:
            p = point(*xy)
            polygon.Append(p.x,p.y)
        board.Add(zone)
        edge_guards.append(zone)
    assert pcb.ExportSpecctraDSN(board, str(output.with_suffix(".dsn")))
    for zone in edge_guards:
        board.RemoveNative(zone)


def finish(board):
    assert board.GetTracks(), "No routes to finish"
    assert all(z.GetIsRuleArea() for z in board.Zones()), "Ground pours already exist"
    for net, bounds in [("/UP_GND",(0.5,0.5,125.5,95.5)),("/GND",(0.5,103.5,125.5,186.5))]:
        for layer in (pcb.F_Cu, pcb.B_Cu):
            zone = pcb.ZONE(board)
            zone.SetNet(board.FindNet(net))
            zone.SetLayer(layer)
            zone.SetZoneName(net.lstrip("/")+" / "+board.GetLayerName(layer))
            zone.SetLocalClearance(pcb.FromMM(0.25))
            zone.SetPadConnection(pcb.ZONE_CONNECTION_THERMAL)
            zone.SetThermalReliefGap(pcb.FromMM(0.25))
            zone.SetThermalReliefSpokeWidth(pcb.FromMM(0.3))
            zone.SetMinThickness(pcb.FromMM(0.25))
            zone.SetIslandRemovalMode(pcb.ISLAND_REMOVAL_MODE_ALWAYS)
            x1,y1,x2,y2 = bounds
            polygon = zone.Outline()
            polygon.NewOutline()
            for xy in [(x1,y1),(x2,y1),(x2,y2),(x1,y2)]:
                p = point(*xy)
                polygon.Append(p.x,p.y)
            board.Add(zone)
    for drawing in board.GetDrawings():
        if isinstance(drawing, pcb.PCB_TEXT):
            drawing.SetText(drawing.GetText().replace("UNROUTED", "ROUTED")
                            .replace("NOT FOR FABRICATION", "FIT CHECK PENDING"))
    board.GetTitleBlock().SetTitle("Workbench S3 r01 - ROUTED BREAKAWAY PANEL")
    board.BuildConnectivity()
    filler = pcb.ZONE_FILLER(board)
    assert filler.Fill(board.Zones())
    # Stitch only where a full clearance ring lies inside both filled ground layers.
    stitching = 0
    copper = {z.GetZoneName(): z for z in board.Zones() if not z.GetIsRuleArea()}
    holes = [p for f in board.GetFootprints() for p in f.Pads() if p.GetDrillSize().x]
    for net, ys in [("/UP_GND",range(12,93,15)),("/GND",range(112,184,15))]:
        polygons = [copper[net.lstrip("/")+" / "+board.GetLayerName(layer)].GetFilledPolysList(layer)
                    for layer in (pcb.F_Cu,pcb.B_Cu)]
        # J9 has a small front ground pocket enclosed by the power/signal routes.
        candidates = ([(81,9.5)] if net == "/UP_GND" else [])
        candidates += [(x,y) for x in range(8,124,15) for y in ys]
        for x,y in candidates:
            center = point(x,y)
            ring = [point(x+0.7*math.cos(i*math.pi/8),y+0.7*math.sin(i*math.pi/8)) for i in range(16)]
            if not all(poly.Contains(p) for poly in polygons for p in [center]+ring):
                continue
            if any(math.hypot(center.x-p.GetPosition().x,center.y-p.GetPosition().y)
                   < max(p.GetDrillSize().x,p.GetDrillSize().y)/2+pcb.FromMM(0.45) for p in holes):
                continue
            if any(isinstance(t,pcb.PCB_VIA) and math.hypot(center.x-t.GetPosition().x,
                   center.y-t.GetPosition().y) < pcb.FromMM(2) for t in board.GetTracks()):
                continue
            via = pcb.PCB_VIA(board)
            via.SetPosition(center)
            via.SetWidth(pcb.FromMM(0.6))
            via.SetDrill(pcb.FromMM(0.3))
            via.SetViaType(pcb.VIATYPE_THROUGH)
            via.SetLayerPair(pcb.F_Cu,pcb.B_Cu)
            via.SetNet(board.FindNet(net))
            board.Add(via)
            stitching += 1
    board.BuildConnectivity()
    assert filler.Fill(board.Zones())
    print(f"Added {stitching} ground stitching vias")


def extract(board, view):
    upper = view == "upper"
    for footprint in list(board.GetFootprints()):
        y = pcb.ToMM(footprint.GetPosition().y)-50
        if footprint.GetReference().startswith("MB") or (y < 99.5) != upper:
            board.RemoveNative(footprint)
    for item in list(board.GetTracks()):
        if item.GetNetname().startswith("/UP_") != upper:
            board.RemoveNative(item)
    for zone in list(board.Zones()):
        if zone.GetZoneName().startswith("BREAKAWAY") or (pcb.ToMM(zone.GetPosition().y)-50 < 99.5) != upper:
            board.RemoveNative(zone)
    for item in list(board.GetDrawings()):
        if item.GetLayer() == pcb.Edge_Cuts or (pcb.ToMM(item.GetPosition().y)-50 < 99.5) != upper:
            board.RemoveNative(item)
    if not upper:
        for item in list(board.GetFootprints())+list(board.GetTracks())+list(board.Zones())+list(board.GetDrawings()):
            item.Rotate(pcb.VECTOR2I(pcb.FromMM(113),pcb.FromMM(143.5)),pcb.EDA_ANGLE(180,pcb.DEGREES_T))
    bounds = [(0,0,126,98 if upper else 86)]
    if not upper:
        bounds.append((89,57.7,109,65))
    for x1,y1,x2,y2 in bounds:
        corners = [(x1,y1),(x2,y1),(x2,y2),(x1,y2),(x1,y1)]
        for start,end in zip(corners,corners[1:]):
            edge = pcb.PCB_SHAPE(board)
            edge.SetShape(pcb.SHAPE_T_SEGMENT)
            edge.SetStart(point(*start))
            edge.SetEnd(point(*end))
            edge.SetWidth(pcb.FromMM(0.05))
            edge.SetLayer(pcb.Edge_Cuts)
            board.Add(edge)
    board.GetTitleBlock().SetTitle(f"Workbench S3 r01 - {view.upper()} - ROUTED")
    board.BuildConnectivity()
    filler = pcb.ZONE_FILLER(board)
    assert filler.Fill(board.Zones())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["prepare","import","finish","upper","lower"])
    parser.add_argument("source",type=Path)
    parser.add_argument("output",type=Path)
    parser.add_argument("--session",type=Path)
    args = parser.parse_args()
    if args.output.exists() or args.output.with_suffix(".kicad_pro").exists():
        parser.error("Output board or project exists; choose a new filename")
    if not args.source.with_suffix(".kicad_pro").exists():
        parser.error("Source project is required to preserve routing and DRC rules")
    app = wx.App(False)
    wx.Log.SetActiveTarget(wx.LogStderr())
    board = pcb.LoadBoard(str(args.source))
    if args.mode == "prepare":
        prepare(board,args.source,args.output)
    else:
        if args.mode == "import":
            assert args.session and pcb.ImportSpecctraSES(board,str(args.session))
        elif args.mode == "finish":
            finish(board)
        else:
            extract(board,args.mode)
        pcb.SaveBoard(str(args.output),board)
        shutil.copy2(args.source.with_suffix(".kicad_pro"),args.output.with_suffix(".kicad_pro"))
    print(json.dumps(dict(output=str(args.output),tracks=len(board.GetTracks()),zones=len(board.Zones()))))


if __name__ == "__main__":
    main()
