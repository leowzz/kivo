# /// script
# requires-python = ">=3.13"
# dependencies = ["sexpdata==1.0.2", "PyYAML==6.0.2"]
# ///
"""Generate the standalone 18-key I2C schematic and placement manifest."""

import argparse
import copy
import csv
import json
from pathlib import Path
import shutil
import uuid

import sexpdata as sx

from generate_workbench import ROOT, child, children
import layout_workbench_s3_schematic as layout


NAME = "keypad-i2c-k18-r01"
NAMESPACE = uuid.UUID("248e6fc3-ea47-4381-b5b1-707458e32ad2")
SOURCE = ROOT / "hardware/workbench-s3-r01"


def uid(name):
    return str(uuid.uuid5(NAMESPACE, name))


def generate(output):
    output.mkdir(parents=True, exist_ok=True)
    if (output / f"{NAME}.kicad_sch").exists():
        raise ValueError("Schematic exists; generate into a new directory")
    original = json.loads((SOURCE / "placement.json").read_text())
    source = sx.loads((SOURCE / "workbench-s3-r01-upper.kicad_sch").read_text())
    originals = {child(s, "property")[2]: s for s in children(source, "symbol")}
    rename = {"J9": "J1", "C3": "C2", "R4": "R3"}
    keep = {"J9", "U1", "C1", "C3", "R1", "R2", "R4"}
    keep |= {f"{prefix}{i}" for prefix in ("SW", "D") for i in range(1, 19)}
    parts, instances, keys = [], [], []

    def net_name(net):
        if not net:
            return None
        name = net.removeprefix("UP_")
        return {"GPIO13": "SDA", "GPIO14": "SCL", "IO_RESET": "RESET"}.get(name, name)

    positions = {"U1": ([50, 8], 90), "J1": ([90, 6], 90),
                 "C1": ([82, 7], 0), "C2": ([52.54, 15.1], 0),
                 "R1": ([68, 5], 0), "R2": ([68, 9], 0), "R3": ([68, 13], 0)}
    for original_part in original["parts"]:
        old_ref = original_part["ref"]
        if old_ref not in keep:
            continue
        part = copy.deepcopy(original_part)
        ref = rename.get(old_ref, old_ref)
        part.update(ref=ref, uuid=uid(ref), section="lower")
        part["nets"] = {number: net_name(net) for number, net in part["nets"].items()}
        if ref == "U1":
            for number in range(21, 26):
                part["nets"][str(number)] = None
        if ref == "J1":
            part["value"] = "I2C / 3V3 / 1x4 FEMALE / 2.54mm"
            part["footprint"] = "Connector_PinSocket_2.54mm:PinSocket_1x04_P2.54mm_Vertical"
        if ref in ("R1", "R2", "R3"):
            part["footprint"] = "Resistor_SMD:R_0402_1005Metric_Pad0.72x0.64mm_HandSolder"
        if ref in ("C1", "C2"):
            part["footprint"] = "Capacitor_SMD:C_0402_1005Metric_Pad0.74x0.62mm_HandSolder"
        if ref == "C1":
            part["value"] = "10u 10V X5R"
        if ref.startswith(("SW", "D")):
            index = int(ref.removeprefix("SW").removeprefix("D")) - 1
            row, col = divmod(index, 6)
            x, y = 14.375 + col * 19.05, 26 + row * 19.05
            part.update(local_pcb=[x, y if ref.startswith("SW") else y - 6], angle=0, side="B")
            if ref.startswith("SW"):
                keys.append(dict(key=f"KEY_{index+1}", row=row, column=col,
                                 row_pin=f"GPA{row+5}", column_pin=f"GPB{col}", x=x, y=y))
        else:
            pos, angle = positions[ref]
            part.update(local_pcb=pos, angle=angle, side="F")
        part["pcb"] = part["local_pcb"]
        parts.append(part)
        symbol = copy.deepcopy(originals[old_ref])
        child(symbol, "uuid")[1] = part["uuid"]
        for prop in children(symbol, "property"):
            if prop[1] in ("Reference", "Value", "Footprint"):
                prop[2] = {"Reference": ref, "Value": part["value"], "Footprint": part["footprint"]}[prop[1]]
        instances.append(symbol)
    for index, old in enumerate(("#FLG03", "#FLG04"), 1):
        symbol = copy.deepcopy(originals[old])
        child(symbol, "uuid")[1] = uid(f"flag{index}")
        next(p for p in children(symbol, "property") if p[1] == "Reference")[2] = f"#FLG0{index}"
        instances.append(symbol)
    source = [s for s in source if not (isinstance(s, list) and str(s[0]) == "symbol")]
    source.extend(instances)
    used = {p["lib"] for p in parts} | {"power:PWR_FLAG"}
    libraries = child(source, "lib_symbols")
    libraries[1:] = [s for s in libraries[1:] if s[1] in used]
    layout.NAME, layout.NAMESPACE = NAME, NAMESPACE
    root_id = uid("sheet")
    page = layout.Page(source, parts, "lower", root_id)
    child(page.tree, "paper")[1] = "A3"
    title = child(page.tree, "title_block")
    child(title, "title")[1] = "Kivo / Standalone 18-Key I2C Keyboard"
    child(title, "rev")[1] = "r01"
    draw_schematic(page)
    page.save(output / f"{NAME}.kicad_sch")
    manifest = dict(name=NAME, sheet_uuid=root_id, width=124, height=76, pitch=19.05,
                    mounting_holes=[[4,4],[120,4],[4,72],[120,72]],
                    connector=dict(ref="J1", pitch=2.54, drill=1.0, signals=["GND","3V3","SDA","SCL"]),
                    expander=dict(part="MCP23017-E/SO", address=32,
                                  rows=["GPA5","GPA6","GPA7"], columns=[f"GPB{i}" for i in range(6)],
                                  unused=["GPA0","GPA1","GPA2","GPA3","GPA4","GPB6","GPB7"]),
                    parts=parts, keys=keys)
    (output / "placement.json").write_text(json.dumps(manifest, indent=2) + "\n")
    with (output / "bom.csv").open("w", newline="") as stream:
        writer = csv.writer(stream)
        writer.writerow(["Reference", "Value", "Footprint", "MPN"])
        writer.writerows([p["ref"], p["value"], p["footprint"], p["mpn"]] for p in parts)
    with (output / "key-coordinates.csv").open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(keys[0]))
        writer.writeheader()
        writer.writerows(keys)
    with (output / "interconnect.csv").open("w", newline="") as stream:
        writer = csv.writer(stream)
        writer.writerow(["J1 pin", "Signal", "YD ESP32-S3 header", "Header pin", "Cable"])
        writer.writerows([(1,"GND","P1",22,"male-to-female Dupont 2.54mm"),
                          (2,"3V3","P1",1,"male-to-female Dupont 2.54mm"),
                          (3,"SDA / GPIO13","P1",19,"male-to-female Dupont 2.54mm"),
                          (4,"SCL / GPIO14","P1",20,"male-to-female Dupont 2.54mm")])
    project = json.loads((SOURCE / "workbench-s3-r01.kicad_pro").read_text())
    project["meta"]["filename"] = f"{NAME}.kicad_pro"
    project["sheets"] = [[root_id, ""]]
    project["board"]["design_settings"]["drc_exclusions"] = []
    project["board"]["design_settings"]["rules"]["min_clearance"] = 0.2
    settings = project["net_settings"]
    default = next(c for c in settings["classes"] if c["name"] == "Default")
    default.update(track_width=0.25, clearance=0.2, via_diameter=0.6, via_drill=0.3)
    power = copy.deepcopy(default)
    power.update(name="Power", priority=0, track_width=0.5)
    settings["classes"] = [default, power]
    settings["netclass_patterns"] = [dict(netclass="Power", pattern=net) for net in ("/3V3","/GND")]
    (output / f"{NAME}.kicad_pro").write_text(json.dumps(project, indent=2)+"\n")
    (output / "Workbench.pretty").mkdir(exist_ok=True)
    for name in ("keyswitch_cherrymx_hotswap_1u", "MountingHole_M3_5.6mm_Head"):
        shutil.copy2(SOURCE / "Workbench.pretty" / f"{name}.kicad_mod", output / "Workbench.pretty")
    for directory in ("3dmodels", "licenses"):
        shutil.copytree(SOURCE / directory, output / directory, dirs_exist_ok=True)
    shutil.copy2(SOURCE / "fp-lib-table", output / "fp-lib-table")
    print(f"Generated {len(parts)} components, {len(keys)} keys: {output}")


def draw_schematic(page):
    page.text("STANDALONE KEYBOARD / 18 MX KEYS / I2C", 15.24, 15.24, 2.03)
    page.group("J1 / 3.3 V POWER + I2C", 12.7, 25.4, 76.2, 127)
    page.group("MCP23017 / ADDRESS 0x20", 81.28, 25.4, 264.16, 127)
    page.group("EXTERNAL ESP32-S3 / FLYING WIRES", 269.24, 25.4, 406.4, 127)
    page.place("J1",50.8,60.96,display="1x4 female / 2.54 mm")
    page.place("C1",33.02,95.25,display="10u / 10V",ref_at=(47,92.71),value_at=(47,96.52))
    page.wire(page.pin("C1",1),(33.02,86.36))
    page.wire(page.pin("C1",2),(33.02,104.14))
    page.flag(0,33.02,86.36,"3V3")
    page.flag(1,33.02,104.14,"GND")
    page.place("U1",167.64,81.28,ref_at=(160.02,49.53))
    for ref,x,pin,signal,y in [("R1",127,13,"SDA",63.5),("R2",101.6,12,"SCL",60.96)]:
        page.place(ref,x,45.72,display="2.2k",ref_at=(x+6.35,44.45),value_at=(x+6.35,48.26))
        page.wire(page.pin(ref,2),(x,y),page.pin("U1",pin))
        page.label(signal,x,y,180)
    page.wire(page.pin("R1",1),(127,36.83),(101.6,36.83),page.pin("R2",1))
    page.label("3V3",127,36.83)
    page.place("C2",205.74,50.8,display="100n",ref_at=(213.36,49.53),value_at=(213.36,53.34))
    page.wire(page.pin("U1",9),(167.64,40.64),(205.74,40.64),page.pin("C2",1))
    page.label("3V3",167.64,40.64)
    page.stub("C2",2)
    page.place("R3",127,74.93,display="10k",ref_at=(139.7,72.39),value_at=(139.7,76.2))
    page.wire(page.pin("R3",1),(119.38,71.12))
    page.label("3V3",119.38,71.12,180)
    page.wire(page.pin("R3",2),(127,83.82),page.pin("U1",18))
    page.label("RESET",127,83.82,180)
    ys = [page.pin("U1",n)[1] for n in (15,16,17)]
    for n,y in zip((15,16,17),ys):
        page.wire(page.pin("U1",n),(147.32,y))
    page.wire(*[(147.32,y) for y in ys],(147.32,111.76),(167.64,111.76),page.pin("U1",10))
    for y in ys[1:]:
        page.dot(147.32,y)
    page.label("GND",167.64,111.76)
    page.text("A0-A2 = GND; INTA/INTB unconnected; poll inputs.",86.36,120.65,1.0)
    notes = ["J1 pin 1: GND -> ESP32-S3 GND", "J1 pin 2: 3V3 -> ESP32-S3 3V3",
             "J1 pin 3: SDA -> ESP32-S3 GPIO13", "J1 pin 4: SCL -> ESP32-S3 GPIO14",
             "4 separate male-to-female Dupont wires.", "2.54 mm pitch; do not plug across arbitrary GPIOs.",
             "3.3 V only. No regulator, USB or display connector.", "Start I2C at 100 kHz with short wires.",
             "SOIC-28W / 1.27 mm exposed leads; soldering iron."]
    for index, note in enumerate(notes):
        page.text(note,274.32,43.18+index*7.62,1.0)
    page.group("KEY MATRIX / ROWS GPA5-GPA7 / COLUMNS GPB0-GPB5",12.7,132.08,406.4,248.92)
    for col in range(6):
        x = round(43.18+60.96*col,6)
        page.label(f"GPB{col}",x,148.59,90)
        for row in range(3):
            y = round(154.94+30.48*row,6)
            page.wire((x,148.59 if row == 0 else round(y-30.48,6)),(x,y))
            index = row*6+col+1
            sw,diode = f"SW{index}",f"D{index}"
            page.place(sw,x+10.16,y,ref_at=(x+10.16,y-5.08))
            page.place(diode,x+20.32,y+8.89,angle=90,ref_at=(x+27.94,y+8.89))
            page.wire((x,y),page.pin(sw,1))
            page.wire(page.pin(sw,2),(x+20.32,y),page.pin(diode,2))
            page.label(f"KEY_{index}_A",x+20.32,y)
            page.wire(page.pin(diode,1),(x+20.32,y+21.59))
            if row < 2:
                page.dot(x,y)
            if col < 5:
                page.dot(x+20.32,y+21.59)
    for row in range(3):
        y = round(176.53+30.48*row,6)
        page.wire((20.32,y),*[(round(63.5+60.96*col,6),y) for col in range(6)])
        page.label(f"GPA{row+5}",20.32,y,90)
    page.text("SW1-SW18: MX hot-swap. D1-D18: 1N4148W SOD-123; cathode stripe faces the row.",17.78,243.84,1.0)
    page.text("Select one row LOW, others HIGH. Read GPB0-GPB5 with pull-ups; debounce in firmware.",15.24,267.97,1.0)
    page.text("GPA7/GPB7 are output-only. GPA0-GPA4 and GPB6/GPB7 unused: configure defined levels.",15.24,274.32,1.0)
    page.text("Preload row latches HIGH before enabling outputs. Existing direct-GPIO firmware is not compatible.",15.24,280.67,1.0)
    page.remaining()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output",type=Path,default=ROOT / "hardware" / NAME)
    generate(parser.parse_args().output)
