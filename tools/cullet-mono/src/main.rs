//! Builds Cullet Mono, vtium's outline Terminus with Nerd Fonts' icons,
//! from pinned sources, and checks what it built. README.md says what the
//! build needs and how to run it.
//!
//! `cullet-mono build --out DIR` fetches the pinned sources into a cache,
//! patches each Terminus (TTF) face with Nerd Fonts' patcher under
//! FontForge, and writes the faces, their notices and a record of the build
//! into DIR. `cullet-mono check DIR` checks faces built earlier, without
//! building them again. Both exit with status 1 and list every failed
//! check when a face is not the one `pins.tsv` pins.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::ExitCode;

mod build;
mod check;
mod finish;
mod pins;
mod recipe;
mod sfnt;

/// SHA-256 from vtium-font's own implementation, included by path so that
/// this tool needs no crate and no dependency on the main workspace.
#[path = "../../../crates/vtium-font/src/sha256.rs"]
#[allow(dead_code)]
mod sha256;

const USAGE: &str = "usage: cullet-mono build --out DIR [--cache DIR]
       cullet-mono check DIR

build  fetches the pinned sources into the cache (default
       $XDG_CACHE_HOME/vtium/cullet-mono), patches both faces with FontForge,
       and writes them with their notices into DIR
check  checks the faces in DIR against pins.tsv without building them";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["build", "--out", out] => build::run(&PathBuf::from(out), &build::cache_dir()),
        ["build", "--out", out, "--cache", cache] | ["build", "--cache", cache, "--out", out] => {
            build::run(&PathBuf::from(out), &PathBuf::from(cache))
        }
        ["check", dir] => check::dir(&PathBuf::from(dir)),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(problems) => {
            for problem in problems {
                eprintln!("cullet-mono: {problem}");
            }
            ExitCode::FAILURE
        }
    }
}
