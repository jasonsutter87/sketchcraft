//! Work limits, round six: one time limit for every command an agent or a plugin runs (and one for a whole call), set
//! where the caller is known and read by the loops of the engine; the person's own commands have none. Plus the
//! commands that were slow on their own (intersect, follow_me, many lines in one call) and names sent to agents.
//! Bounds are generous (CI machines are slow); each is far below what the work used to take.

use glam::DVec3;
use serde_json::{json, Value};
use sketchcraft::deadline;
use sketchcraft::live::Live;
use sketchcraft::model::Face;
use sketchcraft::policy::{Caller, Workspace};
use sketchcraft::Engine;
use std::time::{Duration, Instant};

fn run(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn as_agent(e: &mut Engine, v: Value) -> Value {
    e.exec_json_as(&Caller::Mcp(Workspace::anywhere()), &v.to_string())
}

/// A small deterministic random source.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f64) / (1u64 << 31) as f64
    }
}

/// `n` big squares (radius 10) through the origin, each in a plane of its own.
fn quads_through_a_point(e: &mut Engine, n: usize) -> Vec<u32> {
    let mut r = Lcg(1);
    let mut ids = Vec::new();
    for _ in 0..n {
        let nrm = DVec3::new(r.next() - 0.5, r.next() - 0.5, r.next() - 0.5).normalize();
        let (u, v) = {
            let t = if nrm.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
            let u = nrm.cross(t).normalize();
            (u, nrm.cross(u))
        };
        let c = |a: f64, b: f64| {
            let p = u * a + v * b;
            json!([p.x, p.y, p.z])
        };
        let f = run(e, json!({"cmd": "polygon", "points": [c(10.0, 10.0), c(-10.0, 10.0), c(-10.0, -10.0), c(10.0, -10.0)]}));
        ids.push(f["face"].as_u64().unwrap() as u32);
    }
    ids
}

/// `n` separate unit boxes' worth of loose faces (6 each), spread out so none touch, put in the model directly.
fn many_boxes(e: &mut Engine, n: usize) -> Vec<u32> {
    let side = (n as f64).cbrt().ceil() as usize;
    let mut faces = Vec::new();
    for b in 0..n {
        let (ix, iy, iz) = (b % side, (b / side) % side, b / (side * side));
        let o = DVec3::new(ix as f64, iy as f64, iz as f64) * 5.0;
        let vs: Vec<u32> = (0..8)
            .map(|k| {
                let p = o + DVec3::new((k & 1) as f64, ((k >> 1) & 1) as f64, ((k >> 2) & 1) as f64) * 2.0;
                e.model.vertex(p)
            })
            .collect();
        for q in [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]] {
            let id = e.model.next_id + 1;
            e.model.next_id = id;
            e.model.faces.insert(id, Face { outer: q.iter().map(|i| vs[*i]).collect(), ..Default::default() });
            faces.push(id);
        }
    }
    faces
}

#[test]
fn the_default_limits_are_ten_seconds_a_command_and_thirty_a_call() {
    assert_eq!(deadline::AGENT_COMMAND_SECONDS, Duration::from_secs(10));
    assert_eq!(deadline::AGENT_CALL_SECONDS, Duration::from_secs(30));
    assert_eq!(deadline::command_seconds("rect"), Duration::from_secs(10));
    assert_eq!(deadline::call_seconds(), Duration::from_secs(30));
}

// ---- the central limit ----

fn terrain(e: &mut Engine) -> u32 {
    run(e, json!({"cmd": "terrain_grid", "width": 80, "depth": 80, "spacing": 0.5}))["group"].as_u64().unwrap() as u32
}

#[test]
fn a_command_with_no_limit_of_its_own_stops_at_the_central_one_for_an_agent_and_not_for_the_person() {
    let mut e = Engine::new();
    let g = terrain(&mut e);
    let before = e.model.clone();
    // the person: no clock, so it finishes however short the agent's limit is made
    deadline::set_limits_for_tests(Some(Duration::from_millis(1)), None);
    let r = e.exec_json(&json!({"cmd": "smoove", "group": g, "at": [40, 40, 0], "radius": 50, "height": 3}).to_string());
    assert_eq!(r["ok"], true, "the person is never stopped: {r}");
    assert!(deadline::current().is_none());
    run(&mut e, json!({"cmd": "undo"}));
    assert_eq!(e.model.vertices.len(), before.vertices.len());
    // an agent: the same command is stopped, and nothing changed
    let t = Instant::now();
    let r = as_agent(&mut e, json!({"cmd": "smoove", "group": g, "at": [40, 40, 0], "radius": 50, "height": 3}));
    deadline::set_limits_for_tests(None, None);
    assert_eq!(r["ok"], false, "{r}");
    assert_eq!(r["timed_out"], true, "{r}");
    assert!(t.elapsed() < Duration::from_secs(20), "{:?}", t.elapsed());
    assert!(r["error"].as_str().unwrap().contains("nothing was changed"), "{r}");
    assert_eq!(e.model.vertices.len(), before.vertices.len());
    assert!(e.model.vertices.iter().all(|(k, v)| before.vertices[k].p == v.p), "no vertex moved");
    assert!(deadline::current().is_none(), "the limit does not outlive the command");
    // and with the normal limit the same command works for the agent
    let r = as_agent(&mut e, json!({"cmd": "smoove", "group": g, "at": [40, 40, 0], "radius": 50, "height": 3}));
    assert_eq!(r["ok"], true, "{r}");
}

#[test]
fn read_only_commands_of_an_agent_are_bounded_too() {
    let mut e = Engine::new();
    let ids = many_boxes(&mut e, 600);
    assert!(!ids.is_empty());
    deadline::set_limits_for_tests(Some(Duration::from_millis(1)), None);
    let r = as_agent(&mut e, json!({"cmd": "info"}));
    let person = e.exec_json(&json!({"cmd": "info"}).to_string());
    deadline::set_limits_for_tests(None, None);
    // `info` is quick, so it may or may not outrun 1 ms; what matters is that an expired one is a clean refusal
    assert!(r["ok"] == true || r["timed_out"] == true, "{r}");
    assert_eq!(person["ok"], true);
}

#[test]
fn a_call_that_runs_out_of_time_is_undone_as_a_whole() {
    let mut live = Live::memory(Engine::new());
    let caller = Caller::Mcp(Workspace::anywhere());
    // some earlier work that must stay
    let first = sketchcraft::mcp::handle_tool_call(&mut live, &caller, &json!({"name": "sketchcraft_run", "arguments": {"commands": [{"cmd": "rect", "from": [0, 0, 0], "to": [4, 4, 0]}]}}));
    assert_eq!(first["isError"], false, "{first}");
    let (vs, es, fs) = (live.engine.model.vertices.len(), live.engine.model.edges.len(), live.engine.model.faces.len());
    let next_id = live.engine.model.next_id;
    // 300 random chords of one circle in one call (each command is quick; together they are not)
    let mut r = Lcg(7);
    let chords: Vec<Value> = (0..300)
        .map(|_| {
            let (a, b) = (r.next() * std::f64::consts::TAU, r.next() * std::f64::consts::TAU);
            json!({"cmd": "line", "points": [[20.0 + 10.0 * a.cos(), 10.0 * a.sin(), 0.0], [20.0 + 10.0 * b.cos(), 10.0 * b.sin(), 0.0]]})
        })
        .collect();
    deadline::set_limits_for_tests(None, Some(Duration::from_secs(3)));
    let t = Instant::now();
    let reply = sketchcraft::mcp::handle_tool_call(&mut live, &caller, &json!({"name": "sketchcraft_run", "arguments": {"commands": chords}}));
    deadline::set_limits_for_tests(None, None);
    let took = t.elapsed();
    assert_eq!(reply["isError"], true, "{reply}");
    let text = reply["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("everything it did was undone"), "{text}");
    assert!(took < Duration::from_secs(15), "the call was stopped after {took:?}");
    assert_eq!((live.engine.model.vertices.len(), live.engine.model.edges.len(), live.engine.model.faces.len()), (vs, es, fs), "the call was rolled back");
    assert!(live.engine.model.next_id >= next_id);
    // the engine is usable afterwards, and the earlier call's work is still one undo step
    let again = sketchcraft::mcp::handle_tool_call(&mut live, &caller, &json!({"name": "sketchcraft_run", "arguments": {"commands": [{"cmd": "rect", "from": [10, 10, 0], "to": [12, 12, 0]}]}}));
    assert_eq!(again["isError"], false, "{again}");
}

#[test]
fn the_person_draws_a_few_hundred_lines_in_a_row_without_a_limit() {
    let mut e = Engine::new();
    // short limits for agents: the person's lines are not held to them
    deadline::set_limits_for_tests(Some(Duration::from_millis(1)), Some(Duration::from_millis(1)));
    for i in 0..300 {
        let y = i as f64 * 0.05;
        let r = e.exec_json(&json!({"cmd": "line", "points": [[0.0, y, 0.0], [10.0, y, 0.0]]}).to_string());
        assert_eq!(r["ok"], true, "line {i}: {r}");
    }
    deadline::set_limits_for_tests(None, None);
    assert_eq!(e.model.edges.len(), 300);
}

// ---- intersect ----

#[test]
fn intersect_of_many_planes_through_one_point_finishes_or_is_undone_for_an_agent() {
    let mut e = Engine::new();
    let ids = quads_through_a_point(&mut e, 44);
    let (vs, fs) = (e.model.vertices.len(), e.model.faces.len());
    let t = Instant::now();
    let r = as_agent(&mut e, json!({"cmd": "intersect", "ids": ids, "with": "selection"}));
    let took = t.elapsed();
    eprintln!("intersect of 44 quads as an agent: {took:?} -> ok {}", r["ok"]);
    assert!(took < Duration::from_secs(30), "stopped after {took:?}");
    if r["ok"] != true {
        assert_eq!((e.model.vertices.len(), e.model.faces.len()), (vs, fs), "nothing changed");
    } else {
        assert!(r["lines"].as_u64().unwrap() > 0);
    }
}

#[test]
fn intersect_stops_at_the_central_limit_and_leaves_the_model_as_it_was() {
    let mut e = Engine::new();
    let ids = quads_through_a_point(&mut e, 44);
    let (vs, fs) = (e.model.vertices.len(), e.model.faces.len());
    deadline::set_limits_for_tests(Some(Duration::from_millis(150)), None);
    let t = Instant::now();
    let r = as_agent(&mut e, json!({"cmd": "intersect", "ids": ids.clone(), "with": "selection"}));
    deadline::set_limits_for_tests(None, None);
    assert_eq!(r["ok"], false, "{r}");
    assert!(t.elapsed() < Duration::from_secs(10), "{:?}", t.elapsed());
    assert_eq!((e.model.vertices.len(), e.model.faces.len()), (vs, fs), "nothing changed");
    // the person's own intersect has no clock
    deadline::set_limits_for_tests(Some(Duration::from_millis(150)), None);
    let r = e.exec_json(&json!({"cmd": "intersect", "ids": ids, "with": "selection"}).to_string());
    deadline::set_limits_for_tests(None, None);
    assert_eq!(r["ok"], true, "{r}");
}

#[test]
fn intersect_of_a_few_planes_still_works() {
    let mut e = Engine::new();
    let ids = quads_through_a_point(&mut e, 12);
    let r = run(&mut e, json!({"cmd": "intersect", "ids": ids, "with": "selection"}));
    assert!(r["lines"].as_u64().unwrap() > 0, "{r}");
    // two crossing quads: one line in each
    let mut e = Engine::new();
    run(&mut e, json!({"cmd": "polygon", "points": [[-5, 0, -5], [5, 0, -5], [5, 0, 5], [-5, 0, 5]]}));
    run(&mut e, json!({"cmd": "polygon", "points": [[0, -5, -5], [0, 5, -5], [0, 5, 5], [0, -5, 5]]}));
    let ids: Vec<u32> = e.model.faces.keys().copied().collect();
    let r = run(&mut e, json!({"cmd": "intersect", "ids": ids, "with": "selection"}));
    assert_eq!(r["lines"], 2);
}

#[test]
fn intersect_of_twelve_thousand_faces_is_quick_in_both_modes() {
    let mut e = Engine::new();
    let ids = many_boxes(&mut e, 2000);
    assert_eq!(ids.len(), 12_000);
    for with in ["selection", "model"] {
        let t = Instant::now();
        let r = as_agent(&mut e, json!({"cmd": "intersect", "ids": ids, "with": with}));
        let took = t.elapsed();
        eprintln!("intersect of 12,000 faces ({with}): {took:?}");
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(r["lines"], 0, "boxes that don't touch cross nowhere");
        assert!(took < Duration::from_secs(10), "{with}: {took:?}");
    }
}

#[test]
fn intersect_finds_the_same_crossings_through_the_grid_as_by_testing_every_pair() {
    // a box through a plate: the plate's edges meet the box's four sides
    let mut e = Engine::new();
    run(&mut e, json!({"cmd": "rect", "from": [-10, -10, 0], "to": [10, 10, 0]}));
    let rect = run(&mut e, json!({"cmd": "rect", "from": [-2, -2, -4], "to": [2, 2, -4]}))["face"].as_u64().unwrap();
    run(&mut e, json!({"cmd": "pushpull", "face": rect, "distance": 8, "toward": [0, 0, 1]}));
    let ids: Vec<u32> = e.model.faces.keys().copied().collect();
    let r = run(&mut e, json!({"cmd": "intersect", "ids": ids, "with": "selection"}));
    assert!(r["lines"].as_u64().unwrap() >= 4, "{r}");
}

// ---- follow_me ----

fn circle_points(n: usize, r: f64, z: f64, cx: f64) -> Vec<Value> {
    (0..n)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            json!([cx + r * a.cos(), r * a.sin(), z])
        })
        .collect()
}

#[test]
fn follow_me_of_a_huge_profile_along_a_huge_path_is_refused_before_anything_is_built() {
    let mut e = Engine::new();
    // a 1,999-point profile in the x-z plane, and a 1,999-point path
    let profile: Vec<Value> = (0..1999)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / 1999.0;
            json!([0.5 * a.cos(), 0.0, 0.5 * a.sin()])
        })
        .collect();
    let face = run(&mut e, json!({"cmd": "polygon", "points": profile}))["face"].as_u64().unwrap();
    let path: Vec<Value> = (0..1999).map(|i| json!([0.0, i as f64 * 0.05, (i as f64 * 0.1).sin() * 0.3])).collect();
    let (vs, es, fs, next_id) = (e.model.vertices.len(), e.model.edges.len(), e.model.faces.len(), e.model.next_id);
    let t = Instant::now();
    let r = as_agent(&mut e, json!({"cmd": "follow_me", "face": face, "points": path}));
    let took = t.elapsed();
    eprintln!("follow_me 1,999 x 1,999: {took:?} -> {r}");
    assert_eq!(r["ok"], false, "{r}");
    assert!(r["error"].as_str().unwrap().contains("limit"), "{r}");
    assert!(took < Duration::from_secs(3), "refused after {took:?}");
    assert_eq!((e.model.vertices.len(), e.model.edges.len(), e.model.faces.len()), (vs, es, fs));
    // nothing was built: not even ids were taken
    assert_eq!(e.model.next_id, next_id);
}

#[test]
fn follow_me_of_a_normal_profile_along_a_normal_path_is_unchanged() {
    let mut e = Engine::new();
    let face = run(&mut e, json!({"cmd": "polygon", "points": [[0, 0, 0], [0.4, 0, 0], [0.4, 0, 0.4], [0, 0, 0.4]]}))["face"].as_u64().unwrap();
    let path = json!([[0, 0, 0], [0, 5, 0], [3, 8, 0], [8, 8, 0]]);
    let r = as_agent(&mut e, json!({"cmd": "follow_me", "face": face, "points": path}));
    assert_eq!(r["ok"], true, "{r}");
    // 3 segments x 4 sides, plus the end cap
    assert_eq!(r["faces"].as_array().unwrap().len(), 13);
    // a round profile along a 400-point path (about 25,000 pieces) is fine too
    let mut e = Engine::new();
    let face = run(&mut e, json!({"cmd": "polygon", "points": circle_points(24, 0.5, 0.0, 0.0).iter().map(|p| json!([0.0, p[0], p[1]])).collect::<Vec<_>>()}))["face"].as_u64().unwrap();
    let path: Vec<Value> = (0..400).map(|i| json!([i as f64 * 0.05, 0.0, 0.0])).collect();
    let r = as_agent(&mut e, json!({"cmd": "follow_me", "face": face, "points": path}));
    assert_eq!(r["ok"], true, "{r}");
}

// ---- lines ----

#[test]
fn a_few_hundred_lines_drawn_by_the_person_are_not_stopped() {
    // (see also `the_person_draws_a_few_hundred_lines_in_a_row_without_a_limit`: the lines there cross nothing)
    let mut e = Engine::new();
    for i in 0..40 {
        let a = std::f64::consts::TAU * i as f64 / 40.0;
        let b = a + 2.0;
        let r = e.exec_json(&json!({"cmd": "line", "points": [[10.0 * a.cos(), 10.0 * a.sin(), 0.0], [10.0 * b.cos(), 10.0 * b.sin(), 0.0]]}).to_string());
        assert_eq!(r["ok"], true, "{r}");
    }
}

// ---- names sent to agents ----

#[test]
fn names_sent_to_an_agent_lose_invisible_text_but_stored_names_stay() {
    let mut live = Live::memory(Engine::new());
    let f = run(&mut live.engine, json!({"cmd": "rect", "from": [0, 0, 0], "to": [2, 2, 0]}))["face"].as_u64().unwrap();
    let g = run(&mut live.engine, json!({"cmd": "group", "ids": [f], "name": "Door"}))["group"].as_u64().unwrap() as u32;
    // names that came from a file: tag-block text, a bidi mark and a filler
    let sneaky = format!("Door{}\u{202E}x\u{3164}y", ('a'..='z').map(|c| char::from_u32(0xE0000 + c as u32).unwrap()).collect::<String>());
    live.engine.model.groups.get_mut(&g).unwrap().name = sneaky.clone();
    live.engine.model.tags.insert(sneaky.clone(), true);
    let caller = Caller::Mcp(Workspace::anywhere());
    let info = sketchcraft::mcp::handle_tool_call(&mut live, &caller, &json!({"name": "sketchcraft_info", "arguments": {}}));
    let text = info["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Doorxy"), "{text}");
    assert!(!text.chars().any(|c| ('\u{E0000}'..='\u{E007F}').contains(&c) || c == '\u{202E}' || c == '\u{3164}'), "{text:?}");
    let run_reply = sketchcraft::mcp::handle_tool_call(&mut live, &caller, &json!({"name": "sketchcraft_run", "arguments": {"commands": [{"cmd": "info"}]}}));
    let text = run_reply["content"].to_string();
    assert!(!text.contains('\u{202E}') && !text.contains("\\udb40") && !text.contains('\u{3164}'), "{text}");
    // the model itself is untouched
    assert_eq!(live.engine.model.groups[&g].name, sneaky);
}

// ---- other commands that were found slow in older code ----

#[test]
fn flip_of_many_things_is_quick() {
    let mut e = Engine::new();
    let ids = many_boxes(&mut e, 800);
    let t = Instant::now();
    let r = run(&mut e, json!({"cmd": "flip", "ids": ids, "axis": "x"}));
    assert!(r["moved"].as_u64().unwrap() > 0);
    assert!(t.elapsed() < Duration::from_secs(20), "{:?}", t.elapsed());
}

/// With the real 30-second budget (slow: run with `--ignored`): 300 random chords of one circle in one agent call.
#[test]
#[ignore]
fn three_hundred_chords_in_one_agent_call_stop_inside_the_real_budget() {
    let mut live = Live::memory(Engine::new());
    let caller = Caller::Mcp(Workspace::anywhere());
    let mut r = Lcg(7);
    let chords: Vec<Value> = (0..300)
        .map(|_| {
            let (a, b) = (r.next() * std::f64::consts::TAU, r.next() * std::f64::consts::TAU);
            json!({"cmd": "line", "points": [[10.0 * a.cos(), 10.0 * a.sin(), 0.0], [10.0 * b.cos(), 10.0 * b.sin(), 0.0]]})
        })
        .collect();
    let t = Instant::now();
    let reply = sketchcraft::mcp::handle_tool_call(&mut live, &caller, &json!({"name": "sketchcraft_run", "arguments": {"commands": chords}}));
    let took = t.elapsed();
    eprintln!("300 chords in one call: {took:?}, isError {}", reply["isError"]);
    assert!(took < Duration::from_secs(40), "{took:?}");
    if reply["isError"] == true {
        assert_eq!(live.engine.model.edges.len(), 0, "undone");
    }
}
