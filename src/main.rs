//! wiredock entry point.
//!
//! Wires the domain, infra, and REPL layers together:
//! * parses CLI args by hand,
//! * creates the stderr logger,
//! * spawns the display task (reedline ExternalPrinter backed),
//! * spawns the registry task with tokio-backed connector/listener,
//! * spawns the REPL input task,
//! * awaits Ctrl+C at the top level with `biased` priority.
//!
//! Clean shutdown sequence: Ctrl+C (or REPL EOF) -> send `Shutdown` to the
//! registry -> registry drops every resource explicitly -> process exits 0.

mod domain;
mod infra;
mod repl;

use std::sync::Arc;

use tokio::sync::mpsc;

use crate::domain::event::RegistryInput;
use crate::domain::logger::Logger;
use crate::infra::{StderrLogger, TokioConnector, TokioListener};
use crate::repl::args::CliArgs;
use crate::repl::display::spawn_display_task;
use crate::repl::input::{spawn_repl, InputDeps};
use crate::repl::runtime::{run_registry, RuntimeDeps};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> std::process::ExitCode
{
    let args = match CliArgs::parse_env()
    {
        Ok(a)  => a,
        Err(e) =>
        {
            eprintln!("{e}");
            eprintln!("{}", CliArgs::usage());
            return std::process::ExitCode::from(2);
        }
    };
    if args.help_requested
    {
        eprintln!("{}", CliArgs::usage());
        return std::process::ExitCode::SUCCESS;
    }

    let logger: Arc<dyn Logger> = Arc::new(StderrLogger::new(args.log_level));
    logger.info("wiredock starting");

    let (display_tx, printer)  = spawn_display_task();
    let (input_tx, input_rx)   = mpsc::channel::<RegistryInput>(1024);

    let registry_deps = RuntimeDeps
    {
        connector: TokioConnector,
        listener:  TokioListener,
        logger:    logger.clone(),
        display:   display_tx.clone(),
        input_tx:  input_tx.clone(),
        input_rx,
    };
    let mut registry_task = tokio::spawn(run_registry(registry_deps));


    let repl_task = spawn_repl(InputDeps
    {
        printer,
        input_tx:  input_tx.clone(),
        display:   display_tx.clone(),
        logger:    logger.clone(),
    });

    // Top-level shutdown arbiter. `biased` gives Ctrl+C priority at every
    // iteration. A single Ctrl+C triggers a clean teardown via the registry.
    let shutdown_tx = input_tx.clone();
    let mut registry_done = false;
    
    tokio::select!
    {
        biased;
        _ = tokio::signal::ctrl_c() =>
        {
            logger.info("ctrl-c received from OS");
            let _ = shutdown_tx.send(RegistryInput::Shutdown).await;
        }
        res = &mut registry_task =>
        {
            registry_done = true;
            if let Err(e) = res {
                logger.error(&format!("registry task panicked: {e}"));
            }
        }
    }

    if !registry_done
    {
        let _ = registry_task.await;
    }

    repl_task.abort();
    logger.info("wiredock exited cleanly");
    std::process::ExitCode::SUCCESS
}