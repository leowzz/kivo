# /// script
# requires-python = ">=3.13"
# dependencies = ["sexpdata==1.0.2", "PyYAML==6.0.2"]
# ///
"""Generate the review schematic and placement manifest, never fabrication files."""

from __future__ import annotations

import argparse
import ast
import copy
import csv
import json
from pathlib import Path
import uuid

import sexpdata as sx
import yaml


ROOT = Path(__file__).resolve().parents[2]
PRODUCT = ROOT / "products/kivo-workbench-rp-k18-disp-encp-r02/product.yaml"
MECHANICAL = ROOT / "scripts/modeling/integrated_workstation.py"
NAME = "workbench-r03"
NAMESPACE = uuid.UUID("8f786486-1599-49a3-aeca-071bb31e10e1")


def uid(name):
    return str(uuid.uuid5(NAMESPACE, name))


def node(name, *values):
    return [sx.Symbol(name), *values]


def children(value, name):
    return [item for item in value if isinstance(item, list) and item and str(item[0]) == name]


def child(value, name):
    return next(iter(children(value, name)), None)


def effects(size=1.0, justify=None):
    value = node("effects", node("font", node("size", size, size)))
    if justify:
        value.append(node("justify", sx.Symbol(justify)))
    return value


def formatted_sexpr(value, depth=0):
    if not isinstance(value, list) or not any(isinstance(v, list) for v in value):
        return sx.dumps(value)
    prefix = []
    remaining = iter(value)
    for item in remaining:
        if isinstance(item, list):
            nested = [item, *remaining]
            break
        prefix.append(sx.dumps(item))
    lines = ["(" + " ".join(prefix)]
    lines.extend("  " * (depth + 1) + formatted_sexpr(item, depth + 1) for item in nested)
    lines.append("  " * depth + ")")
    return "\n".join(lines)


class Symbols:
    def __init__(self, library_root):
        self.root = library_root
        self.libraries = {}
        self.used = {}

    def get(self, lib_id):
        if lib_id in self.used:
            return self.used[lib_id]
        library, name = lib_id.split(":")
        if library not in self.libraries:
            data = sx.loads((self.root / f"{library}.kicad_sym").read_text())
            self.libraries[library] = {s[1]: s for s in children(data, "symbol")}
        source = copy.deepcopy(self.libraries[library][name])
        parent = child(source, "extends")
        if parent:
            base = copy.deepcopy(self.get(f"{library}:{parent[1]}"))
            for section in children(base, "symbol"):
                section[1] = name + section[1][len(parent[1]):]
            overridden = {p[1] for p in children(source, "property")}
            base = [v for v in base if not (isinstance(v, list) and str(v[0]) == "property" and v[1] in overridden)]
            base.extend(children(source, "property"))
            source = base
        source[1] = lib_id
        self.used[lib_id] = source
        return source


def pins(symbol):
    return [p for unit in children(symbol, "symbol") for p in children(unit, "pin")]


def literal_model_constants():
    values = {}
    for statement in ast.parse(MECHANICAL.read_text()).body:
        if isinstance(statement, ast.Assign) and len(statement.targets) == 1:
            target = statement.targets[0]
            if isinstance(target, ast.Name):
                try:
                    values[target.id] = ast.literal_eval(statement.value)
                except (ValueError, TypeError):
                    pass
    return values


def generate(library_root, output):
    product = yaml.safe_load(PRODUCT.read_text())
    profile = product["hardware_profile"]
    key_map = profile["inputs"][0]["keys"]
    display = profile["sh1106"]
    control = display["control_panel"]
    assert key_map == {f"KEY_{i}": i for i in range(1, 19)}
    assert len(set(key_map.values()) | {display["sda"], display["scl"], *[v for k, v in control.items() if k != "type"]}) == 25
    constants = literal_model_constants()
    symbols = Symbols(library_root)
    parts = []

    def add(ref, lib, value, nets, schematic, pcb, footprint=None, angle=0, side="F", mpn=""):
        symbol = symbols.get(lib)
        symbol_pins = {child(p, "number")[1]: child(p, "name")[1] for p in pins(symbol)}
        if set(nets) != set(symbol_pins):
            raise ValueError(f"{ref}: pins {symbol_pins}, supplied {nets}")
        if footprint is None:
            footprint = next(p[2] for p in children(symbol, "property") if p[1] == "Footprint")
        part = dict(ref=ref, lib=lib, value=value, nets=nets, schematic=schematic,
                    pcb=pcb, footprint=footprint, angle=angle, side=side,
                    uuid=uid(ref), mpn=mpn)
        parts.append(part)
        return part

    rp = symbols.get("MCU_RaspberryPi:RP2040")
    rp_nets = {}
    for p in pins(rp):
        number, name = child(p, "number")[1], child(p, "name")[1]
        if name.startswith("GPIO"):
            gpio = int(name.removeprefix("GPIO").split("/")[0])
            rp_nets[number] = f"GPIO{gpio}"
        elif name in ("IOVDD", "USB_VDD", "ADC_AVDD", "VREG_VIN"):
            rp_nets[number] = "+3V3"
        elif name in ("DVDD", "VREG_VOUT"):
            rp_nets[number] = "+1V1"
        elif name == "TESTEN":
            rp_nets[number] = "GND"
        else:
            rp_nets[number] = name.removeprefix("~{").removesuffix("}")
    add("U1", "MCU_RaspberryPi:RP2040", "RP2040", rp_nets, (130, 115), (99, 25), mpn="RP2040")
    add("U2", "Memory_Flash:W25Q128JVS", "W25Q128JVSIQ / 16MiB",
        {"1": "QSPI_SS", "2": "QSPI_SD1", "3": "QSPI_SD2", "4": "GND", "5": "QSPI_SD0", "6": "QSPI_SCLK", "7": "QSPI_SD3", "8": "+3V3"},
        (245, 120), (84, 25), mpn="W25Q128JVSIQ")
    add("U3", "Regulator_Linear:AP2112K-3.3", "AP2112K-3.3",
        {"1": "VBUS", "2": "GND", "3": "VBUS", "4": None, "5": "+3V3"},
        (400, 65), (114, 34), mpn="AP2112K-3.3TRG1")
    add("J1", "Connector:USB_C_Receptacle_USB2.0_16P", "USB-C USB2.0",
        {"A1": "GND", "A4": "VBUS", "A5": "CC1", "A6": "USB_CONN_DP", "A7": "USB_CONN_DM", "A8": None, "A9": "VBUS", "A12": "GND",
         "B1": "GND", "B4": "VBUS", "B5": "CC2", "B6": "USB_CONN_DP", "B7": "USB_CONN_DM", "B8": None, "B9": "VBUS", "B12": "GND", "SH": "GND"},
        (270, 55), (115, 3.675), "Connector_USB:USB_C_Receptacle_GCT_USB4105-xx-A_16P_TopMnt_Horizontal", angle=180, mpn="GCT USB4105-GF-A")
    add("U4", "Power_Protection:USBLC6-2SC6", "USBLC6-2SC6",
        {"1": "USB_CONN_DP", "2": "GND", "3": "USB_CONN_DM", "4": "USB_CONN_DM", "5": "VBUS", "6": "USB_CONN_DP"},
        (355, 120), (113, 12), mpn="USBLC6-2SC6")
    add("Y1", "Device:Crystal_GND24", "12MHz ABM8-272-T3",
        {"1": "XIN", "2": "GND", "3": "XTAL_OUT", "4": "GND"},
        (250, 205), (99, 13), "Crystal:Crystal_SMD_3225-4Pin_3.2x2.5mm", mpn="ABM8-272-T3")

    def resistor(ref, value, a, b, sch, pcb, angle=0):
        return add(ref, "Device:R", value, {"1": a, "2": b}, sch, pcb,
                   "Resistor_SMD:R_0603_1608Metric", angle=angle)

    resistor("R1", "5.1k 1%", "CC1", "GND", (330, 45), (107, 8))
    resistor("R2", "5.1k 1%", "CC2", "GND", (355, 45), (122, 8))
    resistor("R3", "27", "USB_CONN_DP", "USB_DP", (400, 120), (106, 22))
    resistor("R4", "27", "USB_CONN_DM", "USB_DM", (430, 120), (106, 25))
    resistor("R5", "1k", "XOUT", "XTAL_OUT", (290, 205), (103, 13))
    resistor("R6", "10k", "+3V3", "QSPI_SS", (200, 120), (86, 18))
    resistor("R7", "1k", "QSPI_SS", "BOOT_BUTTON", (200, 175), (80, 18))
    resistor("R8", "10k", "+3V3", "RUN", (355, 195), (90, 38))
    resistor("R9", "4.7k DNP", "+3V3", f"GPIO{display['sda']}", (430, 195), (77, 44))
    resistor("R10", "4.7k DNP", "+3V3", f"GPIO{display['scl']}", (465, 195), (80, 44))
    for ref in ("R9", "R10"):
        next(p for p in parts if p["ref"] == ref)["dnp"] = True

    def cap(ref, value, supply, sch, pcb, size="0402"):
        footprint = f"Capacitor_SMD:C_{size}_{'1005' if size == '0402' else '1608'}Metric"
        return add(ref, "Device:C", value, {"1": supply, "2": "GND"}, sch, pcb, footprint)

    cap("C1", "1u 10V X7R", "VBUS", (440, 60), (110, 34), "0603")
    cap("C2", "4.7u 10V X7R", "+3V3", (480, 60), (118, 34), "0603")
    cap("C3", "100n 16V X7R", "+3V3", (285, 120), (84, 31))
    cap("C4", "15p C0G", "XIN", (215, 205), (95, 13))
    cap("C5", "15p C0G", "XTAL_OUT", (320, 205), (99, 9))
    cap("C6", "1u 10V X7R", "+3V3", (55, 205), (104, 30))
    cap("C7", "1u 10V X7R", "+1V1", (90, 205), (101, 31))
    decoupling = [(95, 20), (99, 20), (102, 20), (105, 28), (93, 29), (97, 31), (93, 25), (96, 32), (92, 23), (102, 33), (94, 32)]
    for offset, position in enumerate(decoupling):
        supply = "+1V1" if offset >= 9 else "+3V3"
        cap(f"C{offset+8}", "100n 16V X7R", supply, (40 + 45 * offset, 245), position)

    switch_fp = "Button_Switch_SMD:SW_SPST_TL3342"
    add("SW19", "Switch:SW_Push", "BOOTSEL", {"1": "BOOT_BUTTON", "2": "GND"}, (235, 175), (82, 8), switch_fp)
    add("SW20", "Switch:SW_Push", "RESET", {"1": "RUN", "2": "GND"}, (390, 195), (91, 8), switch_fp)
    display_signals = ["GND", "+3V3", f"GPIO{display['scl']}", f"GPIO{display['sda']}",
                       f"GPIO{control['confirm']}", f"GPIO{control['encoder_press']}",
                       f"GPIO{control['encoder_a']}", f"GPIO{control['encoder_b']}", f"GPIO{control['back']}"]
    add("J2", "Connector_Generic:Conn_01x09", "DISPLAY HARNESS - SEE README",
        {str(i+1): net for i, net in enumerate(display_signals)}, (515, 135), (73, 16),
        "Connector_PinHeader_2.54mm:PinHeader_1x09_P2.54mm_Vertical")
    add("J3", "Connector_Generic:Conn_01x04", "SWD 3V3 GND SWDIO SWCLK",
        {"1": "+3V3", "2": "GND", "3": "SWDIO", "4": "SWCLK"}, (515, 195), (120, 20),
        "Connector_PinHeader_2.54mm:PinHeader_1x04_P2.54mm_Vertical")
    add("J4", "Connector_Generic:Conn_01x07", "EXPANSION / 3V3 IO",
        {"1": "GND", "2": "+3V3", "3": "GPIO0", "4": "GPIO23", "5": "GPIO24", "6": "GPIO25", "7": "GPIO29"},
        (515, 75), (10, 3), "Connector_PinHeader_2.54mm:PinHeader_1x07_P2.54mm_Vertical", angle=90)
    add("J5", "Connector_Generic:Conn_01x02", "EXTERNAL BOOT BUTTON",
        {"1": "BOOT_BUTTON", "2": "GND"}, (300, 175), (97.5, 4),
        "Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical", angle=90)
    add("J6", "Connector_Generic:Conn_01x02", "EXTERNAL RESET BUTTON",
        {"1": "RUN", "2": "GND"}, (390, 170), (104, 4),
        "Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical", angle=90)
    key_centers = []
    for index in range(18):
        row, column = divmod(index, 6)
        model_row = constants["KEY_ROWS"] - 1 - row
        panel_x = constants["KEY_X0"] + (column+0.5)*constants["KEY_PITCH"] - constants["PANEL_X0"]
        panel_y = constants["KEY_Y0"] + (model_row+0.5)*constants["KEY_PITCH"] - constants["PANEL_Y0"]
        # Panel Y points towards the rear; KiCad top-view Y points towards the front.
        pcb_x, pcb_y = panel_x - 3.0, 113.0 - panel_y
        key_centers.append(dict(key=f"KEY_{index+1}", gpio=index+1, panel_x=panel_x, panel_y=panel_y, pcb_x=pcb_x, pcb_y=pcb_y))
        add(f"SW{index+1}", "Switch:SW_Push", f"KEY_{index+1} MX HOTSWAP",
            {"1": f"GPIO{index+1}", "2": "GND"}, (55 + column*90, 290 + row*35),
            (pcb_x, pcb_y), "Workbench:keyswitch_cherrymx_hotswap_1u", side="B")

    root_id = uid("sheet")
    schematic = node("kicad_sch", node("version", 20250114), node("generator", "eeschema"),
                     node("uuid", root_id), node("paper", "A2"),
                     node("title_block", node("title", "Kivo Workbench"),
                          node("rev", "r03 DRAFT"), node("date", "2026-09-05")))
    schematic.append(node("lib_symbols", *[value for _, value in sorted(symbols.used.items())]))
    for part in parts:
        symbol = symbols.get(part["lib"])
        x, y = [round(round(v / 2.54) * 2.54, 6) for v in part["schematic"]]
        sp = pins(symbol)
        ymax = max(child(p, "at")[2] for p in sp)
        instance = node("symbol", node("lib_id", part["lib"]), node("at", x, y, 0), node("unit", 1),
                        node("in_bom", sx.Symbol("yes")), node("on_board", sx.Symbol("yes")),
                        node("dnp", sx.Symbol("yes" if part.get("dnp") else "no")), node("uuid", part["uuid"]))
        for offset, (name, value) in enumerate([( "Reference", part["ref"]), ("Value", part["value"]), ("Footprint", part["footprint"])]):
            prop = node("property", name, value, node("at", x, y-ymax-14+offset*2, 0), effects(1.0))
            if name == "Footprint":
                prop[-1].append(node("hide", sx.Symbol("yes")))
            instance.append(prop)
        instance.append(node("instances", node("project", NAME, node("path", f"/{root_id}", node("reference", part["ref"]), node("unit", 1)))))
        schematic.append(instance)
        seen = set()
        for p in sp:
            number = child(p, "number")[1]
            px, py, direction = child(p, "at")[1:]
            px, py = round(x+px, 6), round(y-py, 6)
            net = part["nets"][number]
            if (px, py, net) in seen:
                continue
            seen.add((px, py, net))
            if net is None:
                schematic.append(node("no_connect", node("at", px, py), node("uuid", uid(f"{part['ref']}.{number}.nc"))))
                continue
            dx, dy = {0: (-5.08, 0), 90: (0, 5.08), 180: (5.08, 0), 270: (0, -5.08)}[direction]
            ex, ey = round(px+dx, 6), round(py+dy, 6)
            schematic.append(node("wire", node("pts", node("xy", px, py), node("xy", ex, ey)),
                                  node("stroke", node("width", 0), node("type", sx.Symbol("default"))), node("uuid", uid(f"{part['ref']}.{number}.wire"))))
            label_angle = 90 if dx == 0 else 0
            schematic.append(node("label", net, node("at", ex, ey, label_angle), effects(0.9, "left" if dx >= 0 else "right"), node("uuid", uid(f"{part['ref']}.{number}.label"))))
    for index, (net, x) in enumerate([("VBUS", 40.64), ("GND", 66.04)]):
        symbols.get("power:PWR_FLAG")
        instance = node("symbol", node("lib_id", "power:PWR_FLAG"), node("at", x, 50.8, 0), node("unit", 1),
                        node("in_bom", sx.Symbol("no")), node("on_board", sx.Symbol("no")), node("uuid", uid(f"flag{index}")),
                        node("property", "Reference", f"#FLG0{index+1}", node("at", x, 50, 0), node("effects", node("font", node("size", 1, 1)), node("hide", sx.Symbol("yes")))),
                        node("property", "Value", "PWR_FLAG", node("at", x, 46, 0), effects()),
                        node("instances", node("project", NAME, node("path", f"/{root_id}", node("reference", f"#FLG0{index+1}"), node("unit", 1)))))
        schematic.append(instance)
        schematic.append(node("label", net, node("at", x, 50.8, 0), effects(), node("uuid", uid(f"flaglabel{index}"))))
    child(schematic, "lib_symbols").append(symbols.get("power:PWR_FLAG"))
    for index, (text, position) in enumerate([
        ("REVIEW DRAFT: PCB placement only; no routing or fabrication release.", (35, 25)),
        ("J2 is the MAIN BOARD harness order, not a verified module mating pinout. 3.3V only.", (35, 380)),
        ("R9/R10: DNP until the module's existing I2C pull-ups are checked. Direct keys use firmware pull-ups.", (35, 387)),
        ("GPIO map follows the r02 product YAML. New PCB identity/firmware target and enclosure fit remain to be verified.", (35, 394)),
    ]):
        schematic.append(node("text", text, node("at", *position, 0), effects(1.27, "left"), node("uuid", uid(f"note{index}"))))
    schematic.append(node("embedded_fonts", sx.Symbol("no")))
    output.mkdir(parents=True, exist_ok=True)
    (output / f"{NAME}.kicad_sch").write_text(formatted_sexpr(schematic) + "\n")
    (output / "placement.json").write_text(json.dumps(dict(name=NAME, sheet_uuid=root_id, width=126, height=105, parts=parts, keys=key_centers), indent=2) + "\n")
    with (output / "key-coordinates.csv").open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(key_centers[0]))
        writer.writeheader()
        writer.writerows(key_centers)
    with (output / "bom-draft.csv").open("w", newline="") as stream:
        writer = csv.writer(stream)
        writer.writerow(["Reference", "Value", "Footprint", "MPN", "DNP"])
        for p in parts:
            writer.writerow([p["ref"], p["value"], p["footprint"], p["mpn"], p.get("dnp", False)])
    print(f"Generated {len(parts)} components and {len(key_centers)} key positions in {output}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--symbols", type=Path, default=Path("/Applications/KiCad/KiCad.app/Contents/SharedSupport/symbols"))
    parser.add_argument("--output", type=Path, default=ROOT / "hardware/workbench-r03")
    args = parser.parse_args()
    if (args.output / f"{NAME}.kicad_sch").exists():
        parser.error("Schematic exists; choose --output with a new directory to preserve manual edits.")
    generate(args.symbols, args.output)
