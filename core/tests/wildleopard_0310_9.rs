//! Hardening, round nine: the save protocol holds when work can't be set aside, when the rollback is raced all the time, and when
//! the lock file or the model file is odd.
//!
//! The same rules as round eight (R1: a save somebody was told succeeded is never lost; R2: an aborted call leaves nothing in the file or
//! the model; R3: the other writer's data wins and both sides are told the truth), now with failures injected on purpose
//! (`set_aside_fault`, a save hook that writes on every plan, a lock that is held).

use glam::DVec3;
use serde_json::{json, Value};
use sketchcraft::live::Live;
use sketchcraft::model::{Edge, Face, Group, Id, Model, Vertex};
use sketchcraft::policy::{Caller, Workspace};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

fn agent() -> Caller {
    Caller::Mcp(Workspace::anywhere())
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("sketchcraft-w9-{name}-{}", std::process::id()));
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
            m.vertices.insert(
                id,
                Vertex {
                    p: o + DVec3::new((k & 1) as f64, ((k >> 1) & 1) as f64, ((k >> 2) & 1) as f64),
                    group: Some(g),
                },
            );
            id
        })
        .collect();
    for (a, b) in [
        (0, 1),
        (2, 3),
        (4, 5),
        (6, 7),
        (0, 2),
        (1, 3),
        (4, 6),
        (5, 7),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ] {
        let k = nid(m);
        m.edges.insert(
            k,
            Edge {
                a: vs[a],
                b: vs[b],
                group: Some(g),
                ..Default::default()
            },
        );
    }
    for q in [
        [0, 1, 3, 2],
        [4, 6, 7, 5],
        [0, 4, 5, 1],
        [2, 3, 7, 6],
        [0, 2, 6, 4],
        [1, 5, 7, 3],
    ] {
        let id = nid(m);
        m.faces.insert(
            id,
            Face {
                outer: q.iter().map(|i| vs[*i]).collect(),
                group: Some(g),
                ..Default::default()
            },
        );
    }
}

struct World {
    dir: PathBuf,
    doc: PathBuf,
    groups: Vec<Id>,
    orig: Model,
}

/// A model with `n` grouped boxes, saved.
fn world_named(name: &str, docname: &str, n: usize) -> World {
    let dir = scratch(name);
    let doc = dir.join(docname);
    let mut a = Live::open(doc.clone());
    let side = (n as f64).sqrt().ceil() as usize;
    let mut groups = vec![];
    for i in 0..n {
        let g = nid(&mut a.engine.model);
        a.engine.model.groups.insert(
            g,
            Group {
                name: format!("G{i}"),
                ..Default::default()
            },
        );
        add_box(
            &mut a.engine.model,
            g,
            DVec3::new((i % side) as f64 * 3.0, (i / side) as f64 * 3.0, 0.0),
        );
        groups.push(g);
    }
    a.save(true).unwrap();
    let orig = a.engine.model.clone();
    World {
        dir,
        doc,
        groups,
        orig,
    }
}

fn rect_cmd() -> Value {
    json!({"cmd": "rect", "from": [100, 100, 0], "to": [102, 102, 0]})
}

fn has_rect(m: &Model) -> bool {
    m.vertices
        .values()
        .any(|v| (v.p - DVec3::new(100.0, 100.0, 0.0)).length() < 1e-6)
}

fn rect_count(m: &Model) -> usize {
    m.vertices
        .values()
        .filter(|v| (v.p - DVec3::new(100.0, 100.0, 0.0)).length() < 1e-6)
        .count()
}

/// The app draws the rectangle (a second engine on the same file) and is told it worked.
fn app_draws_rect(doc: &Path) {
    let mut b = Live::open(doc.to_path_buf());
    let r = b.exec(&rect_cmd());
    assert_eq!(r["ok"], true, "{r}");
    assert!(r.get("save_error").is_none(), "{r}");
}

fn move_group(live: &mut Live, g: Id, dz: f64) {
    let r = live.exec_as(
        &agent(),
        &json!({"cmd": "move", "ids": [g], "by": [0.0, 0.0, dz]}),
    );
    assert_eq!(r["ok"], true, "{r}");
}

fn file_model(doc: &Path) -> Model {
    let bytes = std::fs::read(doc).expect("the file is there");
    serde_json::from_slice(&bytes).expect("the file loads")
}

/// How many points of the original model are not where they were.
fn changed(m: &Model, orig: &Model) -> usize {
    orig.vertices
        .iter()
        .filter(|(id, v)| {
            m.vertices
                .get(id)
                .is_none_or(|w| (w.p - v.p).length() > 1e-9)
        })
        .count()
}

fn unsaved_files(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().contains(".unsaved-"))
        .collect()
}

/// An agent engine that writes after every command.
fn agent_engine(w: &World) -> Live {
    let mut a = Live::open(w.doc.clone());
    a.set_call_flush_every(Duration::ZERO);
    a
}

#[allow(dead_code)]
fn assert_clean(w: &World, a: &Live, what: &str) {
    let f = file_model(&w.doc);
    assert_eq!(
        changed(&f, &w.orig),
        0,
        "{what}: points of the call stay in the file"
    );
    assert_eq!(
        changed(&a.engine.model, &w.orig),
        0,
        "{what}: points of the call stay in memory"
    );
}

// ---- nobody else writes ----

fn world(name: &str, n: usize) -> World {
    world_named(name, "m.sketchcraft", n)
}

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

fn mode(p: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).unwrap().permissions().mode() & 0o777
}

/// The agent writes once (group 0); then the app saves while the agent's next command (group 1) is being written, so that
/// command's work is waiting to be set aside. `before`: runs just before that command (a test breaks the set-aside there).
fn agent_meets_app_save(w: &World, a: &mut Live, before: impl FnOnce(&mut Live)) {
    a.begin_call();
    move_group(a, w.groups[0], 5.0);
    before(a);
    let doc = w.doc.clone();
    a.set_save_hook(Some(once_at("serialized", move || app_draws_rect(&doc))));
    move_group(a, w.groups[1], 5.0);
    a.set_save_hook(None);
}

// ---- R0310-52: the person's acknowledged save is never overwritten when the agent's work can't be set aside ----

#[test]
fn when_the_set_aside_fails_the_apps_save_stays_and_the_agent_is_told_the_truth() {
    let w = world("aside-fails", 12);
    let mut a = agent_engine(&w);
    agent_meets_app_save(&w, &mut a, |a| a.set_aside_fault(true));
    move_group(&mut a, w.groups[2], 5.0);
    move_group(&mut a, w.groups[3], 5.0);
    let f = file_model(&w.doc);
    assert_eq!(rect_count(&f), 1, "the app's save is still in the file");
    assert!(
        unsaved_files(&w.dir).is_empty(),
        "nothing could be set aside"
    );
    // the agent's later commands did not overwrite it either
    assert_eq!(
        changed(&f, &w.orig),
        8,
        "only what the agent wrote before the app's save (group 0) is in the file"
    );
    let note = a.finish_call().expect("the agent is told");
    assert!(
        note.contains("NOT") && note.contains("could NOT be set aside"),
        "{note}"
    );
    assert!(
        !note.contains("was set aside (the person was told)"),
        "{note}"
    );
    assert_eq!(
        rect_count(&file_model(&w.doc)),
        1,
        "still the app's file after the call ended"
    );
    let n = a.notice.clone().unwrap_or_default();
    assert!(
        n.contains("couldn't be set aside"),
        "the person is told: {n}"
    );
    // the disk works again: the next look at the file sets the work aside and takes theirs
    a.set_aside_fault(false);
    assert!(a.sync());
    assert!(
        !unsaved_files(&w.dir).is_empty(),
        "the agent's work is now set aside"
    );
    assert!(has_rect(&a.engine.model));
    assert_eq!(rect_count(&file_model(&w.doc)), 1);
    std::fs::remove_dir_all(&w.dir).ok();
}

#[test]
fn an_aborted_call_with_a_failed_set_aside_still_keeps_the_apps_save() {
    let w = world("aside-fails-abort", 12);
    let mut a = agent_engine(&w);
    agent_meets_app_save(&w, &mut a, |a| a.set_aside_fault(true));
    move_group(&mut a, w.groups[2], 5.0);
    a.abort_call();
    let f = file_model(&w.doc);
    assert_eq!(rect_count(&f), 1);
    assert_eq!(
        changed(&f, &w.orig),
        0,
        "the rollback took the call's earlier write back"
    );
    assert_eq!(changed(&a.engine.model, &w.orig), 0);
    std::fs::remove_dir_all(&w.dir).ok();
}

#[test]
fn planted_links_named_like_the_set_aside_file_do_not_stop_it() {
    let w = world("planted", 12);
    let mut a = agent_engine(&w);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut planted = vec![];
    for s in t.saturating_sub(1)..=t + 3 {
        let l = w.dir.join(format!("m.unsaved-{s}.sketchcraft"));
        std::os::unix::fs::symlink(w.dir.join("nowhere"), &l).unwrap();
        planted.push(l);
    }
    let w2 = &w;
    agent_meets_app_save(w2, &mut a, |_| {});
    let note = a.finish_call().expect("told");
    assert!(note.contains("someone else saved"), "{note}");
    assert!(
        !w.dir.join("nowhere").exists(),
        "the links were never followed"
    );
    for l in &planted {
        assert!(std::fs::symlink_metadata(l)
            .unwrap()
            .file_type()
            .is_symlink());
    }
    let real: Vec<PathBuf> = unsaved_files(&w.dir)
        .into_iter()
        .filter(|p| std::fs::symlink_metadata(p).unwrap().file_type().is_file())
        .collect();
    assert_eq!(real.len(), 1, "the work was set aside under another name");
    assert_eq!(mode(&real[0]), 0o600);
    assert_eq!(rect_count(&file_model(&w.doc)), 1);
    std::fs::remove_dir_all(&w.dir).ok();
}

#[test]
fn a_very_long_model_name_can_still_set_work_aside() {
    let name = format!("{}.sketchcraft", "n".repeat(213));
    let w = world_named("long", &name, 12);
    let mut a = agent_engine(&w);
    agent_meets_app_save(&w, &mut a, |_| {});
    let note = a.finish_call().expect("told");
    assert!(note.contains("someone else saved"), "{note}");
    assert_eq!(
        unsaved_files(&w.dir).len(),
        1,
        "the work was set aside under a shortened name"
    );
    assert_eq!(rect_count(&file_model(&w.doc)), 1);
    std::fs::remove_dir_all(&w.dir).ok();
}

#[test]
fn a_plugin_run_whose_work_cannot_be_set_aside_does_not_overwrite_the_apps_save() {
    let w = world("batch-aside", 12);
    let mut a = Live::open(w.doc.clone());
    assert_eq!(
        a.exec_as(&agent(), &json!({"cmd": "begin_batch"}))["ok"],
        true
    );
    move_group(&mut a, w.groups[0], 5.0);
    app_draws_rect(&w.doc);
    a.set_aside_fault(true);
    let r = a.exec_as(&agent(), &json!({"cmd": "end_batch"}));
    let _ = r;
    let f = file_model(&w.doc);
    assert_eq!(rect_count(&f), 1);
    assert_eq!(
        changed(&f, &w.orig),
        0,
        "the plugin's work did not replace the app's file"
    );
    // a later change by the person is not written over theirs blindly either: it is run again on theirs once the work is set aside
    a.set_aside_fault(false);
    a.sync();
    assert!(!unsaved_files(&w.dir).is_empty());
    assert_eq!(rect_count(&file_model(&w.doc)), 1);
    std::fs::remove_dir_all(&w.dir).ok();
}

// ---- R0310-53: the rollback finishes even when somebody saves all the time ----

#[test]
fn a_rollback_completes_while_somebody_saves_every_time_it_plans() {
    let w = world("keeps-saving", 12);
    let mut a = agent_engine(&w);
    a.begin_call();
    for g in &w.groups[..6] {
        move_group(&mut a, *g, 5.0);
    }
    let doc = w.doc.clone();
    let hits = Arc::new(AtomicUsize::new(0));
    let h = hits.clone();
    a.set_save_hook(Some(Box::new(move |p: &str| {
        if p == "serialized" {
            h.fetch_add(1, Ordering::SeqCst);
            app_draws_rect(&doc);
        }
    })));
    let note = a.abort_call().unwrap_or_default();
    a.set_save_hook(None);
    assert!(!note.contains("NOT"), "the rollback finished: {note}");
    assert!(
        hits.load(Ordering::SeqCst) >= 6,
        "the person did save while it was planning"
    );
    let f = file_model(&w.doc);
    assert_eq!(
        changed(&f, &w.orig),
        0,
        "none of the call stays in the file"
    );
    assert_eq!(
        changed(&a.engine.model, &w.orig),
        0,
        "none of the call stays in memory"
    );
    assert!(has_rect(&f), "the person's saves are in the file");
    assert_eq!(
        f.vertices.len(),
        a.engine.model.vertices.len(),
        "memory is the file"
    );
    std::fs::remove_dir_all(&w.dir).ok();
}

#[test]
fn a_rollback_that_cannot_get_the_file_tells_the_person() {
    let w = world("locked-out", 12);
    let mut a = agent_engine(&w);
    a.begin_call();
    move_group(&mut a, w.groups[0], 5.0);
    app_draws_rect(&w.doc);
    // another program holds the lock for longer than a save waits
    let doc = w.doc.clone();
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let t = std::thread::spawn(move || {
        let l = sketchcraft::safety::lock_for(&doc, Duration::from_secs(5)).unwrap();
        tx.send(()).unwrap();
        std::thread::sleep(Duration::from_secs(13));
        drop(l);
    });
    rx.recv().unwrap();
    let note = a.abort_call().expect("the agent is told");
    assert!(note.contains("NOT"), "{note}");
    let n = a.notice.clone().unwrap_or_default();
    assert!(n.contains("AI agent"), "the person is told too: {n}");
    t.join().unwrap();
    assert_eq!(
        rect_count(&file_model(&w.doc)),
        1,
        "the person's save is untouched"
    );
    std::fs::remove_dir_all(&w.dir).ok();
}

// ---- R0310-54: a bad lock path is repaired, or the person is told once ----

#[test]
fn a_bad_lock_path_is_repaired_and_never_followed() {
    use sketchcraft::safety::lock_for_why;
    let d = scratch("lockpath");
    let doc = d.join("m.sketchcraft");
    std::fs::write(&doc, b"x").unwrap();
    let lock = d.join(".m.sketchcraft.lock");
    // a link (even to somewhere else): replaced by a plain lock file, the target never touched
    let target = d.join("target.txt");
    std::fs::write(&target, b"keep").unwrap();
    std::os::unix::fs::symlink(&target, &lock).unwrap();
    let (l, why) = lock_for_why(&doc, Duration::from_millis(200)).unwrap();
    assert!(l.is_some() && why.is_none(), "{why:?}");
    assert!(std::fs::symlink_metadata(&lock)
        .unwrap()
        .file_type()
        .is_file());
    assert_eq!(std::fs::read(&target).unwrap(), b"keep");
    drop(l);
    // a folder
    std::fs::remove_file(&lock).unwrap();
    std::fs::create_dir(&lock).unwrap();
    let (l, why) = lock_for_why(&doc, Duration::from_millis(200)).unwrap();
    assert!(l.is_some() && why.is_none(), "{why:?}");
    drop(l);
    // mode 000
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&lock, std::fs::Permissions::from_mode(0o000)).unwrap();
    }
    let (l, why) = lock_for_why(&doc, Duration::from_millis(200)).unwrap();
    assert!(l.is_some() && why.is_none(), "{why:?}");
    drop(l);
    // a folder that can't be removed: no lock, and it says why
    std::fs::remove_file(&lock).unwrap();
    std::fs::create_dir(&lock).unwrap();
    std::fs::write(lock.join("inside"), b"x").unwrap();
    let (l, why) = lock_for_why(&doc, Duration::from_millis(200)).unwrap();
    assert!(l.is_none());
    assert!(why.is_some_and(|w| w.contains("lock")));
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn the_person_is_told_once_when_no_lock_is_possible() {
    let w = world("nolock", 6);
    let lock = w.dir.join(".m.sketchcraft.lock");
    std::fs::remove_file(&lock).ok();
    std::fs::create_dir(&lock).unwrap();
    std::fs::write(lock.join("inside"), b"x").unwrap();
    let mut a = Live::open(w.doc.clone());
    assert_eq!(a.exec(&rect_cmd())["ok"], true);
    let n = a.notice.clone().unwrap_or_default();
    assert!(n.contains("lock"), "{n}");
    a.notice = None;
    assert_eq!(
        a.exec(&json!({"cmd": "rect", "from": [200, 200, 0], "to": [202, 202, 0]}))["ok"],
        true
    );
    assert!(a.notice.is_none(), "not told again: {:?}", a.notice);
    assert!(has_rect(&file_model(&w.doc)), "saving still works");
    std::fs::remove_dir_all(&w.dir).ok();
}

// ---- R0310-56 ----

#[test]
fn a_file_rewritten_in_place_with_its_old_time_is_noticed() {
    let w = world("spoof", 12);
    let mut a = Live::open(w.doc.clone());
    let before = std::fs::metadata(&w.doc).unwrap();
    let mut bytes = std::fs::read(&w.doc).unwrap();
    let at = bytes
        .windows(4)
        .position(|x| x == b"\"G3\"")
        .expect("a group name");
    bytes[at + 1] = b'H';
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open(&w.doc)
            .unwrap();
        f.write_all(&bytes).unwrap();
        f.set_modified(before.modified().unwrap()).unwrap();
    }
    let after = std::fs::metadata(&w.doc).unwrap();
    assert_eq!(
        (before.len(), before.modified().unwrap()),
        (after.len(), after.modified().unwrap()),
        "same size, same time"
    );
    assert!(a.sync(), "the changed file is picked up");
    assert!(a.engine.model.groups.values().any(|g| g.name == "H3"));
    std::fs::remove_dir_all(&w.dir).ok();
}

#[test]
fn damaged_copies_and_backups_are_private_like_the_other_side_files() {
    let w = world("modes", 6);
    let mut a = Live::open(w.doc.clone());
    a.save(true).unwrap();
    assert_eq!(a.exec(&rect_cmd())["ok"], true);
    a.save(true).unwrap();
    let backups: Vec<PathBuf> = std::fs::read_dir(w.dir.join("Backups"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    assert!(!backups.is_empty());
    for b in &backups {
        assert_eq!(mode(b), 0o600, "{b:?}");
    }
    std::fs::write(&w.doc, b"garbage").unwrap();
    let mut b = Live::open(w.doc.clone());
    b.sync();
    let damaged: Vec<PathBuf> = std::fs::read_dir(&w.dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().contains(".damaged-"))
        .collect();
    assert_eq!(damaged.len(), 1);
    assert_eq!(mode(&damaged[0]), 0o600);
    std::fs::remove_dir_all(&w.dir).ok();
}

#[test]
fn the_reply_about_a_garbage_or_deleted_file_is_accurate() {
    for (name, wipe) in [("garbage-note", true), ("deleted-note", false)] {
        let w = world(name, 6);
        let mut a = agent_engine(&w);
        a.begin_call();
        move_group(&mut a, w.groups[0], 5.0);
        if wipe {
            std::fs::write(&w.doc, b"not a model at all").unwrap();
        } else {
            std::fs::remove_file(&w.doc).unwrap();
        }
        let note = a.abort_call();
        let note = note.unwrap_or_default();
        assert!(!note.contains("their work was kept"), "{note}");
        if wipe {
            assert!(note.contains("couldn't be read"), "{note}");
        }
        assert_eq!(changed(&file_model(&w.doc), &w.orig), 0);
        std::fs::remove_dir_all(&w.dir).ok();
    }
}
