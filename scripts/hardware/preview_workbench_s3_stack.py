# /// script
# requires-python = ">=3.13"
# dependencies = ["matplotlib==3.10.7"]
# ///
"""Draw and check a nominal side envelope; this is not a fitted enclosure model."""

import argparse
import json
import math
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Polygon, Rectangle


def generate(manifest, output):
    data = json.loads(manifest.read_text())
    stack = data["stack"]
    angle = math.radians(stack["tilt_deg"])
    sine, cosine = math.sin(angle), math.cos(angle)
    length = data["boards"]["upper"]["size"][1]
    depth = data["boards"]["lower"]["size"][1]
    front_z = stack["upper_front_underside_z"]
    rear_z = front_z + length*sine
    thickness = 1.6

    def upper_point(y, normal=0):
        return (y*cosine+normal*sine, rear_z-y*sine+normal*cosine)

    def upper_polygon(y1, y2, low, high):
        return [upper_point(y1,low), upper_point(y2,low), upper_point(y2,high), upper_point(y1,high)]

    # Distance to the infinite upper plane is a conservative clearance for the finite PCB.
    def normal_gap(y, z, underside_height=0):
        return (rear_z-z)*cosine-y*sine-underside_height

    parts_height = stack["upper_underside_parts_height"]
    core_gap = normal_gap(57.15, stack["core_top_z"], parts_height)
    antenna_gap = normal_gap(63.39, stack["antenna_top_z"], parts_height)
    ux1, uy1, ux2, uy2 = stack["upper_connector_bounds"]
    lx1, ly1, lx2, ly2 = stack["lower_connector_bounds"]
    upper_connector = upper_polygon(uy1, uy2, -stack["upper_connector_mated_height"], 0)
    connector_to_core = min(p[1] for p in upper_connector) - stack["core_top_z"]
    projected_depth = length*cosine + thickness*sine
    lower_connector_top = stack["lower_connector_mated_height"]
    assert projected_depth <= depth
    assert core_gap > 5 and connector_to_core > 5
    assert antenna_gap >= 15, "Raise the upper board or revise the antenna clearance"
    assert normal_gap(ly2, lower_connector_top, parts_height) > 5
    # Nominal cable endpoints, including lateral offset; this excludes the service loop.
    lower_end = [(lx1+lx2)/2, (ly1+ly2)/2, lower_connector_top]
    upper_y, upper_z = upper_point((uy1+uy2)/2, -stack["upper_connector_mated_height"])
    upper_end = [(ux1+ux2)/2, upper_y, upper_z]
    chord = math.dist(lower_end, upper_end)
    assert stack["cable_length_mm"] > chord+40
    metrics = dict(
        status="NOMINAL ENVELOPES ONLY; measure actual sockets, controls and cable before enclosure release",
        tilt_deg=stack["tilt_deg"], upper_front_underside_z_mm=front_z,
        upper_rear_underside_z_mm=round(rear_z,2), projected_pcb_depth_mm=round(projected_depth,2),
        lower_depth_mm=depth, reduction_from_135mm_percent=round((135-depth)/135*100,1),
        core_to_upper_parts_normal_gap_mm=round(core_gap,2),
        antenna_to_upper_parts_normal_gap_mm=round(antenna_gap,2),
        upper_connector_to_core_vertical_gap_mm=round(connector_to_core,2),
        cable_endpoint_chord_mm=round(chord,2), nominal_cable_length_mm=stack["cable_length_mm"],
        envelope_assumptions_mm=dict(core_top_z=stack["core_top_z"],antenna_top_z=stack["antenna_top_z"],
                                     upper_underside_parts=parts_height,
                                     upper_connector_mated=stack["upper_connector_mated_height"],
                                     lower_connector_mated=lower_connector_top))
    output.mkdir(parents=True, exist_ok=True)
    (output / "stack-clearances.json").write_text(json.dumps(metrics,indent=2)+"\n")
    plt.rcParams.update({"font.family":"DejaVu Sans", "font.size":10})
    fig = plt.figure(figsize=(13,8.8), facecolor="white")
    ax = fig.add_axes([0.08,0.22,0.70,0.66])
    ax.set_aspect("equal")
    ax.set_xlim(-17,106)
    ax.set_ylim(-17,91)
    ax.axis("off")
    upper_color, lower_color, core_color = "#257e8a", "#397249", "#c99a38"
    ax.add_patch(Rectangle((0,-thickness),depth,thickness,fc=lower_color,ec="none"))
    ax.add_patch(Polygon(upper_polygon(0,length,0,thickness),fc=upper_color,ec="none"))
    ax.add_patch(Polygon(upper_polygon(38,length,-parts_height,0),fc="#bed8dc",ec="none"))
    ax.add_patch(Rectangle((0,0),57.15,stack["core_top_z"],fc=core_color,alpha=0.20,ec=core_color,ls="--"))
    ax.add_patch(Rectangle((0,10),57.15,1.6,fc=core_color,ec="none"))
    ax.add_patch(Rectangle((57.15,10),6.24,stack["antenna_top_z"]-10,fc=core_color,alpha=0.55,ec="none"))
    ax.add_patch(Rectangle((19,0),12,8.5,fc="#42474d",ec="none"))
    ax.add_patch(Polygon(upper_connector,fc="#42474d",alpha=0.9))
    ax.add_patch(Rectangle((ly1,0),ly2-ly1,lower_connector_top,fc="#596271",alpha=0.50,ls="--",ec="#42474d"))
    # Cable path is a schematic projection; the lower socket is left of the core in plan view.
    ax.plot([lower_end[1],35,35,upper_y],[lower_end[2],30,40,upper_z],
            color="#ba4d53",lw=3,solid_capstyle="round")
    ax.annotate("4-wire I2C harness\n150 mm nominal",xy=(35,34),xytext=(44,29),
                arrowprops=dict(arrowstyle="-",color="#ba4d53"),color="#92333c",fontsize=9)
    ax.annotate("J9 underside / XH4\nmated envelope: 12 mm",xy=(upper_y,upper_z+4),xytext=(-13,43),
                arrowprops=dict(arrowstyle="-",color="#505050"),fontsize=9)
    ax.annotate("YD ESP32-S3\nremovable core + sockets",xy=(41,14),xytext=(54,21),
                arrowprops=dict(arrowstyle="-",color=core_color),fontsize=8.5)
    ax.annotate("USB-C",xy=(0,12),xytext=(-15,12),va="center",
                arrowprops=dict(arrowstyle="<-",color="#505050"),fontsize=9)
    ax.text(22,59,"Upper PCB / 126 x 98 mm / 30 deg",rotation=-30,color=upper_color,fontsize=12)
    ax.text(41,-5,"Lower PCB / 126 x 86 mm",ha="center",va="top",color=lower_color,fontsize=11)
    ax.text(0,86,"REAR / HIGH",ha="left",fontsize=10,fontweight="bold")
    ax.text(86,86,"FRONT / LOW",ha="right",fontsize=10,fontweight="bold")
    ax.plot([0,0],[-11,-2],color="#a0a0a0",lw=0.7)
    ax.plot([86,86],[-11,-2],color="#a0a0a0",lw=0.7)
    ax.annotate("",xy=(0,-11),xytext=(86,-11),arrowprops=dict(arrowstyle="<->",color="#404040"))
    ax.text(43,-13,"86 mm assembled PCB depth",ha="center",va="top",fontsize=11)
    end_y, end_z = upper_point(length)
    ax.plot([end_y,99],[end_z,end_z],color="#a0a0a0",lw=0.7)
    ax.plot([depth,99],[0,0],color="#a0a0a0",lw=0.7)
    ax.annotate("",xy=(98,0),xytext=(98,front_z),arrowprops=dict(arrowstyle="<->",color="#404040"))
    ax.text(100,front_z/2,"25 mm",va="center",rotation=90)
    fig.text(0.08,0.95,"Workbench S3 / Stacked PCB Study",fontsize=20,fontweight="bold",color="#20262a")
    fig.text(0.08,0.91,"Two circuits, one breakaway panel. All dimensions in mm.",fontsize=11,color="#505b60")
    fig.text(0.80,0.81,"PCB SIZES",fontsize=10,fontweight="bold",color="#505b60")
    fig.text(0.80,0.77,"Upper   126 x 98\nLower   126 x 86\nPanel   126 x 187",fontsize=11,linespacing=1.7,va="top")
    fig.text(0.80,0.60,"NOMINAL GAPS",fontsize=10,fontweight="bold",color="#505b60")
    fig.text(0.80,0.56,f"Core: {core_gap:.1f} mm\nAntenna: {antenna_gap:.1f} mm\nJ9 to core: {connector_to_core:.1f} mm",
             fontsize=11,linespacing=1.7,va="top")
    fig.text(0.80,0.39,"DEPTH CHANGE",fontsize=10,fontweight="bold",color="#505b60")
    fig.text(0.80,0.35,"135 -> 86 mm\n36% less PCB depth",fontsize=11,linespacing=1.7,va="top")
    fig.text(0.08,0.14,"Envelope study, not a fitted assembly or enclosure drawing.",fontsize=11,fontweight="bold",color="#913c33")
    fig.text(0.08,0.10,"Controls, keycaps, case walls, feet and USB plugs are excluded. Connector/body heights require physical measurement.\n"
             "The lower XH connector is beside the core in plan view; their overlapping side projections do not imply a collision.\n"
             "Antenna spacing is nominal geometry only. Keep cable loops and metal outside the antenna region.",
             fontsize=9,color="#505b60",va="top",linespacing=1.5)
    fig.savefig(output / "stack-side.png",dpi=180)
    fig.savefig(output / "stack-side.svg")
    plt.close(fig)
    print(json.dumps(metrics,indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest",type=Path)
    parser.add_argument("output",type=Path)
    args = parser.parse_args()
    generate(args.manifest,args.output)
