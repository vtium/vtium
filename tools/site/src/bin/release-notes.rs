//! release-notes: writes the body of a GitHub release from the release's
//! page on the site.
//!
//! Usage: `release-notes [--root DIR] --out FILE VERSION`
//!
//! It reads `docs/releases/VERSION.md` under DIR, the checkout's root (the
//! current directory unless named), and the site's address from DIR's
//! `zensical.toml`. It writes the page to FILE with each relative link
//! turned into its target's full address on the site, so the links work on
//! GitHub. `make release-notes VERSION=V` runs it, and the release's steps
//! pass FILE to `gh release create --notes-file`.
//!
//! References:
//! - ADR-1508 D3: the page is the one text of the notes, and GitHub's text
//!   is generated from it.

use std::path::PathBuf;
use std::process::ExitCode;

use vtium_site::Site;
use vtium_site::release::{Version, for_github};

const USAGE: &str = "usage: release-notes [--root DIR] --out FILE VERSION";

fn run() -> Result<(), String> {
    let mut root = PathBuf::from(".");
    let mut out = None;
    let mut version = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => root = args.next().ok_or(USAGE)?.into(),
            "--out" => out = Some(PathBuf::from(args.next().ok_or(USAGE)?)),
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ if version.is_none() && !arg.starts_with('-') => version = Some(arg),
            _ => return Err(USAGE.into()),
        }
    }
    let (Some(out), Some(version)) = (out, version) else {
        return Err(USAGE.into());
    };
    let version = Version::parse(&version)?;
    let config = root.join("zensical.toml");
    let text =
        std::fs::read_to_string(&config).map_err(|e| format!("{}: {e}", config.display()))?;
    let site_url = Site::parse(&text)?
        .site_url
        .ok_or("zensical.toml names no site_url")?;
    let page = format!("releases/{}.md", version.as_str());
    let path = root.join("docs").join(&page);
    let notes = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let body = for_github(&notes, &page, &site_url)?;
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&out, body).map_err(|e| format!("{}: {e}", out.display()))?;
    println!("release-notes: wrote {}", out.display());
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("release-notes: {e}");
            ExitCode::FAILURE
        }
    }
}
