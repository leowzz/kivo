"""Check exported KiCad connectivity and mechanical placement, not fabrication readiness."""

import argparse
import csv
import json
from pathlib import Path
import xml.etree.ElementTree as ET

import pcbnew as pcb
import wx


def verify(directory, netlist):
    app = wx.App(False)
    exported = ET.parse(netlist).getroot()
    expected = {}
    for net in exported.findall("./nets/net"):
        name = net.attrib["name"]
        if name.startswith("unconnected-"):
            continue
        for pin in net.findall("node"):
            if not pin.attrib["ref"].startswith("#"):
                expected[(pin.attrib["ref"], pin.attrib["pin"])] = name
    board = pcb.LoadBoard(str(directory / "workbench-s3-r01.kicad_pcb"))
    footprints = {f.GetReference(): f for f in board.GetFootprints()}
    actual = {}
    for ref, footprint in footprints.items():
        for pad in footprint.Pads():
            if pad.GetNumber() and pad.GetNetname():
                key = (ref, pad.GetNumber())
                if key in actual:
                    assert actual[key] == pad.GetNetname(), key
                actual[key] = pad.GetNetname()
    assert expected == actual, dict(missing=sorted(set(expected.items()) - set(actual.items())), extra=sorted(set(actual.items()) - set(expected.items())))

    rows, columns = [4, 5, 6], [7, 8, 9, 10, 11, 12]
    spare = [1, 2, 38, 39, 40, 41, 42, 47]
    p1 = ["+3V3", "+3V3", "EN", "GPIO4", "GPIO5", "GPIO6", "GPIO7",
          "GPIO15", "GPIO16", "GPIO17", "GPIO18", "GPIO8", None, None,
          "GPIO9", "GPIO10", "GPIO11", "GPIO12", "GPIO13", "GPIO14", None, "GND"]
    p2 = ["GND", None, None, "GPIO1", "GPIO2", "GPIO42", "GPIO41", "GPIO40",
          "GPIO39", "GPIO38", None, None, None, "GPIO0", None, None,
          "GPIO47", "GPIO21", None, None, "GND", "GND"]
    for ref, pinout in [("J1", p1), ("J7", p2)]:
        for i, name in enumerate(pinout, 1):
            assert actual.get((ref, str(i))) == ("/" + name if name else None), (ref, i, name)
        assert "PinSocket_1x22_P2.54mm_Vertical" in str(footprints[ref].GetFPID().GetLibItemName())
    for i in range(1, 19):
        row, column = divmod(i-1, 6)
        assert actual[(f"SW{i}", "1")] == f"/GPIO{columns[column]}"
        assert actual[(f"SW{i}", "2")] == actual[(f"D{i}", "2")] == f"/KEY_{i}_A"
        assert actual[(f"D{i}", "1")] == f"/GPIO{rows[row]}"
        assert footprints[f"SW{i}"].GetLayer() == pcb.B_Cu
        assert footprints[f"D{i}"].GetLayer() == pcb.B_Cu
        assert "D_SOD-123" in str(footprints[f"D{i}"].GetFPID().GetLibItemName())
    j2 = ["/GND", "/+3V3", "/GPIO14", "/GPIO13", "/GPIO15", "/GPIO16", "/GPIO17", "/GPIO18", "/GPIO21"]
    assert [actual[("J2", str(i))] for i in range(1, 10)] == j2
    assert [actual[("J4", str(i))] for i in range(1, 11)] == ["/GND", "/+3V3"] + [f"/GPIO{p}" for p in spare]
    assert actual[("J5", "1")] == actual[("J7", "14")] == actual[("R3", "2")] == "/GPIO0"
    assert actual[("J6", "1")] == actual[("J1", "3")] == "/EN"
    assert actual[("J5", "2")] == actual[("J6", "2")] == "/GND"
    assert footprints["R1"].IsDNP() and footprints["R2"].IsDNP()
    for ref in ["C1", "C2", "R1", "R2", "R3"]:
        assert "0805" in str(footprints[ref].GetFPID().GetLibItemName())
        assert "HandSolder" in str(footprints[ref].GetFPID().GetLibItemName())
    for ref, count, x, y, dx, dy in [
        ("J1", 22, 111.7, 55.25, 0, -2.54), ("J7", 22, 86.3, 55.25, 0, -2.54),
        ("J4", 10, 10, 3, 2.54, 0), ("J5", 2, 65, 4, 2.54, 0), ("J6", 2, 74, 4, 2.54, 0),
    ]:
        pads = {pad.GetNumber(): pad for pad in footprints[ref].Pads()}
        assert len(pads) == count
        for i in range(1, count + 1):
            pad = pads[str(i)]
            assert pad.GetAttribute() == pcb.PAD_ATTRIB_PTH
            assert pad.GetLayerSet().Contains(pcb.F_Cu) and pad.GetLayerSet().Contains(pcb.B_Cu)
            assert abs(pcb.ToMM(pad.GetDrillSize().x) - 1.0) < 0.001
            assert abs(pcb.ToMM(pad.GetDrillSize().y) - 1.0) < 0.001
            assert abs(pcb.ToMM(pad.GetPosition().x) - 50 - x - (i-1)*dx) < 0.001, (ref, i, "x")
            assert abs(pcb.ToMM(pad.GetPosition().y) - 50 - y - (i-1)*dy) < 0.001, (ref, i, "y")
    assert len(set(rows + columns + spare + [13, 14, 15, 16, 17, 18, 21])) == 24
    keepouts = [zone for zone in board.Zones() if zone.GetIsRuleArea()]
    assert len(keepouts) == 1
    zone = keepouts[0]
    assert zone.GetDoNotAllowTracks() and zone.GetDoNotAllowVias() and zone.GetDoNotAllowZoneFills()
    assert zone.GetDoNotAllowPads() and zone.GetDoNotAllowFootprints()
    assert zone.GetLayerSet().Contains(pcb.F_Cu) and zone.GetLayerSet().Contains(pcb.B_Cu)
    assert len(board.Zones()) == 1
    with (directory / "key-coordinates.csv").open() as stream:
        centers = list(csv.DictReader(stream))
    assert len(centers) == 18
    for key in centers:
        point = footprints[key["key"].replace("KEY_", "SW")].GetPosition()
        assert abs(pcb.ToMM(point.x) - 50 - float(key["pcb_x"])) < 0.001
        assert abs(pcb.ToMM(point.y) - 50 - float(key["pcb_y"])) < 0.001
    for i in range(18):
        if i % 6:
            assert abs(float(centers[i]["pcb_x"]) - float(centers[i-1]["pcb_x"]) - 19.05) < 0.001
        if i >= 6:
            assert abs(float(centers[i]["pcb_y"]) - float(centers[i-6]["pcb_y"]) - 19.05) < 0.001
    bounds = board.GetBoardEdgesBoundingBox()
    assert abs(pcb.ToMM(bounds.GetWidth()) - 126) < 0.1
    assert abs(pcb.ToMM(bounds.GetHeight()) - 135) < 0.1
    assert board.GetCopperLayerCount() == 2
    assert abs(pcb.ToMM(board.GetDesignSettings().GetBoardThickness()) - 1.6) < 0.001
    assert len(board.GetTracks()) == 0
    print(json.dumps(dict(result="PASS", connected_pins=len(actual), connected_nets=len(set(actual.values())),
                          keys=18, matrix_gpios=9, diodes=18, pitch_mm=19.05,
                          module_socket_row_spacing_mm=25.4, copper_layers=2, thickness_mm=1.6,
                          expansion_gpios=spare, external_button_headers=["BOOT GPIO0", "RESET EN"],
                          status="UNROUTED DRAFT; not fabrication validation"), indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("netlist", type=Path)
    args = parser.parse_args()
    verify(args.directory, args.netlist)
