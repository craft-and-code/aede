//! Stops a cancellable task running under the local server.

use aede_server::CancelOutcome;

use super::{Res, data_dir};
use crate::args::Args;

pub fn cancel(args: &Args) -> Res {
    let [raw] = args.positionals.as_slice() else {
        return Err("give one task ID: aede cancel <task-id>".into());
    };
    let task_id = raw
        .parse::<u64>()
        .ok()
        .filter(|&id| id > 0)
        .ok_or("task ID must be a positive whole number")?;
    match aede_server::cancel_task(&data_dir(args), task_id)? {
        CancelOutcome::Requested => {
            println!("Cancellation requested for server task {task_id}.");
            Ok(())
        }
        CancelOutcome::NotFound => Err(format!(
            "task {task_id} is not a running cancellable scan or fetch on this server"
        )
        .into()),
        CancelOutcome::NoServer => Err("no Aède server is running for this data folder".into()),
        CancelOutcome::Unsupported => {
            Err("local task cancellation is not available on this platform".into())
        }
    }
}
