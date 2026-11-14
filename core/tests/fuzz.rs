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

/// The newer readers: hatch boundaries (lines, arcs, ellipse arcs, splines,
/// polylines), splines, ellipses, text, and every kind of dimension.
#[test]
#[ignore]
fn fuzz_dxf_rich() {
    let seed = concat!(
        "0\nSECTION\n2\nENTITIES\n",
        "0\nHATCH\n8\nH\n10\n0\n20\n0\n30\n0\n2\nSOLID\n70\n1\n71\n0\n91\n2\n",
        "92\n1\n93\n4\n72\n1\n10\n0\n20\n0\n11\n4\n21\n0\n72\n2\n10\n4\n20\n2\n40\n2\n50\n270\n51\n90\n73\n1\n",
        "72\n3\n10\n2\n20\n4\n11\n2\n21\n0\n40\n0.5\n50\n0\n51\n180\n73\n0\n",
        "72\n4\n94\n3\n73\n0\n74\n0\n95\n8\n96\n4\n40\n0\n40\n0\n40\n0\n40\n0\n40\n1\n40\n1\n40\n1\n40\n1\n10\n0\n20\n4\n10\n0\n20\n3\n10\n0\n20\n1\n10\n0\n20\n0\n97\n0\n",
        "92\n3\n72\n1\n73\n1\n93\n3\n10\n1\n20\n1\n42\n0.5\n10\n2\n20\n1\n10\n1\n20\n2\n97\n0\n75\n0\n76\n1\n",
        "0\nSPLINE\n8\nS\n70\n8\n71\n3\n72\n8\n73\n4\n40\n0\n40\n0\n40\n0\n40\n0\n40\n1\n40\n1\n40\n1\n40\n1\n10\n0\n20\n0\n30\n0\n10\n1\n20\n2\n30\n0\n10\n2\n20\n2\n30\n0\n10\n3\n20\n0\n30\n0\n41\n1\n41\n2\n41\n2\n41\n1\n",
        "0\nSPLINE\n8\nS\n70\n8\n71\n3\n74\n3\n11\n0\n21\n0\n31\n0\n11\n1\n21\n1\n31\n0\n11\n2\n21\n0\n31\n0\n",
        "0\nELLIPSE\n8\nE\n10\n5\n20\n5\n30\n0\n11\n2\n21\n0\n31\n0\n40\n0.5\n41\n0.5\n42\n3\n",
        "0\nMTEXT\n8\nT\n10\n0\n20\n0\n30\n0\n1\n{\\fArial|b1;Room}\\P\\S1^2;\\U+00B0 %%c\n",
        "0\nDIMENSION\n8\nD\n70\n1\n10\n0\n20\n-1\n30\n0\n13\n0\n23\n0\n33\n0\n14\n4\n24\n0\n34\n0\n",
        "0\nDIMENSION\n8\nD\n70\n4\n10\n0\n20\n0\n30\n0\n11\n1\n21\n1\n31\n0\n15\n2\n25\n0\n35\n0\n",
        "0\nDIMENSION\n8\nD\n70\n2\n10\n1\n20\n1\n30\n0\n11\n1\n21\n1\n31\n0\n13\n0\n23\n0\n33\n0\n14\n1\n24\n0\n34\n0\n15\n0\n25\n0\n35\n0\n16\n1\n26\n1\n36\n0\n",
        "0\nENDSEC\n0\nEOF\n"
    );
    report("dxf-rich", fuzzlib::fuzz("dxf-rich", seed.as_bytes(), true, 20000, |d| {
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
