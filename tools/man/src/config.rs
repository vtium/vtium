//! vtium-config(1): the synopsis, the commands, the options and the exit
//! statuses from vtium-config's own lists, which its `--help` prints too.

use vtium_config_core::help;
use vtium_config_core::options::VALUE_OPTIONS;

use crate::Stamp;
use crate::roff::{Writer, synopsis};

const TEMPLATE: &str = include_str!("../templates/vtium-config.1.in");

pub fn page(stamp: &Stamp) -> String {
    let mut w = Writer::new(
        "vtium-config",
        1,
        &stamp.date,
        &stamp.source,
        "User Commands",
    );
    w.template(TEMPLATE, &mut |name, w| {
        match name {
            "SYNOPSIS" => w.synopsis([help::SYNOPSIS]),
            "DESCRIPTION" => {
                for paragraph in help::DESCRIPTION {
                    w.paragraph(paragraph);
                }
                w.paragraph(help::DRAFTS);
            }
            "COMMANDS" => commands(w),
            "OPTIONS" => options(w),
            "EXIT STATUS" => {
                for (code, means) in help::EXIT_STATUS {
                    w.item(&format!("\\fB{code}\\fR"), [means]);
                }
            }
            _ => return false,
        }
        true
    });
    w.finish()
}

/// Each command: its forms, what it does, and an example.
fn commands(w: &mut Writer) {
    for topic in help::topics() {
        w.subsection(topic.command.name());
        w.synopsis(
            topic
                .forms
                .iter()
                .map(|f| format!("vtium-config {}", f.synopsis))
                .collect::<Vec<_>>()
                .iter()
                .map(String::as_str),
        );
        w.paragraph(topic.detail);
        w.paragraph("For example:");
        w.example([topic.example]);
    }
}

/// The options, from the parser's table.
fn options(w: &mut Writer) {
    for o in VALUE_OPTIONS {
        w.item(&synopsis(&format!("{} {}", o.name, o.value)), [o.does]);
    }
    w.item(
        &synopsis("--help [TOPIC]"),
        ["print the usage, or with TOPIC the help on one command, and exit"],
    );
    w.item(&synopsis("--version"), ["print the version and exit"]);
}
