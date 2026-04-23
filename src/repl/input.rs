//! Reedline-based REPL loop.
//!
//! This runs on its own tokio task. It blocks the thread on `Reedline::read_line`
//! inside `spawn_blocking` so that the rest of the runtime keeps reacting to
//! network events and Ctrl+C.

use std::sync::Arc;

use reedline::{DefaultPrompt, ExternalPrinter, Reedline, Signal};
use tokio::sync::{mpsc, oneshot};

use crate::domain::event::{CommandOutcome, DisplayLine, RegistryInput};
use crate::domain::logger::Logger;
use crate::repl::parser::parse_line;

pub struct InputDeps
{
    pub printer:   ExternalPrinter<String>,
    pub input_tx:  mpsc::Sender<RegistryInput>,
    pub display:   mpsc::Sender<DisplayLine>,
    pub logger:    Arc<dyn Logger>,
}

/// Spawn the REPL input loop. When the user presses Ctrl+D (EOF) or Ctrl+C
/// inside reedline, it forwards `Shutdown` to the registry and returns.
pub fn spawn_repl(deps: InputDeps) -> tokio::task::JoinHandle<()>
{
    tokio::spawn(async move
    {
        let printer = deps.printer;
        let input_tx = deps.input_tx;
        let display  = deps.display;
        let logger   = deps.logger;

        let mut rl = Reedline::create().with_external_printer(printer);
        let prompt = DefaultPrompt::default();

        loop
        {
            // reedline is blocking; run it on the blocking pool.
            let signal = tokio::task::block_in_place(|| rl.read_line(&prompt));
            match signal
            {
                Ok(Signal::Success(line)) =>
                {
                    match parse_line(&line)
                    {
                        Ok(None) => continue,
                        Ok(Some(cmd)) =>
                        {
                            let (tx, rx) = oneshot::channel::<CommandOutcome>();
                            if input_tx
                                .send(RegistryInput::Command { cmd, reply: tx })
                                .await
                                .is_err()
                            {
                                logger.warn("registry channel closed");
                                break;
                            }
                            match rx.await
                            {
                                Ok(outcome) =>
                                {
                                    let tag = if outcome.success { "OK" } else { "ERR" };
                                    let _ = display.send(DisplayLine::new(
                                        format!("{tag} {}", outcome.message)
                                    )).await;
                                }
                                Err(_) =>
                                {
                                    logger.warn("command reply dropped by registry");
                                }
                            }
                        }
                        Err(e) =>
                        {
                            let _ = display.send(DisplayLine::new(
                                format!("ERR {e}")
                            )).await;
                        }
                    }
                }
                Ok(Signal::CtrlC) | Ok(Signal::CtrlD) =>
                {
                    // invariant: receiver is alive until after this task ends,
                    // because main awaits the registry task after the REPL exits.
                    let _ = input_tx.send(RegistryInput::Shutdown).await;
                    break;
                }
                Ok(_) => continue,
                Err(e) =>
                {
                    logger.error(&format!("reedline error: {e}"));
                    let _ = input_tx.send(RegistryInput::Shutdown).await;
                    break;
                }
            }
        }
    })
}