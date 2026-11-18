//! Batches: `begin_batch` ... `end_batch` from ONE client is one undo step, and the clients in one program stay apart.
use serde_json::{json, Value};
use sketchcraft::live::Live;
use sketchcraft::policy::{Caller, Workspace};
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("sc-batches-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn agent() -> Caller {
    Caller::Mcp(Workspace::anywhere())
}

fn rect_at(x: i64) -> Value {
    json!({"cmd": "rect", "from": [x, 0, 0], "to": [x + 4, 4, 0]})
}

fn faces(l: &Live) -> usize {
    l.engine.model.faces.len()
}

fn ok(l: &mut Live, c: &Caller, v: Value) -> Value {
    let r = l.exec_as(c, &v);
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn refused(l: &mut Live, c: &Caller, v: Value) -> String {
    let r = l.exec_as(c, &v);
    assert_eq!(r["ok"], false, "{v} should have been refused: {r}");
    r["error"].as_str().unwrap_or("").to_string()
}

/// The bug as reported: the app sends begin_batch, several commands, end_batch, and got several undo steps.
#[test]
fn the_apps_own_batch_is_one_undo_step() {
    let dir = scratch("ui");
    let mut live = Live::open(dir.join("m.sketchcraft"));
    let me = Caller::Ui;
    ok(&mut live, &me, json!({"cmd": "begin_batch"}));
    for i in 0..3 {
        ok(&mut live, &me, rect_at(i * 10));
    }
    ok(&mut live, &me, json!({"cmd": "end_batch"}));
    assert_eq!(live.engine.undo_depth(), 1, "three commands, one step");
    ok(&mut live, &me, json!({"cmd": "undo"}));
    assert_eq!(faces(&live), 0, "one undo took all three back");
    ok(&mut live, &me, json!({"cmd": "redo"}));
    assert_eq!(faces(&live), 3, "one redo puts all three back");
    std::fs::remove_dir_all(&dir).ok();
}

/// An AI answer: an agent opens a batch, sends several commands, ends it. One Undo takes the whole answer back, one Redo
/// brings it back; the person's own commands before and after are their own steps.
#[test]
fn an_agents_answer_is_one_undo_step_and_the_persons_commands_are_their_own() {
    let dir = scratch("agent");
    let doc = dir.join("m.sketchcraft");
    let mut live = Live::open(doc.clone());
    let ai = agent();
    let me = Caller::Ui;
    ok(&mut live, &me, rect_at(0));
    let b = ok(&mut live, &ai, json!({"cmd": "begin_batch"}));
    for i in 1..=3 {
        ok(&mut live, &ai, rect_at(i * 10));
    }
    let end = ok(&mut live, &ai, json!({"cmd": "end_batch", "id": b["batch"]}));
    assert_eq!(end["origin"], "an agent");
    ok(&mut live, &me, rect_at(100));
    assert_eq!(faces(&live), 5);
    assert_eq!(live.engine.undo_depth(), 3, "my rectangle, the agent's answer, my rectangle");
    ok(&mut live, &me, json!({"cmd": "undo"}));
    assert_eq!(faces(&live), 4, "my last rectangle only");
    assert_eq!(live.engine.undo_origin(), Some("an agent"));
    ok(&mut live, &me, json!({"cmd": "undo"}));
    assert_eq!(faces(&live), 1, "all three of the agent's rectangles in one Undo");
    ok(&mut live, &me, json!({"cmd": "redo"}));
    assert_eq!(faces(&live), 4, "and all three back in one Redo");
    // the agent can take its own answer back too
    ok(&mut live, &ai, json!({"cmd": "undo"}));
    assert_eq!(faces(&live), 1);
    std::fs::remove_dir_all(&dir).ok();
}

/// The person edits while an agent's batch is open: the batch closes right there (its work so far is one step, saved),
/// and the person's edit is its own step. Nothing of theirs is inside the agent's step.
#[test]
fn the_persons_edit_closes_an_agents_batch_and_stays_its_own_step() {
    let dir = scratch("interleave");
    let mut live = Live::open(dir.join("m.sketchcraft"));
    let ai = agent();
    let b = ok(&mut live, &ai, json!({"cmd": "begin_batch"}));
    ok(&mut live, &ai, rect_at(0));
    ok(&mut live, &ai, rect_at(10));
    ok(&mut live, &Caller::Ui, rect_at(100));
    assert!(!live.engine.in_batch(), "the person's edit closed the agent's batch");
    assert_eq!(live.engine.undo_depth(), 2, "the agent's two commands, then mine");
    // the agent's later commands are single steps; its end_batch for the closed batch says so and changes nothing
    ok(&mut live, &ai, rect_at(20));
    let end = ok(&mut live, &ai, json!({"cmd": "end_batch", "id": b["batch"]}));
    assert_eq!(end["already_ended"], true, "{end}");
    assert_eq!(live.engine.undo_depth(), 3);
    ok(&mut live, &Caller::Ui, json!({"cmd": "undo"}));
    ok(&mut live, &Caller::Ui, json!({"cmd": "undo"}));
    assert_eq!(faces(&live), 2, "undoing the agent's last and mine left its first two together");
    ok(&mut live, &Caller::Ui, json!({"cmd": "undo"}));
    assert_eq!(faces(&live), 0);
    std::fs::remove_dir_all(&dir).ok();
}

/// A batch the app opened for its own commands keeps an agent's or a plugin's commands out, and the agent can't end it.
#[test]
fn another_clients_commands_never_join_the_apps_batch() {
    let dir = scratch("apps-own");
    let mut live = Live::open(dir.join("m.sketchcraft"));
    let ai = agent();
    ok(&mut live, &Caller::Ui, json!({"cmd": "begin_batch"}));
    ok(&mut live, &Caller::Ui, rect_at(0));
    assert!(refused(&mut live, &ai, rect_at(50)).contains("middle of its own change"));
    assert!(refused(&mut live, &Caller::Plugin, rect_at(60)).contains("middle of its own change"));
    assert!(refused(&mut live, &ai, json!({"cmd": "end_batch"})).contains("not an agent's"));
    assert!(refused(&mut live, &ai, json!({"cmd": "begin_batch"})).contains("already open"));
    ok(&mut live, &ai, json!({"cmd": "info"}));
    ok(&mut live, &Caller::Ui, rect_at(10));
    ok(&mut live, &Caller::Ui, json!({"cmd": "end_batch"}));
    assert_eq!(faces(&live), 2, "only the app's two");
    assert_eq!(live.engine.undo_depth(), 1);
    // and the agent can work again
    ok(&mut live, &ai, rect_at(50));
    assert_eq!(live.engine.undo_depth(), 2);
    std::fs::remove_dir_all(&dir).ok();
}

/// A plugin run (the app opens the batch, the plugin's commands fill it) keeps working: one step, and an agent can't join.
#[test]
fn a_plugin_run_is_still_one_step_and_an_agent_cannot_join_it() {
    let dir = scratch("plugin");
    let mut live = Live::open(dir.join("m.sketchcraft"));
    ok(&mut live, &Caller::Ui, json!({"cmd": "begin_batch"}));
    ok(&mut live, &Caller::Plugin, rect_at(0));
    ok(&mut live, &Caller::Plugin, rect_at(10));
    assert!(refused(&mut live, &agent(), rect_at(50)).contains("Another client's batch"));
    assert!(refused(&mut live, &agent(), json!({"cmd": "end_batch"})).contains("not an agent's"));
    ok(&mut live, &Caller::Ui, json!({"cmd": "end_batch"}));
    assert_eq!((faces(&live), live.engine.undo_depth()), (2, 1));
    // a plugin can't open or end batches itself
    refused(&mut live, &Caller::Plugin, json!({"cmd": "begin_batch"}));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn unbalanced_and_nested_calls_get_a_clear_error() {
    let dir = scratch("unbalanced");
    let mut live = Live::open(dir.join("m.sketchcraft"));
    let ai = agent();
    // ending what was never begun
    assert!(refused(&mut live, &Caller::Ui, json!({"cmd": "end_batch"})).contains("no batch is open"));
    assert!(refused(&mut live, &ai, json!({"cmd": "end_batch"})).contains("no batch is open"));
    // nesting
    ok(&mut live, &ai, json!({"cmd": "begin_batch"}));
    ok(&mut live, &ai, rect_at(0));
    assert!(refused(&mut live, &ai, json!({"cmd": "begin_batch"})).contains("don't nest"));
    // the refused begin did not disturb the open batch: it still ends as one step
    ok(&mut live, &ai, rect_at(10));
    ok(&mut live, &ai, json!({"cmd": "end_batch"}));
    assert_eq!((faces(&live), live.engine.undo_depth()), (2, 1));
    // undo is off while a batch is open
    ok(&mut live, &ai, json!({"cmd": "begin_batch"}));
    ok(&mut live, &ai, rect_at(20));
    assert!(refused(&mut live, &ai, json!({"cmd": "undo"})).contains("still running"));
    ok(&mut live, &ai, json!({"cmd": "end_batch"}));
    // a stale end (an id from a batch that is over) is harmless
    let r = ok(&mut live, &ai, json!({"cmd": "end_batch", "id": 1}));
    assert_eq!(r["already_ended"], true);
    assert_eq!(live.engine.undo_depth(), 2);
    // the app: nested begin
    ok(&mut live, &Caller::Ui, json!({"cmd": "begin_batch"}));
    assert!(refused(&mut live, &Caller::Ui, json!({"cmd": "begin_batch"})).contains("don't nest"));
    ok(&mut live, &Caller::Ui, json!({"cmd": "end_batch"}));
    std::fs::remove_dir_all(&dir).ok();
}

/// A batch nobody ends is closed on its own (an agent's or plugin's after 150 s, the app's own after 30 s of silence).
/// The work in it is kept as one step, and saved.
#[test]
fn a_forgotten_batch_is_closed_and_what_it_did_is_one_saved_step() {
    let dir = scratch("stale");
    let doc = dir.join("m.sketchcraft");
    let mut live = Live::open(doc.clone());
    let ai = agent();
    ok(&mut live, &ai, json!({"cmd": "begin_batch"}));
    ok(&mut live, &ai, rect_at(0));
    ok(&mut live, &ai, rect_at(10));
    live.engine.expire_batch_for_test();
    live.housekeeping();
    assert!(!live.engine.in_batch());
    assert_eq!(live.engine.undo_depth(), 1);
    // an expired batch is also closed by the next command
    ok(&mut live, &ai, json!({"cmd": "begin_batch"}));
    live.engine.expire_batch_for_test();
    ok(&mut live, &Caller::Ui, rect_at(50));
    assert!(!live.engine.in_batch());
    // the app's own, left open by a page that went away: a new begin closes it and opens a fresh one
    ok(&mut live, &Caller::Ui, json!({"cmd": "begin_batch"}));
    ok(&mut live, &Caller::Ui, rect_at(60));
    live.engine.expire_batch_for_test();
    ok(&mut live, &Caller::Ui, json!({"cmd": "begin_batch"}));
    assert!(live.engine.in_batch(), "the stale one was closed, a fresh one opened");
    ok(&mut live, &Caller::Ui, json!({"cmd": "end_batch"}));
    assert_eq!(live.engine.undo_depth(), 3, "the agent's two, my rectangle, the stale batch's one");
    let mut check = sketchcraft::Engine::new();
    let r = check.exec_json(&json!({"cmd": "open", "path": doc}).to_string());
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(check.model.faces.len(), 4);
    // the app's own batch that goes quiet (30 s) is closed too, but an agent's that is merely thinking is not
    ok(&mut live, &Caller::Ui, json!({"cmd": "begin_batch"}));
    ok(&mut live, &Caller::Ui, rect_at(70));
    live.engine.idle_batch_for_test();
    live.housekeeping();
    assert!(!live.engine.in_batch());
    ok(&mut live, &ai, json!({"cmd": "begin_batch"}));
    ok(&mut live, &ai, rect_at(80));
    live.engine.idle_batch_for_test();
    live.housekeeping();
    assert!(live.engine.in_batch(), "an agent has until the 150 s limit");
    ok(&mut live, &ai, json!({"cmd": "end_batch"}));
    std::fs::remove_dir_all(&dir).ok();
}
