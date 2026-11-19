#!/usr/bin/env python3
"""Generate the texturing test set in core/assets: dice, can, globe, crate, soccer ball.

Original geometry written as command scripts (like every other asset), built from plain
faces so each model has the topology a texturing feature needs to be tested on.
Origin: on the floor at the middle of the footprint (the ball and globe rest on it).

    python3 tools/texturing_set/texturing_set.py          # writes core/assets/*.jsonl
    python3 tools/texturing_set/texturing_set.py --check  # fails if a file is stale

Pure Python, no packages needed.
"""

import itertools
import json
import math
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ASSETS = ROOT / "core" / "assets"
PHI = (1 + 5 ** 0.5) / 2


class Script:
    def __init__(self, header):
        self.lines = ["# " + h for h in header]
        self.n = 0

    def cmd(self, **kw):
        self.lines.append(json.dumps(kw, separators=(",", ":")))

    def name(self):
        self.n += 1
        return f"s{self.n}"

    def group(self, name, build, material):
        g = self.name()
        self.cmd(cmd="begin_group", name=name, **{"as": g})
        build()
        self.cmd(cmd="end_group")
        self.cmd(cmd="paint", ids=["@" + g], material=material)
        return g

    def box(self, name, lo, hi, material):
        def build():
            f = self.name()
            self.cmd(cmd="rect", **{"from": [lo[0], lo[1], lo[2]], "to": [hi[0], hi[1], lo[2]]}, **{"as": f})
            self.cmd(cmd="pushpull", face="@" + f, distance=hi[2] - lo[2])

        return self.group(name, build, material)

    def poly(self, pts):
        self.cmd(cmd="polygon", points=[[round(c, 6) for c in p] for p in pts])

    def text(self):
        return "\n".join(self.lines) + "\n"


def sub(a, b):
    return [a[i] - b[i] for i in range(3)]


def dot(a, b):
    return sum(a[i] * b[i] for i in range(3))


def cross(a, b):
    return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]


def unit(a):
    l = math.sqrt(dot(a, a))
    return [c / l for c in a]


def dice():
    # 5 cm: a big foam die, easy to see and to texture; six separate square faces
    s = Script(["Dice: a 5 cm cube whose six faces are separately selectable (pips are texture-only).",
                "Tests: box projection, per-face texture placement. Origin: on the floor at the middle."])
    s.cmd(cmd="material", name="Ivory plastic", color=[0.95, 0.93, 0.86], roughness=0.35)
    s.box("Dice", [-0.025, -0.025, 0], [0.025, 0.025, 0.05], "Ivory plastic")
    return s.text()


def can():
    # a 330 ml drinks can: 66 mm across, 122 mm tall
    s = Script(["Can: a 66 x 122 mm drinks can, 32-sided with soft side edges, flat top and bottom caps.",
                "Tests: cylindrical projection, label wrap. Origin: on the floor at the middle."])
    s.cmd(cmd="material", name="Aluminium", color=[0.78, 0.8, 0.82], roughness=0.3, metalness=0.9)

    def body():
        f = s.name()
        s.cmd(cmd="circle", center=[0, 0, 0], radius=0.033, segments=32, **{"as": f})
        s.cmd(cmd="pushpull", face="@" + f, distance=0.122)

    s.group("Can", body, "Aluminium")
    return s.text()


def globe():
    s = Script(["Globe: a faceted 30 cm sphere (16 segments, 10 rings, triangles at the poles) on a base and stem.",
                "Tests: spherical projection. Origin: on the floor at the middle of the base."])
    s.cmd(cmd="material", name="Globe blue", color=[0.25, 0.45, 0.7], roughness=0.5)
    s.cmd(cmd="material", name="Stand brass", color=[0.72, 0.55, 0.25], roughness=0.35, metalness=0.8)
    r, cz, seg, rings = 0.15, 0.2, 16, 10

    def pt(i, j):
        lat = math.pi / 2 - math.pi * j / rings
        lon = 2 * math.pi * i / seg
        return [r * math.cos(lat) * math.cos(lon), r * math.cos(lat) * math.sin(lon), cz + r * math.sin(lat)]

    def sphere():
        for j in range(rings):
            for i in range(seg):
                a, b, c, d = pt(i, j), pt(i + 1, j), pt(i + 1, j + 1), pt(i, j + 1)
                if j == 0:
                    s.poly([a, d, c])
                elif j == rings - 1:
                    s.poly([a, d, b])
                else:
                    s.poly([a, d, c, b])

    s.group("Sphere", sphere, "Globe blue")

    def base():
        f = s.name()
        s.cmd(cmd="circle", center=[0, 0, 0], radius=0.1, segments=24, **{"as": f})
        s.cmd(cmd="pushpull", face="@" + f, distance=0.02)

    s.group("Base", base, "Stand brass")

    def stem():
        f = s.name()
        s.cmd(cmd="circle", center=[0, 0, 0.02], radius=0.012, segments=12, **{"as": f})
        s.cmd(cmd="pushpull", face="@" + f, distance=cz - r - 0.02)

    s.group("Stem", stem, "Stand brass")
    return s.text()


def crate():
    s = Script(["Crate: a 60 cm wooden crate: four corner posts, a floor and lid of five boards each, and four sides of five",
                "planks each, every one a separate closed box. Tests: paint-through across connected faces, planar projection.",
                "Origin: on the floor at the middle."])
    s.cmd(cmd="material", name="Pine", color=[0.76, 0.6, 0.38], roughness=0.8)
    s.cmd(cmd="material", name="Pine dark", color=[0.6, 0.45, 0.27], roughness=0.8)
    h, post, t, top, n = 0.3, 0.05, 0.02, 0.6, 5  # half width, post section, plank thickness, height, planks per side
    s.cmd(cmd="begin_group", name="Crate", **{"as": "crate"})
    for k, (sx, sy) in enumerate([(-1, -1), (1, -1), (1, 1), (-1, 1)], 1):
        x0, x1 = sorted([sx * h, sx * (h - post)])
        y0, y1 = sorted([sy * h, sy * (h - post)])
        s.box(f"Post {k}", [x0, y0, 0], [x1, y1, top], "Pine dark")
    inner = h - post  # side planks run between the posts
    pw = top / n
    for side, (ax, sgn) in enumerate([("y", -1), ("x", 1), ("y", 1), ("x", -1)], 1):
        o0, o1 = sorted([sgn * h, sgn * (h - t)])
        for k in range(n):
            z0 = k * pw
            if ax == "y":
                s.box(f"Side {side} plank {k + 1}", [-inner, o0, z0], [inner, o1, z0 + pw], "Pine")
            else:
                s.box(f"Side {side} plank {k + 1}", [o0, -inner, z0], [o1, inner, z0 + pw], "Pine")
    iw = h - t  # floor and lid: boards across, set between the sides
    for name, z0 in [("Floor", 0.0), ("Lid", top - t)]:
        for k in range(n):
            x0 = -iw + k * (2 * iw / n)
            s.box(f"{name} board {k + 1}", [x0, -iw, z0], [x0 + 2 * iw / n, iw, z0 + t], "Pine")
    s.cmd(cmd="end_group")
    return s.text()


def soccer():
    s = Script(["Soccer ball: a truncated icosahedron, 22 cm across: 12 pentagons and 20 hexagons, 32 flat faces, 90 edges.",
                "Tests: per-face paint, UV unwrap stress, spherical projection. Origin: on the floor at the middle, resting on a hexagon."])
    s.cmd(cmd="material", name="Ball white", color=[0.96, 0.96, 0.96], roughness=0.6)
    s.cmd(cmd="material", name="Ball black", color=[0.08, 0.08, 0.09], roughness=0.6)
    cyc = [(0, 1, 2), (1, 2, 0), (2, 0, 1)]
    verts = set()
    for base in [(0, 1, 3 * PHI), (1, 2 + PHI, 2 * PHI), (PHI, 2, 2 * PHI + 1)]:
        for perm in cyc:  # even permutations only, all sign changes
            p = [base[i] for i in perm]
            for sg in itertools.product([1, -1], repeat=3):
                verts.add(tuple(round(p[i] * sg[i], 9) + 0.0 for i in range(3)))
    verts = [list(v) for v in sorted(verts)]
    assert len(verts) == 60, len(verts)
    # face directions: icosahedron vertices (pentagons), dodecahedron vertices (hexagons)
    pent, hexa = set(), set()
    for perm in cyc:
        for s1, s2 in itertools.product([1, -1], repeat=2):
            v = [0.0, 0.0, 0.0]
            v[perm[1]], v[perm[2]] = s1 * 1.0, s2 * PHI
            pent.add(tuple(round(c, 9) + 0.0 for c in v))
            v = [0.0, 0.0, 0.0]
            v[perm[1]], v[perm[2]] = s1 * PHI, s2 / PHI
            hexa.add(tuple(round(c, 9) + 0.0 for c in v))
    for sg in itertools.product([1, -1], repeat=3):
        hexa.add(tuple(float(c) for c in sg))
    faces = [(unit(list(n)), 5) for n in sorted(pent)] + [(unit(list(n)), 6) for n in sorted(hexa)]
    assert len(faces) == 32, len(faces)
    # turn the ball so a hexagon is underneath
    down, up = faces[12][0], [0, 0, -1]
    axis = cross(down, up)
    sin, cos = math.sqrt(dot(axis, axis)), dot(down, up)
    k = [c / sin for c in axis]

    def rot(v):
        kv = cross(k, v)
        return [v[i] * cos + kv[i] * sin + k[i] * dot(k, v) * (1 - cos) for i in range(3)]

    scale = 0.11 / math.sqrt(dot(verts[0], verts[0]))
    placed = []
    for n, sides in faces:
        best = max(dot(w, n) for w in verts)
        fv = [v for v in verts if abs(dot(v, n) - best) < 1e-6]
        assert len(fv) == sides, (sides, len(fv))
        c = [sum(v[i] for v in fv) / sides for i in range(3)]
        e1 = unit(sub(fv[0], c))
        e2 = cross(n, e1)
        fv.sort(key=lambda v: math.atan2(dot(sub(v, c), e2), dot(sub(v, c), e1)))  # counter-clockwise from outside
        placed.append((sides, [rot([x * scale for x in v]) for v in fv]))
    lift = -min(p[2] for _, pts in placed for p in pts)

    ids = {5: [], 6: []}
    flat = []  # a polygon drawn on the ground comes out facing up; the one under the ball is turned over

    def ball():
        for sd, pts in placed:
            name = s.name()
            ids[sd].append("@" + name)
            if all(abs(p[2] + lift) < 1e-9 for p in pts):
                flat.append("@" + name)
            s.cmd(cmd="polygon", points=[[round(p[0], 6), round(p[1], 6), round(p[2] + lift, 6)] for p in pts], **{"as": name})

    g = s.name()
    s.cmd(cmd="begin_group", name="Soccer ball", **{"as": g})
    ball()
    s.cmd(cmd="end_group")
    if flat:
        s.cmd(cmd="reverse", ids=flat)
    s.cmd(cmd="paint", ids=ids[6], material="Ball white")
    s.cmd(cmd="paint", ids=ids[5], material="Ball black")
    return s.text()


MODELS = {"dice": dice, "can": can, "globe": globe, "crate": crate, "soccer_ball": soccer}

if __name__ == "__main__":
    stale = []
    for key, fn in MODELS.items():
        path = ASSETS / f"{key}.jsonl"
        text = fn()
        if "--check" in sys.argv:
            if not path.exists() or path.read_text() != text:
                stale.append(key)
        else:
            path.write_text(text)
    if stale:
        print("stale:", ", ".join(stale))
        sys.exit(1)
