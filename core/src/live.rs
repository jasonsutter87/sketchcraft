//! A model shared through a file: whoever changes it saves it, everyone else
//! reloads when the file's timestamp moves. That's how the app, the browser
//! bridge and Claude (over MCP) all edit one model at once.
//!
//! The file is the one place a person's work lives, so it is handled
//! carefully:
//!  - it is written whole-or-not-at-all (temp file, flush, rename);
//!  - a model with NaN or absurd numbers is never written;
//!  - the previous version is kept in `Backups/` beside it (a few, rolling);
//!  - a file that can't be read is copied aside and reported, never silently
//!    replaced by an empty model;
//!  - before every command we pick up what another client saved, so two
//!    clients don't overwrite each other's work.

use crate::backups::{self, Backup, Origin, Side};
use crate::policy::Caller;
use crate::safety::{file_stamp, lock_for, lock_for_why, write_atomic, write_atomic_guarded, FileStamp, Written};
use crate::Engine;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Side files (`.damaged-*`, `.unsaved-*`) are only removed past this many (of one kind) or this much disk,
/// the second-oldest first (the oldest, the first thing that was ever set aside, and the newest stay), and always with a notice.
const KEEP_SIDE: usize = 20;
const KEEP_SIDE_BYTES: u64 = 1 << 30;
/// How long a notice about a damaged file stays on the scene.
const NOTICE_FOR: Duration = Duration::from_secs(600);
/// Past this many points, edges and faces the person is told that changes are slow (see `model::MAX_ENTITIES`).
const BIG_MODEL: usize = 1_000_000;
/// A backup is taken at most this often while working.
const BACKUP_EVERY: Duration = Duration::from_secs(300);
/// Destructive commands that take a backup first.
const DESTRUCTIVE: [&str; 10] = ["new", "open", "delete", "purge", "explode", "ungroup", "clear_guides", "delete_view", "delete_material", "purge_materials"];
/// At most one forced backup this often per origin (the person's own steps / an agent's or a plugin's),
/// unless the model changed size a lot (more than half, or the file doubled or halved).
const FORCE_EVERY_UI: Duration = Duration::from_secs(15);
const FORCE_EVERY_OTHER: Duration = Duration::from_secs(60);
/// The biggest file a restore will read.
const MAX_RESTORE_BYTES: u64 = crate::commands::MAX_OPEN_BYTES;

/// A forced ("pre") backup of the outgoing file: who asked, what kind of step, and whether it is
/// exempt from pacing (only a restore the person asked for, or an explicit save, is).
#[derive(Clone, Copy, Debug)]
struct Force {
    origin: Origin,
    kind: &'static str,
    big: bool,
}

/// What an agent is told when a call that ran out of time found somebody else's work in the model.
const KEPT_THEIRS: &str = "This call used up its time, and someone else saved this model while it ran, so it was NOT put back as a whole: their work was kept.";

/// How often the file is written while one agent call runs many commands (the last write is when the call ends).
const CALL_FLUSH_EVERY: Duration = Duration::from_secs(2);

/// One agent call in progress (see `Live::begin_call`).
struct CallMode {
    snap: crate::commands::CallSnapshot,
    /// `Live::loads` when the call began: if it moved, somebody else's save is in the model
    loads: u64,
    was_dirty: bool,
    start_weight: usize,
    destructive: bool,
    /// commands ran that the file doesn't hold yet
    pending: bool,
    /// the file was written during the call
    flushed: bool,
    /// somebody else saved while commands of the call were waiting to be written: theirs was kept
    lost: bool,
    /// somebody else's save is in the file and what the call did since couldn't be set aside, so it is not written (yet)
    held_back: bool,
    last_flush: std::time::Instant,
    /// exactly what the call last wrote to the file (None: nothing since the model it is measured from)
    written: Option<Vec<u8>>,
    /// what the call changed in the file before somebody else's save was loaded, one entry per such save (oldest first)
    segs: Vec<crate::history::Patch>,
    /// the model the call's changes since the last such save are measured from (None: the one the call began with)
    base: Option<crate::model::Model>,
    /// what the call did can't be worked out entry by entry (too big, or what it wrote couldn't be read back)
    unknown: Option<String>,
}

/// Why a save did not happen.
#[derive(Debug)]
enum SaveErr {
    /// the file changed since this program last read or wrote it: nothing was written
    Moved,
    Fail(String),
}

impl std::fmt::Display for SaveErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveErr::Moved => write!(f, "{MOVED_TEXT}"),
            SaveErr::Fail(e) => write!(f, "{e}"),
        }
    }
}

const MOVED_TEXT: &str = "another program saved the model first, so this was not written over it";

/// Whose work a set-aside file holds (the wording of the notice).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Whose {
    /// the person's (a save that failed, an edit)
    Person,
    /// an agent's call
    Agent,
    /// a plugin's run
    Plugin,
}

/// The longest wait for the lock on the model file (a program that holds it longer is stuck, and the save is tried again later).
const LOCK_WAIT: Duration = Duration::from_secs(10);
/// A call's changes are only tracked entry by entry up to about this much memory (past it, they are set aside whole).
const MAX_EFFECT_BYTES: usize = 384 << 20;
/// How many times a rollback plans again when the file moves under it.
const REPLANS: usize = 8;

pub struct Live {
    pub engine: Engine,
    pub doc: PathBuf,
    seen: Option<FileStamp>,
    /// no file: an in-memory model (an agent with no --doc)
    persist: bool,
    last_backup: Option<SystemTime>,
    /// something the person should know (a damaged file was set aside, a save failed)
    pub notice: Option<String>,
    notice_at: Option<SystemTime>,
    /// what the notice says, when it is about repaired groups (the app words it in the person's language)
    notice_repair: Option<(crate::model::GroupRepair, String)>,
    /// commands ran inside a batch and the file doesn't have them yet
    unsaved_batch: bool,
    /// how much the model held when the open batch began, and whether a destructive command ran in it
    batch_weight: usize,
    batch_destructive: bool,
    /// saving is off: the file couldn't be read and couldn't be copied aside
    blocked: bool,
    /// the next time a file that couldn't be read is tried again (see `retry_blocked`)
    retry_after: Option<SystemTime>,
    /// memory holds work the file doesn't (a save failed): never replaced without keeping it first
    dirty: bool,
    /// the side file that holds work set aside; the notice about it stays while the file does
    sticky: Option<PathBuf>,
    /// the last forced backup per origin: when, and how big the file was
    last_forced: HashMap<Origin, (SystemTime, u64)>,
    /// the model file couldn't be read when this program started: nothing is saved over it until the person
    /// restores something or starts over (see `load_failed`)
    held: bool,
    /// the side files this run wrote (names): the cap holds even when the index can't be read
    own_side: Vec<String>,
    /// this is the person's own window (the app, the dev bridge), the one that tells them what an agent changed
    person_view: bool,
    /// how many times somebody else's file replaced what memory held (a call can tell its rollback must not write
    /// the model it started with over their work)
    loads: u64,
    /// an agent call in progress: its commands are written to the file every few seconds and when it ends, not after each
    call: Option<CallMode>,
    /// how often a call's commands are written (tests shorten it)
    flush_every: Duration,
    /// tests only: told where a save is (`serialized`, `locked`, `written`), so a second program can act exactly then
    hook: Option<Box<dyn FnMut(&str) + Send>>,
    /// tests only: setting work aside fails (a full disk)
    aside_fault: bool,
    /// the person was told that no lock could be made for the model file
    lock_told: bool,
}

/// "Left out: ..." for a notice: what was skipped, with the reasons.
fn skipped_text(list: &[String]) -> String {
    format!("{} picture{} left out: {}.", list.len(), if list.len() == 1 { " was" } else { "s were" }, list.join("; "))
}

fn big_model_text(n: usize) -> String {
    format!("This is a big model ({n} points, edges and faces). Everything works, but each change takes a second or more to save and the view takes a few seconds to redraw.")
}

fn stamp(p: &Path) -> Option<FileStamp> {
    file_stamp(p)
}

fn epoch(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn weight(m: &crate::model::Model) -> usize {
    m.faces.len() + m.instances.len() + m.groups.len() + m.definitions.values().map(|d| weight(&d.model)).sum::<usize>()
}

/// True when this process is a Rust test program. Positive signals first: a unit test
/// (`cfg!(test)`) or `SKETCHCRAFT_TESTING`. Cargo gives an integration-test program no run-time
/// environment variable that `cargo run` lacks (`CARGO_TARGET_TMPDIR` is compile-time only), so the
/// rest is the program's folder layout, in both layouts cargo has used: test programs sit in a
/// `deps/` folder, or (Rust 1.100 and later) in `build/<package>/<hash>/out/`. The shipped programs
/// run from a `release/` folder or an install folder, never from either layout.
pub fn running_as_cargo_test() -> bool {
    let on = |k: &str| std::env::var_os(k).is_some_and(|v| !v.is_empty() && v != "0");
    if cfg!(test) || on("SKETCHCRAFT_TESTING") {
        return true;
    }
    std::env::current_exe().ok().is_some_and(|p| exe_in_test_layout(&p))
}

pub fn exe_in_test_layout(p: &Path) -> bool {
    let dirs: Vec<&std::ffi::OsStr> = p.parent().map(|d| d.components().map(|c| c.as_os_str()).collect()).unwrap_or_default();
    let n = dirs.len();
    if n >= 1 && dirs[n - 1] == "deps" {
        return true;
    }
    // .../build/<package>/<hash>/out
    n >= 4 && dirs[n - 1] == "out" && dirs[n - 4] == "build"
}

/// Where the person's model folder is. `SKETCHCRAFT_DOC` always wins (and scopes every folder
/// beside it: Library, Plugins, Backups). In a test program with no `SKETCHCRAFT_DOC`, a throwaway
/// folder in the temp directory, so `cargo test` can never touch the real Documents folder.
pub fn doc_path(var: Option<std::ffi::OsString>, testing: bool, home: Option<PathBuf>) -> PathBuf {
    if let Some(p) = var {
        return PathBuf::from(p);
    }
    if testing {
        return std::env::temp_dir().join(format!("sketchcraft-test-{}", std::process::id())).join("live.sketchcraft");
    }
    home.unwrap_or_else(|| PathBuf::from(".")).join("Documents/SketchCraft/live.sketchcraft")
}

pub fn default_doc() -> PathBuf {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    doc_path(std::env::var_os("SKETCHCRAFT_DOC"), running_as_cargo_test(), home)
}

impl Live {
    pub fn open(doc: PathBuf) -> Self {
        let mut live = Self::blank(Engine::new(), doc, true);
        if live.doc.exists() {
            live.load(true);
        }
        live.seen = stamp(&live.doc);
        live
    }

    fn blank(engine: Engine, doc: PathBuf, persist: bool) -> Self {
        Live { engine, doc, seen: None, persist, last_backup: None, notice: None, notice_at: None, notice_repair: None, unsaved_batch: false, batch_weight: 0, batch_destructive: false, blocked: false, retry_after: None, dirty: false, sticky: None, last_forced: HashMap::new(), held: false, own_side: vec![], person_view: false, loads: 0, call: None, flush_every: CALL_FLUSH_EVERY, hook: None, aside_fault: false, lock_told: false }
    }

    /// A model that is not saved anywhere.
    pub fn memory(engine: Engine) -> Self {
        Self::blank(engine, PathBuf::new(), false)
    }

    /// Open the file into the engine. If it can't be read, set it aside (a copy
    /// that is never overwritten) and say so; `at_start`: the engine stays empty.
    fn load(&mut self, at_start: bool) -> bool {
        let before = self.loads;
        let ok = self.load_inner(at_start);
        if !at_start && self.loads != before {
            self.call_saw_theirs();
        }
        ok
    }

    /// Somebody else's save replaced what memory held while an agent call was running: what the call had written to the
    /// file up to now is worked out as a list of changes (entry by entry, against the model it was measured from), and
    /// what the call does from here is measured from their file. A rollback takes the changes back off whatever the file
    /// holds by then, so it undoes the call's work and nothing else, however many steps the undo list could hold.
    fn call_saw_theirs(&mut self) {
        let Some(c) = self.call.as_mut() else { return };
        if let Some(bytes) = c.written.take() {
            match serde_json::from_slice::<crate::model::Model>(&bytes) {
                Ok(g) => {
                    let base = c.base.as_ref().unwrap_or_else(|| Engine::snapshot_model(&c.snap));
                    let seg = crate::history::patch_between(base, &g);
                    let total: usize = c.segs.iter().map(|s| s.approx_bytes()).sum::<usize>() + seg.approx_bytes();
                    if total > MAX_EFFECT_BYTES {
                        c.unknown = Some("what the call did is too big to be taken back entry by entry".into());
                    } else if !seg.is_empty() {
                        c.segs.push(seg);
                    }
                }
                Err(_) => c.unknown = Some("what the call wrote could not be read back".into()),
            }
        }
        c.base = Some(self.engine.model.clone());
    }

    fn load_inner(&mut self, at_start: bool) -> bool {
        // a file that holds exactly what memory holds (a touch, a sync tool, our own save seen late)
        // is not a change: the undo history stays
        if !at_start && !self.dirty {
            if let (Ok(file), Ok(mem)) = (std::fs::read(&self.doc), crate::format::to_vec(&self.engine.model)) {
                if file == mem {
                    return true;
                }
            }
        }
        // a reload keeps the component being edited, if it is still there
        let ctx = self.engine.ctx;
        // and the parts laid out to look at (worked out again from what was read in)
        let laid_out = self.engine.laid_out.clone();
        // and the group being edited (it is this program's own state: the file's one belongs to whoever saved it)
        let active = self.engine.model.active_group;
        // the person's own steps are kept as changes (what each step did), so that Undo after another
        // program's save takes back the person's step and never the other program's work
        let history = if at_start { None } else { self.engine.history_for_reload() };
        let r = self.engine.exec_json(&json!({"cmd": "open", "path": self.doc}).to_string());
        if r["ok"] != true {
            // the file wasn't taken: the steps (set aside above, not yet worked out) stay as they were
            if let Some(h) = history {
                self.engine.install_history(h);
            }
            return self.load_failed(r, at_start);
        }
        {
            if let Some(d) = ctx.filter(|d| self.engine.model.definitions.contains_key(d)) {
                self.engine.ctx = Some(d);
            }
            if !at_start {
                self.engine.laid_out = laid_out;
            }
            if let Some(g) = active.filter(|g| !at_start && self.engine.model.groups.contains_key(g)) {
                self.engine.model.active_group = Some(g);
            }
            // the file reads fine now: whatever was wrong is over
            self.blocked = false;
            self.held = false;
            self.unsaved_batch = false;
            self.dirty = false;
            if !at_start {
                self.loads += 1;
            }
            let mut lost = (0, 0);
            if let Some(h) = history {
                lost = self.engine.install_history(h);
            }
            if self.sticky.is_none() {
                self.notice = None;
                if lost.0 + lost.1 > 0 {
                    let mut what = vec![];
                    if lost.0 > 0 {
                        what.push(format!("{} of your oldest steps couldn't be kept for Undo", lost.0));
                    }
                    if lost.1 > 0 {
                        what.push(format!("{} step{} couldn't be kept for Redo", lost.1, if lost.1 == 1 { "" } else { "s" }));
                    }
                    self.set_notice(format!("Another program saved this model. {}; the rest still work.", what.join(" and ")));
                }
            }
            let clamped: Vec<String> = r["sheets_clamped"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
            if !clamped.is_empty() {
                self.set_notice(format!("Some drawing-sheet content in this file was over SketchCraft's limits and was trimmed to fit ({}). The file itself is unchanged until the model is saved.", clamped.join("; ")));
            }
            self.set_repair_notice(crate::model::GroupRepair::from_reply(&r), "file");
            let skipped: Vec<String> = r["skipped_pictures"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
            if !skipped.is_empty() {
                self.set_notice(skipped_text(&skipped));
            }
            if self.engine.oversize {
                let note = self.oversize_text();
                let note = if skipped.is_empty() { note } else { format!("{note} {}", skipped_text(&skipped)) };
                self.set_notice(note);
            } else if self.sticky.is_none() && self.engine.model.entity_count() > BIG_MODEL {
                self.set_notice(big_model_text(self.engine.model.entity_count()));
            }
            return true;
        }
    }

    fn oversize_text(&self) -> String {
        let m = &self.engine.model;
        format!("This model is over the size limits ({} points, edges and faces, limit {}; about {} MB, limit {} MB; names and notes {} MB, limit {} MB). It is open for looking at and for cutting down; saving resumes when it is under the limits, and the file on disk is untouched meanwhile. To cut it down: delete geometry or placements; in the Materials panel delete or replace the textures that take the most room (or choose \"Remove unused materials\"); in the Views panel delete scenes whose photos are large. File > Restore from backup... still has the earlier copies.", m.entity_count(), crate::model::MAX_ENTITIES, m.size_estimate() >> 20, crate::model::MAX_MODEL_BYTES >> 20, m.text_bytes() >> 20, crate::model::MAX_TEXT_BYTES >> 20)
    }

    /// The model file couldn't be read: set it aside, keep what memory holds.
    fn load_failed(&mut self, r: Value, at_start: bool) -> bool {
        let why = r["error"].as_str().unwrap_or("unknown error").to_string();
        let copied = self.open_side_file("damaged", "").and_then(|(mut f, p)| {
            let r = std::fs::File::open(&self.doc).and_then(|mut src| std::io::copy(&mut src, &mut f)).and_then(|_| f.sync_all());
            match r {
                Ok(()) => Ok(p),
                Err(e) => {
                    drop(f);
                    std::fs::remove_file(&p).ok();
                    Err(e.to_string())
                }
            }
        });
        match copied {
            Ok(aside) => {
                self.blocked = false;
                self.index_side(&aside);
                if at_start {
                    // the first command must not write an empty model over the file the person's work is in: saving stays off
                    // until the person restores something (this copy, or a backup) or starts over with New or Open
                    self.held = true;
                    self.set_notice(format!("The model file couldn't be opened ({why}). A copy was kept at {}. Nothing will be saved over the file until you choose: File > Restore from backup... lists that copy (Restore works on files of any size the program can read), or File > New or File > Open starts over (the file is then replaced; the copy stays).", aside.display()));
                } else {
                    self.set_notice(format!("The model file couldn't be opened ({why}). A copy was kept at {}; this session carries on with the model in memory. File > Restore from backup... lists it.", aside.display()));
                }
                self.prune_side(Side::Damaged);
            }
            Err(e) => {
                self.blocked = true;
                self.set_notice(format!("The model file couldn't be opened ({why}) and couldn't be copied aside ({e}), so nothing will be saved over it. Repair or move {} and it will be picked up again (saving resumes by itself), or File > New or File > Open starts over. File > Save As... keeps the work in memory in another file meanwhile.", self.doc.display()));
            }
        }
        false
    }

    fn set_notice(&mut self, text: String) {
        self.notice = Some(text);
        self.notice_at = Some(SystemTime::now());
        self.notice_repair = None;
    }

    /// Tell the person what was repaired in a model's groups when it came in (a file, a restored backup), if anything was.
    fn set_repair_notice(&mut self, repair: crate::model::GroupRepair, what: &str) {
        if let Some(text) = repair.notice(what) {
            self.set_notice(text);
            self.notice_repair = Some((repair, what.to_string()));
        }
    }

    /// Note a side file this program wrote in the hidden index beside the model. Only files named there are ever pruned.
    ///
    /// The index is also rebuilt here: a lost, unreadable or garbled index would otherwise be replaced by a list of
    /// one, and the earlier side files would no longer be listed or counted. Files that carry one of our names and
    /// were written when the name says (`backups::provenance_ok`) are put back.
    fn index_side(&mut self, p: &Path) {
        if let Some(name) = p.file_name().map(|n| n.to_string_lossy().into_owned()) {
            let mut names = backups::read_index(&self.doc);
            for (_, secs, path, _) in backups::side_files(&self.doc) {
                if let Some(n) = path.file_name().map(|n| n.to_string_lossy().into_owned()) {
                    if !names.contains(&n) && backups::provenance_ok(secs, &path) {
                        names.push(n);
                    }
                }
            }
            if !names.contains(&name) {
                names.push(name.clone());
            }
            backups::write_index(&self.doc, &names);
            self.own_side.push(name);
        }
    }

    /// Is this side file one this program made (named in the index, written this run, or written when its name says)?
    fn side_is_ours(&self, indexed: &[String], secs: u64, path: &Path) -> bool {
        path.file_name().map(|n| n.to_string_lossy().into_owned()).is_some_and(|n| indexed.contains(&n) || self.own_side.contains(&n)) || backups::provenance_ok(secs, path)
    }

    /// Keep the side files of one kind within `KEEP_SIDE` files and `KEEP_SIDE_BYTES`: the oldest of the ones this
    /// program wrote go first, the newest never, and whatever goes is named in a notice.
    fn prune_side(&mut self, kind: Side) {
        // an index that is gone or can't be read does not lift the cap: files this run wrote, and files that carry our names and
        // were written when the name says, still count and can still be pruned
        let readable = backups::read_index_checked(&self.doc);
        let indexed = readable.clone().unwrap_or_default();
        // only the files this program wrote count toward the limits; others (the person's own) are not ours to count or remove
        let all: Vec<(Side, u64, PathBuf, u64)> = backups::side_files(&self.doc).into_iter().filter(|(k, secs, p, _)| *k == kind && self.side_is_ours(&indexed, *secs, p)).collect();
        let mut total: u64 = all.iter().map(|x| x.3).sum();
        let mut count = all.len();
        let mut removed: Vec<String> = vec![];
        for (i, (_, _, p, size)) in all.iter().enumerate() {
            if i + 1 == all.len() || (count <= KEEP_SIDE && total <= KEEP_SIDE_BYTES) {
                break;
            }
            // the first thing ever set aside stays (a flood of later ones can't push it out)
            if i == 0 && all.len() > 2 {
                continue;
            }
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if std::fs::remove_file(p).is_ok() {
                count -= 1;
                total -= size;
                removed.push(name);
            }
        }
        if !removed.is_empty() {
            if readable.is_some() {
                let left: Vec<String> = indexed.into_iter().filter(|n| !removed.contains(n)).collect();
                backups::write_index(&self.doc, &left);
            }
            self.own_side.retain(|n| !removed.contains(n));
            let msg = format!("{} older {} file{} beside the model {} removed to stay under {KEEP_SIDE} files / {} GB: {}.", removed.len(), kind.word(), if removed.len() == 1 { "" } else { "s" }, if removed.len() == 1 { "was" } else { "were" }, KEEP_SIDE_BYTES >> 30, removed.join(", "));
            let note = match self.notice.take() {
                Some(n) => format!("{n} {msg}"),
                None => msg,
            };
            self.set_notice(note);
        }
    }

    /// Open a new side file beside the model (`<name>.<kind>-<time>[-n]<ext>`), private to the owner (0600 where the system has
    /// modes). The name is taken with "create new" (an existing file, or a link, even a dangling one, is never followed or
    /// written over: the next number is tried), and a model name too long to carry the suffix gets a shortened one.
    fn open_side_file(&self, kind: &str, ext: &str) -> Result<(std::fs::File, PathBuf), String> {
        let t = epoch(SystemTime::now());
        let stem = self.doc.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "live".into());
        let dir = self.doc.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let mut last = String::from("no free name");
        for short in [false, true] {
            // a file name holds at most 255 bytes on most systems: leave room for the suffix and a number
            if !short && stem.len() + kind.len() + ext.len() + 24 > 250 {
                continue;
            }
            let stem_part: String = if short {
                stem.chars()
                    .scan(0usize, |n, c| {
                        *n += c.len_utf8();
                        (*n <= 60).then_some(c)
                    })
                    .collect()
            } else {
                stem.clone()
            };
            for n in 0..1000u32 {
                let p = dir.join(format!("{stem_part}.{kind}-{t}{}{ext}", if n == 0 { String::new() } else { format!("-{n}") }));
                let mut o = std::fs::OpenOptions::new();
                o.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    o.mode(0o600);
                }
                match o.open(&p) {
                    Ok(f) => return Ok((f, p)),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => {
                        last = e.to_string();
                        break;
                    }
                }
            }
        }
        Err(last)
    }

    /// Write `bytes` to a new side file (see `open_side_file`); a file that couldn't be written whole is removed.
    fn create_side_file(&self, kind: &str, ext: &str, bytes: &[u8]) -> Result<PathBuf, String> {
        use std::io::Write as _;
        let (mut f, p) = self.open_side_file(kind, ext)?;
        match f.write_all(bytes).and_then(|_| f.sync_all()) {
            Ok(()) => Ok(p),
            Err(e) => {
                drop(f);
                std::fs::remove_file(&p).ok();
                Err(e.to_string())
            }
        }
    }

    /// Set the model in memory aside (`<name>.unsaved-<time>.sketchcraft` beside the file) before
    /// something replaces it, and keep telling the person until that file is dealt with.
    fn stash(&mut self, why: &str, whose: Whose) -> Result<PathBuf, String> {
        let bytes = crate::format::to_vec(&self.engine.model).map_err(|e| e.to_string())?;
        self.stash_bytes(&bytes, why, whose)
    }

    /// Set a whole model (as a file's bytes) aside. The file is private to the person who owns the model (0600 where the
    /// system has modes) and is never written over another.
    fn stash_bytes(&mut self, bytes: &[u8], why: &str, whose: Whose) -> Result<PathBuf, String> {
        if self.aside_fault {
            return Err("simulated failure".into());
        }
        let path = self.create_side_file("unsaved", ".sketchcraft", bytes)?;
        self.index_side(&path);
        self.sticky = Some(path.clone());
        let what = match whose {
            Whose::Person => "Your changes were kept in",
            Whose::Agent => "What the AI agent did was set aside in",
            Whose::Plugin => "What the plugin did was set aside in",
        };
        self.set_notice(format!("{why} {what} {} (File > Restore from backup... lists it). This message stays until that file is moved or deleted.", path.display()));
        self.prune_side(Side::Unsaved);
        Ok(path)
    }

    /// Another client saved. Replace what is in memory with their file, after setting aside any work
    /// memory holds that the file doesn't (`ours_ahead`, or a failed save). False if the file wasn't taken.
    fn take_theirs(&mut self, ours_ahead: bool, why: &str, whose: Whose) -> bool {
        if ours_ahead || self.dirty {
            if let Err(e) = self.stash(why, whose) {
                let mine = match whose {
                    Whose::Person => "Your unsaved changes",
                    Whose::Agent => "What the AI agent did",
                    Whose::Plugin => "What the plugin did",
                };
                self.set_notice(format!("{why} {mine} couldn't be set aside ({e}), so the other version was not loaded."));
                return false;
            }
        }
        self.load(false)
    }

    /// A plugin run's work meets somebody else's save: theirs is taken once the run's work is set aside. When that fails the file
    /// is left as it is and memory keeps the work (the next change or sync tries again), never written over theirs.
    fn take_theirs_after_batch(&mut self) {
        let now = stamp(&self.doc);
        if self.take_theirs(true, "The plugin run was cut off and another program had saved the model meanwhile.", Whose::Plugin) {
            self.seen = now;
        } else {
            self.dirty = true;
        }
    }

    /// Housekeeping that must not wait for the next command: close a batch nobody closed (saving
    /// what it did), and forget a side-file notice once the file is gone. The bridge and the app call this
    /// on a timer, so the 150 s limit holds even when nothing polls.
    pub fn housekeeping(&mut self) {
        self.tick();
    }

    fn tick(&mut self) {
        if self.sticky.as_ref().is_some_and(|p| !p.exists()) {
            self.sticky = None;
            self.notice = None;
        }
        if self.persist && self.engine.close_expired_batch() {
            self.finish_batch();
        }
    }

    /// What a closed batch that shrank the model a lot, or ran something destructive, forces a backup for.
    fn batch_force(&self) -> Option<Force> {
        let after = weight(&self.engine.model);
        let shrank = self.batch_weight > 0 && after * 4 < self.batch_weight * 3;
        (shrank || self.batch_destructive).then_some(Force { origin: Origin::Plugin, kind: "bat", big: false })
    }

    /// Save what a batch that has ended did (no reply to give anyone).
    fn finish_batch(&mut self) {
        if !self.unsaved_batch {
            return;
        }
        self.unsaved_batch = false;
        if self.doc.exists() && !self.blocked && stamp(&self.doc) != self.seen {
            self.take_theirs_after_batch();
            return;
        }
        let force = self.batch_force();
        match self.save_with(force) {
            Ok(()) => {}
            // another program saved while the batch's work was being written: theirs is kept, the batch's is set aside
            Err(SaveErr::Moved) => self.take_theirs_after_batch(),
            Err(e) => {
                self.dirty = true;
                self.set_notice(format!("Couldn't save your model: {e}"));
            }
        }
    }

    /// Run a command as the person using the app; save the model if it changed.
    pub fn exec(&mut self, cmd: &Value) -> Value {
        self.exec_as(&Caller::Ui, cmd)
    }

    pub fn exec_as(&mut self, caller: &Caller, cmd: &Value) -> Value {
        self.exec_as_from(caller, cmd, None)
    }

    /// `from`: the plugin the app is running this for (named by the app, never by the plugin).
    pub fn exec_as_from(&mut self, caller: &Caller, cmd: &Value, from: Option<&str>) -> Value {
        self.exec_attempt(caller, cmd, from, 0)
    }

    /// One try at a command. When another program saves between the moment this one read the file and the moment it would
    /// write, theirs is taken and the command is run again on it (a few times), so a person is not asked to repeat a step.
    fn exec_attempt(&mut self, caller: &Caller, cmd: &Value, from: Option<&str>, attempt: u8) -> Value {
        self.tick();
        let name = cmd["cmd"].as_str().unwrap_or("");
        // the person's own list of backups and the restore (only the app's own user; never a plugin or an agent)
        if matches!(name, "list_backups" | "restore_backup") {
            // restoring replaces the model: like every change, it waits until the parts are no longer laid out
            if name == "restore_backup" && self.engine.laid_out.is_some() {
                return json!({"ok": false, "error": crate::commands::LAID_OUT_REFUSAL, "laid_out": true});
            }
            return if matches!(caller, Caller::Ui) { self.restore_command(name, cmd) } else { json!({"ok": false, "error": format!("\"{name}\" isn't available to plugins or agents")}) };
        }
        let origin = Origin::of(caller);
        // the person edits (or opens a batch of their own) while a plugin run or an agent's batch is open: that batch ends
        // here (its work is saved first, as its own undo step), so the person's edits are never held inside someone
        // else's batch that a crash would lose. The app's OWN batch is not that: a batch it opened groups its own commands.
        let person_acts = name == "begin_batch" || (!matches!(name, "end_batch" | "undo" | "redo") && serde_json::from_value::<crate::Command>(cmd.clone()).is_ok_and(|c| c.mutates()));
        if self.persist && self.engine.person_edit_ends_batch() && matches!(caller, Caller::Ui) && person_acts {
            self.engine.close_batch();
            self.finish_batch();
        }
        // someone else may have saved since we last looked: start from theirs. Never inside a batch (a
        // plugin run): the rest of its commands would land on their model; theirs is picked up when it ends.
        let size = |e: &Engine| weight(&e.model);
        // an agent's call (and a plugin's run) is weighed once, not after every command
        let in_call = self.call.is_some() && !matches!(caller, Caller::Ui);
        if in_call && self.persist && !self.engine.in_batch() && self.doc.exists() && !self.blocked && stamp(&self.doc) != self.seen {
            // somebody else saved: the commands waiting to be written are set aside when theirs is loaded just below
            if let Some(c) = self.call.as_mut().filter(|c| c.pending) {
                c.lost = true;
            }
        }
        if !self.engine.in_batch() {
            self.sync();
        }
        let weigh = (!in_call && !self.engine.in_batch()) || name == "begin_batch";
        let (before, size_before, depth_before) = (self.engine.revision(), if weigh { size(&self.engine) } else { 0 }, self.engine.undo_depth());
        let destructive = DESTRUCTIVE.contains(&name);
        let mut r = self.engine.exec_value_as_from(caller, cmd.clone(), from);
        // the person starting over (or opening something else) after the model file couldn't be read: saving resumes
        if (self.held || self.blocked) && r["ok"] == true && matches!(caller, Caller::Ui) && matches!(name, "new" | "open") {
            self.held = false;
            self.blocked = false;
        }
        // inside a batch (a plugin run) the file is written once, when the batch ends
        let changed = r["ok"] == true && self.engine.revision() != before;
        if self.engine.in_batch() {
            if name == "begin_batch" && !self.unsaved_batch {
                self.batch_weight = size_before;
                self.batch_destructive = false;
            } else if destructive && r["ok"] == true {
                self.batch_destructive = true;
            }
            self.unsaved_batch |= changed;
            return r;
        }
        if in_call {
            let pending_batch = std::mem::take(&mut self.unsaved_batch);
            let ok = r["ok"] == true;
            if let Some(c) = self.call.as_mut() {
                c.destructive |= destructive && ok;
                c.pending |= ok && (changed || pending_batch);
            }
            if self.call.as_ref().is_some_and(|c| c.pending) {
                // memory holds work the file doesn't yet: another program's save sets it aside before it is replaced
                self.dirty = true;
                if self.call.as_ref().is_some_and(|c| c.last_flush.elapsed() >= self.flush_every) {
                    self.flush_call();
                }
            }
            return r;
        }
        let had_batch = self.unsaved_batch;
        let changed = changed || (r["ok"] == true && self.unsaved_batch);
        self.unsaved_batch = false;
        if changed && self.persist && self.engine.read_only.is_none() {
            // a change that drops a quarter of the model (or a destructive command, or a plugin run that did) keeps
            // the file as it was in Backups first. Forced backups are paced per origin (see `backup`).
            let size_after = size(&self.engine);
            let shrank = !matches!(name, "undo" | "redo") && size_before > 0 && size_after * 4 < size_before * 3;
            let batch = if had_batch { self.batch_force() } else { None };
            let force = if let Some(b) = batch {
                Some(b)
            } else if matches!(name, "new" | "open") || shrank || destructive {
                let kind = match name {
                    "new" => "new",
                    "open" => "open",
                    "delete" | "purge" | "explode" | "ungroup" | "clear_guides" | "delete_view" | "delete_material" | "purge_materials" => "del",
                    _ => "shr",
                };
                Some(Force { origin, kind: if shrank && !destructive { "shr" } else { kind }, big: false })
            } else {
                None
            };
            // another client saved while this command ran: theirs wins, so nobody's work is overwritten.
            // A plugin run's whole output is set aside first (a single command is just run again on their version)
            let mut moved = self.doc.exists() && !self.blocked && stamp(&self.doc) != self.seen;
            let mut tries = 0;
            let saved = loop {
                if moved {
                    let why = "Another program saved the model while a plugin was running.";
                    let kept = if had_batch { self.stash(why, Whose::Plugin).ok() } else { None };
                    if had_batch && kept.is_none() {
                        // their file is not taken and not written over: the plugin's work stays in memory
                        self.dirty = true;
                        return json!({"ok": false, "error": "Another program saved the model while the plugin ran, and the plugin's changes couldn't be set aside, so nothing was saved. Run it again."});
                    }
                    self.seen = stamp(&self.doc);
                    // what this command did is being thrown away: it is not a step to undo
                    self.engine.drop_steps_after(depth_before);
                    if self.load(false) {
                        if !had_batch && attempt < 3 && !matches!(name, "undo" | "redo" | "new" | "open") {
                            return self.exec_attempt(caller, cmd, from, attempt + 1);
                        }
                        return json!({"ok": false, "error": match kept {
                            Some(p) => format!("Someone else saved the model while that ran, so their version was kept. What the plugin made was set aside in {}.", p.display()),
                            None => "Someone else saved the model while that ran, so their version was kept and that change was not applied. Run it again.".into(),
                        }});
                    }
                }
                match self.save_with(force) {
                    Err(SaveErr::Moved) if tries < 2 => {
                        tries += 1;
                        moved = true;
                    }
                    other => break other,
                }
            };
            match saved {                Ok(()) => {
                    self.dirty = false;
                    if changed && matches!(caller, Caller::Mcp(_)) {
                        self.record_agent_change();
                    }
                    if self.notice.as_deref().is_some_and(|n| n.starts_with("Couldn't save")) {
                        self.notice = None;
                    }
                    // an import (or anything else) that made the model big: say so now, not only when the file is next loaded
                    if self.notice.is_none() && self.sticky.is_none() && self.engine.model.entity_count() > BIG_MODEL {
                        self.set_notice(big_model_text(self.engine.model.entity_count()));
                    }
                }
                Err(e) => {
                    self.dirty = true;
                    let e = e.to_string();
                    // the notice that says what to do about an unreadable file stays (it is the better message)
                    if !self.held {
                        self.set_notice(format!("Couldn't save your model: {e}"));
                    }
                    r["save_error"] = json!(e);
                }
            }
        }
        r
    }

    /// An agent's call begins: its commands are written to the file every few seconds and once when it ends (not
    /// after each one: on a big model each write and weigh costs a good part of a second), and `abort_call` can take
    /// the call back without ever overwriting somebody else's save.
    pub fn begin_call(&mut self) {
        if self.call.is_some() {
            return;
        }
        if !self.engine.in_batch() {
            self.sync();
        }
        self.call = Some(CallMode {
            snap: self.engine.call_snapshot(),
            loads: self.loads,
            was_dirty: self.dirty,
            start_weight: if self.persist { weight(&self.engine.model) } else { 0 },
            destructive: false,
            pending: false,
            flushed: false,
            lost: false,
            held_back: false,
            last_flush: std::time::Instant::now(),
            written: None,
            segs: vec![],
            base: None,
            unknown: None,
        });
    }

    /// Write what the call's commands did that the file doesn't hold yet. Another program's save since the last write
    /// wins: what the call did is set aside (see `take_theirs`) and the call goes on from theirs.
    fn flush_call(&mut self) {
        let Some(c) = self.call.as_mut() else { return };
        if !c.pending {
            return;
        }
        let (ran_for, first_write) = (c.last_flush.elapsed(), !c.flushed);
        c.last_flush = std::time::Instant::now();
        c.pending = false;
        let (start_weight, destructive) = (c.start_weight, c.destructive);
        if !self.persist || self.engine.read_only.is_some() || self.engine.in_batch() {
            if self.engine.in_batch() {
                self.unsaved_batch = true;
                if let Some(c) = self.call.as_mut() {
                    c.pending = false;
                }
            }
            return;
        }
        let agent_waits = |s: &mut Self| {
            let now = stamp(&s.doc);
            if let Some(c) = s.call.as_mut() {
                c.lost = true;
            }
            // theirs is only taken (and the file only written over later) once the call's work is safely set aside; when that
            // fails, their file is left exactly as it is, memory keeps the work, and the next flush tries again
            if s.take_theirs(true, "Another program saved the model while an agent was working.", Whose::Agent) {
                s.seen = now;
                if let Some(c) = s.call.as_mut() {
                    c.held_back = false;
                }
            } else {
                s.dirty = true;
                if let Some(c) = s.call.as_mut() {
                    c.pending = true;
                    c.held_back = true;
                }
            }
        };
        if self.doc.exists() && !self.blocked && stamp(&self.doc) != self.seen {
            agent_waits(self);
            return;
        }
        let after = weight(&self.engine.model);
        let shrank = start_weight > 0 && after * 4 < start_weight * 3;
        // a call that has run for a while is about to replace the file for the first time: the file as it is now is kept in
        // Backups first, whatever the backup schedule says, so a crash later in the call can be taken back from there
        let long_call = first_write && ran_for >= self.flush_every;
        let force = if shrank || destructive {
            Some(Force { origin: Origin::Agent, kind: if destructive { "del" } else { "shr" }, big: false })
        } else if long_call {
            Some(Force { origin: Origin::Agent, kind: "big", big: true })
        } else {
            None
        };
        match self.save_inner(force, true) {
            Ok(bytes) => {
                self.dirty = false;
                if let Some(c) = self.call.as_mut() {
                    c.flushed = true;
                    c.written = bytes;
                }
                self.record_agent_change();
                if self.notice.as_deref().is_some_and(|n| n.starts_with("Couldn't save")) {
                    self.notice = None;
                }
                if self.notice.is_none() && self.sticky.is_none() && self.engine.model.entity_count() > BIG_MODEL {
                    self.set_notice(big_model_text(self.engine.model.entity_count()));
                }
            }
            // somebody saved between the check at the top and the write: theirs wins
            Err(SaveErr::Moved) => agent_waits(self),
            Err(SaveErr::Fail(e)) => {
                self.dirty = true;
                if !self.held {
                    self.set_notice(format!("Couldn't save your model: {e}"));
                }
                if let Some(c) = self.call.as_mut() {
                    c.pending = true;
                }
            }
        }
    }

    /// The call ended well: write what is left. A line for the agent if somebody else's save was kept over part of it.
    pub fn finish_call(&mut self) -> Option<String> {
        self.flush_call();
        let c = self.call.take()?;
        if c.held_back {
            return Some("Note: someone else saved this model while the call ran, so their version stays in the file. What the call did after that could NOT be set aside (see the person's notice), so it was not saved.".to_string());
        }
        c.lost.then(|| "Note: someone else saved this model while the call ran, so their version was kept, and what the call had done before that was set aside (the person was told).".to_string())
    }

    /// The call ran out of time: take back what it did, and never somebody else's work with it.
    ///
    /// When nobody else has written the file during the call, the model goes back to what it was when the call began and
    /// the file with it. When somebody has (the app, another engine: their save was loaded while the call ran, or is in the
    /// file now), what the call changed is worked out entry by entry (points, edges, faces, groups, placements... by id) and
    /// taken off THEIR file: an entry the call changed and nobody else touched goes back to what it was before the call; an
    /// entry somebody else changed too stays as they have it, and the call's version of the model is set aside in an
    /// `unsaved-*` file. This does not depend on the undo list (which is capped). Whatever is written is checked against the
    /// file under the lock right before the rename (see `save_inner`); when the file moved meanwhile the plan is made again
    /// on the newer file. Returns what to tell the agent when it is not a whole, clean rollback.
    pub fn abort_call(&mut self) -> Option<String> {
        let c = self.call.take()?;
        let CallMode { snap, loads, was_dirty, flushed, lost, written, segs, base, unknown, .. } = c;
        let undo_len = Engine::snapshot_undo_len(&snap);
        let batch_was_open = Engine::snapshot_batch_open(&snap);
        if !self.persist {
            self.engine.rollback_call(snap);
            return None;
        }
        if self.engine.in_batch() && batch_was_open {
            // a batch that was open when the call began is still open: the file is written when it ends (and is checked then)
            if self.loads != loads {
                return Some(KEPT_THEIRS.into());
            }
            self.engine.rollback_call(snap);
            self.unsaved_batch = true;
            return None;
        }
        let theirs_in_file = self.doc.exists() && !self.blocked && stamp(&self.doc) != self.seen;
        let theirs_in_model = self.loads != loads || lost;
        if !theirs_in_file && !theirs_in_model {
            self.engine.rollback_call(snap);
            if !flushed {
                // the file never held any of it
                self.dirty = was_dirty;
                return None;
            }
            return match self.save_inner(Some(Force { origin: Origin::Agent, kind: "big", big: true }), false) {
                Ok(_) => {
                    self.dirty = false;
                    None
                }
                Err(SaveErr::Fail(e)) => {
                    self.dirty = true;
                    self.set_notice(format!("Couldn't save your model: {e}"));
                    None
                }
                // somebody saved in the moments it took to put the model back and write it: theirs is in the file now
                Err(SaveErr::Moved) => {
                    let base = self.engine.model.clone();
                    self.revert_call(base, vec![], written, unknown, undo_len, batch_was_open)
                }
            };
        }
        // somebody else's work is in the picture: never write the model the call began with
        if self.engine.in_batch() {
            self.engine.close_batch();
        }
        let base = base.unwrap_or_else(|| Engine::into_snapshot_model(snap));
        self.revert_call(base, segs, written, unknown, undo_len, batch_was_open)
    }

    /// Take what an aborted call wrote off the file as it stands now (see `abort_call`). `base`: the model the call's last
    /// changes are measured from; `segs`: what it had written before that; `written`: its last write.
    fn revert_call(&mut self, base: crate::model::Model, mut segs: Vec<crate::history::Patch>, written: Option<Vec<u8>>, mut unknown: Option<String>, undo_len: usize, batch_was_open: bool) -> Option<String> {
        if let Some(bytes) = &written {
            match serde_json::from_slice::<crate::model::Model>(bytes) {
                Ok(g) => {
                    let seg = crate::history::patch_between(&base, &g);
                    if segs.iter().map(|s| s.approx_bytes()).sum::<usize>() + seg.approx_bytes() > MAX_EFFECT_BYTES {
                        unknown = Some("what the call did is too big to be taken back entry by entry".into());
                    } else if !seg.is_empty() {
                        segs.push(seg);
                    }
                }
                Err(_) => unknown = Some("what the call wrote could not be read back".into()),
            }
        }
        let in_file = !segs.is_empty() || unknown.is_some();
        let mut conflicts: Vec<String> = vec![];
        let mut failed: Option<String> = None;
        let mut settled = false;
        // the file was unreadable or gone, so the model before the call was written back (not "their work")
        let mut rewrote: Option<bool> = None;
        for attempt in 0..REPLANS {
            conflicts.clear();
            failed = None;
            rewrote = None;
            // the last tries hold the lock from reading the file to writing it, so a person who saves every second can't keep
            // moving it under the plan (their save waits a moment instead, never longer than the lock wait)
            let locked = attempt + 2 >= REPLANS;
            let _held = if locked {
                match lock_for(&self.doc, LOCK_WAIT) {
                    Ok(l) => l,
                    Err(()) => {
                        failed = Some("another program kept the model file locked".into());
                        break;
                    }
                }
            } else {
                None
            };
            // memory becomes the file as it is now (the call's commands that were never written go with the old memory)
            let readable = if self.doc.exists() && !self.blocked {
                self.seen = stamp(&self.doc);
                self.dirty = false;
                self.load(false)
            } else {
                false
            };
            if !readable {
                // no file, or one that couldn't be read (it was set aside): the model goes back to what the call's last
                // changes are measured from, and is written
                self.engine.replace_after_call(base.clone(), undo_len, batch_was_open);
                rewrote = Some(self.doc.exists());
            } else if !in_file {
                // none of the call ever reached the file: the file is somebody else's, and that is all there is
                self.keep_theirs(undo_len, batch_was_open);
                settled = true;
                break;
            } else if let Some(why) = &unknown {
                failed = Some(why.clone());
                self.keep_theirs(undo_len, batch_was_open);
                settled = true;
                break;
            } else {
                let mut cur: Option<crate::model::Model> = None;
                for seg in segs.iter().rev() {
                    let r = seg.revert_partial(cur.as_ref().unwrap_or(&self.engine.model));
                    match r {
                        Ok((m, c)) => {
                            conflicts.extend(c);
                            cur = Some(m);
                        }
                        Err(e) => {
                            failed = Some(e);
                            break;
                        }
                    }
                }
                match (failed.is_some(), cur) {
                    (false, Some(m)) => self.engine.replace_after_call(m, undo_len, batch_was_open),
                    _ => {
                        // not a consistent result: theirs is kept as it is
                        self.keep_theirs(undo_len, batch_was_open);
                        settled = true;
                        break;
                    }
                }
                conflicts.sort();
                conflicts.dedup();
            }
            match self.save_core(Some(Force { origin: Origin::Agent, kind: "big", big: true }), false, locked) {
                Ok(_) => {
                    self.dirty = false;
                    settled = true;
                    break;
                }
                Err(SaveErr::Moved) => continue,
                Err(SaveErr::Fail(e)) => {
                    self.dirty = true;
                    self.set_notice(format!("Couldn't save your model: {e}"));
                    failed = Some(format!("the model couldn't be saved ({e})"));
                    settled = true;
                    break;
                }
            }
        }
        if !settled {
            if failed.is_none() {
                failed = Some("the file kept changing under it".into());
            }
            conflicts.clear();
            // memory is the file as it is now (never a half-done plan), and what the call wrote is set aside below
            if self.doc.exists() && !self.blocked {
                self.seen = stamp(&self.doc);
                self.dirty = false;
                self.load(false);
            }
            self.keep_theirs(undo_len, batch_was_open);
        }
        // theirs is never lost; the call's version is kept where somebody can find it when part of it stays in the file
        let aside = if (failed.is_some() || !conflicts.is_empty()) && in_file {
            let why = if failed.is_some() { "An AI agent's call ran out of time and could not be fully undone." } else { "An AI agent's call ran out of time and was undone." };
            written.as_ref().and_then(|b| self.stash_bytes(b, why, Whose::Agent).ok())
        } else {
            None
        };
        // the person is told too, not only the agent: part of the call may still be in their model
        if failed.is_some() && aside.is_none() && in_file {
            self.set_notice("An AI agent's call ran out of time, and part of what it did could not be taken back from the model file. File > Restore from backup... has the version from before.".into());
        }
        let where_ = aside.map(|p| format!(" The call's version of the model is set aside in {}.", p.display())).unwrap_or_default();
        Some(if let Some(why) = failed {
            format!("This call used up its time, and someone else saved this model while it ran. Their work was kept, but what the call did was NOT fully taken back ({why}).{where_}")
        } else if !conflicts.is_empty() {
            let shown: Vec<&str> = conflicts.iter().take(3).map(String::as_str).collect();
            format!("This call used up its time, and someone else saved this model while it ran. What the call did was taken back, and their work was kept; {} thing{} the call changed {} also changed by someone else and {} left as they have {} ({}{}).{where_}", conflicts.len(), if conflicts.len() == 1 { "" } else { "s" }, if conflicts.len() == 1 { "was" } else { "were" }, if conflicts.len() == 1 { "was" } else { "were" }, if conflicts.len() == 1 { "it" } else { "them" }, shown.join(", "), if conflicts.len() > 3 { ", ..." } else { "" })
        } else if let Some(unreadable) = rewrote {
            format!("This call used up its time, and the model file {}. The model as it was before the call was written back{}.", if unreadable { "had been replaced by something that couldn't be read" } else { "had been deleted" }, if unreadable { ", and a copy of that file was set aside beside it" } else { "" })
        } else {
            "This call used up its time, and someone else saved this model while it ran. Everything the call did was taken back, entry by entry, and their work was kept.".to_string()
        })
    }

    /// The model in memory is the file as it is, with only the aborted call's steps dropped from the history.
    fn keep_theirs(&mut self, undo_len: usize, batch_was_open: bool) {
        let m = std::mem::take(&mut self.engine.model);
        self.engine.replace_after_call(m, undo_len, batch_was_open);
    }

    /// Write the model to the live file (atomically), keeping backups.
    pub fn save(&mut self, force_backup: bool) -> Result<(), String> {
        self.save_with(force_backup.then_some(Force { origin: Origin::Ui, kind: "big", big: true })).map_err(|e| e.to_string())
    }

    fn save_with(&mut self, force: Option<Force>) -> Result<(), SaveErr> {
        self.save_inner(force, false).map(|_| ())
    }

    /// Tests only: called as a save passes `serialized` (the model is ready as bytes, the lock isn't taken yet), `locked`
    /// (the lock is held and the file is as expected, nothing written) and `written` (the new file is in place, the lock
    /// still held). A second program can act at exactly those moments.
    #[doc(hidden)]
    pub fn set_save_hook(&mut self, hook: Option<Box<dyn FnMut(&str) + Send>>) {
        self.hook = hook;
    }

    /// Tests only: how often a call's commands are written (the default is a couple of seconds).
    #[doc(hidden)]
    pub fn set_call_flush_every(&mut self, every: Duration) {
        self.flush_every = every;
    }

    /// Tests only: setting work aside fails (as on a full disk).
    #[doc(hidden)]
    pub fn set_aside_fault(&mut self, on: bool) {
        self.aside_fault = on;
    }

    fn hit(&mut self, phase: &str) {
        if let Some(mut h) = self.hook.take() {
            h(phase);
            self.hook = Some(h);
        }
    }

    /// The one way the live file is written, for every writer (the app's save, an agent's flush, a rollback):
    ///  1. the model is serialised (no lock held: this is the slow part);
    ///  2. the lock file beside the model is taken (an advisory lock, so two programs never write together);
    ///  3. with the lock held, the file must still be what this program last read or wrote (`FileStamp`: time, size,
    ///     identity); if it is not, nothing is written (`Moved`) and the caller plans again on the newer file;
    ///  4. the outgoing file is backed up if the schedule says so;
    ///  5. the new bytes go to a temp file, flushed, and the stamp is checked once more immediately before the rename
    ///     (a program that ignores the lock still has to get past that), then renamed over the file.
    /// A crash anywhere leaves the old file or the whole new one. `keep`: give the bytes back (a call remembers its last write).
    fn save_inner(&mut self, force: Option<Force>, keep: bool) -> Result<Option<Vec<u8>>, SaveErr> {
        self.save_core(force, keep, false)
    }

    /// `save_inner`; `held`: the caller already holds the lock on the model file (a rollback's last tries), so it is not taken again.
    fn save_core(&mut self, force: Option<Force>, keep: bool, held: bool) -> Result<Option<Vec<u8>>, SaveErr> {
        // a model from a newer SketchCraft is only looked at: never written to the file (see `format`)
        if let Some(ro) = &self.engine.read_only {
            return Err(SaveErr::Fail(ro.refusal()));
        }
        if self.blocked {
            return Err(SaveErr::Fail("saving is off because the model file couldn't be read (see the notice)".into()));
        }
        if self.held {
            return Err(SaveErr::Fail("saving is paused because the model file couldn't be read when the program started (see the notice): restore a copy or start a new model first".into()));
        }
        if self.engine.oversize {
            return Err(SaveErr::Fail(format!("the model is over the size limits ({} points, edges and faces, about {} MB): delete parts of it and saving resumes", self.engine.model.entity_count(), self.engine.model.size_estimate() >> 20)));
        }
        self.engine.model.check_sane().map_err(|e| SaveErr::Fail(e.to_string()))?;
        let bytes = crate::format::to_vec(&self.engine.model).map_err(|e| SaveErr::Fail(e.to_string()))?;
        if let Some(dir) = self.doc.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| SaveErr::Fail(e.to_string()))?;
        }
        let _lock = if held {
            None
        } else {
            self.hit("serialized");
            match lock_for_why(&self.doc, LOCK_WAIT) {
                Ok((l, why)) => {
                    // saving carries on (the file is still checked before it is replaced), and the person is told once
                    if let (None, Some(why), false) = (&l, why, self.lock_told) {
                        self.lock_told = true;
                        let msg =
                            format!("Saving works, but the lock that keeps two programs from saving this model at the same moment can't be used here: {why}. If another program saves at the very same moment, one of the two saves can be missed.");
                        let note = match self.notice.take() {
                            Some(n) => format!("{n} {msg}"),
                            None => msg,
                        };
                        self.set_notice(note);
                    }
                    l
                }
                Err(()) => return Err(SaveErr::Fail("another program is saving this model and didn't finish; it will be tried again".into())),
            }
        };
        let expected = self.seen;
        let doc = self.doc.clone();
        // a file that is not there is nobody's to protect (it was deleted: it is written again)
        let unchanged = move || !doc.exists() || stamp(&doc) == expected;
        if !unchanged() {
            return Err(SaveErr::Moved);
        }
        self.hit("locked");
        self.backup(force);
        let doc = self.doc.clone();
        let guard = move || !doc.exists() || stamp(&doc) == expected;
        match write_atomic_guarded(&self.doc, &bytes, guard).map_err(|e| SaveErr::Fail(e.to_string()))? {
            Written::Refused => return Err(SaveErr::Moved),
            Written::Done(st) => self.seen = st.or_else(|| stamp(&self.doc)),
        }
        self.hit("written");
        Ok(keep.then_some(bytes))
    }

    /// Keep the file as it is now in `Backups/`, if the last backup is old enough (or `force`d). A file
    /// that is blank, unreadable or identical to the newest backup is not worth a slot. A forced backup
    /// is a "pre" one, named for what asked for it; they are paced per origin (15 s for the person's own
    /// steps, 60 s for an agent's or a plugin's) unless the model changed size a lot, and each origin has
    /// its own quota (see `backups`), so an agent's loop can't push out the person's backups.
    fn backup(&mut self, force: Option<Force>) {
        let Some(dir) = self.doc.parent().map(|d| d.join("Backups")) else {
            return;
        };
        let now = SystemTime::now();
        let timed_due = self.last_backup.is_none_or(|t| now.duration_since(t).unwrap_or_default() >= BACKUP_EVERY);
        if (force.is_none() && !timed_due) || !self.doc.is_file() {
            return;
        }
        let Ok(bytes) = std::fs::read(&self.doc) else {
            return;
        };
        // blank or unreadable files are not worth a slot. A big file is certainly not blank (an empty model is a few hundred
        // bytes), so it is only checked for being well-formed JSON, which on a model of millions of entities takes a
        // third of the time of reading it into a model (and no memory)
        if bytes.len() < (1 << 20) {
            match serde_json::from_slice::<crate::model::Model>(&bytes) {
                Ok(m) if !m.is_blank() => {}
                _ => return,
            }
        } else if serde_json::from_slice::<serde::de::IgnoredAny>(&bytes).is_err() {
            return;
        }
        let size = bytes.len() as u64;
        // pacing: a forced backup that comes too soon after the last one from the same origin is held back,
        // unless the file is more than twice or less than half the size of that one (or the force is exempt)
        let force = force.filter(|f| {
            let pace = if f.origin == Origin::Ui { FORCE_EVERY_UI } else { FORCE_EVERY_OTHER };
            match self.last_forced.get(&f.origin) {
                None => true,
                // the person's own steps may beat the clock when the model changed a lot; an agent's or a plugin's may not
                // (a loop alternating two sizes would otherwise push the person's last good copy out of its slots)
                Some((t, sz)) => f.big || now.duration_since(*t).unwrap_or(pace) >= pace || (f.origin == Origin::Ui && (size > sz.saturating_mul(2) || size.saturating_mul(2) < *sz)),
            }
        });
        if force.is_none() && !timed_due {
            return;
        }
        let stem = self.doc.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "live".into());
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let t = epoch(now);
        let all = backups::list(&dir, &stem, t);
        if all.last().is_some_and(|newest| std::fs::read(&newest.path).is_ok_and(|b| b == bytes)) {
            self.last_backup = Some(now);
            return;
        }
        let mut n = 0;
        let target = loop {
            // a name in use in either spelling (plain or pre, any kind) is skipped
            let taken = backups::list(&dir, &stem, t).iter().any(|b| b.secs == t && b.n == n);
            let name = backups::file_name(&stem, t, n, force.map(|f| (Some(f.kind), f.origin)));
            if !taken && !dir.join(&name).exists() {
                break dir.join(name);
            }
            n += 1;
        };
        // whole or not at all: a crash or a full disk must not leave a half file that counts as a slot
        if write_atomic(&target, &bytes).is_ok() {
            // a backup is as private as the model's own side files
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).ok();
            }
            self.last_backup = Some(now);
            if let Some(f) = force {
                self.last_forced.insert(f.origin, (now, size));
            }
            for p in backups::to_delete(&backups::list(&dir, &stem, t), t) {
                std::fs::remove_file(p).ok();
            }
        }
    }

    /// Reload if someone else saved since we last looked. True if reloaded. Not inside a batch
    /// (a plugin run): theirs is picked up when it ends.
    pub fn sync(&mut self) -> bool {
        if !self.persist {
            return false;
        }
        self.tick();
        if self.engine.in_batch() {
            return false;
        }
        self.retry_blocked();
        let now = stamp(&self.doc);
        if now.is_some() && now != self.seen {
            if self.blocked || self.dirty {
                // memory may hold work the file doesn't: set it aside first (and don't reload if that fails)
                let why = if self.call.is_some() { "Another program saved this model while an agent was working." } else { "Another program saved this model while yours couldn't be saved." };
                let ok = self.take_theirs(false, why, if self.call.is_some() { Whose::Agent } else { Whose::Person });
                if ok {
                    self.seen = now;
                }
                return ok;
            }
            self.seen = now;
            let ok = self.load(false);
            if ok {
                self.announce_agent();
            }
            return ok;
        }
        false
    }

    /// Saving was switched off because the file couldn't be read and couldn't be copied aside. It is switched back on when
    /// the cause is gone: the file can be read again (a permission put back, a cloud file downloaded), or it was moved away
    /// (the next save writes a new one). Work in memory that the file doesn't hold is set aside first, like another program's save.
    fn retry_blocked(&mut self) {
        if !self.blocked {
            return;
        }
        if !self.doc.exists() {
            self.blocked = false;
            self.seen = None;
            if self.sticky.is_none() {
                self.notice = None;
            }
            // what memory holds is written now, so it doesn't wait for the next change
            if self.dirty && self.save_with(None).is_ok() {
                self.dirty = false;
            }
        } else if std::fs::File::open(&self.doc).is_ok() {
            // readable now (a file that is only still unreadable costs nothing to look at again): try it, but not more than every few seconds
            if self.retry_after.is_some_and(|t| SystemTime::now() < t) {
                return;
            }
            self.retry_after = Some(SystemTime::now() + Duration::from_secs(5));
            let now = stamp(&self.doc);
            if self.take_theirs(false, "The model file can be read again.", Whose::Person) {
                self.seen = now;
            }
        }
    }

    // ---- What an agent changed ------------------------------------------------------------

    /// The small hidden file beside the model that remembers agent saves until the person's window has said so.
    fn agent_note_path(&self) -> PathBuf {
        self.doc.with_file_name(format!(".{}.agent-activity", self.stem()))
    }

    /// An agent's save went into the file: remember it (how many, first and last time) for the person's window.
    fn record_agent_change(&mut self) {
        if !self.persist {
            return;
        }
        let p = self.agent_note_path();
        let old: Value = std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
        let now = epoch(SystemTime::now());
        let n = old["saves"].as_u64().unwrap_or(0).saturating_add(1);
        let first = old["first"].as_u64().unwrap_or(now);
        let _ = crate::safety::write_atomic(&p, json!({"saves": n, "first": first, "last": now}).to_string().as_bytes());
    }

    /// This is the person's own window: it tells them about agent changes (once), including ones made while
    /// the app was closed. Call it after opening the model.
    pub fn as_person_view(&mut self) {
        self.person_view = true;
        self.announce_agent();
    }

    fn announce_agent(&mut self) {
        if !self.person_view || !self.persist {
            return;
        }
        let p = self.agent_note_path();
        let Some(v) = std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else {
            return;
        };
        let _ = std::fs::remove_file(&p);
        let (n, last) = (v["saves"].as_u64().unwrap_or(1), v["last"].as_u64().unwrap_or(0));
        let ago = epoch(SystemTime::now()).saturating_sub(last);
        let when = if ago < 90 { "just now".to_string() } else if ago < 5400 { format!("{} minutes ago", ago / 60) } else if ago < 172_800 { format!("{} hours ago", ago / 3600) } else { format!("{} days ago", ago / 86_400) };
        self.set_notice(format!(
            "Agent changed this model: {n} save{} by an AI agent, the last {when}. They aren't in your Undo list (an agent's steps belong to its own session); File > Restore from backup... has the version from before.",
            if n == 1 { "" } else { "s" }
        ));
    }

    pub fn scene(&self) -> Value {
        // a model that is damaged in some way nobody thought of must not take the bridge or the app down on every poll
        // laid out, the view draws the laid-out copy of the model as it is now, and says which parts it shows
        let laid = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.engine.laid_out_view())).ok().flatten();
        let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match &laid {
            Some(v) => crate::scene::build(&v.model, None),
            None => crate::scene::build(&self.engine.model, self.engine.ctx),
        }));
        let mut s = built.unwrap_or_else(|_| json!({"faces": [], "edges": [], "error": "SketchCraft couldn't draw this model (it holds something inconsistent). Restore from a backup (File > Restore from backup...) or open another file."}));
        s["doc"] = json!(self.doc);
        s["laid_out"] = laid.as_ref().map_or(Value::Null, crate::parts_layout::summary);
        s["read_only"] = json!(self.engine.read_only.as_ref().map(|r| r.reasons.clone()));
        // who made the step Undo (and Redo) would take back, so the buttons can say
        s["undo_origin"] = json!(self.engine.undo_origin());
        s["redo_origin"] = json!(self.engine.redo_origin());
        if self.engine.read_only.is_some() {
            // stays for as long as the model is open this way
            s["notice"] = json!(crate::format::NOTICE);
        } else if let (Some(n), Some(at)) = (&self.notice, self.notice_at) {
            if SystemTime::now().duration_since(at).unwrap_or_default() < NOTICE_FOR || self.blocked || self.dirty || self.engine.oversize || self.sticky.as_ref().is_some_and(|p| p.exists()) {
                s["notice"] = json!(n);
                if let Some((r, what)) = &self.notice_repair {
                    r.add_to(&mut s["notice_repair"]);
                    s["notice_repair"]["what"] = json!(what);
                }
            }
        }
        s
    }

    // ---- Restore from backup -------------------------------------------------------------

    fn stem(&self) -> String {
        self.doc.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "live".into())
    }

    /// Everything the person can restore from: backups (newest first), then work set aside and copies of
    /// unreadable files (newest first).
    fn restorable(&self) -> Vec<Value> {
        let t = epoch(SystemTime::now());
        let dir = self.doc.parent().map(|d| d.join("Backups")).unwrap_or_default();
        let mut items: Vec<Value> = vec![];
        let mut all: Vec<Backup> = backups::list(&dir, &self.stem(), t);
        all.reverse();
        for b in all {
            let what = if b.pre { backups::kind_label(b.kind.as_deref()).to_string() } else { "Timed backup".to_string() };
            let by = if b.pre && b.origin != Origin::Ui { format!(" by {}", b.origin.name()) } else { String::new() };
            items.push(json!({"kind": "backup", "name": b.name(), "secs": b.secs, "size": b.size, "label": format!("{what}{by}")}));
        }
        // every file named exactly as one of ours is listed, so a lost index never hides a recovery file; the ones this
        // program can vouch for (named in its index, or written by this run) come as they are, the rest say "not verified"
        let indexed = backups::read_index(&self.doc);
        let mut side: Vec<_> = backups::side_files(&self.doc);
        side.reverse();
        for (kind, secs, path, size) in side {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            let vouched = name.as_ref().is_some_and(|n| indexed.contains(n) || self.own_side.contains(n));
            let label = if vouched { kind.label().to_string() } else { format!("{} (not verified: this program didn't note it)", kind.label()) };
            items.push(json!({"kind": kind.word(), "name": name, "secs": secs, "size": size, "label": label, "verified": vouched}));
        }
        items
    }

    /// The file a request names, found by name in the folders we write to: a bare name that is exactly one of
    /// our backups or side files for this model, a plain file (never a link), of readable size.
    fn restore_source(&self, kind: &str, name: &str) -> Result<PathBuf, String> {
        if name.is_empty() || name.contains(['/', '\\', '\0']) || name.starts_with('.') {
            return Err("that isn't one of the model's backups".into());
        }
        let stem = self.stem();
        let path = match kind {
            "backup" if backups::parse(&stem, name).is_some() => self.doc.parent().map(|d| d.join("Backups").join(name)),
            "unsaved" | "damaged" if backups::parse_side(&stem, name).is_some_and(|(s, _)| s.word() == kind) => self.doc.parent().filter(|d| !d.as_os_str().is_empty()).map(|d| d.join(name)).or_else(|| Some(PathBuf::from(name))),
            _ => None,
        };
        let path = path.ok_or_else(|| "that isn't one of the model's backups".to_string())?;
        let meta = std::fs::symlink_metadata(&path).map_err(|_| "that backup is gone".to_string())?;
        if !meta.is_file() {
            return Err("that backup isn't a plain file".into());
        }
        if meta.len() > MAX_RESTORE_BYTES {
            return Err("that file is too big to restore from here".into());
        }
        Ok(path)
    }

    fn restore_command(&mut self, name: &str, cmd: &Value) -> Value {
        if !self.persist {
            return json!({"ok": false, "error": "this model isn't saved anywhere, so it has no backups"});
        }
        if name == "list_backups" {
            return json!({"ok": true, "dir": self.doc.parent().map(|d| d.join("Backups")), "items": self.restorable()});
        }
        if self.engine.in_batch() {
            return json!({"ok": false, "error": "a plugin is still running; restore when it has finished"});
        }
        let (kind, file) = (cmd["kind"].as_str().unwrap_or("backup"), cmd["name"].as_str().unwrap_or(""));
        let src = match self.restore_source(kind, file) {
            Ok(p) => p,
            Err(e) => return json!({"ok": false, "error": e}),
        };
        let mode = cmd["mode"].as_str().unwrap_or("replace");
        if mode == "copy" {
            // a copy beside the model, in a folder of its own, never over anything
            let dir = self.doc.parent().filter(|d| !d.as_os_str().is_empty()).map(|d| d.join("Restored")).unwrap_or_else(|| PathBuf::from("Restored"));
            if let Err(e) = std::fs::create_dir_all(&dir) {
                return json!({"ok": false, "error": format!("couldn't make the Restored folder: {e}")});
            }
            let bytes = match std::fs::read(&src) {
                Ok(b) => b,
                Err(e) => return json!({"ok": false, "error": format!("couldn't read it: {e}")}),
            };
            if serde_json::from_slice::<crate::model::Model>(&bytes).is_err() {
                return json!({"ok": false, "error": "that file isn't a model SketchCraft can read"});
            }
            let base = file.trim_end_matches(".sketchcraft");
            let mut k = 0;
            loop {
                let p = dir.join(if k == 0 { format!("{base}-copy.sketchcraft") } else { format!("{base}-copy-{k}.sketchcraft") });
                match std::fs::OpenOptions::new().write(true).create_new(true).open(&p) {
                    Ok(mut f) => {
                        use std::io::Write;
                        return match f.write_all(&bytes) {
                            Ok(()) => json!({"ok": true, "path": p, "mode": "copy"}),
                            Err(e) => {
                                json!({"ok": false, "error": format!("couldn't write the copy: {e}")})
                            }
                        };
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && k < 1000 => k += 1,
                    Err(e) => return json!({"ok": false, "error": format!("couldn't write the copy: {e}")}),
                }
            }
        }
        self.sync();
        // a model from a newer SketchCraft is only looked at: a restore would write over its file (the copy option keeps every byte)
        if let Some(ro) = &self.engine.read_only {
            return json!({"ok": false, "error": format!("{} \"Save a copy\" of the backup still works.", ro.refusal()), "read_only": true});
        }
        let bytes = match std::fs::read(&src) {
            Ok(b) => b,
            Err(e) => return json!({"ok": false, "error": format!("couldn't read it: {e}")}),
        };
        // and a backup from a newer SketchCraft would come back without what this version doesn't know
        if let Some(ro) = crate::format::check(&bytes) {
            return json!({"ok": false, "error": format!("That backup was saved by a newer SketchCraft ({}), so it can't be restored over your model by this version: update SketchCraft to restore it. \"Save a copy\" keeps it as it is.", ro.reasons.join("; ")), "read_only": true});
        }
        let model: crate::model::Model = match serde_json::from_slice(&bytes) {
            Ok(m) => m,
            Err(e) => return json!({"ok": false, "error": format!("that file isn't a model SketchCraft can read ({e})")}),
        };
        let repair = match self.engine.replace_model(model) {
            Ok(r) => r,
            Err(e) => return json!({"ok": false, "error": e.to_string()}),
        };
        // the person chose what to carry on with: saving resumes
        self.held = false;
        self.blocked = false;
        let skipped = crate::safety::take_skipped();
        if !skipped.is_empty() {
            self.set_notice(skipped_text(&skipped));
        }
        self.set_repair_notice(repair.clone(), "backup");
        // the model it replaced is kept in Backups first (the outgoing file), and the restore is one undoable step
        let mut saved = self.save_with(Some(Force { origin: Origin::Ui, kind: "rst", big: true }));
        // the person asked for this version: a save that landed meanwhile is the outgoing file, kept in Backups first
        for _ in 0..3 {
            if !matches!(saved, Err(SaveErr::Moved)) {
                break;
            }
            self.seen = stamp(&self.doc);
            saved = self.save_with(Some(Force { origin: Origin::Ui, kind: "rst", big: true }));
        }
        match saved {
            Ok(()) => {
                self.dirty = false;
                let mut v = json!({"ok": true, "mode": "replace", "faces": self.engine.model.faces.len()});
                repair.add_to(&mut v);
                v
            }
            Err(e) => {
                self.dirty = true;
                let e = e.to_string();
                self.set_notice(format!("Couldn't save your model: {e}"));
                let mut v = json!({"ok": true, "mode": "replace", "faces": self.engine.model.faces.len(), "save_error": e});
                repair.add_to(&mut v);
                v
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tests_never_default_to_the_real_folder() {
        // no SKETCHCRAFT_DOC, in a test program: a throwaway folder, whatever HOME is
        let p = doc_path(None, true, Some(PathBuf::from("/Users/someone")));
        assert!(p.starts_with(std::env::temp_dir()), "{p:?}");
        assert!(!p.to_string_lossy().contains("Documents/SketchCraft"));
        // outside tests it is the person's folder, and SKETCHCRAFT_DOC wins in both
        assert_eq!(doc_path(None, false, Some(PathBuf::from("/Users/someone"))), PathBuf::from("/Users/someone/Documents/SketchCraft/live.sketchcraft"));
        assert_eq!(doc_path(Some("/x/y.sketchcraft".into()), true, None), PathBuf::from("/x/y.sketchcraft"));
        // and this very process is recognized as a test program
        assert!(running_as_cargo_test());
    }
}
