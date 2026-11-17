//! `vtium-man write DIR` writes vtium's man pages into DIR. `vtium-man
//! check DIR` writes nothing: it exits 1 and names each page in DIR that
//! differs from what the generator writes, is missing, or is one the
//! generator does not write, and exits 0 when the pages match.

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mode, dir) = match args.as_slice() {
        [mode, dir] if mode == "write" || mode == "check" => (mode.as_str(), Path::new(dir)),
        _ => {
            eprintln!("usage: vtium-man write|check DIR");
            return ExitCode::from(2);
        }
    };
    let pages = vtium_man::pages();
    if mode == "write" {
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("vtium-man: cannot create {}: {e}", dir.display());
            return ExitCode::FAILURE;
        }
        for page in &pages {
            let path = dir.join(page.file);
            if let Err(e) = std::fs::write(&path, &page.text) {
                eprintln!("vtium-man: cannot write {}: {e}", path.display());
                return ExitCode::FAILURE;
            }
        }
        return ExitCode::SUCCESS;
    }
    let mut stale = Vec::new();
    for page in &pages {
        let path = dir.join(page.file);
        match std::fs::read_to_string(&path) {
            Ok(text) if text == page.text => {}
            Ok(_) => stale.push(format!(
                "{} differs from what the code says",
                path.display()
            )),
            Err(e) => stale.push(format!("{}: {e}", path.display())),
        }
    }
    let known: Vec<&str> = pages.iter().map(|p| p.file).collect();
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut extra: Vec<String> = entries
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| !known.contains(&name.as_str()))
            .collect();
        extra.sort();
        for name in extra {
            stale.push(format!(
                "{} is not a page the generator writes",
                dir.join(name).display()
            ));
        }
    }
    if stale.is_empty() {
        return ExitCode::SUCCESS;
    }
    for line in stale {
        eprintln!("vtium-man: {line}");
    }
    eprintln!("vtium-man: the man pages are stale. Run make man and commit the pages it writes.");
    ExitCode::FAILURE
}
