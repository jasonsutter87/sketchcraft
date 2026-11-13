#!/usr/bin/env python3
"""Turn a googledriver twin block into a SketchCraft command script.

  python3 tools/twin_import.py <twin_dir> <out.jsonl>

Reads the block's OSM footprints (osm.json) and the building heights
googledriver measured on Google's 3D mesh (heights.json, pulled from
block_clay.blend with Blender if it isn't there yet). Each building becomes
a group: its footprint drawn at its base height and pulled up to its roof.
Streets become flat ribbons. Coordinates are the twin's local meters
(x east, y north), so the result lines up with the twin's panoramas.
"""

import json
import math
import os
import subprocess
import sys

BLENDER = os.environ.get("BLENDER", "/Applications/Blender.app/Contents/MacOS/Blender")
ROAD_WIDTH = {"primary": 14, "secondary": 12, "tertiary": 11, "residential": 9, "service": 5}

EXTRACT = """
import bpy, json, sys
out = {}
for o in bpy.data.collections['Buildings'].objects:
    zs = [(o.matrix_world @ v.co).z for v in o.data.vertices]
    out[o.name.replace('bldg_', '')] = {'base': min(zs), 'top': max(zs)}
json.dump(out, open(sys.argv[-1], 'w'))
"""


def heights(twin):
    path = os.path.join(twin, "heights.json")
    if not os.path.exists(path):
        subprocess.run([BLENDER, "-b", os.path.join(twin, "block_clay.blend"), "--python-expr", EXTRACT, "--", path],
                       check=True, capture_output=True)
    return json.load(open(path))


def signed_area(poly):
    return sum(poly[i][0] * poly[(i + 1) % len(poly)][1] - poly[(i + 1) % len(poly)][0] * poly[i][1] for i in range(len(poly))) / 2


def clean(poly):
    """Drop repeated and nearly collinear points (OSM has plenty)."""
    pts = [p for i, p in enumerate(poly) if i == 0 or math.dist(p, poly[i - 1]) > 0.05]
    if len(pts) > 1 and math.dist(pts[0], pts[-1]) < 0.05:
        pts.pop()
    out = []
    for i, p in enumerate(pts):
        a, c = pts[i - 1], pts[(i + 1) % len(pts)]
        cross = (p[0] - a[0]) * (c[1] - p[1]) - (p[1] - a[1]) * (c[0] - p[0])
        if abs(cross) > 0.02:
            out.append(p)
    return out


def main(twin, out_path):
    osm = json.load(open(os.path.join(twin, "osm.json")))
    hs = heights(twin)
    half = osm["half_m"]
    lines = [f"# {os.path.basename(os.path.normpath(twin))} block, from googledriver", '{"cmd":"new"}']
    footprints = []
    n = 0
    for b in osm["buildings"]:
        h = hs.get(str(b["id"]))
        poly = clean(b["poly"])
        if not h or len(poly) < 3 or not all(abs(x) < half + 20 and abs(y) < half + 20 for x, y in poly):
            continue
        if signed_area(poly) < 0:
            poly = poly[::-1]
        base, top = round(h["base"], 3), round(h["top"], 3)
        footprints.append((poly, base))
        n += 1
        name = f"b{b['id']}"
        pts = [[round(x, 3), round(y, 3), base] for x, y in poly]
        lines += [
            json.dumps({"cmd": "begin_group", "name": f"Building {n}"}),
            json.dumps({"cmd": "polygon", "points": pts, "as": name}),
            json.dumps({"cmd": "pushpull", "face": f"@{name}", "distance": round(top - base, 3)}),
            '{"cmd":"end_group"}',
        ]
    # Streets follow the ground: estimate it from nearby building bases
    # (inverse-distance weighted), split every few meters so slopes bend.
    anchors = [(sum(p[0] for p in c) / len(c), sum(p[1] for p in c) / len(c), z) for c, z in footprints]

    def ground(x, y):
        near = sorted(anchors, key=lambda a: (a[0] - x) ** 2 + (a[1] - y) ** 2)[:6]
        ws = [1 / max((a[0] - x) ** 2 + (a[1] - y) ** 2, 1.0) for a in near]
        return sum(w * a[2] for w, a in zip(ws, near)) / sum(ws) + 0.35  # bases sit 0.4 m under the curb

    lim = half + 15
    lines.append(json.dumps({"cmd": "begin_group", "name": "Streets", "as": "streets"}))
    for r in osm["roads"]:
        w = ROAD_WIDTH.get(r.get("kind"), 8) / 2
        line = r["line"]
        for (x1, y1), (x2, y2) in zip(line, line[1:]):
            dx, dy = x2 - x1, y2 - y1
            L = math.hypot(dx, dy)
            if L < 0.5:
                continue
            nx, ny = -dy / L * w, dx / L * w
            steps = max(1, math.ceil(L / 8))
            for k in range(steps):
                ax, ay = x1 + dx * k / steps, y1 + dy * k / steps
                bx, by = x1 + dx * (k + 1) / steps, y1 + dy * (k + 1) / steps
                if not (abs(ax) < lim and abs(ay) < lim and abs(bx) < lim and abs(by) < lim):
                    continue
                za, zb = round(ground(ax, ay), 3), round(ground(bx, by), 3)
                quad = [[ax + nx, ay + ny, za], [ax - nx, ay - ny, za], [bx - nx, by - ny, zb], [bx + nx, by + ny, zb]]
                if signed_area(quad) < 0:
                    quad = quad[::-1]
                lines.append(json.dumps({"cmd": "polygon", "points": [[round(x, 3), round(y, 3), z] for x, y, z in quad]}))
    lines.append('{"cmd":"end_group"}')
    lines.append('{"cmd":"paint","ids":["@streets"],"color":[0.42,0.43,0.46]}')
    open(out_path, "w").write("\n".join(lines) + "\n")
    print(f"{n} buildings -> {out_path}")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2])
