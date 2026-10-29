//! vtium-vitrify: generates vtium's glyphs from frits, and switches the
//! console's frit (ADR-1200).

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
    vtium_vitrify::banner::show(&args, &mut std::io::stdout());
    let status = vtium_vitrify::cli::run(&args, &mut std::io::stdout(), &mut std::io::stderr());
    ExitCode::from(status)
}
