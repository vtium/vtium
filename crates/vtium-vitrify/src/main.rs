//! vtium-vitrify: builds vitrified font sets for vtium from frits, and
//! keeps the root-owned store they are loaded from (ADR-1200).

use std::ffi::OsString;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if args.len() == 1
        && args
            .first()
            .is_some_and(|a| a == vtium_vitrify::child::PARSE_ARG)
    {
        return ExitCode::from(vtium_vitrify::child::run_parser());
    }
    let status = vtium_vitrify::cli::run(&args, &mut std::io::stdout(), &mut std::io::stderr());
    ExitCode::from(status)
}
