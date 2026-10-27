//! The replay interface's command line over vtium-term.

use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match vtium_conformance::main(&args) {
        Ok(out) => {
            let mut stdout = std::io::stdout().lock();
            if stdout
                .write_all(out.as_bytes())
                .and_then(|()| stdout.flush())
                .is_err()
            {
                return ExitCode::from(3);
            }
            ExitCode::SUCCESS
        }
        Err(failure) => {
            eprintln!("vtium-replay: {}", failure.message);
            ExitCode::from(failure.code)
        }
    }
}
