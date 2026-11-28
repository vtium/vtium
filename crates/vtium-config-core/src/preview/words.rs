//! The words of the previews: the line beside a picture, the line in place
//! of one, and the full text behind each line, which F1 opens. Each line
//! is one line on the screen, and what it leaves out goes behind it.
//!
//! References:
//! - crates/AGENTS.md, "Strings on screen", rules 7 and 8: one line where a paragraph was, and keys in brackets.
//! - ADR-1201 D4: the picture of the console and the line beside it.

use std::fmt::Display;

use vtium_font::Cell;

use crate::made::Made;
use crate::specimen::Summary;
use crate::ui::Message;

/// A line with nothing behind it.
fn line(text: String) -> Message {
    Message {
        line: text,
        more: None,
    }
}

/// A line, and the full text behind it.
fn with_more(text: String, more: String) -> Message {
    Message {
        line: text,
        more: Some(more),
    }
}

/// The display a caption names: the one found, or the one assumed where
/// none is connected.
fn display_words((w, h): (u32, u32), assumed: bool) -> String {
    if assumed {
        format!("an assumed {w}×{h} display, since none is connected")
    } else {
        format!("this {w}×{h} display")
    }
}

/// The line beside a picture of the console: the font, its size, the
/// character cell and the grid that cell gives on the display.
pub fn caption(
    font: &str,
    points: u32,
    display: (u32, u32),
    assumed: bool,
    cell: Cell,
    grid: (usize, usize),
) -> Message {
    line(format!(
        "{font} at {points} pt on {}: {}×{} px character cells, {}×{} characters.",
        display_words(display, assumed),
        cell.width,
        cell.height,
        grid.0,
        grid.1
    ))
}

/// What a box and grey mean in the detailed specimen, and the key that
/// shows the picture of the console in its place.
pub const DETAIL_MARKS: &str = "Characters in grey come from the built-in font, and a box marks a character no font can display. [p] shows a picture of the console.";

/// The line beside the detailed specimen of `font` at `points`, with what
/// the font helper's summary of it says once that has come.
pub fn detail_caption(
    font: &str,
    points: u32,
    summary: Option<&Summary>,
    display: (u32, u32),
    assumed: bool,
) -> Message {
    let Some(s) = summary else {
        return with_more(
            format!("{font} at {points} pt."),
            format!("{font} at {points} pt. {DETAIL_MARKS}"),
        );
    };
    let grid = s
        .grid
        .map(|(c, r)| format!(", {c}×{r} characters"))
        .unwrap_or_default();
    let script = s
        .script
        .as_ref()
        .map(|made| format!(" {}", script_caption(made)))
        .unwrap_or_default();
    with_more(
        format!(
            "{font} at {points} pt: {}×{} px character cells{grid}; it can display {} of the {} characters.",
            s.cell.width, s.cell.height, s.covered, s.of
        ),
        format!(
            "{font} at {points} pt on {}: each character cell is {}×{} pixels{grid}. The font can display {} of the {} characters in the preview.{script} {DETAIL_MARKS}",
            display_words(display, assumed),
            s.cell.width,
            s.cell.height,
            s.covered,
            s.of
        ),
    )
}

/// How the font makes the superscripts and subscripts of the detailed
/// specimen's last line (ADR-1202 D14): the way the font helper made them,
/// `made`, in the reader's words.
pub fn script_caption(made: &Made) -> String {
    match made {
        Made::Outline => "Superscripts and subscripts are drawn from this font's outlines at two thirds of its size.".to_owned(),
        Made::Color => "Superscripts and subscripts in color are this font's color images, scaled smoothly to two thirds of their size.".to_owned(),
        Made::Multiple { normal, small: 1 } => format!(
            "Superscripts and subscripts come from this font at its own size, a smaller size than its other characters, which are drawn at {normal} times its size."
        ),
        Made::Multiple { normal, small } => format!(
            "Superscripts and subscripts come from this font at {small} times its size, a smaller size than its other characters, which are drawn at {normal} times its size."
        ),
        Made::Height { height, multiple: 1 } => format!(
            "Superscripts and subscripts come from this font's {height}-pixel size, a smaller size in the same font file."
        ),
        Made::Height { height, multiple } => format!(
            "Superscripts and subscripts come from this font's {height}-pixel size, a smaller size in the same font file, drawn at {multiple} times that size."
        ),
        Made::Named { spelling, height, multiple: 1 } => format!(
            "Superscripts and subscripts come from {} at {height} pixels, a smaller size of this font that the font set names.",
            font_name(spelling)
        ),
        Made::Named { spelling, height, multiple } => format!(
            "Superscripts and subscripts come from {} at {height} pixels, a smaller size of this font that the font set names, drawn at {multiple} times that size.",
            font_name(spelling)
        ),
        Made::Scaled => "Superscripts and subscripts are this font's own characters, scaled to two thirds of their size. Each pixel stays fully on or fully off, so the edges stay sharp.".to_owned(),
    }
}

/// The name a person knows a font set's `script-source=` font by: a kbd
/// font's name, or the file name of a font named by its path.
fn font_name(spelling: &str) -> &str {
    let named = spelling.split_once(':').map_or(spelling, |(_, rest)| rest);
    named
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .unwrap_or(named)
}

/// The line in place of a picture, where this terminal shows none: the
/// font helper's summary of the font at the size, once it has come.
pub fn no_pictures(summary: Option<&Result<Summary, String>>) -> Message {
    let head = "This terminal shows no pictures";
    match summary {
        None => line(format!("{head}.")),
        Some(Err(why)) => line(format!("{head}, and this size cannot be used: {why}")),
        Some(Ok(s)) => {
            let grid = s
                .grid
                .map(|(c, r)| format!(", {c}×{r} characters"))
                .unwrap_or_default();
            let script = s
                .script
                .as_ref()
                .map(|made| format!(" {}", script_caption(made)))
                .unwrap_or_default();
            with_more(
                format!(
                    "{head}: {}×{} px character cells{grid}; the font can display {} of the {} characters.",
                    s.cell.width, s.cell.height, s.covered, s.of
                ),
                format!(
                    "{head}, so here is a written description instead. With this font and size, each character cell is {}×{} pixels{grid}, and the font can display {} of the {} characters in the preview.{script}",
                    s.cell.width, s.cell.height, s.covered, s.of
                ),
            )
        }
    }
}

/// The message where measuring a font or drawing its picture failed.
pub fn no_picture(why: impl Display) -> String {
    format!("No picture of the console: {why}")
}

/// The line beside the console's own preview of the font set that the
/// console draws now, `font` where it said which.
pub fn console_font_caption(font: Option<&str>) -> Message {
    let font = match font {
        Some(f) => format!("{f}, the font it uses now"),
        None => "the font it uses now".to_owned(),
    };
    line(format!(
        "The console draws this in {font}; [t] tries the highlighted font set."
    ))
}

/// The line beside the console's own preview under a list of sizes.
pub const CONSOLE_SIZE_CAPTION: &str =
    "The console draws this in its current font; a size takes effect when vtium starts again.";

/// The line beside the console's own preview of the font set `name`,
/// which only the document adds. The console reads only installed font
/// sets, so `t` can try it once the document is saved. Under a list of
/// sizes, `sized`, the full text also says when a size takes effect.
pub fn console_caption_in_document(name: &str, sized: bool) -> Message {
    let size = if sized {
        " A size takes effect when vtium starts again."
    } else {
        ""
    };
    with_more(
        format!("The font set {name} is not saved yet; [t] can try it after [Ctrl+S] Save."),
        format!(
            "The console draws this in its current font. The font set {name} is in the document and not saved yet, so [t] can try it once [Ctrl+S] Save installs it.{size}"
        ),
    )
}

/// The Effects tab's line where this terminal shows no pictures.
pub const EFFECT_NO_PICTURES: &str =
    "This terminal shows no pictures, so vtium-config cannot show effects here.";

/// The Effects tab's line where no font can be read.
pub const EFFECT_NO_FONT: &str =
    "vtium-config cannot draw the preview, because none of the fonts can be read.";

/// The Effects tab's line where the picture has no room.
pub const EFFECT_NO_ROOM: &str = "The terminal is too small to show the preview.";

/// The Effects tab's line where the picture failed; the message says why.
pub const EFFECT_NOT_DRAWN: &str = "vtium-config cannot draw the preview of this effect.";

/// The line beside the picture of the filter line or of No filter.
pub const EFFECT_PLAIN: &str = "The picture shows the sample screen with no filter.";

/// The line beside the picture through the vtlook `name`.
pub fn effect_vtlook(name: &str) -> String {
    format!("The picture shows the sample screen through the vtlook {name}.")
}

/// The line beside the picture through the color map `name`.
pub fn effect_map(name: &str) -> String {
    format!("The picture shows the sample screen through the color map {name}.")
}

/// The line beside the picture through the vtlook `name`, whose spatial
/// filters the picture leaves out.
pub fn effect_color_only(name: &str) -> Message {
    with_more(
        format!("The picture shows the vtlook {name} without its spatial filters."),
        format!(
            "The picture shows the sample screen through the vtlook {name}. The picture leaves out the vtlook's spatial filters, which vtium draws only on the GPU."
        ),
    )
}

/// What the full text adds in a vtium console that draws its own vtlook
/// `name`, which the console applies to the picture as well.
pub fn effect_console_vtlook(name: &str) -> String {
    format!("This console draws the picture through its own vtlook {name} as well.")
}

/// Why the picture leaves out a shader.
pub const EFFECT_SHADER: &str = "The picture leaves out this shader: a shader runs only on the GPU, and vtium-config draws the picture on the CPU.";

/// Why vtium cannot use a color map, in the words of its marks.
pub const MAP_OWN: &str = "is in your own folder";
pub const MAP_INACTIVE: &str = "is not active";
pub const MAP_UNTRUSTED: &str = "fails the trust rule";
pub const MAP_MISSING: &str = "is not installed";

/// Why the picture leaves out the highlighted color map, which `why`.
pub fn map_unusable(why: &str) -> String {
    if why == MAP_OWN {
        format!("The picture leaves out this color map: it {why}, so vtium cannot use it.")
    } else {
        format!("The picture leaves out this color map: it {why}, so vtium treats it as missing.")
    }
}

/// Why the picture leaves out a vtlook that does not read, with the
/// reader's first error.
pub fn effect_vtlook_broken(error: &str) -> Message {
    with_more(
        "The picture leaves out this vtlook: it has an error, so vtium draws the built-in vtlook."
            .to_owned(),
        format!(
            "The vtlook has an error, so vtium renders the built-in vtlook in its place. {error}"
        ),
    )
}

/// Why the picture leaves out a vtlook that names the color map `map`,
/// which `why`.
pub fn vtlook_map_unusable(map: &str, why: &str) -> String {
    format!(
        "The picture leaves out this vtlook: its color map {map} {why}, so vtium draws the built-in vtlook."
    )
}

/// The line beside a picture of an effect of kind `what` that vtium could
/// not read, with the reader's first error, its file and line first.
pub fn effect_failed(what: &str, error: &str) -> Message {
    let error = error.trim_end_matches('.');
    let instead = if what == "vtlook" {
        " vtium renders the built-in vtlook in its place."
    } else {
        ""
    };
    with_more(
        format!("The picture leaves out this {what}, because vtium cannot read it."),
        format!(
            "The picture shows the sample screen without this {what}, because vtium cannot read it: {error}.{instead}"
        ),
    )
}
