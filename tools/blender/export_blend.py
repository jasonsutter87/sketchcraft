"""Export a .blend for SketchCraft: run inside Blender.

    Blender -b model.blend --python tools/blender/export_blend.py -- out.json

Writes every mesh object (world-space vertices and faces, its Mirror and
Subdivision modifiers, material per face) and every image Empty (the
reference boards: where they sit, how big, which image) as JSON.
tools/blender/import_sketchcraft.py rebuilds that in SketchCraft.
"""
import json
import os
import sys

import bpy

out_path = sys.argv[sys.argv.index("--") + 1] if "--" in sys.argv else "blend.json"
base = os.path.dirname(bpy.data.filepath)
data = {"source": os.path.basename(bpy.data.filepath), "meshes": [], "boards": []}

for o in bpy.data.objects:
    if o.type == "MESH":
        mw = o.matrix_world
        mats = [s.material.name if s.material else None for s in o.material_slots]
        mesh = {
            "name": o.name,
            "verts": [list(mw @ v.co) for v in o.data.vertices],
            "faces": [list(p.vertices) for p in o.data.polygons],
            "face_materials": [mats[p.material_index] if p.material_index < len(mats) else None for p in o.data.polygons],
            "material_colors": {m.name: list(m.diffuse_color)[:3] for m in o.data.materials if m},
            "hidden": o.hide_get() or o.hide_render,
            "mirror": None,
            "subsurf": None,
            "creases": [],
        }
        for m in o.modifiers:
            if m.type == "MIRROR" and m.show_viewport:
                ref = m.mirror_object.matrix_world if m.mirror_object else mw
                mesh["mirror"] = {"axis": list(m.use_axis), "merge": m.use_mirror_merge, "threshold": m.merge_threshold,
                                  # mirrored about the object's (or mirror object's) origin, along its own axes, in world space
                                  "center": list(ref.translation),
                                  "normals": [list(ref.to_3x3().col[i].normalized()) for i in range(3)]}
            if m.type == "SUBSURF" and m.show_viewport:
                mesh["subsurf"] = {"levels": m.levels, "render_levels": m.render_levels}
        crease = o.data.attributes.get("crease_edge")
        if crease:
            for i, e in enumerate(o.data.edges):
                if crease.data[i].value > 0.5:
                    mesh["creases"].append([e.vertices[0], e.vertices[1]])
        data["meshes"].append(mesh)
    elif o.type == "EMPTY" and o.empty_display_type == "IMAGE" and o.data:
        img = o.data
        path = bpy.path.abspath(img.filepath, start=base) if img.filepath else None
        data["boards"].append({
            "name": o.name,
            "matrix": [list(r) for r in o.matrix_world],
            "size": o.empty_display_size,
            "offset": list(o.empty_image_offset),
            "image": path,
            "pixels": list(img.size),
            "opacity": o.color[3] if o.use_empty_image_alpha else 1.0,
        })

with open(out_path, "w") as f:
    json.dump(data, f)
print(f"exported {len(data['meshes'])} meshes and {len(data['boards'])} reference boards to {out_path}")
