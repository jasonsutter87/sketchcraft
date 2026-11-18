// A small corpus of real SketchUp files (tests/skp_corpus/, MIT, from SketchUp/testup-2), one per format era:
// each must read with the counts we expect, and corrupted copies of each must never panic, hang or balloon.
//   cargo test --release -p sketchcraft-core --test skp_corpus
#[path = "fuzzlib.rs"]
mod fuzzlib;

use glam::DVec3;
use sketchcraft::model::Model;
use std::alloc::{GlobalAlloc, Layout, System};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::{Duration, Instant};

/// The most memory one parse may hold on to (beyond what was held when it started). The real files are
/// under a quarter of a megabyte; a corrupted one that makes the reader allocate gigabytes is a bug.
const MEMORY_LIMIT: usize = 768 << 20;
/// The longest one parse may take. (The reader has its own deadline; this proves it holds.)
const TIME_LIMIT: Duration = Duration::from_secs(5);
/// Corrupted copies per file, and for all files together.
const CASES_PER_FILE: usize = 120;
const TOTAL_TIME: Duration = Duration::from_secs(120);

// ---- a counting allocator, so the memory limit is measured, not hoped for -----------------------

struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            let now = LIVE.fetch_add(l.size(), Relaxed) + l.size();
            PEAK.fetch_max(now, Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) };
        LIVE.fetch_sub(l.size(), Relaxed);
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, l, new) };
        if !q.is_null() {
            if new >= l.size() {
                let now = LIVE.fetch_add(new - l.size(), Relaxed) + (new - l.size());
                PEAK.fetch_max(now, Relaxed);
            } else {
                LIVE.fetch_sub(l.size() - new, Relaxed);
            }
        }
        q
    }
}
#[global_allocator]
static ALLOC: Counting = Counting;

// ---- the corpus ----------------------------------------------------------------------------------

struct Entry {
    file: &'static str,
    /// the version string the file says it was written by
    version: &'static str,
    faces: usize,
    components: usize,
    placed: usize,
}

/// What each file must read as. These are what the reader gives today for files SketchUp wrote, checked against
/// the model each file holds; a change here is a change in what we import, so it should be deliberate.
const CORPUS: [Entry; 6] = [
    Entry { file: "v06-extruded-box.skp", version: "{6.0.1}", faces: 6, components: 1, placed: 1 },
    Entry { file: "v06-screen-text.skp", version: "{6.0.1}", faces: 0, components: 0, placed: 0 },
    Entry { file: "v13-extruded-circle.skp", version: "{13.0.1}", faces: 26, components: 1, placed: 1 },
    Entry { file: "v20-model.skp", version: "{20.2.160}", faces: 63, components: 1, placed: 1 },
    Entry { file: "v21-components.skp", version: "{21.0.0}", faces: 2453, components: 12, placed: 2 },
    Entry { file: "v22-soft-block.skp", version: "{22.0.0}", faces: 69, components: 1, placed: 1 },
];

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/skp_corpus")
}

fn read(file: &str) -> Vec<u8> {
    std::fs::read(dir().join(file)).unwrap_or_else(|e| panic!("{file}: {e}"))
}

#[test]
fn every_file_in_the_corpus_is_listed_and_small() {
    let mut on_disk: Vec<String> = std::fs::read_dir(dir()).unwrap().flatten().filter(|e| e.path().extension().is_some_and(|x| x == "skp")).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    on_disk.sort();
    let mut listed: Vec<&str> = CORPUS.iter().map(|e| e.file).collect();
    listed.sort();
    assert_eq!(on_disk, listed, "every .skp in tests/skp_corpus/ needs a row in CORPUS, and the other way round");
    let total: u64 = std::fs::read_dir(dir()).unwrap().flatten().map(|e| e.metadata().unwrap().len()).sum();
    assert!(total < 3 << 20, "the corpus is {total} bytes; keep it under 3 MB");
    assert!(dir().join("LICENSE").is_file(), "the MIT license of the files must stay beside them");
}

#[test]
fn each_file_reads_with_the_counts_we_expect() {
    for e in &CORPUS {
        let mut m = Model::default();
        let s = sketchcraft::skp::import(&mut m, &read(e.file), DVec3::ZERO, "corpus").unwrap_or_else(|er| panic!("{}: {er}", e.file));
        assert_eq!(s.version, e.version, "{}: version", e.file);
        assert_eq!((s.faces, s.components, s.placed), (e.faces, e.components, e.placed), "{}: faces, components, placed", e.file);
        // what it says it imported is what is in the model
        assert_eq!(m.definitions.len(), e.components, "{}: definitions in the model", e.file);
        assert!(m.faces.len() + m.definitions.values().map(|d| d.model.faces.len()).sum::<usize>() >= e.faces.min(1), "{}: no faces in the model", e.file);
    }
}

#[test]
fn reading_the_same_file_twice_gives_the_same_model() {
    for e in &CORPUS {
        let d = read(e.file);
        let (mut a, mut b) = (Model::default(), Model::default());
        let sa = sketchcraft::skp::import(&mut a, &d, DVec3::ZERO, "x").unwrap();
        let sb = sketchcraft::skp::import(&mut b, &d, DVec3::ZERO, "x").unwrap();
        assert_eq!((sa.faces, sa.components, sa.placed), (sb.faces, sb.components, sb.placed), "{}", e.file);
        assert_eq!(a.faces.len(), b.faces.len(), "{}", e.file);
    }
}

#[test]
fn what_is_not_a_sketchup_file_is_an_error_not_a_panic() {
    for d in [&b""[..], b"x", b"not a sketchup file at all", &[0u8; 64], &[0xffu8; 4096]] {
        let mut m = Model::default();
        assert!(sketchcraft::skp::import(&mut m, d, DVec3::ZERO, "x").is_err());
    }
}

/// Corrupted copies of every file: bit flips, byte changes, nasty numbers, cut and repeated runs, truncation.
/// Whatever the reader says, it must not panic, take longer than TIME_LIMIT, or hold more than MEMORY_LIMIT.
#[test]
fn corrupted_files_never_panic_hang_or_balloon() {
    let started = Instant::now();
    let out = PathBuf::from(option_env!("CARGO_TARGET_TMPDIR").unwrap_or("target"));
    let (mut cases, mut errors, mut ok) = (0usize, 0usize, 0usize);
    let (mut slowest, mut biggest) = (Duration::ZERO, 0usize);
    std::panic::set_hook(Box::new(|_| {}));
    for e in &CORPUS {
        let seed = read(e.file);
        // the same corruptions every run, so a failure here can be reproduced
        let mut r = fuzzlib::Rng(0x9E37_79B9_7F4A_7C15 ^ (e.file.len() as u64).wrapping_mul(0x1234_567) ^ seed.len() as u64);
        for i in 0..CASES_PER_FILE {
            let d = fuzzlib::mutate(&mut r, &seed, false);
            let base = LIVE.load(Relaxed);
            PEAK.store(base, Relaxed);
            let t = Instant::now();
            let res = std::panic::catch_unwind(|| {
                let mut m = Model::default();
                sketchcraft::skp::import(&mut m, &d, DVec3::ZERO, "f").is_ok()
            });
            let took = t.elapsed();
            let used = PEAK.load(Relaxed).saturating_sub(base);
            let fail = match res {
                Err(_) => Some("panicked"),
                Ok(_) if took > TIME_LIMIT => Some("took too long"),
                Ok(_) if used > MEMORY_LIMIT => Some("held too much memory"),
                Ok(good) => {
                    if good { ok += 1 } else { errors += 1 }
                    None
                }
            };
            if let Some(why) = fail {
                let f = out.join(format!("skp-corpus-fail-{}-{i}.skp", e.file));
                std::fs::write(&f, &d).ok();
                let _ = std::panic::take_hook();
                panic!("{} case {i} {why} ({took:?}, {} MB); the input is saved at {}", e.file, used >> 20, f.display());
            }
            cases += 1;
            slowest = slowest.max(took);
            biggest = biggest.max(used);
            assert!(started.elapsed() < TOTAL_TIME, "the corruption run is over its {TOTAL_TIME:?}");
        }
    }
    let _ = std::panic::take_hook();
    println!("{cases} corrupted files: {ok} still read, {errors} refused; slowest {slowest:?}, most memory {} MB", biggest >> 20);
    assert!(cases >= 500);
    // corruption that leaves everything readable, or everything refused, means the corruptions aren't doing their job
    assert!(errors > 0 && ok > 0, "{ok} read, {errors} refused");
}

/// SketchUp 6's `screentext.skp`: a model holding only a screen text ("Screentext", fixed to the window). It reads; the
/// text is found and counted, and named in the summary, but not placed (a screen text has no point in the model).
#[test]
fn a_screen_text_is_read_counted_and_named_not_placed() {
    let mut m = Model::default();
    let s = sketchcraft::skp::import(&mut m, &read("v06-screen-text.skp"), DVec3::ZERO, "t").expect("reads");
    assert_eq!((s.dimensions, s.labels, s.skipped_annotations, s.screen_texts), (0, 0, 1, 1));
    assert_eq!(s.skipped_labels, ["Screentext"]);
    assert!(m.annotations.is_empty());
}

/// Every cut through the screen text's record (and a cut every 64 bytes elsewhere) reads or is refused, never panics.
#[test]
fn every_truncation_of_the_screen_text_file_is_safe() {
    let d = read("v06-screen-text.skp");
    let t = Instant::now();
    for n in (0..d.len()).filter(|n| n % 64 == 0 || (4100..4500).contains(n)) {
        let mut m = Model::default();
        if let Ok(s) = sketchcraft::skp::import(&mut m, &d[..n], DVec3::ZERO, "t") {
            assert!(s.skipped_annotations <= 1 && m.annotations.is_empty(), "cut at {n}");
        }
    }
    assert!(t.elapsed() < Duration::from_secs(60), "{:?}", t.elapsed());
}
