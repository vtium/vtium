//! vtium-config: generates vtium's glyphs from frits, and switches the
//! console's frit (ADR-1200).

use std::ffi::OsString;
use std::process::ExitCode;

fn main() -> ExitCode {
    // A descriptor the process inherited without close-on-exec would reach
    // every program vtium-config starts, the glyph helper that parses fonts
    // among them. Marking them all here, before any command runs, leaves each
    // child holding only what its spawn hands it. vtium-config hands no child
    // an inherited descriptor on purpose.
    if let Err(e) = vtium_platform::cloexec_above_stdio() {
        eprintln!(
            "vtium-config: cannot start safely: cannot mark inherited file descriptors close-on-exec: {e}"
        );
        return ExitCode::from(1);
    }
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
    let status = vtium_config::cli::run(&args, &mut std::io::stdout(), &mut std::io::stderr());
    ExitCode::from(status)
}
