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
    board = pcb.LoadBoard(str(directory / "workbench-r03.kicad_pcb"))
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

    for i in range(1, 19):
        assert actual[(f"SW{i}", "1")] == f"/GPIO{i}"
        assert actual[(f"SW{i}", "2")] == "/GND"
        assert footprints[f"SW{i}"].GetLayer() == pcb.B_Cu
    assert actual[("U1", "56")] == actual[("U2", "1")] == "/QSPI_SS"
    assert actual[("U1", "25")] == actual[("J3", "3")] == "/SWDIO"
    assert all(actual[("U1", pin)] == "/+1V1" for pin in ["23", "45", "50"])
    assert all(actual[("U1", pin)] == "/GND" for pin in ["19", "57"])
    assert actual[("J1", "A5")] == actual[("R1", "1")] == "/CC1"
    assert actual[("J1", "B5")] == actual[("R2", "1")] == "/CC2"
    assert actual[("R1", "2")] == actual[("R2", "2")] == "/GND"
    assert actual[("U1", "47")] == actual[("R3", "2")] == "/USB_DP"
    assert actual[("U1", "46")] == actual[("R4", "2")] == "/USB_DM"
    assert actual[("U3", "5")] == "/+3V3"
    j2 = ["/GND", "/+3V3", "/GPIO27", "/GPIO26", "/GPIO22", "/GPIO28", "/GPIO21", "/GPIO20", "/GPIO19"]
    assert [actual[("J2", str(i))] for i in range(1, 10)] == j2
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
    assert abs(pcb.ToMM(bounds.GetHeight()) - 105) < 0.1
    assert board.GetCopperLayerCount() == 4
    assert abs(pcb.ToMM(board.GetDesignSettings().GetBoardThickness()) - 1.6) < 0.001
    assert len(board.GetTracks()) == 0
    print(json.dumps(dict(result="PASS", connected_pins=len(actual), connected_nets=len(set(actual.values())),
                          keys=18, pitch_mm=19.05, copper_layers=4, thickness_mm=1.6,
                          status="UNROUTED DRAFT; not fabrication validation"), indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("netlist", type=Path)
    args = parser.parse_args()
    verify(args.directory, args.netlist)
