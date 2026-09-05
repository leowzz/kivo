# /// script
# requires-python = ">=3.13"
# dependencies = ["sexpdata==1.0.2", "PyYAML==6.0.2"]
# ///
"""Lay out the S3 schematic as controller and keyboard sheets, preserving circuits."""

import argparse
import copy
import json
import math
from pathlib import Path
import uuid

import sexpdata as sx

from generate_workbench import child, children, effects, formatted_sexpr, node, pins


NAME = "workbench-s3-r01"
UPPER_FILE = "workbench-s3-r01-upper.kicad_sch"
NAMESPACE = uuid.UUID("b4224770-7f10-497e-94ac-667b578d3f05")
UPPER_SHEET_UUID = str(uuid.uuid5(NAMESPACE, "upper-sheet"))
SCHEMATIC_VERSION = 20260306


def uid(name):
    return str(uuid.uuid5(NAMESPACE, name))


def hide(prop):
    style = child(prop, "effects")
    if not child(style, "hide"):
        style.append(node("hide", sx.Symbol("yes")))


class Page:
    def __init__(self, source, parts, section, root_id):
        self.section = section
        self.parts = {p["ref"]: p for p in parts if p["section"] == section}
        self.originals = {child(s, "property")[2]: s for s in children(source, "symbol")}
        self.libraries = {s[1]: s for s in children(child(source, "lib_symbols"), "symbol")}
        self.path = f"/{root_id}" + (f"/{UPPER_SHEET_UUID}" if section == "upper" else "")
        self.tree = node("kicad_sch", node("version", SCHEMATIC_VERSION), node("generator", "eeschema"),
                         node("generator_version", "10.0"),
                         node("uuid", root_id if section == "lower" else uid("upper-file")),
                         node("paper", "A3" if section == "upper" else "A4"),
                         node("title_block", node("title", f"Workbench S3 r01 / {section.upper()}"),
                              node("date", "2026-09-05"), node("rev", "S3 r01")),
                         copy.deepcopy(child(source, "lib_symbols")))
        self.locations = {}
        self.connected = set()
        self.counter = 0
        self.dots = set()

    def add(self, item):
        if not child(item, "uuid"):
            item.append(node("uuid", uid(f"{self.section}/{self.counter}")))
            self.counter += 1
        self.tree.append(item)

    def text(self, text, x, y, size=1.27, justify="left"):
        self.add(node("text", text, node("at", x, y, 0), effects(size, justify)))

    def group(self, title, x1, y1, x2, y2):
        self.add(node("rectangle", node("start", x1, y1), node("end", x2, y2),
                      node("stroke", node("width", 0.254), node("type", sx.Symbol("default"))),
                      node("fill", node("type", sx.Symbol("none")))))
        self.text(title, x1+3.81, y1+5.08, 1.52)

    def wire(self, *points):
        for a, b in zip(points, points[1:]):
            a, b = tuple(round(v, 6) for v in a), tuple(round(v, 6) for v in b)
            if a == b:
                continue
            assert a[0] == b[0] or a[1] == b[1], (a, b)
            self.add(node("wire", node("pts", node("xy", *a), node("xy", *b)),
                          node("stroke", node("width", 0), node("type", sx.Symbol("default")))))

    def dot(self, x, y):
        key = (round(x, 6), round(y, 6))
        if key not in self.dots:
            self.add(node("junction", node("at", *key), node("diameter", 0), node("color", 0, 0, 0, 0)))
            self.dots.add(key)

    def label(self, net, x, y, direction=0):
        if self.section == "upper":
            # Absolute global names preserve the routed PCB's existing /UP_* nets.
            self.add(node("global_label", "/"+net, node("shape", sx.Symbol("input")),
                          node("at", round(x, 6), round(y, 6), direction),
                          effects(0.9, "right" if direction in (180,270) else "left"),
                          node("property", "Intersheetrefs", "${INTERSHEET_REFS}",
                               node("at", x, y, direction),
                               node("effects", node("font", node("size", 1, 1)), node("hide", sx.Symbol("yes"))))))
        else:
            self.add(node("label", net, node("at", round(x, 6), round(y, 6), 90 if direction in (90, 270) else 0),
                          effects(1.0, "right" if direction == 180 else "left")))

    def place(self, ref, x, y, angle=0, display=None, ref_at=None, value_at=None):
        part = self.parts[ref]
        original = copy.deepcopy(self.originals[ref])
        child(original, "at")[1:] = [x, y, angle]
        child(original, "instances")[1:] = [node("project", NAME,
            node("path", self.path, node("reference", ref), node("unit", 1)))]
        symbol = self.libraries[part["lib"]]
        transformed = {}
        radians = math.radians(angle)
        for p in pins(symbol):
            px, py, direction = child(p, "at")[1:]
            transformed[child(p, "number")[1]] = (
                round(x+px*math.cos(radians)-py*math.sin(radians), 6),
                round(y-px*math.sin(radians)-py*math.cos(radians), 6),
                (direction+angle) % 360)
        self.locations[ref] = transformed
        top = min(p[1] for p in transformed.values())
        for prop in children(original, "property"):
            if prop[1] == "Reference":
                child(prop, "at")[1:] = [*(ref_at or (x, top-6.35)), angle]
                prop[-1] = effects(1.0)
            else:
                child(prop, "at")[1:] = [x, y, 0]
                hide(prop)
        self.tree.append(original)
        if display:
            self.text(display, *(value_at or (x, top-3.81)), size=1.0, justify=None)

    def pin(self, ref, number):
        self.connected.add((ref, str(number)))
        return self.locations[ref][str(number)][:2]

    def stub(self, ref, number, length=5.08):
        x, y = self.pin(ref, number)
        direction = self.locations[ref][str(number)][2]
        dx, dy = {0: (-length, 0), 90: (0, length), 180: (length, 0), 270: (0, -length)}[direction]
        end = (round(x+dx, 6), round(y+dy, 6))
        self.wire((x, y), end)
        self.label(self.parts[ref]["nets"][str(number)], *end, direction={0:180, 90:270, 180:0, 270:90}[direction])

    def remaining(self):
        for ref, part in self.parts.items():
            for number, net in part["nets"].items():
                if (ref, number) in self.connected:
                    continue
                if net is None:
                    self.add(node("no_connect", node("at", *self.locations[ref][number][:2])))
                else:
                    self.stub(ref, number)

    def flag(self, index, x, y, net=None):
        ref = f"#FLG0{index+1}"
        item = copy.deepcopy(self.originals[ref])
        child(item, "at")[1:] = [x, y, 0]
        child(item, "instances")[1:] = [node("project", NAME,
            node("path", self.path, node("reference", ref), node("unit", 1)))]
        for prop in children(item, "property"):
            child(prop, "at")[1:] = [x, y-3.81, 0]
            hide(prop)
        self.tree.append(item)
        if net:
            self.label(net, x, y)

    def save(self, output):
        if self.section == "lower":
            # A numbered child with an unnumbered root triggers KiCad's repair dialog.
            self.tree.append(node("sheet_instances", node("path", "/", node("page", "1"))))
        self.tree.append(node("embedded_fonts", sx.Symbol("no")))
        output.write_text(formatted_sexpr(self.tree)+"\n")


def lower_page(page, root_id):
    page.text("LOWER BOARD / SOCKETED ESP32-S3", 15.24, 15.24, 2.03)
    page.text("3.3 V IO  |  native core-board USB-C  |  4-wire cable to upper board", 15.24, 22.86)
    page.group("YD CORE BOARD / 2 x 22 SOCKETS", 12.7, 30.48, 172.72, 120.65)
    page.group("EXPANSION / 22 SPARE GPIO", 177.8, 30.48, 284.48, 120.65)
    page.place("J1", 76.2, 78.74, display="P1 / right socket")
    page.place("J7", 147.32, 78.74, display="P2 / left socket")
    page.place("J4", 251.46, 78.74, display="1 x 24 / 2.54 mm")
    page.text("Pin 1 is at the antenna end. USB/PSRAM/strap pins marked NC stay unused.", 17.78, 114.3, 1.0)
    page.group("EXTERNAL BOOT / RESET", 12.7, 125.73, 172.72, 170.18)
    page.group("UPPER-BOARD CABLE", 177.8, 125.73, 284.48, 163.83)
    page.place("J5", 86.36, 149.86, display="BOOT", ref_at=(86.36,139.7), value_at=(86.36,142.24))
    page.place("J6", 152.4, 149.86, display="RESET", ref_at=(152.4,139.7), value_at=(152.4,142.24))
    page.place("R3", 50.8, 142.24, display="10k", ref_at=(43.18,139.7), value_at=(43.18,143.51))
    page.wire(page.pin("R3","2"), (50.8,149.86), page.pin("J5","1"))
    page.label("GPIO0", 60.96, 149.86)
    page.stub("R3","1", 2.54)
    page.place("J8", 218.44, 148.59, display="JST XH / 4 pins")
    for i, name in enumerate(["1 GND", "2 3V3", "3 SDA / GPIO13", "4 SCL / GPIO14"]):
        page.text(name, 229.87, 146.05+i*2.54, 1.0)
    page.text("Normally-open buttons to GND; EN bias is on the core board.", 17.78, 165.1, 1.0)
    page.text("J8 pins 1-4 -> J9 pins 1-4 (1:1)", 182.88, 160.02, 1.0)
    page.flag(0, 66.04, 53.34)
    page.flag(1, 137.16, 53.34)
    page.add(node("sheet", node("at", 68.58, 181.61), node("size", 38.1, 12.7),
                  node("stroke", node("width", 0.254), node("type", sx.Symbol("default"))),
                  node("fill", node("color", 0, 0, 0, 0)), node("uuid", UPPER_SHEET_UUID),
                  node("property", "Sheetname", "Upper", node("at", 68.58, 179.07, 0), effects(1.27,"left")),
                  node("property", "Sheetfile", UPPER_FILE, node("at", 68.58, 195.58, 0),
                       node("effects", node("font", node("size",1,1)), node("hide", sx.Symbol("yes")))),
                  node("instances", node("project", NAME, node("path", f"/{root_id}", node("page", "2"))))))
    page.text("Keyboard + display", 72.39, 187.96, 1.0)
    page.remaining()


def upper_page(page):
    page.text("UPPER BOARD / I2C INPUTS, DISPLAY & 3 x 6 KEY MATRIX", 15.24, 15.24, 2.03)
    page.group("J9 / POWER ENTRY", 12.7, 25.4, 76.2, 127)
    page.group("MCP23017 / ADDRESS 0x20", 81.28, 25.4, 264.16, 127)
    page.group("DISPLAY / SH1106 + EC11 + KEYS", 269.24, 25.4, 406.4, 127)
    page.place("J9", 50.8, 60.96, display="JST XH / from J8")
    page.place("C1", 33.02, 95.25, display="10u", ref_at=(26.67,92.71), value_at=(26.67,96.52))
    page.place("C2", 58.42, 95.25, display="100n", ref_at=(64.77,92.71), value_at=(64.77,96.52))
    page.wire(page.pin("C1","1"), (33.02,86.36), (58.42,86.36), page.pin("C2","1"))
    page.wire(page.pin("C1","2"), (33.02,104.14), (58.42,104.14), page.pin("C2","2"))
    page.flag(2,33.02,86.36,"UP_3V3")
    page.flag(3,33.02,104.14,"UP_GND")
    page.text("1:1 cable / GND, 3V3, SDA, SCL",17.78,118.11,0.95)
    page.place("U1", 167.64, 81.28, ref_at=(160.02,49.53))
    page.place("R1", 127, 45.72, display="2.2k", ref_at=(133.35,44.45), value_at=(133.35,48.26))
    page.place("R2", 101.6, 45.72, display="2.2k", ref_at=(107.95,44.45), value_at=(107.95,48.26))
    page.wire(page.pin("R1","1"),(127,36.83),(101.6,36.83),page.pin("R2","1"))
    page.label("UP_3V3",127,36.83)
    page.wire(page.pin("R1","2"),(127,63.5),page.pin("U1","13"))
    page.label("UP_GPIO13",127,63.5,180)
    page.wire(page.pin("R2","2"),(101.6,60.96),page.pin("U1","12"))
    page.label("UP_GPIO14",101.6,60.96,180)
    page.place("C3",205.74,50.8,display="100n",ref_at=(213.36,49.53),value_at=(213.36,53.34))
    page.wire(page.pin("U1","9"),(167.64,40.64),(205.74,40.64),page.pin("C3","1"))
    page.label("UP_3V3",167.64,40.64)
    page.stub("C3","2")
    page.place("R4",127,74.93,display="10k",ref_at=(139.7,72.39),value_at=(139.7,76.2))
    page.wire(page.pin("R4","1"),(119.38,71.12))
    page.label("UP_3V3",119.38,71.12,180)
    page.wire(page.pin("R4","2"),(127,83.82),page.pin("U1","18"))
    page.label("UP_IO_RESET",127,83.82,180)
    address_ys = [page.pin("U1",n)[1] for n in (15,16,17)]
    for n,y in zip((15,16,17),address_ys):
        page.wire(page.pin("U1",n),(147.32,y))
    page.wire(*[(147.32,y) for y in address_ys],(147.32,111.76),(167.64,111.76),page.pin("U1",10))
    for y in address_ys[1:]:
        page.dot(147.32,y)
    page.label("UP_GND",167.64,111.76)
    page.text("A0-A2 = GND  |  poll inputs; INTA/INTB unused",86.36,120.65,1.0)
    page.place("J2",335.28,66.04,display="9-pin module harness")
    for i,name in enumerate(["VCC / 3.3 V", "GND", "BAK / Back", "TRB / Encoder B", "TRA / Encoder A", "PSH / Encoder press", "SCL", "SDA", "CON / Confirm"]):
        page.text(name,345.44,55.88+i*2.54,1.0)
    page.text("Module front view: pin 1 is rightmost.",274.32,99.06,1.0)
    page.text("Header pitch 2.54 mm: measure before ordering.",274.32,105.41,1.0)
    page.text("Confirm OLED address 0x3C / 0x3D and pull-ups.",274.32,111.76,1.0)
    page.group("KEY MATRIX / ROWS GPA5-GPA7 / COLUMNS GPB0-GPB5",12.7,132.08,406.4,248.92)
    for col in range(6):
        x = round(43.18+60.96*col,6)
        page.label(f"UP_GPB{col}",x,148.59,90)
        for row in range(3):
            y = round(154.94+30.48*row,6)
            page.wire((x,148.59 if row == 0 else round(y-30.48,6)),(x,y))
            index = row*6+col+1
            sw,diode = f"SW{index}",f"D{index}"
            page.place(sw,x+10.16,y,ref_at=(x+10.16,y-5.08))
            page.place(diode,x+20.32,y+8.89,angle=90,ref_at=(x+27.94,y+8.89))
            page.wire((x,y),page.pin(sw,1))
            page.wire(page.pin(sw,2),(x+20.32,y),page.pin(diode,2))
            page.label(f"UP_KEY_{index}_A",x+20.32,y)
            page.wire(page.pin(diode,1),(x+20.32,y+21.59))
            if row < 2:
                page.dot(x,y)
            if col < 5:
                page.dot(x+20.32,y+21.59)
    for row in range(3):
        y = round(176.53+30.48*row,6)
        xs = [round(63.5+60.96*col,6) for col in range(6)]
        page.wire((20.32,y),*[(x,y) for x in xs])
        page.label(f"UP_GPA{row+5}",20.32,y,90)
    page.text("SW1-SW18: MX hot-swap. D1-D18: 1N4148W; cathode stripe faces the row.",17.78,243.84,1.0)
    page.text("Select one row LOW; inactive rows HIGH. Read columns with pull-ups.",15.24,267.97,1.0)
    page.text("GPA0-GPA4: display controls. GPA7 is output-only; GPB6/GPB7 unused.",15.24,274.32,1.0)
    page.text("Upper and lower nets join through the external J8/J9 cable only.",15.24,280.67,1.0)
    page.remaining()


def write_schematic_pages(source, parts, output):
    root_id = child(source,"uuid")[1]
    lower = Page(source,parts,"lower",root_id)
    upper = Page(source,parts,"upper",root_id)
    lower_page(lower,root_id)
    upper_page(upper)
    lower.save(output/f"{NAME}.kicad_sch")
    upper.save(output/UPPER_FILE)
    return {"lower": {"path": lower.path, "file": f"{NAME}.kicad_sch"},
            "upper": {"path": upper.path, "file": UPPER_FILE}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source",type=Path)
    parser.add_argument("output",type=Path)
    args = parser.parse_args()
    if any((args.output/name).exists() for name in (f"{NAME}.kicad_sch",UPPER_FILE)):
        parser.error("Output schematic exists; choose a new directory")
    source = sx.loads(args.source.read_text())
    for sheet in children(source,"sheet"):
        filename = next(p[2] for p in children(sheet,"property") if p[1] == "Sheetfile")
        nested = sx.loads((args.source.parent/filename).read_text())
        source.extend(children(nested,"symbol"))
    manifest = json.loads((args.source.parent/"placement.json").read_text())
    args.output.mkdir(parents=True,exist_ok=True)
    write_schematic_pages(source,manifest["parts"],args.output)


if __name__ == "__main__":
    main()
