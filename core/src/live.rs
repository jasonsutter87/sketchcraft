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
use crate::safety::write_atomic;
use crate::Engine;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Side files (`.damaged-*`, `.unsaved-*`) are only removed past this many (of one kind) or this much disk,
/// oldest first, never the newest, and always with a notice.
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

pub struct Live {
    pub engine: Engine,
    pub doc: PathBuf,
    seen: Option<SystemTime>,
    /// no file: an in-memory model (an agent with no --doc)
    persist: bool,
    last_backup: Option<SystemTime>,
    /// something the person should know (a damaged file was set aside, a save failed)
    pub notice: Option<String>,
    notice_at: Option<SystemTime>,
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
}

/// "Left out: ..." for a notice: what was skipped, with the reasons.
fn skipped_text(list: &[String]) -> String {
    format!("{} picture{} left out: {}.", list.len(), if list.len() == 1 { " was" } else { "s were" }, list.join("; "))
}

fn big_model_text(n: usize) -> String {
    format!("This is a big model ({n} points, edges and faces). Everything works, but each change takes a second or more to save and the view takes a few seconds to redraw.")
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
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
        live.seen = mtime(&live.doc);
        live
    }

    fn blank(engine: Engine, doc: PathBuf, persist: bool) -> Self {
        Live { engine, doc, seen: None, persist, last_backup: None, notice: None, notice_at: None, unsaved_batch: false, batch_weight: 0, batch_destructive: false, blocked: false, retry_after: None, dirty: false, sticky: None, last_forced: HashMap::new(), held: false, own_side: vec![], person_view: false }
    }

    /// A model that is not saved anywhere.
    pub fn memory(engine: Engine) -> Self {
        Self::blank(engine, PathBuf::new(), false)
    }

    /// Open the file into the engine. If it can't be read, set it aside (a copy
    /// that is never overwritten) and say so; `at_start`: the engine stays empty.
    fn load(&mut self, at_start: bool) -> bool {
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
            if let Some(g) = active.filter(|g| !at_start && self.engine.model.groups.contains_key(g)) {
                self.engine.model.active_group = Some(g);
            }
            // the file reads fine now: whatever was wrong is over
            self.blocked = false;
            self.held = false;
            self.unsaved_batch = false;
            self.dirty = false;
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
        let aside = self.unique_damaged_path();
        match std::fs::copy(&self.doc, &aside) {
            Ok(_) => {
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
    }

    fn unique_damaged_path(&self) -> PathBuf {
        let t = epoch(SystemTime::now());
        let mut n = 0;
        loop {
            let p = self.doc.with_extension(format!("damaged-{t}{}", if n == 0 { String::new() } else { format!("-{n}") }));
            if !p.exists() {
                return p;
            }
            n += 1;
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

    /// Set the model in memory aside (`<name>.unsaved-<time>.sketchcraft` beside the file) before
    /// something replaces it, and keep telling the person until that file is dealt with.
    fn stash(&mut self, why: &str) -> Result<PathBuf, String> {
        let t = epoch(SystemTime::now());
        let mut n = 0;
        let path = loop {
            let p = self.doc.with_extension(format!("unsaved-{t}{}.sketchcraft", if n == 0 { String::new() } else { format!("-{n}") }));
            if !p.exists() {
                break p;
            }
            n += 1;
        };
        let bytes = crate::format::to_vec(&self.engine.model).map_err(|e| e.to_string())?;
        write_atomic(&path, &bytes).map_err(|e| e.to_string())?;
        self.index_side(&path);
        self.sticky = Some(path.clone());
        self.set_notice(format!("{why} Your changes were kept in {} (File > Restore from backup... lists it). This message stays until that file is moved or deleted.", path.display()));
        self.prune_side(Side::Unsaved);
        Ok(path)
    }

    /// Another client saved. Replace what is in memory with their file, after setting aside any work
    /// memory holds that the file doesn't (`ours_ahead`, or a failed save). False if the file wasn't taken.
    fn take_theirs(&mut self, ours_ahead: bool, why: &str) -> bool {
        if ours_ahead || self.dirty {
            if let Err(e) = self.stash(why) {
                self.set_notice(format!("{why} Your unsaved changes couldn't be set aside ({e}), so the other version was not loaded."));
                return false;
            }
        }
        self.load(false)
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
        if self.doc.exists() && !self.blocked && mtime(&self.doc) != self.seen {
            self.seen = mtime(&self.doc);
            self.take_theirs(true, "The plugin run was cut off and another program had saved the model meanwhile.");
            return;
        }
        let force = self.batch_force();
        if let Err(e) = self.save_with(force) {
            self.dirty = true;
            self.set_notice(format!("Couldn't save your model: {e}"));
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
        self.tick();
        let name = cmd["cmd"].as_str().unwrap_or("");
        // the person's own list of backups and the restore (only the app's own user; never a plugin or an agent)
        if matches!(name, "list_backups" | "restore_backup") {
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
        if !self.engine.in_batch() {
            self.sync();
        }
        let size = |e: &Engine| weight(&e.model);
        let (before, size_before, depth_before) = (self.engine.revision(), size(&self.engine), self.engine.undo_depth());
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
            // A plugin run's whole output is set aside first (a single command is just run again)
            if self.doc.exists() && !self.blocked && mtime(&self.doc) != self.seen {
                self.seen = mtime(&self.doc);
                let why = "Another program saved the model while a plugin was running.";
                let kept = if had_batch { self.stash(why).ok() } else { None };
                if had_batch && kept.is_none() {
                    return json!({"ok": false, "error": "Another program saved the model while the plugin ran, and the plugin's changes couldn't be set aside, so nothing was saved. Run it again."});
                }
                // what this command did is being thrown away: it is not a step to undo
                self.engine.drop_steps_after(depth_before);
                if self.load(false) {
                    return json!({"ok": false, "error": match kept {
                        Some(p) => format!("Someone else saved the model while that ran, so their version was kept. What the plugin made was set aside in {}.", p.display()),
                        None => "Someone else saved the model while that ran, so their version was kept and that change was not applied. Run it again.".into(),
                    }});
                }
            }
            match self.save_with(force) {
                Ok(()) => {
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

    /// Write the model to the live file (atomically), keeping backups.
    pub fn save(&mut self, force_backup: bool) -> Result<(), String> {
        self.save_with(force_backup.then_some(Force { origin: Origin::Ui, kind: "big", big: true }))
    }

    fn save_with(&mut self, force: Option<Force>) -> Result<(), String> {
        // a model from a newer SketchCraft is only looked at: never written to the file (see `format`)
        if let Some(ro) = &self.engine.read_only {
            return Err(ro.refusal());
        }
        if self.blocked {
            return Err("saving is off because the model file couldn't be read (see the notice)".into());
        }
        if self.held {
            return Err("saving is paused because the model file couldn't be read when the program started (see the notice): restore a copy or start a new model first".into());
        }
        if self.engine.oversize {
            return Err(format!("the model is over the size limits ({} points, edges and faces, about {} MB): delete parts of it and saving resumes", self.engine.model.entity_count(), self.engine.model.size_estimate() >> 20));
        }
        self.engine.model.check_sane().map_err(|e| e.to_string())?;
        let bytes = crate::format::to_vec(&self.engine.model).map_err(|e| e.to_string())?;
        if let Some(dir) = self.doc.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        self.backup(force);
        write_atomic(&self.doc, &bytes).map_err(|e| e.to_string())?;
        self.seen = mtime(&self.doc);
        Ok(())
    }

    /// Keep the file as it is now in `Backups/`, if the last backup is old enough (or `force`d). A file
    /// that is blank, unreadable or identical to the newest backup is not worth a slot. A forced backup
    /// is a "pre" one, named for what asked for it; they are paced per origin (15 s for the person's own
    /// steps, 60 s for an agent's or a plugin's) unless the model changed size a lot, and each origin has
    /// its own quota (see `backups`), so an agent's loop can't push out the person's backups.
    fn backup(&mut self, force: Option<Force>) {
        let Some(dir) = self.doc.parent().map(|d| d.join("Backups")) else { return };
        let now = SystemTime::now();
        let timed_due = self.last_backup.is_none_or(|t| now.duration_since(t).unwrap_or_default() >= BACKUP_EVERY);
        if (force.is_none() && !timed_due) || !self.doc.is_file() {
            return;
        }
        let Ok(bytes) = std::fs::read(&self.doc) else { return };
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
        let now = mtime(&self.doc);
        if now.is_some() && now != self.seen {
            if self.blocked || self.dirty {
                // memory may hold work the file doesn't: set it aside first (and don't reload if that fails)
                let ok = self.take_theirs(false, "Another program saved this model while yours couldn't be saved.");
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
            let now = mtime(&self.doc);
            if self.take_theirs(false, "The model file can be read again.") {
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
        let Some(v) = std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else { return };
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
        let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::scene::build(&self.engine.model, self.engine.ctx)));
        let mut s = built.unwrap_or_else(|_| json!({"faces": [], "edges": [], "error": "SketchCraft couldn't draw this model (it holds something inconsistent). Restore from a backup (File > Restore from backup...) or open another file."}));
        s["doc"] = json!(self.doc);
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
                            Err(e) => json!({"ok": false, "error": format!("couldn't write the copy: {e}")}),
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
        if let Err(e) = self.engine.replace_model(model) {
            return json!({"ok": false, "error": e.to_string()});
        }
        // the person chose what to carry on with: saving resumes
        self.held = false;
        self.blocked = false;
        let skipped = crate::safety::take_skipped();
        if !skipped.is_empty() {
            self.set_notice(skipped_text(&skipped));
        }
        // the model it replaced is kept in Backups first (the outgoing file), and the restore is one undoable step
        match self.save_with(Some(Force { origin: Origin::Ui, kind: "rst", big: true })) {
            Ok(()) => {
                self.dirty = false;
                json!({"ok": true, "mode": "replace", "faces": self.engine.model.faces.len()})
            }
            Err(e) => {
                self.dirty = true;
                self.set_notice(format!("Couldn't save your model: {e}"));
                json!({"ok": true, "mode": "replace", "faces": self.engine.model.faces.len(), "save_error": e})
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
