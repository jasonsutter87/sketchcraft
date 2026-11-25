//! Hardening, round eight: the call rollback and the save protocol are correct, not just usually right.
//!
//! R1: a save somebody was told succeeded is never lost from the live file.
//! R2: an aborted agent call's changes never stay in the live file or the live model, however big the call and however
//!     small the undo budget.
//! R3: when R1 and R2 meet on the same data, the other writer's data wins, the call's version is set aside, and both
//!     sides are told the truth.
//! R4: a crash at any point leaves a loadable file with every save somebody was told succeeded.
//!
//! Two or three `Live` engines share one file (the app, an agent, a second agent). A save hook lets a second writer act at
//! exactly the moment a save is between "serialised" and "written", so nothing here depends on timing.

use glam::DVec3;
use serde_json::{json, Value};
use sketchcraft::live::Live;
use sketchcraft::model::{Edge, Face, Group, Id, Model, Vertex};
use sketchcraft::policy::{Caller, Workspace};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn agent() -> Caller {
    Caller::Mcp(Workspace::anywhere())
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("sketchcraft-w8-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn nid(m: &mut Model) -> Id {
    m.next_id += 1;
    m.next_id
}

fn add_box(m: &mut Model, g: Id, o: DVec3) {
    let vs: Vec<Id> = (0..8)
        .map(|k| {
            let id = nid(m);
            m.vertices.insert(id, Vertex { p: o + DVec3::new((k & 1) as f64, ((k >> 1) & 1) as f64, ((k >> 2) & 1) as f64), group: Some(g) });
            id
        })
        .collect();
    for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
        let k = nid(m);
        m.edges.insert(k, Edge { a: vs[a], b: vs[b], group: Some(g), ..Default::default() });
    }
    for q in [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]] {
        let id = nid(m);
        m.faces.insert(id, Face { outer: q.iter().map(|i| vs[*i]).collect(), group: Some(g), ..Default::default() });
    }
}

struct World {
    dir: PathBuf,
    doc: PathBuf,
    groups: Vec<Id>,
    orig: Model,
}

/// A model with `n` grouped boxes, saved.
fn world(name: &str, n: usize) -> World {
    let dir = scratch(name);
    let doc = dir.join("m.sketchcraft");
    let mut a = Live::open(doc.clone());
    let side = (n as f64).sqrt().ceil() as usize;
    let mut groups = vec![];
    for i in 0..n {
        let g = nid(&mut a.engine.model);
        a.engine.model.groups.insert(g, Group { name: format!("G{i}"), ..Default::default() });
        add_box(&mut a.engine.model, g, DVec3::new((i % side) as f64 * 3.0, (i / side) as f64 * 3.0, 0.0));
        groups.push(g);
    }
    a.save(true).unwrap();
    let orig = a.engine.model.clone();
    World { dir, doc, groups, orig }
}

fn rect_cmd() -> Value {
    json!({"cmd": "rect", "from": [100, 100, 0], "to": [102, 102, 0]})
}

fn has_rect(m: &Model) -> bool {
    m.vertices.values().any(|v| (v.p - DVec3::new(100.0, 100.0, 0.0)).length() < 1e-6)
}

fn rect_count(m: &Model) -> usize {
    m.vertices.values().filter(|v| (v.p - DVec3::new(100.0, 100.0, 0.0)).length() < 1e-6).count()
}

/// The app draws the rectangle (a second engine on the same file) and is told it worked.
fn app_draws_rect(doc: &Path) {
    let mut b = Live::open(doc.to_path_buf());
    let r = b.exec(&rect_cmd());
    assert_eq!(r["ok"], true, "{r}");
    assert!(r.get("save_error").is_none(), "{r}");
}

fn move_group(live: &mut Live, g: Id, dz: f64) {
    let r = live.exec_as(&agent(), &json!({"cmd": "move", "ids": [g], "by": [0.0, 0.0, dz]}));
    assert_eq!(r["ok"], true, "{r}");
}

fn file_model(doc: &Path) -> Model {
    let bytes = std::fs::read(doc).expect("the file is there");
    serde_json::from_slice(&bytes).expect("the file loads")
}

/// How many points of the original model are not where they were.
fn changed(m: &Model, orig: &Model) -> usize {
    orig.vertices.iter().filter(|(id, v)| m.vertices.get(id).is_none_or(|w| (w.p - v.p).length() > 1e-9)).count()
}

fn unsaved_files(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.to_string_lossy().contains(".unsaved-")).collect()
}

/// An agent engine that writes after every command.
fn agent_engine(w: &World) -> Live {
    let mut a = Live::open(w.doc.clone());
    a.set_call_flush_every(Duration::ZERO);
    a
}

fn assert_clean(w: &World, a: &Live, what: &str) {
    let f = file_model(&w.doc);
    assert_eq!(changed(&f, &w.orig), 0, "{what}: points of the call stay in the file");
    assert_eq!(changed(&a.engine.model, &w.orig), 0, "{what}: points of the call stay in memory");
}

// ---- nobody else writes ----

#[test]
fn alone_the_call_is_put_back_exactly() {
    let w = world("alone", 12);
    let mut a = agent_engine(&w);
    a.begin_call();
    for g in &w.groups {
        move_group(&mut a, *g, 5.0);
    }
    assert!(changed(&file_model(&w.doc), &w.orig) > 0, "the call was written while it ran");
    assert!(a.abort_call().is_none());
    assert_clean(&w, &a, "alone");
    std::fs::remove_dir_all(&w.dir).ok();
}

// ---- the app saves at different moments (R1, R2) ----

#[test]
fn app_saves_before_the_first_flush() {
    let w = world("early", 12);
    let mut a = Live::open(w.doc.clone());
    a.begin_call();
    move_group(&mut a, w.groups[0], 5.0);
    app_draws_rect(&w.doc);
    move_group(&mut a, w.groups[1], 5.0);
    move_group(&mut a, w.groups[2], 5.0);
    let note = a.abort_call();
    let f = file_model(&w.doc);
    assert!(has_rect(&f) && has_rect(&a.engine.model), "the app's save is kept");
    assert_clean(&w, &a, "early");
    let _ = note;
    std::fs::remove_dir_all(&w.dir).ok();
}

#[test]
fn app_saves_mid_call_after_flushes_and_at_the_end() {
    let w = world("mid", 12);
    let mut a = agent_engine(&w);
    a.begin_call();
    move_group(&mut a, w.groups[0], 5.0);
    move_group(&mut a, w.groups[1], 5.0);
    app_draws_rect(&w.doc);
    move_group(&mut a, w.groups[2], 5.0);
    move_group(&mut a, w.groups[3], 5.0);
    let note = a.abort_call().expect("somebody else saved: the agent is told");
    assert!(!note.contains("NOT"), "a full rollback happened: {note}");
    assert!(has_rect(&file_model(&w.doc)) && has_rect(&a.engine.model));
    assert_clean(&w, &a, "mid");
    // the app saves again at the end (after the rollback): everything is kept
    let mut b = Live::open(w.doc.clone());
    assert_eq!(b.exec(&json!({"cmd": "rect", "from": [200, 200, 0], "to": [202, 202, 0]}))["ok"], true);
    assert_eq!(changed(&file_model(&w.doc), &w.orig), 0);
    std::fs::remove_dir_all(&w.dir).ok();
}

#[test]
fn a_big_call_with_a_tiny_undo_budget_leaves_nothing_of_the_call() {
    let w = world("budget", 60);
    let mut a = agent_engine(&w);
    a.engine.undo_budget = 1;
    a.begin_call();
    for (i, g) in w.groups.iter().enumerate() {
        move_group(&mut a, *g, 5.0);
        if i == 20 {
            app_draws_rect(&w.doc);
        }
    }
    assert!(a.engine.undo_depth() < 5, "the undo list really was cut down ({})", a.engine.undo_depth());
    let note = a.abort_call().expect("told");
    assert!(!note.contains("NOT"), "{note}");
    assert!(has_rect(&file_model(&w.doc)));
    assert_clean(&w, &a, "budget");
    std::fs::remove_dir_all(&w.dir).ok();
}

// ---- the rollback window (R0310-49) ----

/// A hook that runs `act` once, the first time a save reaches `phase`.
fn once_at(phase: &'static str, act: impl FnMut() + Send + 'static) -> Box<dyn FnMut(&str) + Send> {
    let mut act = Some(act);
    Box::new(move |p: &str| {
        if p == phase {
            if let Some(mut f) = act.take() {
                f();
            }
        }
    })
}

#[test]
fn app_saves_while_the_rollback_is_being_written() {
    let w = world("window", 12);
    let mut a = agent_engine(&w);
    a.begin_call();
    for g in &w.groups[..6] {
        move_group(&mut a, *g, 5.0);
    }
    let doc = w.doc.clone();
    a.set_save_hook(Some(once_at("serialized", move || app_draws_rect(&doc))));
    let note = a.abort_call().expect("somebody saved in the window: the agent is told");
    a.set_save_hook(None);
    assert!(!note.contains("NOT"), "{note}");
    assert_eq!(rect_count(&file_model(&w.doc)), 1, "the app's rectangle is in the file");
    assert_clean(&w, &a, "window");
    std::fs::remove_dir_all(&w.dir).ok();
}

#[test]
fn app_saves_while_a_flush_is_being_written() {
    let w = world("flush-window", 12);
    let mut a = Live::open(w.doc.clone());
    a.set_call_flush_every(Duration::ZERO);
    a.begin_call();
    move_group(&mut a, w.groups[0], 5.0);
    let doc = w.doc.clone();
    a.set_save_hook(Some(once_at("serialized", move || app_draws_rect(&doc))));
    move_group(&mut a, w.groups[1], 5.0);
    a.set_save_hook(None);
    let note = a.finish_call().expect("the agent is told");
    assert!(note.contains("someone else saved"), "{note}");
    assert!(has_rect(&file_model(&w.doc)), "the app's save is in the file");
    assert!(has_rect(&a.engine.model));
    assert!(!unsaved_files(&w.dir).is_empty(), "the agent's pending work was set aside");
    let n = a.notice.clone().unwrap_or_default();
    assert!(n.contains("AI agent") && !n.contains("Your changes were kept"), "{n}");
    std::fs::remove_dir_all(&w.dir).ok();
}

#[test]
fn a_save_in_the_person_window_is_never_overwritten_and_is_run_again() {
    let w = world("person-window", 4);
    let mut p = Live::open(w.doc.clone());
    let doc = w.doc.clone();
    p.set_save_hook(Some(once_at("serialized", move || app_draws_rect(&doc))));
    let r = p.exec(&json!({"cmd": "rect", "from": [300, 300, 0], "to": [302, 302, 0]}));
    p.set_save_hook(None);
    assert_eq!(r["ok"], true, "{r}");
    let f = file_model(&w.doc);
    assert!(has_rect(&f), "the other writer's rectangle survives");
    assert!(f.vertices.values().any(|v| (v.p - DVec3::new(300.0, 300.0, 0.0)).length() < 1e-6), "the person's rectangle was run again on their version");
    // a plain save over a file that moved is refused, never written
    let mut stale = Live::open(w.doc.clone());
    app_draws_rect_second(&w.doc);
    let before = std::fs::read(&w.doc).unwrap();
    assert!(stale.save(false).is_err());
    assert_eq!(std::fs::read(&w.doc).unwrap(), before);
    std::fs::remove_dir_all(&w.dir).ok();
}

fn app_draws_rect_second(doc: &Path) {
    let mut b = Live::open(doc.to_path_buf());
    assert_eq!(b.exec(&json!({"cmd": "rect", "from": [400, 400, 0], "to": [402, 402, 0]}))["ok"], true);
}

// ---- three writers, undo and redo, conflicts ----

#[test]
fn three_writers_keep_everything_and_the_call_goes() {
    let w = world("three", 12);
    let mut a = agent_engine(&w);
    let mut a2 = Live::open(w.doc.clone());
    a.begin_call();
    move_group(&mut a, w.groups[0], 5.0);
    // a second agent draws a rectangle
    assert_eq!(a2.exec_as(&agent(), &rect_cmd())["ok"], true);
    move_group(&mut a, w.groups[1], 5.0);
    // the app draws another
    let mut b = Live::open(w.doc.clone());
    assert_eq!(b.exec(&json!({"cmd": "rect", "from": [200, 200, 0], "to": [202, 202, 0]}))["ok"], true);
    move_group(&mut a, w.groups[2], 5.0);
    a.abort_call().expect("told");
    let f = file_model(&w.doc);
    assert!(has_rect(&f));
    assert!(f.vertices.values().any(|v| (v.p - DVec3::new(200.0, 200.0, 0.0)).length() < 1e-6));
    assert_clean(&w, &a, "three");
    std::fs::remove_dir_all(&w.dir).ok();
}

#[test]
fn the_apps_undo_and_redo_work_while_an_agent_call_runs() {
    let w = world("undo", 12);
    let mut b = Live::open(w.doc.clone());
    assert_eq!(b.exec(&rect_cmd())["ok"], true);
    let mut a = agent_engine(&w);
    a.begin_call();
    move_group(&mut a, w.groups[0], 5.0);
    let r = b.exec(&json!({"cmd": "undo"}));
    assert_eq!(r["ok"], true, "the app's first undo after an agent flush works: {r}");
    assert!(!has_rect(&b.engine.model));
    move_group(&mut a, w.groups[1], 5.0);
    let r = b.exec(&json!({"cmd": "redo"}));
    assert_eq!(r["ok"], true, "redo works too: {r}");
    assert!(has_rect(&file_model(&w.doc)));
    move_group(&mut a, w.groups[2], 5.0);
    a.abort_call();
    assert!(has_rect(&file_model(&w.doc)));
    assert_clean(&w, &a, "undo");
    std::fs::remove_dir_all(&w.dir).ok();
}

#[test]
fn what_both_changed_stays_the_other_writers_and_the_call_is_set_aside() {
    let w = world("conflict", 12);
    let mut a = agent_engine(&w);
    a.begin_call();
    move_group(&mut a, w.groups[0], 5.0);
    move_group(&mut a, w.groups[1], 5.0);
    // the app moves the same group the call moved
    let mut b = Live::open(w.doc.clone());
    let r = b.exec(&json!({"cmd": "move", "ids": [w.groups[0]], "by": [0.0, 0.0, 2.0]}));
    assert_eq!(r["ok"], true, "{r}");
    move_group(&mut a, w.groups[2], 5.0);
    let note = a.abort_call().expect("told");
    assert!(note.contains("also changed by someone else"), "{note}");
    assert!(!note.contains("Everything the call did"), "no claim of a full rollback: {note}");
    let f = file_model(&w.doc);
    // the second and third groups are back; the first stays where the app put it (original + 5 + 2)
    let g0: Vec<Id> = w.orig.vertices.iter().filter(|(_, v)| v.group == Some(w.groups[0])).map(|(i, _)| *i).collect();
    let (id0, v0) = (g0[0], w.orig.vertices[&g0[0]].p);
    assert!((f.vertices[&id0].p.z - (v0.z + 7.0)).abs() < 1e-9, "the other writer's version is kept");
    for g in &w.groups[1..] {
        for (id, v) in w.orig.vertices.iter().filter(|(_, v)| v.group == Some(*g)) {
            assert!((f.vertices[id].p - v.p).length() < 1e-9);
        }
    }
    let side = unsaved_files(&w.dir);
    assert!(!side.is_empty(), "the call's version is set aside");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&side[0]).unwrap().permissions().mode() & 0o777, 0o600);
    }
    let n = a.notice.clone().unwrap_or_default();
    assert!(n.contains("AI agent"), "{n}");
    std::fs::remove_dir_all(&w.dir).ok();
}

// ---- the file is replaced from outside ----

#[test]
fn another_valid_model_replacing_the_file_is_kept() {
    let w = world("valid", 6);
    let mut a = agent_engine(&w);
    a.begin_call();
    move_group(&mut a, w.groups[0], 5.0);
    let other = w.dir.join("other.sketchcraft");
    let mut o = Live::open(other.clone());
    assert_eq!(o.exec(&rect_cmd())["ok"], true);
    std::fs::copy(&other, &w.doc).unwrap();
    a.abort_call();
    let f = file_model(&w.doc);
    assert!(has_rect(&f) && f.groups.is_empty(), "their model is kept as it is");
    assert!(file_model(&w.doc).vertices.len() == o.engine.model.vertices.len());
    std::fs::remove_dir_all(&w.dir).ok();
}

#[test]
fn a_garbage_file_is_set_aside_and_the_model_written_back_without_the_call() {
    let w = world("garbage", 6);
    let mut a = agent_engine(&w);
    a.begin_call();
    move_group(&mut a, w.groups[0], 5.0);
    std::fs::write(&w.doc, b"not a model at all").unwrap();
    a.abort_call();
    assert_clean(&w, &a, "garbage");
    let damaged = std::fs::read_dir(&w.dir).unwrap().flatten().any(|e| e.file_name().to_string_lossy().contains(".damaged-"));
    assert!(damaged, "the unreadable file was kept aside");
    std::fs::remove_dir_all(&w.dir).ok();
}

#[test]
fn a_deleted_file_is_written_again_without_the_call() {
    let w = world("deleted", 6);
    let mut a = agent_engine(&w);
    a.begin_call();
    move_group(&mut a, w.groups[0], 5.0);
    std::fs::remove_file(&w.doc).unwrap();
    a.abort_call();
    assert_clean(&w, &a, "deleted");
    std::fs::remove_dir_all(&w.dir).ok();
}

#[test]
fn a_batch_open_across_the_timeout_keeps_the_apps_save() {
    let w = world("batch", 6);
    let mut a = Live::open(w.doc.clone());
    assert_eq!(a.exec_as(&agent(), &json!({"cmd": "begin_batch"}))["ok"], true);
    a.begin_call();
    move_group(&mut a, w.groups[0], 5.0);
    app_draws_rect(&w.doc);
    a.abort_call();
    let r = a.exec_as(&agent(), &json!({"cmd": "end_batch"}));
    // the batch's own work is set aside (the file moved under it) and the agent is told
    assert!(r["ok"] == true || r["error"].as_str().is_some_and(|e| e.contains("set aside")), "{r}");
    let f = file_model(&w.doc);
    assert!(has_rect(&f), "the app's save is kept");
    assert_eq!(changed(&f, &w.orig), 0);
    std::fs::remove_dir_all(&w.dir).ok();
}

// ---- crashes (R4) ----

#[test]
fn at_every_step_of_a_call_the_file_loads_and_holds_every_save_told_ok() {
    let w = world("crash", 12);
    app_draws_rect(&w.doc);
    let mut a = agent_engine(&w);
    let seen = Arc::new(AtomicUsize::new(0));
    let bad = Arc::new(Mutex::new(Vec::<String>::new()));
    let (doc, s2, b2) = (w.doc.clone(), seen.clone(), bad.clone());
    a.set_save_hook(Some(Box::new(move |phase: &str| {
        s2.fetch_add(1, Ordering::SeqCst);
        // a crash right here would leave exactly this file
        match std::fs::read(&doc).ok().and_then(|b| serde_json::from_slice::<Model>(&b).ok()) {
            Some(m) if has_rect(&m) => {}
            Some(_) => b2.lock().unwrap().push(format!("{phase}: the rectangle is missing")),
            None => b2.lock().unwrap().push(format!("{phase}: the file doesn't load")),
        }
    })));
    a.begin_call();
    for g in &w.groups[..6] {
        move_group(&mut a, *g, 5.0);
    }
    // a second writer saves in the middle too
    let mut b = Live::open(w.doc.clone());
    assert_eq!(b.exec(&json!({"cmd": "rect", "from": [200, 200, 0], "to": [202, 202, 0]}))["ok"], true);
    for g in &w.groups[6..9] {
        move_group(&mut a, *g, 5.0);
    }
    a.abort_call();
    a.set_save_hook(None);
    assert!(seen.load(Ordering::SeqCst) >= 6, "the hook ran at the save steps");
    assert!(bad.lock().unwrap().is_empty(), "{:?}", bad.lock().unwrap());
    // the file was backed up before the call first wrote to it
    let pre = std::fs::read_dir(w.dir.join("Backups")).map(|d| d.flatten().filter(|e| e.file_name().to_string_lossy().contains("-pre-big-a")).count()).unwrap_or(0);
    assert!(pre > 0, "no backup of the file as it was when the call began writing");
    // and a crash (the program just stops) after a flush leaves a loadable file with the app's rectangle
    let mut c = agent_engine(&w);
    c.begin_call();
    move_group(&mut c, w.groups[0], 5.0);
    drop(c);
    let f = file_model(&w.doc);
    assert!(has_rect(&f));
    std::fs::remove_dir_all(&w.dir).ok();
}

// ---- the lock ----

#[test]
fn the_lock_file_is_held_across_the_check_and_the_rename() {
    let w = world("lock", 2);
    let mut a = Live::open(w.doc.clone());
    let doc = w.doc.clone();
    let held = Arc::new(AtomicUsize::new(0));
    let h2 = held.clone();
    a.set_save_hook(Some(Box::new(move |phase: &str| {
        if phase == "locked" {
            // the lock is ours: a second program can't take it right now
            let lock = doc.with_file_name(format!(".{}.lock", doc.file_name().unwrap().to_string_lossy()));
            let f = std::fs::OpenOptions::new().read(true).write(true).open(&lock).expect("the lock file exists");
            if matches!(f.try_lock(), Err(std::fs::TryLockError::WouldBlock)) {
                h2.fetch_add(1, Ordering::SeqCst);
            }
        }
    })));
    assert_eq!(a.exec(&rect_cmd())["ok"], true);
    assert_eq!(held.load(Ordering::SeqCst), 1);
    std::fs::remove_dir_all(&w.dir).ok();
}

// ---- vertex colors of thousands of groups is linear ----

#[test]
fn vertex_colors_of_thousands_of_groups_is_linear() {
    let mut e = sketchcraft::Engine::new();
    let mut groups = vec![];
    for i in 0..4000 {
        let g = nid(&mut e.model);
        e.model.groups.insert(g, Group { name: format!("G{i}"), ..Default::default() });
        add_box(&mut e.model, g, DVec3::new((i % 64) as f64 * 3.0, (i / 64) as f64 * 3.0, 0.0));
        groups.push(g);
    }
    let t = std::time::Instant::now();
    let r = e.exec_json(&json!({"cmd": "vertex_colors", "faces": groups}).to_string());
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["colors"].as_object().map(|o| o.len()), Some(24_000));
    assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
}
