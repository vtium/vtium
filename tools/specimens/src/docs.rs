//! What `docs/specimens/` holds, drawn and written from the profile table:
//! each built profile's screens, its collage when it takes several, its
//! README and its vtium.conf, and the README of them all. `draw` writes
//! these; `check` compares them with what is committed.

use std::collections::BTreeMap;
use std::path::Path;

use crate::card;
use crate::draw::{self, Image};
use crate::profiles::{Glyphs, PROFILES, Profile};

/// A file's contents: an image, judged by its pixels, or text, judged by
/// its bytes.
#[derive(Debug)]
pub enum Content {
    Png(Image),
    Text(String),
}

/// The profiles the operator has not reviewed, from `provisional.tsv`
/// beside the specimens: one name to a line, `#` starting a comment.
pub fn provisional(dir: &Path) -> Result<Vec<String>, String> {
    let path = dir.join("provisional.tsv");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut names = Vec::new();
    for line in text
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let name = line.split('\t').next().unwrap_or_default();
        match PROFILES.iter().find(|p| p.name == name) {
            Some(p) if matches!(p.glyphs, Glyphs::Builtin { .. }) => names.push(name.to_owned()),
            Some(_) => return Err(format!("provisional.tsv: {name} is pending, not drawn")),
            None => return Err(format!("provisional.tsv: no profile {name}")),
        }
    }
    Ok(names)
}

/// The screens' file names for a profile of `pages` screens.
fn screen_names(pages: usize) -> Vec<String> {
    if pages == 1 {
        vec!["screen.png".to_owned()]
    } else {
        (1..=pages).map(|n| format!("screen-{n}.png")).collect()
    }
}

/// The image that shows a profile whole: its collage, or its one screen.
pub fn preview(pages: usize) -> &'static str {
    if pages == 1 {
        "screen.png"
    } else {
        "collage.png"
    }
}

fn issues(list: &[u32]) -> String {
    list.iter()
        .map(|i| format!("#{i}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The directory under `docs/specimens/` simulated renders go in, which
/// judge nothing.
pub const SIMULATED: &str = "simulated";

/// A profile's directory under `docs/specimens/`.
pub fn dir(p: &Profile) -> String {
    match p.panel {
        Some(_) => format!("{SIMULATED}/{}", p.name),
        None => p.name.to_owned(),
    }
}

/// Every file under `docs/specimens/`, by its path there, but
/// `provisional.tsv`, which the operator keeps.
pub fn files(provisional: &[String]) -> Result<BTreeMap<String, Content>, String> {
    let mut out = BTreeMap::new();
    let (mut rows, mut simulated) = (Vec::new(), Vec::new());
    for p in PROFILES {
        let Glyphs::Builtin { set, .. } = p.glyphs else {
            rows.push(row(p, None, false));
            continue;
        };
        let d = dir(p);
        let screens = draw::screens(p)?;
        let font = draw::font(p)?;
        let layout = draw::layout(p, &font)?;
        let cell = font.cell();
        let names = screen_names(screens.len());
        if screens.len() > 1 {
            out.insert(
                format!("{d}/collage.png"),
                Content::Png(draw::collage(&screens)),
            );
        }
        let pending = provisional.iter().any(|n| n == p.name);
        let facts = Facts {
            cell: format!("{}x{}", cell.width, cell.height),
            grid: format!("{}x{}", layout.cols, layout.rows),
            set,
            screens: screens.len(),
        };
        out.insert(
            format!("{d}/README.md"),
            Content::Text(profile_readme(p, &facts, &names)),
        );
        out.insert(format!("{d}/vtium.conf"), Content::Text(p.conf.to_owned()));
        for (name, img) in names.into_iter().zip(screens) {
            out.insert(format!("{d}/{name}"), Content::Png(img));
        }
        let r = row(p, Some(&facts), pending);
        match p.panel {
            Some(_) => simulated.push(r),
            None => rows.push(r),
        }
    }
    out.insert(
        "README.md".to_owned(),
        Content::Text(index(&rows, &simulated)),
    );
    Ok(out)
}

/// What a built profile draws: its cell, grid, set and screens.
struct Facts {
    cell: String,
    grid: String,
    set: &'static str,
    screens: usize,
}

fn row(p: &Profile, facts: Option<&Facts>, provisional: bool) -> String {
    match (facts, p.glyphs) {
        (Some(f), _) => {
            let status = match (p.panel, provisional) {
                (Some(_), _) => "simulation",
                (None, true) => "provisional",
                (None, false) => "reviewed",
            };
            let shot = preview(f.screens);
            format!(
                "| [{name}]({d}/) | {} | {} | {} | `{}` | {} | {status} | [<img src=\"{d}/{shot}\" width=\"160\">]({d}/{shot}) |",
                p.depth,
                f.cell,
                f.grid,
                f.set,
                f.screens,
                name = p.name,
                d = dir(p),
            )
        }
        (None, Glyphs::Pending { font, issues: list }) => format!(
            "| {} | {} | | | {font} | | pending {} | |",
            p.name,
            p.depth,
            issues(list)
        ),
        (None, Glyphs::Builtin { .. }) => unreachable!("a built profile is drawn"),
    }
}

fn profile_readme(p: &Profile, f: &Facts, names: &[String]) -> String {
    let arrayed = if f.screens == 1 {
        "the whole card on one screen".to_owned()
    } else {
        format!(
            "the card on {} screens, shown together in the collage",
            f.screens
        )
    };
    let images = names
        .iter()
        .map(|n| format!("[{n}]({n})"))
        .collect::<Vec<_>>()
        .join(", ");
    let (title, simulation) = match p.panel {
        Some(panel) => (
            format!("{} (simulated)", p.name),
            format!(
                "**A simulation of the panel's conversion, not vtium's output.** vtium draws this display as color; these screens are that drawing taken through the conversion the panel's Linux driver makes (v7.3): each pixel to gray8 by BT.601 luma, `(77r + 150g + 29b) / 256` (`drivers/gpu/drm/drm_format_internal.h:39-47`), then to the panel's levels as {}. Each level is drawn as the gray it is on a panel whose levels are evenly spaced. They judge nothing: `make specimens-check` leaves `simulated/` out.\n\n",
                panel.driver()
            ),
        ),
        None => (p.name.to_owned(), String::new()),
    };
    format!(
        "# {title}\n\n\
         {simulation}\
         {summary}: {width}x{height}, {depth}, drawn in `{set}` at {cell}, {grid} cells, {arrayed}.\n\n\
         ![{name}]({preview})\n\n\
         | Display | Class | Cell | Grid | Font | Screens |\n\
         |---|---|---|---|---|---|\n\
         | {width}x{height}, {depth} | {class} | {cell} | {grid} | `{set}`, built in | {images} |\n\n\
         ## Why this font\n\n\
         {why}\n\n\
         ## Configuration\n\n\
         [vtium.conf](vtium.conf):\n\n\
         ```ini\n{conf}```\n\n\
         The console chooses this set by itself once #126 and #118's rule land; the screens here draw vtium-font's built-in set directly.\n",
        name = p.name,
        summary = p.summary,
        width = p.width,
        height = p.height,
        depth = p.depth,
        class = p.class,
        set = f.set,
        cell = f.cell,
        grid = f.grid,
        preview = preview(f.screens),
        why = p.why,
        conf = p.conf,
    )
}

fn index(rows: &[String], simulated: &[String]) -> String {
    let titles = card::TITLES
        .iter()
        .enumerate()
        .map(|(i, t)| format!("{}. {t}", i + 1))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "# VTium specimens\n\n\
         vtium's test card, drawn by vtium's renderer in memory for each display profile, from a 1-bit 320x240 panel up to a 2560x1440 monitor (#142). Each profile's directory holds the screens, the collage where the card takes several, the font and configuration vtium uses there and why, and its vtium.conf.\n\n\
         | Profile | Depth | Cell | Grid | Font | Screens | Status | Preview |\n\
         |---|---|---|---|---|---|---|---|\n\
         {rows}\n\n\
         A pending profile waits on the issues it names. A provisional one is drawn and judged, and waits on the operator's review; `provisional.tsv` lists them.\n\n\
         ## Gray panels, simulated\n\n\
         vtium draws a gray panel exactly as a color display, and the panel's driver converts each pixel (#135). These renders take vtium's color drawing through that conversion, as Linux v7.3's drivers make it, to show what such a panel shows. They are simulations, not vtium's output, and judge nothing.\n\n\
         | Profile | Depth | Cell | Grid | Font | Screens | Status | Preview |\n\
         |---|---|---|---|---|---|---|---|\n\
         {simulated}\n\n\
         ## The card\n\n\
         The card is one byte stream of vtium's own, written by `tools/specimens`, in four blocks of 40x30 cells. A block is one 320x240 screen at 8x8, and a page holds as many blocks as its grid fits, centered:\n\n\
         {titles}\n\n\
         `tools/specimens/README.md` lists what the card leaves out and why.\n\n\
         ## Drawing and judging\n\n\
         `make specimens` draws every file here from the profile table. `make specimens-check`, and `cargo test` through `tools/specimens/tests/`, draw them again and fail on any difference from what is committed, `simulated/` aside: an image by its pixels, a text by its bytes, and a file here that the table does not draw. A change that moves a specimen fails until the moved files are committed, each in a commit of its own that lists what moved.\n",
        rows = rows.join("\n"),
        simulated = simulated.join("\n"),
    )
}
