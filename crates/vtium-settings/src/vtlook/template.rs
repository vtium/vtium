//! The built-in vtlook's text, a template generated from the filters'
//! declarations.
//!
//! The template holds `version = 1`, a `[requires]` section with every key
//! at `any`, and, commented out, a section for each filter with each
//! parameter at its default and its help beside it. A file written from
//! it changes no pixel until someone uncomments a filter's section or sets
//! a requirement. vtium-config writes it out to start a new vtlook. The
//! committed file, `templates/default.vtlook`, is compiled in, and a test
//! fails when it differs from what the declarations give.
//!
//! References:
//! - ADR-1304 D4: the template, its generation and its check.
//! - ADR-1304 D14: the template's text.

use vtium_render::filter::{Declaration, Given, Kind};

use super::requires::list;

/// The committed template: the built-in vtlook's text.
pub const TEMPLATE: &str = include_str!("../../templates/default.vtlook");

/// The template's opening, through its `[requires]` section.
const HEAD: &str = "\
# default.vtlook: a pure passthrough vtlook, which changes no pixel.
# vtium's built-in vtlook: a look that names no vtlook draws it. To make a
# vtlook of your own, copy this file under another name, uncomment the
# sections you want and set their values. Within a layer, filters run in the
# order of their sections.
version = 1

# Display requirements. A display that misses one draws the default vtlook;
# the head's other displays keep this vtlook. any is no constraint.
[requires]
min-depth = any     # any, 1, 8, 16 or 24 (32 reads as 24): bits a pixel
max-depth = any
min-size = any      # any, or WxH: the mode's own pixels
max-size = any
min-dpi = any       # any, or 1 to 1000: the density fonts are sized at
max-dpi = any
min-fps = any       # any, refresh or 1 to 1000: the frame rate to keep
gpu = any           # any; yes for the GPU path only; no for the CPU path only
";

/// The paragraph that opens the color filters' sections.
const COLOR: &str = "\
# Color filters run on both paths, merged into one table, after the turn.
";

/// The column a parameter's comment starts at, unless a line is longer.
const COMMENT_COLUMN: usize = 20;

/// The template for the color filters `color`.
pub fn template(color: &[&Declaration]) -> String {
    let mut out = String::from(HEAD);
    if !color.is_empty() {
        out.push('\n');
        out.push_str(COLOR);
    }
    for d in color {
        section(&mut out, d);
    }
    out
}

/// One filter's commented section: its heading, its help, and each
/// parameter at its default with its kind, range and help.
fn section(out: &mut String, d: &Declaration) {
    out.push_str(&format!(
        "\n# [{} {}]\n# {}\n",
        d.layer.word(),
        d.name,
        d.help
    ));
    let lines: Vec<(String, String)> = d
        .params
        .iter()
        .map(|p| {
            let value = p.default.map(written).unwrap_or_default();
            let set = format!("# {} = {value}", p.name);
            (
                set.trim_end().to_owned(),
                format!("{}: {}", kind(p.kind), p.help),
            )
        })
        .collect();
    let width = lines
        .iter()
        .map(|(set, _)| set.chars().count().saturating_add(1))
        .max()
        .unwrap_or(0)
        .max(COMMENT_COLUMN);
    for (set, about) in lines {
        out.push_str(&format!("{set:<width$}# {about}\n"));
    }
}

/// A default as a vtlook writes it. A number keeps a digit after the point,
/// so that it reads as a number.
fn written(g: Given) -> String {
    match g {
        Given::Number(n) => {
            let s = n.to_string();
            if s.contains('.') { s } else { format!("{s}.0") }
        }
        Given::Integer(n) => n.to_string(),
        Given::Color([r, g, b]) => format!("{r},{g},{b}"),
        Given::Choice(w) => w.to_owned(),
        Given::Switch(b) => if b { "yes" } else { "no" }.to_owned(),
    }
}

/// A kind and its range, as the template's comment gives them.
fn kind(k: Kind) -> String {
    match k {
        Kind::Number { min, max } => format!("number, {min} to {max}"),
        Kind::Integer { min, max } => format!("integer, {min} to {max}"),
        Kind::Color => "color, r,g,b".to_owned(),
        Kind::Choice(words) => list(words),
        Kind::Switch => "yes or no".to_owned(),
        Kind::File(suffixes) => format!("file, a {} file, no path", list(suffixes)),
    }
}

#[cfg(test)]
#[expect(clippy::panic, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use crate::vtlook::read_text_with;
    use std::path::Path;
    use vtium_render::filter::{Fixed, Layer, Param};

    /// The color filters the template is generated from: vtium's own.
    fn declared() -> &'static [&'static Declaration] {
        vtium_render::filter::COLOR
    }

    // The committed template is what the declarations give, so a
    // declaration added without its section in the template fails here.
    // `VTIUM_SETTINGS_BLESS=1` rewrites the committed file.
    #[test]
    fn the_committed_template_is_what_the_declarations_give() {
        let generated = template(declared());
        if std::env::var_os("VTIUM_SETTINGS_BLESS").is_some() {
            let path = concat!(env!("CARGO_MANIFEST_DIR"), "/templates/default.vtlook");
            std::fs::write(path, &generated).unwrap_or_default();
            return;
        }
        if TEMPLATE != generated {
            panic!(
                "templates/default.vtlook differs from the declarations; run VTIUM_SETTINGS_BLESS=1 cargo test -p vtium-settings --features filters template\n--- generated\n{generated}"
            );
        }
    }

    // People read the template in a terminal editor, so its lines wrap at
    // 78 columns, as vtium.conf's example does.
    #[test]
    fn the_template_s_lines_fit_78_columns() {
        for line in TEMPLATE.lines() {
            assert!(
                line.chars().count() <= 78,
                "{} columns: {line}",
                line.chars().count()
            );
        }
    }

    // The template reads cleanly as a vtlook, holds every requirement key
    // at any, and holds no filter, so it changes no pixel.
    #[test]
    fn the_template_reads_as_a_passthrough_with_every_requirement_key() {
        let read = read_text_with(
            "default",
            Path::new("default.vtlook"),
            TEMPLATE.as_bytes(),
            declared(),
        );
        let Ok(look) = read else {
            panic!("{read:?}");
        };
        assert!(!look.holds_filters());
        for key in super::super::requires::KEYS {
            assert!(
                TEMPLATE
                    .lines()
                    .any(|l| l.starts_with(&format!("{key} = any"))),
                "{key}"
            );
        }
    }

    // A section for each filter, with each parameter at its default, its
    // kind, range and help; uncommented, the section reads cleanly.
    #[test]
    fn each_filter_has_a_commented_section_with_its_defaults() {
        const SCAN: Declaration = Declaration {
            name: "tint",
            layer: Layer::Color,
            help: "Turns the screen toward one color, as a single-color phosphor.",
            params: &[
                Param {
                    name: "color",
                    kind: Kind::Color,
                    default: Some(Given::Color([255, 176, 0])),
                    help: "the phosphor's color",
                },
                Param {
                    name: "amount",
                    kind: Kind::Number {
                        min: Fixed::of(0, 0),
                        max: Fixed::of(1, 0),
                    },
                    default: Some(Given::Number(Fixed::of(1, 0))),
                    help: "how far toward it",
                },
            ],
        };
        let text = template(&[&SCAN]);
        let tail = text.strip_prefix(HEAD).unwrap_or_default();
        assert_eq!(
            tail,
            "\n# Color filters run on both paths, merged into one table, after the turn.\n\n# [color tint]\n# Turns the screen toward one color, as a single-color phosphor.\n# color = 255,176,0 # color, r,g,b: the phosphor's color\n# amount = 1.0      # number, 0 to 1: how far toward it\n"
        );
        let uncommented: String = text
            .lines()
            .map(|l| {
                l.strip_prefix("# [")
                    .map_or(l.to_owned(), |h| format!("[{h}"))
            })
            .map(|l| {
                l.strip_prefix("# color =")
                    .or_else(|| l.strip_prefix("# amount ="))
                    .map_or(l.clone(), |_| l.trim_start_matches("# ").to_owned())
            })
            .collect::<Vec<_>>()
            .join("\n");
        let read = read_text_with("t", Path::new("t.vtlook"), uncommented.as_bytes(), &[&SCAN]);
        assert!(read.is_ok_and(|l| l.color.len() == 1));
    }
}
