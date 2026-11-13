#!/usr/bin/env python3
"""Generate a SketchCraft script for a Tuscan-style retail center.

  python3 examples/retail/build_retail.py > examples/retail_center.jsonl
  sketchcraft run examples/retail_center.jsonl

A long one-story stucco building with a parapet, a tall corner tower
(brick storefront base, stucco upper with blind arches, corbelled cornice,
terracotta hip roof), a smaller end tower, an arcade with brick piers and a
tile shed roof, and storefronts under tile awnings on timber brackets.

Plan: the long facade faces the street (-Y) and runs along -X from the corner
tower at the origin; the side facade faces +X and runs along +Y. Meters.
"""

import json
import math

out = []
n = 0


def cmd(c, **kw):
    out.append(json.dumps({"cmd": c, **kw}, separators=(",", ":")))


def name(prefix):
    global n
    n += 1
    return f"{prefix}{n}"


def r(v):
    return [round(x, 4) for x in v]


STUCCO = [0.86, 0.79, 0.69]
STUCCO_DK = [0.78, 0.71, 0.61]
BRICK = [0.62, 0.31, 0.23]
TILE = [0.78, 0.39, 0.23]
TRIM = [0.94, 0.91, 0.83]
GLASS = [0.33, 0.41, 0.45]
TIMBER = [0.40, 0.23, 0.14]
CONCRETE = [0.78, 0.77, 0.74]
ASPHALT = [0.36, 0.37, 0.39]


def group(label, color, body):
    g = name("g")
    cmd("begin_group", name=label, **{"as": g})
    body()
    cmd("end_group")
    cmd("paint", ids=[f"@{g}"], color=color)
    return g


def box(x0, y0, z0, x1, y1, z1):
    """A box from its ground rectangle, pulled up (inside the current group)."""
    f = name("f")
    cmd("rect", **{"from": r([x0, y0, z0]), "to": r([x1, y1, z0]), "as": f})
    cmd("pushpull", face=f"@{f}", distance=round(z1 - z0, 4))


def box_group(label, color, *b):
    return group(label, color, lambda: box(*b))


def hip_roof(label, x0, y0, x1, y1, z, rise, eave=0.25):
    """Eave slab plus a four-sided hip roof meeting at a point."""
    def body():
        box(x0, y0, z, x1, y1, z + eave)
        zt = z + eave
        cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
        apex = [cx, cy, zt + rise]
        c = [[x0, y0, zt], [x1, y0, zt], [x1, y1, zt], [x0, y1, zt]]  # counter-clockwise from above
        for i in range(4):
            cmd("polygon", points=[r(c[i]), r(c[(i + 1) % 4]), r(apex)])
    group(label, TILE, body)


def arch_points(xa, xb, spring, rise, y, segs=10):
    """An arched opening in the plane y = const (faces -Y): bottom corners up to
    the springing line, then a round-topped arch, listed counter-clockwise."""
    pts = [[xa, y, 0.0], [xb, y, 0.0], [xb, y, spring]]
    half = (xb - xa) / 2
    rad = (half * half + rise * rise) / (2 * rise)
    cx, cz = (xa + xb) / 2, spring + rise - rad
    a0 = math.atan2(spring - cz, xb - cx)
    a1 = math.atan2(spring - cz, xa - cx)
    for k in range(1, segs):
        a = a0 + (a1 - a0) * k / segs
        pts.append([cx + rad * math.cos(a), y, cz + rad * math.sin(a)])
    pts.append([xa, y, spring])
    return [r(p) for p in pts]


def blind_arch(xa, xb, z, rise, y):
    """A shallow recessed arch panel on a -Y-facing wall (drawn inside the open group)."""
    half = (xb - xa) / 2
    rad = (half * half + rise * rise) / (2 * rise)
    cx, cz = (xa + xb) / 2, z + rise - rad
    a0, a1 = math.atan2(z - cz, xb - cx), math.atan2(z - cz, xa - cx)
    pts = [[xa, y, z], [xb, y, z]]
    for k in range(1, 12):
        a = a0 + (a1 - a0) * k / 12
        pts.append([cx + rad * math.cos(a), y, cz + rad * math.sin(a)])
    f = name("arch")
    cmd("polygon", points=[r(p) for p in pts], **{"as": f})
    cmd("pushpull", face=f"@{f}", distance=-0.08, **{"as": f + "b"})
    cmd("paint", ids=[f"@{f}b"], color=STUCCO_DK)


def storefront_y(x0, x1, z0, z1, y, depth=0.25, mullions=2, transom=None):
    """Recessed glass in a -Y-facing wall, split by mullions and a transom."""
    f = name("w")
    cmd("rect", **{"from": r([x0, y, z0]), "to": r([x1, y, z1]), "as": f})
    g = name("glass")
    cmd("pushpull", face=f"@{f}", distance=-depth, **{"as": g})
    cmd("paint", ids=[f"@{g}"], color=GLASS)
    yb = y + depth
    for k in range(1, mullions + 1):
        x = x0 + (x1 - x0) * k / (mullions + 1)
        cmd("line", points=[r([x, yb, z0]), r([x, yb, z1])])
    if transom:
        cmd("line", points=[r([x0, yb, transom]), r([x1, yb, transom])])
    return g


def storefront_x(y0, y1, z0, z1, x, depth=0.25, mullions=2, transom=None):
    """Recessed glass in a +X-facing wall."""
    f = name("w")
    cmd("rect", **{"from": r([x, y0, z0]), "to": r([x, y1, z1]), "as": f})
    g = name("glass")
    cmd("pushpull", face=f"@{f}", distance=-depth, **{"as": g})
    cmd("paint", ids=[f"@{g}"], color=GLASS)
    xb = x - depth
    for k in range(1, mullions + 1):
        yy = y0 + (y1 - y0) * k / (mullions + 1)
        cmd("line", points=[r([xb, yy, z0]), r([xb, yy, z1])])
    if transom:
        cmd("line", points=[r([xb, y0, transom]), r([xb, y1, transom])])
    return g


def awning_y(x0, x1, y, z, depth=1.4, drop=0.5):
    """A tile shed awning on a -Y wall, with two timber brackets under it."""
    def roof():
        f = name("aw")
        # profile on the plane x = x0, extruded along +X
        cmd("polygon", points=[r([x0, y, z + drop]), r([x0, y, z + drop + 0.12]), r([x0, y - depth, z + 0.12]), r([x0, y - depth, z])], **{"as": f})
        cmd("pushpull", face=f"@{f}", distance=-(x1 - x0))
    group("Awning", TILE, roof)

    def brackets():
        for x in (x0 + 0.35, x1 - 0.47):
            f = name("br")
            cmd("polygon", points=[r([x, y, z - 0.9]), r([x, y, z + drop * 0.8]), r([x, y - depth * 0.8, z + 0.05])], **{"as": f})
            cmd("pushpull", face=f"@{f}", distance=-0.12)
    group("Brackets", TIMBER, brackets)


def awning_x(y0, y1, x, z, depth=1.6, drop=0.55):
    """A tile shed awning on a +X wall, with brackets."""
    def roof():
        f = name("aw")
        cmd("polygon", points=[r([x, y0, z + drop]), r([x + depth, y0, z]), r([x + depth, y0, z + 0.12]), r([x, y0, z + drop + 0.12])], **{"as": f})
        cmd("pushpull", face=f"@{f}", distance=-(y1 - y0))
    group("Awning", TILE, roof)

    def brackets():
        for yy in (y0 + 0.35, y1 - 0.47):
            f = name("br")
            cmd("polygon", points=[r([x, yy, z - 0.95]), r([x + depth * 0.8, yy, z + 0.05]), r([x, yy, z + drop * 0.8])], **{"as": f})
            cmd("pushpull", face=f"@{f}", distance=-0.12)
    group("Brackets", TIMBER, brackets)


def corbels(x0, x1, y, z, step=0.9, along="x"):
    """A row of small corbels under a cornice."""
    def body():
        if along == "x":
            box(x0, y - 0.32, z, x0 + 0.22, y, z + 0.38)
        else:
            box(y, x0, z, y + 0.32, x0 + 0.22, z + 0.38)
    g = group("Corbels", TRIM, body)
    count = int((x1 - x0 - 0.22) / step)
    by = [step, 0, 0] if along == "x" else [0, step, 0]
    if count > 0:
        cmd("array", ids=[f"@{g}"], by=by, count=count)


cmd("new")

# ---------------------------------------------------------------- site
def site():
    box(-80, -32, -0.06, 28, 46, -0.01)
group("Parking lot", ASPHALT, site)
group("Sidewalk", CONCRETE, lambda: (box(-58, -5.5, 0, 2.5, 0, 0.12), box(0, 0, 0, 4, 32, 0.12)))

# ---------------------------------------------------------------- main building
main = name("main")
cmd("begin_group", name="Main building", **{"as": main})
box(-56, 0, 0, 0, 30, 6.6)
cmd("paint", ids=[f"@{main}"], color=STUCCO)
# storefront bays along the long facade, west of the arcade
for i, x in enumerate([-47.6, -43.2, -38.8, -34.4, -30.0]):
    storefront_y(x, x + 2.5, 0.12, 3.0, 0, mullions=1, transom=2.4)
# storefronts on the side facade
for y in (12.2, 17.0, 21.8, 26.6):
    storefront_x(y, y + 3.6, 0.7, 3.5, 0, mullions=2, transom=2.75)
cmd("end_group")
box_group("Parapet cap", TRIM, -56.15, -0.15, 6.5, 0.15, 30.15, 6.78)
box_group("Brick base, side", BRICK, 0, 10.6, 0, 0.14, 30, 0.7)

# brick piers between the long-facade storefronts
def piers():
    for x in [-48.4, -44.0, -39.6, -35.2, -30.8, -27.0]:
        box(x, -0.32, 0, x + 0.62, 0, 3.7)
group("Brick piers", BRICK, piers)
for x in (-47.75, -43.35, -38.95):
    awning_y(x, x + 2.8, 0, 3.75, depth=1.2, drop=0.45)

for y in (12.0, 16.8, 21.6, 26.4):
    awning_x(y, y + 4.0, 0.14, 3.85)

# ---------------------------------------------------------------- arcade
X0, X1, YF = -26.0, -10.8, -3.6

def arcade():
    f = name("arc")
    cmd("polygon", points=[r([X0, YF, 0]), r([X1, YF, 0]), r([X1, YF, 5.3]), r([X0, YF, 5.3])], **{"as": f})
    pier, bays = 0.68, 4
    w = (X1 - X0 - pier * (bays + 1)) / bays
    for k in range(bays):
        xa = X0 + pier + k * (w + pier)
        o = name("open")
        cmd("polygon", points=arch_points(xa, xa + w, 2.9, 0.75, YF), **{"as": o})
        cmd("delete", ids=[f"@{o}"])
    cmd("pushpull", face=f"@{f}", distance=-0.55)
group("Arcade", STUCCO, arcade)

def arcade_piers():
    pier, bays = 0.68, 4
    w = (X1 - X0 - pier * (bays + 1)) / bays
    for k in range(bays + 1):
        xa = X0 + k * (w + pier)
        box(xa - 0.06, YF - 0.12, 0, xa + pier + 0.06, YF + 0.62, 2.95)
group("Arcade piers", BRICK, arcade_piers)
box_group("Arcade ceiling", TRIM, X0, YF + 0.55, 3.95, X1, 0, 4.1)

def arcade_roof():
    f = name("ar")
    cmd("polygon", points=[r([X0 - 0.2, YF - 0.7, 5.2]), r([X0 - 0.2, 0, 6.25]), r([X0 - 0.2, 0, 6.42]), r([X0 - 0.2, YF - 0.7, 5.37])], **{"as": f})
    cmd("pushpull", face=f"@{f}", distance=X1 - X0 + 0.4)
group("Arcade roof", TILE, arcade_roof)
corbels(X0, X1, YF - 0.1, 4.82)

# ---------------------------------------------------------------- corner tower
TX0, TY0, TX1, TY1 = -10.4, -0.6, 0.6, 10.4
tower = name("tower")
cmd("begin_group", name="Corner tower", **{"as": tower})
box(TX0, TY0, 4.3, TX1, TY1, 10.6)  # the brick base below is its own solid
cmd("paint", ids=[f"@{tower}"], color=STUCCO)
blind_arch(-8.4, -2.4, 5.4, 1.3, TY0)
cmd("end_group")

base = name("tbase")
cmd("begin_group", name="Tower base", **{"as": base})
box(TX0 - 0.12, TY0 - 0.12, 0, TX1 + 0.12, TY1 + 0.12, 4.3)
cmd("paint", ids=[f"@{base}"], color=BRICK)
g1 = storefront_y(-8.0, -2.8, 0, 3.9, TY0 - 0.12, depth=0.1, mullions=1, transom=2.7)
g2 = storefront_x(2.8, 8.0, 0, 3.9, TX1 + 0.12, depth=0.1, mullions=1, transom=2.7)
cmd("end_group")
box_group("Tower band", TRIM, TX0 - 0.25, TY0 - 0.25, 4.3, TX1 + 0.25, TY1 + 0.25, 4.7)
box_group("Tower cornice", TRIM, TX0 - 0.45, TY0 - 0.45, 10.2, TX1 + 0.45, TY1 + 0.45, 10.62)
corbels(TX0 + 0.3, TX1 - 0.3, TY0, 9.8)
corbels(TY0 + 0.3, TY1 - 0.3, TX1, 9.8, along="y")
hip_roof("Tower roof", TX0 - 1.2, TY0 - 1.2, TX1 + 1.2, TY1 + 1.2, 10.62, 2.3)

# arch panel on the side face of the tower, too
side = name("tside")
cmd("edit", id=f"@{tower}")
cmd("polygon", points=[r([TX1, y, z]) for y, z in [(2.4, 5.4), (8.4, 5.4)]] + [r([TX1, 5.4 + 3 * math.cos(a), 5.4 + 1.3 * math.sin(a)]) for a in [math.pi * k / 12 for k in range(1, 12)]], **{"as": side})
cmd("pushpull", face=f"@{side}", distance=-0.08, **{"as": side + "b"})
cmd("paint", ids=[f"@{side}b"], color=STUCCO_DK)
cmd("close")

# ---------------------------------------------------------------- end tower
EX0, EY0, EX1, EY1 = -57.0, -0.6, -49.0, 7.4
etower = name("etower")
cmd("begin_group", name="End tower", **{"as": etower})
box(EX0, EY0, 0, EX1, EY1, 8.8)
cmd("end_group")
cmd("paint", ids=[f"@{etower}"], color=STUCCO)

eb = name("ebase")
cmd("begin_group", name="End tower base", **{"as": eb})
box(EX0 - 0.1, EY0 - 0.1, 0, EX1 + 0.1, EY1 + 0.1, 3.9)
cmd("paint", ids=[f"@{eb}"], color=BRICK)
cmd("polygon", points=arch_points(-54.6, -51.4, 2.4, 0.9, EY0 - 0.1), **{"as": "eo"})
cmd("pushpull", face="@eo", distance=-0.08, **{"as": "eodoor"})
cmd("paint", ids=["@eodoor"], color=GLASS)
cmd("end_group")
box_group("End tower band", TRIM, EX0 - 0.22, EY0 - 0.22, 3.9, EX1 + 0.22, EY1 + 0.22, 4.25)
box_group("End tower cornice", TRIM, EX0 - 0.4, EY0 - 0.4, 8.45, EX1 + 0.4, EY1 + 0.4, 8.82)
corbels(EX0 + 0.3, EX1 - 0.3, EY0, 8.05)
hip_roof("End tower roof", EX0 - 1.0, EY0 - 1.0, EX1 + 1.0, EY1 + 1.0, 8.82, 1.8)

# ---------------------------------------------------------------- planting, cars, lights
for x in (-40, -20, -5):
    cmd("insert", asset="palm", at=[x, -7.5, 0])
for x, y in ((-34, -18), (-28, -18), (-14, -18), (6, -14), (9, 14)):
    cmd("insert", asset="car", at=[x, y, 0], rotate=90 if y < 0 else 0)
for x in (-46, -24, 4):
    cmd("insert", asset="streetlight", at=[x, -12, 0], rotate=90)
cmd("insert", asset="tree", at=[8, 4, 0])
cmd("insert", asset="bench", at=[-18, -5, 0.12], rotate=180)

# ---------------------------------------------------------------- views and renders
cmd("save_view", name="Corner", eye=[16, -24, 5.5], target=[-14, 4, 4.2], fov=48)
cmd("save_view", name="Aerial", eye=[34, -48, 34], target=[-24, 10, 0], fov=40)
cmd("save_view", name="Long facade", eye=[-28, -60, 5], target=[-28, 0, 5], fov=40)
cmd("info")
cmd("render", out="out/retail_corner.png", view="Corner", width=1600, height=1000)
cmd("render", out="out/retail_aerial.png", view="Aerial", width=1600, height=1000)
cmd("render", out="out/retail_long.png", view="Long facade", width=1600, height=700, parallel=True, grid=False)

print("# Tuscan-style retail center, generated by examples/retail/build_retail.py")
print("\n".join(out))
