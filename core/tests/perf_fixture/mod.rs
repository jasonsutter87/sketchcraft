//! The big drawing set of design §4, made in code (a file of it would be tens of megabytes): a two-storey house
//! with one group per wall, slab, column and stair, eight tags with cut hatches, furniture as components, and a
//! 52-page A1 set: 12 plans (each with a key plan), 12 elevations, 6 sections, 20 details, 4 parallel 3D views at
//! 40%, 6 schedules, a cover and notes. 70 viewports, about 2,000 dimensions (half of them chains) and 400 text
//! blocks at full size.
//!
//! `Size::full()` is the benchmark (about 200,000 faces); `Size::small()` keeps the same pages and viewports with
//! little geometry, for the tests that run on every PR.

#![allow(dead_code)]

use serde_json::{json, Value};
use sketchcraft::model::Id;
use sketchcraft::Engine;

#[derive(Clone, Copy, Debug)]
pub struct Size {
    /// boxes in the furniture component (6 faces each)
    pub furniture_boxes: usize,
    /// furniture placements per storey
    pub furniture_per_floor: usize,
    /// sheet dimensions, and text blocks
    pub dims: usize,
    pub texts: usize,
}

impl Size {
    pub fn full() -> Size {
        // 2 storeys x 420 placements x 40 boxes x 6 faces = 201,600 faces, plus the building
        Size { furniture_boxes: 40, furniture_per_floor: 420, dims: 2000, texts: 400 }
    }
    pub fn small() -> Size {
        Size { furniture_boxes: 2, furniture_per_floor: 6, dims: 140, texts: 40 }
    }
}

pub const W: f64 = 20.0;
pub const D: f64 = 12.0;
pub const STOREY: f64 = 3.0;
pub const WALL_H: f64 = 2.75;
pub const EXT: f64 = 0.3;
pub const INT: f64 = 0.15;

/// The eight tags with a cut hatch, and the hatch each has.
pub const TAGS: [(&str, &str); 8] = [("ext-wall", "brick"), ("int-wall", "insulation"), ("slab", "concrete"), ("footing", "earth"), ("column", "steel"), ("stair", "wood"), ("roof", "diagonal"), ("parapet", "cross")];

pub struct Fixture {
    pub e: Engine,
    /// every sheet, in order
    pub sheets: Vec<Id>,
    /// every viewport item, with its label
    pub viewports: Vec<(Id, String)>,
    /// the interior wall the tests move: the one at x = 5 m in the south half of the ground floor
    pub wall: Id,
    /// faces in the model with every component placement counted
    pub faces: usize,
}

pub fn ok(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

/// A solid box in whatever is open.
fn block(e: &mut Engine, lo: [f64; 3], hi: [f64; 3]) {
    let f = ok(e, json!({"cmd": "rect", "from": [lo[0], lo[1], lo[2]], "to": [hi[0], hi[1], lo[2]]}))["face"].as_u64().unwrap();
    let up = e.model.normal(f as _).z > 0.0;
    let h = hi[2] - lo[2];
    ok(e, json!({"cmd": "pushpull", "face": f, "distance": if up { h } else { -h }}));
}

/// A group holding one box, tagged.
fn part(e: &mut Engine, name: &str, tag: &str, lo: [f64; 3], hi: [f64; 3]) -> Id {
    let g = ok(e, json!({"cmd": "begin_group", "name": name}))["group"].as_u64().unwrap();
    block(e, lo, hi);
    ok(e, json!({"cmd": "end_group"}));
    ok(e, json!({"cmd": "tag", "ids": [g], "tag": tag}));
    g as Id
}

fn id_of(r: &Value, keys: &[&str]) -> Id {
    for k in keys {
        if let Some(n) = r[*k].as_u64() {
            return n as Id;
        }
        if let Some(n) = r[*k].as_array().and_then(|a| a.first()).and_then(Value::as_u64) {
            return n as Id;
        }
    }
    panic!("no id in {r}");
}

/// The house and its drawing set.
pub fn build(size: Size) -> Fixture {
    let mut e = Engine::new();
    ok(&mut e, json!({"cmd": "begin_batch"}));
    let mut wall = 0;
    for s in 0..2 {
        let z0 = s as f64 * STOREY;
        let top = z0 + WALL_H;
        // the slab under the storey (the ground floor's on a footing)
        part(&mut e, &format!("Slab {s}"), "slab", [0.0, 0.0, z0 - 0.25], [W, D, z0]);
        if s == 0 {
            part(&mut e, "Footing", "footing", [-0.3, -0.3, -0.85], [W + 0.3, D + 0.3, -0.25]);
        }
        // exterior walls, butted at the corners (south and north run the full length)
        part(&mut e, &format!("Wall S{s}"), "ext-wall", [0.0, 0.0, z0], [W, EXT, top]);
        part(&mut e, &format!("Wall N{s}"), "ext-wall", [0.0, D - EXT, z0], [W, D, top]);
        part(&mut e, &format!("Wall W{s}"), "ext-wall", [0.0, EXT, z0], [EXT, D - EXT, top]);
        part(&mut e, &format!("Wall E{s}"), "ext-wall", [W - EXT, EXT, z0], [W, D - EXT, top]);
        // interior walls: across at x = 5, 10, 15 (south and north halves), and along y = 6 between them
        for (i, x) in [5.0, 10.0, 15.0].iter().enumerate() {
            let a = part(&mut e, &format!("Wall I{s}-{i}a"), "int-wall", [*x, EXT, z0], [x + INT, D / 2.0, top]);
            if s == 0 && i == 0 {
                wall = a;
            }
            part(&mut e, &format!("Wall I{s}-{i}b"), "int-wall", [*x, D / 2.0 + INT, z0], [x + INT, D - EXT, top]);
        }
        for (i, (x0, x1)) in [(EXT, 5.0), (5.0 + INT, 10.0), (10.0 + INT, 15.0), (15.0 + INT, W - EXT)].iter().enumerate() {
            part(&mut e, &format!("Wall L{s}-{i}"), "int-wall", [*x0, D / 2.0, z0], [*x1, D / 2.0 + INT, top]);
        }
        // columns in the rooms
        for (i, (x, y)) in [(2.5, 3.0), (7.5, 9.0), (12.5, 3.0), (17.5, 9.0)].iter().enumerate() {
            part(&mut e, &format!("Column {s}-{i}"), "column", [*x, *y, z0], [x + 0.3, y + 0.3, top]);
        }
    }
    // the roof and its parapet, and a stair between the storeys
    part(&mut e, "Roof", "roof", [0.0, 0.0, STOREY + WALL_H], [W, D, STOREY + WALL_H + 0.3]);
    part(&mut e, "Parapet", "parapet", [0.0, 0.0, STOREY + WALL_H + 0.3], [W, EXT, STOREY + WALL_H + 1.2]);
    let sg = ok(&mut e, json!({"cmd": "begin_group", "name": "Stair"}))["group"].as_u64().unwrap();
    for k in 0..15 {
        let x = 11.0 + k as f64 * 0.27;
        block(&mut e, [x, 7.0, 0.0], [x + 0.27, 8.0, (k + 1) as f64 * 0.2]);
    }
    ok(&mut e, json!({"cmd": "end_group"}));
    ok(&mut e, json!({"cmd": "tag", "ids": [sg], "tag": "stair"}));
    // every tag's cut is hatched
    for (t, p) in TAGS {
        ok(&mut e, json!({"cmd": "tag_style", "tag": t, "hatch": {"pattern": p}, "cut": {"fill": "hatch"}}));
    }
    // furniture: one component of many small boxes, placed in every room
    let g = ok(&mut e, json!({"cmd": "begin_group", "name": "Desk"}))["group"].as_u64().unwrap();
    for b in 0..size.furniture_boxes {
        let (i, j) = ((b % 8) as f64, (b / 8) as f64);
        block(&mut e, [i * 0.1, j * 0.1, 0.0], [i * 0.1 + 0.08, j * 0.1 + 0.08, 0.4 + 0.02 * (b % 5) as f64]);
    }
    ok(&mut e, json!({"cmd": "end_group"}));
    let made = ok(&mut e, json!({"cmd": "make_component", "ids": [g], "name": "Desk"}));
    let def = made["component"].as_u64().unwrap();
    ok(&mut e, json!({"cmd": "delete", "ids": [made["instance"].as_u64().unwrap()]}));
    for s in 0..2 {
        for k in 0..size.furniture_per_floor {
            // a grid over the floor, clear of the walls (rooms are 5 m by 6 m)
            let cols = 20;
            let (c, r) = (k % cols, k / cols);
            let x = 0.6 + (c as f64) * 0.95 + if (c % 5) == 4 { 0.2 } else { 0.0 };
            let y = 0.6 + (r as f64 % 11.0) * 1.0 + if r % 11 >= 5 { 0.4 } else { 0.0 };
            ok(&mut e, json!({"cmd": "place", "component": def, "at": [x.min(W - 1.2), y.min(D - 1.2), s as f64 * STOREY + 0.02 * (r / 11) as f64]}));
        }
    }
    ok(&mut e, json!({"cmd": "end_batch"}));

    let faces = e.model.world_faces().len();
    let (sheets, viewports) = sheets(&mut e, size);
    Fixture { e, sheets, viewports, wall, faces }
}

fn add_sheet(e: &mut Engine, name: &str, order: i32) -> Id {
    let r = ok(e, json!({"cmd": "sheet_add", "name": name, "paper": "a1", "order": order, "fields": {"project": "Permit set"}}));
    id_of(&r, &["sheet", "id"])
}

#[allow(clippy::too_many_arguments)]
fn add_vp(e: &mut Engine, out: &mut Vec<(Id, String)>, sheet: Id, label: &str, view: Value, scale: f64, rect: [f64; 4], anchor: [f64; 3], opacity: Option<f64>) -> Id {
    let mut c = json!({"cmd": "viewport_add", "sheet": sheet, "view": view, "scale": scale, "rect_mm": rect, "anchor": anchor, "label": label});
    if let Some(o) = opacity {
        c["opacity"] = json!(o);
    }
    let id = id_of(&ok(e, c), &["viewport", "id", "ids", "viewports"]);
    out.push((id, label.to_string()));
    id
}

/// The 52 pages.
fn sheets(e: &mut Engine, size: Size) -> (Vec<Id>, Vec<(Id, String)>) {
    ok(e, json!({"cmd": "begin_batch"}));
    let mut sheets = Vec::new();
    let mut vps = Vec::new();
    let mut order = 0;
    let mut next = |e: &mut Engine, name: &str, sheets: &mut Vec<Id>| {
        order += 1;
        let s = add_sheet(e, name, order);
        sheets.push(s);
        s
    };
    let plan = |s: usize| json!({"type": "plan", "cut_z": s as f64 * STOREY + 1.2, "floor_z": s as f64 * STOREY});
    let mid = |s: usize| [W / 2.0, D / 2.0, s as f64 * STOREY + 1.2];
    // cover and notes: a 3D view and key plans
    let cover = next(e, "Cover", &mut sheets);
    add_vp(e, &mut vps, cover, "Cover view", json!({"type": "view3d", "eye": [40.0, -30.0, 25.0], "target": [W / 2.0, D / 2.0, 2.0], "fov": 35.0, "parallel": true}), 100.0, [40.0, 40.0, 500.0, 400.0], [W / 2.0, D / 2.0, 2.0], None);
    add_vp(e, &mut vps, cover, "Key plan ground", plan(0), 200.0, [600.0, 60.0, 120.0, 80.0], mid(0), None);
    for n in 0..5 {
        let s = next(e, &format!("General notes {}", n + 1), &mut sheets);
        if n < 2 {
            add_vp(e, &mut vps, s, &format!("Notes key plan {n}"), plan(n), 200.0, [600.0, 60.0, 120.0, 80.0], mid(n), None);
        }
    }
    // 12 plans: 6 per storey, each with a key plan
    for s in 0..2 {
        for k in ["Floor", "Furniture", "Dimension", "Finishes", "Electrical", "Reflected"] {
            let sh = next(e, &format!("{k} plan {s}"), &mut sheets);
            add_vp(e, &mut vps, sh, &format!("{k} plan {s}"), plan(s), 50.0, [30.0, 30.0, 560.0, 380.0], mid(s), None);
            add_vp(e, &mut vps, sh, &format!("{k} key {s}"), plan(s), 200.0, [640.0, 40.0, 120.0, 80.0], mid(s), None);
        }
    }
    // 8 elevation pages: the four sides at 1:50, then again at 1:100 with a 1:20 bay at the east end
    let sides: [(&str, [f64; 3]); 4] = [("South", [0.0, -1.0, 0.0]), ("North", [0.0, 1.0, 0.0]), ("West", [-1.0, 0.0, 0.0]), ("East", [1.0, 0.0, 0.0])];
    for (name, dir) in sides {
        let sh = next(e, &format!("{name} elevation"), &mut sheets);
        add_vp(e, &mut vps, sh, &format!("{name} elevation"), json!({"type": "elevation", "dir": dir}), 50.0, [30.0, 30.0, 560.0, 260.0], [W / 2.0, D / 2.0, 3.0], None);
    }
    for (name, dir) in sides {
        let sh = next(e, &format!("{name} elevation 1:100"), &mut sheets);
        add_vp(e, &mut vps, sh, &format!("{name} elevation 1:100"), json!({"type": "elevation", "dir": dir}), 100.0, [30.0, 30.0, 300.0, 120.0], [W / 2.0, D / 2.0, 3.0], None);
        // the bay at the far right as seen from that side
        let bay = match name {
            "South" => [18.5, D / 2.0, 1.5],
            "North" => [1.5, D / 2.0, 1.5],
            "West" => [W / 2.0, 1.5, 1.5],
            _ => [W / 2.0, D - 1.5, 1.5],
        };
        add_vp(e, &mut vps, sh, &format!("{name} bay 1:20"), json!({"type": "elevation", "dir": dir}), 20.0, [380.0, 30.0, 60.0, 80.0], bay, None);
    }
    // 6 sections: three across (looking +x), three along (looking +y)
    for (i, x) in [2.5, 7.5, 17.5].iter().enumerate() {
        let sh = next(e, &format!("Section A{i}"), &mut sheets);
        add_vp(e, &mut vps, sh, &format!("Section A{i}"), json!({"type": "section", "origin": [x, D / 2.0, 0.0], "normal": [1.0, 0.0, 0.0]}), 50.0, [30.0, 30.0, 400.0, 300.0], [*x, D / 2.0, 3.0], None);
    }
    for (i, y) in [3.0, 9.0, 11.0].iter().enumerate() {
        let sh = next(e, &format!("Section B{i}"), &mut sheets);
        add_vp(e, &mut vps, sh, &format!("Section B{i}"), json!({"type": "section", "origin": [W / 2.0, y, 0.0], "normal": [0.0, 1.0, 0.0]}), 50.0, [30.0, 30.0, 560.0, 300.0], [W / 2.0, *y, 3.0], None);
    }
    // 10 detail pages, two 1:20 plan details each: the corners and the far ends of each storey
    let spots = [(1.0, 1.0), (W - 1.0, 1.0), (1.0, D - 1.0), (W - 1.0, D - 1.0), (17.5, 6.0)];
    for (n, (x, y)) in spots.iter().enumerate() {
        for s in 0..2 {
            let sh = next(e, &format!("Details {n}-{s}"), &mut sheets);
            add_vp(e, &mut vps, sh, &format!("Detail {n}-{s} a"), plan(s), 20.0, [30.0, 30.0, 100.0, 100.0], [*x, *y, s as f64 * STOREY + 1.2], None);
            add_vp(e, &mut vps, sh, &format!("Detail {n}-{s} b"), plan(s), 10.0, [200.0, 30.0, 100.0, 100.0], [*x, *y, s as f64 * STOREY + 1.2], None);
        }
    }
    // 4 parallel 3D views at 40%
    for (i, eye) in [[40.0, -30.0, 25.0], [-20.0, -30.0, 25.0], [-20.0, 40.0, 25.0], [40.0, 40.0, 25.0]].iter().enumerate() {
        let sh = next(e, &format!("Axonometric {i}"), &mut sheets);
        add_vp(e, &mut vps, sh, &format!("Axonometric {i}"), json!({"type": "view3d", "eye": eye, "target": [W / 2.0, D / 2.0, 2.0], "fov": 35.0, "parallel": true}), 100.0, [60.0, 60.0, 500.0, 400.0], [W / 2.0, D / 2.0, 2.0], Some(0.4));
    }
    // 6 schedules
    for n in 0..6 {
        let sh = next(e, &format!("Schedule {}", n + 1), &mut sheets);
        let mut rows = vec![vec!["Mark".to_string(), "Type".into(), "Width".into(), "Height".into(), "Notes".into()]];
        for r in 0..30 {
            rows.push(vec![format!("{}{r:02}", ["D", "W", "R", "F", "L", "E"][n]), format!("Type {}", r % 4), format!("{}", 800 + r * 10), "2100".into(), format!("note {r}")]);
        }
        insert(e, json!({"kind": "table", "sheet": sh, "at_mm": [20.0, 24.0], "cols_mm": [20.0, 30.0, 20.0, 20.0, 40.0], "rows": rows}));
    }
    assert_eq!(sheets.len(), 52, "the set has 52 pages");
    ok(e, json!({"cmd": "end_batch"}));

    // dimensions on the plan pages: half single, half chains along the walls' corners
    let plan_sheets: Vec<(Id, usize)> = sheets.iter().enumerate().filter(|(i, _)| (6..18).contains(i)).map(|(i, s)| (*s, (i - 6) / 6)).collect();
    ok(e, json!({"cmd": "begin_batch"}));
    let corners = |e: &Engine, s: usize| -> Vec<Id> {
        // bottom corners of the storey's walls, south to north then west to east
        let z = s as f64 * STOREY;
        let mut v: Vec<(Id, [f64; 3])> = e.model.vertices.iter().filter(|(_, v)| (v.p.z - z).abs() < 1e-6 && v.group.is_some_and(|g| e.model.groups[&g].name.starts_with("Wall"))).map(|(id, v)| (*id, v.p.to_array())).collect();
        v.sort_by(|a, b| (a.1[1], a.1[0]).partial_cmp(&(b.1[1], b.1[0])).unwrap());
        v.into_iter().map(|(id, _)| id).collect()
    };
    let cs = [corners(e, 0), corners(e, 1)];
    for k in 0..size.dims {
        let (sheet, s) = plan_sheets[k % plan_sheets.len()];
        let c = &cs[s];
        let n = if k % 2 == 0 { 2 } else { 3 + k % 4 };
        let start = (k * 7) % (c.len() - n);
        let anchors: Vec<Value> = (0..n).map(|j| json!({"vertex": c[start + j]})).collect();
        ok(e, json!({"cmd": "sheet_dim", "sheet": sheet, "anchors": anchors, "offset_mm": 4.0 + (k % 9) as f64}));
    }
    ok(e, json!({"cmd": "end_batch"}));
    // text blocks (kept on the sheets; drawn when text is, step 14)
    for k in 0..size.texts {
        let sheet = sheets[k % sheets.len()];
        insert(e, json!({"kind": "text", "sheet": sheet, "at_mm": [620.0, 200.0 + (k % 30) as f64 * 9.0], "paragraphs": [format!("Note {k}: walls to be built as drawn; check all dimensions on site before work starts.")]}));
    }
    (sheets, vps)
}

/// Put a sheet item straight into the model (kinds with no command yet: tables, text).
pub fn insert(e: &mut Engine, item: Value) {
    let it: sketchcraft::drafting::SheetItem = serde_json::from_value(item).unwrap();
    e.model.next_id += 1;
    let id = e.model.next_id;
    e.model.sheet_items.insert(id, std::sync::Arc::new(it));
}
