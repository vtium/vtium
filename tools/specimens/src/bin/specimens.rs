//! specimens: draw the demonstration set, judge it, fetch its fonts, or
//! print the card.
//!
//! ```text
//! specimens draw [--review DIR]
//! specimens check
//! specimens fetch-fonts
//! specimens card PROFILE [PAGE]
//! ```
//!
//! `draw` writes `docs/specimens/` from the profile table, and with
//! `--review` copies each drawn profile's collage, or its one screen, to
//! `DIR/PROFILE.png`. `check` draws the same files and compares them with
//! what is committed; it exits 0 when no accepted profile moved, 1 when
//! one did, and 2 when it cannot run. Both generate the desktop frit's
//! glyphs through vtium-vitrify's engine from the fonts `fetch-fonts` keeps
//! in the cache, and skip the outline profiles, saying why, without them.
//! Both still take `--vitrify PATH`, which the Makefile passes, and ignore
//! it. `card`
//! writes a page of the card's bytes to standard output, for `cat` on a VT
//! of that size.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use vtium_font::Cell;
use vtium_specimens::draw::Sources;
use vtium_specimens::profiles::{Glyphs, PROFILES, profile};
use vtium_specimens::{card, check, docs, draw, fonts, png};

const HERE: &str = env!("CARGO_MANIFEST_DIR");

fn usage() -> ExitCode {
    eprintln!(
        "usage: specimens draw [--review DIR]\n       specimens check\n       \
         specimens fetch-fonts\n       specimens card PROFILE [PAGE]"
    );
    ExitCode::from(2)
}

fn specimens_dir() -> PathBuf {
    Path::new(HERE).join("../../docs/specimens")
}

/// The options after a command: `--review DIR`, and `--vitrify PATH`,
/// taken once and ignored.
#[derive(Default)]
struct Options {
    vitrify: bool,
    review: Option<PathBuf>,
}

fn options(args: &[String]) -> Option<Options> {
    let mut o = Options::default();
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let value = PathBuf::from(it.next()?);
        match flag.as_str() {
            "--vitrify" if !o.vitrify => o.vitrify = true,
            "--review" if o.review.is_none() => o.review = Some(value),
            _ => return None,
        }
    }
    Some(o)
}

fn sources() -> Sources {
    let cells: Vec<Cell> = PROFILES
        .iter()
        .filter(|p| matches!(p.glyphs, Glyphs::Desktop { .. }))
        .map(|p| Cell {
            width: p.cell.0,
            height: p.cell.1,
        })
        .collect();
    Sources {
        desktop: fonts::desktop(&fonts::cache_dir(), &cells),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((command, rest)) = args.split_first() else {
        return usage();
    };
    match (command.as_str(), rest) {
        ("draw", rest) => match options(rest) {
            Some(o) => draw_all(&o),
            None => usage(),
        },
        ("check", rest) => match options(rest) {
            Some(o) if o.review.is_none() => check_all(),
            _ => usage(),
        },
        ("fetch-fonts", []) => match fonts::fetch(&fonts::cache_dir()) {
            Ok(()) => {
                println!(
                    "specimens: the pinned fonts are in {}",
                    fonts::cache_dir().display()
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("specimens: {e}");
                ExitCode::FAILURE
            }
        },
        ("card", [name]) => print_card(name, "1"),
        ("card", [name, page]) => print_card(name, page),
        _ => usage(),
    }
}

fn check_all() -> ExitCode {
    let faults = match check::check(&specimens_dir(), &sources()) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("specimens: {e}");
            return ExitCode::from(2);
        }
    };
    for s in &faults.skipped {
        println!("specimens: skipped {s}");
    }
    for d in &faults.drift {
        println!("specimens: drift in a provisional profile: {d}");
    }
    for f in &faults.goldens {
        eprintln!("specimens: {f}");
    }
    for f in &faults.simulations {
        eprintln!("specimens: simulation stale: {f}");
    }
    if faults.failed() {
        eprintln!(
            "specimens: {} faults in accepted specimens, {} in simulations; `make specimens` redraws them",
            faults.goldens.len(),
            faults.simulations.len()
        );
        ExitCode::FAILURE
    } else {
        println!("specimens: every accepted profile matches its drawing");
        ExitCode::SUCCESS
    }
}

fn draw_all(o: &Options) -> ExitCode {
    let sources = sources();
    let drawn = match check::write(&specimens_dir(), &sources) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("specimens: {e}");
            return ExitCode::from(2);
        }
    };
    println!("specimens: wrote {} files", drawn.files.len());
    for (p, why) in &drawn.skipped {
        println!("specimens: skipped {}: {why}", p.name);
    }
    let Some(review) = &o.review else {
        return ExitCode::SUCCESS;
    };
    if let Err(e) = std::fs::create_dir_all(review) {
        eprintln!("specimens: {}: {e}", review.display());
        return ExitCode::from(2);
    }
    for p in PROFILES {
        let Ok(screens) = draw::screens(p, &sources) else {
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
    let layout = match p.layout() {
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
