use glam::DVec3;
use riseline::Engine;
use serde_json::{json, Value};

fn run(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn faces(e: &mut Engine) -> Vec<Value> {
    run(e, json!({"cmd":"faces"}))["faces"].as_array().unwrap().clone()
}

fn vec3(v: &Value) -> DVec3 {
    DVec3::new(v[0].as_f64().unwrap(), v[1].as_f64().unwrap(), v[2].as_f64().unwrap())
}

/// Every face normal points away from the solid's center.
fn assert_outward(e: &mut Engine, center: DVec3) {
    for f in faces(e) {
        let n = vec3(&f["normal"]);
        let c = vec3(&f["center"]);
        assert!(n.dot(c - center) > 0.0, "face {} points inward: n={n} c={c}", f["id"]);
    }
}

fn make_box(e: &mut Engine) -> u64 {
    let f = run(e, json!({"cmd":"rect","from":[0,0,0],"to":[8,12,0]}))["face"].as_u64().unwrap();
    run(e, json!({"cmd":"pushpull","face":f,"distance":6}));
    f
}

#[test]
fn ground_rect_faces_up() {
    let mut e = Engine::new();
    run(&mut e, json!({"cmd":"rect","from":[0,0,0],"to":[-3,-2,0]}));
    let f = &faces(&mut e)[0];
    assert_eq!(vec3(&f["normal"]), DVec3::Z);
    assert_eq!(f["area"], 6.0);
}

#[test]
fn pull_rect_makes_closed_box() {
    let mut e = Engine::new();
    make_box(&mut e);
    let info = run(&mut e, json!({"cmd":"info"}));
    assert_eq!(info["faces"], 6);
    assert_eq!(info["edges"], 12);
    assert_eq!(info["vertices"], 8);
    assert_outward(&mut e, DVec3::new(4.0, 6.0, 3.0));
}

#[test]
fn pulling_the_top_again_slides_it() {
    let mut e = Engine::new();
    make_box(&mut e);
    let top = run(&mut e, json!({"cmd":"faces","facing":[0,0,1]}))["faces"][0]["id"].clone();
    run(&mut e, json!({"cmd":"pushpull","face":top,"distance":3}));
    let info = run(&mut e, json!({"cmd":"info"}));
    assert_eq!(info["faces"], 6, "no new faces when sliding");
    assert_eq!(info["bounds"]["max"][2], 9.0);
    assert_outward(&mut e, DVec3::new(4.0, 6.0, 4.5));
}

#[test]
fn window_on_wall_cuts_hole_and_recesses() {
    let mut e = Engine::new();
    make_box(&mut e);
    // front wall is y = 0, facing -Y
    let win = run(&mut e, json!({"cmd":"rect","from":[2,0,2],"to":[4,0,4]}))["face"].clone();
    let front: Vec<Value> = run(&mut e, json!({"cmd":"faces","facing":[0,-1,0]}))["faces"].as_array().unwrap().clone();
    assert_eq!(front.len(), 2, "wall + window");
    let wall = front.iter().find(|f| f["id"] != win).unwrap();
    assert_eq!(wall["holes"], 1);
    assert_eq!(wall["area"], 8.0 * 6.0 - 4.0);
    assert_eq!(vec3(&front.iter().find(|f| f["id"] == win).unwrap()["normal"]), -DVec3::Y);

    let r = run(&mut e, json!({"cmd":"pushpull","face":win,"distance":-0.3}));
    let back = r["face"].clone();
    let all = faces(&mut e);
    assert!(all.iter().all(|f| f["id"] != win), "window face is gone (open recess)");
    let back = all.iter().find(|f| f["id"] == back).unwrap();
    assert_eq!(vec3(&back["normal"]), -DVec3::Y, "recess back faces the street");
    assert!((vec3(&back["center"]).y - 0.3).abs() < 1e-9);
    // recess sides face into the opening
    for id in r["created"].as_array().unwrap() {
        let f = all.iter().find(|f| &f["id"] == id).unwrap();
        let toward_axis = DVec3::new(3.0, 0.15, 3.0) - vec3(&f["center"]);
        assert!(vec3(&f["normal"]).dot(toward_axis) > 0.0, "side {id} faces the wrong way");
    }
}

#[test]
fn bay_pulls_out_of_wall() {
    let mut e = Engine::new();
    make_box(&mut e);
    let bay = run(&mut e, json!({"cmd":"rect","from":[2,0,2],"to":[6,0,5]}))["face"].clone();
    run(&mut e, json!({"cmd":"pushpull","face":bay,"distance":1}));
    let info = run(&mut e, json!({"cmd":"info"}));
    assert_eq!(info["bounds"]["min"][1], -1.0);
    // 6 box faces (wall keeps its hole) + 4 bay sides + bay front
    assert_eq!(info["faces"], 11);
}

#[test]
fn undo_redo_and_failed_commands_dont_stick() {
    let mut e = Engine::new();
    make_box(&mut e);
    run(&mut e, json!({"cmd":"undo"}));
    assert_eq!(run(&mut e, json!({"cmd":"info"}))["faces"], 1);
    run(&mut e, json!({"cmd":"redo"}));
    assert_eq!(run(&mut e, json!({"cmd":"info"}))["faces"], 6);
    let bad = e.exec_json(r#"{"cmd":"pushpull","face":99999,"distance":1}"#);
    assert_eq!(bad["ok"], false);
    let bad = e.exec_json(r#"{"cmd":"polygon","points":[[0,0,0],[1,0,0],[1,1,5],[0,1,0]]}"#);
    assert_eq!(bad["ok"], false);
    assert_eq!(run(&mut e, json!({"cmd":"info"}))["faces"], 6);
}

#[test]
fn groups_keep_houses_separate() {
    let mut e = Engine::new();
    let g1 = run(&mut e, json!({"cmd":"begin_group","name":"House A"}))["group"].clone();
    make_box(&mut e);
    run(&mut e, json!({"cmd":"end_group"}));
    run(&mut e, json!({"cmd":"begin_group","name":"House B"}));
    // shares a wall position with A, but must not weld to it
    let f = run(&mut e, json!({"cmd":"rect","from":[8,0,0],"to":[14,12,0]}))["face"].clone();
    run(&mut e, json!({"cmd":"pushpull","face":f,"distance":4}));
    run(&mut e, json!({"cmd":"end_group"}));
    assert_eq!(run(&mut e, json!({"cmd":"info"}))["faces"], 12);
    run(&mut e, json!({"cmd":"move","ids":[g1],"by":[-1,0,0]}));
    let b = run(&mut e, json!({"cmd":"info"}))["bounds"].clone();
    assert_eq!(b["min"][0], -1.0);
    assert_eq!(b["max"][0], 14.0);
}

#[test]
fn closed_line_makes_a_face() {
    let mut e = Engine::new();
    let r = run(&mut e, json!({"cmd":"line","points":[[0,0,0],[4,0,0],[4,3,0],[0,0,0]]}));
    assert!(r["face"].is_u64());
    assert_eq!(r["edges"].as_array().unwrap().len(), 3);
}

#[test]
fn render_and_export_write_files() {
    let dir = std::env::temp_dir().join(format!("riseline-test-{}", std::process::id()));
    let mut e = Engine::new();
    make_box(&mut e);
    let png = dir.join("box.png").to_string_lossy().to_string();
    run(&mut e, json!({"cmd":"render","out":png,"width":320,"height":200}));
    let img = image::open(&png).unwrap().to_rgb8();
    let distinct: std::collections::HashSet<_> = img.pixels().map(|p| p.0).collect();
    assert!(distinct.len() > 20, "render looks blank");
    let obj = dir.join("box.obj").to_string_lossy().to_string();
    let r = run(&mut e, json!({"cmd":"export_obj","path":obj}));
    assert_eq!(r["triangles"], 12);
    let save = dir.join("box.riseline").to_string_lossy().to_string();
    run(&mut e, json!({"cmd":"save","path":save}));
    let mut e2 = Engine::new();
    run(&mut e2, json!({"cmd":"open","path":save}));
    assert_eq!(run(&mut e2, json!({"cmd":"info"}))["faces"], 6);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn door_at_ground_splits_wall_and_recesses() {
    let mut e = Engine::new();
    make_box(&mut e);
    let door = run(&mut e, json!({"cmd":"rect","from":[2,0,0],"to":[3,0,2.2]}))["face"].clone();
    let front: Vec<Value> = run(&mut e, json!({"cmd":"faces","facing":[0,-1,0]}))["faces"].as_array().unwrap().clone();
    assert_eq!(front.len(), 2, "wall + door");
    let wall = front.iter().find(|f| f["id"] != door).unwrap();
    assert_eq!(wall["holes"], 0, "door touches the edge, so the wall is reshaped, not holed");
    assert_eq!(wall["points"], 8);
    assert!((wall["area"].as_f64().unwrap() - (48.0 - 2.2)).abs() < 1e-6);

    let r = run(&mut e, json!({"cmd":"pushpull","face":door,"distance":-0.3}));
    let all = faces(&mut e);
    assert!(all.iter().all(|f| f["id"] != door), "door base removed: the recess is open");
    let back = all.iter().find(|f| f["id"] == r["face"]).unwrap();
    assert_eq!(vec3(&back["normal"]), -DVec3::Y);
    // the recess floor faces up, its ceiling faces down
    let created: Vec<&Value> = r["created"].as_array().unwrap().iter().map(|id| all.iter().find(|f| &f["id"] == id).unwrap()).collect();
    assert!(created.iter().any(|f| vec3(&f["normal"]) == DVec3::Z));
    assert!(created.iter().any(|f| vec3(&f["normal"]) == -DVec3::Z));
}

#[test]
fn band_across_wall_splits_it_in_two() {
    let mut e = Engine::new();
    make_box(&mut e);
    // full-width band through the middle of the front wall
    run(&mut e, json!({"cmd":"rect","from":[0,0,2],"to":[8,0,3]}));
    let front: Vec<Value> = run(&mut e, json!({"cmd":"faces","facing":[0,-1,0]}))["faces"].as_array().unwrap().clone();
    assert_eq!(front.len(), 3, "below band, band, above band");
    let total: f64 = front.iter().map(|f| f["area"].as_f64().unwrap()).sum();
    assert!((total - 48.0).abs() < 1e-6);
}

#[test]
fn line_across_top_then_raise_ridge_makes_gable() {
    let mut e = Engine::new();
    make_box(&mut e); // 8 x 12 x 6
    // ridge line across the roof, midpoint to midpoint of the short edges
    let r = run(&mut e, json!({"cmd":"line","points":[[4,0,6],[4,12,6]]}));
    assert_eq!(r["faces"].as_array().unwrap().len(), 2, "roof split in two");
    let ridge = r["edges"][0].clone();
    run(&mut e, json!({"cmd":"move","ids":[ridge],"by":[0,0,3]}));
    let info = run(&mut e, json!({"cmd":"info"}));
    assert_eq!(info["bounds"]["max"][2], 9.0);
    // gable ends are pentagons that still face front/back
    let front: Vec<Value> = run(&mut e, json!({"cmd":"faces","facing":[0,-1,0]}))["faces"].as_array().unwrap().clone();
    assert_eq!(front.len(), 1);
    assert_eq!(front[0]["points"], 5);
    assert!((front[0]["area"].as_f64().unwrap() - (48.0 + 12.0)).abs() < 1e-6);
    assert_outward(&mut e, DVec3::new(4.0, 6.0, 3.5));
}

#[test]
fn lines_closing_a_loop_make_a_face() {
    let mut e = Engine::new();
    run(&mut e, json!({"cmd":"line","points":[[0,0,0],[5,0,0],[5,4,0]]}));
    let r = run(&mut e, json!({"cmd":"line","points":[[5,4,0],[0,4,0],[0,0,0]]}));
    assert_eq!(r["faces"].as_array().unwrap().len(), 1);
    assert_eq!(run(&mut e, json!({"cmd":"info"}))["faces"], 1);
    // a line through the middle divides it into two faces
    let r = run(&mut e, json!({"cmd":"line","points":[[2,0,0],[2,4,0]]}));
    assert_eq!(r["faces"].as_array().unwrap().len(), 2);
    assert_eq!(run(&mut e, json!({"cmd":"info"}))["faces"], 2);
}

#[test]
fn line_ending_mid_edge_splits_it() {
    let mut e = Engine::new();
    run(&mut e, json!({"cmd":"line","points":[[0,0,0],[10,0,0]]}));
    run(&mut e, json!({"cmd":"line","points":[[4,0,0],[4,5,0]]}));
    assert_eq!(run(&mut e, json!({"cmd":"info"}))["edges"], 3);
}

#[test]
fn rotate_scale_duplicate_array() {
    let mut e = Engine::new();
    let g = run(&mut e, json!({"cmd":"begin_group","name":"A"}))["group"].clone();
    make_box(&mut e);
    run(&mut e, json!({"cmd":"end_group"}));
    run(&mut e, json!({"cmd":"rotate","ids":[g],"center":[0,0,0],"angle":90}));
    let b = run(&mut e, json!({"cmd":"info"}))["bounds"].clone();
    assert!((b["min"][0].as_f64().unwrap() + 12.0).abs() < 1e-9 && (b["max"][1].as_f64().unwrap() - 8.0).abs() < 1e-9);
    run(&mut e, json!({"cmd":"scale","ids":[g],"center":[0,0,0],"factor":0.5}));
    let b = run(&mut e, json!({"cmd":"info"}))["bounds"].clone();
    assert!((b["max"][2].as_f64().unwrap() - 3.0).abs() < 1e-9);
    let r = run(&mut e, json!({"cmd":"array","ids":[g],"by":[0,10,0],"count":3}));
    assert_eq!(r["ids"].as_array().unwrap().len(), 3);
    assert_eq!(run(&mut e, json!({"cmd":"info"}))["faces"], 24);
    // mirror keeps faces pointing out
    run(&mut e, json!({"cmd":"scale","ids":[g],"center":[0,0,0],"factor":[-1,1,1]}));
    let faces = run(&mut e, json!({"cmd":"faces","group":g}))["faces"].as_array().unwrap().clone();
    let c = faces.iter().map(|f| vec3(&f["center"])).sum::<DVec3>() / faces.len() as f64;
    for f in faces {
        assert!(vec3(&f["normal"]).dot(vec3(&f["center"]) - c) > 0.0);
    }
}

#[test]
fn pulling_one_corner_up_folds_the_face() {
    let mut e = Engine::new();
    let f = run(&mut e, json!({"cmd":"rect","from":[0,0,0],"to":[4,4,0]}))["face"].clone();
    let _ = f;
    // grab the corner vertex and lift it: the square can't stay flat
    let v = e.model.vertices.iter().find(|(_, v)| v.p == DVec3::new(4.0, 4.0, 0.0)).map(|(k, _)| *k).unwrap();
    run(&mut e, json!({"cmd":"move","ids":[v],"by":[0,0,2]}));
    let info = run(&mut e, json!({"cmd":"info"}));
    assert_eq!(info["faces"], 2, "folded into two flat triangles");
    assert!(e.model.edges.values().any(|e| e.soft), "fold line is soft");
}

#[test]
fn circle_pulls_into_smooth_cylinder() {
    let mut e = Engine::new();
    let f = run(&mut e, json!({"cmd":"circle","center":[0,0,0],"radius":1,"segments":24}))["face"].clone();
    run(&mut e, json!({"cmd":"pushpull","face":f,"distance":2}));
    let info = run(&mut e, json!({"cmd":"info"}));
    assert_eq!(info["faces"], 26);
    let soft = e.model.edges.values().filter(|e| e.soft).count();
    assert_eq!(soft, 24, "every vertical seam is soft");
    assert_outward(&mut e, DVec3::new(0.0, 0.0, 1.0));
}

#[test]
fn arc_closed_with_a_line_makes_a_face() {
    let mut e = Engine::new();
    run(&mut e, json!({"cmd":"arc","start":[0,0,0],"end":[4,0,0],"bulge":1}));
    let r = run(&mut e, json!({"cmd":"line","points":[[4,0,0],[0,0,0]]}));
    assert_eq!(r["faces"].as_array().unwrap().len(), 1);
    let b = run(&mut e, json!({"cmd":"info"}))["bounds"].clone();
    assert!((b["max"][1].as_f64().unwrap().abs().max(b["min"][1].as_f64().unwrap().abs()) - 1.0).abs() < 1e-6);
}

#[test]
fn offset_in_and_out() {
    let mut e = Engine::new();
    let f = run(&mut e, json!({"cmd":"rect","from":[0,0,0],"to":[10,6,0]}))["face"].clone();
    let inner = run(&mut e, json!({"cmd":"offset","face":f,"distance":1}))["face"].clone();
    let fs = faces(&mut e);
    let a = |id: &Value| fs.iter().find(|x| &x["id"] == id).unwrap()["area"].as_f64().unwrap();
    assert!((a(&inner) - 8.0 * 4.0).abs() < 1e-6);
    assert!((a(&f) - (60.0 - 32.0)).abs() < 1e-6);
    let ring = run(&mut e, json!({"cmd":"offset","face":f,"distance":-0.5}))["face"].clone();
    let fs = faces(&mut e);
    let a = |id: &Value| fs.iter().find(|x| &x["id"] == id).unwrap()["area"].as_f64().unwrap();
    assert!((a(&ring) - (11.0 * 7.0 - 60.0)).abs() < 1e-6);
}

#[test]
fn follow_me_sweeps_trim_along_a_path() {
    let mut e = Engine::new();
    // 0.3 x 0.3 square profile standing at the origin, facing +X
    let f = run(&mut e, json!({"cmd":"polygon","points":[[0,0,0],[0,0.3,0],[0,0.3,0.3],[0,0,0.3]]}))["face"].clone();
    let r = run(&mut e, json!({"cmd":"follow_me","face":f,"points":[[0,0,0],[5,0,0],[5,5,0]]}));
    // 4 sides x 2 segments + end cap
    assert_eq!(r["faces"].as_array().unwrap().len(), 9);
    let b = run(&mut e, json!({"cmd":"info"}))["bounds"].clone();
    assert!((b["max"][0].as_f64().unwrap() - 5.0).abs() < 0.31);
    assert!((b["max"][1].as_f64().unwrap() - 5.0).abs() < 1e-6);
}

#[test]
fn follow_me_around_a_roof_makes_a_closed_cornice() {
    let mut e = Engine::new();
    make_box(&mut e);
    let top = run(&mut e, json!({"cmd":"faces","facing":[0,0,1]}))["faces"][0]["id"].clone();
    // small profile at the corner (0,0,6), perpendicular to the front edge
    let prof = run(&mut e, json!({"cmd":"polygon","points":[[0,0,6],[0,-0.4,6],[0,-0.4,6.3],[0,0,6.3]]}))["face"].clone();
    let r = run(&mut e, json!({"cmd":"follow_me","face":prof,"around":top}));
    assert_eq!(r["faces"].as_array().unwrap().len(), 16, "4 profile sides x 4 roof edges, closed");
    assert!(faces(&mut e).iter().all(|f| f["id"] != prof), "profile used up");
}
