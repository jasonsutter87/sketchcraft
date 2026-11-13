use glam::DVec3;
use sketchcraft::Engine;
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
    let dir = std::env::temp_dir().join(format!("sketchcraft-test-{}", std::process::id()));
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
    let save = dir.join("box.sketchcraft").to_string_lossy().to_string();
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

#[test]
fn components_place_edit_explode() {
    let mut e = Engine::new();
    let g = run(&mut e, json!({"cmd":"begin_group","name":"Door"}))["group"].clone();
    let f = run(&mut e, json!({"cmd":"rect","from":[0,0,0],"to":[1,0.1,0]}))["face"].clone();
    run(&mut e, json!({"cmd":"pushpull","face":f,"distance":2.1}));
    run(&mut e, json!({"cmd":"end_group"}));
    let r = run(&mut e, json!({"cmd":"make_component","ids":[g],"name":"Door"}));
    let first = r["instance"].clone();
    assert_eq!(run(&mut e, json!({"cmd":"info"}))["faces"], 0, "geometry moved into the component");
    run(&mut e, json!({"cmd":"place","component":"Door","at":[5,0,0],"rotate":90}));
    run(&mut e, json!({"cmd":"duplicate","ids":[first],"by":[0,5,0]}));
    assert_eq!(e.model.world_faces().len(), 18);
    // edit through one placement: every placement grows
    run(&mut e, json!({"cmd":"edit","id":first}));
    let top = run(&mut e, json!({"cmd":"faces","facing":[0,0,1]}))["faces"][0]["id"].clone();
    run(&mut e, json!({"cmd":"pushpull","face":top,"distance":0.4}));
    run(&mut e, json!({"cmd":"close"}));
    let b = run(&mut e, json!({"cmd":"info"}))["bounds"].clone();
    assert!((b["max"][2].as_f64().unwrap() - 2.5).abs() < 1e-9);
    assert!(e.model.world_faces().iter().all(|f| f.outer.iter().all(|p| p.z <= 2.5 + 1e-9)));
    // explode one back to plain geometry
    run(&mut e, json!({"cmd":"explode","id":first}));
    assert_eq!(run(&mut e, json!({"cmd":"info"}))["faces"], 6);
    assert_eq!(e.model.world_faces().len(), 18);
}

#[test]
fn hide_tags_and_soften() {
    let mut e = Engine::new();
    let g = run(&mut e, json!({"cmd":"begin_group","name":"A"}))["group"].clone();
    make_box(&mut e);
    run(&mut e, json!({"cmd":"end_group"}));
    run(&mut e, json!({"cmd":"tag","ids":[g],"tag":"Trees"}));
    run(&mut e, json!({"cmd":"tag_visible","tag":"Trees","visible":false}));
    assert_eq!(e.model.world_faces().len(), 0);
    run(&mut e, json!({"cmd":"tag_visible","tag":"Trees","visible":true}));
    run(&mut e, json!({"cmd":"hide","ids":[g]}));
    assert_eq!(e.model.world_faces().len(), 0);
    run(&mut e, json!({"cmd":"unhide"}));
    assert_eq!(e.model.world_faces().len(), 6);
    // open the group, split the roof, then soften the now-coplanar seam
    run(&mut e, json!({"cmd":"edit","id":g}));
    run(&mut e, json!({"cmd":"line","points":[[0,6,6],[8,6,6]]}));
    run(&mut e, json!({"cmd":"close"}));
    let r = run(&mut e, json!({"cmd":"soften","coplanar":true}));
    assert_eq!(r["edges"], 1);
    assert_eq!(e.model.world_edges().iter().filter(|x| !x.soft).count(), 12 + 2, "box edges plus the two wall splits, not the seam");
}

#[test]
fn every_asset_builds_and_places() {
    let mut e = Engine::new();
    let list = run(&mut e, json!({"cmd":"assets"}))["assets"].as_array().unwrap().clone();
    assert!(list.len() >= 9);
    for (i, a) in list.iter().enumerate() {
        let r = e.exec_json(&json!({"cmd":"insert","asset":a["key"],"at":[i as f64 * 6.0, 0, 0]}).to_string());
        assert_eq!(r["ok"], true, "{}: {r}", a["key"]);
    }
    // a second door reuses the first door's component
    run(&mut e, json!({"cmd":"insert","asset":"door","at":[0,10,0]}));
    assert_eq!(run(&mut e, json!({"cmd":"components"}))["components"].as_array().unwrap().len(), list.len());
    assert!(e.model.world_faces().len() > 200);
}

#[test]
fn glb_stl_and_obj_round_trip() {
    let dir = std::env::temp_dir().join(format!("sketchcraft-io-{}", std::process::id()));
    let p = |n: &str| dir.join(n).to_string_lossy().to_string();
    let mut e = Engine::new();
    make_box(&mut e);
    run(&mut e, json!({"cmd":"insert","asset":"tree","at":[12,0,0]}));
    let r = run(&mut e, json!({"cmd":"export_glb","path":p("m.glb")}));
    let glb = std::fs::read(p("m.glb")).unwrap();
    assert_eq!(&glb[0..4], b"glTF");
    assert_eq!(u32::from_le_bytes(glb[8..12].try_into().unwrap()) as usize, glb.len());
    assert!(r["triangles"].as_u64().unwrap() > 12);
    let r = run(&mut e, json!({"cmd":"export_stl","path":p("m.stl"),"scale":1000}));
    let stl = std::fs::read(p("m.stl")).unwrap();
    assert_eq!(stl.len(), 84 + 50 * r["triangles"].as_u64().unwrap() as usize);
    run(&mut e, json!({"cmd":"export_obj","path":p("m.obj")}));
    let mut e2 = Engine::new();
    let r = run(&mut e2, json!({"cmd":"import_obj","path":p("m.obj"),"up":"z"}));
    assert!(r["faces"].as_u64().unwrap() > 12);
    assert_eq!(r["skipped"], 0);
    let b = run(&mut e2, json!({"cmd":"info"}))["bounds"].clone();
    assert!((b["max"][2].as_f64().unwrap() - 6.0).abs() < 1e-3 || b["max"][2].as_f64().unwrap() > 4.0);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn crossing_lines_on_a_face_make_four_panes() {
    let mut e = Engine::new();
    run(&mut e, json!({"cmd":"rect","from":[0,0,0],"to":[4,3,0]}));
    run(&mut e, json!({"cmd":"line","points":[[2,0,0],[2,3,0]]}));
    run(&mut e, json!({"cmd":"line","points":[[0,2,0],[4,2,0]]}));
    let fs = faces(&mut e);
    assert_eq!(fs.len(), 4, "a mullion and a transom make four panes");
    let total: f64 = fs.iter().map(|f| f["area"].as_f64().unwrap()).sum();
    assert!((total - 12.0).abs() < 1e-9, "no overlapping faces");
}

#[test]
fn builders_walls_stairs_roofs_columns() {
    let mut e = Engine::new();
    // a closed 6 x 4 room
    let room = run(&mut e, json!({"cmd":"wall","points":[[0,0,0],[6,0,0],[6,4,0],[0,4,0]],"closed":true,"thickness":0.2,"height":2.7}))["group"].clone();
    let fs = run(&mut e, json!({"cmd":"faces","group":room}))["faces"].as_array().unwrap().clone();
    let top: f64 = fs.iter().filter(|f| f["normal"][2] == 1.0).map(|f| f["area"].as_f64().unwrap()).sum();
    // ring of walls: (6.2 x 4.2) - (5.8 x 3.8)
    assert!((top - (6.2 * 4.2 - 5.8 * 3.8)).abs() < 1e-6, "wall top area {top}");
    // an open L-shaped wall
    run(&mut e, json!({"cmd":"wall","points":[[10,0,0],[14,0,0],[14,3,0]]}));
    // stairs up 2.7 m: 15 risers of 18 cm
    let s = run(&mut e, json!({"cmd":"stairs","at":[0,6,0],"rise":2.7,"direction":90}));
    assert_eq!(s["risers"], 15);
    // a gable roof on a box top
    let b = run(&mut e, json!({"cmd":"rect","from":[20,0,0],"to":[28,6,0]}))["face"].clone();
    run(&mut e, json!({"cmd":"pushpull","face":b,"distance":3}));
    let top = run(&mut e, json!({"cmd":"pick","at":[24,3,3],"facing":[0,0,1]}))["face"].clone();
    run(&mut e, json!({"cmd":"roof","face":top,"kind":"gable","pitch":30}));
    let bmax = run(&mut e, json!({"cmd":"info"}))["bounds"]["max"][2].as_f64().unwrap();
    assert!(bmax > 3.0 + 3.0 * (30f64).to_radians().tan() - 0.01, "ridge rises: {bmax}");
    run(&mut e, json!({"cmd":"roof","face":top,"kind":"hip","pitch":25}));
    run(&mut e, json!({"cmd":"column","at":[30,0,0],"round":true}));
}

#[test]
fn image_materials_paint_and_project() {
    let mut e = Engine::new();
    make_box(&mut e);
    // 1x1 PNG
    let png = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    run(&mut e, json!({"cmd":"texture","name":"Brick","data":png,"width":1.2,"height":0.6,"color":[0.6,0.3,0.2]}));
    let front = run(&mut e, json!({"cmd":"pick","at":[4,0,3],"facing":[0,-1,0]}))["face"].clone();
    run(&mut e, json!({"cmd":"paint","ids":[front],"material":"Brick"}));
    let scene = sketchcraft::scene::build(&e.model, None);
    let tri_tex = scene["triTex"].as_array().unwrap();
    assert_eq!(tri_tex.iter().filter(|t| t.as_i64() == Some(0)).count(), 2, "the painted wall's two triangles use texture 0");
    // tile size: an 8 m wide wall spans 8 / 1.2 tiles
    let uvs: Vec<f64> = scene["uvs"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    let i = tri_tex.iter().position(|t| t.as_i64() == Some(0)).unwrap();
    let us: Vec<f64> = (0..3).map(|k| uvs[(i * 3 + k) * 2]).collect();
    let span = us.iter().cloned().fold(f64::MIN, f64::max) - us.iter().cloned().fold(f64::MAX, f64::min);
    assert!((span - 8.0 / 1.2).abs() < 1e-3, "u span {span}");
    // project it from the front instead
    run(&mut e, json!({"cmd":"texture","name":"Brick","project":[0,1,0]}));
    assert!(e.model.materials["Brick"].texture.as_ref().unwrap().project.is_some());
    // unknown materials and bad data are refused
    assert_eq!(e.exec_json(&json!({"cmd":"paint","ids":[front],"material":"Nope"}).to_string())["ok"], false);
    assert_eq!(e.exec_json(&json!({"cmd":"texture","name":"X","data":"hello"}).to_string())["ok"], false);
}

#[test]
fn big_obj_imports_fast_as_one_component() {
    // a 200 x 200 grid of quads, every face written twice (front and back,
    // like many double-sided exports), with an .mtl next to it
    let dir = std::env::temp_dir().join(format!("sketchcraft-bigobj-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("grid.mtl"), "newmtl Paint\nKd 1 0 0\n").unwrap();
    let mut s = String::from("mtllib grid.mtl\no Ground\nusemtl Paint\n");
    let n = 200;
    for y in 0..=n {
        for x in 0..=n {
            s += &format!("v {} 0 {}\n", x as f64 * 0.1, -(y as f64) * 0.1);
        }
    }
    let at = |x: usize, y: usize| y * (n + 1) + x + 1;
    for y in 0..n {
        for x in 0..n {
            let (a, b, c, d) = (at(x, y), at(x + 1, y), at(x + 1, y + 1), at(x, y + 1));
            s += &format!("f {a} {b} {c} {d}\nf {d} {c} {b} {a}\n");
        }
    }
    let path = dir.join("Big+Grid.obj");
    std::fs::write(&path, s).unwrap();
    let mut e = Engine::new();
    let t = std::time::Instant::now();
    let r = run(&mut e, json!({"cmd":"import_obj","path":path.to_string_lossy()}));
    assert!(t.elapsed().as_secs_f64() < 10.0, "took {:?}", t.elapsed());
    assert_eq!(r["faces"], n * n, "the back copies are dropped");
    let info = run(&mut e, json!({"cmd":"info"}));
    assert_eq!(info["faces"], 0, "the mesh lives in a component, not the model");
    let scene = sketchcraft::scene::build(&e.model, None);
    assert_eq!(scene["components"][0]["name"], "Big Grid");
    assert_eq!(scene["colors"][0].as_f64(), Some(1.0), "color from the .mtl");
    // the grid is flat: no hard edges inside it, only its border
    assert_eq!(scene["edges"].as_array().unwrap().len() / 6, 4 * n);
    // drawing next to it stays quick
    let t = std::time::Instant::now();
    for i in 0..20 {
        run(&mut e, json!({"cmd":"rect","from":[30 + i,0,0],"to":[30.5 + i as f64,1,0]}));
    }
    assert!(t.elapsed().as_secs_f64() < 2.0, "20 edits took {:?}", t.elapsed());
    std::fs::remove_dir_all(dir).ok();
}

fn box_group(e: &mut Engine, from: [f64; 3], to: [f64; 3], name: &str) -> u64 {
    let g = run(e, json!({"cmd":"begin_group","name":name}))["group"].as_u64().unwrap();
    let f = run(e, json!({"cmd":"rect","from":[from[0],from[1],from[2]],"to":[to[0],to[1],from[2]]}))["face"].as_u64().unwrap();
    // pull toward +z whichever way the new face looks
    let up = e.model.normal(f as _).z > 0.0;
    let h = to[2] - from[2];
    run(e, json!({"cmd":"pushpull","face":f,"distance": if up { h } else { -h }}));
    run(e, json!({"cmd":"end_group"}));
    g
}

fn volume(e: &mut Engine, id: u64) -> f64 {
    let r = run(e, json!({"cmd":"volume","id":id}));
    assert_eq!(r["solid"], true, "{id} should be a solid: {r}");
    r["volume"].as_f64().unwrap()
}

#[test]
fn solid_tools_union_subtract_intersect_split() {
    let close = |a: f64, b: f64| (a - b).abs() < 1e-6;
    // two 2 m cubes overlapping by 1 m along x: overlap is 1 x 2 x 2 = 4 m³
    let fresh = || {
        let mut e = Engine::new();
        let a = box_group(&mut e, [0.0, 0.0, 0.0], [2.0, 2.0, 2.0], "A");
        let b = box_group(&mut e, [1.0, 0.0, 0.0], [3.0, 2.0, 2.0], "B");
        (e, a, b)
    };
    let (mut e, a, b) = fresh();
    assert!(close(volume(&mut e, a), 8.0));
    let g = run(&mut e, json!({"cmd":"solid","op":"union","ids":[a,b]}))["group"].as_u64().unwrap();
    assert!(close(volume(&mut e, g), 12.0), "union");
    // the merged block is a clean box: 6 faces
    assert_eq!(e.model.faces.values().filter(|f| f.group == Some(g as u32)).count(), 6);

    let (mut e, a, b) = fresh();
    let g = run(&mut e, json!({"cmd":"solid","op":"subtract","ids":[a,b]}))["group"].as_u64().unwrap();
    assert!(close(volume(&mut e, g), 4.0), "B minus A");
    assert!(e.model.groups.get(&(a as u32)).is_none(), "subtract removes the cutter");

    let (mut e, a, b) = fresh();
    let g = run(&mut e, json!({"cmd":"solid","op":"trim","ids":[a,b]}))["group"].as_u64().unwrap();
    assert!(close(volume(&mut e, g), 4.0));
    assert!(e.model.groups.get(&(a as u32)).is_some(), "trim keeps the cutter");

    let (mut e, a, b) = fresh();
    let g = run(&mut e, json!({"cmd":"solid","op":"intersect","ids":[a,b]}))["group"].as_u64().unwrap();
    assert!(close(volume(&mut e, g), 4.0), "overlap");

    let (mut e, a, b) = fresh();
    let r = run(&mut e, json!({"cmd":"solid","op":"split","ids":[a,b]}));
    let gs: Vec<u64> = r["groups"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    assert_eq!(gs.len(), 3);
    let total: f64 = gs.iter().map(|g| volume(&mut e, *g)).sum();
    assert!(close(total, 12.0));
}

#[test]
fn subtracting_a_cylinder_leaves_a_hole_that_is_still_solid() {
    let mut e = Engine::new();
    let slab = box_group(&mut e, [0.0, 0.0, 0.0], [4.0, 4.0, 0.5], "Slab");
    let drill = run(&mut e, json!({"cmd":"begin_group","name":"Drill"}))["group"].as_u64().unwrap();
    let c = run(&mut e, json!({"cmd":"circle","center":[2,2,-1],"normal":[0,0,1],"radius":0.5,"segments":24}))["face"].as_u64().unwrap();
    let up = e.model.normal(c as _).z > 0.0;
    run(&mut e, json!({"cmd":"pushpull","face":c,"distance": if up { 3.0 } else { -3.0 }}));
    run(&mut e, json!({"cmd":"end_group"}));
    let g = run(&mut e, json!({"cmd":"solid","op":"subtract","ids":[drill,slab]}))["group"].as_u64().unwrap();
    let v = volume(&mut e, g);
    let hole = 24.0 / 2.0 * 0.5f64.powi(2) * (2.0 * std::f64::consts::PI / 24.0).sin() * 0.5;
    assert!((v - (8.0 - hole)).abs() < 1e-6, "{v}");
    // top face kept whole, with a hole in it
    let top = e.model.faces.values().filter(|f| f.group == Some(g as u32) && !f.holes.is_empty()).count();
    assert_eq!(top, 2, "top and bottom faces each get the hole");
    // and it can be cut again
    let b2 = box_group(&mut e, [3.0, 3.0, -1.0], [5.0, 5.0, 1.0], "Corner");
    let g2 = run(&mut e, json!({"cmd":"solid","op":"subtract","ids":[b2,g]}))["group"].as_u64().unwrap();
    assert!((volume(&mut e, g2) - (8.0 - hole - 0.5)).abs() < 1e-6);
}

#[test]
fn open_shapes_are_not_solids() {
    let mut e = Engine::new();
    let g = run(&mut e, json!({"cmd":"begin_group","name":"Sheet"}))["group"].as_u64().unwrap();
    run(&mut e, json!({"cmd":"rect","from":[0,0,0],"to":[1,1,0]}));
    run(&mut e, json!({"cmd":"end_group"}));
    let b = box_group(&mut e, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0], "B");
    assert_eq!(run(&mut e, json!({"cmd":"volume","id":g}))["solid"], false);
    let r = e.exec_json(&json!({"cmd":"solid","op":"union","ids":[g,b]}).to_string());
    assert_eq!(r["ok"], false);
    assert!(r["error"].as_str().unwrap().contains("isn't a solid"));
}

#[test]
fn a_machined_bracket_takes_seven_cuts_in_a_row() {
    // plate + boss, then a bore, a slot and four bolt holes: every result
    // has to stay a closed solid for the next cut, and pieces must not pile up
    let cyl = |e: &mut Engine, c: [f64; 3], r: f64, h: f64| -> u64 {
        let g = run(e, json!({"cmd":"begin_group","name":"Cyl"}))["group"].as_u64().unwrap();
        let f = run(e, json!({"cmd":"circle","center":c,"radius":r,"segments":32}))["face"].as_u64().unwrap();
        let up = e.model.normal(f as _).z > 0.0;
        run(e, json!({"cmd":"pushpull","face":f,"distance": if up { h } else { -h }}));
        run(e, json!({"cmd":"end_group"}));
        g
    };
    let mut e = Engine::new();
    let plate = box_group(&mut e, [0.0, 0.0, 0.0], [4.0, 2.5, 0.5], "Bracket");
    let boss = cyl(&mut e, [1.2, 1.25, 0.5], 0.8, 1.0);
    let mut u = run(&mut e, json!({"cmd":"solid","op":"union","ids":[plate,boss]}))["group"].as_u64().unwrap();
    let bore = cyl(&mut e, [1.2, 1.25, -1.0], 0.45, 3.0);
    u = run(&mut e, json!({"cmd":"solid","op":"subtract","ids":[bore,u]}))["group"].as_u64().unwrap();
    let slot = box_group(&mut e, [2.6, 0.9, -1.0], [3.6, 1.6, 2.0], "Slot");
    u = run(&mut e, json!({"cmd":"solid","op":"subtract","ids":[slot,u]}))["group"].as_u64().unwrap();
    for x in [0.35, 3.65] {
        for y in [0.35, 2.15] {
            let h = cyl(&mut e, [x, y, -1.0], 0.12, 3.0);
            u = run(&mut e, json!({"cmd":"solid","op":"subtract","ids":[h,u]}))["group"].as_u64().unwrap();
        }
    }
    let ngon = |r: f64| 16.0 * r * r * (2.0 * std::f64::consts::PI / 32.0).sin();
    let expect = 5.0 + ngon(0.8) * 1.0 - ngon(0.45) * 1.5 - 0.35 - 4.0 * ngon(0.12) * 0.5;
    let v = volume(&mut e, u);
    assert!((v - expect).abs() < 1e-6, "{v} vs {expect}");
    let faces = e.model.faces.values().filter(|f| f.group == Some(u as u32)).count();
    assert!(faces < 250, "faces stay merged: {faces}");
}

#[test]
fn intersect_faces_splits_where_shapes_cross() {
    // a box group with a thin slab pushed through it: intersecting draws the
    // ring where the slab passes through the box's walls
    let mut e = Engine::new();
    let b = box_group(&mut e, [0.0, 0.0, 0.0], [2.0, 2.0, 2.0], "Box");
    let s = box_group(&mut e, [-1.0, -1.0, 0.9], [3.0, 3.0, 1.1], "Slab");
    let faces_before = e.model.faces.values().filter(|f| f.group == Some(b as u32)).count();
    let r = run(&mut e, json!({"cmd":"intersect","ids":[b, s]}));
    assert!(r["lines"].as_u64().unwrap() >= 8, "{r}");
    let faces_after = e.model.faces.values().filter(|f| f.group == Some(b as u32)).count();
    // each of the box's 4 walls is cut twice (slab top and bottom): 4 -> 12 wall faces
    assert_eq!(faces_after, faces_before + 8, "walls split by the slab");
    // the slab's top and bottom get the square where the box passes through
    let slab_holes = e.model.faces.values().filter(|f| f.group == Some(s as u32)).count();
    assert!(slab_holes > 6);
}

#[test]
fn flip_along_mirrors_in_place() {
    let mut e = Engine::new();
    let g = run(&mut e, json!({"cmd":"begin_group","name":"L"}))["group"].as_u64().unwrap();
    // an L shape, wider at x = 0
    run(&mut e, json!({"cmd":"polygon","points":[[0,0,0],[3,0,0],[3,1,0],[1,1,0],[1,2,0],[0,2,0]]}));
    run(&mut e, json!({"cmd":"end_group"}));
    run(&mut e, json!({"cmd":"flip","ids":[g],"axis":"red"}));
    let f = e.model.faces.values().find(|f| f.group == Some(g as u32)).unwrap();
    let pts = e.model.pts(&f.outer);
    // the tall part moved to x in [2, 3]; bounds unchanged
    assert!(pts.iter().any(|p| (p.x - 2.0).abs() < 1e-9 && (p.y - 2.0).abs() < 1e-9));
    assert!(pts.iter().all(|p| p.x >= -1e-9 && p.x <= 3.0 + 1e-9));
    let before = f.outer.len();
    assert_eq!(before, 6);
    // faces still face the same way after a mirror
    assert!(e.model.normal(*e.model.faces.iter().find(|(_, f)| f.group == Some(g as u32)).unwrap().0).z.abs() > 0.99);
}

#[test]
fn lines_closing_a_loop_on_a_face_cut_a_new_face() {
    // like drawing a window on a wall with the Line tool
    let mut e = Engine::new();
    run(&mut e, json!({"cmd":"polygon","points":[[-1,-1,1],[3,-1,1],[3,3,1],[-1,3,1]]}));
    run(&mut e, json!({"cmd":"line","points":[[0,0,1],[2,0,1],[2,2,1],[0,2,1],[0,0,1]]}));
    assert_eq!(e.model.faces.len(), 2);
    let holed = e.model.faces.values().filter(|f| f.holes.len() == 1).count();
    assert_eq!(holed, 1, "the wall gets the window as a hole");
}

#[test]
fn section_cut_fills_walls_with_their_openings() {
    // a 4 x 4 room of 0.2 m walls, 2.7 m tall: cut level at 1.2 m gives one
    // filled ring (outline with the room as a hole)
    let mut e = Engine::new();
    run(&mut e, json!({"cmd":"wall","points":[[0,0,0],[4,0,0],[4,4,0],[0,4,0]],"closed":true,"thickness":0.2,"height":2.7}));
    let dir = std::env::temp_dir().join(format!("sketchcraft-sec-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let svg = dir.join("plan.svg").to_string_lossy().to_string();
    let r = run(&mut e, json!({"cmd":"section","origin":[0,0,1.2],"normal":[0,0,-1],"path":svg}));
    assert_eq!(r["regions"], 1, "{r}");
    // fill area = ring area: 4.2² - 3.8² = 3.2 m²
    let f: Vec<f64> = r["fill"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
    let area: f64 = f.chunks(9).map(|t| {
        let (a, b, c) = (glam::DVec3::new(t[0], t[1], t[2]), glam::DVec3::new(t[3], t[4], t[5]), glam::DVec3::new(t[6], t[7], t[8]));
        (b - a).cross(c - a).length() / 2.0
    }).sum();
    assert!((area - 3.2).abs() < 1e-6, "{area}");
    let text = std::fs::read_to_string(&svg).unwrap();
    assert!(text.starts_with("<svg") && text.contains("evenodd") && text.matches("<path").count() == 1);
    let dxf = dir.join("plan.dxf").to_string_lossy().to_string();
    run(&mut e, json!({"cmd":"section","origin":[0,0,1.2],"normal":[0,0,-1],"path":dxf}));
    let d = std::fs::read_to_string(&dxf).unwrap();
    assert_eq!(d.matches("\nLINE\n").count(), 8, "outer and inner square");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn sandbox_contours_smoove_stamp_drape_flip() {
    let mut e = Engine::new();
    // two contour rings: a 20 m square at 0 m and a 10 m square at 2 m
    let lines = |z: f64, h: f64| json!({"cmd":"line","points":[[-h,-h,z],[h,-h,z],[h,h,z],[-h,h,z],[-h,-h,z]]});
    run(&mut e, lines(0.0, 10.0));
    run(&mut e, lines(2.0, 5.0));
    let ids: Vec<u32> = e.model.edges.keys().copied().collect();
    // drawing closed rings made faces: the contours are the edges only
    let g = run(&mut e, json!({"cmd":"terrain_from_contours","ids":ids,"spacing":2.5}))["group"].as_u64().unwrap() as u32;
    let tris = e.model.faces.values().filter(|f| f.group == Some(g)).count();
    assert!(tris > 20, "{tris}");
    // every face is a triangle facing up; inside edges are soft, the rim is not
    assert!(e.model.faces.iter().filter(|(_, f)| f.group == Some(g)).all(|(id, f)| f.outer.len() == 3 && e.model.normal(*id).z > 0.0));
    let rim = e.model.edges.values().filter(|x| x.group == Some(g) && !x.soft).count();
    assert!(rim >= 4 && rim < tris);
    let top = |e: &Engine| e.model.vertices.values().map(|v| v.p.z).fold(f64::MIN, f64::max);
    assert!((top(&e) - 2.0).abs() < 1e-9);

    // smoove the middle up (the inner ring sits 5-7 m out)
    run(&mut e, json!({"cmd":"smoove","group":g,"at":[0,0,0],"radius":12,"height":4}));
    assert!(top(&e) > 4.0);

    // stamp a 4 m pad at z = 1 in the middle: everything inside is flat at 1
    let pad = run(&mut e, json!({"cmd":"polygon","points":[[-2,-2,1],[2,-2,1],[2,2,1],[-2,2,1]]}))["face"].as_u64().unwrap();
    run(&mut e, json!({"cmd":"stamp","group":g,"face":pad,"offset":2}));
    let inside: Vec<f64> = e.model.vertices.values().filter(|v| v.group == Some(g) && v.p.x.abs() <= 2.0 && v.p.y.abs() <= 2.0).map(|v| v.p.z).collect();
    assert!(!inside.is_empty() && inside.iter().all(|z| (z - 1.0).abs() < 1e-9), "{inside:?}");

    // drape a path across the terrain: it splits faces
    let before = e.model.faces.values().filter(|f| f.group == Some(g)).count();
    let path = run(&mut e, json!({"cmd":"line","points":[[-9,-7,10],[9,-7,10]]}))["edges"][0].as_u64().unwrap();
    let r = run(&mut e, json!({"cmd":"drape","group":g,"ids":[path]}));
    assert!(r["lines"].as_u64().unwrap() > 0);
    assert!(e.model.faces.values().filter(|f| f.group == Some(g)).count() > before);

    // flip an inside edge of a fresh grid
    let grid = run(&mut e, json!({"cmd":"terrain_grid","at":[50,0,0],"width":2,"depth":1,"spacing":1}))["group"].as_u64().unwrap() as u32;
    let diag = *e.model.edges.iter().find(|(_, x)| x.group == Some(grid) && x.soft).unwrap().0;
    run(&mut e, json!({"cmd":"flip_edge","edge":diag}));
    assert_eq!(e.model.faces.values().filter(|f| f.group == Some(grid)).count(), 4);
    assert!(e.model.edges.get(&diag).is_none(), "the old diagonal is gone");
}

#[test]
fn exports_dae_dxf_pdf() {
    let mut e = Engine::new();
    make_box(&mut e);
    let dir = std::env::temp_dir().join(format!("sketchcraft-x2-{}", std::process::id()));
    let p = |n: &str| dir.join(n).to_string_lossy().to_string();
    let r = run(&mut e, json!({"cmd":"export_dae","path":p("m.dae")}));
    assert_eq!(r["triangles"], 12);
    let dae = std::fs::read_to_string(p("m.dae")).unwrap();
    assert!(dae.contains("<COLLADA") && dae.contains("<up_axis>Z_UP</up_axis>") && dae.contains("<triangles"));
    let r = run(&mut e, json!({"cmd":"export_dxf","path":p("m.dxf")}));
    let dxf = std::fs::read_to_string(p("m.dxf")).unwrap();
    assert_eq!(dxf.matches("\n3DFACE\n").count() as u64, r["triangles"].as_u64().unwrap());
    run(&mut e, json!({"cmd":"export_pdf","path":p("m.pdf"),"width":320,"height":200,"title":"Box"}));
    let pdf = std::fs::read(p("m.pdf")).unwrap();
    assert!(pdf.starts_with(b"%PDF-1.4") && pdf.ends_with(b"%%EOF\n"));
    // the xref offsets point at the objects
    let find = |needle: &[u8]| pdf.windows(needle.len()).rposition(|w| w == needle).unwrap();
    let xref = find(b"\nxref\n") + 1;
    let table = String::from_utf8(pdf[xref..].to_vec()).unwrap();
    let first: usize = table.lines().nth(3).unwrap()[..10].parse().unwrap();
    assert!(pdf[first..].starts_with(b"1 0 obj"));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn animation_frames_through_saved_views() {
    let mut e = Engine::new();
    make_box(&mut e);
    run(&mut e, json!({"cmd":"save_view","name":"A","eye":[10,-10,8],"target":[0,0,0]}));
    run(&mut e, json!({"cmd":"save_view","name":"B","eye":[-10,-10,8],"target":[0,0,1]}));
    let dir = std::env::temp_dir().join(format!("sketchcraft-anim-{}", std::process::id()));
    let r = run(&mut e, json!({"cmd":"animate","path":dir.to_string_lossy(),"fps":4,"transition":1,"delay":0.5,"width":64,"height":40}));
    // A held 2 frames, 3 in between, B held 2 frames
    assert_eq!(r["frames"], 7, "{r}");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 7);
    // no views of its own: a turntable instead
    let mut e2 = Engine::new();
    make_box(&mut e2);
    let cams = sketchcraft::animate::cameras(&e2.model, &sketchcraft::animate::Anim { fps: 10.0, transition: 2.0, delay: 1.0, views: vec![], turntable: 3.0 }, &Default::default()).unwrap();
    assert_eq!(cams.len(), 30);
    let d0 = glam::DVec3::from(cams[0].eye).distance(glam::DVec3::from(cams[0].target));
    let d1 = glam::DVec3::from(cams[15].eye).distance(glam::DVec3::from(cams[15].target));
    assert!((d0 - d1).abs() < 1e-9, "circles at a constant distance");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn drawing_sheets_pdf() {
    let mut e = Engine::new();
    make_box(&mut e);
    run(&mut e, json!({"cmd":"save_view","name":"Corner","eye":[10,-10,8],"target":[0,0,0]}));
    let dir = std::env::temp_dir().join(format!("sketchcraft-sheets-{}", std::process::id()));
    let path = dir.join("set.pdf").to_string_lossy().to_string();
    let r = run(&mut e, json!({"cmd":"sheets","path":path,"sheet":"a4","project":"Box","pages":[
        {"title":"Plan","viewports":[{"view":"top","scale":"1:50"}]},
        {"title":"Views"}
    ]}));
    assert_eq!(r["pages"], 2);
    let pdf = std::fs::read(&path).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains("/Count 2"));
    assert!(text.contains("Sheet 1 of 2") && text.contains("1:50"));
    // a bad scale is an error, not a broken file
    let bad = e.exec_json(&json!({"cmd":"sheets","path":path,"pages":[{"title":"x","viewports":[{"view":"top","scale":"one to fifty"}]}]}).to_string());
    assert_eq!(bad["ok"], false);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn dynamic_components_and_report() {
    // a 1 x 0.1 x 1 m picket as a component
    let mut e = Engine::new();
    let g = run(&mut e, json!({"cmd":"begin_group","name":"Picket"}))["group"].as_u64().unwrap();
    let f = run(&mut e, json!({"cmd":"polygon","points":[[0,0,0],[0.1,0,0],[0.1,0.1,0],[0,0.1,0]]}))["face"].as_u64().unwrap();
    let up = e.model.normal(f as _).z > 0.0;
    run(&mut e, json!({"cmd":"pushpull","face":f,"distance": if up { 1.0 } else { -1.0 }}));
    run(&mut e, json!({"cmd":"end_group"}));
    let inst = run(&mut e, json!({"cmd":"make_component","ids":[g],"name":"Picket"}))["instance"].as_u64().unwrap();
    let faces0 = e.model.world_faces().len();

    // a fence: 1.2 m tall pickets, 9 copies every 0.25 m, painted
    let r = run(&mut e, json!({"cmd":"set_attrs","id":inst,"attrs":{"height":"1.2","lenz":"=height","copies":"=round(2/0.25)","spacing":"0.25","material":"#a05a2c"}}));
    assert_eq!(r["values"]["copies"], 8.0);
    let faces = e.model.world_faces();
    assert_eq!(faces.len(), faces0 * 9, "the picket and 8 copies");
    let top = faces.iter().flat_map(|f| f.outer.iter()).map(|p| p.z).fold(f64::MIN, f64::max);
    assert!((top - 1.2).abs() < 1e-9, "stretched to 1.2 m: {top}");
    let far = faces.iter().flat_map(|f| f.outer.iter()).map(|p| p.x).fold(f64::MIN, f64::max);
    assert!((far - (2.0 + 0.1)).abs() < 1e-9, "last copy starts at 2 m: {far}");
    assert!(faces.iter().all(|f| f.color.map_or(false, |c| (c[0] - 0xa0 as f32 / 255.0).abs() < 1e-6)));

    // a formula error is reported, not saved silently broken
    let bad = e.exec_json(&json!({"cmd":"set_attrs","id":inst,"attrs":{"lenx":"=nope*2"}}).to_string());
    assert_eq!(bad["ok"], false);
    run(&mut e, json!({"cmd":"set_attrs","id":inst,"attrs":{"lenx":""}}));

    // hidden by formula
    run(&mut e, json!({"cmd":"set_attrs","id":inst,"attrs":{"hidden":"=if(height>1, 1, 0)"}}));
    assert_eq!(e.model.world_faces().len(), 0);
    run(&mut e, json!({"cmd":"set_attrs","id":inst,"attrs":{"hidden":""}}));

    // report
    let dir = std::env::temp_dir().join(format!("sketchcraft-report-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let csv = dir.join("r.csv").to_string_lossy().to_string();
    let r = run(&mut e, json!({"cmd":"report","path":csv}));
    let row = &r["rows"][0];
    assert_eq!(row["type"], "Component");
    assert_eq!(row["size_z_m"], "1.2");
    assert!(row["attributes"].as_str().unwrap().contains("copies=8"));
    let text = std::fs::read_to_string(&csv).unwrap();
    assert!(text.starts_with("type,id,name,component") && text.lines().count() == 2);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn formulas() {
    use sketchcraft::dynamic::eval;
    let look = |n: &str| match n {
        "a" => Ok(2.0),
        _ => Err(anyhow::anyhow!("no {n}")),
    };
    assert_eq!(eval("=1+2*3", &look).unwrap(), 7.0);
    assert_eq!(eval("(1+2)*3", &look).unwrap(), 9.0);
    assert_eq!(eval("2^3^2", &look).unwrap(), 512.0);
    assert_eq!(eval("-a + max(1, a, 3)", &look).unwrap(), 1.0);
    assert_eq!(eval("if(a>=2, 10, 20)", &look).unwrap(), 10.0);
    assert_eq!(eval("round(2.6)+floor(1.9)+ceil(0.1)", &look).unwrap(), 5.0);
    assert!(eval("1/0", &look).is_err());
    assert!(eval("b+1", &look).is_err());
    assert!(eval("1+", &look).is_err());
}

#[test]
fn ifc_export_with_classifier() {
    let mut e = Engine::new();
    let w = run(&mut e, json!({"cmd":"wall","points":[[0,0,0],[4,0,0],[4,3,0],[0,3,0]],"closed":true}))["group"].as_u64().unwrap();
    let slab = box_group(&mut e, [0.0, 0.0, -0.2], [4.0, 3.0, 0.0], "Thing");
    run(&mut e, json!({"cmd":"classify","ids":[slab],"class":"ifcslab"}));
    assert_eq!(e.exec_json(&json!({"cmd":"classify","ids":[slab],"class":"IfcSpaceship"}).to_string())["ok"], false);
    let dir = std::env::temp_dir().join(format!("sketchcraft-ifc-{}", std::process::id()));
    let path = dir.join("m.ifc").to_string_lossy().to_string();
    let r = run(&mut e, json!({"cmd":"export_ifc","path":path,"project":"Test house"}));
    assert_eq!(r["elements"], 2);
    let t = std::fs::read_to_string(&path).unwrap();
    assert!(t.starts_with("ISO-10303-21;") && t.trim_end().ends_with("END-ISO-10303-21;"));
    assert!(t.contains("FILE_SCHEMA(('IFC4'))"));
    assert!(t.contains("=IFCWALL(") && t.contains("=IFCSLAB("), "wall guessed from its name, slab classified");
    assert!(t.contains("IFCTRIANGULATEDFACESET") && t.contains("IFCRELCONTAINEDINSPATIALSTRUCTURE"));
    // every #ref points at a line that exists
    let n = t.lines().filter(|l| l.starts_with('#')).count();
    for cap in t.split('#').skip(1) {
        let num: String = cap.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(k) = num.parse::<usize>() {
            assert!(k >= 1 && k <= n, "#{k} out of range");
        }
    }
    let _ = w;
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn add_location_from_tiles() {
    // fake tiles: flat ground 100 m up, and a plain green map
    let terrarium = {
        let h: f64 = 100.0 + 32768.0;
        let (r, g) = ((h / 256.0).floor() as u8, (h % 256.0) as u8);
        let img = image::RgbImage::from_pixel(256, 256, image::Rgb([r, g, 0]));
        let mut b = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img).write_to(&mut b, image::ImageFormat::Png).unwrap();
        b.into_inner()
    };
    let map = {
        let img = image::RgbImage::from_pixel(256, 256, image::Rgb([40, 160, 60]));
        let mut b = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img).write_to(&mut b, image::ImageFormat::Png).unwrap();
        b.into_inner()
    };
    let get = |url: &str| -> anyhow::Result<Vec<u8>> { Ok(if url.contains("terrarium") { terrarium.clone() } else { map.clone() }) };
    let mut e = Engine::new();
    let p = sketchcraft::geo::add_location(&mut e.model, 30.2747, -97.7404, 200.0, 10, true, &get).unwrap();
    assert!((p.elevation - 100.0).abs() < 0.01, "{}", p.elevation);
    let faces: Vec<_> = e.model.faces.values().filter(|f| f.group == Some(p.group)).collect();
    assert_eq!(faces.len(), 200, "10 x 10 cells, two triangles each");
    assert!(e.model.vertices.values().filter(|v| v.group == Some(p.group)).all(|v| v.p.z.abs() < 0.01), "flat at the origin's height");
    assert!(faces.iter().all(|f| f.material.as_deref() == Some("Location imagery")));
    let t = e.model.materials["Location imagery"].texture.as_ref().unwrap();
    assert!(t.data.starts_with("data:image/jpeg") && t.project == Some([0.0, 0.0, -1.0]));
    let loc = e.model.location.as_ref().unwrap();
    assert!((loc.lat - 30.2747).abs() < 1e-9);
    // bad input is refused
    assert!(sketchcraft::geo::add_location(&mut e.model, 95.0, 0.0, 200.0, 10, false, &get).is_err());
}
