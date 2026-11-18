//! vtium-gallery(1): the synopsis, the description and the options from
//! the usage that the gallery script prints.

use crate::Stamp;
use crate::roff::{Writer, synopsis};

const TEMPLATE: &str = include_str!("../templates/vtium-gallery.1.in");

/// The gallery script, which a build with image support installs as
/// vtium-gallery.
const SCRIPT: &str = include_str!("../../gallery/gallery");

/// The usage's parts: the synopsis, the prose paragraphs, and the
/// options, each its forms and what it does.
struct Usage {
    synopsis: String,
    paragraphs: Vec<String>,
    options: Vec<(String, String)>,
}

/// Reads the script's `usage` function: its `printf 'usage: %s ...'`
/// line and the text of its here-document.
fn parse(script: &str) -> Usage {
    let synopsis = script
        .lines()
        .find_map(|l| l.trim().strip_prefix("printf 'usage: %s "))
        .and_then(|l| l.split_once("\\n'"))
        .map(|(args, _)| format!("vtium-gallery {args}"))
        .unwrap_or_default();
    let body = script
        .split_once("cat <<'EOF'\n")
        .and_then(|(_, rest)| rest.split_once("\nEOF\n"))
        .map(|(body, _)| body)
        .unwrap_or_default();
    let mut usage = Usage {
        synopsis,
        paragraphs: Vec::new(),
        options: Vec::new(),
    };
    for block in body.split("\n\n") {
        if block.trim_start().starts_with('-') {
            for line in block.lines() {
                if line.trim_start().starts_with('-') {
                    let line = line.trim();
                    let (forms, does) = line.split_once("  ").unwrap_or((line, ""));
                    usage
                        .options
                        .push((forms.to_owned(), does.trim().to_owned()));
                } else if let Some((_, does)) = usage.options.last_mut() {
                    does.push(' ');
                    does.push_str(line.trim());
                }
            }
        } else {
            let paragraph = block.split_whitespace().collect::<Vec<_>>().join(" ");
            if !paragraph.is_empty() {
                usage.paragraphs.push(paragraph);
            }
        }
    }
    usage
}

pub fn page(stamp: &Stamp) -> String {
    let usage = parse(SCRIPT);
    let mut w = Writer::new(
        "vtium-gallery",
        1,
        &stamp.date,
        &stamp.source,
        "User Commands",
    );
    w.template(TEMPLATE, &mut |name, w| {
        match name {
            "SYNOPSIS" => w.synopsis([usage.synopsis.as_str()]),
            "DESCRIPTION" => {
                for p in &usage.paragraphs {
                    w.paragraph(p);
                }
            }
            "OPTIONS" => {
                for (forms, does) in &usage.options {
                    let forms: Vec<String> = forms.split(", ").map(synopsis).collect();
                    w.item(&forms.join(", "), [does.as_str()]);
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
    fn the_usage_has_a_synopsis_paragraphs_and_options() {
        let usage = parse(SCRIPT);
        assert!(
            usage.synopsis.starts_with("vtium-gallery [-s N]"),
            "{}",
            usage.synopsis
        );
        assert!(!usage.paragraphs.is_empty());
        let forms: Vec<&str> = usage.options.iter().map(|(f, _)| f.as_str()).collect();
        assert!(forms.contains(&"-s, --size N"), "{forms:?}");
        assert!(forms.contains(&"--no-probe"), "{forms:?}");
        let wait = usage
            .options
            .iter()
            .find(|(f, _)| f.starts_with("-w"))
            .map(|(_, d)| d.as_str());
        assert!(
            wait.is_some_and(|d| d.ends_with("can need longer")),
            "{wait:?}"
        );
    }
}
