//! docs/looks.md's examples, read by the real vtlook reader.
//!
//! Each fenced block whose info string is `vtlook` must read cleanly. Each
//! `vtlook-error` block must give exactly the errors of the `text` block
//! that follows it, as `vtium check-config` prints them. The block that
//! starts with the built-in vtlook's first line must be the template vtium
//! compiles in. So a doc example that stops reading as the page says fails
//! here.
//!
//! References:
//! - ADR-1304 D13: docs/looks.md carries the examples, and these two tests read them.
//! - ADR-1304 D14: the info strings that name what each block holds.
#![cfg(feature = "filters")]

use std::path::PathBuf;

use vtium_settings::vtlook::{self, Report};

/// The page's fenced blocks, each with its info string and its text.
fn blocks() -> Vec<(String, String)> {
    let page = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/looks.md"))
        .unwrap_or_default();
    let mut out = Vec::new();
    let mut open: Option<(String, String)> = None;
    for line in page.lines() {
        match (&mut open, line.strip_prefix("```")) {
            (None, Some(info)) => open = Some((info.trim().to_owned(), String::new())),
            (Some(_), Some("")) => out.extend(open.take()),
            (Some((_, text)), _) => {
                text.push_str(line);
                text.push('\n');
            }
            (None, None) => {}
        }
    }
    out
}

/// The vtlook's name and file, from a block's first line,
/// `# /etc/vtium/looks/NAME.vtlook`, or a stand-in where it names none.
fn named(text: &str) -> (String, PathBuf) {
    let file = text
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("# "))
        .and_then(|l| l.split(':').next())
        .filter(|p| p.ends_with(".vtlook"))
        .unwrap_or("/etc/vtium/looks/example.vtlook");
    let path = PathBuf::from(file);
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    (name, path)
}

fn shown(reports: &[Report]) -> Vec<String> {
    reports.iter().map(|r| format!("{}: {r}", r.at())).collect()
}

#[test]
fn each_example_reads_as_the_page_says() {
    let blocks = blocks();
    let mut clean = 0;
    let mut wrong = 0;
    for (i, (info, text)) in blocks.iter().enumerate() {
        let (name, file) = named(text);
        match info.as_str() {
            "vtlook" => {
                let read = vtlook::read_text(&name, &file, text.as_bytes());
                assert!(
                    read.is_ok(),
                    "{name}: {:?}",
                    read.err().map(|e| shown(&e.reports))
                );
                clean += 1;
            }
            "vtlook-error" => {
                let Some((_, said)) = blocks.get(i + 1).filter(|(info, _)| info == "text") else {
                    panic!("{name}: a vtlook-error block needs a text block after it");
                };
                let errors = vtlook::read_text(&name, &file, text.as_bytes())
                    .err()
                    .map(|e| shown(&e.reports))
                    .unwrap_or_default();
                assert_eq!(errors, said.lines().collect::<Vec<_>>(), "{name}");
                wrong += 1;
            }
            _ => {}
        }
    }
    // the page holds examples of both kinds, so an empty reading fails
    assert!(
        clean >= 2 && wrong >= 3,
        "{clean} clean and {wrong} wrong examples"
    );
}

#[test]
fn the_page_s_template_is_the_one_vtium_compiles_in() {
    let template: Vec<String> = blocks()
        .into_iter()
        .filter(|(info, text)| info == "vtlook" && text.starts_with("# default.vtlook:"))
        .map(|(_, text)| text)
        .collect();
    assert_eq!(template, [vtlook::TEMPLATE.to_owned()]);
}
