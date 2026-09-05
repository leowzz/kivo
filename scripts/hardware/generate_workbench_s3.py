# /// script
# requires-python = ">=3.13"
# dependencies = ["sexpdata==1.0.2", "PyYAML==6.0.2"]
# ///
"""Generate the socketed YD ESP32-S3 carrier review draft."""

import argparse
import csv
import json
from pathlib import Path
import uuid

import sexpdata as sx
import yaml

from generate_workbench import (
    ROOT, PRODUCT, Symbols, child, children, effects, formatted_sexpr,
    literal_model_constants, node, pins,
)

NAME = "workbench-s3-r01"
NAMESPACE = uuid.UUID("3f1fb97a-87b5-4d4a-a1c3-385ad77071bf")


def uid(name):
    return str(uuid.uuid5(NAMESPACE, name))


def generate(library_root, output):
    product = yaml.safe_load(PRODUCT.read_text())
    assert sum(len(group["buttons"]) for group in product["layout"]["groups"]) == 18
    constants = literal_model_constants()
    symbols = Symbols(library_root)
    parts = []
    upper_size, lower_size = [126, 98], [126, 86]
    panel_size = [126, 187]
    rows, columns = ["GPA5", "GPA6", "GPA7"], [f"GPB{i}" for i in range(6)]
    display = dict(sda=13, scl=14, control_panel=dict(
        type="ec11_confirm_back", confirm="GPA0", encoder_press="GPA1",
        encoder_a="GPA2", encoder_b="GPA3", back="GPA4"))
    expander = dict(part="MCP23017-E/SO", address=0x20,
                    rows=rows, columns=columns, controls=display["control_panel"],
                    output_only_pins=["GPA7", "GPB7"], unused_pins=["GPB6", "GPB7"],
                    i2c_target_hz=400000, scan_target_hz=1000, oled_chunk_bytes=8)
    # Supplied module drawing: front view, display readable and encoder on the right.
    display_module = dict(
        origin=[8,3], size=[64.90,35.03],
        mounting_holes=[[2.87,2.85],[61.90,2.90],[2.95,32.06],[61.93,31.88]],
        connector=dict(ref="J2", pin1=[31.70,1.93], pin9=[11.38,1.93], pitch=2.54,
                       signals=["VCC","GND","BAK","TRB","TRA","PSH","SCL","SDA","CON"]),
        source_view="Front; encoder right; square pin 1 at the right end",
        pitch_basis="Standard 2.54 mm assumed; drawing does not explicitly dimension pitch")
    expansion = [1,2,4,5,6,7,8,9,10,11,12,15,16,17,18,21,38,39,40,41,42,47]
    # Manufacturer P1/P2 numbering: pin 1 is at the antenna end.
    p1 = ["+3V3", "+3V3", "EN", "GPIO4", "GPIO5", "GPIO6", "GPIO7",
          "GPIO15", "GPIO16", "GPIO17", "GPIO18", "GPIO8", None, None,
          "GPIO9", "GPIO10", "GPIO11", "GPIO12", "GPIO13", "GPIO14", None, "GND"]
    p2 = ["GND", None, None, "GPIO1", "GPIO2", "GPIO42", "GPIO41", "GPIO40",
          "GPIO39", "GPIO38", None, None, None, "GPIO0", None, None,
          "GPIO47", "GPIO21", None, None, "GND", "GND"]

    def upper_net(net):
        return "UP_" + net.lstrip("+") if net else None

    def add(ref, lib, value, nets, schematic, pcb, footprint, angle=0, side="F", mpn="", dnp=False, section="lower"):
        symbol = symbols.get(lib)
        symbol_pins = {child(p, "number")[1] for p in pins(symbol)}
        if set(nets) != symbol_pins:
            raise ValueError(f"{ref}: expected {symbol_pins}, supplied {nets}")
        panel_position = pcb if section == "upper" else [126-pcb[0], 187-pcb[1]]
        if section == "upper":
            nets = {pin: upper_net(net) for pin, net in nets.items()}
        part = dict(ref=ref, lib=lib, value=value, nets=nets, schematic=schematic,
                    pcb=panel_position, local_pcb=pcb, section=section,
                    footprint=footprint, angle=angle, side=side,
                    uuid=uid(ref), mpn=mpn, dnp=dnp)
        parts.append(part)
        return part

    header = "Connector_PinHeader_2.54mm:PinHeader_1x{:02d}_P2.54mm_Vertical"
    socket = "Connector_PinSocket_2.54mm:PinSocket_1x22_P2.54mm_Vertical"
    add("J1", "Connector_Generic:Conn_01x22", "YD P1 / RIGHT SOCKET / 1x22",
        {str(i+1): n for i, n in enumerate(p1)}, (105, 105), (111.7, 55.25), socket, angle=180)
    add("J7", "Connector_Generic:Conn_01x22", "YD P2 / LEFT SOCKET / 1x22",
        {str(i+1): n for i, n in enumerate(p2)}, (215, 105), (86.3, 55.25), socket, angle=180)
    control = display["control_panel"]
    j2 = ["+3V3", "GND", control["back"], control["encoder_b"],
          control["encoder_a"], control["encoder_press"],
          f"GPIO{display['scl']}", f"GPIO{display['sda']}", control["confirm"]]
    j2_position = [a+b for a,b in zip(display_module["origin"], display_module["connector"]["pin1"])]
    add("J2", "Connector_Generic:Conn_01x09", "DISPLAY / MODULE PIN 1 = 3V3",
        {str(i+1): n for i, n in enumerate(j2)}, (330, 95), j2_position, header.format(9),
        angle=-90, section="upper")
    add("J4", "Connector_Generic:Conn_01x24", "EXPANSION / 22 SPARE GPIO / 3V3 IO",
        {str(i+1): n for i, n in enumerate(["GND", "+3V3"] + [f"GPIO{p}" for p in expansion])},
        (440, 95), (10, 3), header.format(24), angle=90)
    add("J5", "Connector_Generic:Conn_01x02", "EXTERNAL BOOT / GPIO0 TO GND",
        {"1": "GPIO0", "2": "GND"}, (300, 160), (73, 4), header.format(2), angle=90)
    add("J6", "Connector_Generic:Conn_01x02", "EXTERNAL RESET / EN TO GND",
        {"1": "EN", "2": "GND"}, (430, 160), (80, 4), header.format(2), angle=90)
    for ref, supply, value, sch, pos in [
        ("C1", "+3V3", "10u 10V X7R", (80, 200), (77, 20)),
        ("C2", "+3V3", "100n 16V X7R", (130, 200), (82, 20)),
        ("C3", "+3V3", "100n 16V X7R / U1", (175, 200), (89, 24)),
    ]:
        add(ref, "Device:C", value, {"1": supply, "2": "GND"}, sch, pos,
            "Capacitor_SMD:C_0805_2012Metric_Pad1.18x1.45mm_HandSolder", section="upper")
    for ref, signal, value, sch, pos, dnp in [
        ("R1", "GPIO13", "2.2k I2C PULLUP", (220, 200), (78, 28), False),
        ("R2", "GPIO14", "2.2k I2C PULLUP", (285, 200), (83, 28), False),
        ("R3", "GPIO0", "10k", (380, 200), (72, 12), False),
        ("R4", "IO_RESET", "10k U1 RESET PULLUP", (340, 200), (111, 28), False),
    ]:
        add(ref, "Device:R", value, {"1": "+3V3", "2": signal}, sch, pos,
            "Resistor_SMD:R_0805_2012Metric_Pad1.20x1.40mm_HandSolder", dnp=dnp,
            section="lower" if ref == "R3" else "upper")
    # GPA7/GPB7 are output-only on current silicon. GPA7 drives a row; GPB7 is unused.
    io_nets = {str(i): f"GPB{i-1}" for i in range(1,7)}
    io_nets.update({"7":None, "8":None, "9":"+3V3", "10":"GND", "11":None,
                   "12":"GPIO14", "13":"GPIO13", "14":None, "15":"GND",
                   "16":"GND", "17":"GND", "18":"IO_RESET", "19":None, "20":None})
    io_nets.update({str(21+i): f"GPA{i}" for i in range(8)})
    add("U1", "Interface_Expansion:MCP23017x-x-SO", "MCP23017-E/SO / 0x20",
        io_nets, (485,215), (99,22), "Package_SO:SOIC-28W_7.5x17.9mm_P1.27mm",
        mpn="MCP23017-E/SO", section="upper")
    cable_signals = ["GND", "+3V3", "GPIO13", "GPIO14"]
    cable = [dict(pin=i+1, lower=net, upper=upper_net(net)) for i, net in enumerate(cable_signals)]
    harness = "Connector_JST:JST_XH_B4B-XH-A_1x04_P2.50mm_Vertical"
    add("J8", "Connector_Generic:Conn_01x04", "LOWER / I2C4 TO J9 / 1:1",
        {str(i+1): net for i, net in enumerate(cable_signals)}, (115, 160), (47, 25), harness)
    add("J9", "Connector_Generic:Conn_01x04", "UPPER / I2C4 TO J8 / 1:1",
        {str(i+1): net for i, net in enumerate(cable_signals)}, (515, 160), (83, 8), harness,
        side="B", section="upper")
    key_centers = []
    for index in range(18):
        row, column = divmod(index, 6)
        model_row = constants["KEY_ROWS"] - 1 - row
        panel_x = constants["KEY_X0"] + (column+0.5)*constants["KEY_PITCH"] - constants["PANEL_X0"]
        panel_y = constants["KEY_Y0"] + (model_row+0.5)*constants["KEY_PITCH"] - constants["PANEL_Y0"]
        pcb_x, pcb_y = panel_x - 3.0, 48 + row*19.05
        key_centers.append(dict(key=f"KEY_{index+1}", row_pin=rows[row], column_pin=columns[column],
                                panel_x=panel_x, panel_y=panel_y, pcb_x=pcb_x, pcb_y=pcb_y))
        key_net = f"KEY_{index+1}_A"
        sx_pos, sy_pos = 55 + column*90, 270 + row*35
        add(f"SW{index+1}", "Switch:SW_Push", f"KEY_{index+1} MX HOTSWAP",
            {"1": columns[column], "2": key_net}, (sx_pos, sy_pos), (pcb_x, pcb_y),
            "Workbench:keyswitch_cherrymx_hotswap_1u", side="B", section="upper")
        # Row sinks current. Diode stripe/cathode goes to row, anode to the switch.
        add(f"D{index+1}", "Device:D", "1N4148W / K TO ROW",
            {"1": rows[row], "2": key_net}, (sx_pos+35, sy_pos), (pcb_x, pcb_y-6),
            "Diode_SMD:D_SOD-123", side="B", mpn="1N4148W", section="upper")

    root_id = uid("sheet")
    schematic = node("kicad_sch", node("version", 20250114), node("generator", "eeschema"),
                     node("uuid", root_id), node("paper", "A2"),
                     node("title_block", node("title", "Kivo Workbench"),
                          node("rev", "S3 r01 / SPLIT"), node("date", "2026-09-05")))
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
    for index, (net, x) in enumerate([("+3V3",40.64), ("GND",66.04), ("UP_3V3",91.44), ("UP_GND",116.84)]):
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
        ("I2C4 TWO-BOARD DRAFT: J8/J9 pins 1-4 = GND 3V3 SDA SCL. MCP23017 at 0x20 shares the OLED bus.", (35, 25)),
        ("J2 matches supplied module diagram: pins 1-9 = VCC GND BAK TRB TRA PSH SCL SDA CON. VCC = 3.3V ONLY.", (35, 380)),
        ("R1/R2: 2.2k I2C pull-ups; check module parallel pull-ups. GPA5-7 rows: selected LOW, inactive HIGH; diode K to row.", (35, 387)),
        ("UP_* nets are on the upper board. Lower and upper nets join ONLY through the external cable; see interconnect.csv.", (35, 394)),
    ]):
        schematic.append(node("text", text, node("at", *position, 0), effects(1.27, "left"), node("uuid", uid(f"note{index}"))))
    schematic.append(node("embedded_fonts", sx.Symbol("no")))
    output.mkdir(parents=True, exist_ok=True)
    (output / f"{NAME}.kicad_sch").write_text(formatted_sexpr(schematic) + "\n")
    (output / "placement.json").write_text(json.dumps(dict(
        name=NAME, sheet_uuid=root_id, width=panel_size[0], height=panel_size[1],
        boards=dict(upper=dict(size=upper_size, mounting_holes=[[4,4],[122,4],[4,94],[122,94]]),
                    lower=dict(size=lower_size, mounting_holes=[[4,4],[122,4],[4,82],[122,82]])),
        panel=dict(gap=[98,101], tab_centers=[20,63,106], tab_neck_width=5,
                   mouse_bite_rows=[98.75,100.25], holes_per_row=6, drill=0.5, pitch=0.8,
                   copper_keepout=[0,96,126,103]),
        stack=dict(tilt_deg=30, upper_front_underside_z=25, lower_pcb_z=0,
                   core_top_z=20, antenna_top_z=14.5, upper_underside_parts_height=3.5,
                   upper_connector_mated_height=12, lower_connector_mated_height=12, cable_length_mm=150,
                   upper_connector_bounds=[80.05,4.1,93.45,10.85], lower_connector_bounds=[44.05,22.15,57.45,28.9]),
        matrix_rows=rows, matrix_columns=columns, interconnect=cable, expander=expander,
        display=display, display_module=display_module, expansion_gpios=expansion, module_p1=p1, module_p2=p2,
        antenna_keepout=[75, 57.15, 123, 78.39], parts=parts, keys=key_centers), indent=2) + "\n")
    with (output / "interconnect.csv").open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=["pin", "lower", "upper"])
        writer.writeheader()
        writer.writerows(cable)
    wiring = dict(schema="workbench-s3-i2c-hardware-draft", status="requires-new-firmware-support",
                  board_profile_id="yd-esp32-s3", i2c=dict(sda_gpio=13,scl_gpio=14,target_hz=400000),
                  expander=expander, display=dict(controller="SH1106",address_candidates=[0x3C,0x3D]),
                  matrix=dict(rows=rows,columns=columns,diode_direction="COL2ROW",
                              keys={k["key"]: [k["row_pin"],k["column_pin"]] for k in key_centers}))
    (output / "firmware-profile-draft.yaml").write_text(
        "# Hardware contract only, NOT a loadable Kivo product/profile schema.\n"
        "# MCP23017 scanning, encoder polling, display scheduling and rollover support are pending.\n"
        + yaml.safe_dump(wiring, sort_keys=False))
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
    parser.add_argument("--output", type=Path, default=ROOT / "hardware/workbench-s3-r01")
    args = parser.parse_args()
    if (args.output / f"{NAME}.kicad_sch").exists():
        parser.error("Schematic exists; choose --output with a new directory to preserve manual edits.")
    generate(args.symbols, args.output)
