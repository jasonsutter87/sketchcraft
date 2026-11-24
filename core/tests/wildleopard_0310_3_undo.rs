//! The undo history's size estimate against the memory the program really holds. In a file of its own: it reads the
//! whole process's memory, so no other test may run beside it.

use base64::Engine as _;
use serde_json::json;
use sketchcraft::commands::UNDO_BYTES;
use sketchcraft::Engine;

/// Counts the bytes the program has allocated and not yet freed: the same on every system (a process's resident size
/// also holds freed memory the system hasn't taken back yet, which depends on the allocator and the system).
struct Counting;
static LIVE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
unsafe impl std::alloc::GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: std::alloc::Layout) -> *mut u8 {
        LIVE.fetch_add(l.size(), std::sync::atomic::Ordering::Relaxed);
        std::alloc::System.alloc(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: std::alloc::Layout) {
        LIVE.fetch_sub(l.size(), std::sync::atomic::Ordering::Relaxed);
        std::alloc::System.dealloc(p, l)
    }
    unsafe fn realloc(&self, p: *mut u8, l: std::alloc::Layout, new: usize) -> *mut u8 {
        LIVE.fetch_add(new, std::sync::atomic::Ordering::Relaxed);
        LIVE.fetch_sub(l.size(), std::sync::atomic::Ordering::Relaxed);
        std::alloc::System.realloc(p, l, new)
    }
}
#[global_allocator]
static A: Counting = Counting;

fn live() -> usize {
    LIVE.load(std::sync::atomic::Ordering::Relaxed)
}

fn rss() -> usize {
    let o = std::process::Command::new("ps").args(["-o", "rss=", "-p", &std::process::id().to_string()]).output().unwrap();
    String::from_utf8_lossy(&o.stdout).trim().parse::<usize>().unwrap() * 1024
}

#[test]
fn the_undo_budget_keeps_real_memory_near_the_budget() {
    // an 8 MB picture of noise (it doesn't compress), 11 MB as a data URL
    let mut x: u64 = 1;
    let img = image::RgbImage::from_fn(1700, 1700, |_, _| {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        image::Rgb([(x >> 33) as u8, (x >> 41) as u8, (x >> 49) as u8])
    });
    let mut png = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    let data = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&png));
    let step = data.len();
    let mut e = Engine::new();
    let base = live();
    let rss_base = rss();
    for _ in 0..120 {
        let r = e.exec_json(&json!({"cmd": "texture", "name": "t", "data": data, "width": 1.0, "height": 1.0}).to_string());
        assert_eq!(r["ok"], true, "{r}");
    }
    let held = live().saturating_sub(base);
    let resident = rss().saturating_sub(rss_base);
    let depth = e.undo_depth();
    eprintln!("depth {depth} step {} MB live heap {} MB resident growth {} MB", step >> 20, held >> 20, resident >> 20);
    // the history is cut to the budget by its own estimate...
    assert!(depth * step <= UNDO_BYTES + step, "{depth} steps of {} MB", step >> 20);
    assert!(depth >= 20, "undo depth {depth} is too shallow for a model this size");
    // ... and that estimate is close to what the process really holds (within about 15%)
    assert!(held < UNDO_BYTES + UNDO_BYTES * 15 / 100, "{} MB held for a budget of {} MB", held >> 20, UNDO_BYTES >> 20);
    // what the system holds on top is bounded too (freed memory not yet handed back): well under twice the budget
    assert!(resident < UNDO_BYTES * 2, "{} MB resident for a budget of {} MB", resident >> 20, UNDO_BYTES >> 20);
    // normal models keep a deep history
    let mut small = Engine::new();
    for i in 0..150 {
        let r = small.exec_json(&json!({"cmd": "rect", "from": [i, 0, 0], "to": [i + 1, 1, 0]}).to_string());
        assert_eq!(r["ok"], true, "{r}");
    }
    assert_eq!(small.undo_depth(), 150);
}
