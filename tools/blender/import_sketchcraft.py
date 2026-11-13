#!/usr/bin/env python3
"""Rebuild an exported .blend (tools/blender/export_blend.py) in SketchCraft.

    python3 tools/blender/import_sketchcraft.py blend.json out.sketchcraft [--levels 2] [--bridge http://127.0.0.1:5175]

Talks to a running `sketchcraft bridge`. For every mesh: applies its Mirror
modifier (welding points on the mirror plane, like Blender's merge), then
builds it as a SketchCraft subdivision surface (`subd`, Catmull-Clark like
Blender's Subdivision modifier) at --levels (default: the modifier's
level, or 2 when that's 0), and keeps the raw cage too, on a hidden "Cage"
tag. Every image Empty becomes a reference board: a face showing the same
image at the same place and size. Needs Pillow for the boards.
"""
import argparse
import base64
import io
import json
import math
import urllib.request

ap = argparse.ArgumentParser()
ap.add_argument("blend_json")
ap.add_argument("out")
ap.add_argument("--levels", type=int, default=None)
ap.add_argument("--bridge", default="http://127.0.0.1:5175")
args = ap.parse_args()


def ex(cmd):
    r = json.load(urllib.request.urlopen(urllib.request.Request(args.bridge + "/exec", json.dumps(cmd).encode()), timeout=600))
    if not r.get("ok"):
        raise SystemExit(f"{cmd.get('cmd')}: {r.get('error')}")
    return r


def sub(a, b): return [a[i] - b[i] for i in range(3)]
def add(a, b): return [a[i] + b[i] for i in range(3)]
def mul(a, k): return [x * k for x in a]
def dot(a, b): return sum(a[i] * b[i] for i in range(3))
def cross(a, b): return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
def norm(a):
    l = math.sqrt(dot(a, a)) or 1.0
    return [x / l for x in a]


def mirrored(mesh):
    """Apply a Mirror modifier: the mesh, plus its reflection; points within
    the merge distance of the plane are shared (welded)."""
    verts, faces, mats = [list(v) for v in mesh["verts"]], [list(f) for f in mesh["faces"]], list(mesh.get("face_materials") or [None] * len(mesh["faces"]))
    creases = [list(c) for c in mesh.get("creases", [])]
    mir = mesh.get("mirror")
    if not mir:
        return verts, faces, mats, creases
    for axis in range(3):
        if not mir["axis"][axis]:
            continue
        c, n = mir["center"], norm(mir["normals"][axis])
        thr = mir["threshold"] if mir["merge"] else -1.0
        index = {}
        n0 = len(verts)
        for i in range(n0):
            d = dot(sub(verts[i], c), n)
            if abs(d) <= thr:
                # on the plane: snap onto it, and the mirror shares it
                verts[i] = sub(verts[i], mul(n, d))
                index[i] = i
            else:
                index[i] = len(verts)
                verts.append(sub(verts[i], mul(n, 2 * d)))
        # reflected faces wind the other way, so they still face out
        faces += [[index[i] for i in reversed(f)] for f in faces[: len(faces)]]
        mats += mats[: len(mats)]
        creases += [[index[a], index[b]] for a, b in creases]
    return verts, faces, mats, creases


def basis(n):
    """SketchCraft's texture axes for a face looking along n (geom::basis)."""
    u = norm(cross([0, 0, 1], n)) if abs(n[2]) < 0.9 else norm(cross([1, 0, 0], n))
    return u, cross(n, u)


def board(b):
    """An image Empty -> a textured face, same place, same size, same way up."""
    from PIL import Image

    m = b["matrix"]
    col = lambda i: [m[0][i], m[1][i], m[2][i]]
    origin = col(3)
    sx, sy = math.sqrt(dot(col(0), col(0))), math.sqrt(dot(col(1), col(1)))
    R, U = norm(col(0)), norm(col(1))
    pw, ph = b["pixels"]
    # Blender: the image's longer side is the display size
    if pw >= ph:
        w, h = b["size"], b["size"] * ph / pw
    else:
        w, h = b["size"] * pw / ph, b["size"]
    w, h = w * sx, h * sy
    ox, oy = b["offset"]
    ll = add(origin, add(mul(R, ox * w), mul(U, oy * h)))
    corners = [ll, add(ll, mul(R, w)), add(add(ll, mul(R, w)), mul(U, h)), add(ll, mul(U, h))]
    # SketchCraft lays a projected texture along basis(-dir); choose the
    # direction so the face's normal is R x U, then resample the image so
    # (u, v) on the face lands on the same pixel Blender shows
    n = norm(cross(R, U))
    u, v = basis(n)
    side_u = abs(dot(u, R)) > 0.5
    W, H = (w, h) if side_u else (h, w)
    img = Image.open(b["image"]).convert("RGB")
    # texture pixel (x, y), y down, covers U' = x/tw, V' = 1 - y/th
    tw, th = (pw, ph) if side_u else (ph, pw)
    def st(Up, Vp):
        p = add(mul(u, Up * W), mul(v, Vp * H))
        return dot(p, R) / w, dot(p, U) / h
    # s, t are affine in U', V': find the map, shifted into [0, 1)
    s0, t0 = st(0, 0)
    su, tu_ = [a - b_ for a, b_ in zip(st(1, 0), (s0, t0))]
    sv, tv_ = [a - b_ for a, b_ in zip(st(0, 1), (s0, t0))]
    shift_s = -min(0.0, su) - min(0.0, sv)
    shift_t = -min(0.0, tu_) - min(0.0, tv_)
    # input pixel X = s*pw, Y = (1-t)*ph, with U' = x/tw, V' = 1 - y/th
    a = su * pw / tw
    bb = -sv * pw / th
    c = (s0 + shift_s + sv) * pw
    d = -tu_ * ph / tw
    e = tv_ * ph / th
    f = (1 - (t0 + shift_t + tv_)) * ph
    out = img.transform((tw, th), Image.AFFINE, (a, bb, c, d, e, f), resample=Image.BICUBIC)
    buf = io.BytesIO()
    out.save(buf, "JPEG", quality=88)
    data = "data:image/jpeg;base64," + base64.b64encode(buf.getvalue()).decode()
    return corners, data, W, H, mul(n, -1), ll


src = json.load(open(args.blend_json))
ex({"cmd": "new"})
made = []
for mesh in src["meshes"]:
    verts, faces, mats, creases = mirrored(mesh)
    sub_levels = (mesh.get("subsurf") or {}).get("levels", 0)
    levels = args.levels if args.levels is not None else (sub_levels if sub_levels > 0 else 2)
    colors = mesh.get("material_colors") or {}
    for name, c in colors.items():
        ex({"cmd": "material", "name": name, "color": c})
    smooth = ex({"cmd": "subd", "name": f"{mesh['name']} (smooth, level {levels})", "points": verts, "faces": faces, "materials": mats, "creases": creases, "levels": levels})["group"]
    cage = ex({"cmd": "subd", "name": f"{mesh['name']} (cage)", "points": verts, "faces": faces, "materials": mats, "levels": 0})["group"]
    ex({"cmd": "tag", "ids": [cage], "tag": "Cage"})
    made.append((mesh["name"], len(verts), len(faces), levels, smooth))
ex({"cmd": "tag_visible", "tag": "Cage", "visible": False})

for i, b in enumerate(src["boards"]):
    corners, data, W, H, d, o = board(b)
    name = f"Reference {i + 1} ({b['name']})"
    g = ex({"cmd": "begin_group", "name": name})["group"]
    ex({"cmd": "polygon", "points": corners})
    ex({"cmd": "end_group"})
    ex({"cmd": "texture", "name": name, "data": data, "width": W, "height": H, "project": d, "origin": o})
    ex({"cmd": "paint", "ids": [g], "material": name})
    ex({"cmd": "tag", "ids": [g], "tag": "References"})
ex({"cmd": "tag_visible", "tag": "References", "visible": True})
ex({"cmd": "save", "path": args.out})
for name, nv, nf, lv, g in made:
    print(f"{name}: cage {nv} points / {nf} faces (mirror applied), smooth at level {lv}")
print(f"{len(src['boards'])} reference boards; saved {args.out}")
