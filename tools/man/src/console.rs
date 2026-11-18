//! vtium(8): the synopsis and each subcommand's forms and help from the
//! console's usage table, and the options that only the command line
//! takes from the key table.

use vtium::notice::usage::{COMMANDS, SYNOPSIS};
use vtium_settings::keys::{KEYS, Kind, Scope};

use crate::Stamp;
use crate::roff::{Writer, synopsis};

const TEMPLATE: &str = include_str!("../templates/vtium.8.in");

pub fn page(stamp: &Stamp) -> String {
    let mut w = Writer::new(
        "vtium",
        8,
        &stamp.date,
        &stamp.source,
        "System Administration",
    );
    w.template(TEMPLATE, &mut |name, w| {
        match name {
            "SYNOPSIS" => {
                let forms: Vec<String> = std::iter::once(SYNOPSIS.to_owned())
                    .chain(
                        COMMANDS
                            .iter()
                            .flat_map(|c| c.forms)
                            .map(|f| format!("vtium {}", f.synopsis)),
                    )
                    .collect();
                w.synopsis(forms.iter().map(String::as_str));
            }
            "OPTIONS" => options(w),
            "COMMANDS" => commands(w),
            _ => return false,
        }
        true
    });
    w.finish()
}

/// The options that only the command line takes. The others are
/// vtium.conf's settings, which vtium.conf(5) lists.
fn options(w: &mut Writer) {
    for k in KEYS.iter().filter(|k| k.scope == Scope::CommandLine) {
        let long = match k.kind {
            Kind::Switch => format!("--{}", k.name),
            _ => format!("--{}=VALUE", k.name),
        };
        let tag = match k.short {
            Some(c) => format!("{}, {}", synopsis(&format!("-{c}")), synopsis(&long)),
            None => synopsis(&long),
        };
        w.item(&tag, k.paragraphs());
    }
}

/// Each subcommand: its forms, then its help without its `Usage:` line.
fn commands(w: &mut Writer) {
    for command in &COMMANDS {
        w.subsection(&format!("vtium {}", command.word));
        let forms: Vec<String> = command
            .forms
            .iter()
            .map(|f| format!("vtium {}", f.synopsis))
            .collect();
        w.synopsis(forms.iter().map(String::as_str));
        for paragraph in help_paragraphs((command.help)()) {
            w.paragraph(&paragraph);
        }
    }
}

/// The paragraphs of a usage text: its `Usage:` line left out, its
/// paragraphs split at blank lines, and each paragraph's lines joined.
fn help_paragraphs(text: &str) -> Vec<String> {
    let body = match text.split_once('\n') {
        Some((first, rest)) if first.starts_with("Usage:") => rest,
        _ => text,
    };
    body.split("\n\n")
        .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|p| !p.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_usage_text_loses_its_usage_line_and_keeps_its_paragraphs() {
        assert_eq!(
            help_paragraphs("Usage: vtium x N\n\nOne line\nwrapped.\n\nTwo.\n"),
            ["One line wrapped.", "Two."]
        );
        assert_eq!(
            help_paragraphs("Usage: vtium blank --vt N\nTurns off.\n"),
            ["Turns off."]
        );
    }
}
