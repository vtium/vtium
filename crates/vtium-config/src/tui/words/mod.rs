//! The TUI's words, in one place so they can be reviewed and reworded
//! together (ADR-1201 D3). They are English, as ADR-1402 keeps what
//! vtium's tools print in a terminal. A choice between wordings is two
//! entries, never one sentence built from pieces.
//!
//! The words every tab shares, such as the status line, the live trial
//! and the questions, are in this file. Each tab's own words are in a file
//! of the tab's name, and this module re-exports them, so every caller
//! names a word as `words::NAME`.

mod effects;
mod flow;
mod fonts;
mod help;
mod looks;
mod resources;
mod settings;

pub use effects::*;
pub use flow::*;
pub use fonts::*;
pub use help::*;
pub use looks::*;
pub use resources::*;
pub use settings::*;

use std::fmt::Display;

/// The actions the tab bar shows beside every tab but Flow, which has its
/// own, and Help, which shows none: Save and Install, each in its long and
/// short form.
///
/// References:
/// - ADR-1403 D7, "What carries over": the actions in the tab bar.
pub const ACTIONS: [(&str, &str); 2] = [("Save", "Save"), ("Install", "Install")];

/// How the TUI previews a change, and whether the live trial works, at
/// the right of the status line (ADR-1201 D4).
pub const PREVIEW_ASKING: &str = "checking this terminal";

pub const PREVIEW_PICTURE: &str = "image preview";

pub const PREVIEW_TEXT: &str = "text preview";

/// The short forms, for a status line whose left side does not fit beside
/// the long ones.
pub const PREVIEW_PICTURE_SHORT: &str = "image";

pub const PREVIEW_TEXT_SHORT: &str = "text";

pub const TRIAL_KEY: &str = "t: try live";

/// The status line's word for `t` on a font set that only the draft adds,
/// which the console can try once the draft is installed.
pub const TRIAL_AFTER_INSTALL: &str = "t: after install";

pub const TRIAL_NONE: &str = "no live trial";

/// The status line's trial words on a tab other than the Looks tab, which
/// is where `t` tries a font set.
pub const TRIAL_IN_LOOKS: &str = "live trial in Looks";

/// The status line's trial words in the Looks tab where `t` does nothing:
/// among the color schemes and the pointer themes, and in a detail.
pub const TRIAL_ON_FONT_SETS: &str = "live trial on font sets";

/// Why the live trial does not work, on the message line.
pub const OPS_OFF: &str = "Live trial is off: this vtium console has allow-font-ops turned off, so t cannot try a font on it. To turn it on, change allow-font-ops in the Settings tab, then install.";

pub const NOT_VTIUM: &str = "Live trial is off: this terminal is not a vtium console, so t cannot try a font on it. You can still install fonts.";

/// The live trial (ADR-1201 D4).
pub const TRIAL_OFF: &str = "The live trial works only in a vtium console with allow-font-ops on.";

pub fn trial_countdown(font: impl Display, left: u32) -> String {
    format!(
        "Trying {font} on the console for {left} s. Press k to keep it, or any other key to put the font back."
    )
}

pub fn trial_kept(font: impl Display) -> String {
    format!(
        "The console keeps {font} until you quit vtium-config. To keep {font} after that, choose it on a look's card in the Looks tab, then press s to save and i to install."
    )
}

pub fn trial_back(font: impl Display) -> String {
    format!("The console is back to {font}.")
}

pub fn trial_same(font: impl Display) -> String {
    format!("The console already uses {font}.")
}

pub fn trial_refused(font: impl Display) -> String {
    format!("The console refused {font}. Its log says why.")
}

/// The line beside the detailed specimen (ADR-1201 D4).
pub fn detail_caption(
    font: impl Display,
    points: u32,
    summary: Option<&crate::specimen::Summary>,
    (w, h): (u32, u32),
    assumed: bool,
) -> String {
    let marks = "Characters in grey come from the built-in font, and a box marks a character no font can display. Press p to see the console.";
    let Some(s) = summary else {
        return format!("{font} at {points} pt. {marks}");
    };
    let display = if assumed {
        format!("on a {w}x{h} display (assumed)")
    } else {
        format!("on this {w}x{h} display")
    };
    let grid = s
        .grid
        .map(|(c, r)| format!(", and the console will be {c}x{r} characters {display}"))
        .unwrap_or_default();
    let script = s
        .script
        .as_ref()
        .map(|made| format!(" {}", script_caption(made)))
        .unwrap_or_default();
    format!(
        "{font} at {points} pt: each character cell is {}x{} pixels{grid}. The font can display {} of the {} characters in the preview.{script} {marks}",
        s.cell.width, s.cell.height, s.covered, s.of
    )
}

/// How the font makes the superscripts and subscripts of the detailed
/// specimen's last line, in a sentence or two (ADR-1202 D14): the way the
/// glyph helper made them, `made`, in the reader's words.
pub fn script_caption(made: &crate::made::Made) -> String {
    use crate::made::Made;
    match made {
        Made::Outline => "Superscripts and subscripts are drawn from this font's outlines at two thirds of its size.".to_owned(),
        Made::Color => "Superscripts and subscripts in color are this font's color images, scaled smoothly to two thirds of their size.".to_owned(),
        Made::Multiple { normal, small: 1 } => format!(
            "Superscripts and subscripts come from this font at its own size, a smaller size than its other characters, which are drawn at {normal} times its size."
        ),
        Made::Multiple { normal, small } => format!(
            "Superscripts and subscripts come from this font at {small} times its size, a smaller size than its other characters, which are drawn at {normal} times its size."
        ),
        Made::Height {
            height,
            multiple: 1,
        } => format!(
            "Superscripts and subscripts come from this font's {height}-pixel size, a smaller size in the same font file."
        ),
        Made::Height { height, multiple } => format!(
            "Superscripts and subscripts come from this font's {height}-pixel size, a smaller size in the same font file, drawn at {multiple} times that size."
        ),
        Made::Named {
            spelling,
            height,
            multiple: 1,
        } => format!(
            "Superscripts and subscripts come from {} at {height} pixels, a smaller size of this font that the font set names.",
            font_name(spelling)
        ),
        Made::Named {
            spelling,
            height,
            multiple,
        } => format!(
            "Superscripts and subscripts come from {} at {height} pixels, a smaller size of this font that the font set names, drawn at {multiple} times that size.",
            font_name(spelling)
        ),
        Made::Scaled => "Superscripts and subscripts are this font's own characters, scaled to two thirds of their size. Each pixel stays fully on or fully off, so the edges stay sharp.".to_owned(),
    }
}

/// The name a person knows a font set's `script-source=` font by: a kbd
/// font's name, or the file name of a font named by its path.
pub(super) fn font_name(spelling: &str) -> &str {
    let named = spelling.split_once(':').map_or(spelling, |(_, rest)| rest);
    named
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .unwrap_or(named)
}

/// The line beside a picture of the console (ADR-1201 D4).
pub fn caption(c: &super::shot::Caption) -> String {
    let (w, h) = c.display;
    let display = if c.assumed {
        format!("a {w}x{h} display (assumed, because no display was found)")
    } else {
        format!("this {w}x{h} display")
    };
    format!(
        "{} at {} pt on {display}: each character cell is {}x{} pixels, and the console will be {} characters wide and {} lines high.",
        c.font, c.points, c.cell.width, c.cell.height, c.grid.0, c.grid.1
    )
}

pub const KEPT: &str = "not used: the console kept its font (see its log)";

/// Marks the choice that an install writes.
pub const TO_SAVE: &str = "to install";

/// Marks what vtium.conf names now.
pub const SAVED: &str = "installed";

pub fn remove_unread(name: impl Display) -> String {
    format!("{name}.vtfrit could not be read when vtium-config started")
}

/// A size's line when the terminal draws no pictures.
pub const NO_PICTURES: &str =
    "This terminal does not support images, so here is a written description instead.";

pub fn no_picture(why: impl Display) -> String {
    format!("No preview image of the console: {why}")
}

pub fn size_summary(s: &crate::specimen::Summary) -> String {
    let grid = s
        .grid
        .map(|(c, r)| format!(" the console will be {c}x{r} characters, and"))
        .unwrap_or_else(|| " and".to_owned());
    let script = s
        .script
        .as_ref()
        .map(|made| format!(" {}", script_caption(made)))
        .unwrap_or_default();
    format!(
        "With this font and size, each character cell is {}x{} pixels,{grid} the font can display {} of the {} characters in the preview.{script}",
        s.cell.width, s.cell.height, s.covered, s.of
    )
}

pub fn no_summary(why: impl Display) -> String {
    format!("This size cannot be used: {why}")
}

/// The install view, which installs the changes made in the views.
pub fn saved(files: usize) -> String {
    if files == 1 {
        "Installed 1 file.".to_owned()
    } else {
        format!("Installed {files} files.")
    }
}

/// The screen below the TUI's minimum size (ADR-1201 D3).
pub fn too_small(cols: usize, rows: usize) -> String {
    format!(
        "This terminal is too small: vtium-config needs at least 80x25 characters, and this terminal is {cols}x{rows}."
    )
}

pub const TOO_SMALL_COMMANDS: &str =
    "Make this terminal larger, or use these commands, which work at any size:";

/// What `u` does below the minimum size, under the generated key line,
/// while the key table's row for `u` is still planned and the key line
/// leaves `u` out.
pub const TOO_SMALL_UNDO: &str = "Press u to put back the font the console started with.";

/// The status line (ADR-1403 D7).
pub const NO_DRAFT: &str = "no draft";

pub fn draft_state(name: impl Display, changes: usize, unsaved: usize, problems: usize) -> String {
    let mut changes = match changes {
        0 => "no changes".to_owned(),
        1 => "1 change".to_owned(),
        n => format!("{n} changes"),
    };
    if unsaved > 0 {
        changes = format!("{changes} · {unsaved} not saved");
    }
    let problems = match problems {
        0 => "no problems".to_owned(),
        1 => "1 problem".to_owned(),
        n => format!("{n} problems"),
    };
    format!("draft {name} · {changes} · {problems}")
}

/// What `s` says once the unsaved changes are in the draft.
pub fn saved_draft(n: usize, name: &str) -> String {
    match n {
        1 => format!("Saved 1 change into draft {name}."),
        n => format!("Saved {n} changes into draft {name}."),
    }
}

pub const NOTHING_TO_SAVE: &str = "Nothing to save: the draft already holds every change.";

/// The question `q` asks while changes are not saved (ADR-1403 D7).
pub fn quit_question(n: usize) -> String {
    let changes = match n {
        1 => "1 change is".to_owned(),
        n => format!("{n} changes are"),
    };
    format!(
        "{changes} not saved. Save them before you quit? y saves them and quits, n quits without them, and Esc goes back."
    )
}

/// A question that throws the draft away, with the unsaved changes it
/// also throws away.
pub fn losing_unsaved(question: String, n: usize) -> String {
    match n {
        0 => question,
        1 => format!("{question} This includes 1 change that is not saved."),
        n => format!("{question} This includes {n} changes that are not saved."),
    }
}

/// What leaving shows while the console's font comes back.
pub const LEAVING: &str = "Restoring the console's font…";

/// Why vtium.conf cannot take an edit.
pub fn conf_edit(e: &vtium_settings::ConfEdit) -> String {
    match e {
        vtium_settings::ConfEdit::BadKey(key) => format!(
            "There is no setting named {key:?}. A setting's name uses lowercase letters, digits and -."
        ),
        vtium_settings::ConfEdit::BadValue { key, value } => format!(
            "{key} cannot be set to {value:?}: vtium would read it back as a different value."
        ),
    }
}

pub const NO_DESK: &str = "This is a sample setup with no draft, so changes here are not saved.";

pub const NOT_DONE: &str = "Nothing changed.";

/// `a`, `a and b`, `a, b and c`.
fn list(items: impl Iterator<Item = String>) -> String {
    let items: Vec<String> = items.collect();
    match items.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        Some((last, _)) => last.clone(),
        None => String::new(),
    }
}

pub const INSTALLING: &str = "Installing. sudo may ask for your password.";

pub const NOTHING_TO_INSTALL: &str =
    "Nothing to install: the draft is the same as the installed vtium.conf.";

pub fn install_question(added: usize, removed: usize, frits: usize) -> String {
    match frits {
        0 => format!("Install the draft? {}.", lines(added, removed)),
        1 => format!(
            "Install the draft? {}, and 1 font set.",
            lines(added, removed)
        ),
        n => format!(
            "Install the draft? {}, and {n} font sets.",
            lines(added, removed)
        ),
    }
}

/// The first half of Install's question for a draft that wires no
/// complete route, and of the refusal without a terminal (ADR-1403 D7,
/// D8).
pub const NO_ROUTE: &str = "This draft has no complete path: no connected display is sent to a head that a connected keyboard reaches.";

/// What `vtium config install` says, with no terminal to ask and no
/// `--force`, of a draft that wires no complete route (ADR-1403 D8).
pub fn no_route_force() -> String {
    format!("{NO_ROUTE} To install it anyway, run vtium config install --force.")
}

/// What `vtium config install` says when the person answers no.
pub const NO_ROUTE_NOT_INSTALLED: &str =
    "Nothing installed: you answered no. The draft keeps your changes.";

/// Install's question for a draft whose only problem is that it wires no
/// complete route (ADR-1403 D7).
pub fn no_route_question(added: usize, removed: usize, frits: usize) -> String {
    let install = install_question(added, removed, frits);
    let counted = install
        .strip_prefix("Install the draft? ")
        .unwrap_or(&install);
    format!(
        "{NO_ROUTE} After the install, nobody at this machine may be able to type into vtium. Install it anyway? {counted}"
    )
}

pub const NOT_PREPARED: &str = "The install is not ready yet. Press i again.";

pub fn install_failed(why: &str) -> String {
    format!("Not installed: {why}")
}

pub fn installed(name: &str) -> String {
    format!("Installed draft {name}. The console uses it after vtium restarts.")
}

/// `3 lines added, 1 removed`.
fn lines(added: usize, removed: usize) -> String {
    crate::drafts::lines(added, removed)
}

pub fn came_went(came: &[String], went: &[String]) -> String {
    let came = (!came.is_empty()).then(|| format!("connected: {}", list(came.iter().cloned())));
    let went = (!went.is_empty()).then(|| format!("disconnected: {}", list(went.iter().cloned())));
    match (came, went) {
        (Some(c), Some(w)) => format!("{c}; {w}"),
        (Some(one), None) | (None, Some(one)) => one,
        (None, None) => "no device changes".to_owned(),
    }
}

/// The quit question below the minimum size, on one row before its keys:
/// the changes quitting loses.
pub fn small_question(unsaved: usize) -> String {
    match unsaved {
        1 => "1 change is not saved.".to_owned(),
        n => format!("{n} changes are not saved."),
    }
}

pub const NOT_LEFT: &str = "Nothing is lost.";

/// The short form of [`frits_waiting`], for a status line too narrow for
/// the long one.
pub fn frits_waiting_short(n: usize) -> String {
    format!("{n} to install")
}

pub fn frits_waiting(n: usize) -> String {
    if n == 1 {
        "1 font set to install".to_owned()
    } else {
        format!("{n} font sets to install")
    }
}

/// A terminal named in a sentence: `the terminal desk`, or for a name
/// vtium-config gave it, `terminal-1` alone.
pub fn terminal(head: &str) -> String {
    if head.starts_with("terminal-") {
        head.to_owned()
    } else {
        format!("the terminal {head}")
    }
}

#[cfg(test)]
mod tests {
    use crate::made::Made;

    // ADR-1202 D14: the caption under the line of superscripts and
    // subscripts says how the font makes them, each way in words of its
    // own, and names the font a script-source= line gives
    // covers: sgr.script
    #[test]
    fn the_script_caption_names_each_way() {
        let cases = [
            (
                Made::Outline,
                "Superscripts and subscripts are drawn from this font's outlines at two thirds of its size.",
            ),
            (
                Made::Color,
                "Superscripts and subscripts in color are this font's color images, scaled smoothly to two thirds of their size.",
            ),
            (
                Made::Multiple {
                    normal: 2,
                    small: 1,
                },
                "Superscripts and subscripts come from this font at its own size, a smaller size than its other characters, which are drawn at 2 times its size.",
            ),
            (
                Made::Multiple {
                    normal: 4,
                    small: 3,
                },
                "Superscripts and subscripts come from this font at 3 times its size, a smaller size than its other characters, which are drawn at 4 times its size.",
            ),
            (
                Made::Height {
                    height: 8,
                    multiple: 1,
                },
                "Superscripts and subscripts come from this font's 8-pixel size, a smaller size in the same font file.",
            ),
            (
                Made::Height {
                    height: 8,
                    multiple: 2,
                },
                "Superscripts and subscripts come from this font's 8-pixel size, a smaller size in the same font file, drawn at 2 times that size.",
            ),
            (
                Made::Named {
                    spelling: "kbd:ter-112n".to_owned(),
                    height: 12,
                    multiple: 1,
                },
                "Superscripts and subscripts come from ter-112n at 12 pixels, a smaller size of this font that the font set names.",
            ),
            (
                Made::Named {
                    spelling: "file:/usr/share/fonts/small/ter-u12n.psf.gz".to_owned(),
                    height: 12,
                    multiple: 2,
                },
                "Superscripts and subscripts come from ter-u12n.psf.gz at 12 pixels, a smaller size of this font that the font set names, drawn at 2 times that size.",
            ),
            (
                Made::Scaled,
                "Superscripts and subscripts are this font's own characters, scaled to two thirds of their size. Each pixel stays fully on or fully off, so the edges stay sharp.",
            ),
        ];
        for (made, want) in cases {
            assert_eq!(super::script_caption(&made), want, "{made:?}");
        }
    }

    // ADR-1202 D14: the font view's caption beside the detailed specimen,
    // and its written description where the terminal shows no images, say
    // how the font makes its superscripts and subscripts after what it
    // covers; with no superscript the font draws, they say nothing of them
    // covers: sgr.script
    #[test]
    fn the_font_views_captions_say_how_the_font_makes_superscripts() {
        let mut summary = crate::specimen::Summary {
            cell: vtium_font::Cell {
                width: 8,
                height: 16,
            },
            grid: None,
            covered: 40,
            of: 51,
            script: Some(Made::Height {
                height: 8,
                multiple: 1,
            }),
        };
        assert_eq!(
            super::detail_caption("ega", 12, Some(&summary), (1920, 1080), false),
            "ega at 12 pt: each character cell is 8x16 pixels. The font can display 40 of the 51 characters in the preview. Superscripts and subscripts come from this font's 8-pixel size, a smaller size in the same font file. Characters in grey come from the built-in font, and a box marks a character no font can display. Press p to see the console."
        );
        assert_eq!(
            super::size_summary(&summary),
            "With this font and size, each character cell is 8x16 pixels, and the font can display 40 of the 51 characters in the preview. Superscripts and subscripts come from this font's 8-pixel size, a smaller size in the same font file."
        );
        summary.script = None;
        assert!(!super::size_summary(&summary).contains("Superscripts"));
        assert!(
            !super::detail_caption("ega", 12, Some(&summary), (1920, 1080), false)
                .contains("Superscripts")
        );
    }
    // ADR-1403 D7's fixed word: Install, Revert and New each call the
    // working copy the draft
    #[test]
    fn install_revert_and_new_name_the_draft() {
        assert_eq!(
            super::install_question(3, 1, 0),
            "Install the draft? 3 lines added, 1 removed."
        );
        assert_eq!(
            super::revert_question(3, 1),
            "Throw away the draft's changes? 3 lines added and 1 removed will be lost."
        );
        assert_eq!(
            super::NEW_QUESTION,
            "Start a new configuration? The draft's heads, rules and looks will be lost."
        );
    }

    // ADR-1403 D7 and D8 word Install's question and the refusal for a
    // draft that wires no complete route
    #[test]
    fn install_asks_before_it_installs_a_draft_with_no_complete_path() {
        assert_eq!(
            super::no_route_question(3, 1, 0),
            "This draft has no complete path: no connected display is sent to a head that a connected keyboard reaches. After the install, nobody at this machine may be able to type into vtium. Install it anyway? 3 lines added, 1 removed."
        );
        assert_eq!(
            super::no_route_force(),
            "This draft has no complete path: no connected display is sent to a head that a connected keyboard reaches. To install it anyway, run vtium config install --force."
        );
    }
}
