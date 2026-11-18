//! vtium.conf(5): the file's format from the template, every key from the
//! key table, and the sections from the text that vtium compiles in as its
//! built-in sections.

use vtium_settings::keys::{
    Default, FRIT_CELL_MAX, FRIT_CELL_MIN, KEYS, Key, Kind, Level, SB_MEMORY, SECTIONS, Scope,
    TEXT_MAX,
};

use crate::Stamp;
use crate::roff::{Writer, inline};

const TEMPLATE: &str = include_str!("../templates/vtium.conf.5.in");

pub fn page(stamp: &Stamp) -> String {
    let mut w = Writer::new("vtium.conf", 5, &stamp.date, &stamp.source, "File Formats");
    w.template(TEMPLATE, &mut |name, w| match name {
        "SETTINGS" => {
            settings(w);
            true
        }
        "SECTIONS" => {
            sections(w, vtium_settings::TEMPLATE);
            true
        }
        _ => false,
    });
    w.finish()
}

/// What a key holds.
fn kind(k: &Key) -> String {
    match k.kind {
        Kind::Switch => "a switch, on or off".to_owned(),
        Kind::Number { unit, word, .. } => format!("a number in {unit}{}", or(word)),
        Kind::Text => "text".to_owned(),
        Kind::Path => "an absolute path".to_owned(),
        Kind::Paths => "absolute paths, separated by `:`".to_owned(),
        Kind::Choice(c) => format!("one of `{}`", c.join("`, `")),
        Kind::Command => "a command and its arguments".to_owned(),
        Kind::Rgb => "a color, `r,g,b`".to_owned(),
        Kind::Mode => "a display mode, one to a line".to_owned(),
        Kind::KeyBinding => {
            "key combinations, such as `<Ctrl><Alt>Delete`, separated by commas".to_owned()
        }
        Kind::DecoColor => "a color: `fg`, a palette number or `#rrggbb`".to_owned(),
        Kind::Char => "one character".to_owned(),
        Kind::Size { .. } => "a size, `WxH`".to_owned(),
        Kind::Cell => "a character cell in pixels, `WIDTHxHEIGHT`".to_owned(),
    }
}

/// A number key's word, after the number.
fn or(word: Option<&str>) -> String {
    word.map(|w| format!(", or `{w}`")).unwrap_or_default()
}

/// The values a key takes, or `None` where the kind says it all.
fn bound(k: &Key) -> Option<String> {
    Some(match k.kind {
        Kind::Switch | Kind::Choice(_) => return None,
        Kind::Number { min, max, .. } => {
            format!("{min} to {max}; vtium clamps a number outside that range")
        }
        Kind::Text => format!("at most {TEXT_MAX} bytes, with no control character"),
        Kind::Path => format!("at most {TEXT_MAX} bytes, with no control character"),
        Kind::Paths => format!("at most {TEXT_MAX} bytes in all, with no control character"),
        Kind::Command => format!("at most {TEXT_MAX} bytes in a file"),
        Kind::Rgb => "each part 0 to 255".to_owned(),
        Kind::Mode => "a name of at most 63 bytes, and each side 1 to 65535".to_owned(),
        Kind::KeyBinding => {
            "modifiers Shift, Lock, Control, Alt and Logo, and keys as xkb names them".to_owned()
        }
        Kind::DecoColor => "a palette number from 0 to 255, or six hex digits".to_owned(),
        Kind::Char => "no control character or space".to_owned(),
        Kind::Size { max } => format!("each side 1 to {max}"),
        Kind::Cell => format!("each side {FRIT_CELL_MIN} to {FRIT_CELL_MAX}, with no leading zero"),
    })
}

/// A key's default as the source tree sets it, which is what a build gets
/// unless the package's build names another default.
fn default(k: &Key) -> String {
    if k.name == SB_MEMORY.name {
        return format!(
            "{} without image support, {} with it",
            vtium_term::SB_MEMORY_DEFAULT_MINSPEC,
            vtium_term::SB_MEMORY_DEFAULT_IMAGES
        );
    }
    match k.tree_default() {
        Default::Switch(true) => "on".to_owned(),
        Default::Switch(false) => "off".to_owned(),
        Default::Number(n) => n.to_string(),
        Default::Text("") => "empty".to_owned(),
        Default::Text(t) => format!("`{t}`"),
        Default::Command(c) => format!("`{}`", c.join(" ")),
        Default::Rgb([r, g, b]) => format!("`{r},{g},{b}`"),
        Default::Unset => "unset".to_owned(),
    }
}

/// Where in the file a key may be set, and its command-line option.
fn place(k: &Key) -> String {
    let mut places: Vec<&str> = Vec::new();
    for level in k.levels.iter() {
        places.push(match level {
            Level::Top => "the top level",
            Level::Look => "a `[look NAME]` section",
            Level::Head => "a `[head NAME]` section",
        });
    }
    let places = match places.as_slice() {
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
        [] => "no section".to_owned(),
    };
    let option = match (k.kind, k.scope) {
        (_, Scope::Section) => return format!("Set it in {places}."),
        (Kind::Switch, _) => format!("`--{0}` and `--no-{0}`", k.name),
        (Kind::Command, _) => format!("`--{} -- COMMAND...`", k.name),
        _ => format!("`--{}=VALUE`", k.name),
    };
    format!("Set it in {places}, or on vtium's command line with {option}.")
}

/// Every key the file takes, by the key table's sections and groups.
fn settings(w: &mut Writer) {
    for section in &SECTIONS {
        let keys: Vec<&&Key> = section
            .groups
            .iter()
            .flat_map(|g| g.keys)
            .filter(|k| k.scope != Scope::CommandLine)
            .collect();
        if keys.is_empty() {
            continue;
        }
        w.subsection(section.title);
        w.paragraph(section.about);
        for k in keys {
            let mut facts = format!("It takes {}", kind(k));
            if let Some(bound) = bound(k) {
                facts.push_str(&format!(": {bound}"));
            }
            facts.push_str(&format!(". Default: {}.", default(k)));
            let paragraphs: Vec<String> = k
                .paragraphs()
                .map(str::to_owned)
                .chain([facts, place(k)])
                .collect();
            w.item(
                &format!("\\fB{}\\fR", inline(k.name)),
                paragraphs.iter().map(String::as_str),
            );
        }
    }
    // Every key the file takes is in a section, so none is left out.
    let shown = SECTIONS
        .iter()
        .flat_map(|s| s.groups)
        .flat_map(|g| g.keys)
        .filter(|k| k.scope != Scope::CommandLine)
        .count();
    let all = KEYS
        .iter()
        .filter(|k| k.scope != Scope::CommandLine)
        .count();
    assert_eq!(
        shown, all,
        "a key of vtium.conf is in no section of the key table"
    );
}

/// The sections, from the built-in sections' text: its comments become
/// paragraphs, and its settings, rules and indented lines become examples.
fn sections(w: &mut Writer, template: &str) {
    let mut prose: Vec<&str> = Vec::new();
    let mut example: Vec<&str> = Vec::new();
    let flush_prose = |prose: &mut Vec<&str>, w: &mut Writer| {
        if !prose.is_empty() {
            w.paragraph(&prose.join(" "));
            prose.clear();
        }
    };
    let flush_example = |example: &mut Vec<&str>, w: &mut Writer| {
        if !example.is_empty() {
            w.example(example.iter().copied());
            example.clear();
        }
    };
    for line in template.lines() {
        let comment = line
            .strip_prefix('#')
            .map(|l| l.strip_prefix(' ').unwrap_or(l));
        match comment {
            Some("") => {
                flush_prose(&mut prose, w);
                flush_example(&mut example, w);
            }
            Some(text) if !text.starts_with("  ") && !is_setting(text) => {
                flush_example(&mut example, w);
                prose.push(text);
            }
            Some(text) => {
                flush_prose(&mut prose, w);
                // an indented line keeps its own indent past the comment's
                example.push(if is_setting(text) { line } else { &text[2..] });
            }
            None if line.trim().is_empty() => {
                flush_prose(&mut prose, w);
                flush_example(&mut example, w);
            }
            None => {
                flush_prose(&mut prose, w);
                example.push(line);
            }
        }
    }
    flush_prose(&mut prose, w);
    flush_example(&mut example, w);
}

/// Whether a comment's text is a setting written out, as `look = default`.
fn is_setting(text: &str) -> bool {
    text.split_once(" = ").is_some_and(|(key, _)| {
        !key.is_empty() && key.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sections_text_becomes_paragraphs_and_examples() {
        let mut w = Writer::new("x", 5, "2026-11-17", "vtium 1", "File Formats");
        sections(
            &mut w,
            "# A head is a screen,\n# with consoles.\n#\n# look = default\n#   [look big]\n[head console]\n# Next.\n",
        );
        let page = w.finish();
        let body = page
            .split_once("Formats\"\n")
            .map(|(_, b)| b)
            .unwrap_or_default();
        assert_eq!(
            body,
            "A head is a screen, with consoles.\n.PP\n.RS 4\n.nf\n# look = default\n[look big]\n[head console]\n.fi\n.RE\n.PP\nNext.\n"
        );
    }
}
