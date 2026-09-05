"""Verify the separate circuits and breakaway geometry of the unrouted S3 draft."""

import argparse
import csv
import json
from pathlib import Path
import xml.etree.ElementTree as ET

import pcbnew as pcb
import wx


def near(actual, expected):
    assert abs(actual - expected) < 0.001, (actual, expected)


def xy(item):
    point = item.GetPosition()
    return pcb.ToMM(point.x) - 50, pcb.ToMM(point.y) - 50


def panel_point(x, y, section, view):
    return (126-x, 187-y) if section == "lower" and view == "panel" else (x, y)


def verify_board(path, data, expected, view):
    board = pcb.LoadBoard(str(path))
    footprints = {f.GetReference(): f for f in board.GetFootprints()}
    parts = {p["ref"]: p for p in data["parts"] if view == "panel" or p["section"] == view}
    expected = {key: net for key, net in expected.items() if key[0] in parts}
    actual = {}
    for ref, footprint in footprints.items():
        for pad in footprint.Pads():
            if pad.GetNumber() and pad.GetNetname():
                key = (ref, pad.GetNumber())
                if key in actual:
                    assert actual[key] == pad.GetNetname(), key
                actual[key] = pad.GetNetname()
    assert expected == actual, dict(
        missing=sorted(set(expected.items()) - set(actual.items())),
        extra=sorted(set(actual.items()) - set(expected.items())))
    for ref, part in parts.items():
        footprint = footprints[ref]
        target = panel_point(*part["local_pcb"], part["section"], view)
        for coordinate, value in zip(xy(footprint), target):
            near(coordinate, value)
        assert footprint.GetLayer() == (pcb.B_Cu if part["side"] == "B" else pcb.F_Cu)
        assert footprint.IsDNP() == part["dnp"]
        for number, net in part["nets"].items():
            assert actual.get((ref, number)) == ("/"+net if net else None), (ref, number)

    # Independent pad-coordinate checks catch underside mirroring and panel rotation.
    for ref, count, x, y, dx, dy in [
        ("J1",22,111.7,55.25,0,-2.54), ("J7",22,86.3,55.25,0,-2.54),
        ("J4",24,10,3,2.54,0), ("J5",2,73,4,2.54,0), ("J6",2,80,4,2.54,0),
    ]:
        if ref not in parts:
            continue
        pads = {pad.GetNumber(): pad for pad in footprints[ref].Pads()}
        assert len(pads) == count
        for i in range(1, count + 1):
            pad = pads[str(i)]
            assert pad.GetAttribute() == pcb.PAD_ATTRIB_PTH
            assert pad.GetLayerSet().Contains(pcb.F_Cu) and pad.GetLayerSet().Contains(pcb.B_Cu)
            near(pcb.ToMM(pad.GetDrillSize().x), 1)
            near(pcb.ToMM(pad.GetDrillSize().y), 1)
            target = panel_point(x+(i-1)*dx, y+(i-1)*dy, "lower", view)
            for coordinate, value in zip(xy(pad), target):
                near(coordinate, value)
    for ref, section, origin in [
        ("J8","lower",(47,25)), ("J9","upper",(83,8)),
    ]:
        if ref not in parts:
            continue
        pads = {pad.GetNumber(): pad for pad in footprints[ref].Pads()}
        assert len(pads) == 4
        assert str(footprints[ref].GetFPID().GetLibItemName()) == "JST_XH_B4B-XH-A_1x04_P2.50mm_Vertical"
        for i in range(1, 5):
            pad = pads[str(i)]
            assert pad.GetAttribute() == pcb.PAD_ATTRIB_PTH
            near(pcb.ToMM(pad.GetDrillSize().x), 0.95)
            near(pcb.ToMM(pad.GetDrillSize().y), 0.95)
            target = panel_point(origin[0]+(i-1)*2.50, origin[1], section, view)
            for coordinate, value in zip(xy(pad), target):
                near(coordinate, value)

    if view in ("panel", "upper"):
        io_pads = {pad.GetNumber(): pad for pad in footprints["U1"].Pads()}
        assert len(io_pads) == 28
        assert str(footprints["U1"].GetFPID().GetLibItemName()) == "SOIC-28W_7.5x17.9mm_P1.27mm"
        for i in range(1,29):
            pad = io_pads[str(i)]
            assert pad.GetAttribute() == pcb.PAD_ATTRIB_SMD
            near(xy(pad)[0], 99+(-4.65 if i <= 14 else 4.65))
            near(xy(pad)[1], 22+(-8.255+(i-1)*1.27 if i <= 14 else 8.255-(i-15)*1.27))
        pads = {pad.GetNumber(): pad for pad in footprints["J2"].Pads()}
        assert len(pads) == 9
        for i in range(1,10):
            pad = pads[str(i)]
            assert pad.GetAttribute() == pcb.PAD_ATTRIB_PTH
            near(pcb.ToMM(pad.GetDrillSize().x), 1)
            # Module pin 1 is the rightmost square pad in the supplied front view.
            near(xy(pad)[0], 8+11.38+(9-i)*2.54)
            near(xy(pad)[1], 3+1.93)

    keepouts = {zone.GetZoneName(): zone for zone in board.Zones()}
    assert len(keepouts) == {"panel":2, "upper":0, "lower":1}[view]
    for name, zone in keepouts.items():
        assert zone.GetIsRuleArea()
        assert zone.GetDoNotAllowTracks() and zone.GetDoNotAllowVias() and zone.GetDoNotAllowZoneFills()
        assert zone.GetLayerSet().Contains(pcb.F_Cu) and zone.GetLayerSet().Contains(pcb.B_Cu)
        antenna = name.startswith("ANTENNA")
        assert zone.GetDoNotAllowPads() == zone.GetDoNotAllowFootprints() == antenna
        x1, y1, x2, y2 = data["antenna_keepout"] if antenna else data["panel"]["copper_keepout"]
        for x in [x1+0.1, (x1+x2)/2, x2-0.1]:
            for y in [y1+0.1, (y1+y2)/2, y2-0.1]:
                px, py = panel_point(x, y, "lower" if antenna else "upper", view)
                assert zone.Outline().Contains(pcb.VECTOR2I(pcb.FromMM(px+50), pcb.FromMM(py+50)))
    holes = [pad for ref, f in footprints.items() if ref.startswith("MB") for pad in f.Pads()]
    assert len(holes) == (36 if view == "panel" else 0)
    if view == "panel":
        targets = {(round(x+d,3),y) for x in [20,63,106] for y in [98.75,100.25]
                   for d in [-2,-1.2,-0.4,0.4,1.2,2]}
        assert {(round(xy(pad)[0],3),round(xy(pad)[1],3)) for pad in holes} == targets
        for pad in holes:
            assert pad.GetAttribute() == pcb.PAD_ATTRIB_NPTH and not pad.GetNetname()
            near(pcb.ToMM(pad.GetDrillSize().x), 0.5)
            near(pcb.ToMM(pad.GetDrillSize().y), 0.5)
        for ref in parts:
            for pad in footprints[ref].Pads():
                if pad.GetNumber():
                    bounds = pad.GetBoundingBox()
                    low, high = pcb.ToMM(bounds.GetTop())-50, pcb.ToMM(bounds.GetBottom())-50
                    assert high < 96 or low > 103, (ref,pad.GetNumber(),"electrical pad in trim band")
    mounts = {ref for ref in footprints if ref.startswith("H_")}
    assert len(mounts) == {"panel":12, "upper":8, "lower":4}[view]
    assert len({xy(footprints[ref]) for ref in mounts}) == len(mounts)
    for section, settings in data["boards"].items():
        if view != "panel" and view != section:
            continue
        for i, center in enumerate(settings["mounting_holes"], 1):
            hole = footprints[f"H_{section[0].upper()}{i}"]
            for coordinate, value in zip(xy(hole), panel_point(*center, section, view)):
                near(coordinate, value)
            assert all(p.GetAttribute() == pcb.PAD_ATTRIB_NPTH for p in hole.Pads())
    if view in ("panel", "upper"):
        module = data["display_module"]
        assert module["origin"] == [8,3] and module["size"] == [64.90,35.03]
        centers = [[2.87,2.85],[64.90-3,2.90],[2.95,35.03-2.97],[64.90-2.97,35.03-3.15]]
        for i, (x,y) in enumerate(centers,1):
            hole = footprints[f"H_D{i}"]
            near(xy(hole)[0], 8+x)
            near(xy(hole)[1], 3+y)
            assert all(p.GetAttribute() == pcb.PAD_ATTRIB_NPTH for p in hole.Pads())
    assert set(footprints) == set(parts) | mounts | {f"MB{i}" for i in range(1,len(holes)//6+1)}
    outlines = pcb.SHAPE_POLY_SET()
    assert board.GetBoardPolygonOutlines(outlines, False), "Invalid/open Edge.Cuts"
    assert outlines.OutlineCount() == 1, "Panel must remain mechanically connected"
    assert outlines.HoleCount(0) == {"panel":3, "upper":0, "lower":1}[view]
    if view == "panel":
        for x, inside in [(5,False),(20,True),(40,False),(63,True),(80,False),(106,True),(120,False)]:
            assert outlines.Contains(pcb.VECTOR2I(pcb.FromMM(x+50),pcb.FromMM(149.5))) == inside
    width, height = [126,187] if view == "panel" else data["boards"][view]["size"]
    bounds = board.GetBoardEdgesBoundingBox()
    assert abs(pcb.ToMM(bounds.GetWidth())-width) < 0.1
    assert abs(pcb.ToMM(bounds.GetHeight())-height) < 0.1
    assert board.GetCopperLayerCount() == 2
    near(pcb.ToMM(board.GetDesignSettings().GetBoardThickness()), 1.6)
    assert len(board.GetTracks()) == 0, "This checker describes the unrouted placement stage"
    return dict(view=view, connected_pins=len(actual), connected_nets=len(set(actual.values())),
                electrical_parts=len(parts), mounts=len(mounts), breakaway_holes=len(holes),
                outline_holes=outlines.HoleCount(0), size_mm=[width,height])


def verify(directory, netlist, individual_boards=None):
    app = wx.App(False)
    data = json.loads((directory / "placement.json").read_text())
    expected = {}
    for net in ET.parse(netlist).getroot().findall("./nets/net"):
        name = net.attrib["name"]
        if not name.startswith("unconnected-"):
            for pin in net.findall("node"):
                if not pin.attrib["ref"].startswith("#"):
                    expected[(pin.attrib["ref"],pin.attrib["pin"])] = name
    parts = {p["ref"]: p for p in data["parts"]}
    assert len(parts) == 52 and {key[0] for key in expected} == set(parts)
    section_nets = {section: {net for (ref,_),net in expected.items() if parts[ref]["section"] == section}
                    for section in ["upper","lower"]}
    assert not section_nets["upper"] & section_nets["lower"], "No electrical net may span the tabs"
    assert all(net.startswith("/UP_") for net in section_nets["upper"])
    p1 = ["+3V3","+3V3","EN","GPIO4","GPIO5","GPIO6","GPIO7","GPIO15","GPIO16","GPIO17","GPIO18",
          "GPIO8",None,None,"GPIO9","GPIO10","GPIO11","GPIO12","GPIO13","GPIO14",None,"GND"]
    p2 = ["GND",None,None,"GPIO1","GPIO2","GPIO42","GPIO41","GPIO40","GPIO39","GPIO38",None,None,
          None,"GPIO0",None,None,"GPIO47","GPIO21",None,None,"GND","GND"]
    for ref, pinout in [("J1",p1),("J7",p2)]:
        for i, net in enumerate(pinout, 1):
            assert expected.get((ref,str(i))) == ("/"+net if net else None)
        assert "PinSocket_1x22_P2.54mm_Vertical" in parts[ref]["footprint"]
    rows, columns = ["GPA5","GPA6","GPA7"], [f"GPB{i}" for i in range(6)]
    assert data["matrix_rows"] == rows and data["matrix_columns"] == columns
    for i in range(1,19):
        row, column = divmod(i-1, 6)
        assert expected[(f"SW{i}","1")] == f"/UP_{columns[column]}"
        assert expected[(f"SW{i}","2")] == expected[(f"D{i}","2")] == f"/UP_KEY_{i}_A"
        assert expected[(f"D{i}","1")] == f"/UP_{rows[row]}"
        assert parts[f"SW{i}"]["side"] == parts[f"D{i}"]["side"] == "B"
        assert parts[f"D{i}"]["footprint"] == "Diode_SMD:D_SOD-123"
    display = ["UP_3V3","UP_GND","UP_GPA4","UP_GPA3","UP_GPA2","UP_GPA1","UP_GPIO14","UP_GPIO13","UP_GPA0"]
    assert [expected[("J2",str(i))] for i in range(1,10)] == ["/"+s for s in display]
    spare = [1,2,4,5,6,7,8,9,10,11,12,15,16,17,18,21,38,39,40,41,42,47]
    assert data["expansion_gpios"] == spare
    assert [expected[("J4",str(i))] for i in range(1,25)] == ["/GND","/+3V3"]+[f"/GPIO{i}" for i in spare]
    assert expected[("J5","1")] == expected[("J7","14")] == expected[("R3","2")] == "/GPIO0"
    assert expected[("J6","1")] == expected[("J1","3")] == "/EN"
    assert expected[("J5","2")] == expected[("J6","2")] == "/GND"
    for ref, signal in [("R1","GPIO13"),("R2","GPIO14"),("R4","IO_RESET")]:
        assert not parts[ref]["dnp"]
        assert expected[(ref,"1")] == "/UP_3V3" and expected[(ref,"2")] == "/UP_"+signal
        assert parts[ref]["value"].startswith("10k" if ref == "R4" else "2.2k")
    for ref in ["C1","C2","C3"]:
        assert expected[(ref,"1")] == "/UP_3V3" and expected[(ref,"2")] == "/UP_GND"
    assert parts["C3"]["value"].startswith("100n")
    for ref in ["C1","C2","C3","R1","R2","R3","R4"]:
        assert "0805" in parts[ref]["footprint"] and "HandSolder" in parts[ref]["footprint"]
    io = [f"GPB{i}" for i in range(6)]+[None,None,"3V3","GND",None,"GPIO14","GPIO13",None,
          "GND","GND","GND","IO_RESET",None,None]+[f"GPA{i}" for i in range(8)]
    assert parts["U1"]["mpn"] == "MCP23017-E/SO"
    assert data["expander"]["address"] == 0x20
    for i, net in enumerate(io,1):
        assert expected.get(("U1",str(i))) == ("/UP_"+net if net else None), (i,net)
    inputs = columns+[f"GPA{i}" for i in range(5)]
    assert not set(inputs) & {"GPA7","GPB7"}, "MCP23017 GPA7/GPB7 cannot serve as inputs"
    signals = ["GND","+3V3","GPIO13","GPIO14"]
    with (directory / "interconnect.csv").open() as stream:
        cable = list(csv.DictReader(stream))
    assert len(cable) == 4
    for i, signal in enumerate(signals, 1):
        upper = "UP_"+signal.lstrip("+")
        assert expected[("J8",str(i))] == "/"+signal and expected[("J9",str(i))] == "/"+upper
        assert cable[i-1] == dict(pin=str(i),lower=signal,upper=upper)
        assert data["interconnect"][i-1] == dict(pin=i,lower=signal,upper=upper)
    with (directory / "key-coordinates.csv").open() as stream:
        centers = list(csv.DictReader(stream))
    assert len(centers) == 18
    for i, key in enumerate(centers):
        row, col = divmod(i, 6)
        assert key["row_pin"] == rows[row] and key["column_pin"] == columns[col]
        near(float(key["pcb_x"]), 16.875+col*19.05)
        near(float(key["pcb_y"]), 48+row*19.05)
        for coordinate, target in zip(parts[f"SW{i+1}"]["local_pcb"], [float(key["pcb_x"]),float(key["pcb_y"])]):
            near(coordinate, target)
    reports = [verify_board(directory / "workbench-s3-r01.kicad_pcb",data,expected,"panel")]
    if individual_boards:
        for view in ["upper","lower"]:
            reports.append(verify_board(individual_boards / f"{view}.kicad_pcb",data,expected,view))
    print(json.dumps(dict(result="PASS", boards=reports, mcu_application_gpios=2, expander_matrix_pins=9,
                          spare_gpios=22, independent_circuits=True, cable_pins=4,
                          status="UNROUTED DRAFT; firmware support pending; not fabrication validation"), indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("netlist", type=Path)
    parser.add_argument("--individual-boards", type=Path)
    args = parser.parse_args()
    verify(args.directory, args.netlist, args.individual_boards)
