//! Batches and undo/redo.
//! Arms of `Engine::run`; each works on the model being edited (inside `edit`, the open component's definition).

use super::*;

impl Engine {
    pub(super) fn run_session(&mut self, cmd: Command) -> Result<Value> {
        Ok(match cmd {
            Command::BeginBatch => {
                if self.batch.is_some() {
                    bail!(
                        "a batch is already open{}: batches don't nest, so end it first (one nobody ends is closed on its own)",
                        match self.batch_kind {
                            BatchKind::Agent => " (an agent's)",
                            BatchKind::Plugin => " (a plugin's run)",
                            _ => "",
                        }
                    );
                }
                self.batch_kind = if self.untrusted { BatchKind::Agent } else { BatchKind::Fresh };
                self.batch_last = Some(std::time::Instant::now());
                self.batch = Some((self.model.clone(), self.revision, std::time::Instant::now()));
                self.batch_seq += 1;
                json!({"batch": self.batch_seq})
            }
            Command::EndBatch { id } => {
                let agent = self.untrusted;
                let current = id.is_none_or(|i| i == self.batch_seq);
                match self.batch_kind() {
                    BatchKind::None if id.is_none() => bail!("no batch is open, so there is nothing to end"),
                    BatchKind::Agent if current && !agent => bail!("that batch is an agent's, not the app's"),
                    BatchKind::Fresh | BatchKind::App | BatchKind::Plugin if current && agent => bail!("that batch is the app's, not an agent's"),
                    _ => {}
                }
                // an end that names a batch that is already over (closed by the timer or by the person's own edit, or
                // replaced) leaves the current one alone; what that batch did is already its own undo step
                let over = self.batch.is_none() || !current;
                if current {
                    self.last_batch = None;
                    self.end_batch();
                }
                if over && self.last_batch.is_none() {
                    return Ok(json!({"already_ended": true}));
                }
                // a plugin's run that changed the model: who, and how many things (the app says so, with Undo)
                match self.last_batch.take() {
                    Some((who, n)) if n > 0 => json!({"origin": who, "changed": n}),
                    _ => json!({}),
                }
            }
            Command::Undo => {
                if self.batch.is_some() {
                    bail!("a plugin is still running; Undo works when it has finished");
                }
                self.step_history(true)?
            }
            Command::Redo => {
                if self.batch.is_some() {
                    bail!("a plugin is still running; Redo works when it has finished");
                }
                self.step_history(false)?
            }
            other => unreachable!("command routed to the wrong group: {other:?}"),
        })
    }
}
