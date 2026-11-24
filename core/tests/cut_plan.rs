//! The woodworker pack, part one (docs/dev/woodworker.md): stock and prices, the cut plan, cutting diagrams, labels.
//! The fixture is a small base cabinet: a carcass of 18 mm sheet, shelves, a 6 mm back and a face frame of 1×2 lumber.
use serde_json::{json, Value};
use sketchcraft::policy::{Caller, Workspace};
use sketchcraft::Engine;

#[path = "pdftext/mod.rs"]
mod pdftext;

fn run(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

/// A box from `lo` to `hi` as its own group, on a material.
fn board(e: &mut Engine, name: &str, lo: [f64; 3], hi: [f64; 3], material: Option<&str>) -> u64 {
    let g = run(e, json!({"cmd": "begin_group", "name": name}))["group"].as_u64().unwrap();
    let f = run(e, json!({"cmd": "rect", "from": [lo[0], lo[1], lo[2]], "to": [hi[0], hi[1], lo[2]]}))["face"].as_u64().unwrap();
    run(e, json!({"cmd": "pushpull", "face": f, "distance": hi[2] - lo[2], "toward": [0, 0, 1]}));
    run(e, json!({"cmd": "end_group"}));
    if let Some(m) = material {
        run(e, json!({"cmd": "material", "name": m, "color": [0.8, 0.7, 0.5]}));
        run(e, json!({"cmd": "paint", "ids": [g], "material": m}));
    }
    g
}

const T: f64 = 0.018;
const W: f64 = 0.6;
const D: f64 = 0.56;
const H: f64 = 0.72;
const IN: f64 = 0.0254;

/// 2 sides, top and bottom stretchers... the carcass (18 mm "Birch ply"), 2 shelves, a 6 mm "Hardboard" back, and a
/// face frame of 1×2 "Maple": 2 stiles and 3 rails.
fn cabinet(e: &mut Engine) {
    cabinet_in_order(e, false);
}

fn cabinet_in_order(e: &mut Engine, reverse: bool) {
    let ply = Some("Birch ply");
    let (t, w) = (0.75 * IN, 1.5 * IN);
    type Spec<'a> = (&'a str, [f64; 3], [f64; 3], Option<&'a str>);
    let mut specs: Vec<Spec> = vec![
        ("Side left", [0.0, 0.0, 0.0], [T, D, H], ply),
        ("Side right", [W - T, 0.0, 0.0], [W, D, H], ply),
        ("Bottom", [T, 0.0, 0.1], [W - T, D, 0.1 + T], ply),
        ("Top", [T, 0.0, H - T], [W - T, D, H], ply),
        ("Shelf 1", [T, 0.02, 0.3], [W - T, D - 0.02, 0.3 + T], ply),
        ("Shelf 2", [T, 0.02, 0.5], [W - T, D - 0.02, 0.5 + T], ply),
        ("Back", [T, D, 0.0], [W - T, D + 0.006, H], Some("Hardboard")),
        ("Stile left", [0.0, -t, 0.0], [w, 0.0, H], Some("Maple")),
        ("Stile right", [W - w, -t, 0.0], [W, 0.0, H], Some("Maple")),
        ("Rail top", [w, -t, H - w], [W - w, 0.0, H], Some("Maple")),
        ("Rail mid", [w, -t, 0.4], [W - w, 0.0, 0.4 + w], Some("Maple")),
        ("Rail bottom", [w, -t, 0.0], [W - w, 0.0, w], Some("Maple")),
    ];
    if reverse {
        specs.reverse();
    }
    for (name, lo, hi, m) in specs {
        board(e, name, lo, hi, m);
    }
}

fn prices(e: &mut Engine) {
    run(
        e,
        json!({"cmd": "cut_stock", "kerf": 0.003, "materials": {
            "Birch ply": {"sheet_price": 60.0},
            "Hardboard": {"sheet_price": 18.5},
            "Maple": {"board_foot_price": 9.0, "grain": true},
        }}),
    );
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

#[test]
fn a_cabinet_gets_a_valid_cut_plan() {
    let mut e = Engine::new();
    cabinet(&mut e);
    prices(&mut e);
    let plan = run(&mut e, json!({"cmd": "cut_plan"}));
    assert_eq!(plan["summary"]["parts"], 12);
    assert_eq!(plan["summary"]["unplaced"], 0);
    // 6 ply parts on one sheet, the back on its own hardboard sheet, 5 maple parts on boards
    assert_eq!(plan["sheets"].as_array().unwrap().len(), 2, "{plan}");
    let kerf = num(&plan["kerf"]);
    // every part is placed once, inside its sheet, and no two overlap (nor their kerf)
    let mut seen = std::collections::BTreeSet::new();
    for s in plan["sheets"].as_array().unwrap() {
        let [sl, sw] = [num(&s["size"][0]), num(&s["size"][1])];
        let ps = s["parts"].as_array().unwrap();
        for p in ps {
            assert!(seen.insert(p["number"].as_u64().unwrap()), "placed twice: {p}");
            let (x, y, w, h) = (num(&p["x"]), num(&p["y"]), num(&p["w"]), num(&p["h"]));
            assert!(x >= -1e-9 && y >= -1e-9 && x + w <= sl + 1e-9 && y + h <= sw + 1e-9, "outside the sheet: {p}");
        }
        for (i, a) in ps.iter().enumerate() {
            for b in &ps[i + 1..] {
                let (ax, ay, aw, ah) = (num(&a["x"]), num(&a["y"]), num(&a["w"]), num(&a["h"]));
                let (bx, by, bw, bh) = (num(&b["x"]), num(&b["y"]), num(&b["w"]), num(&b["h"]));
                // apart by at least a kerf along one axis
                let apart = ax + aw + kerf <= bx + 1e-6 || bx + bw + kerf <= ax + 1e-6 || ay + ah + kerf <= by + 1e-6 || by + bh + kerf <= ay + 1e-6;
                assert!(apart, "overlap (or less than a kerf apart): {a} {b}");
            }
        }
    }
    // the part list says where each one went
    for p in plan["parts"].as_array().unwrap() {
        assert!(!p["on"].is_null(), "{p}");
    }
    assert_eq!(seen.len() + plan["boards"].as_array().unwrap().iter().map(|b| b["parts"].as_array().unwrap().len()).sum::<usize>(), 12);
}

#[test]
fn boards_are_packed_end_to_end_with_the_kerf() {
    let mut e = Engine::new();
    cabinet(&mut e);
    prices(&mut e);
    let plan = run(&mut e, json!({"cmd": "cut_plan"}));
    let boards = plan["boards"].as_array().unwrap();
    assert!(!boards.is_empty());
    for b in boards {
        assert_eq!(b["stock"], "1×2", "{b}");
        let len = num(&b["length"]);
        let ps = b["parts"].as_array().unwrap();
        let mut end = 0.0;
        for p in ps {
            let (x, l) = (num(&p["x"]), num(&p["length"]));
            assert!(x + 1e-6 >= end, "overlaps the part before it: {b}");
            end = x + l + 0.003;
            assert!(x + l <= len + 1e-9, "past the board's end: {b}");
        }
    }
    // the five maple parts: 2 stiles of 0.72 and 3 rails of 0.5236: 2×0.723 + 3×0.5266 > 2.44? no: one 8 ft board takes them
    // (1.446 + 1.58 = 3.03 m of parts), so two boards
    assert_eq!(boards.len(), 2, "{plan}");
}

#[test]
fn the_kerf_is_respected_when_it_decides_the_count() {
    // three 800 mm parts and two cuts between them: 2.406 m fits a 2.44 m board with a 3 mm kerf, 2.46 m does not with 30 mm
    let mut e = Engine::new();
    for i in 0..3 {
        board(&mut e, &format!("Slat {i}"), [0.0, i as f64 * 0.1, 0.0], [0.8, i as f64 * 0.1 + 0.05, 0.02], None);
    }
    run(&mut e, json!({"cmd": "cut_stock", "kerf": 0.003}));
    assert_eq!(run(&mut e, json!({"cmd": "cut_plan"}))["summary"]["boards"], 1);
    assert_eq!(run(&mut e, json!({"cmd": "cut_plan", "kerf": 0.03}))["summary"]["boards"], 2);
    // trying a kerf does not save it
    assert_eq!(run(&mut e, json!({"cmd": "cut_plan"}))["kerf"], 0.003);
}

#[test]
fn grain_decides_whether_a_part_may_turn() {
    // a 1.0 × 0.5 m panel and a sheet 1.2 × 0.9 m: it fits only length-along-x; two panels need turning to share a sheet
    let mut e = Engine::new();
    for i in 0..2 {
        board(&mut e, &format!("Panel {i}"), [0.0, i as f64 * 2.0, 0.0], [1.0, i as f64 * 2.0 + 0.5, 0.018], Some("Oak ply"));
    }
    run(&mut e, json!({"cmd": "cut_stock", "kerf": 0.0, "materials": {"Oak ply": {"sheet": [1.2, 1.1], "grain": false}}}));
    // no grain: the second panel may turn, but 1.0 + 0.5 along y = 1.0 wide... they stack (0.5 + 0.5 = 1.0 <= 1.1)
    let free = run(&mut e, json!({"cmd": "cut_plan"}));
    assert_eq!(free["sheets"].as_array().unwrap().len(), 1);
    // a sheet 0.6 wide takes a part only length-first: then a 0.5 part fits one per sheet either way
    run(&mut e, json!({"cmd": "cut_stock", "materials": {"Oak ply": {"sheet": [1.2, 0.6]}}}));
    let grained_free = run(&mut e, json!({"cmd": "cut_plan"}));
    assert_eq!(grained_free["sheets"].as_array().unwrap().len(), 2);
    // the turned part: a panel 0.5 long × 0.3 wide on a 0.4 × 1.0 sheet fits only when turned, and only without grain
    let mut e = Engine::new();
    board(&mut e, "Panel", [0.0, 0.0, 0.0], [0.9, 0.35, 0.018], Some("Oak ply"));
    run(&mut e, json!({"cmd": "cut_stock", "kerf": 0.0, "materials": {"Oak ply": {"sheet": [0.4, 1.0], "grain": false}}}));
    let turned = run(&mut e, json!({"cmd": "cut_plan"}));
    assert_eq!(turned["summary"]["unplaced"], 0);
    let p = &turned["sheets"][0]["parts"][0];
    assert_eq!(p["rotated"], true, "{p}");
    run(&mut e, json!({"cmd": "cut_stock", "materials": {"Oak ply": {"grain": true}}}));
    let held = run(&mut e, json!({"cmd": "cut_plan"}));
    assert_eq!(held["summary"]["unplaced"], 1, "{held}");
    assert_eq!(held["unplaced"][0]["reason"], "bigger than a sheet");
    // and with grain a placed part is never turned
    run(&mut e, json!({"cmd": "cut_stock", "materials": {"Oak ply": {"sheet": [1.2, 0.6]}}}));
    assert_eq!(run(&mut e, json!({"cmd": "cut_plan"}))["sheets"][0]["parts"][0]["rotated"], false);
}

#[test]
fn the_plan_is_deterministic_and_does_not_depend_on_storage_order() {
    // the same cabinet made in the opposite order (so every id differs): the same placements, by name
    let build = |reverse: bool| {
        let mut e = Engine::new();
        cabinet_in_order(&mut e, reverse);
        prices(&mut e);
        let mut plan = run(&mut e, json!({"cmd": "cut_plan"}));
        // ids differ between the two models; what a person sees (numbers, names, places) must not
        let strip = |v: &mut Value| {
            fn go(v: &mut Value) {
                match v {
                    Value::Object(m) => {
                        m.remove("id");
                        m.values_mut().for_each(go);
                    }
                    Value::Array(a) => a.iter_mut().for_each(go),
                    _ => {}
                }
            }
            go(v)
        };
        strip(&mut plan);
        plan
    };
    let (a, b) = (build(false), build(true));
    assert_eq!(a.to_string(), b.to_string());
    // and asking twice gives the same answer
    let mut e = Engine::new();
    cabinet(&mut e);
    let once = run(&mut e, json!({"cmd": "cut_plan"}));
    assert_eq!(once.to_string(), run(&mut e, json!({"cmd": "cut_plan"})).to_string());
}

#[test]
fn the_cost_is_the_stock_bought_at_its_price() {
    let mut e = Engine::new();
    cabinet(&mut e);
    prices(&mut e);
    let plan = run(&mut e, json!({"cmd": "cut_plan"}));
    let rows = plan["cost"]["by_material"].as_array().unwrap();
    let row = |m: &str| rows.iter().find(|r| r["material"] == m).unwrap_or_else(|| panic!("no {m}: {plan}")).clone();
    assert_eq!(row("Birch ply")["sheets"], 1);
    assert_eq!(num(&row("Birch ply")["cost"]), 60.0);
    assert_eq!(num(&row("Hardboard")["cost"]), 18.5);
    // maple: two 8 ft boards of 1×2 by the nominal size: 1 × 2 × 96 / 144 = 1.333 bd ft each, at $9
    let maple = row("Maple");
    assert_eq!(maple["boards"], 2);
    assert!((num(&maple["board_feet"]) - 2.67).abs() < 0.011, "{maple}");
    assert!((num(&maple["cost"]) - 24.0).abs() < 0.2, "{maple}");
    let total: f64 = rows.iter().map(|r| num(&r["cost"])).sum();
    assert!((num(&plan["cost"]["total"]) - total).abs() < 0.011);
    assert!((num(&plan["cost"]["total"]) - (60.0 + 18.5 + num(&maple["cost"]))).abs() < 0.02);
    // a price per board instead
    run(&mut e, json!({"cmd": "cut_stock", "materials": {"Maple": {"board_foot_price": 0.0, "board_price": 7.25}}}));
    let plan = run(&mut e, json!({"cmd": "cut_plan"}));
    let maple = plan["cost"]["by_material"].as_array().unwrap().iter().find(|r| r["material"] == "Maple").unwrap().clone();
    assert_eq!(num(&maple["cost"]), 14.5);
    // a material with no price says so
    run(&mut e, json!({"cmd": "cut_stock", "materials": {"Hardboard": null}}));
    let plan = run(&mut e, json!({"cmd": "cut_plan"}));
    let hb = plan["cost"]["by_material"].as_array().unwrap().iter().find(|r| r["material"] == "Hardboard").unwrap().clone();
    assert_eq!(hb["priced"], false);
    assert_eq!(num(&hb["cost"]), 0.0);
}

#[test]
fn stock_is_saved_with_the_model_and_one_undo_takes_it_back() {
    let mut e = Engine::new();
    cabinet(&mut e);
    prices(&mut e);
    let dir = std::env::temp_dir().join(format!("sc-cut-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cabinet.sketchcraft");
    run(&mut e, json!({"cmd": "save", "path": path}));
    let mut f = Engine::new();
    run(&mut f, json!({"cmd": "open", "path": path}));
    assert_eq!(f.model.cut_stock.materials["Birch ply"].sheet_price, 60.0);
    assert_eq!(run(&mut f, json!({"cmd": "cut_plan"}))["cost"]["total"], run(&mut e, json!({"cmd": "cut_plan"}))["cost"]["total"]);
    // one undo step
    run(&mut e, json!({"cmd": "cut_stock", "currency": "€"}));
    assert_eq!(e.model.cut_stock.currency, "€");
    run(&mut e, json!({"cmd": "undo"}));
    assert_eq!(e.model.cut_stock.currency, "$");
    assert!(e.model.cut_stock.materials.contains_key("Birch ply"));
    // silly numbers are refused
    for bad in [json!({"kerf": -1}), json!({"kerf": 1.0}), json!({"sheet": [0.0, 1.0]}), json!({"materials": {"X": {"sheet_price": -5}}}), json!({"materials": {"X": {"colour": 1}}}), json!({"board_lengths": []})] {
        let mut v = bad.clone();
        v["cmd"] = json!("cut_stock");
        let r = e.exec_json(&v.to_string());
        assert_eq!(r["ok"], false, "{bad} -> {r}");
    }
}

#[test]
fn diagrams_are_a_pdf_with_a_page_per_sheet_and_an_svg_with_every_part() {
    let mut e = Engine::new();
    cabinet(&mut e);
    prices(&mut e);
    let dir = std::env::temp_dir().join(format!("sc-cut-d-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let pdf = dir.join("cut.pdf");
    let r = run(&mut e, json!({"cmd": "export_cut_diagrams", "path": pdf, "paper": "a4"}));
    assert_eq!((r["sheets"].as_u64(), r["boards"].as_u64()), (Some(2), Some(2)));
    let doc = pdftext::parse(&std::fs::read(&pdf).unwrap());
    // a summary, two sheets and one page of boards
    assert_eq!(doc.pages().len(), 4);
    let first = doc.text(doc.pages()[0]).join(" ");
    assert!(first.contains("Cutting plan") && first.contains("Birch ply") && first.contains("$"), "{first}");
    let sheet1 = doc.text(doc.pages()[1]).join(" ");
    assert!(sheet1.contains("Sheet 1 of 2") && sheet1.contains("Side left"), "{sheet1}");
    let svg = dir.join("cut.svg");
    run(&mut e, json!({"cmd": "export_cut_diagrams", "path": svg}));
    let s = std::fs::read_to_string(&svg).unwrap();
    assert!(s.starts_with("<svg") && s.contains("Side left") && s.contains("Rail mid"), "{}", &s[..200.min(s.len())]);
    // another extension is refused
    let r = e.exec_json(&json!({"cmd": "export_cut_diagrams", "path": dir.join("cut.txt")}).to_string());
    assert_eq!(r["ok"], false);
}

#[test]
fn labels_carry_the_number_name_size_material_and_a_grain_arrow() {
    let mut e = Engine::new();
    cabinet(&mut e);
    prices(&mut e);
    let plan = run(&mut e, json!({"cmd": "cut_plan"}));
    // the part list is what the labels print
    let parts = plan["parts"].as_array().unwrap();
    let side = parts.iter().find(|p| p["name"] == "Side left").unwrap();
    let rail = parts.iter().find(|p| p["name"] == "Rail mid").unwrap();
    assert_eq!(side["material"], "Birch ply");
    assert_eq!(side["kind"], "sheet");
    assert_eq!(rail["stock"], "1×2");
    assert_eq!(rail["grain"], true);
    // numbers run 1..12 in the order of parts_layout (longest first)
    let nums: Vec<u64> = parts.iter().map(|p| p["number"].as_u64().unwrap()).collect();
    assert_eq!(nums, (1..=12).collect::<Vec<_>>());
    let dir = std::env::temp_dir().join(format!("sc-cut-l-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("labels.pdf");
    let r = run(&mut e, json!({"cmd": "export_cut_labels", "path": out, "paper": "a4"}));
    assert_eq!(r["labels"], 12);
    let doc = pdftext::parse(&std::fs::read(&out).unwrap());
    assert_eq!(doc.pages().len(), 1);
    assert!((doc.media_box(doc.pages()[0])[2] - 210.0 * 72.0 / 25.4).abs() < 0.5);
    let text = doc.text(doc.pages()[0]).join("\n");
    for want in ["Side left", "Rail mid", "Birch ply", "Maple", "Hardboard", "Sheet 1 of 2", "Board 1 of 2", &format!("#{}", side["number"]), "720 mm", "18 mm"] {
        assert!(text.contains(want), "label text has no {want:?}:\n{text}");
    }
    // sheet goods without a grain say so, lumber and grained materials get an arrow
    assert!(text.contains("no grain direction"));
    assert!(text.contains("grain"));
}

#[test]
fn imperial_models_label_in_inches_and_name_lumber() {
    let mut e = Engine::new();
    cabinet(&mut e);
    run(&mut e, json!({"cmd": "units", "units": "in"}));
    let plan = run(&mut e, json!({"cmd": "cut_plan"}));
    let _ = plan;
    let dir = std::env::temp_dir().join(format!("sc-cut-i-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("labels.pdf");
    run(&mut e, json!({"cmd": "export_cut_labels", "path": out}));
    let doc = pdftext::parse(&std::fs::read(&out).unwrap());
    // letter paper by default for inches, and sizes in inches: 18 mm is 23/32", read to 1/16 as 11/16"
    assert!((doc.media_box(doc.pages()[0])[2] - 215.9 * 72.0 / 25.4).abs() < 0.5);
    let text = doc.text(doc.pages()[0]).join("\n");
    assert!(text.contains('"') && !text.contains(" mm"), "{text}");
}

#[test]
fn nothing_to_plan_and_oversize_parts_are_reported_not_hidden() {
    let mut e = Engine::new();
    let empty = run(&mut e, json!({"cmd": "cut_plan"}));
    assert_eq!(empty["summary"]["parts"], 0);
    let dir = std::env::temp_dir().join(format!("sc-cut-e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let r = e.exec_json(&json!({"cmd": "export_cut_labels", "path": dir.join("l.pdf")}).to_string());
    assert_eq!(r["ok"], false);
    // a 3 m panel on 2.44 m sheets, and a 3 m board
    board(&mut e, "Long panel", [0.0, 0.0, 0.0], [3.0, 0.5, 0.018], None);
    board(&mut e, "Long beam", [0.0, 1.0, 0.0], [3.0, 1.0 + 0.0381, 0.0381], None);
    let plan = run(&mut e, json!({"cmd": "cut_plan"}));
    assert_eq!(plan["summary"]["unplaced"], 2, "{plan}");
    assert_eq!(plan["sheets"].as_array().unwrap().len(), 0);
    // longer boards sold fix it
    run(&mut e, json!({"cmd": "cut_stock", "lengths_by_size": {"2×2": [3.05]}, "board_lengths": [2.44, 3.05]}));
    let plan = run(&mut e, json!({"cmd": "cut_plan"}));
    assert_eq!(plan["summary"]["unplaced"], 1, "{plan}");
    assert_eq!(plan["boards"][0]["length"], 3.05);
}

#[test]
fn agents_and_plugins_may_plan_and_agents_write_only_pdfs_and_svgs_in_their_folder() {
    let mut e = Engine::new();
    cabinet(&mut e);
    let dir = std::env::temp_dir().join(format!("sc-cut-p-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let agent = Caller::Mcp(Workspace::at(&dir));
    let ask = |e: &mut Engine, who: &Caller, v: Value| e.exec_json_as(who, &v.to_string());
    for who in [&Caller::Plugin, &agent] {
        assert_eq!(ask(&mut e, who, json!({"cmd": "cut_plan"}))["ok"], true);
        assert_eq!(ask(&mut e, who, json!({"cmd": "cut_stock", "kerf": 0.004}))["ok"], true);
    }
    // a plugin cannot write files
    assert_eq!(ask(&mut e, &Caller::Plugin, json!({"cmd": "export_cut_labels", "path": dir.join("a.pdf")}))["ok"], false);
    assert_eq!(ask(&mut e, &agent, json!({"cmd": "export_cut_labels", "path": dir.join("a.pdf")}))["ok"], true);
    assert_eq!(ask(&mut e, &agent, json!({"cmd": "export_cut_diagrams", "path": dir.join("a.svg")}))["ok"], true);
    // not an extension an agent may write, and not outside its folder
    assert_eq!(ask(&mut e, &agent, json!({"cmd": "export_cut_diagrams", "path": dir.join("run.sh")}))["ok"], false);
    assert_eq!(ask(&mut e, &agent, json!({"cmd": "export_cut_labels", "path": std::env::temp_dir().join("outside.pdf")}))["ok"], false);
}

#[test]
fn many_odd_parts_never_overlap_or_leave_the_stock() {
    // 40 plywood parts of pseudo-random sizes (the same every run), with and without grain, a kerf and a trim
    let mut seed = 12345u64;
    let mut next = |lo: f64, hi: f64| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        lo + ((seed >> 33) as f64 / (1u64 << 31) as f64) * (hi - lo)
    };
    let mut e = Engine::new();
    for i in 0..40 {
        let (l, w) = (next(0.15, 1.3), next(0.12, 0.7));
        let (l, w) = ((l * 1000.0).round() / 1000.0, (w * 1000.0).round() / 1000.0);
        board(&mut e, &format!("P{i:02}"), [0.0, i as f64 * 2.0, 0.0], [l, i as f64 * 2.0 + w, 0.018], Some("Ply"));
    }
    for grain in [false, true] {
        run(&mut e, json!({"cmd": "cut_stock", "kerf": 0.004, "trim": 0.01, "materials": {"Ply": {"grain": grain}}}));
        let plan = run(&mut e, json!({"cmd": "cut_plan"}));
        assert_eq!(plan["summary"]["unplaced"], 0, "{plan}");
        let mut placed = 0;
        for s in plan["sheets"].as_array().unwrap() {
            let ps = s["parts"].as_array().unwrap();
            placed += ps.len();
            for a in ps {
                let (x, y, w, h) = (num(&a["x"]), num(&a["y"]), num(&a["w"]), num(&a["h"]));
                assert!(x >= 0.01 - 1e-9 && y >= 0.01 - 1e-9 && x + w <= 2.44 - 0.01 + 1e-9 && y + h <= 1.22 - 0.01 + 1e-9, "outside the trimmed sheet: {a}");
                if grain {
                    assert_eq!(a["rotated"], false, "{a}");
                }
            }
            for (i, a) in ps.iter().enumerate() {
                for b in &ps[i + 1..] {
                    let (ax, ay, aw, ah) = (num(&a["x"]), num(&a["y"]), num(&a["w"]), num(&a["h"]));
                    let (bx, by, bw, bh) = (num(&b["x"]), num(&b["y"]), num(&b["w"]), num(&b["h"]));
                    let apart = ax + aw + 0.004 <= bx + 1e-6 || bx + bw + 0.004 <= ax + 1e-6 || ay + ah + 0.004 <= by + 1e-6 || by + bh + 0.004 <= ay + 1e-6;
                    assert!(apart, "overlap: {a} {b}");
                }
            }
        }
        assert_eq!(placed, 40);
        // waste is what is not part: between 0 and 100
        let w = num(&plan["summary"]["waste_pct"]);
        assert!((0.0..100.0).contains(&w), "{w}");
    }
}
