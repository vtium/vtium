//! vtium-clipboard, also installed as vticlip: copies text from a file or
//! from its standard input to the console's clipboard. `src/lib.rs` says
//! how.

use std::process::ExitCode;

fn main() -> ExitCode {
    let argv: Vec<_> = std::env::args_os().collect();
    let exit = vtium_clipboard::run(
        &argv,
        std::io::stdin().lock(),
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
    );
    ExitCode::from(exit as u8)
}
