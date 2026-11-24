//! One time limit for everything an agent or a plugin asks for.
//!
//! The person's own commands have no clock. A command from an agent or a plugin runs under a limit (see
//! `AGENT_COMMAND_SECONDS`), and an agent's whole `sketchcraft_run` call under a larger one (`AGENT_CALL_SECONDS`).
//! The limit lives here, per thread, so the hot loops of the engine can reach it without being handed anything:
//!
//! * `over()` says whether the limit has passed;
//! * `check()` is the same as an error, for code that returns a `Result`;
//! * `poll()` is for tight loops that can't return an error: it looks at the clock every call when the work between
//!   calls is heavy and only every few calls when it is light (see `poll`), and, once the limit has passed, unwinds out of the command (the engine catches that, puts the model back as
//!   it was and says the command ran out of time).
//!
//! Threads a command starts for itself pick the limit up with `current()` and `adopt()`.

use std::cell::Cell;
use std::time::{Duration, Instant};

/// How long one command from an agent or a plugin may run, whatever it is.
pub const AGENT_COMMAND_SECONDS: Duration = Duration::from_secs(10);

/// How long one whole `sketchcraft_run` call from an agent may run (all of its commands together).
pub const AGENT_CALL_SECONDS: Duration = Duration::from_secs(30);

/// A few commands are slow by nature (a photoreal picture, a rendered animation): they get this long instead.
pub const HEAVY_COMMAND_SECONDS: Duration = Duration::from_secs(25);

/// How long the command called `name` may run when an agent or a plugin asks for it.
pub fn command_seconds(name: &str) -> Duration {
    if let Some(d) = TEST_COMMAND.with(Cell::get) {
        return d;
    }
    match name {
        "photoreal" | "animate" => HEAVY_COMMAND_SECONDS,
        _ => AGENT_COMMAND_SECONDS,
    }
}

/// How long one whole call from an agent may run.
pub fn call_seconds() -> Duration {
    TEST_CALL.with(Cell::get).unwrap_or(AGENT_CALL_SECONDS)
}

/// For tests only: shorten the limits on this thread (None puts them back), so a test of what happens at the limit
/// doesn't have to wait for it.
pub fn set_limits_for_tests(command: Option<Duration>, call: Option<Duration>) {
    TEST_COMMAND.with(|c| c.set(command));
    TEST_CALL.with(|c| c.set(call));
}

/// The panic payload `poll` unwinds with.
#[derive(Debug)]
pub struct Exceeded;

thread_local! {
    static LIMIT: Cell<Option<Instant>> = const { Cell::new(None) };
    static CALL_LIMIT: Cell<Option<Instant>> = const { Cell::new(None) };
    static TEST_COMMAND: Cell<Option<Duration>> = const { Cell::new(None) };
    static TEST_CALL: Cell<Option<Duration>> = const { Cell::new(None) };
    static TICKS: Cell<u32> = const { Cell::new(0) };
    static STRIDE: Cell<u32> = const { Cell::new(1) };
    static LAST_LOOK: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// The limit now in force on this thread (to hand to a thread the command starts).
pub fn current() -> Option<Instant> {
    LIMIT.with(Cell::get)
}

/// Put a limit taken from `current()` on this (new) thread.
pub fn adopt(limit: Option<Instant>) {
    LIMIT.with(|c| c.set(limit));
}

/// Has the limit passed?
pub fn over() -> bool {
    LIMIT.with(Cell::get).is_some_and(|d| Instant::now() >= d)
}

/// Has the whole call's limit passed (the one that decides a call is undone as a whole)?
pub fn call_over() -> bool {
    CALL_LIMIT.with(Cell::get).is_some_and(|d| Instant::now() >= d)
}

/// An error once the limit has passed.
pub fn check() -> anyhow::Result<()> {
    if over() {
        anyhow::bail!("that took too long, so it was stopped (use fewer or simpler outlines, or a smaller model)");
    }
    Ok(())
}

/// The most calls of `poll` that may pass between two looks at the clock.
const MAX_STRIDE: u32 = 16;

/// For tight loops: costs a counter, and once the limit has passed unwinds. How often it looks at the clock follows
/// the work: it starts by looking at every call, and only after seeing that the calls come very fast (a few
/// microseconds apart) does it look at every 2nd, 4th ... up to every 16th. Heavy iterations (one poll per big step)
/// therefore always read the clock, and the limit is overshot by at most a handful of light steps.
#[inline]
pub fn poll() {
    let Some(limit) = LIMIT.with(Cell::get) else { return };
    let n = TICKS.with(|t| {
        let n = t.get().wrapping_add(1);
        t.set(n);
        n
    });
    if n < STRIDE.with(Cell::get) {
        return;
    }
    TICKS.with(|t| t.set(0));
    let now = Instant::now();
    if now >= limit {
        std::panic::resume_unwind(Box::new(Exceeded));
    }
    let stride = STRIDE.with(Cell::get);
    let since = LAST_LOOK.with(|l| l.replace(Some(now))).map(|t| now.duration_since(t));
    if let Some(d) = since {
        // per call of poll
        let per = d / stride;
        let next = if per > Duration::from_micros(20) { 1 } else if per < Duration::from_micros(3) { (stride * 2).min(MAX_STRIDE) } else { stride };
        STRIDE.with(|s| s.set(next));
    }
}

/// Unwind now if the limit has passed (for a place that runs rarely, such as after worker threads have finished).
pub fn poll_now() {
    if over() {
        std::panic::resume_unwind(Box::new(Exceeded));
    }
}

/// For a thread a command starts: run `f` under the limit `current()` gave (take it before starting the thread),
/// and turn the unwind of `poll` into `None`. After the threads have been joined, call `poll_now()` so the command
/// itself stops.
pub fn in_worker<T>(limit: Option<Instant>, f: impl FnOnce() -> T) -> Option<T> {
    adopt(limit);
    STRIDE.with(|s| s.set(1));
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(v) => Some(v),
        Err(p) if is_exceeded(&*p) => None,
        Err(p) => std::panic::resume_unwind(p),
    }
}

/// Is this panic payload `poll`'s?
pub fn is_exceeded(payload: &(dyn std::any::Any + Send)) -> bool {
    payload.is::<Exceeded>()
}

struct Back(Option<Instant>, Option<Instant>);
impl Drop for Back {
    fn drop(&mut self) {
        LIMIT.with(|c| c.set(self.0));
        CALL_LIMIT.with(|c| c.set(self.1));
    }
}

/// Run `f` with the limit `limit` from now (an earlier limit already in force still stands), then put things back
/// (also when `f` unwinds).
pub fn scope<T>(limit: Duration, f: impl FnOnce() -> T) -> T {
    let before = current();
    let _back = Back(before, CALL_LIMIT.with(Cell::get));
    let d = Instant::now() + limit;
    LIMIT.with(|c| c.set(Some(before.map_or(d, |b| b.min(d)))));
    STRIDE.with(|s| s.set(1));
    TICKS.with(|t| t.set(0));
    LAST_LOOK.with(|l| l.set(None));
    f()
}

/// Run `f` with one limit for a whole call (`limit` from now) that every command inside it is also held to.
pub fn call_scope<T>(limit: Duration, f: impl FnOnce() -> T) -> T {
    let _back = Back(current(), CALL_LIMIT.with(Cell::get));
    CALL_LIMIT.with(|c| c.set(Some(Instant::now() + limit)));
    f()
}

/// Holds a call's limit until it is dropped (for a caller that can't wrap its loop in a closure).
pub struct CallGuard(Option<Instant>);
impl Drop for CallGuard {
    fn drop(&mut self) {
        CALL_LIMIT.with(|c| c.set(self.0));
    }
}

/// Start a call's limit that lasts until the guard is dropped.
pub fn call_guard(limit: Duration) -> CallGuard {
    let g = CallGuard(CALL_LIMIT.with(Cell::get));
    CALL_LIMIT.with(|c| c.set(Some(Instant::now() + limit)));
    g
}

/// Run one command under its own limit, and under the call's too when there is one.
pub fn command_scope<T>(limit: Duration, f: impl FnOnce() -> T) -> T {
    let call = CALL_LIMIT.with(Cell::get);
    scope(limit, || {
        if let Some(c) = call {
            LIMIT.with(|l| l.set(l.get().map(|d| d.min(c))));
        }
        f()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_is_free_without_a_limit_and_unwinds_after_it() {
        for _ in 0..1000 {
            poll();
        }
        let r = std::panic::catch_unwind(|| {
            scope(Duration::from_millis(20), || loop {
                poll();
            })
        });
        assert!(is_exceeded(&*r.unwrap_err()));
        assert!(current().is_none(), "the limit is put back after an unwind");
    }

    #[test]
    fn a_call_limit_holds_every_command_inside_it() {
        call_scope(Duration::from_millis(30), || {
            command_scope(Duration::from_secs(60), || {
                std::thread::sleep(Duration::from_millis(60));
                assert!(over() && call_over());
            });
        });
        assert!(!call_over());
    }
}
