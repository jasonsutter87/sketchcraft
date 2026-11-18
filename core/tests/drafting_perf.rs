//! The big-document benchmark (#8, design §4): the 52-page A1 permit set of `perf_fixture` against the budget table.
//!
//! - `SKETCHCRAFT_PERF=full` (the `perf` workflow, release build, ubuntu 4 vCPU): the full fixture (about 200,000
//!   faces); every measure must be within its budget and within 20% of `drafting_perf.baseline.json` (when that has
//!   a number for it). `SKETCHCRAFT_PERF_WRITE_BASELINE=1` writes the measured numbers there instead of checking them.
//! - otherwise (every `cargo test`): the small fixture, the same pages and counters, no timing checks.
//!
//! Counter assertions are exact in both. The results go to stdout as a table, and to `$GITHUB_STEP_SUMMARY` and
//! `$SKETCHCRAFT_PERF_OUT` (JSON) when those are set.

#[path = "perf_fixture/mod.rs"]
mod perf_fixture;

use perf_fixture::{build, ok, Size};
use serde_json::{json, Map, Value};
use sketchcraft::model::Id;
use sketchcraft::Engine;
use std::time::Instant;

/// (key, what, budget in ms or MB)
const BUDGETS: [(&str, &str, f64); 8] = [
    ("open_added_ms", "Open the file (time added over the model alone)", 300.0),
    ("first_page_cold_ms", "First view of a page (cold, the slowest page kind)", 1500.0),
    ("switch_page_p95_ms", "Switch to a page already drawn (p95)", 50.0),
    ("commit_edit_ms", "Commit a dimension or paper line edit", 50.0),
    ("wall_redraw_max_ms", "Move one wall: slowest redrawn viewport", 1000.0),
    ("export_cold_ms", "Export all 52 pages, cold", 20_000.0),
    ("export_warm_ms", "Export all 52 pages, warm", 5_000.0),
    ("pdf_mb", "PDF size", 60.0),
];
/// Peak memory (Linux only: VmHWM).
const RSS_BUDGET_MB: f64 = 1536.0;
/// How far over the baseline a timing may be before it is a regression.
const OVER_BASELINE: f64 = 1.2;

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn p95(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() as f64 * 0.95).ceil() as usize).saturating_sub(1).min(v.len() - 1)]
}

fn status_mb(key: &str) -> Option<f64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = s.lines().find(|l| l.starts_with(key))?;
    let kb: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024.0)
}

/// The peak memory of one phase (Linux only): the high-water mark since the last phase, then reset to what is held
/// now (`/proc/self/clear_refs` 5), so each phase's peak is its own.
fn phase(got: &mut Map<String, Value>, name: &str) {
    if let Some(hwm) = status_mb("VmHWM:") {
        got.insert(name.to_string(), json!(hwm.round()));
        let _ = std::fs::write("/proc/self/clear_refs", "5");
    }
}

fn scratch() -> std::path::PathBuf {
    let base = std::env::var_os("SKETCHCRAFT_PERF_DIR").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let d = base.join(format!("sc-drafting-perf-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn open(path: &std::path::Path) -> (Engine, f64) {
    let mut e = Engine::new();
    let t = Instant::now();
    ok(&mut e, json!({"cmd": "open", "path": path}));
    (e, ms(t))
}

#[test]
fn drafting_perf() {
    let full = std::env::var("SKETCHCRAFT_PERF").is_ok_and(|v| v == "full");
    let size = if full { Size::full() } else { Size::small() };
    let dir = scratch();
    let t = Instant::now();
    let mut f = build(size);
    let build_ms = ms(t);
    let mut phases = Map::new();
    let mut got = Map::new();
    // counter checks: all are reported with the timings, then fail the test (exact in both sizes)
    let mut wrong: Vec<String> = Vec::new();
    let mut expect = |ok: bool, what: String| {
        if !ok {
            wrong.push(what);
        }
    };
    got.insert("faces".into(), json!(f.faces));
    got.insert("fixture_ms".into(), json!(build_ms.round()));
    if full {
        assert!(f.faces >= 190_000, "the full fixture is about 200,000 faces ({})", f.faces);
    }
    let dims = f.e.model.sheet_items.values().filter(|i| matches!(&***i, sketchcraft::drafting::SheetItem::Known(sketchcraft::drafting::ItemKind::Dimension(_)))).count();
    assert_eq!(dims, size.dims);

    // open: the file with its sheets, against the same model without them
    let with = dir.join("set.sketchcraft");
    ok(&mut f.e, json!({"cmd": "save", "path": with}));
    let mut bare_model = f.e.model.clone();
    bare_model.sheets.clear();
    bare_model.sheet_items.clear();
    let bare = dir.join("bare.sketchcraft");
    let mut b = Engine::new();
    b.model = bare_model;
    ok(&mut b, json!({"cmd": "save", "path": bare}));
    drop(b);
    // the engine that built the set is done with: the rest is measured on the opened file alone
    drop(f);
    phase(&mut phases, "fixture");
    let (mut t_with, mut t_bare) = (Vec::new(), Vec::new());
    for _ in 0..3 {
        t_bare.push(open(&bare).1);
        t_with.push(open(&with).1);
    }
    got.insert("open_added_ms".into(), json!((median(t_with) - median(t_bare)).max(0.0)));
    phase(&mut phases, "open");
    let (mut e, _) = open(&with);
    expect(e.drawings.stats().drawn == 0, "nothing is drawn at open".into());
    let sheets: Vec<Id> = {
        let mut v: Vec<(i32, Id)> = e.model.sheets.iter().map(|(id, s)| (s.order, *id)).collect();
        v.sort();
        v.into_iter().map(|(_, id)| id).collect()
    };
    let by_name = |e: &Engine, n: &str| *e.model.sheets.iter().find(|(_, s)| s.name == n).unwrap().0;

    // the first view of a page, cold: a plan page, an elevation, a section, a detail page, an axonometric
    let mut cold = Vec::new();
    for n in ["Floor plan 0", "South elevation", "Section A0", "Details 4-0", "Axonometric 0"] {
        e.drawings.clear();
        let s = by_name(&e, n);
        let t = Instant::now();
        e.sheet_page(s).unwrap();
        cold.push(ms(t));
        e.drawings.wait();
    }
    got.insert("first_page_cold_ms".into(), json!(cold.iter().cloned().fold(0.0, f64::max)));
    phase(&mut phases, "first_pages");

    // export cold, then warm
    e.drawings.clear();
    e.drawings.reset_stats();
    let pdf = dir.join("set.pdf");
    let t = Instant::now();
    ok(&mut e, json!({"cmd": "export_sheets", "path": pdf}));
    got.insert("export_cold_ms".into(), json!(ms(t)));
    let s = e.drawings.stats();
    expect(s.drawn == 70, format!("a cold export draws each of the 70 viewports once ({} drawn)", s.drawn));
    got.insert("kept_mb".into(), json!(e.drawings.bytes() as f64 / 1048576.0));
    got.insert("kept_viewports".into(), json!(e.drawings.len()));
    got.insert("hatches_made".into(), json!(s.hatch_made));
    got.insert("hatches_kept".into(), json!(s.hatch_kept));
    phase(&mut phases, "export_cold");
    let t = Instant::now();
    ok(&mut e, json!({"cmd": "export_sheets", "path": pdf}));
    got.insert("export_warm_ms".into(), json!(ms(t)));
    let s = e.drawings.stats();
    expect(s.drawn == 70, format!("a warm export draws nothing ({} drawn, {} let go for memory)", s.drawn - 70, s.evicted));
    got.insert("pdf_mb".into(), json!(std::fs::metadata(&pdf).unwrap().len() as f64 / 1048576.0));
    phase(&mut phases, "export_warm");

    // switching between pages already drawn
    let mut sw = Vec::new();
    for s in &sheets {
        let t = Instant::now();
        e.sheet_page(*s).unwrap();
        sw.push(ms(t));
    }
    e.drawings.wait();
    got.insert("switch_page_p95_ms".into(), json!(p95(sw)));

    // committing paper edits: a dimension and a line (neither redraws a viewport)
    let plan = by_name(&e, "Floor plan 0");
    let vs: Vec<Id> = e.model.vertices.keys().take(2).copied().collect();
    let mut edits = Vec::new();
    for k in 0..5 {
        let t = Instant::now();
        ok(&mut e, json!({"cmd": "sheet_dim", "sheet": plan, "anchors": [{"vertex": vs[0]}, {"vertex": vs[1]}], "offset_mm": 20 + k}));
        edits.push(ms(t));
        let t = Instant::now();
        ok(&mut e, json!({"cmd": "sheet_line", "sheet": plan, "pts_mm": [[700, 500 + k], [800, 500 + k]]}));
        edits.push(ms(t));
    }
    e.drawings.wait();
    got.insert("commit_edit_ms".into(), json!(median(edits)));
    expect(e.drawings.stats().dropped == 0, "paper edits drop no drawing".into());

    // moving one wall: exactly the viewports that see it are redrawn (30 of 70: see `drafting_cache`)
    let wall = *e.model.groups.iter().find(|(_, g)| g.name == "Wall I0-0a").unwrap().0;
    e.drawings.wait();
    let before = e.drawings.stats();
    ok(&mut e, json!({"cmd": "move", "ids": [wall], "by": [0.4, 0.0, 0.0]}));
    e.drawings.wait();
    let after_move = e.drawings.stats();
    expect(after_move.dropped - before.dropped == 30, format!("moving one wall drops exactly the 30 viewports that see it ({})", after_move.dropped - before.dropped));
    // the slowest of them, drawn one page at a time
    let stale: Vec<Id> = {
        let kept: std::collections::HashMap<Id, bool> = e.kept_viewports().into_iter().collect();
        let mut pages: Vec<Id> = e.model.sheet_items.iter().filter_map(|(id, i)| match &**i {
            sketchcraft::drafting::SheetItem::Known(sketchcraft::drafting::ItemKind::Viewport(v)) if !kept[id] => Some(v.sheet),
            _ => None,
        }).collect();
        pages.sort();
        pages.dedup();
        pages
    };
    // the model's world geometry (every placement expanded, triangulated) is worked out once after a change and shared by
    // all the redrawn viewports: timed on its own, not charged to the first one
    let t = Instant::now();
    let w = e.drawings.world();
    w.faces(&e.model);
    w.edges(&e.model);
    w.tris(&e.model);
    got.insert("world_ms".into(), json!(ms(t)));
    let mut worst: f64 = 0.0;
    let mut slowest = String::new();
    for s in stale {
        let n0 = e.drawings.stats().drawn;
        let t = Instant::now();
        e.sheet_page(s).unwrap();
        let n = (e.drawings.stats().drawn - n0).max(1);
        let each = ms(t) / n as f64;
        if each > worst {
            worst = each;
            slowest = format!("{} ({n} drawn)", e.model.sheets[&s].name);
        }
    }
    got.insert("wall_redraw_slowest".into(), json!(slowest));
    e.drawings.wait();
    got.insert("wall_redraw_max_ms".into(), json!(worst));
    expect(e.drawings.stats().drawn - after_move.drawn == 30, format!("and redraws them once each ({})", e.drawings.stats().drawn - after_move.drawn));
    phase(&mut phases, "wall_move");
    // the whole run's peak is the largest phase's
    if let Some(r) = phases.values().filter_map(Value::as_f64).reduce(f64::max) {
        got.insert("peak_rss_mb".into(), json!(r));
    }
    got.insert("peaks_mb".into(), Value::Object(phases));
    std::fs::remove_dir_all(&dir).ok();

    for w in &wrong {
        println!("counter wrong: {w}");
    }
    report(&got, full);
    assert!(wrong.is_empty(), "counters:\n{}", wrong.join("\n"));
}

fn report(got: &Map<String, Value>, full: bool) {
    let base_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/drafting_perf.baseline.json");
    let baseline: Map<String, Value> = std::fs::read_to_string(&base_path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let mut table = String::from("| Measure | Got | Budget | Baseline |\n|---|---|---|---|\n");
    let mut fails = Vec::new();
    let num = |k: &str| got.get(k).and_then(Value::as_f64).unwrap_or(f64::NAN);
    for (k, what, budget) in BUDGETS {
        let v = num(k);
        let b = baseline.get(k).and_then(Value::as_f64);
        table.push_str(&format!("| {what} | {v:.1} | {budget} | {} |\n", b.map_or("-".into(), |b| format!("{b:.1}"))));
        if full && v > budget {
            fails.push(format!("{what}: {v:.1} is over the budget {budget}"));
        }
        // a regression: more than 20% over the baseline (and over 5 ms, so tiny numbers don't flap)
        if let (true, Some(b)) = (full, b) {
            if v > b * OVER_BASELINE && v - b > 5.0 {
                fails.push(format!("{what}: {v:.1} is more than 20% over the baseline {b:.1}"));
            }
        }
    }
    if let Some(r) = got.get("peak_rss_mb").and_then(Value::as_f64) {
        table.push_str(&format!("| Peak memory (MB) | {r:.0} | {RSS_BUDGET_MB} | - |\n"));
        if full && r > RSS_BUDGET_MB {
            fails.push(format!("peak memory {r:.0} MB is over {RSS_BUDGET_MB} MB"));
        }
    }
    table.push_str(&format!("| Peak memory by phase (MB) | {} | | |\n", got.get("peaks_mb").map_or("-".into(), |v| v.to_string())));
    table.push_str(&format!("| Slowest viewport redrawn after the wall move | {} | | |\n", got.get("wall_redraw_slowest").map_or("-".into(), |v| v.to_string())));
    table.push_str(&format!("| Faces / fixture build (ms) | {} / {} | | |\n", got["faces"], got["fixture_ms"]));
    table.push_str(&format!("| After a change: the model's world geometry, once (ms) | {:.1} | | |\n", num("world_ms")));
    table.push_str(&format!("| Kept drawings (MB, viewports) / hatches made, reused | {:.0}, {} / {}, {} | | |\n", num("kept_mb"), got["kept_viewports"], got["hatches_made"], got["hatches_kept"]));
    table.push_str("| Pan and zoom an A1 page (fps) | not measured: the sheet editor (step 11) isn't built | 50 | |\n");
    println!("{}\n{table}", if full { "drafting_perf (full)" } else { "drafting_perf (small: no timing checks)" });
    if let Some(p) = std::env::var_os("GITHUB_STEP_SUMMARY") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).create(true).open(p) {
            let _ = writeln!(f, "## Drafting performance{}\n\n{table}", if full { "" } else { " (small)" });
        }
    }
    if let Some(p) = std::env::var_os("SKETCHCRAFT_PERF_OUT") {
        let _ = std::fs::write(p, serde_json::to_string_pretty(&Value::Object(got.clone())).unwrap());
    }
    if full && std::env::var("SKETCHCRAFT_PERF_WRITE_BASELINE").is_ok_and(|v| v == "1") {
        let keep: Map<String, Value> = BUDGETS.iter().filter_map(|(k, ..)| got.get(*k).map(|v| (k.to_string(), json!((v.as_f64().unwrap_or(0.0) * 10.0).round() / 10.0)))).collect();
        std::fs::write(&base_path, serde_json::to_string_pretty(&Value::Object(keep)).unwrap() + "\n").unwrap();
        println!("wrote {}", base_path.display());
        return;
    }
    assert!(fails.is_empty(), "over budget or regressed:\n{}", fails.join("\n"));
}
