// Mutation fuzzing of the importers: a hostile file shouldn't crash or hang
// the app. Run with: cargo test --release --test fuzz -- --ignored --nocapture
#[path = "fuzzlib.rs"]
mod fuzzlib;

use glam::DVec3;
use sketchcraft::model::Model;

fn report(name: &str, (p, ms): (usize, u128)) {
    println!("{name}: {p} panics, slowest {ms} ms");
}

#[test]
#[ignore]
fn fuzz_dxf() {
    let seed = std::fs::read("../out/fuzz-seed.dxf").unwrap_or_else(|_| b"0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n8\nW\n90\n3\n70\n1\n10\n0\n20\n0\n10\n100\n20\n0\n42\n1.0\n10\n100\n20\n100\n0\nCIRCLE\n8\nF\n10\n5\n20\n5\n30\n0\n40\n3\n0\nENDSEC\n0\nEOF\n".to_vec());
    report("dxf", fuzzlib::fuzz("dxf", &seed, true, 20000, |d| {
        let mut m = Model::default();
        let _ = sketchcraft::dxf::import(&mut m, &String::from_utf8_lossy(d), "f", None, DVec3::ZERO);
    }));
}

#[test]
#[ignore]
fn fuzz_obj() {
    let seed = b"mtllib x.mtl\no Box\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 0 0 1\nvt 0 0\nvn 0 0 1\nusemtl red\nf 1/1/1 2/1/1 3/1/1 4/1/1\nf 1 2 5\nf -1 -2 -3\n".to_vec();
    report("obj", fuzzlib::fuzz("obj", &seed, true, 20000, |d| {
        let mut m = Model::default();
        let _ = sketchcraft::import::import_obj(&mut m, &String::from_utf8_lossy(d), None, "f", false, 1.0, DVec3::ZERO);
    }));
}

#[test]
#[ignore]
fn fuzz_skp() {
    let dir = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join("Documents/SketchCraft/skp-samples");
    let Ok(rd) = std::fs::read_dir(&dir) else { return println!("no SketchUp samples; skipped") };
    let iters: usize = std::env::var("FUZZ_SKP_ITERS").ok().and_then(|v| v.parse().ok()).unwrap_or(1500);
    let mut files: Vec<_> = rd.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "skp")).collect();
    files.sort_by_key(|e| e.metadata().map(|m| m.len()).unwrap_or(0));
    for e in files.into_iter().take(3) {
        let seed = std::fs::read(e.path()).unwrap();
        let name = format!("skp-{}", e.path().file_stem().unwrap().to_string_lossy().replace(' ', "_"));
        report(&name, fuzzlib::fuzz(&name, &seed, false, iters, |d| {
            let mut m = Model::default();
            let _ = sketchcraft::skp::import(&mut m, d, DVec3::ZERO, "f");
        }));
    }
}
