//! The woodworker pack, part two (docs/dev/woodworker.md): one DXF per part for a CNC router, and joinery between boards
//! (dado, rabbet, tenon). The fixture is the same small base cabinet as `cut_plan.rs`.
use serde_json::{json, Value};
use sketchcraft::policy::{Caller, Workspace};
use sketchcraft::Engine;
use std::collections::BTreeMap;

fn run(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn fails(e: &mut Engine, v: Value) -> String {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], false, "{v} should fail: {r}");
    r["error"].as_str().unwrap_or_default().to_string()
}

fn board(e: &mut Engine, name: &str, lo: [f64; 3], hi: [f64; 3]) -> u64 {
    let g = run(e, json!({"cmd": "begin_group", "name": name}))["group"].as_u64().unwrap();
    let f = run(e, json!({"cmd": "rect", "from": [lo[0], lo[1], lo[2]], "to": [hi[0], hi[1], lo[2]]}))["face"].as_u64().unwrap();
    run(e, json!({"cmd": "pushpull", "face": f, "distance": hi[2] - lo[2], "toward": [0, 0, 1]}));
    run(e, json!({"cmd": "end_group"}));
    g
}

const T: f64 = 0.018;
const W: f64 = 0.6;
const D: f64 = 0.56;
const H: f64 = 0.72;

type Ids = BTreeMap<&'static str, u64>;

/// The carcass of the base cabinet: 2 sides, bottom, top, 2 shelves (set back 20 mm, so their dados stop short of the
/// edges) and a 6 mm back.
fn cabinet(e: &mut Engine) -> Ids {
    let specs: [(&'static str, [f64; 3], [f64; 3]); 7] = [
        ("Side left", [0.0, 0.0, 0.0], [T, D, H]),
        ("Side right", [W - T, 0.0, 0.0], [W, D, H]),
        ("Bottom", [T, 0.0, 0.1], [W - T, D, 0.1 + T]),
        ("Top", [T, 0.0, H - T], [W - T, D, H]),
        ("Shelf 1", [T, 0.02, 0.3], [W - T, D - 0.02, 0.3 + T]),
        ("Shelf 2", [T, 0.02, 0.5], [W - T, D - 0.02, 0.5 + T]),
        ("Back", [T, D - 0.006, 0.0], [W - T, D, H]),
    ];
    specs.into_iter().map(|(n, lo, hi)| (n, board(e, n, lo, hi))).collect()
}

fn scratch(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("sc-cnc-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

// ----- reading a DXF back -------------------------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
struct Ent {
    kind: String,
    layer: String,
    codes: Vec<(i32, String)>,
}

impl Ent {
    fn f(&self, code: i32) -> f64 {
        self.codes.iter().find(|c| c.0 == code).unwrap_or_else(|| panic!("{self:?} has no {code}")).1.trim().parse().unwrap()
    }
    fn all(&self, code: i32) -> Vec<f64> {
        self.codes.iter().filter(|c| c.0 == code).map(|c| c.1.trim().parse().unwrap()).collect()
    }
}

struct Dxf {
    insunits: i32,
    layers: Vec<String>,
    ents: Vec<Ent>,
}

fn parse(text: &str) -> Dxf {
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len() % 2, 0, "pairs of lines");
    let pairs: Vec<(i32, String)> = lines.chunks(2).map(|c| (c[0].trim().parse().unwrap(), c[1].to_string())).collect();
    assert_eq!(pairs.last().unwrap(), &(0, "EOF".to_string()));
    let mut d = Dxf { insunits: 0, layers: vec![], ents: vec![] };
    let mut section = String::new();
    let mut i = 0;
    while i < pairs.len() {
        let (c, v) = &pairs[i];
        if *c == 9 && v == "$INSUNITS" {
            d.insunits = pairs[i + 1].1.trim().parse().unwrap();
        }
        if *c == 0 && v == "SECTION" {
            section = pairs[i + 1].1.clone();
        }
        if section == "TABLES" && *c == 0 && v == "LAYER" {
            let name = pairs[i..].iter().find(|p| p.0 == 2).unwrap().1.clone();
            d.layers.push(name);
        }
        if section == "ENTITIES" && *c == 0 && v != "SECTION" && v != "ENDSEC" && v != "EOF" {
            let mut e = Ent { kind: v.clone(), ..Default::default() };
            i += 1;
            while i < pairs.len() && pairs[i].0 != 0 {
                if pairs[i].0 == 8 {
                    e.layer = pairs[i].1.clone();
                }
                e.codes.push(pairs[i].clone());
                i += 1;
            }
            d.ents.push(e);
            continue;
        }
        i += 1;
    }
    d
}

impl Dxf {
    fn on(&self, layer: &str) -> Vec<&Ent> {
        self.ents.iter().filter(|e| e.layer == layer).collect()
    }
    fn arcs(&self) -> Vec<&Ent> {
        self.ents.iter().filter(|e| e.kind == "ARC").collect()
    }
    /// The bounding box of the OUTLINE layer, in mm: [min x, min y, max x, max y].
    fn bbox(&self, layer: &str) -> [f64; 4] {
        let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for e in self.on(layer) {
            let (xs, ys): (Vec<f64>, Vec<f64>) = match e.kind.as_str() {
                "LWPOLYLINE" => (e.all(10), e.all(20)),
                "LINE" => (vec![e.f(10), e.f(11)], vec![e.f(20), e.f(21)]),
                _ => continue,
            };
            for x in xs {
                b[0] = b[0].min(x);
                b[2] = b[2].max(x);
            }
            for y in ys {
                b[1] = b[1].min(y);
                b[3] = b[3].max(y);
            }
        }
        b
    }
    /// Does every line and arc on `layer` join up with others at both ends (a closed run of pieces)?
    fn closed(&self, layer: &str) -> bool {
        let mut ends: Vec<[f64; 2]> = vec![];
        for e in self.on(layer) {
            match e.kind.as_str() {
                "LWPOLYLINE" => {
                    assert_eq!(e.f(70) as i32 & 1, 1, "a closed polyline");
                    return true;
                }
                "LINE" => ends.extend([[e.f(10), e.f(20)], [e.f(11), e.f(21)]]),
                "ARC" => {
                    let (c, r) = ([e.f(10), e.f(20)], e.f(40));
                    for a in [e.f(50), e.f(51)] {
                        ends.push([c[0] + r * a.to_radians().cos(), c[1] + r * a.to_radians().sin()]);
                    }
                }
                _ => {}
            }
        }
        !ends.is_empty() && ends.iter().all(|p| ends.iter().filter(|q| (p[0] - q[0]).hypot(p[1] - q[1]) < 1e-3).count() == 2)
    }
}

fn files_of(r: &Value) -> Vec<(String, u64, String)> {
    r["files"].as_array().unwrap().iter().map(|f| (f["file"].as_str().unwrap().to_string(), f["id"].as_u64().unwrap(), f["name"].as_str().unwrap().to_string())).collect()
}

fn near(a: f64, b: f64, tol: f64, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: {a} vs {b}");
}

// ----- CNC files -----------------------------------------------------------------------------------------------------

#[test]
fn every_part_gets_a_dxf_in_the_cut_plans_numbering() {
    let mut e = Engine::new();
    let ids = cabinet(&mut e);
    let dir = scratch("each");
    let r = run(&mut e, json!({"cmd": "export_cnc", "dir": dir.to_string_lossy()}));
    assert_eq!(r["count"], 7);
    let plan = run(&mut e, json!({"cmd": "cut_plan"}));
    let order: Vec<u64> = plan["parts"].as_array().unwrap().iter().map(|p| p["id"].as_u64().unwrap()).collect();
    let files = files_of(&r);
    assert_eq!(files.iter().map(|f| f.1).collect::<Vec<_>>(), order, "the same numbers as the cut plan");
    // numbered, padded, named by the part: the same names every time
    for (i, (file, _, name)) in files.iter().enumerate() {
        assert!(file.starts_with(&format!("{:02}-", i + 1)) && file.ends_with(".dxf"), "{file}");
        assert!(file.contains(&name.replace(' ', "-")), "{file} for {name}");
        assert!(dir.join(file).exists());
    }
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 7);
    let again = scratch("each2");
    let r2 = run(&mut e, json!({"cmd": "export_cnc", "dir": again.to_string_lossy()}));
    assert_eq!(files_of(&r2), files);
    for (file, ..) in &files {
        assert_eq!(std::fs::read(dir.join(file)).unwrap(), std::fs::read(again.join(file)).unwrap(), "{file}: the same bytes twice");
    }
    // each file: millimeters, a closed OUTLINE the part's size
    let sizes: BTreeMap<&str, [f64; 2]> = [("Side left", [720.0, 560.0]), ("Bottom", [564.0, 560.0]), ("Shelf 1", [564.0, 520.0]), ("Back", [720.0, 564.0])].into_iter().collect();
    for (file, id, name) in &files {
        let d = parse(&std::fs::read_to_string(dir.join(file)).unwrap());
        assert_eq!(d.insunits, 4, "{file}");
        assert!(d.layers.contains(&"OUTLINE".to_string()), "{file}");
        assert!(d.closed("OUTLINE"), "{file}");
        let b = d.bbox("OUTLINE");
        near(b[0], 0.0, 0.01, "left");
        near(b[1], 0.0, 0.01, "bottom");
        if let Some(s) = sizes.get(name.as_str()) {
            near(b[2], s[0], 0.01, &format!("{name} length"));
            near(b[3], s[1], 0.01, &format!("{name} width"));
        }
        assert_eq!(d.ents.len(), 1, "a plain board is one closed outline: {file}");
        assert!(ids.values().any(|v| v == id));
    }
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(again);
}

#[test]
fn a_zip_holds_the_same_files() {
    let mut e = Engine::new();
    cabinet(&mut e);
    let dir = scratch("zip");
    let dirout = dir.join("files");
    let r = run(&mut e, json!({"cmd": "export_cnc", "dir": dirout.to_string_lossy()}));
    let z = dir.join("cnc.zip");
    let rz = run(&mut e, json!({"cmd": "export_cnc", "path": z.to_string_lossy()}));
    assert_eq!(files_of(&r), files_of(&rz));
    let mut a = zip::ZipArchive::new(std::fs::File::open(&z).unwrap()).unwrap();
    assert_eq!(a.len(), 7);
    for (i, (file, ..)) in files_of(&r).iter().enumerate() {
        let mut f = a.by_index(i).unwrap();
        assert_eq!(f.name().unwrap().as_ref() as &str, file.as_str());
        let mut bytes = vec![];
        std::io::Read::read_to_end(&mut f, &mut bytes).unwrap();
        assert_eq!(bytes, std::fs::read(dirout.join(file)).unwrap());
    }
    // the same bytes when asked again
    let z2 = dir.join("cnc2.zip");
    run(&mut e, json!({"cmd": "export_cnc", "path": z2.to_string_lossy()}));
    assert_eq!(std::fs::read(&z).unwrap(), std::fs::read(&z2).unwrap());
    // a .dxf is for one part only; a folder and a path together, or neither, are refused
    assert!(fails(&mut e, json!({"cmd": "export_cnc", "path": dir.join("a.dxf").to_string_lossy()})).contains("one .dxf per part"));
    assert!(fails(&mut e, json!({"cmd": "export_cnc", "path": dir.join("a.txt").to_string_lossy()})).contains(".zip"));
    assert!(fails(&mut e, json!({"cmd": "export_cnc"})).contains("path"));
    assert!(fails(&mut e, json!({"cmd": "export_cnc", "path": z.to_string_lossy(), "tool_diameter": 5.0})).contains("tool diameter"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_dado_becomes_a_pocket_layer_with_dogbones_only_when_asked() {
    let mut e = Engine::new();
    let ids = cabinet(&mut e);
    // Shelf 1 is set back 20 mm: its dado in the left side stops short of both edges
    let j = run(&mut e, json!({"cmd": "dado", "ids": [ids["Side left"], ids["Shelf 1"]]}));
    let side = j["cut"].as_u64().unwrap();
    let dir = scratch("dado");
    let find = |r: &Value, id: u64| files_of(r).into_iter().find(|f| f.1 == id).unwrap().0;
    let off = run(&mut e, json!({"cmd": "export_cnc", "dir": dir.join("off").to_string_lossy(), "dogbones": false}));
    let d = parse(&std::fs::read_to_string(dir.join("off").join(find(&off, side))).unwrap());
    assert!(d.layers.contains(&"POCKET_6.0mm".to_string()), "{:?}", d.layers);
    assert!(d.arcs().is_empty(), "no arcs without dogbones");
    let pocket = d.on("POCKET_6.0mm");
    assert_eq!(pocket.len(), 1);
    // 520 mm along the board's width, 18 mm across: the shelf's footprint, in the side's own frame
    let b = d.bbox("POCKET_6.0mm");
    let (w, h) = (b[2] - b[0], b[3] - b[1]);
    assert!((w - 520.0).abs() < 0.01 && (h - 18.0).abs() < 0.01 || (w - 18.0).abs() < 0.01 && (h - 520.0).abs() < 0.01, "{b:?}");
    // the outline is still the whole board
    let o = d.bbox("OUTLINE");
    near(o[2] - o[0], 720.0, 0.01, "length");
    near(o[3] - o[1], 560.0, 0.01, "width");
    assert_eq!(off["reliefs"], 0);

    let on = run(&mut e, json!({"cmd": "export_cnc", "dir": dir.join("on").to_string_lossy(), "dogbones": true, "tool_diameter": 0.006}));
    let d = parse(&std::fs::read_to_string(dir.join("on").join(find(&on, side))).unwrap());
    let arcs = d.arcs();
    assert_eq!(arcs.len(), 4, "one at each corner of the stopped dado");
    assert_eq!(on["reliefs"], 4);
    for a in &arcs {
        assert_eq!(a.layer, "POCKET_6.0mm");
        near(a.f(40), 3.0, 1e-6, "relief radius is half the tool");
        // centered on a corner of the pocket's box
        let (x, y) = (a.f(10), a.f(20));
        assert!([b[0], b[2]].iter().any(|v| (v - x).abs() < 0.01) && [b[1], b[3]].iter().any(|v| (v - y).abs() < 0.01), "{a:?} vs {b:?}");
    }
    assert!(d.closed("POCKET_6.0mm"), "lines and arcs join into one closed run");
    assert!(d.closed("OUTLINE"));
    // the other parts have no inside corners
    for (file, id, _) in files_of(&on) {
        if id != side {
            assert!(parse(&std::fs::read_to_string(dir.join("on").join(&file)).unwrap()).arcs().is_empty(), "{file}");
        }
    }
    // a through dado (a shelf as deep as the side) runs off the edge: no relief where the cutter leaves the board
    let bottom = run(&mut e, json!({"cmd": "dado", "ids": [ids["Side right"], ids["Bottom"]]}));
    let r = run(&mut e, json!({"cmd": "export_cnc", "dir": dir.join("thru").to_string_lossy(), "dogbones": true}));
    let d = parse(&std::fs::read_to_string(dir.join("thru").join(find(&r, bottom["cut"].as_u64().unwrap()))).unwrap());
    assert!(d.arcs().is_empty(), "an open-ended dado needs no relief");
    assert!(d.layers.iter().any(|l| l.starts_with("POCKET_")));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_hole_goes_on_the_inside_layer_and_gets_dogbones() {
    let mut e = Engine::new();
    let ids = cabinet(&mut e);
    let cutter = board(&mut e, "Cutter", [0.2, 0.2, H - 0.03], [0.25, 0.25, H + 0.01]);
    run(&mut e, json!({"cmd": "solid", "op": "subtract", "ids": [cutter, ids["Top"]]}));
    let dir = scratch("hole");
    let top = |r: &Value| files_of(r).into_iter().find(|f| f.2 == "Top").unwrap().0;
    let off = run(&mut e, json!({"cmd": "export_cnc", "dir": dir.join("a").to_string_lossy()}));
    let d = parse(&std::fs::read_to_string(dir.join("a").join(top(&off))).unwrap());
    assert_eq!(d.on("INSIDE").len(), 1);
    let b = d.bbox("INSIDE");
    near(b[2] - b[0], 50.0, 0.01, "hole width");
    near(b[3] - b[1], 50.0, 0.01, "hole height");
    assert!(d.on("OUTLINE").len() == 1 && d.arcs().is_empty());
    let on = run(&mut e, json!({"cmd": "export_cnc", "dir": dir.join("b").to_string_lossy(), "dogbones": true, "tool_diameter": 0.008}));
    let d = parse(&std::fs::read_to_string(dir.join("b").join(top(&on))).unwrap());
    assert_eq!(d.arcs().len(), 4);
    assert!(d.arcs().iter().all(|a| a.layer == "INSIDE" && (a.f(40) - 4.0).abs() < 1e-6));
    assert!(d.closed("INSIDE"));
    // a square-cornered outline needs none
    assert!(d.on("OUTLINE").iter().all(|e| e.kind == "LWPOLYLINE"));
    let _ = std::fs::remove_dir_all(dir);
}

// ----- joinery -------------------------------------------------------------------------------------------------------

fn solid(e: &mut Engine, id: u64) -> f64 {
    let v = run(e, json!({"cmd": "volume", "id": id}));
    assert_eq!(v["solid"], true, "{v}");
    v["volume"].as_f64().unwrap()
}

fn is_solid(e: &mut Engine, id: u64) -> bool {
    run(e, json!({"cmd": "check_solid", "ids": [id]}))["solid"] == true
}

fn sizes(e: &mut Engine) -> BTreeMap<String, Vec<f64>> {
    run(e, json!({"cmd": "parts"}))["parts"].as_array().unwrap().iter().map(|p| (p["name"].as_str().unwrap().to_string(), p["size"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect())).collect()
}

fn group_count(e: &mut Engine) -> usize {
    run(e, json!({"cmd": "parts"}))["parts"].as_array().unwrap().len()
}

#[test]
fn a_dado_is_cut_where_the_shelf_stands_and_undoes_in_one_step() {
    let mut e = Engine::new();
    let ids = cabinet(&mut e);
    let before = solid(&mut e, ids["Side left"]);
    let sizes_before = sizes(&mut e);
    let j = run(&mut e, json!({"cmd": "dado", "ids": [ids["Side left"], ids["Shelf 1"]], "depth": 0.008, "clearance": 0.0004}));
    let side = j["cut"].as_u64().unwrap();
    // shelf footprint 520 x 18 mm, 0.4 mm of clearance across, 8 mm deep
    let want = 0.52 * (T + 0.0004) * 0.008;
    near(j["removed_volume"].as_f64().unwrap(), want, want * 1e-3, "removed volume");
    near(before - solid(&mut e, side), want, want * 1e-3, "volume after");
    assert!(is_solid(&mut e, side), "still a solid");
    assert_eq!(j["joint"], "dado");
    // the cut list and plan see the same boards
    assert_eq!(sizes(&mut e), sizes_before, "the board's bounding size is unchanged");
    assert_eq!(group_count(&mut e), 7);
    let plan = run(&mut e, json!({"cmd": "cut_plan"}));
    assert_eq!(plan["summary"]["parts"], 7);
    assert_eq!(plan["summary"]["unplaced"], 0);
    // one undo step
    run(&mut e, json!({"cmd": "undo"}));
    near(solid(&mut e, ids["Side left"]), before, 1e-12, "restored");
    assert_eq!(sizes(&mut e), sizes_before);
    run(&mut e, json!({"cmd": "redo"}));
    assert_eq!(run(&mut e, json!({"cmd": "parts"}))["parts"].as_array().unwrap().len(), 7);
}

#[test]
fn a_dado_keeps_the_boards_name_and_other_joints_refuse_what_does_not_fit() {
    let mut e = Engine::new();
    let ids = cabinet(&mut e);
    // too deep (through), a bad number, the same board twice, the wrong count
    assert!(fails(&mut e, json!({"cmd": "dado", "ids": [ids["Side left"], ids["Shelf 1"]], "depth": 0.018})).contains("through"));
    assert!(fails(&mut e, json!({"cmd": "dado", "ids": [ids["Side left"], ids["Shelf 1"]], "depth": -1.0})).contains("depth"));
    assert!(fails(&mut e, json!({"cmd": "dado", "ids": [ids["Side left"], ids["Side left"]]})).contains("different"));
    assert!(fails(&mut e, json!({"cmd": "dado", "ids": [ids["Side left"]]})).contains("two boards"));
    // the shelf does not touch the right side's outer face, nor a board that floats
    assert!(fails(&mut e, json!({"cmd": "dado", "ids": [ids["Side left"], ids["Side right"]]})).contains("don't touch"));
    let far = board(&mut e, "Floater", [0.1, 0.1, 0.9], [0.4, 0.4, 0.918]);
    for cmd in ["dado", "rabbet", "tenon"] {
        let msg = fails(&mut e, json!({"cmd": cmd, "ids": [ids["Side left"], far]}));
        assert!(msg.contains("don't touch"), "{cmd}: {msg}");
    }
    // a board lying flat on another is no dado
    let flat = board(&mut e, "Flat", [0.1, 0.1, H], [0.3, 0.3, H + 0.018]);
    assert!(fails(&mut e, json!({"cmd": "dado", "ids": [ids["Top"], flat]})).contains("lies flat"));
    // nothing changed by all those refusals
    assert_eq!(group_count(&mut e), 9);
    assert!(is_solid(&mut e, ids["Side left"]));
}

#[test]
fn a_rabbet_takes_the_edge_where_the_back_sits() {
    let mut e = Engine::new();
    let ids = cabinet(&mut e);
    // the back is 6 mm thick and stands against the side's inner face at its rear edge
    let before = solid(&mut e, ids["Side left"]);
    let sizes_before = sizes(&mut e);
    let j = run(&mut e, json!({"cmd": "rabbet", "ids": [ids["Side left"], ids["Back"]], "depth": 0.009}));
    let side = j["cut"].as_u64().unwrap();
    let want = H * 0.006 * 0.009;
    near(j["removed_volume"].as_f64().unwrap(), want, want * 1e-3, "removed volume");
    near(before - solid(&mut e, side), want, want * 1e-3, "volume after");
    assert!(is_solid(&mut e, side));
    assert_eq!(sizes(&mut e), sizes_before);
    // an L-shaped section now: the face on the inside is cut back 6 mm from the rear edge
    let dir = scratch("rabbet");
    let r = run(&mut e, json!({"cmd": "export_cnc", "dir": dir.to_string_lossy(), "dogbones": true}));
    let f = files_of(&r).into_iter().find(|f| f.1 == side).unwrap().0;
    let d = parse(&std::fs::read_to_string(dir.join(f)).unwrap());
    assert!(d.layers.iter().any(|l| l.starts_with("POCKET_9.0mm")), "{:?}", d.layers);
    run(&mut e, json!({"cmd": "undo"}));
    near(solid(&mut e, ids["Side left"]), before, 1e-12, "undone");
    // an explicit width from the edge
    let j = run(&mut e, json!({"cmd": "rabbet", "ids": [ids["Side left"], ids["Back"]], "depth": 0.009, "width": 0.01}));
    near(j["removed_volume"].as_f64().unwrap(), H * 0.01 * 0.009, 1e-8, "wider");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_tenon_and_its_mortise_fit_and_the_board_grows_by_the_tenon() {
    let mut e = Engine::new();
    // a rail 60 x 18 mm, 0.4 m long, stands with its end on the face of a 40 mm post
    let post = board(&mut e, "Post", [0.0, 0.0, 0.0], [0.1, 0.1, 0.04]);
    let rail = board(&mut e, "Rail", [0.1, 0.02, 0.011], [0.5, 0.08, 0.029]);
    let (pv, rv) = (solid(&mut e, post), solid(&mut e, rail));
    let before = sizes(&mut e);
    let j = run(&mut e, json!({"cmd": "tenon", "ids": [post, rail], "length": 0.025, "thickness": 0.006, "shoulder": 0.01, "clearance": 0.0}));
    let (post2, rail2) = (j["cut"].as_u64().unwrap(), j["fit"].as_u64().unwrap());
    // the tenon is 6 x 40 x 25 mm: the post lost it, the rail gained it
    let want = 0.006 * 0.04 * 0.025;
    near(pv - solid(&mut e, post2), want, want * 1e-3, "mortise volume");
    near(solid(&mut e, rail2) - rv, want, want * 1e-3, "tenon volume");
    assert!(is_solid(&mut e, post2) && is_solid(&mut e, rail2));
    // the cut list: the rail is 25 mm longer, the post is the same
    let after = sizes(&mut e);
    near(after["Rail"][0], before["Rail"][0] + 0.025, 1e-9, "rail length");
    assert_eq!(after["Post"], before["Post"]);
    // a clearance makes the mortise a little bigger than the tenon
    let mut e2 = Engine::new();
    let post = board(&mut e2, "Post", [0.0, 0.0, 0.0], [0.1, 0.1, 0.04]);
    let rail = board(&mut e2, "Rail", [0.1, 0.02, 0.011], [0.5, 0.08, 0.029]);
    let pv = solid(&mut e2, post);
    let j = run(&mut e2, json!({"cmd": "tenon", "ids": [post, rail], "length": 0.025, "thickness": 0.006, "shoulder": 0.01, "clearance": 0.0005}));
    let loose = pv - solid(&mut e2, j["cut"].as_u64().unwrap());
    assert!(loose > want * 1.1, "{loose} vs {want}");
    // one undo step takes back both boards
    run(&mut e, json!({"cmd": "undo"}));
    assert_eq!(sizes(&mut e), before);
    near(solid(&mut e, post), pv, 1e-12, "post restored");
    near(solid(&mut e, rail), rv, 1e-12, "rail restored");
    // refuses a tenon too long for the post, or one that would break out of its face
    assert!(fails(&mut e, json!({"cmd": "tenon", "ids": [post, rail], "length": 0.1})).contains("too long"));
    assert!(fails(&mut e, json!({"cmd": "tenon", "ids": [post, rail], "thickness": 0.018})).contains("thickness"));
    // a board that lies on the rail's face instead of meeting it with its end is no tenon
    let plank = board(&mut e, "Plank", [0.2, 0.02, 0.029], [0.3, 0.08, 0.047]);
    let msg = fails(&mut e, json!({"cmd": "tenon", "ids": [rail, plank]}));
    assert!(msg.contains("with its end"), "{msg}");
}

#[test]
fn joinery_and_cnc_keep_to_what_agents_may_do() {
    let mut e = Engine::new();
    let ids = cabinet(&mut e);
    let dir = scratch("perm");
    let agent = Caller::Mcp(Workspace::at(&dir));
    let ask = |e: &mut Engine, who: &Caller, v: Value| e.exec_json_as(who, &v.to_string());
    // plugins write no files; they may cut joints
    assert_eq!(ask(&mut e, &Caller::Plugin, json!({"cmd": "export_cnc", "path": dir.join("a.zip")}))["ok"], false);
    assert_eq!(ask(&mut e, &Caller::Plugin, json!({"cmd": "dado", "ids": [ids["Side left"], ids["Shelf 1"]]}))["ok"], true);
    run(&mut e, json!({"cmd": "undo"}));
    // an agent writes a .zip in its workspace, never elsewhere, and never a folder
    let ok = ask(&mut e, &agent, json!({"cmd": "export_cnc", "path": "parts.zip"}));
    assert_eq!(ok["ok"], true, "{ok}");
    assert!(dir.join("parts.zip").exists());
    assert_eq!(ask(&mut e, &agent, json!({"cmd": "export_cnc", "path": "/tmp/outside.zip"}))["ok"], false);
    assert_eq!(ask(&mut e, &agent, json!({"cmd": "export_cnc", "path": "parts2.txt"}))["ok"], false);
    assert_eq!(ask(&mut e, &agent, json!({"cmd": "export_cnc", "dir": "cncfolder"}))["ok"], false);
    assert!(!dir.join("cncfolder").exists());
    // it does not replace a file it did not make
    std::fs::write(dir.join("mine.zip"), b"x").unwrap();
    assert_eq!(ask(&mut e, &agent, json!({"cmd": "export_cnc", "path": "mine.zip"}))["ok"], false);
    assert_eq!(std::fs::read(dir.join("mine.zip")).unwrap(), b"x");
    // an agent may cut joints
    assert_eq!(ask(&mut e, &agent, json!({"cmd": "rabbet", "ids": [ids["Side left"], ids["Back"]]}))["ok"], true);
    let _ = std::fs::remove_dir_all(dir);
}
