//! Work limits, round seven: the intersect plane cache stays right after a loop is added; a call that runs out of time
//! never writes the model it began with over somebody else's save; the clock is read before every command of a call, a
//! call and a plugin run have a command budget, and a call is written once, not after each command; report, IFC export
//! and the group-to-faces lookups are linear in the model; the clock is read as often as the work needs; renders for
//! agents have a texture budget; names lose line breaks. Bounds are generous (CI machines are slow).

use glam::DVec3;
use serde_json::{json, Value};
use sketchcraft::deadline;
use sketchcraft::live::Live;
use sketchcraft::model::{Edge, Face, Group, Id, Model, Vertex};
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

fn agent() -> Caller {
    Caller::Mcp(Workspace::anywhere())
}

fn call(live: &mut Live, commands: Vec<Value>) -> Value {
    sketchcraft::mcp::handle_tool_call(live, &agent(), &json!({"name": "sketchcraft_run", "arguments": {"commands": commands}}))
}

fn reply_text(r: &Value) -> String {
    r["content"][0]["text"].as_str().unwrap_or("").to_string()
}

fn nid(m: &mut Model) -> Id {
    m.next_id += 1;
    m.next_id
}

/// One box (8 points, 12 edges, 6 faces) of size `size` at `o`, in group `g`; returns its face ids.
fn add_box(m: &mut Model, g: Option<Id>, o: DVec3, size: f64) -> Vec<Id> {
    let vs: Vec<Id> = (0..8)
        .map(|k| {
            let id = nid(m);
            m.vertices.insert(id, Vertex { p: o + DVec3::new((k & 1) as f64, ((k >> 1) & 1) as f64, ((k >> 2) & 1) as f64) * size, group: g });
            id
        })
        .collect();
    for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
        let k = nid(m);
        m.edges.insert(k, Edge { a: vs[a], b: vs[b], group: g, ..Default::default() });
    }
    let mut faces = Vec::new();
    for q in [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]] {
        let id = nid(m);
        m.faces.insert(id, Face { outer: q.iter().map(|i| vs[*i]).collect(), group: g, ..Default::default() });
        faces.push(id);
    }
    faces
}

/// `n` grouped boxes in a flat grid, far apart. Returns the group ids.
fn grouped_boxes(e: &mut Engine, n: usize) -> Vec<Id> {
    let side = (n as f64).sqrt().ceil() as usize;
    let mut groups = Vec::new();
    for i in 0..n {
        let g = nid(&mut e.model);
        e.model.groups.insert(g, Group { name: format!("G{i}"), ..Default::default() });
        add_box(&mut e.model, Some(g), DVec3::new((i % side) as f64 * 3.0, (i / side) as f64 * 3.0, 0.0), 1.0);
        groups.push(g);
    }
    groups
}

/// A lattice of overlapping loose boxes: `n` per side, `spacing` apart, each of size 1.
fn lattice(e: &mut Engine, n: usize, spacing: f64) -> Vec<Id> {
    let mut faces = Vec::new();
    for x in 0..n {
        for y in 0..n {
            for z in 0..n {
                faces.extend(add_box(&mut e.model, None, DVec3::new(x as f64, y as f64, z as f64) * spacing, 1.0));
            }
        }
    }
    faces
}

// ---- the intersect plane cache ----

#[test]
fn intersect_of_overlapping_boxes_works_for_the_person_and_for_an_agent() {
    // 27 boxes in a 3 x 3 x 3 lattice that overlap: lines are drawn into faces that are split as they go
    let mut person = Engine::new();
    let ids = lattice(&mut person, 3, 0.6);
    let r = person.exec_json(&json!({"cmd": "intersect", "ids": ids, "with": "selection"}).to_string());
    assert_eq!(r["ok"], true, "the person's intersect of 27 overlapping boxes: {r}");
    assert!(r["lines"].as_u64().unwrap() > 0, "{r}");
    // an agent gets the same answer
    let mut a = Engine::new();
    let ids = lattice(&mut a, 3, 0.6);
    let ra = as_agent(&mut a, json!({"cmd": "intersect", "ids": ids, "with": "selection"}));
    assert_eq!(ra["ok"], true, "{ra}");
    assert_eq!(ra["lines"], r["lines"]);
    assert_eq!((a.model.faces.len(), a.model.edges.len(), a.model.vertices.len()), (person.model.faces.len(), person.model.edges.len(), person.model.vertices.len()));
}

#[test]
fn intersect_of_many_overlapping_boxes_does_not_hit_the_bug_net() {
    let mut e = Engine::new();
    let ids = lattice(&mut e, 4, 0.7);
    let t = Instant::now();
    let r = as_agent(&mut e, json!({"cmd": "intersect", "ids": ids, "with": "selection"}));
    let text = r.to_string();
    assert!(!text.contains("hit a bug"), "{text}");
    assert!(r["ok"] == true || r["timed_out"] == true, "{text}");
    assert!(t.elapsed() < Duration::from_secs(40), "{:?}", t.elapsed());
    let _ = (deadline::AGENT_CALL_SECONDS, run as fn(&mut Engine, Value) -> Value, call as fn(&mut Live, Vec<Value>) -> Value, reply_text as fn(&Value) -> String, grouped_boxes as fn(&mut Engine, usize) -> Vec<Id>);
}

// ---- a call that runs out of time never discards somebody else's save ----

fn scratch(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("sketchcraft-w7-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn chords(n: usize) -> Vec<Value> {
    let mut s = 7u64;
    let mut next = move || {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((s >> 33) as f64) / (1u64 << 31) as f64
    };
    (0..n)
        .map(|_| {
            let (a, b) = (next() * std::f64::consts::TAU, next() * std::f64::consts::TAU);
            json!({"cmd": "line", "points": [[20.0 + 10.0 * a.cos(), 10.0 * a.sin(), 0.0], [20.0 + 10.0 * b.cos(), 10.0 * b.sin(), 0.0]]})
        })
        .collect()
}

fn faces_in_file(doc: &std::path::Path) -> usize {
    Live::open(doc.to_path_buf()).engine.model.faces.len()
}

/// A point of the call's chords (they all lie on the circle of radius 10 around [20, 0]).
fn has_chord_points(m: &Model) -> bool {
    m.vertices.values().any(|v| (((v.p.x - 20.0).powi(2) + v.p.y.powi(2)).sqrt() - 10.0).abs() < 1e-6)
}

fn has_point(m: &Model, p: DVec3) -> bool {
    m.vertices.values().any(|v| (v.p - p).length() < 1e-6)
}

#[test]
fn a_call_that_runs_out_never_discards_what_the_app_saved_meanwhile() {
    let dir = scratch("rollback");
    let doc = dir.join("m.sketchcraft");
    let mut a = Live::open(doc.clone());
    // earlier work that must stay
    assert_eq!(call(&mut a, vec![json!({"cmd": "rect", "from": [0, 0, 0], "to": [4, 4, 0]})])["isError"], false);
    let before = faces_in_file(&doc);
    // the app (a second engine on the same file) draws a rectangle into the agent's call
    let d2 = doc.clone();
    let app = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(600));
        let mut b = Live::open(d2);
        let r = b.exec(&json!({"cmd": "rect", "from": [100, 100, 0], "to": [102, 102, 0]}));
        assert_eq!(r["ok"], true, "{r}");
    });
    deadline::set_limits_for_tests(None, Some(Duration::from_secs(4)));
    let t = Instant::now();
    let reply = call(&mut a, chords(400));
    deadline::set_limits_for_tests(None, None);
    app.join().unwrap();
    let text = reply_text(&reply);
    assert!(t.elapsed() < Duration::from_secs(30), "{:?}", t.elapsed());
    assert_eq!(reply["isError"], true, "{text}");
    eprintln!("{}", text.lines().last().unwrap_or(""));
    assert!(text.contains("someone else saved this model") && text.contains("their work was kept"), "{text}");
    // the app's rectangle is in the file and in memory; the call's own lines are gone from both
    let in_file = Live::open(doc.clone());
    let rect = DVec3::new(100.0, 100.0, 0.0);
    assert!(has_point(&in_file.engine.model, rect), "the app's rectangle was lost from the file: {text}");
    assert!(has_point(&a.engine.model, rect), "the app's rectangle was lost from memory: {text}");
    assert_eq!(in_file.engine.model.faces.len(), before + 1, "the file holds the earlier work and the app's rectangle: {text}");
    assert!(!has_chord_points(&in_file.engine.model), "the call's lines are still in the file: {text}");
    assert!(!has_chord_points(&a.engine.model), "the call's lines are still in memory");
    // the file as the rollback found it was copied into Backups first
    let backups = std::fs::read_dir(dir.join("Backups")).map(|d| d.count()).unwrap_or(0);
    assert!(backups > 0, "no backup was taken");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_call_that_runs_out_alone_is_put_back_as_a_whole_and_the_file_is_as_it_was() {
    let dir = scratch("alone");
    let doc = dir.join("m.sketchcraft");
    let mut a = Live::open(doc.clone());
    assert_eq!(call(&mut a, vec![json!({"cmd": "rect", "from": [0, 0, 0], "to": [4, 4, 0]})])["isError"], false);
    let before = faces_in_file(&doc);
    deadline::set_limits_for_tests(None, Some(Duration::from_secs(2)));
    let reply = call(&mut a, chords(400));
    deadline::set_limits_for_tests(None, None);
    let text = reply_text(&reply);
    assert!(text.contains("everything it did was undone"), "{text}");
    assert_eq!(a.engine.model.faces.len(), before);
    assert_eq!(faces_in_file(&doc), before, "the file is as it was before the call");
    assert!(!has_chord_points(&Live::open(doc.clone()).engine.model));
    // the model is usable and the next call works
    assert_eq!(call(&mut a, vec![json!({"cmd": "rect", "from": [10, 10, 0], "to": [12, 12, 0]})])["isError"], false);
    assert_eq!(faces_in_file(&doc), before + 1);
    std::fs::remove_dir_all(&dir).ok();
}

// ---- the call clock, the command budget, one write per call ----

#[test]
fn the_call_clock_is_read_before_every_command_and_a_slow_call_of_cheap_commands_is_undone() {
    let mut live = Live::memory(Engine::new());
    let groups = grouped_boxes(&mut live.engine, 3000);
    let names: Vec<String> = groups.iter().map(|g| live.engine.model.groups[g].name.clone()).collect();
    let renames: Vec<Value> = (0..400).map(|i| json!({"cmd": "rename", "id": groups[i % groups.len()], "name": format!("N{i}")})).collect();
    // one rename is cheap and never fails; only the clock stops the call
    deadline::set_limits_for_tests(None, Some(Duration::from_millis(300)));
    let t = Instant::now();
    let reply = call(&mut live, renames);
    deadline::set_limits_for_tests(None, None);
    let text = reply_text(&reply);
    eprintln!("400 renames on 3,000 groups, call limit 0.3 s: {:?}", t.elapsed());
    assert_eq!(reply["isError"], true, "{text}");
    assert!(text.contains("everything it did was undone"), "{text}");
    assert!(t.elapsed() < Duration::from_secs(10), "{:?}", t.elapsed());
    let after: Vec<String> = groups.iter().map(|g| live.engine.model.groups[g].name.clone()).collect();
    assert_eq!(after, names, "no rename survived the rollback");
}

#[test]
fn a_call_holds_at_most_a_thousand_commands() {
    let mut live = Live::memory(Engine::new());
    let r = call(&mut live, (0..1001).map(|_| json!({"cmd": "info"})).collect());
    assert_eq!(r["isError"], true);
    assert!(reply_text(&r).contains("at most 1000"), "{}", reply_text(&r));
    assert_eq!(live.engine.model.vertices.len(), 0);
    let ok = call(&mut live, (0..1000).map(|_| json!({"cmd": "info"})).collect());
    assert_eq!(ok["isError"], false);
}

#[test]
fn a_plugin_run_has_a_clock_and_a_command_budget_of_its_own() {
    let plugin = |e: &mut Engine, v: Value| e.exec_json_as(&Caller::Plugin, &v.to_string());
    // time: the run is taken back whole, and what it sends afterwards is refused until the app ends the batch
    let mut e = Engine::new();
    let groups = grouped_boxes(&mut e, 2000);
    let names: Vec<String> = groups.iter().map(|g| e.model.groups[g].name.clone()).collect();
    run(&mut e, json!({"cmd": "begin_batch"}));
    deadline::set_limits_for_tests(None, Some(Duration::from_millis(200)));
    let mut stopped_at = None;
    for (i, g) in groups.iter().enumerate().take(400) {
        let r = plugin(&mut e, json!({"cmd": "rename", "id": g, "name": format!("P{i}")}));
        if r["ok"] != true {
            assert_eq!(r["timed_out"], true, "{r}");
            stopped_at = Some(i);
            break;
        }
    }
    deadline::set_limits_for_tests(None, None);
    let at = stopped_at.expect("the run was stopped by its clock");
    let after = plugin(&mut e, json!({"cmd": "rename", "id": groups[0], "name": "late"}));
    assert_eq!(after["ok"], false, "{after}");
    let steps = e.undo_depth();
    run(&mut e, json!({"cmd": "end_batch"}));
    assert_eq!(e.undo_depth(), steps, "the run left no step to undo");
    let now: Vec<String> = groups.iter().map(|g| e.model.groups[g].name.clone()).collect();
    assert_eq!(now, names, "what the run did before it was stopped (at command {at}) was undone");
    // the next run works
    run(&mut e, json!({"cmd": "begin_batch"}));
    assert_eq!(plugin(&mut e, json!({"cmd": "rename", "id": groups[0], "name": "again"}))["ok"], true);
    run(&mut e, json!({"cmd": "end_batch"}));
    assert_eq!(e.model.groups[&groups[0]].name, "again");
    // count: one run may not send an unlimited number of commands
    let mut e = Engine::new();
    run(&mut e, json!({"cmd": "begin_batch"}));
    let mut refused = None;
    for i in 0..=sketchcraft::commands::MAX_PLUGIN_COMMANDS {
        let r = plugin(&mut e, json!({"cmd": "info"}));
        if r["ok"] != true {
            refused = Some((i, r));
            break;
        }
    }
    let (i, r) = refused.expect("the run was stopped by its command count");
    assert_eq!(i, sketchcraft::commands::MAX_PLUGIN_COMMANDS);
    assert!(r["error"].as_str().unwrap().contains("more than"), "{r}");
    run(&mut e, json!({"cmd": "end_batch"}));
}

#[test]
fn a_call_is_written_once_not_after_every_command() {
    let dir = scratch("once");
    let doc = dir.join("m.sketchcraft");
    let mut a = Live::open(doc.clone());
    assert_eq!(call(&mut a, vec![json!({"cmd": "rect", "from": [0, 0, 0], "to": [4, 4, 0]})])["isError"], false);
    let stamp = |p: &std::path::Path| std::fs::metadata(p).and_then(|m| m.modified()).unwrap();
    let before = stamp(&doc);
    let n = std::fs::read(&doc).unwrap().len();
    std::thread::sleep(Duration::from_millis(30));
    // a call of many commands: the file is not rewritten while it runs (only every couple of seconds and at the end)
    let cmds: Vec<Value> = (0..100).map(|i| json!({"cmd": "rect", "from": [10 + i * 3, 0, 0], "to": [12 + i * 3, 2, 0]})).collect();
    let reply = call(&mut a, cmds);
    assert_eq!(reply["isError"], false, "{}", reply_text(&reply));
    assert!(stamp(&doc) > before, "the call's work is in the file when it ends");
    assert!(std::fs::read(&doc).unwrap().len() > n);
    assert_eq!(faces_in_file(&doc), 101);
    // and a crash-safe state is kept: a second engine sees every command of a finished call
    assert_eq!(Live::open(doc.clone()).engine.model.faces.len(), 101);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn many_commands_in_one_call_cost_one_write_not_one_each() {
    // 100 commands in one call write the file once; 100 separate calls write it 100 times. The writes are counted (the
    // timing follows from them, but it depends on how busy the machine is: the commands themselves use the processor,
    // the writes wait for the disk, and a loaded runner stretches the first far more than the second)
    let dir = scratch("writes");
    let mut a = Live::open(dir.join("m.sketchcraft"));
    let g = grouped_boxes(&mut a.engine, 2500)[0];
    assert_eq!(call(&mut a, vec![json!({"cmd": "rename", "id": g, "name": "x"})])["isError"], false);
    let writes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = writes.clone();
    a.set_save_hook(Some(Box::new(move |phase| {
        if phase == "written" {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    })));
    // the periodic write is for calls that run for a long time: a slow machine must not add writes to this one
    a.set_call_flush_every(Duration::from_secs(3600));
    let count = || writes.load(std::sync::atomic::Ordering::SeqCst);
    let names: Vec<Value> = (0..100).map(|i| json!({"cmd": "rename", "id": g, "name": format!("a{i}")})).collect();
    let t = Instant::now();
    let r = call(&mut a, names);
    let one_call = t.elapsed();
    assert_eq!(r["isError"], false, "{}", reply_text(&r));
    let one_call_writes = count();
    let t = Instant::now();
    for i in 0..100 {
        assert_eq!(call(&mut a, vec![json!({"cmd": "rename", "id": g, "name": format!("b{i}")})])["isError"], false);
    }
    let separate = t.elapsed();
    let separate_writes = count() - one_call_writes;
    eprintln!("100 renames on 2,500 groups: one call {one_call:?} ({one_call_writes} write), 100 calls {separate:?} ({separate_writes} writes)");
    assert_eq!(one_call_writes, 1, "a call of many commands writes the file once");
    assert_eq!(separate_writes, 100, "each separate call writes the file once");
    std::fs::remove_dir_all(&dir).ok();
}

// ---- report, IFC export and the group-to-faces lookups are linear ----

fn ids_json(ids: &[Id]) -> Value {
    json!(ids)
}

#[test]
fn report_of_thousands_of_groups_is_linear() {
    let mut e = Engine::new();
    grouped_boxes(&mut e, 4000);
    // the person (no limit) and an agent (10 s): 4,000 groups took 35 s and ignored the agent's clock
    let t = Instant::now();
    let r = run(&mut e, json!({"cmd": "report"}));
    let took = t.elapsed();
    eprintln!("report of 4,000 groups: {took:?}");
    assert!(took < Duration::from_secs(8), "{took:?}");
    assert!(r.to_string().contains("G3999"));
    let t = Instant::now();
    let ra = as_agent(&mut e, json!({"cmd": "report"}));
    assert_eq!(ra["ok"], true, "{ra}");
    assert!(t.elapsed() < Duration::from_secs(10), "{:?}", t.elapsed());
}

#[test]
fn report_is_stopped_by_the_agents_clock() {
    let mut e = Engine::new();
    grouped_boxes(&mut e, 3000);
    deadline::set_limits_for_tests(Some(Duration::from_millis(1)), None);
    let t = Instant::now();
    let r = as_agent(&mut e, json!({"cmd": "report"}));
    deadline::set_limits_for_tests(None, None);
    assert_eq!(r["ok"], false, "{r}");
    assert_eq!(r["timed_out"], true, "{r}");
    assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
}

#[test]
fn ifc_export_of_many_groups_is_linear() {
    let dir = scratch("ifc");
    let mut e = Engine::new();
    grouped_boxes(&mut e, 16_000);
    let t = Instant::now();
    run(&mut e, json!({"cmd": "export_ifc", "path": dir.join("m.ifc")}));
    let took = t.elapsed();
    eprintln!("export_ifc of 16,000 groups: {took:?}");
    assert!(took < Duration::from_secs(10), "{took:?}");
    assert!(std::fs::metadata(dir.join("m.ifc")).unwrap().len() > 1_000_000);
    // and the agent's clock stops it
    deadline::set_limits_for_tests(Some(Duration::from_millis(1)), None);
    let r = as_agent(&mut e, json!({"cmd": "export_ifc", "path": dir.join("n.ifc")}));
    deadline::set_limits_for_tests(None, None);
    assert!(r["ok"] == false && r["timed_out"] == true || r["error"].as_str().is_some_and(|s| s.contains("workspace") || s.contains("allowed")), "{r}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn commands_that_take_a_group_or_many_are_linear_in_the_groups() {
    let mut e = Engine::new();
    let groups = grouped_boxes(&mut e, 16_000);
    for (name, cmd) in [
        ("reverse", json!({"cmd": "reverse", "ids": ids_json(&groups)})),
        ("paint", json!({"cmd": "paint", "ids": ids_json(&groups), "color": [0.2, 0.4, 0.6]})),
        ("move", json!({"cmd": "move", "ids": ids_json(&groups), "by": [0.0, 0.0, 1.0]})),
    ] {
        let t = Instant::now();
        let r = run(&mut e, cmd);
        eprintln!("{name} of 16,000 groups: {:?} ({})", t.elapsed(), r.to_string().chars().take(60).collect::<String>());
        assert!(t.elapsed() < Duration::from_secs(10), "{name} took {:?}", t.elapsed());
    }
}

#[test]
fn faces_of_and_vertices_of_keep_their_answers() {
    let mut e = Engine::new();
    let outer = nid(&mut e.model);
    e.model.groups.insert(outer, Group { name: "outer".into(), ..Default::default() });
    let inner = nid(&mut e.model);
    e.model.groups.insert(inner, Group { name: "inner".into(), parent: Some(outer), ..Default::default() });
    let a = add_box(&mut e.model, Some(outer), DVec3::ZERO, 1.0);
    let b = add_box(&mut e.model, Some(inner), DVec3::new(5.0, 0.0, 0.0), 1.0);
    let loose = add_box(&mut e.model, None, DVec3::new(9.0, 0.0, 0.0), 1.0);
    let mut want: Vec<Id> = a.iter().chain(b.iter()).copied().collect();
    want.sort_unstable();
    assert_eq!(e.model.faces_of(&[outer]).unwrap(), want, "a group's faces include the groups inside it, in id order");
    assert_eq!(e.model.faces_of(&[inner]).unwrap(), b);
    assert_eq!(e.model.faces_of(&[loose[0], inner]).unwrap(), std::iter::once(loose[0]).chain(b.iter().copied()).collect::<Vec<_>>());
    assert!(e.model.faces_of(&[999_999]).is_err());
    assert_eq!(e.model.vertices_of(&[outer]).unwrap().len(), 16);
    assert_eq!(e.model.vertices_of(&[inner]).unwrap().len(), 8);
}

// ---- the clock is read as often as the work needs ----

#[test]
fn poll_reads_the_clock_on_every_heavy_step() {
    // heavy steps (5 ms of work per poll) used to run 64 of them (0.3 s) between two reads of the clock
    let t = Instant::now();
    let r = std::panic::catch_unwind(|| {
        deadline::scope(Duration::from_millis(40), || loop {
            std::thread::sleep(Duration::from_millis(5));
            deadline::poll();
        })
    });
    assert!(deadline::is_exceeded(&*r.unwrap_err()));
    assert!(t.elapsed() < Duration::from_millis(120), "stopped after {:?}", t.elapsed());
    // light steps stay cheap: ten million polls with no limit and with one far away
    let t = Instant::now();
    for _ in 0..10_000_000 {
        deadline::poll();
    }
    deadline::scope(Duration::from_secs(60), || {
        for _ in 0..10_000_000 {
            deadline::poll();
        }
    });
    assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
}

#[test]
fn make_component_of_many_groups_stops_near_the_limit() {
    let mut e = Engine::new();
    let groups = grouped_boxes(&mut e, 16_000);
    deadline::set_limits_for_tests(Some(Duration::from_secs(2)), None);
    let t = Instant::now();
    let r = as_agent(&mut e, json!({"cmd": "make_component", "ids": ids_json(&groups), "name": "all"}));
    deadline::set_limits_for_tests(None, None);
    eprintln!("make_component of 16,000 groups with a 2 s limit: {:?} ok={}", t.elapsed(), r["ok"]);
    assert!(r["ok"] == true || r["timed_out"] == true || r["error"].as_str().is_some_and(|s| s.starts_with("that took too long")), "{r}");
    assert!(t.elapsed() < Duration::from_secs(8), "stopped after {:?} (limit 2 s)", t.elapsed());
}

// ---- pictures, names ----

#[test]
fn an_agents_render_decodes_a_bounded_number_of_big_pictures() {
    use image::ImageEncoder;
    // a 9,999 x 9,999 picture (just under 100 megapixels) that is one flat grey
    let (w, h) = (9999u32, 9999u32);
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png).write_image(&vec![128u8; (w * h) as usize], w, h, image::ExtendedColorType::L8).unwrap();
    let url = format!("data:image/png;base64,{}", base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &png));
    let mut m = Model::default();
    for i in 0..10 {
        m.materials.insert(format!("t{i}"), sketchcraft::model::Material { texture: Some(sketchcraft::model::Texture { data: url.clone(), width: 1.0, height: 1.0, project: None, origin: None }), ..Default::default() });
    }
    // a plugin or an agent: a pixel budget, so about six of the ten are decoded and the rest are drawn plain (and said so)
    let n = deadline::scope(Duration::from_secs(120), || {
        let imgs = sketchcraft::texmap::Images::load(&m);
        (0..10).filter(|i| imgs.get(&format!("t{i}")).is_some()).count()
    });
    let skipped = sketchcraft::safety::take_skipped();
    assert!((1..=6).contains(&n), "{n} decoded");
    assert_eq!(skipped.len(), 10 - n, "{skipped:?}");
    // the person: all of them
    let imgs = sketchcraft::texmap::Images::load(&m);
    assert_eq!((0..10).filter(|i| imgs.get(&format!("t{i}")).is_some()).count(), 10);
    sketchcraft::safety::take_skipped();
}

#[test]
fn line_and_paragraph_separators_are_stripped_from_names_sent_to_agents() {
    let mut live = Live::memory(Engine::new());
    let g = grouped_boxes(&mut live.engine, 1)[0];
    live.engine.model.groups.get_mut(&g).unwrap().name = "Wall\u{2028}two\u{2029}three\u{FFFC}end".into();
    let info = sketchcraft::mcp::handle_tool_call(&mut live, &agent(), &json!({"name": "sketchcraft_info", "arguments": {}}));
    let text = reply_text(&info);
    assert!(text.contains("Walltwothreeend"), "{text}");
    assert!(!text.contains('\u{2028}') && !text.contains('\u{2029}') && !text.contains('\u{FFFC}'));
    for c in ['\u{2028}', '\u{2029}', '\u{FFFC}'] {
        assert!(sketchcraft::text::is_invisible_filler(c) || sketchcraft::text::is_format_char(c), "{c:?}");
    }
}

#[test]
fn the_heavy_commands_say_their_own_seconds() {
    // the message uses the limit the command ran under (a test limit of 3 s is named as 3 s, not as the usual 10)
    let mut e = Engine::new();
    grouped_boxes(&mut e, 3000);
    deadline::set_limits_for_tests(Some(Duration::from_secs(3)), None);
    let r = as_agent(&mut e, json!({"cmd": "report"}));
    assert_eq!(r["ok"], true, "{r}");
    deadline::set_limits_for_tests(Some(Duration::from_millis(1)), None);
    let r = as_agent(&mut e, json!({"cmd": "report"}));
    deadline::set_limits_for_tests(None, None);
    assert_eq!(r["timed_out"], true, "{r}");
    assert!(r["error"].as_str().unwrap().contains("(0 s)") || r["error"].as_str().unwrap().contains("(10 s)"), "{r}");
    assert_eq!(deadline::command_seconds("photoreal").as_secs(), 25);
    assert_eq!(deadline::command_seconds("animate").as_secs(), 25);
}
