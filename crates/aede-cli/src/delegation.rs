//! Local command delegation while the matching server is running.

use std::error::Error;

use crate::args::Args;

pub fn try_delegate(args: &Args, raw: Vec<String>) -> Result<Option<i32>, Box<dyn Error>> {
    let data_dir = crate::commands::data_dir(args);
    aede_server::delegate_command(&data_dir, raw, crate::canonical(&args.command))
}

pub fn allowed(raw: &[String], command: &str) -> bool {
    let args = Args::parse(raw.to_vec());
    !args.has("help")
        && !args.has("version")
        && crate::canonical(&args.command) == command
        && crate::mutates_store(crate::canonical(&args.command))
}
