#!/usr/bin/env python3
"""A Lexus IS300 (XE10, 1999-2005) modeled in SketchCraft from a blueprint.

    python3 examples/is300/build_is300.py path/to/blueprint.jpg out.sketchcraft

Talks to a running `sketchcraft bridge`. The blueprint is the common
four-view sheet (front, rear, side, top; 800 x 993 px) with printed sizes:
4,400 mm long, 1,725 mm wide, 1,420 mm tall, 2,670 mm wheelbase. Every
number below was measured off it in pixels and scaled by those sizes.

How it's built (the box-modeling way, in SketchCraft):
1. reference boards: the side, top and front views on planes, lined up
2. a cage of cross-sections along the car (heights from the side view,
   widths from the top, the cabin's tumblehome from the front), smoothed
   with `subd` (Catmull-Clark); the shoulder line creased so it stays crisp;
   glass painted on the cage's window faces
3. wheels, lights, grille and mirrors as solids
Car axes: X along the car (front at -X), Y across, Z up, ground at 0.
"""
import base64
import io
import json
import math
import sys
import urllib.request

from PIL import Image

BLUEPRINT, OUT = sys.argv[1], sys.argv[2]
BRIDGE = "http://127.0.0.1:5175"


def ex(cmd):
    r = json.load(urllib.request.urlopen(urllib.request.Request(BRIDGE + "/exec", json.dumps(cmd).encode()), timeout=600))
    if not r.get("ok"):
        raise SystemExit(f"{cmd.get('cmd')}: {r.get('error')}")
    return r


# ---- the blueprint's pixel frame -> meters ---------------------------------------------------
K = 4.400 / (713 - 40)  # side and top views: bumper to bumper is 673 px = 4,400 mm
KF = 1.725 / 262  # front view: 262 px across = 1,725 mm
SIDE_X0, GROUND_Y, TOP_CY = 376.5, 585.0, 815.0  # car center (x), ground line, top view centerline
FRONT_CX, FRONT_GROUND = 168.0, 258.0


def X(px):
    return (px - SIDE_X0) * K


def Z(py):
    return (GROUND_Y - py) * K


def interp(pts, x):
    """Piecewise-linear through (x, value) pairs."""
    if x <= pts[0][0]:
        return pts[0][1]
    for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
        if x <= x1:
            return y0 + (y1 - y0) * (x - x0) / (x1 - x0)
    return pts[-1][1]


# side view: the roofline / hood / deck (top silhouette), y in pixels
TOP = [(40, 480), (46, 476), (58, 471), (75, 465), (100, 455), (150, 444), (200, 438), (228, 439), (260, 420), (300, 394), (330, 380), (400, 371), (470, 368), (520, 373), (560, 389), (600, 409), (625, 421), (680, 425), (700, 427), (708, 440), (713, 470)]
# the beltline (bottom of the side windows) between the windshield and the deck
BELT = [(228, 441), (330, 444), (450, 441), (530, 437), (600, 428), (625, 421)]
# the body's bottom edge: front bumper, sills, rear bumper
BOTTOM = [(40, 545), (60, 552), (97, 556), (120, 553), (640, 553), (700, 550), (713, 548)]
# top view: half-width of the body (px), nose to tail
HALF = [(40, 55), (46, 80), (58, 100), (75, 118), (100, 131), (650, 131), (680, 120), (700, 100), (708, 80), (713, 55)]
# the cabin's roof edge half-width (top view), windshield base to rear window base
ROOF_HALF = [(228, 100), (330, 82), (400, 78), (520, 78), (625, 95)]
WINDSHIELD = (228, 330)
REAR_GLASS = (520, 625)
SIDE_GLASS = (240, 518)
B_PILLAR = (392, 408)
AXLES = (154.0, 563.5)
ARCH_R = 57.0  # wheel arch radius (px)
TIRE_R = 47.4  # tire radius (px): about 620 mm across
GREENHOUSE = (228, 625)

STATIONS = [40, 46, 58, 75, 97, 115, 135, 154, 173, 193, 211, 228, 260, 300, 330, 375, 392, 408, 450, 490, 506, 520, 545, 563.5, 581, 600, 625, 650, 680, 700, 708, 713]


def section(x):
    """One side of the cross-section at x (px): points (half-width m, z m), bottom center to top center."""
    yb = interp(BOTTOM, x)
    # wheel arches: the bottom edge follows the arch over each wheel
    for a in AXLES:
        if abs(x - a) < ARCH_R:
            yb = min(yb, 537.5 - math.sqrt(ARCH_R**2 - (x - a) ** 2))
    zb = Z(yb)
    hw = interp(HALF, x) * K
    in_cabin = GREENHOUSE[0] < x < GREENHOUSE[1]
    zbelt = Z(interp(BELT, x)) if in_cabin else Z(interp(TOP, x))
    ztop = Z(interp(TOP, x))
    zmid = Z(GROUND_Y - 0.6 / K) if zbelt > 0.75 else zb + 0.6 * (zbelt - zb)
    zmid = min(max(zmid, zb + 0.18), zbelt - 0.12)
    low = min(zb + 0.12, zmid - 0.06)
    if in_cabin:
        hr = interp(ROOF_HALF, x) * K
        lift = 0.045  # smoothing pulls a cage inward; this lands the roof on the blueprint
        pts = [(0, zb), (hw * 0.86, zb), (hw, low), (hw, zmid), (hw * 0.965, zbelt), (hw * 0.90, zbelt + 0.03), (hr, ztop - 0.07 + lift), (0, ztop + lift)]
    else:
        # hood or deck: a gentle crown, no cabin
        pts = [(0, zb), (hw * 0.86, zb), (hw, low), (hw, zmid), (hw * 0.965, zbelt - 0.02), (hw * 0.88, zbelt + 0.005), (hw * 0.5, zbelt + 0.025), (0, zbelt + 0.035)]
    return pts


# ---- the cage --------------------------------------------------------------------------------
points, faces, mats, creases = [], [], [], []
rings = []
for x in STATIONS:
    one = section(x)
    ring = []
    # right side (y < 0) bottom center -> top center, then the left side back down
    for i, (h, z) in enumerate(one):
        ring.append(len(points))
        points.append([X(x), -h, z])
    for h, z in reversed(one[1:-1]):
        ring.append(len(points))
        points.append([X(x), h, z])
    rings.append(ring)
n = len(rings[0])  # 14: 0 bottom center, 1..6 right side, 7 top center, 8..13 left side

def glass_for(xa, xb, k):
    """Is the band between stations xa..xb, ring edge k (k -> k+1), a window?"""
    mid = (xa + xb) / 2
    top_band = k in (6, 7)  # roof edge <-> top center, both sides
    side_band = k in (5, 8)  # greenhouse side
    if top_band and (WINDSHIELD[0] <= mid <= WINDSHIELD[1] or REAR_GLASS[0] <= mid <= REAR_GLASS[1]):
        return True
    if side_band and SIDE_GLASS[0] <= mid <= SIDE_GLASS[1] and not (B_PILLAR[0] <= mid <= B_PILLAR[1]):
        return True
    return False


for s in range(len(rings) - 1):
    a, b = rings[s], rings[s + 1]
    for k in range(n):
        k2 = (k + 1) % n
        # outward-facing: around the ring one way, along the car the other
        faces.append([a[k], a[k2], b[k2], b[k]])
        in_cabin = GREENHOUSE[0] < (STATIONS[s] + STATIONS[s + 1]) / 2 < GREENHOUSE[1]
        mats.append("Dark tint" if in_cabin and glass_for(STATIONS[s], STATIONS[s + 1], k) else "Body paint")
    # the shoulder character line stays crisp
    creases.append([a[4], b[4]])
    creases.append([a[n - 4], b[n - 4]])
# nose and tail caps
faces.append(list(reversed(rings[0])))
mats.append("Body paint")
faces.append(list(rings[-1]))
mats.append("Body paint")

ex({"cmd": "new"})
ex({"cmd": "units", "units": "mm"})
for name, c in [("Body paint", [0.74, 0.75, 0.77]), ("Dark tint", [0.10, 0.12, 0.14]), ("Tire rubber", [0.06, 0.06, 0.06]), ("Rim metal", [0.78, 0.79, 0.80]), ("Headlight", [1.0, 0.98, 0.92]), ("Tail light", [0.85, 0.06, 0.05]), ("Grille trim", [0.08, 0.08, 0.09]), ("Mirror glass", [0.55, 0.58, 0.62])]:
    ex({"cmd": "material", "name": name, "color": c})

car = ex({"cmd": "begin_group", "name": "Lexus IS300"})["group"]
body = ex({"cmd": "subd", "name": "Body", "points": points, "faces": faces, "materials": mats, "creases": creases, "levels": 2})["group"]


def solid_box(name, x0, y0, z0, x1, y1, z1, mat):
    g = ex({"cmd": "begin_group", "name": name})["group"]
    f = ex({"cmd": "rect", "from": [x0, y0, z0], "to": [x1, y1, z0]})["face"]
    ex({"cmd": "pushpull", "face": f, "distance": z1 - z0, "toward": [0, 0, 1]})
    ex({"cmd": "end_group"})
    ex({"cmd": "paint", "ids": [g], "material": mat})
    return g


def disc(name, cx, cy, cz, r, width, side, mat, segs=28):
    """A cylinder across the car (axis along Y), `side` -1 right / +1 left: grows outward."""
    g = ex({"cmd": "begin_group", "name": name})["group"]
    f = ex({"cmd": "circle", "center": [cx, cy, cz], "radius": r, "normal": [0, 1, 0], "segments": segs})["face"]
    ex({"cmd": "pushpull", "face": f, "distance": width, "toward": [0, side, 0]})
    ex({"cmd": "end_group"})
    ex({"cmd": "paint", "ids": [g], "material": mat})
    return g


# ---- wheels: 215 mm tires on the 1,495 / 1,485 mm tracks --------------------------------------
tire_r = TIRE_R * K
for axle, track, label in [(AXLES[0], 1.495, "Front"), (AXLES[1], 1.485, "Rear")]:
    for side, name in [(-1, "right"), (1, "left")]:
        y_in = side * (track / 2 - 0.1075)
        g = ex({"cmd": "begin_group", "name": f"{label} {name} wheel"})["group"]
        disc("Tire", X(axle), y_in, tire_r, tire_r, 0.215, side, "Tire rubber")
        disc("Rim", X(axle), y_in + side * 0.2, tire_r, tire_r * 0.66, 0.02, side, "Rim metal", 20)
        disc("Hub", X(axle), y_in + side * 0.22, tire_r, tire_r * 0.16, 0.012, side, "Rim metal", 12)
        ex({"cmd": "end_group"})

# ---- lights, grille, mirrors: set into the body's surface, measured on the views -------------
def surface_x(half_width_m, front=True):
    """Where the body (top view) is this wide, from the nose or the tail: x in meters."""
    target = half_width_m / K
    xs = [x / 2 for x in range(80, 1427)]
    xs = xs if front else list(reversed(xs))
    for px in xs:
        if interp(HALF, px) >= target:
            return X(px)
    return X(xs[-1])


for side in (-1, 1):
    lo, hi = sorted((side * 0.40, side * 0.62))
    # twin round headlamps in one housing: flush with the nose, 0.58-0.68 m up
    xh = surface_x(0.62)
    solid_box("Headlight", xh - 0.07, lo, 0.58, xh + 0.09, hi, 0.68, "Headlight")
    # the clear "Altezza" taillights wrap the rear corners, 0.70-0.85 m up
    lo, hi = sorted((side * 0.38, side * 0.66))
    xt = surface_x(0.66, front=False)
    solid_box("Taillight", xt - 0.09, lo, 0.70, xt + 0.07, hi, 0.85, "Tail light")
    # door mirrors where the A-pillar meets the door, tucked against the glass
    hw = interp(HALF, 282) * K * 0.93
    lo, hi = sorted((side * hw, side * (hw + 0.15)))
    solid_box("Mirror", X(270), lo, Z(444), X(292), hi, Z(428), "Body paint")
xg = surface_x(0.30)
solid_box("Grille", xg - 0.004, -0.27, 0.48, xg + 0.08, 0.27, 0.60, "Grille trim")
xi = surface_x(0.34)
solid_box("Lower intake", xi - 0.004, -0.30, 0.31, xi + 0.08, 0.30, 0.40, "Grille trim")
ex({"cmd": "end_group"})


# ---- reference boards: each view, cropped, lined up with the car ------------------------------
img = Image.open(BLUEPRINT).convert("RGB")


def board(name, crop, ll, R, U, w, h):
    """A face showing `crop` of the blueprint: lower-left at ll, R = image right, U = image up."""
    piece = img.crop(crop)
    n = [R[1] * U[2] - R[2] * U[1], R[2] * U[0] - R[0] * U[2], R[0] * U[1] - R[1] * U[0]]
    # SketchCraft lays a projected texture along basis(normal): u = Z x n (or X x n when flat)
    def cross(a, b):
        return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
    def norm(a):
        l = math.sqrt(sum(c * c for c in a))
        return [c / l for c in a]
    u = norm(cross([0, 0, 1], n)) if abs(n[2]) < 0.9 else norm(cross([1, 0, 0], n))
    v = cross(n, u)
    dot = lambda a, b: sum(a[i] * b[i] for i in range(3))
    # how the picture must turn so the face's (u, v) shows it the right way round
    ur, ug = dot(u, R), dot(u, U)
    if abs(ur) > 0.5:
        W, H = w, h
        piece = piece if ur > 0 else piece.transpose(Image.FLIP_LEFT_RIGHT)
        if dot(v, U) < 0:
            piece = piece.transpose(Image.FLIP_TOP_BOTTOM)
    else:
        W, H = h, w
        piece = piece.transpose(Image.ROTATE_270 if ug > 0 else Image.ROTATE_90)
        if dot(v, R) * (1 if ug > 0 else -1) < 0:
            piece = piece.transpose(Image.FLIP_TOP_BOTTOM)
    buf = io.BytesIO()
    piece.save(buf, "JPEG", quality=90)
    data = "data:image/jpeg;base64," + base64.b64encode(buf.getvalue()).decode()
    corners = [ll, [ll[i] + R[i] * w for i in range(3)], [ll[i] + R[i] * w + U[i] * h for i in range(3)], [ll[i] + U[i] * h for i in range(3)]]
    # the face's own lower-left, along u and v, anchors the tiles
    origin = min(corners, key=lambda p: (dot(p, u), dot(p, v)))
    g = ex({"cmd": "begin_group", "name": name})["group"]
    ex({"cmd": "polygon", "points": corners})
    ex({"cmd": "end_group"})
    ex({"cmd": "texture", "name": name, "data": data, "width": W, "height": H, "project": [-c for c in n], "origin": origin})
    ex({"cmd": "paint", "ids": [g], "material": name})
    ex({"cmd": "tag", "ids": [g], "tag": "Blueprint"})
    ex({"cmd": "opacity", "material": name, "value": 0.55})
    ex({"cmd": "lock", "ids": [g]})
    return g


# side view behind the car (seen from the front camera, -Y)
sx0, sx1, sy0, sy1 = 20, 780, 320, 612
board("Blueprint: side", (sx0, sy0, sx1, sy1), [X(sx0), 1.6, Z(sy1)], [1, 0, 0], [0, 0, 1], (sx1 - sx0) * K, (sy1 - sy0) * K)
# top view under the car
tx0, tx1, ty0, ty1 = 20, 780, 655, 990
board("Blueprint: top", (tx0, ty0, tx1, ty1), [X(tx0), (ty1 - TOP_CY) * K, -0.01], [1, 0, 0], [0, -1, 0], (tx1 - tx0) * K, (ty1 - ty0) * K)
# front view behind the tail (seen from the left camera, -X): image right is -Y
fx0, fx1, fy0, fy1 = 10, 330, 20, 270
board("Blueprint: front", (fx0, fy0, fx1, fy1), [3.0, -(fx0 - FRONT_CX) * KF, (FRONT_GROUND - fy1) * KF], [0, -1, 0], [0, 0, 1], (fx1 - fx0) * KF, (fy1 - fy0) * KF)
ex({"cmd": "tag_visible", "tag": "Blueprint", "visible": True})
ex({"cmd": "save_view", "name": "Three-quarter", "eye": [-5.2, -4.6, 2.0], "target": [0.1, 0, 0.6], "fov": 35})
ex({"cmd": "save", "path": OUT})
info = ex({"cmd": "info"})
print(f"cage: {len(points)} points, {len(faces)} faces; model {info['faces']} faces; saved {OUT}")
