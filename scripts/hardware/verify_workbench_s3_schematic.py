# /// script
# requires-python = ">=3.13"
# dependencies = ["sexpdata==1.0.2", "PyYAML==6.0.2"]
# ///
"""Check schematic-layout parity and PCB associations without changing design files."""

import argparse
import copy
import json
from pathlib import Path
import xml.etree.ElementTree as ET

import sexpdata as sx

from generate_workbench import child, children


def check_page_metadata(source):
    root = sx.loads(source.read_text())
    instances = child(root,"sheet_instances")
    assert instances is not None, "Missing root page number triggers KiCad's repair dialog"
    root_page = next(p for p in children(instances,"path") if p[1] == "/")
    assert child(root_page,"page")[1] == "1"
    sheets = children(root,"sheet")
    assert len(sheets) == 1
    project = child(child(sheets[0],"instances"),"project")
    assert child(child(project,"path"),"page")[1] == "2"
    upper_file = next(p[2] for p in children(sheets[0],"property") if p[1] == "Sheetfile")
    upper = sx.loads((source.parent/upper_file).read_text())
    for page in (root,upper):
        assert child(page,"version")[1] == 20260306, "Schematic must use the KiCad 10 format"
        assert child(page,"generator_version")[1] == "10.0"


def netlist(path):
    tree = ET.parse(path)
    source = sx.loads(Path(tree.findtext("./design/source")).read_text())
    root_id = child(source,"uuid")[1]
    components = {}
    paths = {}
    for part in tree.findall("./components/comp"):
        ref = part.attrib["ref"]
        components[ref] = (part.findtext("value"), part.findtext("footprint"),
                           tuple(sorted(part.find("libsource").attrib.items())),
                           tuple(sorted((p.attrib["name"],p.attrib.get("value"))
                                        for p in part.findall("property")
                                        if p.attrib["name"] not in ("Sheetname","Sheetfile"))),
                           part.findtext("tstamps"))
        paths[ref] = "/"+root_id+part.find("sheetpath").attrib["tstamps"].rstrip("/")+"/"+part.findtext("tstamps")
    connected = {}
    unconnected = set()
    for net in tree.findall("./nets/net"):
        for pin in net.findall("node"):
            if pin.attrib["ref"].startswith("#"):
                continue
            key = pin.attrib["ref"],pin.attrib["pin"]
            if net.attrib["name"].startswith("unconnected-"):
                unconnected.add(key)
            else:
                connected[key] = net.attrib["name"]
    return components, connected, unconnected, paths


def pcb_content(tree):
    result = copy.deepcopy(tree)
    for fp in children(result,"footprint"):
        fp[:] = [entry for entry in fp if not (
            isinstance(entry,list) and str(entry[0]) in ("path","sheetfile","sheetname"))]
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before",type=Path)
    parser.add_argument("after",type=Path)
    parser.add_argument("--pcb",type=Path)
    parser.add_argument("--before-pcb",type=Path)
    args = parser.parse_args()
    before,after = netlist(args.before),netlist(args.after)
    assert before[:3] == after[:3], "Components, pin membership or net names changed"
    assert len(after[0]) == 52 and len(after[1]) == 185
    if args.pcb:
        tree = sx.loads(args.pcb.read_text())
        actual = {next(p[2] for p in children(fp,"property") if p[1] == "Reference"):
                  child(fp,"path")[1] for fp in children(tree,"footprint") if child(fp,"path")}
        assert actual == after[3], "PCB symbol associations do not match the exported schematic"
        if args.before_pcb:
            original = sx.loads(args.before_pcb.read_text())
            assert pcb_content(original) == pcb_content(tree), "PCB content changed beyond schematic associations"
    check_page_metadata(Path(ET.parse(args.after).findtext("./design/source")))
    print(json.dumps(dict(result="PASS",components=52,connected_pins=185,
                          connected_nets=len(set(after[1].values())),
                          sheet_pages=["1","2"],native_kicad_10_format=True,
                          pcb_associations_checked=bool(args.pcb),
                          pcb_geometry_unchanged=bool(args.before_pcb)),indent=2))


if __name__ == "__main__":
    main()
