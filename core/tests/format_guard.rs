//! The format guard (#57): a file saved by a newer SketchCraft opens read-only, so an older copy (the app, the bridge, an
//! agent's MCP server) can never write over it and silently drop what it doesn't know.

use serde_json::{json, Value};
use sketchcraft::live::Live;
use sketchcraft::policy::{Caller, Workspace};
use sketchcraft::Engine;
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("sc-fmt-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn ok(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

/// A model with something in every field of `Model` (what the commands can't make is patched in as JSON).
fn full_model() -> Engine {
    let mut e = Engine::new();
    let f = ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [2, 2, 0]}))["face"].clone();
    let g = ok(&mut e, json!({"cmd": "group", "ids": [f], "name": "Wall"}))["group"].clone();
    ok(&mut e, json!({"cmd": "tag", "ids": [g], "tag": "Walls"}));
    ok(&mut e, json!({"cmd": "tag_color", "tag": "Walls", "color": [0.5, 0.25, 0.75]}));
    ok(&mut e, json!({"cmd": "material", "name": "Brick", "color": [0.7, 0.3, 0.2]}));
    let f2 = ok(&mut e, json!({"cmd": "rect", "from": [5, 5, 0], "to": [6, 6, 0]}))["face"].clone();
    let made = ok(&mut e, json!({"cmd": "make_component", "ids": [f2], "name": "Part"}));
    ok(&mut e, json!({"cmd": "guide", "point": [1, 1, 0], "direction": [0, 0, 1]}));
    ok(&mut e, json!({"cmd": "save_view", "name": "Front", "eye": [5, -9, 3], "target": [1, 1, 0], "fov": 40}));
    ok(&mut e, json!({"cmd": "dimension", "from": [0, 0, 0], "to": [2, 0, 0], "offset": [0, -1, 0]}));
    ok(&mut e, json!({"cmd": "text", "at": [1, 1, 0], "text": "Hello"}));
    ok(&mut e, json!({"cmd": "units", "units": "ft"}));
    let inst = made["instance"].clone();
    let mut v = serde_json::to_value(&e.model).unwrap();
    v["environment"] = json!("studio");
    v["photo_points"] = json!([{"name": "Corner", "marks": {"a.jpg": [0.5, -0.25]}}]);
    v["photo_edges"] = json!([[0, 0]]);
    v["photo_guides"] = json!([1]);
    v["cages"] = json!({"77": {"points": [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], "faces": [[0, 1, 2]], "levels": 1}});
    v["timeline"] = json!({"clips": [{"name": "Tour", "seconds": 4.0, "kind": "scenes", "views": ["Front"], "hold": 1.0}]});
    v["classes"] = json!({inst.to_string(): "IfcWall"});
    v["location"] = json!({"lat": 30.27, "lon": -97.74, "elevation": 149.0});
    v["clouds"] = json!({"88": {"name": "Scan", "points": "AACAPwAAAEAAAEBA", "colors": "", "hidden": false, "version": 1}});
    v["sections"] = json!([{"id": 99, "name": "Section 1", "origin": [0.0, 0.0, 1.0], "normal": [0.0, 0.0, 1.0]}]);
    v["active_section"] = json!(99);
    v["axes"] = json!([[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
    // next_id stays above every id used
    v["next_id"] = json!(200);
    e.model = serde_json::from_value(v).expect("the patched model reads");
    e
}

/// A file from a newer SketchCraft: today's model with a table this build has never heard of and a higher format.
/// (`sheets` is not it any more: 0.3.8 reads those, format 2. See sheets.rs.)
fn newer_file(dir: &std::path::Path, name: &str, tweak: impl Fn(&mut Value)) -> (PathBuf, Vec<u8>) {
    let mut e = full_model();
    let p = dir.join("src.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": p}));
    let mut v: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    tweak(&mut v);
    let out = dir.join(name);
    let bytes = serde_json::to_vec(&v).unwrap();
    std::fs::write(&out, &bytes).unwrap();
    (out, bytes)
}

fn format_two(v: &mut Value) {
    v["format"] = json!(3);
    v["needs"] = json!(["holograms"]);
    v["holograms"] = json!({"1": {"name": "H101", "size": "ARCH D"}});
}
fn unknown_key(v: &mut Value) {
    v["holograms"] = json!({"1": {"name": "H101"}});
}
fn needs_only(v: &mut Value) {
    v["needs"] = json!(["holograms"]);
}

type Tweak = fn(&mut Value);
const KINDS: [(&str, Tweak); 3] = [("format 3", format_two), ("unknown key", unknown_key), ("needs", needs_only)];

fn agent(dir: &std::path::Path) -> Caller {
    Caller::Mcp(Workspace::at(dir))
}

// ---- round trip ------------------------------------------------------------------------------

#[test]
fn a_model_with_every_field_survives_save_open_save() {
    let dir = scratch("roundtrip");
    let mut e = full_model();
    let (a, b) = (dir.join("a.sketchcraft"), dir.join("b.sketchcraft"));
    ok(&mut e, json!({"cmd": "save", "path": a}));
    let first: Value = serde_json::from_slice(&std::fs::read(&a).unwrap()).unwrap();
    // every key the guard knows is in the file (the full model has something in each field), and it says format 1.
    // (the drawing-sheet keys are written only by a model that has sheets, which is format 2: see sheets.rs)
    for k in sketchcraft::format::KNOWN_KEYS.iter().filter(|k| !["needs", "drafting", "sheets", "sheet_items", "title_blocks"].contains(k)) {
        assert!(first.get(*k).is_some(), "the full model has nothing in \"{k}\" (add it to full_model): {:?}", first.as_object().unwrap().keys().collect::<Vec<_>>());
    }
    assert_eq!(first["format"], 1);
    assert!(first.get("needs").is_none());
    // a file this build wrote is not read-only for itself
    let mut f = Engine::new();
    let r = ok(&mut f, json!({"cmd": "open", "path": a}));
    assert!(r["read_only"].is_null(), "{r}");
    assert!(f.read_only.is_none());
    ok(&mut f, json!({"cmd": "save", "path": b}));
    // byte for byte (open drops only the open-group marker, which is None here)
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap(), "save, open, save changed the file");
    // and the second round
    let mut g = Engine::new();
    ok(&mut g, json!({"cmd": "open", "path": b}));
    let c = dir.join("c.sketchcraft");
    ok(&mut g, json!({"cmd": "save", "path": c}));
    assert_eq!(std::fs::read(&b).unwrap(), std::fs::read(&c).unwrap());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn files_from_before_the_guard_open_normally_and_gain_a_format() {
    let dir = scratch("old");
    let mut e = full_model();
    let a = dir.join("a.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": a}));
    let mut v: Value = serde_json::from_slice(&std::fs::read(&a).unwrap()).unwrap();
    v.as_object_mut().unwrap().remove("format");
    let old = dir.join("old.sketchcraft");
    std::fs::write(&old, serde_json::to_vec(&v).unwrap()).unwrap();
    let mut f = Engine::new();
    ok(&mut f, json!({"cmd": "open", "path": old}));
    assert!(f.read_only.is_none());
    ok(&mut f, json!({"cmd": "rect", "from": [9, 9, 0], "to": [10, 10, 0]}));
    let out = dir.join("out.sketchcraft");
    ok(&mut f, json!({"cmd": "save", "path": out}));
    let w: Value = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    assert_eq!(w["format"], 1);
    // an old file's live copy works the same way
    let mut live = Live::open(old.clone());
    let r = live.exec(&json!({"cmd": "rect", "from": [20, 0, 0], "to": [21, 1, 0]}));
    assert_eq!(r["ok"], true, "{r}");
    assert!(live.scene()["notice"].is_null());
    assert_eq!(serde_json::from_slice::<Value>(&std::fs::read(&old).unwrap()).unwrap()["format"], 1);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_normal_live_session_is_unaffected() {
    let dir = scratch("normal");
    let doc = dir.join("live.sketchcraft");
    let mut live = Live::open(doc.clone());
    assert_eq!(live.exec(&json!({"cmd": "rect", "from": [0, 0, 0], "to": [2, 2, 0]}))["ok"], true);
    assert_eq!(live.exec(&json!({"cmd": "undo"}))["ok"], true);
    assert_eq!(live.exec(&json!({"cmd": "redo"}))["ok"], true);
    let mut again = Live::open(doc.clone());
    assert!(again.engine.read_only.is_none());
    assert_eq!(again.exec_as(&agent(&dir), &json!({"cmd": "rect", "from": [5, 5, 0], "to": [6, 6, 0]}))["ok"], true);
    assert!(again.scene()["read_only"].is_null());
    std::fs::remove_dir_all(&dir).ok();
}

// ---- newer files -----------------------------------------------------------------------------

#[test]
fn a_newer_file_opens_read_only() {
    for (what, tweak) in KINDS {
        let dir = scratch("open");
        let (p, _) = newer_file(&dir, "n.sketchcraft", tweak);
        let mut e = Engine::new();
        let r = ok(&mut e, json!({"cmd": "open", "path": p}));
        assert!(r["read_only"].is_array(), "{what}: {r}");
        assert!(e.read_only.is_some(), "{what}");
        // looking works
        assert_eq!(e.exec_json(&json!({"cmd": "info"}).to_string())["ok"], true);
        assert!(e.exec_json(&json!({"cmd": "info"}).to_string())["read_only"].is_array());
        // changing doesn't
        let r = e.exec_json(&json!({"cmd": "rect", "from": [0, 0, 0], "to": [1, 1, 0]}).to_string());
        assert_eq!(r["ok"], false, "{what}: {r}");
        for c in [json!({"cmd": "undo"}), json!({"cmd": "redo"}), json!({"cmd": "begin_batch"}), json!({"cmd": "units", "units": "m"})] {
            assert_eq!(e.exec_json(&c.to_string())["ok"], false, "{what}: {c}");
        }
        // starting a new model ends it, and so does opening a normal file
        ok(&mut e, json!({"cmd": "new"}));
        assert!(e.read_only.is_none());
        ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [1, 1, 0]}));
        ok(&mut e, json!({"cmd": "open", "path": p}));
        assert!(e.read_only.is_some());
        let plain = dir.join("plain.sketchcraft");
        ok(&mut full_model(), json!({"cmd": "save", "path": plain}));
        ok(&mut e, json!({"cmd": "open", "path": plain}));
        assert!(e.read_only.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[test]
fn a_newer_file_refuses_save_even_to_another_path() {
    for (what, tweak) in KINDS {
        let dir = scratch("save");
        let (p, bytes) = newer_file(&dir, "n.sketchcraft", tweak);
        let mut e = Engine::new();
        ok(&mut e, json!({"cmd": "open", "path": p}));
        for target in [p.clone(), dir.join("copy.sketchcraft")] {
            let r = e.exec_json(&json!({"cmd": "save", "path": target}).to_string());
            assert_eq!(r["ok"], false, "{what}: {r}");
            assert!(r["error"].as_str().unwrap().contains("newer SketchCraft"), "{r}");
        }
        assert_eq!(std::fs::read(&p).unwrap(), bytes, "{what}: the file changed");
        assert!(!dir.join("copy.sketchcraft").exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[test]
fn a_newer_live_file_is_never_overwritten() {
    for (what, tweak) in KINDS {
        let dir = scratch("live");
        let (p, bytes) = newer_file(&dir, "live.sketchcraft", tweak);
        let mut live = Live::open(p.clone());
        assert!(live.engine.read_only.is_some(), "{what}");
        let note = live.scene()["notice"].as_str().unwrap_or("").to_string();
        assert_eq!(note, "Saved by a newer SketchCraft \u{2014} update to edit. You can look, but changes won't be saved here.", "{what}");
        assert!(live.scene()["read_only"].is_array());
        let person = live.exec(&json!({"cmd": "rect", "from": [0, 0, 0], "to": [1, 1, 0]}));
        let robot = live.exec_as(&agent(&dir), &json!({"cmd": "rect", "from": [0, 0, 0], "to": [1, 1, 0]}));
        let plugin = live.exec_as(&Caller::Plugin, &json!({"cmd": "rect", "from": [0, 0, 0], "to": [1, 1, 0]}));
        for r in [&person, &robot, &plugin] {
            assert_eq!(r["ok"], false, "{what}: {r}");
            assert!(r.get("save_error").is_none(), "{r}");
        }
        assert!(live.exec(&json!({"cmd": "undo"}))["ok"] == false);
        assert!(live.save(true).is_err());
        assert!(live.save(false).is_err());
        live.housekeeping();
        // a second program opening the same file reads it the same way, and a reload keeps it read-only
        let mut other = Live::open(p.clone());
        assert!(other.engine.read_only.is_some());
        assert!(!other.sync());
        assert_eq!(std::fs::read(&p).unwrap(), bytes, "{what}: the live file changed");
        assert!(!dir.join("Backups").exists() || std::fs::read_dir(dir.join("Backups")).unwrap().all(|b| std::fs::read(b.unwrap().path()).unwrap() == bytes), "{what}: a backup of something else");
        // the person starts over: saving resumes, into a new file's worth of the same path, with the old one kept in Backups
        let n = live.exec(&json!({"cmd": "new"}));
        assert_eq!(n["ok"], true, "{n}");
        assert!(live.engine.read_only.is_none());
        let r = live.exec(&json!({"cmd": "rect", "from": [0, 0, 0], "to": [1, 1, 0]}));
        assert_eq!(r["ok"], true, "{r}");
        assert!(live.scene()["notice"].is_null() || !live.scene()["notice"].as_str().unwrap().starts_with("Saved by a newer"));
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[test]
fn a_file_that_turns_newer_while_open_is_picked_up_read_only() {
    let dir = scratch("reload");
    let doc = dir.join("live.sketchcraft");
    let mut live = Live::open(doc.clone());
    assert_eq!(live.exec(&json!({"cmd": "rect", "from": [0, 0, 0], "to": [2, 2, 0]}))["ok"], true);
    // a newer program saves over it
    let mut v: Value = serde_json::from_slice(&std::fs::read(&doc).unwrap()).unwrap();
    format_two(&mut v);
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let bytes = serde_json::to_vec(&v).unwrap();
    std::fs::write(&doc, &bytes).unwrap();
    let r = live.exec(&json!({"cmd": "rect", "from": [5, 5, 0], "to": [6, 6, 0]}));
    assert_eq!(r["ok"], false, "{r}");
    assert!(live.engine.read_only.is_some());
    assert_eq!(std::fs::read(&doc).unwrap(), bytes);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_newer_file_refuses_restore_over_it_and_a_newer_backup_is_not_restored() {
    for (what, tweak) in KINDS {
        let dir = scratch("restore");
        let doc = dir.join("live.sketchcraft");
        // a normal live model with a backup in Backups
        let mut live = Live::open(doc.clone());
        live.exec(&json!({"cmd": "rect", "from": [0, 0, 0], "to": [2, 2, 0]}));
        live.exec(&json!({"cmd": "new"}));
        live.exec(&json!({"cmd": "rect", "from": [0, 0, 0], "to": [3, 3, 0]}));
        let list = live.exec(&json!({"cmd": "list_backups"}));
        let items = list["items"].as_array().unwrap().clone();
        assert!(!items.is_empty(), "{list}");
        let name = items[0]["name"].clone();
        // 1. the live file turns newer: Restore (replace) is refused, the file is untouched
        let mut v: Value = serde_json::from_slice(&std::fs::read(&doc).unwrap()).unwrap();
        tweak(&mut v);
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let bytes = serde_json::to_vec(&v).unwrap();
        std::fs::write(&doc, &bytes).unwrap();
        let r = live.exec(&json!({"cmd": "restore_backup", "kind": "backup", "name": name, "mode": "replace"}));
        assert_eq!(r["ok"], false, "{what}: {r}");
        assert!(r["error"].as_str().unwrap().contains("newer SketchCraft"), "{r}");
        assert_eq!(std::fs::read(&doc).unwrap(), bytes, "{what}");
        // ...but a copy of the backup, byte for byte, is still allowed
        let c = live.exec(&json!({"cmd": "restore_backup", "kind": "backup", "name": name, "mode": "copy"}));
        assert_eq!(c["ok"], true, "{c}");
        std::fs::remove_dir_all(&dir).ok();

        // 2. a backup that is itself newer is not restored over a normal model
        let dir = scratch("restore2");
        let doc = dir.join("live.sketchcraft");
        let mut live = Live::open(doc.clone());
        live.exec(&json!({"cmd": "rect", "from": [0, 0, 0], "to": [2, 2, 0]}));
        live.exec(&json!({"cmd": "new"}));
        live.exec(&json!({"cmd": "rect", "from": [0, 0, 0], "to": [3, 3, 0]}));
        let list = live.exec(&json!({"cmd": "list_backups"}));
        let name = list["items"][0]["name"].as_str().unwrap().to_string();
        let backup = dir.join("Backups").join(&name);
        let mut bv: Value = serde_json::from_slice(&std::fs::read(&backup).unwrap()).unwrap();
        tweak(&mut bv);
        std::fs::write(&backup, serde_json::to_vec(&bv).unwrap()).unwrap();
        let before = std::fs::read(&doc).unwrap();
        let r = live.exec(&json!({"cmd": "restore_backup", "kind": "backup", "name": name, "mode": "replace"}));
        assert_eq!(r["ok"], false, "{what}: {r}");
        assert!(r["error"].as_str().unwrap().contains("newer SketchCraft"), "{r}");
        assert_eq!(std::fs::read(&doc).unwrap(), before, "{what}: the model's file changed");
        assert!(live.engine.read_only.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[test]
fn an_agent_is_told_why() {
    for (what, tweak) in KINDS {
        let dir = scratch("agent");
        let (p, bytes) = newer_file(&dir, "live.sketchcraft", tweak);
        // the MCP server's own start: --doc FILE
        let mut live = Live::open(p.clone());
        let ws = agent(&dir);
        let r = live.exec_as(&ws, &json!({"cmd": "rect", "from": [0, 0, 0], "to": [1, 1, 0]}));
        assert_eq!(r["ok"], false, "{what}: {r}");
        let why = r["error"].as_str().unwrap();
        assert!(why.starts_with("This model was saved by a newer SketchCraft"), "{why}");
        assert!(why.contains("read-only") && why.contains("Update SketchCraft"), "{why}");
        assert!(why.contains("holograms") || why.contains("format is 3"), "{why}");
        // reading is fine, and says so
        let i = live.exec_as(&ws, &json!({"cmd": "info"}));
        assert_eq!(i["ok"], true);
        assert!(i["read_only"].is_array());
        assert_eq!(std::fs::read(&p).unwrap(), bytes);
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[test]
fn the_library_will_not_place_a_newer_part() {
    let dir = scratch("lib");
    let (p, _) = newer_file(&dir, "part.sketchcraft", format_two);
    let err = sketchcraft::library::load_file(p.to_str().unwrap()).unwrap_err().to_string();
    assert!(err.contains("newer SketchCraft"), "{err}");
    let plain = dir.join("plain.sketchcraft");
    ok(&mut full_model(), json!({"cmd": "save", "path": plain}));
    assert!(sketchcraft::library::load_file(plain.to_str().unwrap()).is_ok());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_file_with_every_current_key_and_nothing_else_is_not_newer() {
    // the keys this build writes are the keys it knows (guards against a field added to Model and not to KNOWN_KEYS)
    let dir = scratch("known");
    let mut e = full_model();
    let a = dir.join("a.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": a}));
    assert!(sketchcraft::format::check(&std::fs::read(&a).unwrap()).is_none());
    let mut w = Engine::new();
    let b = dir.join("b.sketchcraft");
    ok(&mut w, json!({"cmd": "save", "path": b}));
    assert!(sketchcraft::format::check(&std::fs::read(&b).unwrap()).is_none());
    std::fs::remove_dir_all(&dir).ok();
}
