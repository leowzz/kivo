"""Place, import routing, and verify the standalone 18-key I2C board using KiCad Python."""

import argparse
import json
import math
from pathlib import Path
import xml.etree.ElementTree as ET

import pcbnew as pcb
import wx


LIBRARIES = Path("/Applications/KiCad/KiCad.app/Contents/SharedSupport/footprints")


def point(x,y):
    return pcb.VECTOR2I(pcb.FromMM(x+50),pcb.FromMM(y+50))


def xy(item):
    p = item.GetPosition()
    return pcb.ToMM(p.x)-50,pcb.ToMM(p.y)-50


def text(board,value,x,y,layer=pcb.F_SilkS,size=0.9):
    item = pcb.PCB_TEXT(board)
    item.SetText(value)
    item.SetPosition(point(x,y))
    item.SetTextSize(pcb.VECTOR2I(pcb.FromMM(size),pcb.FromMM(size)))
    item.SetTextThickness(pcb.FromMM(0.12))
    item.SetLayer(layer)
    item.SetMirrored(layer == pcb.B_SilkS)
    board.Add(item)


def rule_area(board,name,vertices):
    zone = pcb.ZONE(board)
    zone.SetIsRuleArea(True)
    zone.SetZoneName(name)
    zone.SetLayerSet(pcb.LSET.AllCuMask(2))
    zone.SetDoNotAllowTracks(True)
    zone.SetDoNotAllowVias(True)
    zone.SetDoNotAllowZoneFills(True)
    zone.SetDoNotAllowPads(False)
    zone.SetDoNotAllowFootprints(False)
    polygon = zone.Outline()
    polygon.NewOutline()
    for x,y in vertices:
        p = point(x,y)
        polygon.Append(p.x,p.y)
    board.Add(zone)
    return zone


def place(data,directory,output):
    if output.exists():
        raise ValueError("Output board exists; choose a fresh placement path")
    board = pcb.BOARD()
    board.SetCopperLayerCount(2)
    board.GetDesignSettings().SetBoardThickness(pcb.FromMM(1.6))
    board.GetTitleBlock().SetTitle("Kivo 18-Key I2C / r01")
    nets = {}
    for name in sorted({n for p in data["parts"] for n in p["nets"].values() if n}):
        net = pcb.NETINFO_ITEM(board,"/"+name)
        board.Add(net)
        nets[name] = net
    settings = board.GetDesignSettings().m_NetSettings
    for name,width in [("Default",0.25),("Power",0.5)]:
        cls = pcb.NETCLASS(name)
        cls.SetTrackWidth(pcb.FromMM(width))
        cls.SetClearance(pcb.FromMM(0.2))
        cls.SetViaDiameter(pcb.FromMM(0.6))
        cls.SetViaDrill(pcb.FromMM(0.3))
        if name == "Default":
            settings.SetDefaultNetclass(cls)
        else:
            settings.SetNetclass(name,cls)
    for name in ("/3V3","/GND"):
        settings.SetNetclassPatternAssignment(name,"Power")
    settings.ClearAllCaches()
    settings.RecomputeEffectiveNetclasses()
    footprints = {}
    for part in data["parts"]:
        library,name = part["footprint"].split(":")
        path = directory / "Workbench.pretty" if library == "Workbench" else LIBRARIES / (library+".pretty")
        fp = pcb.FootprintLoad(str(path),name)
        if fp is None:
            raise ValueError(part["footprint"])
        fp.SetReference(part["ref"])
        fp.SetValue(part["value"])
        fp.SetPosition(point(*part["local_pcb"]))
        fp.SetPath(pcb.KIID_PATH(f"/{data['sheet_uuid']}/{part['uuid']}"))
        fp.SetSheetfile(data["name"]+".kicad_sch")
        board.Add(fp)
        if part["side"] == "B":
            fp.Flip(fp.GetPosition(),False)
        fp.SetOrientationDegrees(part["angle"])
        fp.Reference().SetVisible(False)
        fp.Value().SetVisible(False)
        for pad in fp.Pads():
            number = pad.GetNumber()
            if number and part["nets"][number]:
                pad.SetNet(nets[part["nets"][number]])
        footprints[part["ref"]] = fp
    w,h = data["width"],data["height"]
    corners = [(0,0),(w,0),(w,h),(0,h),(0,0)]
    for start,end in zip(corners,corners[1:]):
        edge = pcb.PCB_SHAPE(board)
        edge.SetShape(pcb.SHAPE_T_SEGMENT)
        edge.SetStart(point(*start))
        edge.SetEnd(point(*end))
        edge.SetWidth(pcb.FromMM(0.05))
        edge.SetLayer(pcb.Edge_Cuts)
        board.Add(edge)
    for index,(x,y) in enumerate(data["mounting_holes"],1):
        hole = pcb.FootprintLoad(str(directory / "Workbench.pretty"),"MountingHole_M3_5.6mm_Head")
        hole.SetReference(f"H{index}")
        hole.SetPosition(point(x,y))
        hole.SetBoardOnly(True)
        hole.Reference().SetVisible(False)
        hole.Value().SetVisible(False)
        board.Add(hole)
        rule_area(board,f"SCREW / H{index}",[(x+3*math.cos(i*math.pi/32),y+3*math.sin(i*math.pi/32)) for i in range(64)])
    text(board,"KIVO / 18 KEY I2C",22,5,size=1.1)
    text(board,"r01 / 3V3 ONLY",22,8.5)
    text(board,"MCP23017 / 0x20",49,1.3,size=0.8)
    text(board,"U1",44,15.5,size=0.8)
    text(board,"C2",57.8,15.1,size=0.8)
    for part in data["parts"]:
        if part["ref"] in ("C1","R1","R2","R3"):
            x,y = part["local_pcb"]
            text(board,part["ref"],x+4,y,size=0.8)
    for index,name in enumerate(data["connector"]["signals"]):
        text(board,name,90+index*2.54,10,size=0.8)
        text(board,name,90+index*2.54,9.5,pcb.B_SilkS,size=0.8)
    text(board,"J1 / 2.54 mm FEMALE",94,13.5,size=0.8)
    text(board,"J1 / SQUARE PAD = GND",93.81,13,pcb.B_SilkS,size=0.8)
    for key in data["keys"]:
        number = key["row"]*6+key["column"]+1
        text(board,f"K{number}",key["x"],key["y"]+7.4,size=0.9)
        text(board,f"K{number}",key["x"],key["y"]+7.4,pcb.B_SilkS,size=0.9)
        text(board,f"D{number}",key["x"]-3.8,key["y"]-6,pcb.B_SilkS,size=0.8)
    # Keep both adjacent U1 supply pins on short, via-free routes to C2.
    io_pads = {p.GetNumber():p for p in footprints["U1"].Pads()}
    cap_pads = {p.GetNumber():p for p in footprints["C2"].Pads()}
    for io_pin,cap_pin in (("9","1"),("10","2")):
        a,b = io_pads[io_pin],cap_pads[cap_pin]
        ax,ay = xy(a)
        bx,by = xy(b)
        points = [(ax,ay),(ax,by-abs(bx-ax)),(bx,by)]
        for start,end in zip(points,points[1:]):
            item = pcb.PCB_TRACK(board)
            item.SetStart(point(*start))
            item.SetEnd(point(*end))
            item.SetWidth(pcb.FromMM(0.4))
            item.SetLayer(pcb.F_Cu)
            item.SetNet(a.GetNet())
            item.SetLocked(True)
            board.Add(item)
    board.SetFileName(str(output))
    pcb.SaveBoard(str(output),board)
    guards = []
    for x1,y1,x2,y2 in [(0,0,w,.3),(0,h-.3,w,h),(0,0,.3,h),(w-.3,0,w,h)]:
        guards.append(rule_area(board,"ROUTER EDGE GUARD",[(x1,y1),(x2,y1),(x2,y2),(x1,y2)]))
    if not pcb.ExportSpecctraDSN(board,str(output.with_suffix(".dsn"))):
        raise RuntimeError("DSN export failed")
    print(json.dumps(dict(parts=len(footprints),nets=len(nets),board=str(output))))


def finish(data,source,session,output):
    if output.exists():
        raise ValueError("Routed output exists; preserve edits with a new output path")
    board = pcb.LoadBoard(str(source))
    if not pcb.ImportSpecctraSES(board,str(session)):
        raise RuntimeError("Routing import failed")
    for layer in (pcb.F_Cu,pcb.B_Cu):
        zone = pcb.ZONE(board)
        zone.SetNet(board.FindNet("/GND"))
        zone.SetLayer(layer)
        zone.SetZoneName("GND / "+board.GetLayerName(layer))
        zone.SetLocalClearance(pcb.FromMM(0.25))
        zone.SetPadConnection(pcb.ZONE_CONNECTION_THERMAL)
        zone.SetThermalReliefGap(pcb.FromMM(0.25))
        zone.SetThermalReliefSpokeWidth(pcb.FromMM(0.3))
        zone.SetMinThickness(pcb.FromMM(0.25))
        zone.SetIslandRemovalMode(pcb.ISLAND_REMOVAL_MODE_ALWAYS)
        polygon = zone.Outline()
        polygon.NewOutline()
        for x,y in [(.5,.5),(data["width"]-.5,.5),(data["width"]-.5,data["height"]-.5),(.5,data["height"]-.5)]:
            p = point(x,y)
            polygon.Append(p.x,p.y)
        board.Add(zone)
    board.BuildConnectivity()
    filler = pcb.ZONE_FILLER(board)
    assert filler.Fill(board.Zones())
    polygons = [z.GetFilledPolysList(z.GetLayer()) for z in board.Zones() if not z.GetIsRuleArea()]
    holes = [p for f in board.GetFootprints() for p in f.Pads() if p.GetDrillSize().x]
    for x in range(8,data["width"]-5,12):
        for y in range(8,data["height"]-5,12):
            center = point(x,y)
            ring = [point(x+.7*math.cos(i*math.pi/8),y+.7*math.sin(i*math.pi/8)) for i in range(16)]
            if not all(poly.Contains(p) for poly in polygons for p in [center]+ring):
                continue
            if any(math.hypot(center.x-p.GetPosition().x,center.y-p.GetPosition().y)
                   < max(p.GetDrillSize().x,p.GetDrillSize().y)/2+pcb.FromMM(.45) for p in holes):
                continue
            via = pcb.PCB_VIA(board)
            via.SetPosition(center)
            via.SetWidth(pcb.FromMM(.6))
            via.SetDrill(pcb.FromMM(.3))
            via.SetViaType(pcb.VIATYPE_THROUGH)
            via.SetLayerPair(pcb.F_Cu,pcb.B_Cu)
            via.SetNet(board.FindNet("/GND"))
            board.Add(via)
    board.BuildConnectivity()
    assert filler.Fill(board.Zones())
    pcb.SaveBoard(str(output),board)
    print(json.dumps(dict(board=str(output),tracks=len(board.GetTracks()),zones=len(board.Zones()))))


def verify(data,board_path,netlist):
    board = pcb.LoadBoard(str(board_path))
    fps = {f.GetReference():f for f in board.GetFootprints()}
    expected = {(p["ref"],n):"/"+net for p in data["parts"] for n,net in p["nets"].items() if net}
    actual = {(f.GetReference(),p.GetNumber()):p.GetNetname() for f in fps.values() for p in f.Pads() if p.GetNumber() and p.GetNetname()}
    exported,unconnected = {},set()
    for net in ET.parse(netlist).findall("./nets/net"):
        for node in net.findall("node"):
            if node.attrib["ref"].startswith("#"):
                continue
            key = (node.attrib["ref"],node.attrib["pin"])
            if "no_connect" in node.attrib.get("pintype", "").split("+"):
                assert len(net.findall("node")) == 1
                unconnected.add(key)
            else:
                exported[key] = net.attrib["name"]
    assert unconnected == {(p["ref"],n) for p in data["parts"] for n,net in p["nets"].items() if net is None}
    assert actual == expected, "PCB pads differ from manifest"
    assert exported == expected, dict(missing=set(expected.items())-set(exported.items()),extra=set(exported.items())-set(expected.items()))
    assert set(fps) == {p["ref"] for p in data["parts"]} | {f"H{i}" for i in range(1,5)}
    assert board.GetCopperLayerCount() == 2
    bounds = board.GetBoardEdgesBoundingBox()
    assert abs(pcb.ToMM(bounds.GetWidth())-data["width"]) < .1
    assert abs(pcb.ToMM(bounds.GetHeight())-data["height"]) < .1
    for part in data["parts"]:
        fp = fps[part["ref"]]
        assert all(abs(a-b)<.001 for a,b in zip(xy(fp),part["local_pcb"]))
        assert fp.GetLayer() == (pcb.B_Cu if part["side"] == "B" else pcb.F_Cu)
        for pad in fp.Pads():
            number = pad.GetNumber()
            if number:
                assert pad.GetNetname() == ("/"+part["nets"][number] if part["nets"][number] else "")
    jack = fps["J1"]
    assert str(jack.GetFPID().GetLibItemName()) == "PinSocket_1x04_P2.54mm_Vertical"
    for pad in jack.Pads():
        n = int(pad.GetNumber())
        assert all(abs(a-b)<.001 for a,b in zip(xy(pad),(90+(n-1)*2.54,6)))
        assert pad.GetAttribute() == pcb.PAD_ATTRIB_PTH
        assert abs(pcb.ToMM(pad.GetDrillSize().x)-1) < .001
        signal = data["connector"]["signals"][n-1]
        for layer in (pcb.F_SilkS,pcb.B_SilkS):
            labels = [item for item in board.GetDrawings() if isinstance(item,pcb.PCB_TEXT)
                      and item.GetLayer() == layer and item.GetText() == signal]
            assert len(labels) == 1 and abs(xy(labels[0])[0]-xy(pad)[0]) < .001
            assert labels[0].IsMirrored() == (layer == pcb.B_SilkS)
    for i in range(18):
        row,col = divmod(i,6)
        assert actual[(f"SW{i+1}","1")] == f"/GPB{col}"
        assert actual[(f"SW{i+1}","2")] == actual[(f"D{i+1}","2")] == f"/KEY_{i+1}_A"
        assert actual[(f"D{i+1}","1")] == f"/GPA{row+5}"
    io = {p.GetNumber():p for p in fps["U1"].Pads()}
    assert len(io) == 28 and all(p.GetAttribute() == pcb.PAD_ATTRIB_SMD for p in io.values())
    assert actual[("U1","28")] == "/GPA7" and ("U1","8") not in actual
    assert len([z for z in board.Zones() if z.GetIsRuleArea()]) == 4
    assert len([z for z in board.Zones() if not z.GetIsRuleArea()]) == 2
    assert board.GetTracks()
    print(json.dumps(dict(components=len(data["parts"]),connected_pins=len(actual),nets=len(set(actual.values())),keys=18,
                         connector_pitch_mm=2.54,board_mm=[data["width"],data["height"]],netlist_parity="PASS")))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode",choices=["place","finish","verify"])
    parser.add_argument("manifest",type=Path)
    parser.add_argument("board",type=Path)
    parser.add_argument("--output",type=Path)
    parser.add_argument("--session",type=Path)
    parser.add_argument("--netlist",type=Path)
    args = parser.parse_args()
    app = wx.App(False)
    wx.Log.SetActiveTarget(wx.LogStderr())
    data = json.loads(args.manifest.read_text())
    if args.mode == "place":
        place(data,args.manifest.parent,args.board)
    elif args.mode == "finish":
        if not args.output or not args.session:
            parser.error("finish requires --output and --session")
        finish(data,args.board,args.session,args.output)
    else:
        if not args.netlist:
            parser.error("verify requires --netlist")
        verify(data,args.board,args.netlist)
