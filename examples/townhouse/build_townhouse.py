#!/usr/bin/env python3
"""Two-story 3 bed / 2 bath townhouse (end unit), from the "3BR 2BA Townhouse
Blueprint" floor plans (sheet A-101). Writes townhouse.jsonl, a SketchCraft
command script.

Plan coordinates in feet: x across the house (0 = party wall on the left,
22 = right side), y from the street (0 = front face) to the back (48).
Main floor 22 x 48 with the rear 13 ft one story; upper floor 22 x 35 over
the front. Openings are cut with Solid Tools; doors and windows are the
built-in components, sized with Dynamic Component attributes.
"""
import json
import os

FT = 0.3048
EXT = 8 / 12  # exterior wall, 8"
INT = 4.5 / 12  # interior wall, 4 1/2"
FLOOR = 10.0  # floor to floor
WALL_H = 9.0  # walls up to the next floor's framing
DOOR_H = 6 + 8 / 12

out = []


def cmd(c):
    out.append(c)


def m(*xyz):
    return [round(v * FT, 6) for v in xyz]


def box(name, x0, y0, z0, x1, y1, z1, as_=None):
    cmd({"cmd": "begin_group", "name": name, **({"as": as_} if as_ else {})})
    cmd({"cmd": "polygon", "points": [m(x0, y0, z0), m(x1, y0, z0), m(x1, y1, z0), m(x0, y1, z0)], "as": "_f"})
    cmd({"cmd": "pushpull", "face": "@_f", "distance": round((z1 - z0) * FT, 6)})
    cmd({"cmd": "end_group"})


def cut(target, x0, y0, z0, x1, y1, z1):
    """Cut a box out of the solid named `target` (keeps the name)."""
    box("cutter", x0, y0, z0, x1, y1, z1, as_="_cut")
    cmd({"cmd": "solid", "op": "subtract", "ids": ["@_cut", f"@{target}"], "as": target})


def wall(name, pts, thick, z, h, closed=False):
    cmd({"cmd": "wall", "points": [m(x, y, z) for x, y in pts], "thickness": round(thick * FT, 6), "height": round(h * FT, 6), "closed": closed, "as": name})
    cmd({"cmd": "rename", "id": f"@{name}", "name": name.replace("_", " ").capitalize()})


def window(target, side, a, b, sill, head, z0):
    """A window in an exterior wall: `side` front/rear/right, from a to b along it."""
    t = EXT + 0.4
    if side == "front":
        cut(target, a, -0.2, z0 + sill, b, t, z0 + head)
        at, rot = m(a, 0, z0 + sill), 0
    elif side == "rear":
        y = REAR[target]
        cut(target, a, y - t, z0 + sill, b, y + 0.2, z0 + head)
        at, rot = m(b, y, z0 + sill), 180
    else:  # right side, x = 22
        cut(target, 22 - t, a, z0 + sill, 22.2, b, z0 + head)
        at, rot = m(22, a, z0 + sill), 90
    cmd({"cmd": "insert", "asset": "window", "at": at, "rotate": rot, "as": "_w"})
    cmd({"cmd": "set_attrs", "id": "@_w", "attrs": {"lenx": str(round((b - a) * FT, 4)), "lenz": str(round((head - sill) * FT, 4))}})


def opening(target, x0, y0, x1, y1, z0, h=DOOR_H):
    """A doorway through an interior wall (a box spanning it)."""
    cut(target, x0, y0, z0, x1, y1, z0 + h)


REAR = {"main_walls": 48.0, "upper_walls": 35.0}

cmd({"cmd": "new"})
cmd({"cmd": "units", "units": "ft"})

# ---- ground and floors -------------------------------------------------------
box("Foundation", -0.5, -0.5, -1.0, 22.5, 48.5, 0.0, as_="foundation")
box("Driveway", 0.5, -24.0, -0.3, 11.0, -0.5, -0.05, as_="drive")
box("Front walk", 13.0, -10.0, -0.3, 17.0, -0.5, -0.05, as_="walk")

# ---- main floor ----------------------------------------------------------------
h = EXT / 2
wall("main_walls", [(h, h), (22 - h, h), (22 - h, 48 - h), (h, 48 - h)], EXT, 0, WALL_H, closed=True)
# front: overhead garage door, front door, foyer window
cut("main_walls", 1.7, -0.2, 0, 9.8, EXT + 0.4, 7.0)
cmd({"cmd": "insert", "asset": "garage_door", "at": m(1.7, 0, 0), "as": "_g"})
cmd({"cmd": "set_attrs", "id": "@_g", "attrs": {"lenx": str(round(8.1 * FT, 4)), "lenz": str(round(7 * FT, 4))}})
cut("main_walls", 13.8, -0.2, 0, 16.8, EXT + 0.4, DOOR_H)
cmd({"cmd": "insert", "asset": "door", "at": m(13.8, 0, 0), "as": "_d"})
cmd({"cmd": "set_attrs", "id": "@_d", "attrs": {"lenx": str(round(3 * FT, 4)), "lenz": str(round(DOOR_H * FT, 4))}})
window("main_walls", "front", 18.8, 21.2, 3, 7, 0)
# right side: foyer, two in the living room
window("main_walls", "right", 3.0, 6.0, 3, 7, 0)
window("main_walls", "right", 21.8, 25.8, 2.5, 7, 0)
window("main_walls", "right", 28.8, 32.8, 2.5, 7, 0)
# rear: primary bedroom, bath 1
window("main_walls", "rear", 3.3, 10.3, 3, 7, 0)
window("main_walls", "rear", 14.8, 17.2, 4.5, 6.5, 0)

# interior walls (centerlines)
wall("suite_wall", [(EXT, 35), (22 - EXT, 35)], INT, 0, WALL_H)
opening("suite_wall", 9.7, 34, 12.3, 36, 0)
wall("bath1_wall", [(13, 35), (13, 48 - EXT)], INT, 0, WALL_H)
opening("bath1_wall", 12, 42.3, 14, 44.7, 0)
opening("bath1_wall", 12, 36.4, 14, 38.8, 0)
wall("wic_wall", [(13, 40), (22 - EXT, 40)], INT, 0, WALL_H)
wall("garage_back_wall", [(EXT, 20), (15, 20)], INT, 0, WALL_H)
opening("garage_back_wall", 11.8, 19, 14.2, 21, 0)
wall("garage_wall", [(11, EXT), (11, 20)], INT, 0, WALL_H)
opening("garage_wall", 10, 3.5, 12, 6.5, 0)
wall("laundry_wall", [(15, 8), (15, 20)], INT, 0, WALL_H)
opening("laundry_wall", 14, 10.0, 16, 14.0, 0)
wall("foyer_wall", [(11, 8), (15, 8)], INT, 0, WALL_H)
wall("pantry_wall", [(11, 16), (15, 16)], INT, 0, WALL_H)

# stair along the right wall, up toward the back
cmd({"cmd": "stairs", "at": m(22 - EXT, 8, 0), "rise": round(FLOOR * FT, 4), "direction": 90, "width": round(2.6 * FT, 4), "tread": round(12 / 16 * FT, 4), "as": "stair"})

# second floor framing, with the stair opening; flat roof over the one-story rear
box("Second floor", 0, 0, WALL_H, 22, 35, FLOOR, as_="floor2")
cut("floor2", 18.6, 8.0, WALL_H - 0.5, 22 - EXT, 20.2, FLOOR + 0.5)
box("Rear roof", -0.5, 35, WALL_H, 22.5, 48.75, FLOOR, as_="rear_roof")

# ---- upper floor ---------------------------------------------------------------
z2 = FLOOR
wall("upper_walls", [(h, h), (22 - h, h), (22 - h, 35 - h), (h, 35 - h)], EXT, z2, WALL_H, closed=True)
window("upper_walls", "front", 3.3, 9.3, 3, 7, z2)
window("upper_walls", "front", 12.2, 18.3, 3, 7, z2)
window("upper_walls", "right", 2.2, 5.7, 3, 7, z2)
window("upper_walls", "right", 28.2, 32.0, 3, 7, z2)
window("upper_walls", "rear", 3.8, 8.7, 3, 7, z2)
window("upper_walls", "rear", 14.8, 18.3, 3, 7, z2)

wall("bedroom_wall", [(EXT, 23), (22 - EXT, 23)], INT, z2, WALL_H)
opening("bedroom_wall", 6.2, 22, 9.0, 24, z2)
opening("bedroom_wall", 12.8, 22, 15.5, 24, z2)
wall("bed_divider", [(12, 23), (12, 35 - EXT)], INT, z2, WALL_H)
wall("bath2_wall", [(8, 15), (8, 23)], INT, z2, WALL_H)
opening("bath2_wall", 7, 18.3, 9, 20.6, z2)
wall("bath2_back", [(EXT, 15), (8, 15)], INT, z2, WALL_H)
wall("stair_rail", [(18.6, 8), (18.6, 20.2)], INT, z2, 3.0)

# gable roof over the upper floor, 6/12, a foot of overhang
cmd({"cmd": "polygon", "points": [m(0, 0, z2 + WALL_H), m(22, 0, z2 + WALL_H), m(22, 35, z2 + WALL_H), m(0, 35, z2 + WALL_H)], "as": "_top"})
cmd({"cmd": "roof", "face": "@_top", "kind": "gable", "pitch": 26.57, "overhang": round(1.0 * FT, 4), "thickness": 0.2, "as": "roof"})
cmd({"cmd": "delete", "ids": ["@_top"]})

# ---- paint and classify ----------------------------------------------------------
SIDING = [0.86, 0.84, 0.78]
for g in ["main_walls", "upper_walls"]:
    cmd({"cmd": "paint", "ids": [f"@{g}"], "color": SIDING})
for g in ["suite_wall", "bath1_wall", "wic_wall", "garage_back_wall", "garage_wall", "laundry_wall", "foyer_wall", "pantry_wall", "bedroom_wall", "bed_divider", "bath2_wall", "bath2_back", "stair_rail"]:
    cmd({"cmd": "paint", "ids": [f"@{g}"], "color": [0.97, 0.97, 0.95]})
cmd({"cmd": "paint", "ids": ["@roof", "@rear_roof"], "color": [0.27, 0.29, 0.33]})
cmd({"cmd": "paint", "ids": ["@foundation", "@floor2"], "color": [0.74, 0.73, 0.70]})
cmd({"cmd": "paint", "ids": ["@drive", "@walk"], "color": [0.66, 0.65, 0.62]})
cmd({"cmd": "paint", "ids": ["@stair"], "color": [0.62, 0.45, 0.30]})
cmd({"cmd": "classify", "ids": ["@floor2", "@foundation"], "class": "IfcSlab"})
cmd({"cmd": "classify", "ids": ["@rear_roof"], "class": "IfcRoof"})
cmd({"cmd": "classify", "ids": ["@stair_rail"], "class": "IfcRailing"})

# ---- views -------------------------------------------------------------------------
cmd({"cmd": "save_view", "name": "Street", "eye": m(52, -48, 6), "target": m(10, 18, 9), "fov": 42})
cmd({"cmd": "save_view", "name": "Aerial", "eye": m(70, -40, 60), "target": m(11, 22, 4), "fov": 40})
cmd({"cmd": "save_view", "name": "Rear", "eye": m(-25, 95, 14), "target": m(11, 30, 8), "fov": 42})

here = os.path.dirname(os.path.abspath(__file__))
with open(os.path.join(here, "townhouse.jsonl"), "w") as f:
    f.write("# Two-story townhouse (3 bed, 2 bath, 1-car garage). Generated by build_townhouse.py.\n")
    for c in out:
        f.write(json.dumps(c) + "\n")
print(len(out), "commands")
