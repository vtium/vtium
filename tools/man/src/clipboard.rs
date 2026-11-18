//! vtium-clipboard(1): the synopsis, the description and the exit
//! statuses from the help that `vtium-clipboard --help` prints.

use std::ffi::OsString;

use crate::Stamp;
use crate::roff::Writer;

const TEMPLATE: &str = include_str!("../templates/vtium-clipboard.1.in");

/// The program's help, as it prints it under its full name.
fn help() -> String {
    let argv: Vec<OsString> = vec!["vtium-clipboard".into(), "--help".into()];
    let mut out = Vec::new();
    let mut err = Vec::new();
    vtium_clipboard::run(&argv, std::io::empty(), &mut out, &mut err);
    String::from_utf8_lossy(&out).into_owned()
}

/// The help's parts: its usage lines, its prose paragraphs, and its exit
/// statuses, each a status and what it means.
struct Help {
    usage: Vec<String>,
    paragraphs: Vec<String>,
    statuses: Vec<(String, String)>,
}

fn parse(text: &str) -> Help {
    let mut help = Help {
        usage: Vec::new(),
        paragraphs: Vec::new(),
        statuses: Vec::new(),
    };
    for block in text.split("\n\n") {
        if let Some(usage) = block.strip_prefix("Usage: ") {
            help.usage = usage.lines().map(|l| l.trim().to_owned()).collect();
        } else if let Some(list) = block.strip_prefix("Exit status:\n") {
            for line in list.lines() {
                let line = line.trim();
                match line.split_once("  ") {
                    Some((code, means)) if code.bytes().all(|b| b.is_ascii_digit()) => {
                        help.statuses
                            .push((code.to_owned(), means.trim().to_owned()));
                    }
                    _ => {
                        if let Some((_, means)) = help.statuses.last_mut() {
                            means.push(' ');
                            means.push_str(line);
                        }
                    }
                }
            }
        } else {
            let paragraph = block.split_whitespace().collect::<Vec<_>>().join(" ");
            if !paragraph.is_empty() {
                help.paragraphs.push(paragraph);
            }
        }
    }
    help
}

pub fn page(stamp: &Stamp) -> String {
    let help = parse(&help());
    let mut w = Writer::new(
        "vtium-clipboard",
        1,
        &stamp.date,
        &stamp.source,
        "User Commands",
    );
    w.template(TEMPLATE, &mut |name, w| {
        match name {
            "SYNOPSIS" => w.synopsis(help.usage.iter().map(String::as_str)),
            "DESCRIPTION" => {
                for p in &help.paragraphs {
                    match p.strip_prefix("For example: ") {
                        Some(example) => {
                            w.paragraph("For example:");
                            w.example([example]);
                        }
                        None => w.paragraph(p),
                    }
                }
            }
            "EXIT STATUS" => {
                for (code, means) in &help.statuses {
                    w.item(&format!("\\fB{code}\\fR"), [means.as_str()]);
                }
            }
            _ => return false,
        }
        true
    });
    w.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_help_has_usage_paragraphs_and_statuses() {
        let help = parse(&help());
        assert_eq!(
            help.usage.first().map(String::as_str),
            Some("vtium-clipboard copy [FILE]")
        );
        assert!(help.paragraphs.len() > 3);
        let codes: Vec<&str> = help.statuses.iter().map(|(c, _)| c.as_str()).collect();
        assert_eq!(codes, ["0", "1", "2", "3"]);
        assert!(
            help.statuses[3].1.ends_with("so a copy there exits 3 too."),
            "{}",
            help.statuses[3].1
        );
    }
}
