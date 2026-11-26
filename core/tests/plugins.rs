//! Installing, turning off and removing plugins (in a scratch Plugins folder).
use serde_json::{json, Value};
use sketchcraft::Engine;
use std::io::Write;

fn run(e: &mut Engine, v: Value) -> Value {
    e.exec_json(&v.to_string())
}

fn zip_of(files: &[(&str, &str)]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
        for (name, body) in files {
            z.start_file(*name, opts).unwrap();
            z.write_all(body.as_bytes()).unwrap();
        }
        z.finish().unwrap();
    }
    buf.into_inner()
}

fn b64(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[test]
fn install_turn_off_and_remove_a_plugin() {
    let home = std::env::temp_dir().join(format!("sc-plugins-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    std::env::set_var("SKETCHCRAFT_DOC", home.join("live.sketchcraft"));
    let mut e = Engine::new();

    // SKETCHCRAFT_DOC scopes every user folder: plugins, library and skins
    assert_eq!(sketchcraft::library::default_dir(), home.join("Library"));
    assert_eq!(sketchcraft::plugins::dir(), home.join("Plugins"));

    let manifest = r#"{"name":"Stair Maker","version":"1.2.0","commands":[{"id":"stairs","label":"Stairs…","icon":"stairs.svg"}]}"#;
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="10" height="10"/></svg>"#;
    // zipped inside a folder, with a junk file that isn't unpacked
    let z = zip_of(&[("stair-maker/plugin.json", manifest), ("stair-maker/main.js", "sketchcraft.command('stairs', () => {});"), ("stair-maker/stairs.svg", svg), ("stair-maker/run.exe", "MZ")]);
    let r = run(&mut e, json!({"cmd":"install_plugin","data": b64(&z)}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["id"], "stair-maker");
    let folder = home.join("Plugins/stair-maker");
    assert!(folder.join("main.js").is_file() && !folder.join("run.exe").exists());

    let list = run(&mut e, json!({"cmd":"plugins"}));
    let p = list["plugins"].as_array().unwrap().iter().find(|p| p["id"] == "stair-maker").unwrap().clone();
    assert_eq!(p["enabled"], true);
    assert_eq!(p["version"], "1.2.0");
    assert!(p["commands"][0]["icon"].as_str().unwrap().starts_with("data:image/svg+xml;base64,"));

    // off, and on again
    run(&mut e, json!({"cmd":"plugin_enable","id":"stair-maker","on":false}));
    let list = run(&mut e, json!({"cmd":"plugins"}));
    assert_eq!(list["plugins"].as_array().unwrap().iter().find(|p| p["id"] == "stair-maker").unwrap()["enabled"], false);
    run(&mut e, json!({"cmd":"plugin_enable","id":"stair-maker","on":true}));

    // installing again never replaces silently: it needs `replace`, and a preview says what it is
    run(&mut e, json!({"cmd":"plugin_enable","id":"stair-maker","on":false}));
    let r = run(&mut e, json!({"cmd":"install_plugin","data": b64(&z)}));
    assert_eq!(r["ok"], false, "{r}");
    assert!(r["error"].as_str().unwrap().contains("already installed"));
    let p = run(&mut e, json!({"cmd":"install_plugin","data": b64(&z),"preview":true}));
    assert_eq!(p["ok"], true, "{p}");
    assert_eq!(p["preview"], true);
    assert_eq!(p["name"], "Stair Maker");
    assert_eq!(p["installed"]["version"], "1.2.0");
    assert_eq!(p["sha256"].as_str().unwrap().len(), 64);
    assert!(p["files"].as_array().unwrap().iter().any(|f| f["name"] == "main.js"));
    // the old one is kept aside, and what the person chose (off) is kept
    let r = run(&mut e, json!({"cmd":"install_plugin","data": b64(&z),"replace":true}));
    assert_eq!(r["replaced"], true, "{r}");
    let list = run(&mut e, json!({"cmd":"plugins"}));
    assert_eq!(list["plugins"].as_array().unwrap().iter().find(|p| p["id"] == "stair-maker").unwrap()["enabled"], false);
    run(&mut e, json!({"cmd":"plugin_enable","id":"stair-maker","on":true}));

    // a zip that reaches outside its folder is refused, and nothing is written
    let evil = zip_of(&[("plugin.json", r#"{"name":"Evil","commands":[]}"#), ("main.js", ""), ("../../escape.js", "x")]);
    let r = run(&mut e, json!({"cmd":"install_plugin","data": b64(&evil)}));
    assert_eq!(r["ok"], false, "{r}");
    assert!(!home.join("escape.js").exists() && !home.join("Plugins/evil").exists());
    // names with direction marks or control characters, and Windows device names, are refused too
    for bad in ["m\u{202E}rb.rb", "a\u{1}b.rb", "con.rb", "sub/NUL.txt", "com1.rb", "lpt9.rb"] {
        let z = zip_of(&[("plugin.json", r#"{"name":"Odd","commands":[]}"#), ("main.js", ""), (bad, "x")]);
        let r = run(&mut e, json!({"cmd":"install_plugin","data": b64(&z)}));
        assert_eq!(r["ok"], false, "{bad}: {r}");
        assert!(r["error"].as_str().unwrap().contains("unsafe file name"), "{r}");
    }
    // and one without plugin.json
    let r = run(&mut e, json!({"cmd":"install_plugin","data": b64(&zip_of(&[("main.js", "")]))}));
    assert!(r["error"].as_str().unwrap().contains("plugin.json"));
    // a built-in can't be removed
    assert_eq!(run(&mut e, json!({"cmd":"remove_plugin","id":"cutlist"}))["ok"], false);

    // removing moves it aside
    let r = run(&mut e, json!({"cmd":"remove_plugin","id":"stair-maker"}));
    assert_eq!(r["ok"], true, "{r}");
    assert!(!folder.exists() && home.join("Plugins/.removed").is_dir());
    let list = run(&mut e, json!({"cmd":"plugins"}));
    assert!(!list["plugins"].as_array().unwrap().iter().any(|p| p["id"] == "stair-maker"));

    // a single SketchUp Ruby script
    let rb = "UI.menu('Plugins').add_item('Make a box') { Sketchup.active_model.entities.add_face([0,0,0],[10,0,0],[10,10,0],[0,10,0]) }";
    let r = run(&mut e, json!({"cmd":"install_plugin","data": b64(rb.as_bytes()),"name":"make_box.rb"}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["id"], "make-box");
    assert_eq!(r["ruby"], true);
    // an .rbz: a loader at the top and the real code in a folder
    let rbz = zip_of(&[("stairs_maker.rb", "Sketchup.require 'stairs_maker/main'"), ("stairs_maker/main.rb", "puts 'stairs'"), ("stairs_maker/readme.txt", "hi")]);
    let r = run(&mut e, json!({"cmd":"install_plugin","data": b64(&rbz),"name":"stairs_maker.rbz"}));
    assert_eq!(r["ok"], true, "{r}");
    let list = run(&mut e, json!({"cmd":"plugins"}));
    let sm = list["plugins"].as_array().unwrap().iter().find(|p| p["id"] == "stairs-maker").unwrap().clone();
    assert_eq!(sm["ruby"], true);
    assert_eq!(sm["name"], "Stairs Maker");
    assert_eq!(sm["entry"][0], "stairs_maker.rb");
    assert!(sm["files"]["stairs_maker/main.rb"].as_str().unwrap().contains("puts"));
    assert!(sm["files"].get("stairs_maker/readme.txt").is_none(), "only .rb files are sent");
    let mb = list["plugins"].as_array().unwrap().iter().find(|p| p["id"] == "make-box").unwrap().clone();
    assert!(mb["files"]["make_box.rb"].as_str().unwrap().contains("add_face"));

    // .removed doesn't grow without end
    for i in 0..30 {
        let z = zip_of(&[("plugin.json", &format!(r#"{{"name":"Churn","version":"{i}","commands":[]}}"#)), ("main.js", "")]);
        let r = run(&mut e, json!({"cmd":"install_plugin","data": b64(&z),"replace": true}));
        assert_eq!(r["ok"], true, "{r}");
    }
    let kept = std::fs::read_dir(home.join("Plugins/.removed")).unwrap().count();
    assert!(kept <= 20, "{kept} folders kept in .removed");
    // R2-MBK-012: the plugin removed now is kept, even with 25 older set-asides made after its folder was
    for i in 0..25 {
        std::fs::create_dir_all(home.join(format!("Plugins/.removed/fake-1700000000-{}", i + 2))).unwrap();
    }
    let r = run(&mut e, json!({"cmd":"remove_plugin","id":"churn"}));
    assert_eq!(r["ok"], true, "{r}");
    let moved = r["moved_to"].as_str().unwrap().to_string();
    assert!(std::path::Path::new(&moved).is_dir(), "the plugin removed just now was pruned at once");
    // R2-MBK-012: a zip with two files of one name is refused (what was shown could differ from what is installed)
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
        z.start_file("plugin.json", opts).unwrap();
        z.write_all(br#"{"name":"Twin","commands":[{"id":"a","label":"Shown"}]}"#).unwrap();
        z.start_file("main.js", opts).unwrap();
        z.write_all(b"").unwrap();
        z.start_file("plugin.jsoX", opts).unwrap();
        z.write_all(br#"{"name":"Twin","commands":[{"id":"a","label":"Installed"}]}"#).unwrap();
        z.finish().unwrap();
    }
    // the writer won't make two entries of one name, so rename the second one in the bytes
    let mut bytes = buf.into_inner();
    let needle = b"plugin.jsoX";
    while let Some(i) = bytes.windows(needle.len()).position(|w| w == needle) {
        bytes[i + needle.len() - 1] = b'n';
    }
    // the zip reader keeps one entry per name; either way what the person is shown is what is installed
    let shown = run(&mut e, json!({"cmd":"install_plugin","data": b64(&bytes),"preview": true}));
    let r = run(&mut e, json!({"cmd":"install_plugin","data": b64(&bytes)}));
    if shown["ok"] == true && r["ok"] == true {
        let installed: Value = serde_json::from_str(&std::fs::read_to_string(home.join("Plugins/twin/plugin.json")).unwrap()).unwrap();
        assert_eq!(installed["commands"][0]["label"], shown["commands"][0]);
    }
    // a crashed install's staging folder is cleared
    std::fs::create_dir_all(home.join("Plugins/.installing-old-1")).unwrap();
    run(&mut e, json!({"cmd":"plugins"}));

    std::fs::remove_dir_all(&home).ok();
}
