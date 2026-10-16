# /// script
# requires-python = ">=3.13"
# dependencies = ["Pillow==12.3.0"]
# ///
"""Reproducible point-to-point wiring drawings for the YD-RP2040 Workbench.

An ordinary isolated-pad 2.54 mm perfboard, NOT a manufactured PCB layout.
Coordinates X/Y are numbered from the component face, USB edge up.
The solder-side drawing mirrors X while retaining the original hole numbers.
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import html
import json
import re
from dataclasses import asdict, dataclass
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OUTPUT = ROOT / "output/modeling/workbench-serviceable"
GRID_COLS, GRID_ROWS, PITCH = 31, 23, 2.54
BOARD_W, BOARD_D = 80.0, 60.0
FONT = next((str(p) for p in map(Path, [
    "/System/Library/Fonts/STHeiti Medium.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
]) if p.exists()), None)
COLORS = {"keys-1": "#c7792e", "keys-2": "#228b83", "keys-3": "#886ab0",
          "screen": "#367ab6", "toggle": "#b34e79", "power": "#cb4444", "ground": "#303943"}
LEFT_SIGNALS = ["GP0", "GP1", "GND", "GP2", "GP3", "GP4", "GP5", "GND", "GP6", "GP7",
                "GP8", "GP9", "GND", "GP10", "GP11", "GP12", "GP13", "GND", "GP14", "GP15"]
RIGHT_SIGNALS = ["VOUT", "VIN", "GND", "GP23/RGB", "3V3", "GP29", "GP28", "AGND", "GP27", "GP26",
                 "RUN", "GP22", "GND", "GP21", "GP20", "GP19", "GP18", "GND", "GP17", "GP16"]
RP_PINS = {**{i + 1: {"signal": n, "xy": [4, i + 2]} for i, n in enumerate(LEFT_SIGNALS)},
           **{40 - i: {"signal": n, "xy": [11, i + 2]} for i, n in enumerate(RIGHT_SIGNALS)}}
GPIO_PIN = {int(d["signal"][2:]): p for p, d in RP_PINS.items() if re.fullmatch(r"GP\d+", d["signal"])}
SCREEN = [
    # Actual module pin numbers, verified against the user's interface photo.
    {"pin": 9, "label": "CON", "role": "确认", "gpio": 22, "j4": 8},
    {"pin": 8, "label": "SDA", "role": "屏幕数据", "gpio": 26, "j4": 3},
    {"pin": 7, "label": "SCL", "role": "屏幕时钟", "gpio": 27, "j4": 4},
    {"pin": 6, "label": "PSH", "role": "编码器按压", "gpio": 28, "j4": 7},
    {"pin": 5, "label": "TRA", "role": "编码器 A", "gpio": 21, "j4": 5},
    {"pin": 4, "label": "TRB", "role": "编码器 B", "gpio": 20, "j4": 6},
    {"pin": 3, "label": "BAK", "role": "返回", "gpio": 19, "j4": 9},
    {"pin": 2, "label": "GND", "role": "地", "gpio": None, "j4": 1},
    {"pin": 1, "label": "VCC", "role": "3.3 V 电源", "gpio": None, "j4": 2},
]


@dataclass
class Contact:
    connector: str
    pin: int
    x: int
    y: int
    net: str
    rp_pin: int | None
    cable_end: str
    state: str = "connected"


@dataclass
class Wire:
    net: str
    group: str
    points: list[list[float]]
    bare: bool = False
    note: str = "Insulated wire; solder only at the two end pads"


def contacts():
    result = []
    for row in range(3):
        for col in range(7):
            for side in range(2):
                pin = col * 2 + side + 1
                key = row * 6 + col + 1
                net = f"K{key}/GP{key}" if side == 0 else "GND"
                end = f"K{key}.{'A' if side == 0 else 'B'}"
                state = "connected"
                rp_pin = GPIO_PIN[key] if side == 0 else 38
                if col == 6:
                    net, end, rp_pin = "NC", "", None
                    state = "male pin removed; matching female cavity blocked" if pin == 13 else "guide pin present; electrically NC"
                result.append(Contact(f"J{row+1}", pin, 17 + col, 6 + row * 5 + side,
                                      net, rp_pin, end, state))
    for pin in range(1, 11):
        x, y = 17 + (pin - 1) // 2, 2 + (pin - 1) % 2
        module = next((s for s in SCREEN if s["j4"] == pin), None)
        if module:
            net = f"GP{module['gpio']}" if module["gpio"] is not None else ("3V3" if module["label"] == "VCC" else "GND")
            rp_pin = GPIO_PIN[module["gpio"]] if module["gpio"] is not None else (36 if net == "3V3" else 38)
            result.append(Contact("J4", pin, x, y, net, rp_pin, f"M1.{module['pin']} {module['label']}"))
        else:
            result.append(Contact("J4", pin, x, y, "NC", None, "",
                                  "male pin removed; matching female cavity blocked"))
    result.extend([Contact("J5", 1, 28, 2, "GND", 38, "toggle contact A / COM"),
                   Contact("J5", 2, 29, 2, "GP0", 1, "toggle contact B / selected throw")])
    return result


def wiring(ports):
    result = []
    signal_index = 0
    for c in ports:
        if c.net == "NC" or c.net == "GND":
            continue
        start = RP_PINS[c.rp_pin]["xy"]
        end = [c.x, c.y]
        if c.connector in ("J1", "J2", "J3"):
            group = f"keys-{c.connector[-1]}"
            track = 12.1 + signal_index * .16
            points = [start, [track, start[1]], [track, c.y], end]
            signal_index += 1
        elif c.connector == "J5":
            group = "toggle"
            points = [start, [start[0], .65], [c.x, .65], end]
        else:
            group = "power" if c.net == "3V3" else "screen"
            track = 15.1 + c.pin * .085
            points = [start, [track, start[1]], [track, c.y], end]
        result.append(Wire(c.net, group, points))
    # Bare ground bus only on unused X26 pads, and each row's six ground pads.
    result.append(Wire("GND", "ground", [[26, 2], [26, 22]], True,
                       "Bare tinned wire joins X26/Y02..Y22; keep these pads exclusively GND"))
    result.append(Wire("GND", "ground", [RP_PINS[38]["xy"], [26, 4]]))
    result.append(Wire("GND", "ground", [[17, 2], [16, 2], [16, 4], [26, 4]]))
    result.append(Wire("GND", "ground", [[28, 2], [26, 2]]))
    for row in range(3):
        yy = 7 + row * 5
        result.append(Wire("GND", "ground", [[17, yy], [22, yy]], True,
                           "Bridge only the six ground pads X17..X22; stop before the NC pin at X23"))
        result.append(Wire("GND", "ground", [[22, yy], [26, yy]], False,
                           "Insulated jumper crosses the X23 NC pad without soldering or contact"))
    return result


def check(ports, wires):
    occupied = {}
    for p in ports:
        assert (p.x, p.y) not in occupied
        occupied[(p.x, p.y)] = p.net
    for number, rp in RP_PINS.items():
        assert tuple(rp["xy"]) not in occupied
        occupied[tuple(rp["xy"])] = rp["signal"]
    product = (ROOT / "products/kivo-workbench-rp-k18-disp-encp-r02/product.yaml").read_text()
    keys = {int(k): int(g) for k, g in re.findall(r"^\s+KEY_(\d+):\s+(\d+)\s*$", product, re.M)}
    assert keys == {k: k for k in range(1, 19)}
    expected_fields={"sda":26,"scl":27,"confirm":22,"encoder_press":28,
                     "encoder_a":21,"encoder_b":20,"back":19}
    for field, expected in expected_fields.items():
        assert int(re.search(rf"^\s+{field}:\s+(\d+)\s*$", product, re.M)[1]) == expected
    roles={"CON":"confirm","SDA":"sda","SCL":"scl","PSH":"encoder_press",
           "TRA":"encoder_a","TRB":"encoder_b","BAK":"back"}
    for s in SCREEN:
        if s['label'] in roles:
            assert s['gpio']==expected_fields[roles[s['label']]]
    assert [s['label'] for s in SCREEN]==['CON','SDA','SCL','PSH','TRA','TRB','BAK','GND','VCC']
    assert [s['pin'] for s in SCREEN]==list(range(9,0,-1))
    gpio = [int(p.net.split("GP")[-1]) for p in ports if "GP" in p.net]
    assert len(gpio) == 26 and len(set(gpio)) == 26 and 0 in gpio and 23 not in gpio
    assert len(ports) == 54
    assert {s["pin"] for s in SCREEN} == set(range(1, 10))
    assert len({s["j4"] for s in SCREEN}) == 9
    for wire in wires:
        if not wire.bare:
            for x, y in (wire.points[0], wire.points[-1]):
                assert (x, y) in occupied or x == 26
        else:
            (x1, y1), (x2, y2) = wire.points
            for yy in range(int(y1), int(y2) + 1):
                for xx in range(int(x1), int(x2) + 1):
                    assert occupied.get((xx, yy), "GND") == "GND", (xx, yy)
    # Graph connectivity for GND: all return contacts must reach U1.P38.
    def all_ground_points(w):
        if not w.bare:
            return [tuple(w.points[0]), tuple(w.points[-1])]
        (x1, y1), (x2, y2) = w.points
        return [(x, y) for x in range(int(x1), int(x2) + 1)
                for y in range(int(y1), int(y2) + 1)]
    graph = {}
    for w in wires:
        if w.net != "GND":
            continue
        points = all_ground_points(w)
        for p in points:
            graph.setdefault(p, set()).update(points)
    seen, pending = set(), [tuple(RP_PINS[38]["xy"])]
    while pending:
        node = pending.pop()
        if node not in seen:
            seen.add(node); pending.extend(graph.get(node, ()))
    assert all((c.x, c.y) in seen for c in ports if c.net == "GND")
    # Every live connector signal/power has exactly one wire to its correct MCU pad.
    for c in ports:
        if c.net in ("GND", "NC"):
            continue
        expected_pin=GPIO_PIN[int(c.net.split('GP')[-1])] if 'GP' in c.net else 36
        assert c.rp_pin==expected_pin
        if c.connector in ('J1','J2','J3'):
            key=int(c.cable_end.split('.')[0][1:])
            assert c.net==f"K{key}/GP{keys[key]}"
        matches = [w for w in wires if w.points[-1] == [c.x, c.y]]
        assert len(matches) == 1 and matches[0].points[0] == RP_PINS[c.rp_pin]["xy"] and matches[0].net==c.net
    return {"board": "YD-RP2040, 80x60 mm isolated-pad perfboard, 31x23 hole grid",
            "contacts": len(ports), "gpio_inputs": len(gpio),
            "checks": ["18 key GPIOs and 7 display/control GPIOs match the r02 product",
                       "GP0 uses U1 physical pin 1; avoids onboard RGB GP23",
                       "all signal/power contacts have a unique correct MCU endpoint",
                       "all 20 ground returns reach U1.P38 through the ground bus",
                       "bare ground links never touch signal or NC contacts",
                       "M1 physical order and J4 mapping match the supplied interface photo"],
            "unverified": ["physical soldering, wire insulation, continuity and component fit",
                           "GP0 toggle firmware configuration is proposed, not flashed"]}


class Canvas:
    """One drawing produces both a high-resolution PNG and an editable SVG."""
    def __init__(self, width, height):
        self.image = Image.new("RGB", (width, height), "#f5f6f4")
        self.draw = ImageDraw.Draw(self.image)
        self.svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">',
                    '<rect width="100%" height="100%" fill="#f5f6f4"/>']
    def text(self, x, y, value, size=24, color="#263540", anchor=None):
        f = ImageFont.truetype(FONT, size) if FONT else ImageFont.load_default(size=size)
        if anchor == "middle":
            x -= self.draw.textlength(str(value), font=f) / 2
        self.draw.text((x, y), str(value), fill=color, font=f)
        self.svg.append(f'<text x="{x:.2f}" y="{y + size:.2f}" fill="{color}" font-family="PingFang SC,Noto Sans CJK SC,sans-serif" font-size="{size}">{html.escape(str(value))}</text>')
    def rect(self, bounds, fill="white", stroke=None, radius=0, width=2):
        self.draw.rounded_rectangle(bounds, radius, fill, stroke, width)
        x0, y0, x1, y1 = bounds
        self.svg.append(f'<rect x="{x0}" y="{y0}" width="{x1-x0}" height="{y1-y0}" rx="{radius}" fill="{fill}" stroke="{stroke or "none"}" stroke-width="{width}"/>')
    def line(self, points, color, width=4):
        self.draw.line(points, fill=color, width=width, joint="curve")
        self.svg.append(f'<polyline points="{" ".join(f"{x:.2f},{y:.2f}" for x,y in points)}" fill="none" stroke="{color}" stroke-width="{width}" stroke-linejoin="round" stroke-linecap="round"/>')
    def circle(self, x, y, r, fill, stroke=None, width=2):
        self.draw.ellipse((x-r, y-r, x+r, y+r), fill, stroke, width)
        self.svg.append(f'<circle cx="{x:.2f}" cy="{y:.2f}" r="{r}" fill="{fill}" stroke="{stroke or "none"}" stroke-width="{width}"/>')
    def save(self, stem):
        self.image.save(stem.with_suffix(".png"))
        stem.with_suffix(".svg").write_text("\n".join(self.svg + ["</svg>"]))


def color_for(c):
    if c.net == "GND": return COLORS["ground"]
    if c.net == "3V3": return COLORS["power"]
    if c.net == "NC": return "#a2a7aa"
    return COLORS[f"keys-{c.connector[-1]}"] if c.connector in ("J1", "J2", "J3") else COLORS["toggle" if c.connector == "J5" else "screen"]


def board_drawing(out, ports, wires, group="components"):
    back = group != "components"
    labels = {"components": "元件面：USB 朝上，按此孔位安装排针与排母",
              "all": "焊接面：完整接线，保持 USB 朝上，左右已镜像",
              "keys": "焊接面：18 键信号与 GND，保持 USB 朝上",
              "controls": "焊接面：屏幕编码器、3.3 V 与扭子，保持 USB 朝上"}
    canvas = Canvas(2200, 1550)
    canvas.text(50, 28, labels[group], 36)
    canvas.text(50, 84, "2.54 mm 独立焊盘洞洞板 · 31 列 × 23 行 · 外形约 80 × 60 mm · 孔号始终沿用元件面坐标", 25)
    canvas.text(50, 128, "X 为列、Y 为行；U1.P 是 RP2040 实物脚号，J1–J5 是本方案插头号。", 23)
    ox, oy, pitch = 160, 235, 35
    def xy(x, y):
        return ox + ((GRID_COLS - x) if back else (x - 1)) * pitch, oy + (y - 1) * pitch
    canvas.rect((ox-28, oy-28, ox+30*pitch+28, oy+22*pitch+28), "#e6dfc8", "#c1b79b", 18)
    for x in range(1, GRID_COLS+1):
        px, _ = xy(x, 1)
        canvas.text(px, oy-65, f"{x:02}", 16, anchor="middle")
        canvas.text(px, oy+22*pitch+35, f"{x:02}", 16, anchor="middle")
    for y in range(1, GRID_ROWS+1):
        _, py = xy(1, y)
        canvas.text(ox-66, py-10, f"Y{y:02}", 16)
        for x in range(1, GRID_COLS+1):
            px, _ = xy(x, y)
            canvas.circle(px, py, 4.5, "#faf9f3", "#baae92", 1)
    # Draw hardware before wiring so the copper-face wires remain visible.
    x1, y1 = xy(4, 2); x2, y2 = xy(11, 21)
    lo, hi = sorted((x1, x2))
    if not back:
        canvas.rect((lo-30, y1-30, hi+30, y2+30), "#d5e5df", "#77988b", 8)
        canvas.rect(((lo+hi)/2-32, y1-48, (lo+hi)/2+32, y1-13), "#a3afb6", "#647681", 4)
    for x in (4, 11):
        px, py = xy(x, 2); _, end_y = xy(x, 21)
        if not back:
            canvas.rect((px-11, py-14, px+11, end_y+14), "#35444b", radius=4)
    for p, info in RP_PINS.items():
        px, py = xy(*info["xy"])
        canvas.circle(px, py, 7, "#e1c56b", "#574d34", 2)
        signal = info["signal"]
        # Label the MCU pads on the inward side of the two socket rows.
        is_left_on_page = px == lo
        tx = px + 13 if is_left_on_page else px - 118
        canvas.text(tx, py-26, f"P{p:02} {signal}", 15)
    for name in ("J1", "J2", "J3", "J4", "J5"):
        members = [c for c in ports if c.connector == name]
        pts = [xy(c.x,c.y) for c in members]
        xlo, ylo = min(p[0] for p in pts), min(p[1] for p in pts)
        xhi, yhi = max(p[0] for p in pts), max(p[1] for p in pts)
        if not back:
            canvas.rect((xlo-14,ylo-14,xhi+14,yhi+14), "#f8f7f1", "#8b8d88", 5)
        title = {"J1":"J1 · K1–6", "J2":"J2 · K7–12", "J3":"J3 · K13–18",
                 "J4":"J4 · 屏幕/编码器", "J5":"J5 · 扭子"}[name]
        canvas.text((xlo+xhi)/2, yhi+17, title, 18, anchor="middle")
        for c in members:
            px,py=xy(c.x,c.y)
            if "removed" in c.state:
                canvas.circle(px,py,8,"#f5f6f4","#a2a7aa",2)
                canvas.line([(px-6,py-6),(px+6,py+6)],"#a2a7aa",2)
                canvas.line([(px-6,py+6),(px+6,py-6)],"#a2a7aa",2)
            else:
                canvas.circle(px,py,8,color_for(c),"#ffffff",2)
            canvas.text(px,py-28,str(c.pin),13,anchor="middle")
    if back:
        selected = [w for w in wires if w.group == "ground" or group == "all" or
                    (group == "keys" and w.group.startswith("keys-")) or
                    (group == "controls" and w.group in ("screen","toggle","power"))]
        for w in sorted(selected,key=lambda w: not w.bare):
            pts=[xy(x,y) for x,y in w.points]
            canvas.line(pts,"#fafbf8",8 if w.bare else 6)
            canvas.line(pts,COLORS[w.group],5 if w.bare else 3)
            endpoints = [pts[0],pts[-1]]
            if w.bare:
                (ax,ay),(bx,by)=w.points
                endpoints=[xy(x,y) for x in range(int(ax),int(bx)+1) for y in range(int(ay),int(by)+1)]
            for px,py in endpoints:
                canvas.circle(px,py,5,COLORS[w.group],"#ffffff",1)
    # A readable wiring table next to the physical board.
    tx=1320
    canvas.text(tx,200,"U1：YD-RP2040，USB 朝上",27)
    if group in ("components","keys","all"):
        canvas.text(tx,250,"按键插头：奇数脚接信号，偶数脚回地",22)
        canvas.text(tx,290,"键       GPIO     U1 实物脚    插头脚",21)
        for k in range(1,19):
            c=next(c for c in ports if c.cable_end==f"K{k}.A")
            canvas.text(tx,327+(k-1)*29,f"K{k:02}     GP{k:02}      P{c.rp_pin:02}        {c.connector}.{c.pin}",20,COLORS[f"keys-{(k-1)//6+1}"])
        start_y=885
    else:
        start_y=255
    canvas.text(tx,start_y,"屏幕原板 M1 → J4 → U1",24)
    for i,s in enumerate(SCREEN):
        c=next(c for c in ports if c.connector=="J4" and c.pin==s["j4"])
        net="3V3" if s["label"]=="VCC" else ("GND" if s["label"]=="GND" else f"GP{s['gpio']}")
        canvas.text(tx,start_y+43+i*29,f"{s['label']:<4}  M1.{s['pin']} → J4.{s['j4']} → P{c.rp_pin:02} {net}",19,color_for(c))
    canvas.text(70,1250,"安装：U1 两排 1×20 母排；J1–J3 用 2×7 公排；J4 用 2×5 公排；J5 用 1×2 公排。",24)
    canvas.text(70,1295,"焊线：彩线全部用绝缘软线，只焊两端；交叉处不连接。粗黑线才是焊盘之间的裸地线桥。",24)
    canvas.text(70,1340,"地线：X26/Y02–Y22 为总线；每排只桥接 X17–X22 的六个地脚，禁止桥到 X23 的空脚。",24)
    canvas.text(70,1385,"供电：只用 U1.P36 的 3V3；VOUT、VIN、GP23/RGB、RUN 和其余空脚均不接。",24)
    canvas.text(70,1430,"扭子：J5.1 → GND，J5.2 → GP0 / U1.P1；软件需配置 GP0 上拉输入，本次未修改固件。",24)
    canvas.save(out/f"perfboard-{group}")


def connector_drawing(out, ports):
    canvas=Canvas(2200,1580)
    canvas.text(45,25,"公母排插针序：先认插合面，再认焊线面",38)
    canvas.text(45,84,"公头固定在洞洞板；母头在线束上。下面两图都把插合面朝向自己，因此左右互为镜像。",26)
    def plug(x,y,name,cols,rows,pins,mirror=False,title=""):
        canvas.text(x,y,title,24)
        width=cols*112+24
        canvas.rect((x,y+48,x+width,y+48+rows*96),"#ffffff","#c9cfcd",12)
        for c in pins:
            index=(c.pin-1)//rows if rows==2 else c.pin-1
            rr=(c.pin-1)%rows if rows==2 else 0
            xx=x+68+((cols-1-index) if mirror else index)*112
            yy=y+96+rr*96
            blocked="removed" in c.state
            canvas.circle(xx,yy,13,"#d5dbdc" if blocked else color_for(c),"#6d777b",2)
            if blocked:
                canvas.line([(xx-9,yy-9),(xx+9,yy+9)],"#586770",3)
                canvas.line([(xx-9,yy+9),(xx+9,yy-9)],"#586770",3)
            canvas.text(xx,yy-40,str(c.pin),20,anchor="middle")
            desc="封孔" if blocked and mirror else ("缺针" if blocked else c.net.split('/')[0])
            if name=="J4" and not blocked:
                module=next(s for s in SCREEN if s["j4"]==c.pin)
                desc="GND" if module['label']=="GND" else f"{module['label']}/{c.net}"
            canvas.text(xx,yy+22,desc,16,anchor="middle")
    for index,name in enumerate(("J1","J2","J3")):
        y=155+index*300
        members=[c for c in ports if c.connector==name]
        plug(45,y,name,7,2,members,False,f"{name} 公头 · 插合面（1 在左上）")
        plug(930,y,name,7,2,members,True,f"{name} 母头 · 插合面（1 在右上）")
    members=[c for c in ports if c.connector=="J4"]
    plug(45,1060,"J4",5,2,members,False,"J4 公头 · 屏幕编码器 · 插合面")
    plug(740,1060,"J4",5,2,members,True,"J4 母头 · 屏幕编码器 · 插合面")
    plug(1450,1060,"J5",2,1,[c for c in ports if c.connector=="J5"],False,"J5 公头 · 扭子")
    canvas.text(45,1365,"J1–J3：拔掉公头 13 脚，封住母头同号 13 孔。14 脚保留导向，但不接电。",25)
    canvas.text(45,1410,"J4：拔掉公头 10 脚，封住母头同号 10 孔。禁止把 J4 当作原屏幕板的直通排线。",25)
    canvas.text(45,1455,"母头从焊线背面看时，左右再翻转一次；逐根以编号测通断，不按线色或外观猜顺序。",25)
    canvas.text(45,1500,"两根单排母插若拼成双排，必须固定成一体，并保留缺针/封孔定位。J5 两线接普通机械触点，无极性。",23)
    canvas.save(out/"connector-pinouts")


def screen_drawing(out, ports):
    canvas=Canvas(2200,1410)
    canvas.text(45,25,"屏幕编码器与扭子：原板丝印 → 线束 → 洞洞板 → RP2040",36)
    canvas.text(45,88,"观察方向：屏幕正面朝自己，编码器在右，排针在上。M1 表示原模块，避免与按键插头 J1 混淆。",25)
    canvas.rect((65,170,1340,500),"#d9e4ee","#7e98a9",16)
    for i,s in enumerate(SCREEN):
        xx=130+i*140
        color=COLORS["power"] if s["label"]=="VCC" else (COLORS["ground"] if s["label"]=="GND" else COLORS["screen"])
        canvas.circle(xx,220,12,color,"#ffffff",2)
        canvas.text(xx,251,s["label"],23,color,anchor="middle")
        canvas.text(xx,290,f"M1.{s['pin']}",21,anchor="middle")
    canvas.rect((115,345,750,460),"#162530",radius=8)
    canvas.text(250,374,"屏幕",31,"#ffffff")
    canvas.circle(1120,400,64,"#34434b","#81919b",4)
    canvas.text(995,475,"编码器在右侧",20)
    canvas.text(1410,188,"原理图编号是反向的",28)
    canvas.text(1410,245,"照片最左：CON = M1.9",25)
    canvas.text(1410,290,"照片最右：VCC = M1.1",25,COLORS["power"])
    canvas.text(1410,335,"右端方焊盘是 VCC / 3.3 V",25,COLORS["power"])
    canvas.text(1410,405,"先标 VCC 红色，再接其余八线。",23)
    cols=[70,300,530,850,1190,1480]
    for xx,label in zip(cols,["原板丝印","原板针号","新线束插头","RP2040 信号","实物脚号","洞洞板插头孔位"]):
        canvas.text(xx,555,label,25)
    for i,s in enumerate(SCREEN):
        c=next(c for c in ports if c.connector=="J4" and c.pin==s["j4"])
        net="3V3" if s["label"]=="VCC" else ("GND" if s["label"]=="GND" else f"GP{s['gpio']}")
        values=[s['label'],f"M1.{s['pin']}",f"J4.{s['j4']}",net,f"U1.P{c.rp_pin:02}",f"X{c.x:02}/Y{c.y:02}"]
        for xx,value in zip(cols,values):
            canvas.text(xx,610+i*45,value,25,color_for(c))
    canvas.rect((55,1060,2130,1340),"#ffffff","#d1d7d3",12)
    canvas.text(85,1083,"扭子开关：只接一对随档位通断的机械触点",29)
    canvas.text(85,1138,"J5.1（GND） ── 触点 A / COM       触点 B / 某一侧 ── J5.2（GP0 / U1.P1）",27)
    canvas.text(85,1190,"两脚开关直接各接一线；三脚或六脚开关先用万用表找 COM 和一侧，剩余触点绝缘。",25)
    canvas.text(85,1235,"配置 GP0 上拉输入后：断开为 1，导通接地为 0。这里只定义接线，快捷键总锁定功能尚未写入产品配置。",24)
    canvas.text(85,1280,"M1.VCC 只接 3.3 V；GP23 已连板载 RGB，不用于扭子。所有触点与排插均在断电状态下焊接。",24)
    canvas.save(out/"screen-toggle-wiring")


def keys_drawing(out):
    canvas=Canvas(2200,1370)
    canvas.text(45,25,"18 键与分排线束：每颗轴保留独立的信号线和地线",36)
    canvas.text(45,88,"USB / 屏幕一侧始终朝上。翻到焊接背面后，左右顺序反过来；键号不改变。",25)
    for mirrored,ox in [(False,45),(True,1140)]:
        canvas.text(ox,155,"键帽正面：从左到右读 K1–K6" if not mirrored else "焊接背面：从左到右读 K6–K1",27)
        for row in range(3):
            yy=235+row*295
            canvas.text(ox,yy-35,f"J{row+1} · {'屏幕旁一排' if row==0 else ('中间一排' if row==1 else '靠使用者一排')}",24)
            for col in range(6):
                key=row*6+col+1
                xx=ox+(5-col if mirrored else col)*166
                canvas.rect((xx,yy,xx+150,yy+205),"#ffffff","#c4ccc8",10)
                canvas.text(xx+75,yy+15,f"K{key}",30,anchor="middle")
                canvas.text(xx+75,yy+57,f"GP{key} / U1.P{GPIO_PIN[key]}",18,anchor="middle")
                color=COLORS[f"keys-{row+1}"]
                for side,(offset,net) in enumerate([(40,"信号"),(110,"GND")]):
                    px=xx+offset
                    canvas.circle(px,yy+111,8,color if side==0 else COLORS['ground'])
                    canvas.line([(px,yy+119),(px,yy+147)],color if side==0 else COLORS['ground'],4)
                    canvas.text(px,yy+149,f"J{row+1}.{col*2+side+1}",17,anchor="middle")
                    canvas.text(px,yy+176,net,16,anchor="middle")
    canvas.text(50,1160,"每颗轴只用两根金属电触点；图中触点符号不表示实际 MX 引脚位置，塑料定位柱与 LED 脚不参与。",25)
    canvas.text(50,1210,"轴与轴之间不串地线；18 根返回线分别进插头偶数脚，在固定洞洞板上才汇合。",25)
    canvas.text(50,1260,"每对软线多留 25–35 mm 局部余量；整排线束另多留 80–120 mm。插头 13 脚定位、14 脚不接电。",25)
    canvas.save(out/"keys-harness")


def write_draft(out):
    screen_rows=[]
    for s in SCREEN:
        pin=GPIO_PIN[s['gpio']] if s['gpio'] is not None else (36 if s['label']=='VCC' else 38)
        net=f"GP{s['gpio']}" if s['gpio'] is not None else ('3V3' if s['label']=='VCC' else 'GND')
        screen_rows.append(f"| {s['label']} | M1.{s['pin']} | J4.{s['j4']} | {net} | U1.P{pin} |")
    draft=f'''---
title: Workbench 洞洞板接线详图
subtitle: YD-RP2040 · 18 键 · 已核对屏幕编码器针序 · GP0 扭子
template: doc
theme: paper
mode: light
lang: zh
---

按现有 r02 的 GPIO 接法绘制，扭子新增接 GP0。
用 2.54 mm 独立焊盘洞洞板，至少 31 列 × 23 行。
所有插头、孔号和焊线都来自同一份逐点接线表。
本次只完成接线设计，未焊接、上电或修改固件。

## A 元件面：先按孔位装插座

![元件面：USB 朝上。U1 两排母座，J1–J3 三排键区公头，J4 屏幕公头，J5 扭子公头。]({out/'perfboard-components.png'})

| 对象 | 规格 | 元件面孔位 |
| --- | --- | --- |
| U1 左母座 | 1×20，2.54 mm | X04，Y02–Y21，P1 在上 |
| U1 右母座 | 1×20，2.54 mm | X11，Y02–Y21，P40 在上 |
| J1：K1–K6 | 2×7 公排 | X17–X23，Y06/Y07 |
| J2：K7–K12 | 2×7 公排 | X17–X23，Y11/Y12 |
| J3：K13–K18 | 2×7 公排 | X17–X23，Y16/Y17 |
| J4：屏幕编码器 | 2×5 公排 | X17–X21，Y02/Y03 |
| J5：扭子 | 1×2 公排 | X28/X29，Y02 |

两排 MCU 母座间距 17.78 mm。
控制板焊公排针，插入洞洞板的两排母座。
洞洞板外形约 80 × 60 mm，周边留边按实板调整。
X/Y 是孔号，不是 GPIO 编号。

## B 18 键：每颗轴单独走两根线

![左图为键帽正面，右图为焊接背面。翻面后左右镜像，键号与插头针号不改变。]({out/'keys-harness.png'})

每颗轴的一根金属触点接对应奇数脚。
另一根金属触点单独回对应偶数地脚。
两根电触点无极性，塑料定位柱和 LED 脚不参与。
六条地线只在固定洞洞板上汇合。

| 插头 | 信号脚 1/3/5/7/9/11 | 地脚 2/4/6/8/10/12 |
| --- | --- | --- |
| J1 | K1–K6 → GP1–GP6 | 每颗键独立回 GND |
| J2 | K7–K12 → GP7–GP12 | 每颗键独立回 GND |
| J3 | K13–K18 → GP13–GP18 | 每颗键独立回 GND |

每对线多留 25–35 mm 的局部 U 形余量。
整排线束另多留 80–120 mm 的维修余量。
拔键帽时托住轴壳，拆盖前先拔对应插头。

## C 焊接面：完整走线与键区分图

![完整焊接面。保持 USB 朝上，从背后看时左右已经镜像。孔号仍沿用元件面的编号。]({out/'perfboard-all.png'})

![只显示 18 根按键信号线与固定地线汇流排。]({out/'perfboard-keys.png'})

所有彩线都使用绝缘软线，只焊两端。
交叉处不连接，不把中途经过的焊盘一起焊上。
粗黑线表示裸地线桥，圆点表示焊接点。
彩线路径供整理参考，可用绝缘软线上下错开。
图示间距不作为导线外径或 PCB 走线尺寸。

主地线沿 X26/Y02–Y22 焊接，接 U1.P38。
每排只把 X17–X22 的六个地脚桥接。
再用绝缘线跨过 X23，接到 X26 主地线。
X23 是空脚位置，不能搭上裸地线。

## D 屏幕编码器：照片针序已经核对

![原模块正面从左至右：CON、SDA、SCL、PSH、TRA、TRB、BAK、GND、VCC。原理图 1 号在最右端 VCC。]({out/'screen-toggle-wiring.png'})

本方案用 M1 表示原屏幕模块。
原板标的 J1 不等于本方案的按键插头 J1。
M1 的 9 针通过独立转接线进入新的 J4。
J4 采用 2×5 定位插头，不能当作直通排线。

| 正面左到右 | 原板针号 | 新 J4 针号 | GPIO/网络 | RP2040 实物脚 |
| --- | --- | --- | --- | --- |
{chr(10).join(screen_rows)}

原板右端方焊盘是 VCC，接 3.3 V。
不接 U1 的 VOUT 或 VIN。
TRA/TRB 与当前配置的 A/B 对应关系保持一致。

## E 屏幕、电源和扭子在板背怎么走

![只显示屏幕编码器、电源、扭子及地线。蓝线为屏幕控制，红线为 3.3 V，粉线为 GP0。]({out/'perfboard-controls.png'})

J5.1 接 GND，J5.2 接 GP0，即 U1.P1。
两脚扭子各接一根线。
三脚或六脚扭子先用万用表确认 COM 与一侧。
剩余触点单独绝缘，不接电源。

配置 GP0 上拉输入后，断开读 1，导通读 0。
当前产品未加入 GP0，接线后还需配置功能。
GP23 接着板载 RGB，本方案不使用它。

## F 公母插头：插合面互为镜像

![公头和母头均以插合面朝向自己绘制，因此左右镜像。J1–J3 的定位点为 13 脚；J4 的定位点为 10 脚。]({out/'connector-pinouts.png'})

J1–J3：公头去掉 13 针，母头封住同号 13 孔。
14 针保留作导向，但不接电。
J4：公头去掉 10 针，母头封住同号 10 孔。
缺针和封孔必须使用同一个编号。

公头插合面看，1 号在左上。
母头插合面看，1 号在右上。
母头从焊线背面看，左右再翻转一次。
对接时必须 1 对 1，2 对 2，依次核对。

## G 焊接顺序与用料

1. 按元件面图焊两排 MCU 母座。
2. 焊 J1–J5，并处理缺针定位。
3. 先做粗黑 GND 总线与三排地线桥。
4. 按分图逐根焊信号线与 3.3 V 线。
5. 不插 MCU 和模块，逐脚测通断与短路。
6. 标清母头方向，再插控制板和模块测试。

| 用料 | 数量 |
| --- | --- |
| 独立焊盘洞洞板 | 31×23 孔，约 80×60 mm |
| 1×20 母排 | 2 条，MCU 座 |
| 2×7 公排/母插 | 各 3 组，键区 |
| 2×5 公排/母插 | 各 1 组，屏幕 |
| 1×2 公排/母插 | 各 1 组，扭子 |
| 28 AWG 多股软线 | 信号与每键返回线 |
| 24–26 AWG 线 | 固定地线汇流排 |

双排插头可用两条单排拼装，必须固定成一体。
若使用条形连通洞洞板，先切断不属于同一网络的铜条。
本图按独立焊盘设计。

## H 已完成的核对

| 核对项 | 状态 |
| --- | --- |
| 18 键 GPIO | ok 与现有 r02 一致 |
| 屏幕与编码器 GPIO | ok 7 路与 r02 一致 |
| 原屏幕 9 针顺序 | ok 按你提供的接口图片核对 |
| 54 个插头位置 | ok 逐点表与图纸一致 |
| 26 路 GPIO | ok 无重复占用，新增 GP0 |
| 20 条返回地线 | ok 都连到 U1.P38 |
| 裸地线桥 | ok 不碰信号脚与空脚 |
| 实物焊接与上电 | warn 尚未验证 |
| 扭子功能 | warn 接线已定义，固件配置未修改 |

图片、可编辑 SVG、逐点 CSV 和网络 JSON 一并放入模型包。
针脚图以 YD-RP2040 为准，不能按 RP2040 裸芯片脚号接线。
'''
    (out/"wiring-details.am.md").write_text(draft)


def generate(out=DEFAULT_OUTPUT, announce=True):
    out=out.resolve()
    out.mkdir(parents=True,exist_ok=True)
    ports=contacts(); wires=wiring(ports)
    verified=check(ports,wires)
    for group in ("components","all","keys","controls"):
        board_drawing(out,ports,wires,group)
    connector_drawing(out,ports)
    screen_drawing(out,ports)
    keys_drawing(out)
    write_draft(out)
    with (out/"perfboard-connections.csv").open("w",newline="") as f:
        fields=list(asdict(ports[0]))
        writer=csv.DictWriter(f,fieldnames=fields);writer.writeheader()
        writer.writerows(asdict(c) for c in ports)
    with (out/"rp2040-socket-pinout.csv").open("w",newline="") as f:
        writer=csv.writer(f);writer.writerow(["U1_physical_pin","signal","perfboard_X","perfboard_Y","view"])
        for p,info in sorted(RP_PINS.items()):
            writer.writerow([p,info["signal"],*info["xy"],"component side, USB up"])
    manifest={"verified":verified,"contacts":[asdict(c) for c in ports],
              "wires":[asdict(w) for w in wires],
              "screen_module_left_to_right":SCREEN,
              "source_image_sha256":hashlib.sha256((ROOT/"refer/display-encoder-interface.png").read_bytes()).hexdigest()}
    (out/"perfboard-wiring.json").write_text(json.dumps(manifest,ensure_ascii=False,indent=2)+"\n")
    if announce:
        print(json.dumps({"output":str(out.resolve()),**verified},ensure_ascii=False))
    return [*(f"perfboard-{name}.{ext}" for name in ("components","all","keys","controls") for ext in ("png","svg")),
            *(f"{name}.{ext}" for name in ("connector-pinouts","screen-toggle-wiring","keys-harness") for ext in ("png","svg")),
            "perfboard-connections.csv","rp2040-socket-pinout.csv","perfboard-wiring.json"]


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-root",type=Path,default=DEFAULT_OUTPUT)
    generate(parser.parse_args().output_root)
