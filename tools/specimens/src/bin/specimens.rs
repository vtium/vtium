//! specimens: draw the demonstration set, judge it, or print the card.
//!
//! ```text
//! specimens draw [--review DIR]
//! specimens check
//! specimens card PROFILE [PAGE]
//! ```
//!
//! `draw` writes `docs/specimens/` from the profile table, and with
//! `--review` copies each built profile's collage, or its one screen, to
//! `DIR/PROFILE.png`. `check` draws the same files and compares them with
//! what is committed, the simulated renders' faults reported apart; it exits 0 when all match, 1 when one does not and 2
//! when it cannot run. `card` writes a page of the card's bytes to
//! standard output, for `cat` on a VT of that size.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use vtium_specimens::profiles::profile;
use vtium_specimens::{card, check, docs, draw, png};

const HERE: &str = env!("CARGO_MANIFEST_DIR");

fn usage() -> ExitCode {
    eprintln!(
        "usage: specimens draw [--review DIR]\n       specimens check\n       specimens card PROFILE [PAGE]"
    );
    ExitCode::from(2)
}

fn specimens_dir() -> PathBuf {
    Path::new(HERE).join("../../docs/specimens")
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["draw"] => draw_all(None),
        ["draw", "--review", dir] => draw_all(Some(Path::new(dir))),
        ["check"] => match check::check(&specimens_dir()) {
            Ok(faults) if faults.is_empty() => {
                println!("specimens: every file matches its drawing");
                ExitCode::SUCCESS
            }
            Ok(faults) => {
                for f in &faults.goldens {
                    eprintln!("specimens: {f}");
                }
                for f in &faults.simulations {
                    eprintln!("specimens: simulation stale: {f}");
                }
                eprintln!(
                    "specimens: {} faults in the specimens, {} in the simulations; `make specimens` redraws them",
                    faults.goldens.len(),
                    faults.simulations.len()
                );
                ExitCode::FAILURE
            }
            Err(e) => {
                eprintln!("specimens: {e}");
                ExitCode::from(2)
            }
        },
        ["card", name] => print_card(name, "1"),
        ["card", name, page] => print_card(name, page),
        _ => usage(),
    }
}

fn draw_all(review: Option<&Path>) -> ExitCode {
    let dir = specimens_dir();
    match check::write(&dir) {
        Ok(files) => println!("specimens: wrote {} files", files.len()),
        Err(e) => {
            eprintln!("specimens: {e}");
            return ExitCode::from(2);
        }
    }
    let Some(review) = review else {
        return ExitCode::SUCCESS;
    };
    if let Err(e) = std::fs::create_dir_all(review) {
        eprintln!("specimens: {}: {e}", review.display());
        return ExitCode::from(2);
    }
    for p in vtium_specimens::profiles::PROFILES {
        let Ok(screens) = draw::screens(p) else {
            continue;
        };
        let img = if screens.len() == 1 {
            screens[0].clone()
        } else {
            draw::collage(&screens)
        };
        let path = review.join(format!("{}.png", p.name));
        if let Err(e) = std::fs::write(&path, png::encode(&img)) {
            eprintln!("specimens: {}: {e}", path.display());
            return ExitCode::from(2);
        }
        println!(
            "specimens: {} ({})",
            path.display(),
            docs::preview(screens.len())
        );
    }
    ExitCode::SUCCESS
}

fn print_card(name: &str, page: &str) -> ExitCode {
    let Some(p) = profile(name) else {
        eprintln!("specimens: no profile {name}");
        return ExitCode::from(2);
    };
    let layout = match draw::font(p).and_then(|f| draw::layout(p, &f)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("specimens: {e}");
            return ExitCode::from(2);
        }
    };
    let Some(n) = page
        .parse::<usize>()
        .ok()
        .filter(|n| (1..=layout.pages).contains(n))
    else {
        eprintln!("specimens: {name} has pages 1 to {}", layout.pages);
        return ExitCode::from(2);
    };
    let bytes = card::page(p, &layout, n - 1);
    match std::io::stdout().write_all(&bytes) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("specimens: {e}");
            ExitCode::from(2)
        }
    }
}
