//! Hatch patterns (#4, design step 7): the `.pat` parser and its limits, the built-in set, the generator
//! (clipping with holes, dash phase, angle and scale, limits), the commands (tag hatches, patterns kept in the
//! model, undo, file, policy), and hatched cut regions on sheets (SVG, PDF, a DXF `HATCH` that round-trips) and
//! on screen (the `section` command).

#[path = "pdftext/mod.rs"]
mod pdftext;
#[path = "fuzzlib.rs"]
mod fuzzlib;

use serde_json::{json, Value};
use sketchcraft::draft::hatch::{self, Placed};
use sketchcraft::draft::pat::{self, Units};
use sketchcraft::draft::{self, ir::*};
use sketchcraft::layout::{PageSpec, ViewportSpec};
use sketchcraft::policy::{permits, vet, Caller, Workspace};
use sketchcraft::Engine;

fn ok(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn fails(e: &mut Engine, v: Value) -> String {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], false, "{v} -> {r}");
    r["error"].as_str().unwrap_or_default().to_string()
}

fn scratch(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("sc-hatch-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

// ---- the parser ---------------------------------------------------------------------------------

#[test]
fn parses_headers_families_dashes_comments_and_units() {
    let text = "\u{feff}; a comment\r\n*MINE, my own; trailing\r\n45, 0,0, 0,3.175\r\n\r\n*Dashy,with dashes\n0, 1,2, 0.5,4, 2,-1,0,-1 ; after\n;%UNITS=INCH\n*inchy\n90, 0,0, 0,0.5, 0.25,-0.25\n";
    let p = pat::parse(text, Units::Mm);
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    assert_eq!(p.patterns.len(), 3);
    let a = &p.patterns[0];
    assert_eq!((a.name.as_str(), a.desc.as_str()), ("MINE", "my own"));
    assert_eq!(a.families.len(), 1);
    assert_eq!(a.families[0].delta, [0.0, 3.175]);
    let b = &p.patterns[1];
    assert_eq!(b.families[0].origin, [1.0, 2.0]);
    assert_eq!(b.families[0].dash, vec![2.0, -1.0, 0.0, -1.0]);
    // the units line turns inches into millimetres for what follows
    let c = &p.patterns[2];
    assert!((c.families[0].delta[1] - 12.7).abs() < 1e-9 && (c.families[0].dash[0] - 6.35).abs() < 1e-9, "{c:?}");
    // the caller's default applies when the file says nothing
    let i = pat::parse("*x\n0,0,0,0,1,1,-1\n", Units::Inch);
    assert!((i.patterns[0].families[0].delta[1] - 25.4).abs() < 1e-9);
}

#[test]
fn junk_lines_are_skipped_with_reasons_and_good_patterns_kept() {
    let text = "0,0,0,0,1\n*good\n45,0,0,0,2\nnot,a,number,at,all\n0,0,0\n0,0,0,1,0\n0,0,0,0,1,-1,-2\n0,0,0,0,1,0,0\n*\n*bad name!\n0,0,0,0,1\n*empty\n*good\n0,0,0,0,1\n0,nan,0,0,1\n";
    let p = pat::parse(text, Units::Mm);
    let names: Vec<&str> = p.patterns.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["good"], "{:?}", p.warnings);
    let w = p.warnings.join("\n");
    for want in ["outside any pattern", "isn't one", "needs angle", "no spacing", "only gaps", "no length", "isn't a pattern name", "no lines it can draw", "appears twice"] {
        assert!(w.contains(want), "{want:?} not in\n{w}");
    }
}

#[test]
fn limits_hold_for_families_dashes_patterns_numbers_and_size() {
    // 65 families: the pattern is dropped
    let mut t = String::from("*many\n");
    for i in 0..65 {
        t += &format!("{i},0,0,0,1\n");
    }
    t += "*after\n0,0,0,0,1\n";
    let p = pat::parse(&t, Units::Mm);
    assert_eq!(p.patterns.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), vec!["after"]);
    assert!(p.warnings.iter().any(|w| w.contains("more than 64 lines")));
    // 17 dashes: that line is skipped
    let p = pat::parse(&format!("*d\n0,0,0,0,1{}\n0,0,0,0,1\n", ",1".repeat(17)), Units::Mm);
    assert_eq!(p.patterns[0].families.len(), 1);
    assert!(p.warnings[0].contains("more than 16 dashes"));
    // too many patterns
    let mut t = String::new();
    for i in 0..300 {
        t += &format!("*p{i}\n0,0,0,0,1\n");
    }
    let p = pat::parse(&t, Units::Mm);
    assert_eq!(p.patterns.len(), pat::MAX_PATTERNS);
    // huge numbers are refused, not kept
    let p = pat::parse("*h\n0,1e300,0,0,1\n0,0,0,0,1e6\n", Units::Mm);
    assert!(p.patterns.is_empty());
    // only the first 256 KB is read
    let big = format!("*big\n0,0,0,0,1\n{}", ";".repeat(pat::MAX_TEXT + 10));
    let p = pat::parse(&big, Units::Mm);
    assert_eq!(p.patterns.len(), 1);
    assert!(p.warnings[0].contains("only the start"));
    // the warning list itself is bounded
    let p = pat::parse(&"x\n".repeat(5000), Units::Mm);
    assert!(p.warnings.len() <= 21 && p.warnings.last().unwrap().starts_with("and "), "{}", p.warnings.len());
}

/// Every pattern a parse keeps is within the limits and drawable.
fn check_parsed(p: &pat::Parsed) {
    assert!(p.patterns.len() <= pat::MAX_PATTERNS);
    assert!(p.warnings.len() <= 21);
    for q in &p.patterns {
        assert!(pat::valid_name(&q.name), "{:?}", q.name);
        assert!(!q.families.is_empty() && q.families.len() <= pat::MAX_FAMILIES);
        for f in &q.families {
            assert!(f.dash.len() <= pat::MAX_DASHES);
            assert!(f.delta[1].abs() >= 1e-6);
            assert!([f.angle, f.origin[0], f.origin[1], f.delta[0], f.delta[1]].iter().chain(f.dash.iter()).all(|v| v.is_finite()));
        }
    }
}

#[test]
fn fuzz_garbage_and_mutated_pat_text_never_panics_and_stays_in_limits() {
    let mut r = fuzzlib::Rng(0x5eed_4a7c_0ff1_ce55);
    let seeds = [pat::BUILTIN.as_bytes().to_vec(), b"*A,b\n45,0,0,0,1,1,-1,0,-1\n;%UNITS=INCH\n*B\n0,0,0,0.1,0.2\n".to_vec()];
    let square = vec![vec![[0.0, 0.0], [30.0, 0.0], [30.0, 20.0], [0.0, 20.0]]];
    for i in 0..1500 {
        let bytes: Vec<u8> = if i % 3 == 0 {
            // pure garbage: random bytes, biased toward the characters the format uses
            let alphabet = b"*,;.-+0123456789eE \n\r\tabcXYZ%=\x00\xff";
            (0..r.below(400)).map(|_| if r.below(4) == 0 { r.next() as u8 } else { alphabet[r.below(alphabet.len())] }).collect()
        } else {
            fuzzlib::mutate(&mut r, &seeds[i % 2], true)
        };
        let text = String::from_utf8_lossy(&bytes);
        let p = pat::parse(&text, if i % 2 == 0 { Units::Mm } else { Units::Inch });
        check_parsed(&p);
        // whatever was kept can be laid over a region without panicking or going past the limit
        for q in p.patterns.iter().take(3) {
            let got = hatch::generate(&square, &hatch::place(q, 1.0, (i % 360) as f64), 20_000);
            assert!(got.lines.len() <= 20_000);
            assert!(got.lines.iter().flatten().flatten().all(|v| v.is_finite()));
        }
    }
}

#[test]
fn the_builtins_are_ours_parse_cleanly_and_round_trip_as_text() {
    assert!(pat::BUILTIN.contains("CC0"), "the built-ins say their licence");
    let p = pat::parse(pat::BUILTIN, Units::Mm);
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    let names: Vec<&str> = p.patterns.iter().map(|p| p.name.as_str()).collect();
    for want in ["diagonal", "cross", "steel", "grid", "dots", "brick", "concrete", "earth", "insulation", "wood", "sand"] {
        assert!(names.contains(&want), "{want} missing from {names:?}");
    }
    // nothing named like Autodesk's set
    assert!(!names.iter().any(|n| n.to_ascii_uppercase().starts_with("ANSI") || n.to_ascii_uppercase().starts_with("AR-")));
    for q in &p.patterns {
        let back = pat::parse(&pat::to_text(q, &q.name), Units::Inch);
        assert_eq!(back.patterns.len(), 1, "{}", q.name);
        assert_eq!(back.patterns[0].families, q.families, "{} (to_text says mm, whatever the default)", q.name);
    }
}

// ---- the generator ------------------------------------------------------------------------------

fn fam(angle: f64, base: [f64; 2], offset: [f64; 2], dash: &[f64]) -> Placed {
    Placed { angle, base, offset, dash: dash.to_vec() }
}

fn inside(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
    let mut c = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
            c = !c;
        }
    }
    c
}

#[test]
fn lines_are_clipped_to_the_region_and_holes_stay_open() {
    let outer = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
    let hole = vec![[3.0, 3.0], [7.0, 3.0], [7.0, 7.0], [3.0, 7.0]];
    let rings = vec![outer.clone(), hole.clone()];
    // horizontal lines every 1 mm, offset half a millimetre so none runs along an edge
    let got = hatch::generate(&rings, &[fam(0.0, [0.0, 0.5], [0.0, 1.0], &[])], 1000);
    assert!(got.over.is_none());
    // rows through the hole are split in two, the others are whole: 6 whole + 4 rows x 2
    assert_eq!(got.lines.len(), 6 + 8, "{:?}", got.lines);
    let total: f64 = got.lines.iter().map(|[a, b]| (b[0] - a[0]).hypot(b[1] - a[1])).sum();
    assert!((total - (100.0 - 16.0)).abs() < 1e-6, "the length covers the area between: {total}");
    for [a, b] in &got.lines {
        let m = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
        assert!(inside(m, &outer) && !inside(m, &hole), "{a:?}-{b:?}");
        for p in [a, b] {
            assert!(p[0] >= -1e-9 && p[0] <= 10.0 + 1e-9);
        }
    }
    // the same at 30 degrees: every segment's middle is in the region, outside the hole
    let got = hatch::generate(&rings, &[fam(30.0, [0.1, 0.0], [0.0, 0.7], &[])], 5000);
    assert!(got.lines.len() > 10);
    for [a, b] in &got.lines {
        let m = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
        assert!(inside(m, &outer) && !inside(m, &hole));
        let ang = (b[1] - a[1]).atan2(b[0] - a[0]).to_degrees();
        assert!((ang - 30.0).abs() < 1e-6, "{ang}");
    }
}

#[test]
fn dashes_start_at_the_line_origin_whatever_the_region() {
    // dash 2 on, 1 off, a dot, 1 off: period 4, from x = 0
    let f = fam(0.0, [0.0, 0.5], [0.0, 10.0], &[2.0, -1.0, 0.0, -1.0]);
    let a = hatch::generate(&[vec![[0.5, 0.0], [9.0, 0.0], [9.0, 1.0], [0.5, 1.0]]], std::slice::from_ref(&f), 1000);
    let xs: Vec<(f64, f64)> = a.lines.iter().map(|[p, q]| (p[0], q[0])).collect();
    // the first dash is cut by the region's left side at 0.5; then 3 (dot), 4-6, 7 (dot), 8-9 (cut at the right)
    let want = [(0.5, 2.0), (3.0, 3.0), (4.0, 6.0), (7.0, 7.0), (8.0, 9.0)];
    assert_eq!(xs.len(), want.len(), "{xs:?}");
    for (g, w) in xs.iter().zip(want) {
        assert!((g.0 - w.0).abs() < 1e-9 && (g.1 - w.1).abs() < 1e-9, "{xs:?}");
    }
    // a region further along gets the same phase
    let b = hatch::generate(&[vec![[100.0, 0.0], [104.0, 0.0], [104.0, 1.0], [100.0, 1.0]]], &[f], 1000);
    let xs: Vec<(f64, f64)> = b.lines.iter().map(|[p, q]| (p[0], q[0])).collect();
    assert_eq!(xs, vec![(100.0, 102.0), (103.0, 103.0)]);
}

#[test]
fn angle_and_scale_turn_and_stretch_the_pattern() {
    let p = pat::parse("*t\n0, 0,0, 0,2, 3,-1\n", Units::Mm).patterns.remove(0);
    let placed = hatch::place(&p, 2.5, 90.0);
    assert_eq!(placed.len(), 1);
    let f = &placed[0];
    assert!((f.angle - 90.0).abs() < 1e-9);
    // spacing 2 x 2.5 = 5 mm, now along -x (the step across a 0 degree line, turned 90)
    assert!((f.offset[0] + 5.0).abs() < 1e-9 && f.offset[1].abs() < 1e-9, "{:?}", f.offset);
    assert_eq!(f.dash, vec![7.5, -2.5]);
    // drawn: vertical lines 5 mm apart, dashes 7.5 long
    let got = hatch::generate(&[vec![[0.1, 0.0], [20.0, 0.0], [20.0, 20.0], [0.1, 20.0]]], &placed, 1000);
    let mut xs: Vec<i64> = got.lines.iter().map(|[a, _]| (a[0] * 1000.0).round() as i64).collect();
    xs.dedup();
    xs.sort();
    xs.dedup();
    assert_eq!(xs, vec![5000, 10000, 15000]);
    assert!(got.lines.iter().all(|[a, b]| (a[0] - b[0]).abs() < 1e-9 && (b[1] - a[1]).abs() <= 7.5 + 1e-9));
    // model scale: the drawing's 1:N divides it
    let u = sketchcraft::drafting::HatchUse { scale: 100.0, scale_mode: sketchcraft::drafting::ScaleMode::Model, ..Default::default() };
    assert!((hatch::scale_k(&u, 50.0) - 2.0).abs() < 1e-12);
    let u = sketchcraft::drafting::HatchUse { scale: 3.0, ..Default::default() };
    assert!((hatch::scale_k(&u, 50.0) - 3.0).abs() < 1e-12, "paper scale ignores the drawing's");
}

#[test]
fn pathological_hatches_degrade_with_a_reason_and_never_hang() {
    let big = vec![vec![[0.0, 0.0], [1000.0, 0.0], [1000.0, 1000.0], [0.0, 1000.0]]];
    // lines closer than 0.05 mm
    let got = hatch::generate(&big, &[fam(45.0, [0.0, 0.0], [0.0, 0.01], &[])], hatch::MAX_SEGMENTS);
    assert!(got.lines.is_empty() && got.over.as_deref().unwrap().contains("closer than"));
    // too many segments: refused before the work
    let t = std::time::Instant::now();
    let got = hatch::generate(&big, &[fam(0.0, [0.0, 0.0], [0.0, 0.06], &[0.01, -0.01])], hatch::MAX_SEGMENTS);
    assert!(got.lines.is_empty() && got.over.is_some(), "{:?}", got.over);
    assert!(t.elapsed().as_secs_f64() < 2.0);
    // a limit reached while drawing also stops it
    let got = hatch::generate(&big, &[fam(0.0, [0.0, 0.0], [0.0, 1.0], &[])], 100);
    assert!(got.lines.is_empty() && got.over.is_some());
    // nonsense in: nothing out
    assert!(hatch::generate(&[vec![[f64::NAN, 0.0], [1.0, 0.0], [1.0, 1.0]]], &[fam(0.0, [0.0, 0.0], [0.0, 1.0], &[])], 100).lines.is_empty());
    assert!(hatch::generate(&[], &[fam(0.0, [0.0, 0.0], [0.0, 1.0], &[])], 100).lines.is_empty());
    assert!(hatch::generate(&big, &[fam(f64::INFINITY, [0.0, 0.0], [0.0, 1.0], &[])], 100).lines.is_empty());
}

// ---- commands, file, undo, policy ---------------------------------------------------------------

/// A hollow box (walls around a room: the plan cut has a hole) on the "Walls" tag, and a solid block on "Slab".
fn model() -> Engine {
    let mut e = Engine::new();
    let g = ok(&mut e, json!({"cmd": "begin_group", "name": "Walls"}))["group"].as_u64().unwrap();
    let f = ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [4, 3, 0]}))["face"].as_u64().unwrap();
    let inner = ok(&mut e, json!({"cmd": "rect", "from": [0.3, 0.3, 0], "to": [3.7, 2.7, 0]}))["face"].as_u64().unwrap();
    ok(&mut e, json!({"cmd": "delete", "ids": [inner]}));
    let ring = if e.model.faces.contains_key(&(f as _)) { f } else { *e.model.faces.keys().next().unwrap() as u64 };
    let up = e.model.normal(ring as _).z > 0.0;
    ok(&mut e, json!({"cmd": "pushpull", "face": ring, "distance": if up { 2.5 } else { -2.5 }}));
    ok(&mut e, json!({"cmd": "end_group"}));
    ok(&mut e, json!({"cmd": "tag", "ids": [g], "tag": "Walls"}));
    let s = ok(&mut e, json!({"cmd": "begin_group", "name": "Slab"}))["group"].as_u64().unwrap();
    let f = ok(&mut e, json!({"cmd": "rect", "from": [5, 0, 0], "to": [7, 3, 0]}))["face"].as_u64().unwrap();
    let up = e.model.normal(f as _).z > 0.0;
    ok(&mut e, json!({"cmd": "pushpull", "face": f, "distance": if up { 2.5 } else { -2.5 }}));
    ok(&mut e, json!({"cmd": "end_group"}));
    ok(&mut e, json!({"cmd": "tag", "ids": [s], "tag": "Slab"}));
    e
}

#[test]
fn tag_style_sets_changes_and_removes_a_hatch_and_checks_it() {
    let mut e = model();
    let r = ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "hatch": {"pattern": "brick", "color": "#AA0000", "weight_mm": 0.18, "scale": 2, "angle": 370, "scale_mode": "model"}}));
    assert_eq!(r["style"]["cut"]["fill"], "hatch");
    let h = e.model.drafting.cut_hatch(Some("Walls")).unwrap().clone();
    assert_eq!((h.pattern.as_str(), h.color.as_str(), h.weight_mm, h.scale), ("brick", "#aa0000", 0.18, 2.0));
    assert!((h.angle - 10.0).abs() < 1e-9);
    assert_eq!(h.scale_mode, sketchcraft::drafting::ScaleMode::Model);
    // only what is given changes
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "hatch": {"angle": 45}}));
    let h2 = e.model.drafting.cut_hatch(Some("Walls")).unwrap();
    assert_eq!((h2.pattern.as_str(), h2.angle, h2.scale), ("brick", 45.0, 2.0));
    // the line style beside it is untouched, and vice versa
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "weight_mm": 0.7}));
    assert!(e.model.drafting.cut_hatch(Some("Walls")).is_some());
    // checks: nothing changes when one fails
    let before = e.model.drafting.clone();
    assert!(fails(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "hatch": {"pattern": "nope"}})).contains("no hatch pattern"));
    assert!(fails(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "hatch": {"color": "red"}})).contains("#rrggbb"));
    assert!(fails(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "hatch": {"scale": 0}})).contains("scale"));
    assert!(fails(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "hatch": {"weight_mm": 5}})).contains("weight"));
    assert!(fails(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "hatch": {"scale_mode": "inches"}})).contains("scale_mode"));
    assert!(fails(&mut e, json!({"cmd": "tag_style", "tag": "Slab", "hatch": {"angle": 10}})).contains("which pattern"));
    assert_eq!(e.model.drafting, before);
    // pattern names compare without case
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Slab", "hatch": {"pattern": "CONCRETE"}}));
    assert_eq!(e.model.drafting.cut_hatch(Some("Slab")).unwrap().pattern, "concrete");
    // removed: the tag keeps its line weight; a style with nothing left goes
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "hatch": {"pattern": ""}}));
    assert!(e.model.drafting.cut_hatch(Some("Walls")).is_none());
    assert_eq!(e.model.drafting.tag_style("Walls").unwrap().weight_mm, 0.7);
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Slab", "hatch": {"pattern": ""}}));
    assert!(e.model.drafting.tag_style("Slab").is_none());
    // rename_tag carries the hatch
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "hatch": {"pattern": "cross"}}));
    ok(&mut e, json!({"cmd": "rename_tag", "tag": "Walls", "to": "Masonry"}));
    assert_eq!(e.model.drafting.cut_hatch(Some("Masonry")).unwrap().pattern, "cross");
}

const MY_PAT: &str = ";%UNITS=INCH\n*Shingle, roof shingles\n0, 0,0, 0,0.25\n90, 0,0, 0.25,0.5, 0.25,-0.25\n*Other\n45,0,0,0,0.1\n";

#[test]
fn hatch_pattern_keeps_a_pat_in_the_model_and_lists_it() {
    let mut e = model();
    let r = ok(&mut e, json!({"cmd": "hatch_pattern", "name": "Shingle", "text": MY_PAT}));
    assert_eq!(r["families"], 2);
    // stored as one pattern, in millimetres, under its name
    let stored = e.model.drafting.patterns["Shingle"].clone();
    assert!(stored.starts_with(";%UNITS=MM\n*Shingle,") && !stored.contains("Other"), "{stored}");
    let p = pat::lookup(&e.model.drafting, "shingle").unwrap();
    assert!((p.families[0].delta[1] - 6.35).abs() < 1e-9);
    // the only pattern in a text can be kept under another name; units can be given
    ok(&mut e, json!({"cmd": "hatch_pattern", "name": "Lap", "text": "*x\n0,0,0,0,0.2\n", "units": "inch"}));
    assert!((pat::lookup(&e.model.drafting, "Lap").unwrap().families[0].delta[1] - 5.08).abs() < 1e-9);
    // a text with several patterns needs the name of one of them
    assert!(fails(&mut e, json!({"cmd": "hatch_pattern", "name": "Third", "text": MY_PAT})).contains("name the one"));
    assert!(fails(&mut e, json!({"cmd": "hatch_pattern", "name": "brick", "text": MY_PAT})).contains("built-in"));
    assert!(fails(&mut e, json!({"cmd": "hatch_pattern", "name": "bad/name", "text": MY_PAT})).contains("name"));
    assert!(fails(&mut e, json!({"cmd": "hatch_pattern", "name": "Z", "text": "garbage"})).contains("no hatch pattern could be read"));
    assert!(fails(&mut e, json!({"cmd": "hatch_pattern", "name": "Z", "text": MY_PAT, "units": "furlongs"})).contains("units"));
    // the list: built-ins first, then the model's; a text is only previewed
    let l = ok(&mut e, json!({"cmd": "hatch_patterns"}));
    let names: Vec<&str> = l["patterns"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap()).collect();
    assert_eq!(names.first(), Some(&"diagonal"));
    assert!(names.contains(&"Shingle") && names.contains(&"Lap"));
    let prev = ok(&mut e, json!({"cmd": "hatch_patterns", "text": MY_PAT}));
    assert_eq!(prev["patterns"].as_array().unwrap().len(), 2);
    assert_eq!(e.model.drafting.patterns.len(), 2, "previewing keeps nothing");
    // a tag can use it; then it can't be deleted until the tag lets go
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "hatch": {"pattern": "shingle"}}));
    assert_eq!(e.model.drafting.cut_hatch(Some("Walls")).unwrap().pattern, "Shingle");
    assert!(fails(&mut e, json!({"cmd": "hatch_pattern", "name": "Shingle", "delete": true})).contains("used by tag Walls"));
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "hatch": {"pattern": "diagonal"}}));
    ok(&mut e, json!({"cmd": "hatch_pattern", "name": "shingle", "delete": true}));
    assert!(!e.model.drafting.patterns.contains_key("Shingle"));
}

#[test]
fn hatches_survive_save_and_open_and_undo_one_step_each() {
    let dir = scratch("file");
    let mut e = model();
    ok(&mut e, json!({"cmd": "hatch_pattern", "name": "Shingle", "text": MY_PAT}));
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "hatch": {"pattern": "Shingle", "scale": 3}}));
    let a = dir.join("a.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": a}));
    let f: Value = serde_json::from_slice(&std::fs::read(&a).unwrap()).unwrap();
    assert_eq!(f["format"], 2, "hatches need the newer format, so an older build opens it read-only");
    assert_eq!(f["drafting"]["tag_styles"]["Walls"]["cut"]["hatch"]["pattern"], "Shingle");
    assert!(f["drafting"]["patterns"]["Shingle"].as_str().unwrap().contains("*Shingle"));
    let mut g = Engine::new();
    ok(&mut g, json!({"cmd": "open", "path": a}));
    assert_eq!(g.model.drafting, e.model.drafting);
    // undo: one step per command, back to no pattern at all
    ok(&mut g, json!({"cmd": "tag_style", "tag": "Walls", "hatch": {"angle": 30}}));
    ok(&mut g, json!({"cmd": "undo"}));
    assert_eq!(g.model.drafting.cut_hatch(Some("Walls")).unwrap().angle, 0.0);
    ok(&mut g, json!({"cmd": "redo"}));
    assert_eq!(g.model.drafting.cut_hatch(Some("Walls")).unwrap().angle, 30.0);
    let mut e2 = model();
    ok(&mut e2, json!({"cmd": "hatch_pattern", "name": "Shingle", "text": MY_PAT}));
    ok(&mut e2, json!({"cmd": "tag_style", "tag": "Walls", "hatch": {"pattern": "Shingle"}}));
    ok(&mut e2, json!({"cmd": "undo"}));
    assert!(e2.model.drafting.cut_hatch(Some("Walls")).is_none());
    assert!(e2.model.drafting.patterns.contains_key("Shingle"));
    ok(&mut e2, json!({"cmd": "undo"}));
    assert!(e2.model.drafting.patterns.is_empty());
    // listing takes no undo step
    ok(&mut e2, json!({"cmd": "hatch_patterns"}));
    ok(&mut e2, json!({"cmd": "redo"}));
    assert!(e2.model.drafting.patterns.contains_key("Shingle"), "the redo is still there after a read-only command");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn agents_and_plugins_may_load_and_use_patterns() {
    for who in [Caller::Plugin, Caller::Mcp(Workspace::anywhere())] {
        assert!(permits(&who, "hatch_pattern") && permits(&who, "hatch_patterns") && permits(&who, "tag_style"));
        assert!(!permits(&who, "import_pat"), "there is no path-reading command");
    }
    let mut c = json!({"cmd": "hatch_pattern", "name": "x", "text": MY_PAT});
    assert!(vet(&Caller::Mcp(Workspace::anywhere()), &mut c).is_ok());
}

// ---- sheets ---------------------------------------------------------------------------------------

fn plan_sheet(e: &Engine) -> Doc {
    let vp = ViewportSpec { view: "plan".into(), cut: Some(1.2), floor: Some(0.0), scale: Some("1:50".into()), x: 20.0, y: 20.0, w: 260.0, h: 170.0, ..Default::default() };
    let page = PageSpec { title: "Plan".into(), viewports: vec![vp], table: vec![], ..Default::default() };
    draft::sheet::build(&e.model, "A3", "Test", vec![page], false, &[], "2026-11-18").unwrap()
}

fn section_sheet(e: &Engine) -> Doc {
    let vp = ViewportSpec { view: "section".into(), section: Some([2.0, 1.5, 1.0, 0.0, 1.0, 0.0]), scale: Some("1:50".into()), x: 20.0, y: 20.0, w: 260.0, h: 170.0, ..Default::default() };
    let page = PageSpec { title: "Section".into(), viewports: vec![vp], table: vec![], ..Default::default() };
    draft::sheet::build(&e.model, "A3", "Test", vec![page], false, &[], "2026-11-18").unwrap()
}

fn hatches(d: &Doc) -> Vec<HatchPrim> {
    let mut out = vec![];
    d.pages[0].walk(&mut |p| {
        if let Prim::Hatch(h) = p {
            out.push(h.clone());
        }
    });
    out
}

#[test]
fn a_plan_hatches_each_tagged_region_and_leaves_the_rest_solid() {
    let mut e = model();
    let before = plan_sheet(&e);
    assert!(hatches(&before).is_empty(), "no hatch until a tag asks for one");
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "hatch": {"pattern": "diagonal", "color": "#c00000", "weight_mm": 0.18}}));
    let d = plan_sheet(&e);
    let hs = hatches(&d);
    assert!(!hs.is_empty(), "the Walls regions are hatched");
    assert!(hs.iter().any(|h| h.rings.len() == 2), "the walls' cut is a ring with the room as its hole: {:?}", hs.iter().map(|h| h.rings.len()).collect::<Vec<_>>());
    for h in &hs {
        assert_eq!(h.name, "diagonal");
        assert!(h.tint.is_none() && h.note.is_none());
        assert_eq!(h.pen, Pen::new(0.18, [0xc0, 0, 0]));
        assert!(!h.lines.is_empty());
        // every line stays in its region
        let outer = &h.rings[0];
        for [a, b] in &h.lines {
            let m = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
            let holes = h.rings[1..].iter().filter(|r| inside(m, r)).count();
            assert!(inside(m, outer) && holes == 0, "{m:?}");
        }
    }
    // the Slab is still the solid poché
    let solid = {
        let mut n = 0;
        d.pages[0].walk(&mut |p| {
            if let Prim::Path(PathPrim { fill: Some(f), pen: Some(_), .. }) = p {
                if f.rgb == Pens::new(false).cut_fill {
                    n += 1;
                }
            }
        });
        n
    };
    assert!(solid >= 1, "the untagged-hatch region keeps its solid fill");
    // model scale: a 1:100 drawing gets lines twice as close as 1:50 on the paper
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "hatch": {"scale_mode": "model", "scale": 50}}));
    let h50 = &hatches(&plan_sheet(&e))[0];
    let sp = |h: &HatchPrim| {
        let f = &h.families[0];
        let (s, c) = f.angle.to_radians().sin_cos();
        (f.offset[0] * -s + f.offset[1] * c).abs()
    };
    assert!((sp(h50) - 2.5).abs() < 1e-9, "2.5 mm x 50 of model at 1:50 is 2.5 mm of paper: {}", sp(h50));
}

#[test]
fn svg_and_pdf_clip_to_the_region_and_draw_the_hatch() {
    let mut e = model();
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "hatch": {"pattern": "brick", "color": "#336699", "weight_mm": 0.25}}));
    let d = plan_sheet(&e);
    let s = draft::svg::page(&d.pages[0]);
    assert!(s.contains("<g data-hatch=\"brick\" clip-path=\"url(#"), "{}", &s[..s.len().min(2000)]);
    assert!(s.contains("clip-rule=\"evenodd\""));
    assert!(s.contains("stroke=\"#336699\" stroke-width=\"0.25\""));
    // the PDF: an even-odd clip (W*) and the hatch's pen width
    let pdf = pdftext::parse(&draft::pdf::write(&d).unwrap());
    let page = pdf.pages()[0];
    let c = pdf.content(page);
    assert!(c.contains("W*"), "an even-odd clip");
    assert!(pdf.line_widths_mm(page).iter().any(|w| (w - 0.25).abs() < 0.01));
    assert!(c.contains("0.2 0.4 0.6 RG"), "the hatch colour");
    // a section viewport through the walls hatches its cut too
    let d = section_sheet(&e);
    assert!(!hatches(&d).is_empty(), "the section's cut walls are hatched");
    assert!(draft::svg::page(&d.pages[0]).contains("data-hatch=\"brick\""));
}

/// A DXF as (group code, value) pairs.
fn groups(dxf: &str) -> Vec<(i32, String)> {
    let l: Vec<&str> = dxf.lines().collect();
    l.chunks(2).filter(|c| c.len() == 2).filter_map(|c| Some((c[0].trim().parse().ok()?, c[1].trim().to_string()))).collect()
}

/// The pattern HATCH entities of a DXF read back: name, boundary rings, families.
#[allow(clippy::type_complexity)]
fn read_hatches(dxf: &str) -> Vec<(String, Vec<Vec<[f64; 2]>>, Vec<Placed>)> {
    let g = groups(dxf);
    let mut out = vec![];
    let mut i = 0;
    while i < g.len() {
        if g[i] != (0, "HATCH".into()) {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        let mut end = j;
        while end < g.len() && g[end].0 != 0 {
            end += 1;
        }
        let body = &g[j..end];
        let num = |k: usize| body[k].1.parse::<f64>().unwrap();
        let solid = body.iter().find(|(c, _)| *c == 70).map(|(_, v)| v == "1").unwrap_or(true);
        if solid {
            i = end;
            continue;
        }
        let name = body.iter().find(|(c, _)| *c == 2).unwrap().1.clone();
        let mut rings = vec![];
        let mut fams = vec![];
        j = 0;
        while j < body.len() {
            match body[j].0 {
                93 => {
                    let n: usize = body[j].1.parse().unwrap();
                    let mut r = vec![];
                    for k in 0..n {
                        r.push([num(j + 1 + 2 * k), num(j + 2 + 2 * k)]);
                    }
                    rings.push(r);
                    j += 1 + 2 * n;
                }
                53 => {
                    let nd: usize = body[j + 5].1.parse().unwrap();
                    fams.push(Placed { angle: num(j), base: [num(j + 1), num(j + 2)], offset: [num(j + 3), num(j + 4)], dash: (0..nd).map(|k| num(j + 6 + k)).collect() });
                    j += 6 + nd;
                }
                _ => j += 1,
            }
        }
        out.push((name, rings, fams));
        i = end;
    }
    out
}

#[test]
fn dxf_writes_a_native_hatch_that_round_trips_to_the_same_lines() {
    let mut e = model();
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "hatch": {"pattern": "brick", "scale": 1.5, "angle": 30, "weight_mm": 0.35, "color": "#00ff00"}}));
    let d = plan_sheet(&e);
    let page = &d.pages[0];
    let dxf = draft::dxf::page(page);
    let g = groups(&dxf);
    // a custom pattern hatch (76 = 2) with the pattern's two families, odd parity
    let at = g.iter().position(|x| *x == (2, "BRICK".into())).expect("a HATCH named BRICK");
    let tail: Vec<&(i32, String)> = g[at..].iter().take_while(|x| x.0 != 0).collect();
    assert!(tail.contains(&&(70, "0".into())) && tail.contains(&&(76, "2".into())) && tail.contains(&&(78, "2".into())) && tail.contains(&&(75, "0".into())));
    assert!(tail.contains(&&(52, "30".into())) && tail.contains(&&(41, "1.5".into())), "{tail:?}");
    // lineweight and colour on the entity
    let ent_start = g[..at].iter().rposition(|x| *x == (0, "HATCH".into())).unwrap();
    let head = &g[ent_start..at];
    assert!(head.contains(&(370, "35".into())) && head.contains(&(420, "65280".into())), "{head:?}");
    // read back, the pattern and boundary regenerate exactly the lines the PDF and SVG draw
    let back = read_hatches(&dxf);
    let prims = hatches(&d);
    assert_eq!(back.len(), prims.len());
    for ((name, rings, fams), h) in back.iter().zip(&prims) {
        assert_eq!(name, "BRICK");
        assert_eq!(fams.len(), 2);
        for (a, b) in fams.iter().zip(&h.families) {
            assert!((a.angle - b.angle).abs() < 1e-9 && a.dash.len() == b.dash.len());
            for (x, y) in a.base.iter().chain(&a.offset).chain(&a.dash).zip(b.base.iter().chain(&b.offset).chain(&b.dash)) {
                assert!((x - y).abs() < 1e-9, "{a:?} vs {b:?}");
            }
        }
        let again = hatch::generate(rings, fams, hatch::MAX_SEGMENTS);
        assert_eq!(again.lines.len(), h.lines.len());
        for (p, q) in again.lines.iter().zip(&h.lines) {
            for k in 0..2 {
                // the DXF is y up from the page's bottom
                assert!((p[k][0] - q[k][0]).abs() < 1e-6 && (p[k][1] - (page.h - q[k][1])).abs() < 1e-6, "{p:?} vs {q:?}");
            }
        }
    }
}

#[test]
fn a_hatch_too_dense_for_its_region_prints_a_tint_and_the_export_says_so() {
    let dir = scratch("dense");
    let mut e = model();
    // 2.5 mm x 0.01 = 0.025 mm apart: closer than the limit
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "hatch": {"pattern": "diagonal", "scale": 0.01}}));
    let d = plan_sheet(&e);
    let hs = hatches(&d);
    assert!(hs.iter().all(|h| h.lines.is_empty() && h.tint.is_some() && h.note.is_some()));
    assert!(draft::svg::page(&d.pages[0]).contains("data-hatch-tint=\"1\""));
    let s = ok(&mut e, json!({"cmd": "sheet_add", "name": "P", "paper": "a3"}))["sheet"].as_u64().unwrap();
    ok(&mut e, json!({"cmd": "viewport_add", "sheet": s, "view": {"type": "plan", "cut_z": 1.2, "floor_z": 0.0}, "scale": "1:50", "rect_mm": [15, 15, 250, 180]}));
    for ext in ["pdf", "svg", "dxf"] {
        let path = dir.join(format!("p.{ext}")).to_string_lossy().to_string();
        let r = ok(&mut e, json!({"cmd": "export_sheets", "path": path}));
        let w: Vec<&str> = r["warnings"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert!(w.iter().any(|w| w.contains("hatch \"diagonal\" drawn as a plain tint") && w.contains("closer than")), "{ext}: {w:?}");
    }
    // a pattern the model lost is a tint too, never an error
    let mut d2 = e.model.drafting.clone();
    if let Some(sketchcraft::drafting::Keep::Known(t)) = d2.tag_styles.get_mut("Walls") {
        t.cut.hatch.as_mut().unwrap().pattern = "gone".into();
    }
    e.model.drafting = d2;
    let hs = hatches(&plan_sheet(&e));
    assert!(hs.iter().all(|h| h.note.as_deref().unwrap().contains("no hatch pattern \"gone\"")));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn saved_sheets_export_with_hatched_cuts() {
    let dir = scratch("saved");
    let mut e = model();
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "hatch": {"pattern": "cross"}}));
    let s = ok(&mut e, json!({"cmd": "sheet_add", "name": "P", "paper": "a3"}))["sheet"].as_u64().unwrap();
    ok(&mut e, json!({"cmd": "viewport_add", "sheet": s, "view": {"type": "plan", "cut_z": 1.2, "floor_z": 0.0}, "scale": "1:50", "rect_mm": [15, 15, 250, 180]}));
    let path = dir.join("p.svg").to_string_lossy().to_string();
    let r = ok(&mut e, json!({"cmd": "export_sheets", "path": path}));
    assert!(r["warnings"].as_array().unwrap().is_empty(), "{r}");
    let svg = std::fs::read_to_string(&path).unwrap();
    assert!(svg.contains("data-hatch=\"cross\""));
    std::fs::remove_dir_all(&dir).ok();
}

// ---- on screen --------------------------------------------------------------------------------

#[test]
fn the_section_command_gives_the_screen_hatch_lines_per_tag() {
    let mut e = model();
    let plain = ok(&mut e, json!({"cmd": "section", "origin": [0, 0, 1.2], "normal": [0, 0, -1]}));
    assert!(plain["hatch"].as_array().unwrap().is_empty());
    let all_fill = plain["fill"].as_array().unwrap().len();
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "hatch": {"pattern": "diagonal", "color": "#ff0000", "scale_mode": "model", "scale": 50}}));
    let r = ok(&mut e, json!({"cmd": "section", "origin": [0, 0, 1.2], "normal": [0, 0, -1]}));
    let h = r["hatch"].as_array().unwrap();
    assert_eq!(h.len(), 1, "one hatch, the Walls tag's: {r}");
    assert_eq!(h[0]["tag"], "Walls");
    assert_eq!(h[0]["color"], "#ff0000");
    let lines: Vec<f64> = h[0]["lines"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert!(lines.len() >= 6 && lines.len().is_multiple_of(6));
    // in world metres, on the cut plane, inside the walls' footprint
    for p in lines.chunks(3) {
        assert!((p[2] - 1.2).abs() < 1e-6 && p[0] > -1e-6 && p[0] < 4.0 + 1e-6 && p[1] > -1e-6 && p[1] < 3.0 + 1e-6, "{p:?}");
    }
    // the hatched regions are listed apart from the solid fill (paper under the hatch)
    let solid = r["fill"].as_array().unwrap().len();
    let paper = r["hatchFill"].as_array().unwrap().len();
    assert!(paper > 0 && solid > 0 && solid + paper == all_fill, "{solid} + {paper} vs {all_fill}");
}
