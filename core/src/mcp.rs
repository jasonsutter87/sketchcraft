//! MCP server over stdio (newline-delimited JSON-RPC 2.0), so an AI agent
//! can model in SketchCraft directly. Three tools:
//!
//!   sketchcraft_run     run a list of commands, get each reply
//!   sketchcraft_render  look at the model (returns a PNG image)
//!   sketchcraft_info    counts, bounds, groups
//!
//! With `--doc FILE` the model is loaded from and saved back to FILE after
//! every change, so the app (or a later session) sees the same model.

use crate::render::{self, RenderOpts};
use crate::Engine;
use base64::Engine as _;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

const REFERENCE: &str = r#"SketchCraft commands (JSON objects). Units meters, Z up, front of a street usually faces -Y. Add "as":"name" to remember a result id (face, group, instance, first of ids/edges); use "@name" anywhere an id goes.
DRAW
- rect {from, to}: axis-aligned rectangle (corners share one coordinate). On an existing face: strictly inside cuts a hole (window); touching its edge splits it (door at ground).
- polygon {points}: planar face (within 1 cm is flattened).
- line {points}: edges. Ending mid-edge splits it; edge-to-edge across a face cuts the face in two; closing a flat loop makes a face.
- circle {center, radius, normal?=[0,0,1], segments?=24}; arc {start, end, bulge, normal?, segments?=12}
- offset {face, distance}: + inward (new face inside), - outward (ring).
MODIFY
- pushpull {face, distance}: + out along normal, - in. Faces in a bigger surface carve recesses.
- follow_me {face, points | edges | around:faceId}: sweep a profile (cornice, trim, eave).
- move {ids, by}; rotate {ids, center, axis?=[0,0,1], angle(deg)}; scale {ids, center, factor: n | [x,y,z]}. Moving an edge/vertex reshapes faces (raise a ridge line for a gable; bent faces auto-fold).
- duplicate {ids, by}; array {ids, by, count}; copy {group, by}; delete {ids}; reverse {ids}; paint {ids, color:[r,g,b] 0..1}
ORGANIZE
- begin_group {name} / end_group; group {ids, name?}; edit {id} (open a group or component) / close
- make_component {ids, name}; place {component(id|name), at, rotate?}; explode {id}; components
- wall {points, thickness?=0.2, height?=2.7, closed?}; stairs {at, rise, direction?(deg), width?, tread?}; roof {face, kind?=gable|hip|shed, pitch?(deg), overhang?, thickness?}; column {at, size?, height?, round?}
- texture {name, data(data: URL), width?, height?, color?, project?:[x,y,z]|null, origin?}; paint {ids, material:name} paints with it
- units {units: m|cm|mm|ft|in} (display only; geometry is meters); dimension {from, to, offset}; text {at, text, leader?}
- intersect {ids, with?: model|selection}: Intersect Faces (draws the crossing lines, splitting faces). flip {ids, axis: red|green|blue}: Flip Along
- section {origin, normal, path?: .svg|.dxf}: section cut lines + filled regions per object; path writes it flat in mm
- Sandbox: terrain_from_contours {ids: contour edges/groups, spacing?=1}; terrain_grid {at?, width, depth, spacing?=1}; smoove {group, at, radius, height}; stamp {group, face: footprint, offset?=1}; drape {group, ids: edges}; flip_edge {edge}
- export_dae {path} (Collada), export_dxf {path} (3D faces, mm), export_pdf {path, title?, + render options}
- import_fbx {path | data(base64), at?, name?}: binary FBX 7.x -> one component, a group per mesh, material colors
- animate {path: .mp4|.mov|folder, fps?=24, transition?=2 s, delay?=1 s, views?: names, turntable?=8 s, + render options}: scene animation (turntable with <2 views)
- match_photo {name, width, height, red:[[x1,y1,x2,y2]x2], green:[..x2], origin:[x,y], distance?=15, photo?: data URL}: camera from a photo's vanishing lines, saved as a view with the photo
- sheets {path(.pdf), sheet?=a3|a4|a2|letter|tabloid, project?, pages?: [{title, viewports:[{view: saved name|iso|top|front|back|left|right, scale?: "1:100", cut?: meters (floor plan at that height), x?,y?,w?,h? mm, label?}]}]}: LayOut-style drawing sheets
- set_attrs {id: placement or component, attrs:{name: value|"=formula"}}: Dynamic Components (lenx/leny/lenz m, copies, spacing, material, hidden; formulas with min/max/round/if, parent!name). report {path?.csv}: Generate Report
- classify {ids, class: IfcWall|IfcSlab|IfcRoof|IfcDoor|IfcWindow|IfcColumn|IfcBeam|IfcStair|IfcRailing|IfcCovering|IfcFurniture|IfcPlate|IfcMember|"" }; export_ifc {path, project?}: IFC4 (unclassified things typed from their names)
- add_location {lat, lon, size?=300 m, cells?=60, imagery?=true}: real terrain (+ OpenStreetMap imagery) around a place; origin at ground, +Y north
- blueprint {path(.pdf), sheet?=a3, project?}: every floor plan + 4 elevations, white on blue, true scale. levels: floor heights. sheets also takes blueprint:true
- bevel {edges, size m, segments?=1 (chamfer) | >1 (round)}: outside edges of solid groups. divide {edge, count}. slice {id, origin, normal}: two pieces
- photoreal {out? (else data URL), samples?=32, bounces?=6, sun_azimuth? (deg from north; default lights the view), sun_altitude?=32, or date?="2026-06-21" + time?="15:30" (+ utc_offset?) for the real sun at the model location, exposure?, + render options (view/camera/width/height)}: path-traced render. material {name, color}: named material (glass/metal/light by name). save_image {path, data}
- skins: app skins in ~/Documents/SketchCraft/Skins; save_skin {name, skin}: save one (JSON: name, dark, tokens {bg, panel, line, line-strong, text, muted, faint, accent, accent-soft, accent-ink, danger, glass, radius, font}, backdrop?)
- hierarchy: groups nest. group {ids} puts groups/components inside a new group (faces become its geometry); set_parent {ids, parent|null}; ungroup {id}; lock {ids, locked}; begin_group inside an open group nests, end_group backs out one level. Moving/deleting/copying a group carries everything inside it.
- tags: tag {ids, tag}; tag_visible {tag, visible}; tag_color {tag, color|null}; rename_tag {tag, to}
- photo_solve {points:[{name?, marks:{viewName:[u,v]}}], edges?:[[i,j]], fixed?, scale?:{a,b,length}, place?=true, build?}: multi-photo modeling. Marks are photo-centered, y up, in half-photo-heights. Corners marked in 2+ matched photos (all matched with the origin on the same real corner) are solved to 3D along with each photo distance; returns points, misses (m), distances
- subd {name?, points, faces:[[i,j,k,l],...], materials?:[name|null per face], creases?:[[i,j]], levels?=2}: Catmull-Clark smooth surface from a cage (box modeling); subdivide {id, levels?, keep_sharp?}: smooth an existing group
- cage_edit {id, moves?:[[i,x,y,z]], mirror?=true, crease?/uncrease?:[[i,j]], op?:{kind: insert_loop {edge:[i,j], t} | extrude {faces, distance} | delete_faces {faces} | merge {points}}, levels?, symmetry?: "auto"|"off"|0|1|2}: edit a subd object's cage (scene.cages has it) and rebuild
- opacity {material, value 0-1}: see-through in the view (reference image boards)
- environment {kind: suburban|city|park|desert|waterfront|studio|none}: a ready-made setting around the model (locked group tagged Environment; studio = seamless backdrop + softbox lights, renders without sky/sun). pushpull also takes toward:[x,y,z]
- sun {date?, time?, utc_offset?}: real sun azimuth/altitude at the model location
- door_swing {hinge, strike, toward}: door swing symbol for plans. blueprint also takes notes:[...]; sheets takes notes:[...] (a line starting with a hash is a heading)
- solid {op: union|subtract|intersect|trim|split|outer_shell, ids:[group or instance, ...]}: solid tools on closed shapes; subtract/trim: the first cuts the second. volume {id} -> {solid, volume m3}
- import_skp {path | data(base64)}: SketchUp files, legacy 6-8 and modern 2013+ (geometry, nested components, colors, textures)
- insert {asset, at, rotate?}: built-ins door, window, garage_door, tree, palm, car, streetlight, bench, hydrant (doors/windows face -Y, origin bottom-left on the wall).
- hide {ids}; unhide {ids?}; tag {ids, tag}; tag_visible {tag, visible}; soften {ids | coplanar:true}; rename {id, name}
- guide {point, direction?}; clear_guides; save_view {name, eye, target, fov?}; delete_view {name}
INSPECT / FILES
- pick {at, facing?}: face under a point; faces {group?, facing?}; info; undo; redo; new
- save/open {path} (.sketchcraft); export_obj/export_glb {path}; export_stl {path, scale?}; import_obj {path, up?="y", scale?, at?}
- render {out, view?: iso|front|back|left|right|top|<saved view>, camera?, width?, height?}
Tips: build each house in its own group; pick faces by point after pulls; check your work with sketchcraft_render."#;

fn mtime(doc: &Option<String>) -> Option<std::time::SystemTime> {
    doc.as_ref().and_then(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
}

pub fn serve(mut engine: Engine, doc: Option<String>) {
    let mut seen = mtime(&doc);
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
        let id = msg.get("id").cloned();
        let method = msg["method"].as_str().unwrap_or("");
        let result = match method {
            "initialize" => Some(json!({
                "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18"),
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "sketchcraft", "version": env!("CARGO_PKG_VERSION")},
                "instructions": "SketchCraft is a push/pull 3D modeler. Build with sketchcraft_run, check your work with sketchcraft_render (look at the image), iterate."
            })),
            "ping" => Some(json!({})),
            "tools/list" => Some(json!({"tools": tools()})),
            "tools/call" => {
                // the app (or a person) may have changed the model since our last call
                let now = mtime(&doc);
                if now.is_some() && now != seen {
                    engine.exec_json(&json!({"cmd": "open", "path": doc}).to_string());
                }
                let r = call(&mut engine, &doc, &msg["params"]);
                seen = mtime(&doc);
                Some(r)
            }
            _ => None,
        };
        let Some(id) = id else { continue }; // notification
        let reply = match result {
            Some(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
            None => json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": format!("unknown method {method}")}}),
        };
        writeln!(out, "{reply}").ok();
        out.flush().ok();
    }
}

fn tools() -> Value {
    json!([
        {
            "name": "sketchcraft_run",
            "description": format!("Run SketchCraft modeling commands in order and return each reply. Stops at the first error (the failed command changes nothing).\n\n{REFERENCE}"),
            "inputSchema": {
                "type": "object",
                "properties": {"commands": {"type": "array", "items": {"type": "object"}, "description": "Command objects, e.g. {\"cmd\":\"rect\",\"from\":[0,0,0],\"to\":[8,12,0],\"as\":\"lot\"}"}},
                "required": ["commands"]
            }
        },
        {
            "name": "sketchcraft_render",
            "description": "Render the current model and return the image. view: iso (default), front, back, left, right, top; or camera {eye, target, fov}.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "view": {"type": "string"},
                    "camera": {"type": "object", "properties": {"eye": {"type": "array"}, "target": {"type": "array"}, "fov": {"type": "number"}}},
                    "width": {"type": "integer"},
                    "height": {"type": "integer"},
                    "edges": {"type": "boolean"}
                }
            }
        },
        {
            "name": "sketchcraft_info",
            "description": "Model summary: counts, bounds, groups, materials, and every face (id, normal, center, area).",
            "inputSchema": {"type": "object", "properties": {}}
        }
    ])
}

fn text(t: String, is_error: bool) -> Value {
    json!({"content": [{"type": "text", "text": t}], "isError": is_error})
}

fn call(engine: &mut Engine, doc: &Option<String>, params: &Value) -> Value {
    let args = &params["arguments"];
    match params["name"].as_str().unwrap_or("") {
        "sketchcraft_run" => {
            let Some(cmds) = args["commands"].as_array() else { return text("commands must be an array".into(), true) };
            let mut lines = Vec::new();
            let mut failed = false;
            for c in cmds {
                let r = engine.exec_json(&c.to_string());
                lines.push(r.to_string());
                if r["ok"] != true {
                    failed = true;
                    lines.push(format!("stopped at: {c}"));
                    break;
                }
            }
            if let Some(path) = doc {
                if let Some(dir) = std::path::Path::new(path).parent() {
                    std::fs::create_dir_all(dir).ok();
                }
                engine.exec_json(&json!({"cmd": "save", "path": path}).to_string());
            }
            text(lines.join("\n"), failed)
        }
        "sketchcraft_render" => {
            let mut opts: RenderOpts = serde_json::from_value(args.clone()).unwrap_or_default();
            opts.width = opts.width.min(1600);
            opts.height = opts.height.min(1200);
            let img = render::render(&engine.model, &opts);
            let mut png = std::io::Cursor::new(Vec::new());
            if let Err(e) = img.write_to(&mut png, image::ImageFormat::Png) {
                return text(e.to_string(), true);
            }
            let cam = render::camera_for(&engine.model, &opts);
            json!({"content": [
                {"type": "image", "mimeType": "image/png", "data": base64::engine::general_purpose::STANDARD.encode(png.into_inner())},
                {"type": "text", "text": format!("camera eye {:?} target {:?} fov {}", cam.eye, cam.target, cam.fov)}
            ]})
        }
        "sketchcraft_info" => {
            let info = engine.exec_json(r#"{"cmd":"info"}"#);
            let faces = engine.exec_json(r#"{"cmd":"faces"}"#);
            text(format!("{info}\n{}", faces["faces"]), false)
        }
        other => text(format!("unknown tool {other}"), true),
    }
}
