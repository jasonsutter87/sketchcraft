//! The command layer. Everything that changes or inspects a model goes
//! through `Engine::exec`, whether it comes from a mouse in the app, a script,
//! the CLI or an AI agent over MCP. Commands are JSON:
//!
//!   {"cmd":"rect","from":[0,0,0],"to":[8,12,0]}
//!   {"cmd":"pushpull","face":7,"distance":6}
//!
//! Each reply is JSON too: {"ok":true, ...} or {"ok":false,"error":"..."}.

use crate::export;
use crate::history::Step;
use crate::model::{Id, Model};
use crate::policy::Caller;
use crate::render;
use anyhow::{bail, Result};
use glam::DVec3;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

// `Engine::run`'s arms live in one file per area. Each works on the model being edited
// (inside `edit`, the open component's definition), borrowed field by field so an arm can
// still reach the engine's other fields; hence a macro, not a method.
macro_rules! active_model {
    ($engine:expr) => {
        match $engine.ctx {
            Some(d) => std::sync::Arc::make_mut(&mut $engine.model.definitions.get_mut(&d).unwrap().model),
            None => &mut $engine.model,
        }
    };
}

mod annotate;
mod build;
mod command;
mod draw;
mod drawings;
mod files;
mod groups;
mod library;
mod media;
mod paint;
mod rendering;
mod sections;
mod session;
mod shape;
mod sheets;
mod tables;
mod autodraft;
mod tags;

pub use command::{Command, Factor};


















fn axis_index(a: &str) -> Result<usize> {
    Ok(match a.to_ascii_lowercase().as_str() {
        "red" | "x" => 0,
        "green" | "y" => 1,
        "blue" | "z" => 2,
        _ => bail!("axis is red, green or blue (x, y, z)"),
    })
}












/// "#rrggbb" (or "rrggbb") to sRGB 0..1.
fn hex_rgb(s: &str) -> Result<[f32; 3]> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("a color is written #rrggbb, like #ffffff (got {s:?})");
    }
    let v = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).map(|x| x as f32 / 255.0);
    Ok([v(0)?, v(2)?, v(4)?])
}





impl Command {
    pub fn mutates(&self) -> bool {
        !matches!(
            self,
            Command::Faces { .. }
                | Command::Volume { .. }
                | Command::Photoreal { .. }
                | Command::SaveImage { .. }
                | Command::Sun { .. }
                | Command::Library { .. }
                | Command::Thumbnail { .. }
                | Command::SaveToLibrary { .. }
                | Command::LibraryAdd { .. }
                | Command::Reveal { .. }
                | Command::Skins
                | Command::SaveSkin { .. }
                | Command::Report { .. }
                | Command::Options { .. }
                | Command::CloudData { .. }
                | Command::Connected { .. }
                | Command::SameAs { .. }
                | Command::Section { .. }
                | Command::Info
                | Command::Parts { .. }
                | Command::Plugins
                | Command::PluginEnable { .. }
                | Command::InstallPlugin { .. }
                | Command::RemovePlugin { .. }
                | Command::SaveText { .. }
                | Command::Components
                | Command::Pick { .. }
                | Command::BeginBatch
                | Command::EndBatch { .. }
                | Command::Undo
                | Command::Redo
                | Command::Render { .. }
                | Command::Save { .. }
                | Command::ExportObj { .. }
                | Command::ExportGlb { .. }
                | Command::ExportStl { .. }
                | Command::ExportDae { .. }
                | Command::ExportIfc { .. }
                | Command::ExportSheets { .. }
                | Command::SheetsInfo { .. }
                | Command::SheetTrim { preview: true, .. }
                | Command::SheetExtend { preview: true, .. }
                | Command::ViewportPoints { .. }
                | Command::SheetRender { .. }
                | Command::SheetIr { .. }
                | Command::DraftingStatus
                | Command::HatchPatterns { .. }
                | Command::Blueprint { .. }
                | Command::Levels
                | Command::Animate { .. }
                | Command::TimelineFrames { .. }
                | Command::VideoFrame { .. }
                | Command::VideoFinish { .. }
                | Command::ExportDwg { .. }
                | Command::ExportDxf { .. }
                | Command::ExportPdf { .. }
                | Command::Assets
        )
    }
}

#[derive(Default)]
pub struct Engine {
    pub model: Model,
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// Script names: `"as":"front"` on a command stores its result id,
    /// and any later `"@front"` value is replaced by it.
    pub names: HashMap<String, Id>,
    /// Component definition being edited (`edit`), if any.
    pub ctx: Option<Id>,
    /// bumps on every change to the model (progressive renders start over)
    revision: u64,
    /// a photoreal render in progress
    pt: Option<crate::pt::Progress>,
    /// memory undo history may use (estimated bytes); 0: the default
    pub undo_budget: usize,
    /// an open batch: the model before it began, the revision then, and when
    batch: Option<(Model, u64, std::time::Instant)>,
    /// whose the open batch is (see `BatchKind`)
    batch_kind: BatchKind,
    /// when a command last joined the open batch (an app-made batch that goes quiet is closed, see `BATCH_IDLE_APP`)
    batch_last: Option<std::time::Instant>,
    /// counts batches, so an `end_batch` can say which one it means
    batch_seq: u64,
    /// the model was opened with more geometry than the limit allows: it can be looked at and
    /// cut down, but nothing that adds geometry runs and it can't be saved until it is under
    pub oversize: bool,
    /// the model came from a file saved by a newer SketchCraft (see `format`): it can be looked at, but nothing changes it
    /// and nothing is saved over its file, so an older program can never damage it. New, Open and Restore end it.
    pub read_only: Option<crate::format::ReadOnly>,
    /// the last Undo (true) or Redo (false) that was refused because of another program's change, and left
    /// its step in the list: asking again skips that step
    refused: Option<bool>,
    /// the command being run comes from an agent or a plugin (tighter limits); false: the person, whose importers
    /// and terrain may use the whole model budget
    untrusted: bool,
    /// set while `environment` runs its own fixed list of commands: they run as the person (the policy check passed for the
    /// whole), but keep the limits of whoever asked for it
    nested_untrusted: bool,
    /// who made each undo step (`you`, `plugin “X”`, `an agent`), oldest first; kept the same length as `undo`
    step_origins: Vec<String>,
    /// the same for the redo list
    redo_origins: Vec<String>,
    /// who the open batch's changes are from (the last plugin to send a command into it)
    batch_origin: Option<String>,
    /// what the last batch that a plugin made changed: who, and how many things (read by `end_batch`'s reply)
    last_batch: Option<(String, usize)>,
    /// drawings of sheet viewports and hatches kept between page views and exports (never saved; see `draft::cache`)
    pub drawings: crate::draft::cache::Drawings,
}

/// Whose an open batch is. A batch groups ONE client's commands into one undo step; another client's commands never join it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BatchKind {
    /// no batch is open
    #[default]
    None,
    /// the app opened it and nothing has joined yet: the first command decides whose it is (the app's own, or a plugin's run)
    Fresh,
    /// the app's own commands (a reorder, a multi-part edit); a plugin or agent command is refused until it ends
    App,
    /// a plugin run the app opened; the person's own edit closes it (saving what it did first)
    Plugin,
    /// an agent's (an AI answer's commands); the person's own edit closes it the same way
    Agent,
}

/// A batch the app opened for its own commands that has gone quiet this long is closed (the page that opened it went
/// away). A plugin run or an agent may think for longer, so those only have `BATCH_MAX`.
pub const BATCH_IDLE_APP: std::time::Duration = std::time::Duration::from_secs(30);

/// An agent's batch that has gone this long without a command from the agent is closed (what it did so far stays one
/// undo step), so plugins are not kept waiting on an agent that has stopped.
pub const BATCH_IDLE_AGENT: std::time::Duration = std::time::Duration::from_secs(60);

/// A batch nobody closed (the app went away mid-run) is closed after this (a plugin run is
/// limited to 60 s, a Ruby replay to the same).
pub const BATCH_MAX: std::time::Duration = std::time::Duration::from_secs(150);

/// The biggest model file `open` and Restore read (a model in memory is several times its file's size).
/// A model of the biggest size the program writes (`model::MAX_MODEL_BYTES`) is under this, with room: what the app
/// saved it can open again.
pub const MAX_OPEN_BYTES: u64 = 768 << 20;
/// What an agent's `open` reads: its own model cap (`model::MAX_MODEL_BYTES_UNTRUSTED`, 160 MB) plus the room a file has over
/// its estimate. A library part or a thumbnail is held to `library::MAX_LIBRARY_BYTES`.
pub const MAX_OPEN_BYTES_UNTRUSTED: u64 = 256 << 20;

const UNDO_LIMIT: usize = 200;
/// Undo history never holds more than about this much (per-step sizes are estimates).
const UNDO_BYTES: usize = 768 << 20;

fn v3(a: [f64; 3]) -> DVec3 {
    DVec3::from(a)
}




impl Engine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Run one JSON command line as the person using the app (no limits on
    /// which command, beyond the sanity checks) and return a JSON reply.
    pub fn exec_json(&mut self, line: &str) -> Value {
        self.exec_json_as(&Caller::Ui, line)
    }

    /// Run one JSON command line for `caller` (see `policy`). Never panics:
    /// bad input is an error reply, and a bug inside a command is an error
    /// reply with the model put back as it was.
    pub fn exec_json_as(&mut self, caller: &Caller, line: &str) -> Value {
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => return json!({"ok": false, "error": format!("bad JSON: {e}")}),
        };
        self.exec_value_as(caller, v)
    }

    pub fn exec_value_as(&mut self, caller: &Caller, v: Value) -> Value {
        self.exec_value_as_from(caller, v, None)
    }

    /// Like `exec_value_as`; `from` names the plugin that sent the command (set by the app, never by the
    /// plugin), so the undo step it makes says who made it.
    pub fn exec_value_as_from(&mut self, caller: &Caller, v: Value, from: Option<&str>) -> Value {
        // a panic anywhere (not only inside a command) is an error reply
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.exec_value_tracked(caller, v, from))).unwrap_or_else(|_| json!({"ok": false, "error": "SketchCraft hit a bug running that."}))
    }

    /// Who an undo step is from, for the history: the person, a named plugin, or an agent.
    pub fn origin_label(caller: &Caller, from: Option<&str>) -> String {
        match caller {
            Caller::Ui => "you".into(),
            Caller::Mcp(_) => "an agent".into(),
            Caller::Plugin => match from.map(|f| f.chars().filter(|c| !c.is_control() && *c != '“' && *c != '”').take(40).collect::<String>()).filter(|f| !f.trim().is_empty()) {
                Some(n) => format!("plugin “{}”", n.trim()),
                None => "a plugin".into(),
            },
        }
    }

    /// Who made the step Undo would take back, and the one Redo would do again.
    pub fn undo_origin(&self) -> Option<&str> {
        if self.undo.is_empty() { None } else { self.step_origins.last().map(String::as_str) }
    }
    pub fn redo_origin(&self) -> Option<&str> {
        if self.redo.is_empty() { None } else { self.redo_origins.last().map(String::as_str) }
    }

    /// Keep each step's origin beside the step: the commands that push or take steps say who made them.
    fn exec_value_tracked(&mut self, caller: &Caller, v: Value, from: Option<&str>) -> Value {
        let label = Self::origin_label(caller, from);
        let cmd = v.get("cmd").and_then(Value::as_str).unwrap_or("").to_string();
        let (u0, r0) = (self.undo.len(), self.redo.len());
        // a plugin's commands inside a batch make the batch the plugin's
        if self.batch.is_some() && !matches!(caller, Caller::Ui) {
            self.batch_origin = Some(label.clone());
        }
        let mut r = self.exec_value_inner(caller, v);
        let (ul, rl) = (self.undo.len(), self.redo.len());
        match cmd.as_str() {
            "undo" | "redo" if r["ok"] == true => {
                let back = cmd == "undo";
                let (from_list, from_n, to_list, grew) = if back { (&mut self.step_origins, u0.saturating_sub(ul), &mut self.redo_origins, rl > r0) } else { (&mut self.redo_origins, r0.saturating_sub(rl), &mut self.step_origins, ul > u0) };
                let mut last = None;
                for _ in 0..from_n {
                    last = from_list.pop();
                }
                let who = last.unwrap_or_else(|| "an earlier change".into());
                if grew {
                    to_list.push(who.clone());
                }
                if let Some(o) = r.as_object_mut() {
                    o.insert("origin".into(), json!(who));
                }
            }
            _ => {
                while self.step_origins.len() < ul {
                    self.step_origins.push(label.clone());
                }
                self.step_origins.truncate(ul);
            }
        }
        self.redo_origins.truncate(rl);
        while self.redo_origins.len() < rl {
            self.redo_origins.insert(0, "an earlier change".into());
        }
        r
    }

    fn exec_value_inner(&mut self, caller: &Caller, mut v: Value) -> Value {
        if self.batch_expired() {
            self.end_batch();
        }
        if let Err(e) = self.batch_gate(caller, &v) {
            return json!({"ok": false, "error": e});
        }
        let name = v.as_object_mut().and_then(|o| o.remove("as")).and_then(|n| n.as_str().map(String::from));
        if let Err(e) = crate::policy::vet(caller, &mut v) {
            return json!({"ok": false, "error": e});
        }
        self.untrusted = !matches!(caller, Caller::Ui) || self.nested_untrusted;
        if let Err(e) = self.resolve(&mut v) {
            crate::policy::settle(caller, false);
            return json!({"ok": false, "error": e.to_string()});
        }
        let r = match serde_json::from_value::<Command>(v) {
            Ok(cmd) => self.exec(cmd),
            Err(e) => {
                crate::policy::settle(caller, false);
                return json!({"ok": false, "error": format!("bad command: {e}")});
            }
        };
        crate::policy::settle(caller, r["ok"] == true);
        if let (Some(n), true) = (name, r["ok"] == true) {
            if let Some(id) = ["face", "group", "instance", "guide", "dimension", "text", "sheet", "viewport"]
                .iter()
                .find_map(|k| r[*k].as_u64())
                .or_else(|| r["ids"][0].as_u64())
                .or_else(|| r["edges"][0].as_u64())
            {
                self.names.insert(n, id as Id);
            }
        }
        r
    }

    /// Is the open batch past its limit: too long since it began, or (an app-made one) too long since it was last used?
    fn batch_expired(&self) -> bool {
        let Some(b) = &self.batch else { return false };
        b.2.elapsed() > BATCH_MAX || self.batch_idle_limit().is_some_and(|idle| self.batch_last.is_some_and(|t| t.elapsed() > idle))
    }

    /// How long the open batch may go without a command from its owner before it is closed (a plugin run has only
    /// `BATCH_MAX`).
    fn batch_idle_limit(&self) -> Option<std::time::Duration> {
        match self.batch_kind {
            BatchKind::Fresh | BatchKind::App => Some(BATCH_IDLE_APP),
            BatchKind::Agent => Some(BATCH_IDLE_AGENT),
            _ => None,
        }
    }

    /// The longest the open batch can still stay open.
    fn batch_left(&self) -> std::time::Duration {
        let Some(b) = &self.batch else { return std::time::Duration::ZERO };
        let total = BATCH_MAX.saturating_sub(b.2.elapsed());
        match (self.batch_idle_limit(), self.batch_last) {
            (Some(idle), Some(t)) => total.min(idle.saturating_sub(t.elapsed())),
            _ => total,
        }
    }

    /// Whose batch is open, if one is.
    pub fn batch_kind(&self) -> BatchKind {
        if self.batch.is_some() { self.batch_kind } else { BatchKind::None }
    }

    /// Would the person's own edit close the open batch (a plugin's or an agent's)? An app-made batch is the person's
    /// own, so their edits join it.
    pub fn person_edit_ends_batch(&self) -> bool {
        matches!(self.batch_kind(), BatchKind::Plugin | BatchKind::Agent)
    }

    /// Batches belong to one client: decide whether this command may run while a batch is open, and note who it joins.
    /// `begin_batch` and `end_batch` are checked in `session.rs`.
    fn batch_gate(&mut self, caller: &Caller, v: &Value) -> Result<(), String> {
        if self.batch.is_none() {
            return Ok(());
        }
        let name = v.get("cmd").and_then(Value::as_str).unwrap_or("");
        if matches!(name, "begin_batch" | "end_batch" | "undo" | "redo") || !serde_json::from_value::<Command>(v.clone()).is_ok_and(|c| c.mutates()) {
            // an agent that is still reading is still at work on its batch
            if self.batch_kind == BatchKind::Agent && matches!(caller, Caller::Mcp(_)) {
                self.batch_last = Some(std::time::Instant::now());
            }
            return Ok(());
        }
        let mine = match caller {
            Caller::Ui => BatchKind::App,
            Caller::Plugin => BatchKind::Plugin,
            Caller::Mcp(_) => BatchKind::Agent,
        };
        match (self.batch_kind, mine) {
            // the first command decides whose a fresh batch is
            (BatchKind::Fresh, _) => self.batch_kind = mine,
            (k, m) if k == m => {}
            // the person's edit into a plugin's or an agent's batch: the host closes that batch first (see `Live`)
            (BatchKind::Plugin | BatchKind::Agent, BatchKind::App) => {}
            (BatchKind::App, _) => return Err("The app is in the middle of its own change; try again in a moment.".into()),
            _ => return Err(format!("Another client's batch is open (a plugin run or an agent's changes); try again when it has finished (at the latest in {} s).", self.batch_left().as_secs().max(1))),
        }
        self.batch_last = Some(std::time::Instant::now());
        Ok(())
    }

    /// Is a batch (see `BeginBatch`) open?
    pub fn in_batch(&self) -> bool {
        self.batch.is_some()
    }

    #[doc(hidden)]
    pub fn expire_batch_for_test(&mut self) {
        if let Some(b) = self.batch.as_mut() {
            b.2 = std::time::Instant::now() - BATCH_MAX - std::time::Duration::from_secs(1);
        }
    }

    /// As if the open batch's owner sent its last command `secs` seconds ago.
    #[doc(hidden)]
    pub fn quiet_batch_for_test(&mut self, secs: u64) {
        self.batch_last = Some(std::time::Instant::now() - std::time::Duration::from_secs(secs));
    }

    #[doc(hidden)]
    pub fn idle_batch_for_test(&mut self) {
        self.batch_last = Some(std::time::Instant::now() - BATCH_IDLE_APP - std::time::Duration::from_secs(1));
    }

    /// Close a batch nobody closed in time (the page went away mid-run). True if one was closed.
    pub fn close_expired_batch(&mut self) -> bool {
        if self.batch_expired() {
            self.end_batch();
            return true;
        }
        false
    }

    /// Close a batch: everything since it began is ONE undo step.
    fn end_batch(&mut self) {
        let who = self.batch_origin.take();
        if let Some((before, rev, _)) = self.batch.take() {
            if self.revision != rev {
                // a plugin's run: say how much it changed (the app tells the person)
                self.last_batch = who.as_ref().map(|w| (w.clone(), crate::history::patch_between(&before, &self.model).things()));
                self.undo.push(Step::Snap(Box::new(before)));
                self.step_origins.push(who.unwrap_or_else(|| "you".into()));
                self.redo.clear();
                self.trim_undo();
            }
        }
    }

    /// Close an open batch now (the person went on working): what it did becomes one undo step.
    pub fn close_batch(&mut self) {
        self.end_batch();
    }

    /// Take back, without a trace in the history, whatever steps were added after the history had `depth` of them
    /// (their change is being thrown away, for instance because another program saved first): the model goes back
    /// to what it was before them.
    pub fn drop_steps_after(&mut self, depth: usize) {
        let mut before = None;
        while self.undo.len() > depth {
            if let Some(Step::Snap(m)) = self.undo.pop() {
                before = Some(*m);
            }
        }
        if let Some(m) = before {
            let next = self.model.next_id;
            self.model = m;
            self.model.next_id = self.model.next_id.max(next);
            self.drawings.clear();
        }
    }

    /// The undo and redo lists in a form that survives the reload of a model somebody else saved
    /// (see `history`: nothing is worked out yet, so this is instant). The lists are taken out of the
    /// engine: give the result to `install_history` once the reload is over, whether it worked or not.
    pub fn history_for_reload(&mut self) -> Option<crate::history::History> {
        if self.undo.is_empty() && self.redo.is_empty() {
            return None;
        }
        let (undo, redo) = (std::mem::take(&mut self.undo), std::mem::take(&mut self.redo));
        Some(crate::history::carry(undo, redo, &self.model))
    }

    /// Put a carried history back after a reload. Returns how many steps could not be kept.
    pub fn install_history(&mut self, h: crate::history::History) -> (usize, usize) {
        self.undo = h.undo;
        self.redo = h.redo;
        // what each step was made by isn't carried across another program's save
        self.step_origins = vec!["an earlier change".to_string(); self.undo.len()];
        self.redo_origins = vec!["an earlier change".to_string(); self.redo.len()];
        self.refused = None;
        self.trim_undo();
        (h.dropped, h.dropped_redo)
    }

    /// A step that is not a whole-model copy is about to move through the model: the copy next to it on the other
    /// list (the redo step beyond an Undo, the undo step beyond a Redo) is paired with the model as it is right now, so
    /// that it survives another program's save as a set of changes instead of being let go (the copy alone says
    /// nothing about what it leads from once a patch sits between).
    fn pair_neighbour(&mut self, back: bool) {
        let other = if back { &mut self.redo } else { &mut self.undo };
        if let Some(Step::Snap(_)) = other.last() {
            if let Some(Step::Snap(m)) = other.pop() {
                let now = std::sync::Arc::new(self.model.clone());
                // Undo: the redo copy is the model one step ahead of this one; Redo: the undo copy is one step behind
                other.push(if back { Step::Stale { before: now, after: std::sync::Arc::new(*m) } } else { Step::Stale { before: std::sync::Arc::new(*m), after: now } });
            }
        }
    }

    /// Replace the whole model with `model` as one undoable step (a restore from a backup).
    pub fn replace_model(&mut self, model: Model) -> Result<()> {
        if self.batch.is_some() {
            bail!("a plugin is still running; try again when it has finished");
        }
        // a file that names points, placements or groups that aren't there would draw nothing but trouble
        if let Some(what) = model.broken_refs(false).first() {
            bail!("that file isn't a consistent model ({what} refers to something that isn't there), so it was not restored");
        }
        if let Err(why) = model.check_nesting(crate::nesting::MAX_EXPANDED) {
            bail!("that file can't be restored: {why}");
        }
        let mut model = model;
        model.repair_ids();
        model.vet_pictures();
        model.vet_sheets();
        // a model over the size limits is restored for viewing and cutting down, like Open: the limits never block getting work back
        model.check_sane_with(false)?;
        let over = model.over_limits();
        // ids are never handed out twice: not after a restore either (an agent or another program may still hold the ones in use now)
        model.next_id = model.next_id.max(self.model.next_id);
        let before = std::mem::replace(&mut self.model, model);
        self.undo.push(Step::Snap(Box::new(before)));
        self.redo.clear();
        self.ctx = None;
        self.names.clear();
        self.read_only = None;
        self.oversize = over;
        self.revision += 1;
        self.trim_undo();
        Ok(())
    }

    /// How many steps Undo can go back.
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// Bumps on every change to the model (and undo/redo); a client can tell
    /// whether a command changed anything.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Build (or replace, or remove) the environment around the model: one
    /// undo step, in a locked group tagged "Environment".
    pub fn environment(&mut self, kind: &str) -> Result<Value> {
        let (was, outer) = (self.nested_untrusted, self.untrusted);
        self.nested_untrusted = outer;
        let r = self.environment_inner(kind);
        self.nested_untrusted = was;
        // the nested commands set the flag as they ran: it goes back to what the caller's own command had
        self.untrusted = outer;
        r
    }

    fn environment_inner(&mut self, kind: &str) -> Result<Value> {
        if self.ctx.is_some() {
            bail!("close the component you're editing first");
        }
        let before = self.model.clone();
        let depth = self.undo.len();
        let fail = |e: &mut Engine, msg: String| -> Result<Value> {
            e.model = before.clone();
            e.undo.truncate(depth);
            bail!(msg)
        };
        // the old one goes
        let old: Vec<Id> = self.model.groups.iter().filter(|(_, g)| g.name.starts_with("Environment") && g.parent.is_none()).map(|(&k, _)| k).collect();
        for g in &old {
            self.model.groups.get_mut(g).unwrap().locked = false;
        }
        if !old.is_empty() {
            self.model.delete(&old)?;
        }
        self.model.environment = None;
        self.model.active_group = None;
        if kind == "none" {
            self.undo.push(Step::Snap(Box::new(before)));
            return Ok(json!({"environment": null}));
        }
        // around what's there (or a 10 m square at the origin)
        let (lo, hi) = self.model.world_bounds().unwrap_or((DVec3::new(-5.0, -5.0, 0.0), DVec3::new(5.0, 5.0, 0.0)));
        let cmds = crate::environment::build(kind, lo, hi)?;
        let title = crate::environment::KINDS.iter().find(|k| k.0 == kind).map(|k| k.1.split(':').next().unwrap_or(kind)).unwrap_or(kind);
        let outer = self.model.begin_group(format!("Environment: {title}"));
        let saved_names = std::mem::take(&mut self.names);
        for c in cmds {
            let r = self.exec_json(&c.to_string());
            if r["ok"] != true {
                self.names = saved_names;
                return fail(self, format!("building the {kind} environment: {}", r["error"]));
            }
            // pieces made by end_group step back out to the environment group
            if self.model.active_group.is_none() {
                self.model.active_group = Some(outer);
            }
        }
        self.names = saved_names;
        self.model.active_group = None;
        // placed assets sit inside it too
        let loose: Vec<Id> = self.model.instances.iter().filter(|(k, i)| i.parent.is_none() && !before.instances.contains_key(k)).map(|(&k, _)| k).collect();
        self.model.set_parent(&loose, Some(outer))?;
        self.model.set_tag(&[outer], Some("Environment".into()))?;
        self.model.tags.entry("Environment".into()).or_insert(true);
        self.model.groups.get_mut(&outer).unwrap().locked = true;
        self.model.environment = Some(kind.to_string());
        // all of it is one undo step
        self.undo.truncate(depth);
        self.undo.push(Step::Snap(Box::new(before)));
        self.redo.clear();
        Ok(json!({"environment": kind, "group": outer}))
    }

    fn resolve(&self, v: &mut Value) -> Result<()> {
        match v {
            Value::String(s) if s.starts_with('@') => {
                let Some(id) = self.names.get(&s[1..]) else { bail!("unknown name {s}") };
                *v = json!(id);
            }
            Value::Array(a) => a.iter_mut().try_for_each(|x| self.resolve(x))?,
            Value::Object(o) => o.values_mut().try_for_each(|x| self.resolve(x))?,
            _ => {}
        }
        Ok(())
    }

    /// Run a command. A command that fails, panics, or leaves a non-finite
    /// or absurd number in the model changes nothing (and costs no undo step).
    pub fn exec(&mut self, cmd: Command) -> Value {
        let is_env = matches!(cmd, Command::Environment { .. });
        // a model from a newer SketchCraft is only looked at: whatever would change it, or write it out as a model file, is refused
        if let Some(ro) = &self.read_only {
            if (cmd.mutates() || is_env || matches!(cmd, Command::Undo | Command::Redo | Command::BeginBatch | Command::Save { .. })) && !matches!(cmd, Command::New { .. } | Command::Open { .. }) {
                return json!({"ok": false, "error": ro.refusal(), "read_only": true});
            }
        }
        let was_read_only = self.read_only.clone();
        let is_new = matches!(cmd, Command::New { .. });
        // opening a file replaces the model and the history with it: Undo must never
        // step back to a model that isn't the one on disk
        let is_open = matches!(cmd, Command::Open { .. });
        let mutates = cmd.mutates() || is_env;
        if mutates {
            self.refused = None;
        }
        if mutates || matches!(cmd, Command::Undo | Command::Redo) {
            self.revision += 1;
        }
        // everything needed to put things back as they were
        let saved = mutates.then(|| (self.model.clone(), self.ctx, self.names.clone(), self.undo.len()));
        // Undo and Redo replace the model too: what kept drawings it reaches is worked out from the model before
        let pre_step = (matches!(cmd, Command::Undo | Command::Redo) && !self.drawings.is_empty()).then(|| self.model.clone());
        let count_before = self.model.entity_count();
        let bytes_before = if mutates { self.model.size_estimate() } else { 0 };
        let text_before = if mutates { self.model.text_bytes() } else { 0 };
        let oversize_before = self.oversize;
        // how much one command may add: the person's importers and terrain may use up to the model's total; everything
        // else (repeat commands such as `array`, and anything an agent or a plugin runs) is held to MAX_GROWTH
        let grow_cap = if self.untrusted {
            crate::model::MAX_GROWTH
        } else if matches!(cmd, Command::ImportObj { .. } | Command::ImportSkp { .. } | Command::ImportDxf { .. } | Command::ImportDwg { .. } | Command::ImportFbx { .. } | Command::ImportIfc { .. } | Command::ImportCloud { .. } | Command::TerrainGrid { .. } | Command::TerrainFromContours { .. } | Command::Open { .. }) {
            crate::model::MAX_ENTITIES
        } else {
            crate::model::MAX_GROWTH
        };
        crate::model::set_growth_limit(grow_cap);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Command::Environment { kind } = &cmd {
                // pushes its own (single) undo step
                return self.environment(kind).map(|mut v| {
                    v["ok"] = json!(true);
                    (v, false)
                });
            }
            crate::safety::take_skipped();
            self.run(cmd).map(|mut v| {
                if is_new {
                    self.read_only = None;
                }
                // dimensions on sheets follow what the command did, in the same undo step (see `anchors`)
                if mutates && !is_new && !is_open {
                    self.settle_anchors();
                }
                v["ok"] = json!(true);
                // pictures left out of a render, an import or a projection are said, never dropped silently
                let skipped = crate::safety::take_skipped();
                if !skipped.is_empty() && v.is_object() {
                    v["skipped_pictures"] = json!(skipped);
                }
                (v, mutates)
            })
        }));
        let restore = |e: &mut Engine, saved: Option<(Model, Option<Id>, HashMap<String, Id>, usize)>| {
            if let Some((model, ctx, names, depth)) = saved {
                e.read_only = was_read_only.clone();
                e.model = model;
                e.ctx = ctx;
                e.names = names;
                e.undo.truncate(depth);
            }
        };
        match outcome {
            Ok(Ok((v, push_undo))) => {
                if mutates {
                    // a model over the geometry limit (opened that way) may only get smaller; one command may not add more than MAX_GROWTH
                    let count = self.model.entity_count();
                    let bytes = self.model.size_estimate();
                    let text = self.model.text_bytes();
                    let within = count <= crate::model::MAX_ENTITIES && bytes <= crate::model::MAX_MODEL_BYTES && text <= crate::model::MAX_TEXT_BYTES && !self.model.attrs_over();
                    // components must not contain themselves, nest too deep or multiply past the limit (a model that did
                    // would take the program down when drawn); a model already over the limit may only get smaller
                    let nest_err = self.nesting_error(is_open, saved.as_ref().map(|(m, ..)| m));
                    let checked = if let Some(e) = nest_err {
                        Err(e)
                    } else if is_open && self.oversize {
                        self.model.check_sane_with(false)
                    } else if self.oversize && within {
                        self.oversize = false;
                        self.model.check_sane_est(bytes)
                    } else if self.oversize {
                        self.model.check_sane_with(false).and_then(|_| if count > count_before || bytes > bytes_before || text > text_before { Err(anyhow::anyhow!("this model is over the size limits ({} points, edges and faces; about {} MB), so it is open for viewing and for deleting parts only; that would add more", count_before, bytes_before >> 20)) } else { Ok(()) })
                    } else if !is_open && count > count_before.saturating_add(grow_cap) {
                        Err(anyhow::anyhow!("that would add {} points, edges and faces at once (the limit is {}), so it was not done", count - count_before, grow_cap))
                    } else if !is_open && self.untrusted && bytes > bytes_before.saturating_add(crate::model::MAX_GROWTH_BYTES_UNTRUSTED) {
                        Err(anyhow::anyhow!("that would add {} MB of text, images, attributes or scan points at once (the limit for an agent or a plugin is {} MB), so it was not done", (bytes - bytes_before) >> 20, crate::model::MAX_GROWTH_BYTES_UNTRUSTED >> 20))
                    } else if !is_open && self.untrusted && text > crate::model::MAX_TEXT_BYTES_UNTRUSTED && text > text_before {
                        Err(anyhow::anyhow!("the model's names, notes and attributes are about {} MB; an agent or a plugin can't add more past {} MB (the person's own naming has the rest of the room), so it was not done ({})", text >> 20, crate::model::MAX_TEXT_BYTES_UNTRUSTED >> 20, self.model.text_use()))
                    } else if !is_open && self.untrusted && bytes > crate::model::MAX_MODEL_BYTES_UNTRUSTED && bytes > bytes_before {
                        Err(anyhow::anyhow!("the model is about {} MB; an agent or a plugin can't make it bigger past {} MB (the person can), so it was not done", bytes >> 20, crate::model::MAX_MODEL_BYTES_UNTRUSTED >> 20))
                    } else {
                        self.model.check_sane_est(bytes)
                    };
                    if let Err(e) = checked {
                        restore(self, saved);
                        self.oversize = oversize_before;
                        return json!({"ok": false, "error": e.to_string()});
                    }
                    // kept drawings this change reached are dropped (and the pages looked at last redrawn in the background)
                    if let Some((before, ..)) = &saved {
                        self.drawings_changed(before);
                    }
                    if is_open {
                        self.undo.clear();
                    } else if push_undo && self.batch.is_none() && v.get("unchanged") != Some(&json!(true)) {
                        // (a command that says it changed nothing, such as a `dimension_auto` with everything already
                        // there, takes no undo step)
                        if let Some((before, ..)) = saved {
                            self.undo.push(Step::Snap(Box::new(before)));
                        }
                    }
                    self.redo.clear();
                    self.trim_undo();
                }
                if let Some(before) = &pre_step {
                    self.drawings_changed(before);
                }
                v
            }
            Ok(Err(e)) => {
                restore(self, saved);
                json!({"ok": false, "error": e.to_string()})
            }
            Err(panic) => {
                let msg = panic.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| panic.downcast_ref::<String>().cloned()).unwrap_or_else(|| "unknown".into());
                eprintln!("sketchcraft: a command panicked: {msg}");
                restore(self, saved);
                if msg.starts_with(crate::model::IDS_OUT) {
                    return json!({"ok": false, "error": "this model has used up its ids (the counter is at the top of its range), so nothing more can be added; export it (OBJ or SketchUp) and import it into a new model. Nothing was changed."});
                }
                json!({"ok": false, "error": "SketchCraft hit a bug running that. Nothing was changed."})
            }
        }
    }

    /// Why the model as it now stands has components that can't be allowed (see `nesting`), if it does.
    fn nesting_error(&self, is_open: bool, before: Option<&Model>) -> Option<anyhow::Error> {
        if let Some(d) = self.ctx.and_then(|d| self.model.definitions.get(&d)) {
            let had = before.and_then(|b| b.definitions.get(&self.ctx?)).is_some_and(|d| !d.model.definitions.is_empty());
            if !d.model.definitions.is_empty() && !had {
                return Some(anyhow::anyhow!("components can't be made, imported or opened while one is being edited (their ids would collide with the model's own): close the component first. Nothing was changed."));
            }
        }
        if self.model.definitions.is_empty() && self.model.instances.is_empty() {
            return None;
        }
        let limit = if self.untrusted { crate::nesting::MAX_EXPANDED_UNTRUSTED } else { crate::nesting::MAX_EXPANDED };
        match self.model.expanded_count() {
            Err(p) => Some(anyhow::anyhow!("{p}, so it was not done")),
            Ok(n) if n > limit => {
                // not a new problem when the model was already this big (an agent may still delete or edit what is there)
                let was = if is_open { None } else { before.and_then(|b| b.expanded_count().ok()) };
                match was {
                    Some(w) if n <= w => None,
                    _ => Some(anyhow::anyhow!("the components in it would add up to about {n} edges and faces once every placement is counted (the limit is {limit}): a component placed many times inside a component placed many times multiplies. It was not done.")),
                }
            }
            Ok(_) => None,
        }
    }

    /// Keep undo history within `UNDO_LIMIT` steps and `UNDO_BYTES` of memory
    /// (the newest step is always kept).
    fn trim_undo(&mut self) {
        let mut total: usize = self.undo.iter().map(Step::approx_bytes).sum();
        while self.undo.len() > UNDO_LIMIT || (self.undo.len() > 1 && total > if self.undo_budget == 0 { UNDO_BYTES } else { self.undo_budget }) {
            total = total.saturating_sub(self.undo[0].approx_bytes());
            self.undo.remove(0);
            if !self.step_origins.is_empty() {
                self.step_origins.remove(0);
            }
        }
        // what Redo holds counts against the same memory (patches are small; copies are not)
        let limit = if self.undo_budget == 0 { UNDO_BYTES } else { self.undo_budget };
        let mut redo_total: usize = self.redo.iter().map(Step::approx_bytes).sum();
        while self.redo.len() > 1 && redo_total > limit {
            redo_total = redo_total.saturating_sub(self.redo[0].approx_bytes());
            self.redo.remove(0);
            if !self.redo_origins.is_empty() {
                self.redo_origins.remove(0);
            }
        }
    }

    /// Undo (`back`) or Redo one step. A step taken since the last reload is a copy of the model; one taken before
    /// another program saved is worked out now (see `history`) and applied on top of the model as it is, touching only
    /// what that step changed. If another program's change stands in the way the step is refused with a reason and
    /// stays in the list; asking again skips it and goes one step further.
    fn step_history(&mut self, back: bool) -> Result<Value> {
        let verb = if back { "Undo" } else { "Redo" };
        let mut skipped = 0;
        loop {
            let step = if back { self.undo.pop() } else { self.redo.pop() };
            let Some(step) = step else {
                if skipped > 0 {
                    self.refused = None;
                    bail!("{verb}: {skipped} step{} couldn't be {} because another program changed what {} touched, and there is nothing further {}", if skipped == 1 { "" } else { "s" }, if back { "taken back" } else { "done again" }, if skipped == 1 { "it" } else { "they" }, if back { "back" } else { "ahead" });
                }
                bail!("nothing to {}", verb.to_lowercase())
            };
            let step = match step {
                Step::Snap(other) => {
                    // a step taken since the last reload: the model is exactly as that step left it
                    let next = self.model.next_id;
                    let was = std::mem::replace(&mut self.model, *other);
                    // ids are never handed out twice: not after Undo either (another program may have used the ones this step freed)
                    self.model.next_id = self.model.next_id.max(next);
                    let was = Step::Snap(Box::new(was));
                    if back { self.redo.push(was) } else { self.undo.push(was) }
                    self.refused = None;
                    return Ok(json!({}));
                }
                Step::Stale { before, after } => {
                    self.pair_neighbour(back);
                    let p = crate::history::patch_between(&before, &after);
                    if p.is_empty() {
                        // it changed nothing in the model's content (a view, a selection): not a step
                        continue;
                    }
                    p
                }
                Step::Patch(p) => {
                    self.pair_neighbour(back);
                    p
                }
            };
            match step.apply(&self.model, !back) {
                Ok(m) => {
                    let next = self.model.next_id;
                    self.model = m;
                    self.model.next_id = self.model.next_id.max(next);
                    if back { self.redo.push(Step::Patch(step)) } else { self.undo.push(Step::Patch(step)) }
                    self.refused = None;
                    return Ok(if skipped > 0 { json!({"skipped": skipped}) } else { json!({}) });
                }
                Err(why) => {
                    if self.refused == Some(back) {
                        // asked again after a refusal: this step is let go and the next one is tried
                        self.refused = None;
                        skipped += 1;
                        continue;
                    }
                    // kept: the other program may take its change back, and then this works
                    if back { self.undo.push(Step::Patch(step)) } else { self.redo.push(Step::Patch(step)) }
                    self.refused = Some(back);
                    bail!("{verb} stopped: this step can't be {} right now because {why} (another program changed it). The step is kept: try again after that program's change is taken back, or press {verb} again to skip this step and go one {}.", if back { "taken back" } else { "done again" }, if back { "further back" } else { "further ahead" });
                }
            }
        }
    }

    fn run(&mut self, cmd: Command) -> Result<Value> {
        // inside `edit`, geometry commands work on the component's definition
        if let Some(d) = self.ctx {
            if !self.model.definitions.contains_key(&d) {
                self.ctx = None;
            }
        }
        // each area's arms live in commands/<area>.rs
        match cmd {
            c @ (
                Command::New { .. } | Command::Rect { .. } | Command::Polygon { .. } | Command::Line { .. } |
                Command::Circle { .. } | Command::Arc { .. } | Command::Offset { .. } | Command::FollowMe { .. } |
                Command::Pushpull { .. } | Command::Move { .. } | Command::Rotate { .. } | Command::Scale { .. } |
                Command::Duplicate { .. } | Command::Array { .. } | Command::Delete { .. } | Command::Reverse { .. }
            ) => self.run_draw(c),
            c @ (
                Command::Weld { .. } | Command::ExplodeCurve { .. } | Command::OrientFaces { .. } |
                Command::Text3d { .. } | Command::Subd { .. } | Command::CageEdit { .. } |
                Command::Subdivide { .. } | Command::FlipEdge { .. } | Command::Bevel { .. } |
                Command::Divide { .. } | Command::Slice { .. } | Command::Intersect { .. } | Command::Flip { .. } |
                Command::Solid { .. } | Command::Volume { .. }
            ) => self.run_shape(c),
            c @ (
                Command::RaiseWalls { .. } | Command::DoorSwing { .. } | Command::TerrainFromContours { .. } |
                Command::TerrainGrid { .. } | Command::Smoove { .. } | Command::Stamp { .. } |
                Command::Drape { .. } | Command::Wall { .. } | Command::Stairs { .. } | Command::Roof { .. } |
                Command::Column { .. } | Command::Insert { .. } | Command::Assets { .. }
            ) => self.run_build(c),
            c @ (
                Command::BeginGroup { .. } | Command::EndGroup { .. } | Command::Group { .. } |
                Command::Faces { .. } | Command::Pick { .. } | Command::Copy { .. } | Command::MakeComponent { .. } |
                Command::Place { .. } | Command::ComponentFaceMe { .. } | Command::ComponentCuts { .. } |
                Command::Edit { .. } | Command::Close { .. } | Command::Explode { .. } | Command::Components { .. } |
                Command::Ungroup { .. } | Command::MakeUnique { .. } | Command::Connected { .. } |
                Command::SameAs { .. } | Command::Resize { .. } | Command::SetAxes { .. }
            ) => self.run_groups(c),
            c @ (
                Command::Hide { .. } | Command::Unhide { .. } | Command::Tag { .. } | Command::TagVisible { .. } |
                Command::TagColor { .. } | Command::RenameTag { .. } | Command::SetParent { .. } |
                Command::TagStyle { .. } | Command::EdgeStyle { .. } | Command::HatchPattern { .. } | Command::HatchPatterns { .. } |
                Command::Lock { .. } | Command::Soften { .. } | Command::Rename { .. } | Command::Opacity { .. } |
                Command::SetAttrs { .. } | Command::Classify { .. } | Command::Reveal { .. }
            ) => self.run_tags(c),
            c @ (
                Command::Paint { .. } | Command::Texture { .. } | Command::Purge { .. } |
                Command::DeleteMaterial { .. } | Command::PurgeMaterials { .. } | Command::Skins { .. } |
                Command::SaveSkin { .. } | Command::Material { .. }
            ) => self.run_paint(c),
            c @ (
                Command::Photoreal { .. } | Command::Render { .. } | Command::Environment { .. } |
                Command::Sun { .. } | Command::AddLocation { .. } | Command::Thumbnail { .. } |
                Command::SaveImage { .. }
            ) => self.run_rendering(c),
            c @ (
                Command::Section { .. } | Command::SectionPlane { .. } | Command::SectionEdit { .. } |
                Command::SectionOff { .. }
            ) => self.run_sections(c),
            c @ (
                Command::Guide { .. } | Command::Dimension { .. } | Command::Text { .. } | Command::Units { .. } |
                Command::ClearGuides { .. } | Command::ShowView { .. } | Command::SaveView { .. } |
                Command::MatchPhoto { .. } | Command::DeleteView { .. } | Command::Parts { .. } |
                Command::Info { .. } | Command::ProjectPhoto { .. } | Command::PhotoSolve { .. } |
                Command::Finish { .. } | Command::Options { .. } | Command::Report { .. }
            ) => self.run_annotate(c),
            c @ (
                Command::Save { .. } | Command::Open { .. } | Command::ExportGlb { .. } | Command::ExportStl { .. } |
                Command::ImportDxf { .. } | Command::ImportDwg { .. } | Command::ExportDwg { .. } |
                Command::ExportIfc { .. } | Command::ExportDae { .. } | Command::ExportDxf { .. } |
                Command::ExportPdf { .. } | Command::ImportObj { .. } | Command::ImportSkp { .. } |
                Command::ImportCloud { .. } | Command::CloudData { .. } | Command::Cloud { .. } |
                Command::ImportIfc { .. } | Command::ImportFbx { .. } | Command::ExportObj { .. }
            ) => self.run_files(c),
            c @ (
                Command::Plugins { .. } | Command::PluginEnable { .. } | Command::InstallPlugin { .. } |
                Command::RemovePlugin { .. } | Command::SaveText { .. } | Command::Library { .. } |
                Command::InsertFile { .. } | Command::SaveToLibrary { .. } | Command::LibraryAdd { .. }
            ) => self.run_library(c),
            c @ (
                Command::Animate { .. } | Command::Blueprint { .. } | Command::Levels { .. } |
                Command::Timeline { .. } | Command::TimelineFrames { .. } | Command::VideoFrame { .. } |
                Command::VideoFinish { .. }
            ) => self.run_media(c),
            c @ (
                Command::Sheets { .. } | Command::ExportSheets { .. } | Command::SheetAdd { .. } | Command::SheetEdit { .. } |
                Command::SheetDelete { .. } | Command::ViewportAdd { .. } | Command::ViewportEdit { .. } |
                Command::SheetItemDelete { .. } | Command::SheetsInfo { .. } | Command::SheetDim { .. } | Command::SheetReattach { .. } |
                Command::SheetLine { .. } | Command::SheetText { .. } | Command::SheetTable { .. } | Command::SheetTablePaste { .. } | Command::SheetTrim { .. } | Command::SheetExtend { .. } | Command::SheetOffset { .. } |
                Command::SheetDuplicate { .. } | Command::SheetOrder { .. }
            ) => self.run_sheets(c),
            c @ (Command::ViewportPoints { .. } | Command::DimensionAuto { .. } | Command::LayoutPreset { .. } | Command::SheetRender { .. } | Command::SheetIr { .. }) => self.run_autodraft(c),
            c @ (
                Command::BeginBatch { .. } | Command::EndBatch { .. } | Command::Undo { .. } | Command::Redo { .. }
            ) => self.run_session(c),
            Command::DraftingStatus => Ok(self.drafting_status()),
        }
    }
}

/// After placing: a component that cuts openings, put on a face, cuts one.
/// What happened, for the reply (null when nothing was asked).
fn place_cut(m: &mut Model, instance: Id, host: Option<Id>) -> Value {
    let Some(host) = host else { return Value::Null };
    let Some(i) = m.instances.get(&instance) else { return Value::Null };
    let (def_id, t) = (i.def, i.transform);
    let Some(d) = m.definitions.get(&def_id).filter(|d| d.cuts) else { return Value::Null };
    let def = d.model.clone();
    match crate::openings::cut(m, &def, t, host) {
        Ok(n) => json!({"faces": n}),
        Err(e) => json!({"error": e.to_string()}),
    }
}

fn round(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// A readable name from a download's file name: "uploads_files_2787791_Mercedes+Benz+GLS+580"
/// -> "Mercedes Benz GLS 580".
fn tidy_name(stem: &str) -> String {
    let s = stem.replace(['+', '_'], " ");
    let words: Vec<&str> = s.split_whitespace().collect();
    // drop store prefixes like "uploads files 2787791"
    let start = words.iter().position(|w| !matches!(w.to_ascii_lowercase().as_str(), "uploads" | "files" | "upload" | "file") && !w.chars().all(|c| c.is_ascii_digit())).unwrap_or(0);
    let name = words[start..].join(" ");
    if name.is_empty() { stem.to_string() } else { name }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CageOp {
    InsertLoop { edge: [usize; 2], #[serde(default = "half")] t: f64 },
    Extrude { faces: Vec<usize>, distance: f64 },
    DeleteFaces { faces: Vec<usize> },
    Merge { points: Vec<usize> },
}

fn half() -> f64 {
    0.5
}

#[derive(Debug, Clone, Deserialize)]
pub struct KnownLength {
    pub a: usize,
    pub b: usize,
    pub length: f64,
}

#[derive(serde::Serialize)]
struct SunAt {
    azimuth: f64,
    altitude: f64,
    lat: f64,
    lon: f64,
    /// false: no location set, so a default (40 N, 98 W) was used
    located: bool,
    utc_offset: f64,
}

/// The real sun for the model's location at a local date ("2026-06-21") and time ("15:30").
fn sun_at(model: &Model, date: Option<&str>, time: Option<&str>, utc_offset: Option<f64>) -> Result<SunAt> {
    let located = model.location.is_some();
    let (lat, lon) = model.location.as_ref().map(|l| (l.lat, l.lon)).unwrap_or((40.0, -98.0));
    let parts: Vec<&str> = date.unwrap_or("2026-06-21").split('-').collect();
    let num = |i: usize| parts.get(i).and_then(|s| s.trim().parse::<i64>().ok());
    let (Some(y), Some(mo), Some(da)) = (num(0), num(1), num(2)) else { bail!("date is year-month-day, like 2026-06-21") };
    if !(1..=12).contains(&mo) || !(1..=31).contains(&da) {
        bail!("date is year-month-day, like 2026-06-21");
    }
    let hm: Vec<&str> = time.unwrap_or("15:00").split(':').collect();
    let Some(h) = hm.first().and_then(|s| s.trim().parse::<f64>().ok()) else { bail!("time is hours:minutes, like 15:30") };
    let m = hm.get(1).and_then(|s| s.trim().parse::<f64>().ok()).unwrap_or(0.0);
    let utc_offset = utc_offset.unwrap_or((lon / 15.0).round());
    let (azimuth, altitude) = crate::pt::solar_position(lat, lon, y as i32, mo as u32, da as u32, h + m / 60.0, utc_offset);
    Ok(SunAt { azimuth, altitude, lat, lon, located, utc_offset })
}

/// The sun for a sun-study frame: [azimuth, altitude] from the model's location.
fn timeline_sun(model: &Model, date: &str, time: &str) -> Result<[f64; 2]> {
    let s = sun_at(model, Some(date), Some(time), None)?;
    Ok([s.azimuth, s.altitude])
}

/// Where a video's frames wait to be encoded (a session name is letters and digits only).
fn video_dir(session: &str) -> Result<std::path::PathBuf> {
    if session.is_empty() || session.len() > 40 || !session.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        bail!("bad video session name");
    }
    Ok(crate::safety::cache_dir("video").join(session))
}

/// Where app skins live: ~/Documents/SketchCraft/Skins.
fn skins_dir() -> std::path::PathBuf {
    crate::library::default_dir().parent().map(|p| p.join("Skins")).unwrap_or_else(|| std::path::PathBuf::from("Skins"))
}
