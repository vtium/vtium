//! What `docs/specimens/` holds, drawn and written from the profile table:
//! each built profile's screens, its collage when it takes several, its
//! README and its vtium.conf, and the README of them all. `draw` writes
//! these; `check` compares them with what is committed.

use std::collections::BTreeMap;
use std::path::Path;

use crate::card;
use crate::draw::{self, Image, Sources};
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
        if PROFILES.iter().all(|p| p.name != name) {
            return Err(format!("provisional.tsv: no profile {name}"));
        }
        names.push(name.to_owned());
    }
    Ok(names)
}

/// The screens `p` draws on `layout`: the card's pages, then the message
/// bars'.
pub fn screen_count(p: &Profile, layout: &crate::profiles::Layout) -> usize {
    layout.pages + p.bar_screens
}

/// The screens' file names for a profile of `pages` screens.
fn screen_names(pages: usize) -> Vec<String> {
    if pages == 1 {
        vec!["screen.png".to_owned()]
    } else {
        (1..=pages).map(|n| format!("screen-{n}.png")).collect()
    }
}

/// Every image a profile of `pages` screens has: its screens, and its
/// collage when it has several.
pub fn image_names(pages: usize) -> Vec<String> {
    let mut names = screen_names(pages);
    if pages > 1 {
        names.push("collage.png".to_owned());
    }
    names
}

/// The image that shows a profile whole: its collage, or its one screen.
pub fn preview(pages: usize) -> &'static str {
    if pages == 1 {
        "screen.png"
    } else {
        "collage.png"
    }
}

/// The directory under `docs/specimens/` simulated renders go in: they
/// show a panel's conversion and judge nothing of vtium's drawing.
pub const SIMULATED: &str = "simulated";

/// A profile's directory under `docs/specimens/`.
pub fn dir(p: &Profile) -> String {
    match p.panel {
        Some(_) => format!("{SIMULATED}/{}", p.name),
        None => p.name.to_owned(),
    }
}

/// What `docs/specimens/` holds, by each file's path there, but
/// `provisional.tsv`, which the operator keeps; and the profiles whose
/// screens could not be drawn here, each with why, whose images are
/// neither drawn nor judged.
#[derive(Debug)]
pub struct Drawn {
    pub files: BTreeMap<String, Content>,
    pub skipped: Vec<(&'static Profile, String)>,
}

/// Every file under `docs/specimens/`. Its text comes from the profile
/// table alone, so it is the same whether a profile is drawn or skipped.
pub fn files(provisional: &[String], sources: &Sources) -> Result<Drawn, String> {
    let mut files = BTreeMap::new();
    let mut skipped = Vec::new();
    let (mut rows, mut simulated) = (Vec::new(), Vec::new());
    for p in PROFILES {
        let d = dir(p);
        let names = screen_names(screen_count(p, &p.layout()?));
        match draw::skipped(p, sources) {
            Some(why) => skipped.push((p, why.to_owned())),
            None => {
                let screens = draw::screens(p, sources)?;
                if screens.len() > 1 {
                    files.insert(
                        format!("{d}/collage.png"),
                        Content::Png(draw::collage(&screens)),
                    );
                }
                for (name, img) in names.iter().zip(screens) {
                    files.insert(format!("{d}/{name}"), Content::Png(img));
                }
            }
        }
        files.insert(
            format!("{d}/README.md"),
            Content::Text(profile_readme(p, &names)?),
        );
        files.insert(format!("{d}/vtium.conf"), Content::Text(p.conf.to_owned()));
        let r = row(p, provisional.iter().any(|n| n == p.name))?;
        match p.panel {
            Some(_) => simulated.push(r),
            None => rows.push(r),
        }
    }
    files.insert(
        "README.md".to_owned(),
        Content::Text(index(&rows, &simulated)),
    );
    Ok(Drawn { files, skipped })
}

/// The profile's font, as its README's table names it.
fn font_cell(p: &Profile) -> String {
    match p.glyphs {
        Glyphs::Builtin { set, .. } => format!("`{set}`, built in"),
        Glyphs::Desktop { .. } => {
            "`desktop`, the frit [`desktop.vtfrit`](../../../tools/specimens/frits/desktop.vtfrit)"
                .to_owned()
        }
    }
}

fn row(p: &Profile, provisional: bool) -> Result<String, String> {
    let l = p.layout()?;
    let status = match (p.panel, provisional) {
        (Some(_), true) => "simulation, provisional",
        (Some(_), false) => "simulation",
        (None, true) => "provisional",
        (None, false) => "accepted",
    };
    let screens = screen_count(p, &l);
    let shot = preview(screens);
    Ok(format!(
        "| [{name}]({d}/) | {} | {}x{} | {}x{} | `{}` | {} | {status} | [<img src=\"{d}/{shot}\" width=\"160\">]({d}/{shot}) |",
        p.depth,
        p.cell.0,
        p.cell.1,
        l.cols,
        l.rows,
        p.glyphs.set(),
        screens,
        name = p.name,
        d = dir(p),
    ))
}

fn profile_readme(p: &Profile, names: &[String]) -> Result<String, String> {
    let l = p.layout()?;
    let card = if l.pages == 1 {
        "the whole card on one screen".to_owned()
    } else {
        format!("the card on {} screens", l.pages)
    };
    let bars = match p.bar_screens {
        1 => "vtium's message bars on one more",
        _ => "vtium's message bars on two more",
    };
    let arrayed = format!("{card} and {bars}, shown together in the collage");
    let source = match p.glyphs {
        Glyphs::Builtin { .. } => {
            "The console chooses this set by itself once #126 and #118's rule land; the screens here draw vtium-font's built-in set directly."
        }
        Glyphs::Desktop { .. } => {
            "The console loads a set by name from #126. The screens here draw the glyphs vtium-vitrify's engine, the one the helper runs (ADR-1301 D2), generates from the frit and the pinned fonts `make specimens-fonts` fetches, in the profile's cell (`tools/specimens/README.md`); where those fonts are absent the check skips this profile while it is provisional, and fails it once accepted."
        }
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
                "**A simulation of the panel's conversion, not vtium's output.** vtium draws this display as color; these screens are that drawing taken through the conversion the panel's Linux driver makes (v7.3): each pixel to gray8 by BT.601 luma, `(77r + 150g + 29b) / 256` (`drivers/gpu/drm/drm_format_internal.h:39-47`), then to the panel's levels as {}. Each level is drawn as the gray it is on a panel whose levels are evenly spaced. They judge nothing: `make specimens-check` reports a stale one apart from the specimens that judge vtium.\n\n",
                panel.driver()
            ),
        ),
        None => (p.name.to_owned(), String::new()),
    };
    Ok(format!(
        "# {title}\n\n\
         {simulation}\
         {summary}: {width}x{height}, {depth}, drawn in `{set}` at {cell}, {grid} cells, {arrayed}.\n\n\
         ![{name}]({preview})\n\n\
         | Display | Class | Cell | Grid | Font | Screens |\n\
         |---|---|---|---|---|---|\n\
         | {width}x{height}, {depth} | {class} | {cell} | {grid} | {font} | {images} |\n\n\
         ## Why this font\n\n\
         {why}\n\n\
         ## Configuration\n\n\
         [vtium.conf](vtium.conf):\n\n\
         ```ini\n{conf}```\n\n\
         {source}\n",
        name = p.name,
        summary = p.summary,
        width = p.width,
        height = p.height,
        depth = p.depth,
        class = p.class,
        set = p.glyphs.set(),
        cell = format!("{}x{}", p.cell.0, p.cell.1),
        grid = format!("{}x{}", l.cols, l.rows),
        font = font_cell(p),
        preview = preview(screen_count(p, &l)),
        why = p.why,
        conf = p.conf,
    ))
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
         vtium's test card, drawn by vtium's renderer in memory for each display profile, from a 1-bit 320x240 panel up to a 2560x1440 monitor (#142). Each profile's directory holds the screens and their collage, the font and configuration vtium uses there and why, and its vtium.conf.\n\n\
         | Profile | Depth | Cell | Grid | Font | Screens | Status | Preview |\n\
         |---|---|---|---|---|---|---|---|\n\
         {rows}\n\n\
         A provisional profile waits on the operator's review, and `provisional.tsv` lists it: a drawing that moves it is reported as drift and fails nothing. An accepted profile fails on a moved pixel.\n\n\
         ## Gray panels, simulated\n\n\
         vtium draws a gray panel exactly as a color display, and the panel's driver converts each pixel (#135). These renders take vtium's color drawing through that conversion, as Linux v7.3's drivers make it, to show what such a panel shows. They are simulations, not vtium's output, and judge nothing of vtium's drawing; a stale one is reported as its own kind of fault.\n\n\
         | Profile | Depth | Cell | Grid | Font | Screens | Status | Preview |\n\
         |---|---|---|---|---|---|---|---|\n\
         {simulated}\n\n\
         ## The card\n\n\
         The card is one byte stream of vtium's own, written by `tools/specimens`, in blocks of 40x30 cells. A block is one 320x240 screen at 8x8, and a page holds as many blocks as its grid fits, centered:\n\n\
         {titles}\n\n\
         `tools/specimens/README.md` lists what the card leaves out and why.\n\n\
         ## The message bars\n\n\
         After the card, each profile draws vtium's message bars, its notices and its on-screen display (ADR-1401 D5, D7), drawn by the console's own overlay code, on one screen, or on two on the 320x240 displays, where the four notices fill a screen of their own. A display shows one box, across its top, so the screens stack four bands, each the top of a drawing of its own with a line beneath the bar saying what it is: four notices at once, the most a head shows; the on-screen display's full line sharing the box with a notice; the full line alone; and the minimal dot in the top-right cell.\n\n\
         ## Drawing and judging\n\n\
         `make specimens` draws every file here from the profile table. `make specimens-check`, and `cargo test` through `tools/specimens/tests/`, draw them again and compare them with what is committed: an image by its pixels, a text by its bytes, and a file here that the table does not draw. A difference in an accepted profile fails; one in a provisional profile is drift, reported and not failed; a stale simulation is reported as its own kind of fault. The outline profiles are drawn only where the pinned fonts are at hand, as `make specimens-fonts` gives them, and `cargo test` draws none; elsewhere a provisional one is skipped with why, and an accepted one fails. A branch that moves specimens regenerates them in their own commit, listing what moved; `provisional.tsv` is the only file here written by hand, and a conflict in the rest resolves by `make specimens-fonts && make specimens`.\n",
        rows = rows.join("\n"),
        simulated = simulated.join("\n"),
    )
}
