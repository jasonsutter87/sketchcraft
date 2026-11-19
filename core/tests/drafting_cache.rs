//! Kept drawings for big sheet sets (#8, design step 4, §4): a viewport's drawing is kept between page views and
//! exports, dropped exactly when a change can reach it (counter assertions on the 52-page, 70-viewport set), and
//! a kept page is always the page drawn afresh. Hatches are kept per region; the pages looked at last are redrawn
//! in the background; `drafting_status` reports it.

#[path = "perf_fixture/mod.rs"]
mod perf_fixture;

use perf_fixture::{build, ok, Fixture, Size};
use serde_json::json;
use sketchcraft::model::Id;
use sketchcraft::Engine;
use std::collections::BTreeSet;

fn scratch(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("sc-drawcache-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn export(e: &mut Engine, dir: &std::path::Path) {
    ok(e, json!({"cmd": "export_sheets", "path": dir.join("set.pdf")}));
}

/// The labels of the viewports whose drawings are not kept.
fn stale(f: &Fixture) -> BTreeSet<String> {
    let kept: std::collections::HashMap<Id, bool> = f.e.kept_viewports().into_iter().collect();
    f.viewports.iter().filter(|(id, _)| !kept.get(id).copied().unwrap_or(false)).map(|(_, l)| l.clone()).collect()
}

/// Every page drawn from the kept drawings equals the page drawn afresh.
fn kept_equals_fresh(f: &mut Fixture, why: &str) {
    for s in f.sheets.clone() {
        let kept = f.e.sheet_page(s).unwrap();
        let fresh = f.e.sheet_page_fresh(s).unwrap();
        assert!(kept.items == fresh.items, "{why}: sheet {} drawn from kept drawings differs from a fresh drawing", f.e.model.sheets[&s].name);
    }
    f.e.drawings.wait();
}

fn set(labels: &[&str]) -> BTreeSet<String> {
    labels.iter().map(|s| s.to_string()).collect()
}

#[test]
fn the_set_has_52_a1_pages_and_70_viewports_and_nothing_is_drawn_at_open() {
    let f = build(Size::small());
    assert_eq!(f.sheets.len(), 52);
    assert_eq!(f.viewports.len(), 70);
    assert!(f.e.model.sheets.values().all(|s| s.paper.name.eq_ignore_ascii_case("a1") && s.paper.w_mm == 841.0), "A1 pages");
    // nothing is drawn until a page is looked at or exported
    assert_eq!(f.e.drawings.len(), 0);
    assert_eq!(f.e.drawings.stats().drawn, 0);
    // saved and opened: still nothing drawn
    let dir = scratch("open");
    let mut e = f.e;
    ok(&mut e, json!({"cmd": "save", "path": dir.join("set.sketchcraft")}));
    let mut o = Engine::new();
    ok(&mut o, json!({"cmd": "open", "path": dir.join("set.sketchcraft")}));
    assert_eq!(o.model.sheets.len(), 52);
    assert_eq!(o.drawings.stats().drawn, 0);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn one_wall_moved_redraws_exactly_the_viewports_that_see_it() {
    let mut f = build(Size::small());
    let dir = scratch("wall");
    export(&mut f.e, &dir);
    let cold = f.e.drawings.stats();
    assert_eq!(cold.drawn, 70, "a cold export draws every viewport once");
    assert_eq!(f.e.drawings.len(), 70);
    // a warm export draws nothing
    export(&mut f.e, &dir);
    let warm = f.e.drawings.stats();
    assert_eq!(warm.drawn, 70);
    assert_eq!(warm.kept, 70);

    // the interior wall at x = 5 m in the south half of the ground floor moves 0.4 m east
    ok(&mut f.e, json!({"cmd": "move", "ids": [f.wall], "by": [0.4, 0.0, 0.0]}));
    let gone = stale(&f);
    // what sees it: the ground floor's six plans and six key plans, the cover and notes key plans, every elevation
    // at 1:50 and 1:100 (the wall stands inside each frame), the sections whose kept side holds it (A0 looks +x
    // from x = 2.5; B0 cuts it at y = 3 and looks +y, B1 and B2 look +y from beyond it: not), and the four
    // axonometrics and the cover view, and the west bay at 1:20 (it looks along the south rooms, past the wall's
    // end). Not: the upper floor, the other bays, sections A1/A2 (behind them), any detail (corners and the far
    // east end), schedules.
    let mut want = set(&["Cover view", "Key plan ground", "Notes key plan 0", "South elevation", "North elevation", "West elevation", "East elevation", "West bay 1:20", "South elevation 1:100", "North elevation 1:100", "West elevation 1:100", "East elevation 1:100", "Section A0", "Section B0", "Axonometric 0", "Axonometric 1", "Axonometric 2", "Axonometric 3"]);
    for k in ["Floor", "Furniture", "Dimension", "Finishes", "Electrical", "Reflected"] {
        want.insert(format!("{k} plan 0"));
        want.insert(format!("{k} key 0"));
    }
    assert_eq!(gone, want, "exactly the viewports that see the wall are dropped");
    assert_eq!(f.e.drawings.stats().dropped, want.len() as u64);
    // the next export draws exactly those again
    export(&mut f.e, &dir);
    assert_eq!(f.e.drawings.stats().drawn, 70 + want.len() as u64, "{} of 70 redrawn", want.len());
    // and every page drawn from what was kept is the page drawn afresh
    kept_equals_fresh(&mut f, "after moving a wall");

    // a paper edit (a dimension, a line) redraws no viewport
    let before = f.e.drawings.stats();
    let s = f.sheets[6];
    ok(&mut f.e, json!({"cmd": "sheet_line", "sheet": s, "pts_mm": [[700, 500], [800, 500]]}));
    let v = *f.e.model.vertices.keys().next().unwrap();
    let w = *f.e.model.vertices.keys().nth(1).unwrap();
    ok(&mut f.e, json!({"cmd": "sheet_dim", "sheet": s, "anchors": [{"vertex": v}, {"vertex": w}]}));
    assert!(stale(&f).is_empty(), "paper edits keep every viewport");
    assert_eq!(f.e.drawings.stats().dropped, before.dropped);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn kept_drawings_always_equal_fresh_ones_through_edits_undo_and_redo() {
    let mut f = build(Size::small());
    kept_equals_fresh(&mut f, "cold");
    let wall = f.wall;
    let desk = *f.e.model.instances.keys().next().unwrap();
    let upper = f.e.model.groups.iter().find(|(_, g)| g.name == "Wall I1-2b").map(|(id, _)| *id).unwrap();
    // each edit, then what it may not drop (a viewport that can't see it)
    let edits: Vec<(serde_json::Value, &str)> = vec![
        (json!({"cmd": "move", "ids": [wall], "by": [0.3, 0.0, 0.0]}), "Floor plan 1"),
        (json!({"cmd": "move", "ids": [desk], "by": [0.0, 0.5, 0.0]}), "Floor plan 1"),
        (json!({"cmd": "hide", "ids": [upper]}), "Floor plan 0"),
        (json!({"cmd": "tag_style", "tag": "column", "hatch": {"pattern": "grid", "scale": 2.0}}), "Detail 0-0 a"),
        (json!({"cmd": "text", "at": [3.0, 3.0, 0.1], "text": "Living"}), "Floor plan 1"),
        (json!({"cmd": "undo"}), "Floor plan 1"),
        (json!({"cmd": "redo"}), "Floor plan 1"),
        (json!({"cmd": "undo"}), "Floor plan 1"),
        // (the column style: columns on both floors, none at the south-west corner)
        (json!({"cmd": "undo"}), "Detail 0-0 a"),
        (json!({"cmd": "units", "units": "ft"}), ""),
    ];
    for (cmd, keeps) in edits {
        let why = cmd.to_string();
        ok(&mut f.e, cmd);
        if !keeps.is_empty() {
            assert!(!stale(&f).contains(keeps), "{why} should keep {keeps}");
        } else {
            assert_eq!(stale(&f).len(), 70, "{why} reaches every drawing");
        }
        kept_equals_fresh(&mut f, &why);
    }
}

#[test]
fn a_component_edit_redraws_what_shows_its_placements() {
    let mut f = build(Size::small());
    let dir = scratch("component");
    export(&mut f.e, &dir);
    // editing the desk's definition changes every placement: both floors' plans
    let def = *f.e.model.definitions.keys().next().unwrap();
    let placed = f.e.model.instances.iter().find(|(_, i)| i.def == def).map(|(id, _)| *id).unwrap();
    ok(&mut f.e, json!({"cmd": "edit", "id": placed}));
    let faces: Vec<Id> = f.e.model.definitions[&def].model.faces.keys().copied().collect();
    ok(&mut f.e, json!({"cmd": "move", "ids": faces, "by": [0.05, 0.0, 0.0]}));
    ok(&mut f.e, json!({"cmd": "close"}));
    let s = stale(&f);
    assert!(s.contains("Floor plan 0") && s.contains("Floor plan 1"), "{s:?}");
    assert!(!s.contains("Section A1") || !s.contains("Schedule 1"));
    kept_equals_fresh(&mut f, "component edit");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn hatches_are_kept_per_region_and_reused_when_a_viewport_is_redrawn() {
    let mut f = build(Size::small());
    let dir = scratch("hatch");
    export(&mut f.e, &dir);
    let cold = f.e.drawings.stats();
    // (plans of the same floor drawn in the same place share their regions' hatches already)
    assert!(cold.hatch_made > 100, "{cold:?}");
    // moving one wall redraws the viewports that see it: their other hatched regions come from the cache
    ok(&mut f.e, json!({"cmd": "move", "ids": [f.wall], "by": [0.4, 0.0, 0.0]}));
    export(&mut f.e, &dir);
    let after = f.e.drawings.stats();
    let made = after.hatch_made - cold.hatch_made;
    let kept = after.hatch_kept - cold.hatch_kept;
    assert!(kept > 5 * made.max(1), "most hatches reused: {made} made, {kept} kept");
    kept_equals_fresh(&mut f, "hatches reused");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_pages_looked_at_last_are_redrawn_in_the_background() {
    let mut f = build(Size::small());
    let plan = f.sheets[6];
    let page = f.e.sheet_page(plan).unwrap();
    assert!(!page.items.is_empty());
    let drawn = f.e.drawings.stats().drawn;
    assert_eq!(drawn, 2, "a page view draws its own viewports only");
    ok(&mut f.e, json!({"cmd": "move", "ids": [f.wall], "by": [0.4, 0.0, 0.0]}));
    let st = ok(&mut f.e, json!({"cmd": "drafting_status"}));
    assert!(st["revision"].as_u64().unwrap() > 0);
    f.e.drawings.wait();
    let s = f.e.drawings.stats();
    assert_eq!(s.drawn_background, 2, "the page looked at was redrawn on the worker");
    assert!(!stale(&f).contains("Floor plan 0"));
    // so looking at it again draws nothing
    let again = f.e.sheet_page(plan).unwrap();
    assert_eq!(f.e.drawings.stats().drawn, s.drawn);
    assert!(again.items == f.e.sheet_page_fresh(plan).unwrap().items);
    // a change while the worker draws makes it throw its work away (nothing stale is ever kept)
    ok(&mut f.e, json!({"cmd": "move", "ids": [f.wall], "by": [0.1, 0.0, 0.0]}));
    ok(&mut f.e, json!({"cmd": "move", "ids": [f.wall], "by": [0.1, 0.0, 0.0]}));
    f.e.drawings.wait();
    kept_equals_fresh(&mut f, "changes while drawing in the background");
    // the status says what is kept, for anyone (read-only)
    let st = ok(&mut f.e, json!({"cmd": "drafting_status"}));
    assert_eq!(st["drawing"], false);
    assert!(st["kept"].as_u64().unwrap() >= 2);
    use sketchcraft::policy::{permits, Caller, Workspace};
    assert!(permits(&Caller::Plugin, "drafting_status"));
    assert!(permits(&Caller::Mcp(Workspace::at(std::env::temp_dir())), "drafting_status"));
}

#[test]
fn the_cache_lets_go_of_the_least_recently_used_past_its_budget() {
    let mut f = build(Size::small());
    let dir = scratch("budget");
    export(&mut f.e, &dir);
    let all = f.e.drawings.bytes();
    assert!(all > 0);
    f.e.drawings.set_budget(all / 4);
    assert!(f.e.drawings.bytes() <= all / 4);
    assert!(f.e.drawings.len() < 70);
    assert!(f.e.drawings.stats().evicted > 0);
    // what is left is still right
    f.e.drawings.set_budget(sketchcraft::draft::cache::DEFAULT_BUDGET);
    kept_equals_fresh(&mut f, "after evicting");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_tags_cut_style_redraws_what_shows_that_tag_and_keeps_the_rest() {
    // fill, hatch and outline: each a change to one tag's cut style. The columns stand in the rooms on both floors;
    // the corner details show none, so they stay kept.
    let mut f = build(Size::small());
    let dir = scratch("cutstyle");
    let changes = [
        json!({"cmd": "tag_style", "tag": "column", "cut": {"fill": "solid", "color": "#336699"}}),
        json!({"cmd": "tag_style", "tag": "column", "hatch": {"pattern": "cross", "scale": 2}}),
        json!({"cmd": "tag_style", "tag": "column", "cut": {"fill": "hatch", "outline_mm": 0.7}}),
    ];
    for c in changes {
        let why = c.to_string();
        export(&mut f.e, &dir);
        assert!(stale(&f).is_empty(), "everything kept before {why}");
        ok(&mut f.e, c);
        let s = stale(&f);
        for keep in ["Detail 0-0 a", "Detail 0-1 a", "Detail 1-0 b", "Detail 3-1 a"] {
            assert!(!s.contains(keep), "{why} keeps {keep}: {s:?}");
        }
        assert!(s.contains("Floor plan 0") && s.contains("Floor plan 1") && s.contains("Section A0"), "{why} redraws the plans and sections that show it: {s:?}");
        kept_equals_fresh(&mut f, &why);
    }
    // an intersection priority (#88): resolving it unions each tag's cut regions over the whole plane, so every plan
    // and section is redrawn (the corner details too); drawings with no cut that don't show columns stay kept
    export(&mut f.e, &dir);
    ok(&mut f.e, json!({"cmd": "tag_style", "tag": "column", "cut": {"priority": 5}}));
    let s = stale(&f);
    for redrawn in ["Floor plan 0", "Floor plan 1", "Detail 0-0 a", "Detail 3-1 b", "Section A0", "Section B2", "Key plan ground"] {
        assert!(s.contains(redrawn), "a priority redraws {redrawn}: {s:?}");
    }
    assert!(!s.contains("East bay 1:20"), "an elevation that shows no column is kept: {s:?}");
    kept_equals_fresh(&mut f, "column priority");
    // and while one is set, any change redraws the cuts (here: the slab raised over the walls it carries)
    export(&mut f.e, &dir);
    ok(&mut f.e, json!({"cmd": "tag_style", "tag": "slab", "cut": {"priority": 9}}));
    let s = stale(&f);
    assert!(s.contains("Floor plan 0") && s.contains("Detail 0-0 a"), "{s:?}");
    kept_equals_fresh(&mut f, "slab priority");
    ok(&mut f.e, json!({"cmd": "move", "ids": [f.wall], "by": [0.2, 0.0, 0.0]}));
    assert!(stale(&f).contains("Detail 3-1 b"), "a wall moved while priorities are set redraws every cut");
    kept_equals_fresh(&mut f, "wall moved with priorities");
    std::fs::remove_dir_all(&dir).ok();
}

// ---- memory and speed (the two budgets the first CI run missed) ------------------------------------------------

/// `items` with every packed run of lines opened into its paths, groups too: the display list as it was before
/// packing.
fn unpacked(items: &[sketchcraft::draft::ir::Prim]) -> Vec<sketchcraft::draft::ir::Prim> {
    use sketchcraft::draft::ir::{expanded, Group, Prim};
    expanded(items)
        .iter()
        .map(|p| match p {
            Prim::Group(g) => Prim::Group(Group { children: std::sync::Arc::new(unpacked(&g.children)), ..g.clone() }),
            other => other.clone(),
        })
        .collect()
}

/// (lines packed, paths), groups opened.
fn count_lines(items: &[sketchcraft::draft::ir::Prim]) -> (usize, usize) {
    use sketchcraft::draft::ir::Prim;
    let (mut packed, mut paths) = (0, 0);
    for p in items {
        match p {
            Prim::Lines(l) => packed += l.len(),
            Prim::Path(_) => paths += 1,
            Prim::Group(g) => {
                let (a, b) = count_lines(&g.children);
                packed += a;
                paths += b;
            }
            _ => {}
        }
    }
    (packed, paths)
}

#[test]
fn kept_drawings_hold_plain_lines_packed_and_write_the_same_files() {
    use sketchcraft::draft::{cache::bytes_of, dxf, ir, irjson, pdf, raster, svg};
    let mut f = build(Size { furniture_boxes: 8, furniture_per_floor: 24, dims: 40, texts: 4 });
    // a plan page (furniture below the cut, cut walls, hatches), an elevation and a section (hidden lines with their
    // provenance), an axonometric at 40%, a detail page
    for name in ["Floor plan 0", "South elevation", "Section A0", "Axonometric 0", "Details 4-0"] {
        let s = *f.e.model.sheets.iter().find(|(_, s)| s.name == name).unwrap().0;
        let page = f.e.sheet_page(s).unwrap();
        let (packed, paths) = count_lines(&page.items);
        // (the details' few lines are short runs between cut regions, left as paths)
        assert!(packed > 0 || name == "Details 4-0", "{name}: line work is packed ({packed} packed, {paths} paths)");
        // the same page with every line a path again: every file written from it is byte for byte the same
        let open = ir::Page { items: unpacked(&page.items), ..page.clone() };
        assert_eq!(count_lines(&open.items), (0, packed + paths));
        assert!(svg::page(&page) == svg::page(&open), "{name}: SVG");
        assert!(dxf::page(&page) == dxf::page(&open), "{name}: DXF");
        assert_eq!(irjson::page(&page, None), irjson::page(&open, None), "{name}: the editor's JSON");
        assert!(raster::page(&page, 600) == raster::page(&open, 600), "{name}: the picture");
        let doc = |p: &ir::Page| ir::Doc { title: "Permit set".into(), lang: String::new(), pages: vec![p.clone()] };
        assert!(pdf::write(&doc(&page)).unwrap() == pdf::write(&doc(&open)).unwrap(), "{name}: PDF");
        // walking a page sees the paths (what tests and the hatch warnings read)
        let mut walked = 0;
        page.walk(&mut |p| walked += usize::from(matches!(p, ir::Prim::Path(_))));
        assert_eq!(walked, packed + paths, "{name}: walk opens packed lines");
        // provenance survives packing: the elevation's and the section's lines still name their edges
        if name == "South elevation" || name == "Section A0" {
            let mut with_prov = 0;
            page.walk(&mut |p| with_prov += usize::from(matches!(p, ir::Prim::Path(pp) if pp.prov.is_some())));
            assert!(with_prov > 20, "{name}: {with_prov} lines with provenance");
        }
    }
    // what a kept drawing is counted at: a packed line about 34 bytes (its ends and a pen index; 46 with provenance),
    // where the same line as a path was about 240
    let plan = *f.e.model.sheets.iter().find(|(_, s)| s.name == "Floor plan 0").unwrap().0;
    let page = f.e.sheet_page(plan).unwrap();
    let (packed, _) = count_lines(&page.items);
    let (now, before) = (bytes_of(&page.items), bytes_of(&unpacked(&page.items)));
    assert!(packed > 1000, "{packed} lines");
    assert!(now * 2 < before, "packed {now} bytes, as paths {before}");
    assert!(f.e.drawings.bytes() > 0);
    kept_equals_fresh(&mut f, "packed");
}

#[test]
fn the_cover_only_ever_hides_what_the_rays_would() {
    // a hidden-line drawing with the cover (big triangles that certainly hide what is behind them, judged without a
    // ray) equals the drawing with every sample's ray cast: elevations, sections, axonometrics, close details, with
    // and without the hidden lines drawn dashed
    use glam::DVec3;
    use sketchcraft::draft::hidden::{Proj, View};
    use sketchcraft::draft::ir::{Pens, Prim};
    let f = build(Size { furniture_boxes: 8, furniture_per_floor: 24, dims: 0, texts: 0 });
    let m = &f.e.model;
    let pens = Pens::new(false);
    let mid = DVec3::new(perf_fixture::W / 2.0, perf_fixture::D / 2.0, 3.0);
    let iso = DVec3::new(-20.0, 30.0, -25.0).normalize();
    // (look, paper mm per metre, anchor, the section plane)
    type Section = Option<(DVec3, DVec3)>;
    let views: Vec<(DVec3, f64, DVec3, Section)> = vec![
        (DVec3::Y, 20.0, mid, None),
        (DVec3::NEG_X, 20.0, mid, None),
        (iso, 10.0, mid, None),
        (DVec3::new(1.0, 1.0, -0.8).normalize(), 10.0, mid, None),
        (DVec3::X, 20.0, DVec3::new(2.5, perf_fixture::D / 2.0, 3.0), Some((DVec3::new(2.5, 6.0, 0.0), DVec3::X))),
        (DVec3::Y, 20.0, DVec3::new(10.0, 3.0, 1.5), Some((DVec3::new(10.0, 3.0, 0.0), DVec3::Y))),
        (DVec3::Y, 200.0, DVec3::new(5.0, 1.0, 0.4), Some((DVec3::new(5.0, 0.8, 0.0), DVec3::Y))),
        (iso, 1000.0, DVec3::new(1.0, 1.0, 0.4), None),
    ];
    let mut hidden_spans = 0;
    for (i, (look, k, anchor, clip)) in views.into_iter().enumerate() {
        for beyond in [false, true] {
            let mk = |cover: bool| View { m, proj: Proj::new(anchor, look, k, [300.0, 200.0]), rect: [40.0, 40.0, 520.0, 320.0], clip, beyond, pens: &pens, inject_panic: false, cover };
            let (with, without) = (mk(true).lines().unwrap(), mk(false).lines().unwrap());
            // (from outside, the house's walls hide all of the closest detail)
            assert!(!with.is_empty() || (i == 7 && !beyond), "view {i}: something is drawn");
            assert!(with == without, "view {i} (beyond {beyond}): the cover changed the drawing ({} against {} lines)", with.len(), without.len());
            if beyond {
                hidden_spans += with.iter().filter(|p| matches!(p, Prim::Path(pp) if pp.pen.as_ref().is_some_and(|q| !q.dash.is_empty()))).count();
            }
        }
    }
    assert!(hidden_spans > 1000, "the views do hide things ({hidden_spans} hidden spans)");
}
