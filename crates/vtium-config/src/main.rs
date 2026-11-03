//! vtium-config: generates vtium's glyphs from frits, and switches the
//! console's frit (ADR-1200).

use std::ffi::OsString;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if args.len() == 1
        && args
            .first()
            .is_some_and(|a| a == vtium_config::child::PARSE_ARG)
    {
        return ExitCode::from(vtium_config::child::run_parser());
    }
    let euid = rustix::process::geteuid().as_raw();
    if let Some(why) = vtium_config::cli::root_refusal(euid, &args) {
        eprintln!("vtium-config: {why}");
        return ExitCode::from(2);
    }
    if let Some((first, rest)) = args.split_first()
        && first == vtium_config::tui::port::UNBLOCK_ARG
    {
        return ExitCode::from(vtium_config::tui::port::run_with_signals(rest));
    }
    vtium_config::banner::show(&args, &mut std::io::stdout());
    let status = vtium_config::cli::run(&args, &mut std::io::stdout(), &mut std::io::stderr());
    ExitCode::from(status)
}
