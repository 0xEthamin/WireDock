//! Display task. Consumes `DisplayLine` values and prints them through
//! `reedline`'s `ExternalPrinter` so async events do not corrupt the prompt.

use reedline::ExternalPrinter;
use tokio::sync::mpsc;

use crate::domain::event::DisplayLine;

/// Spawn the display task and return both the channel sender and the
/// `ExternalPrinter` handle (so the REPL can hand it to its `Reedline`).
pub fn spawn_display_task()
    -> (mpsc::Sender<DisplayLine>, ExternalPrinter<String>)
{
    let printer = ExternalPrinter::<String>::default();
    let printer_for_task = printer.clone();
    let (tx, mut rx) = mpsc::channel::<DisplayLine>(1024);

    tokio::spawn(async move
    {
        while let Some(line) = rx.recv().await
        {
            // If the printer is disconnected (reedline dropped), we stop.
            if printer_for_task.print(line.text).is_err()
            {
                break;
            }
        }
    });

    (tx, printer)
}