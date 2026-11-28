//! What the console logs of the lists of active resources when it reads
//! the batch: each finding of the lists as a warning, once for each batch
//! that differs from the one the console held.

use std::os::unix::fs::PermissionsExt;

use vtium_settings::Trust;
use vtium_settings::cullet::Rule;
use vtium_settings::mounts::Mounts;

use super::batch::load_batch_with;
use crate::scratch::{Scratch, batch_places, file};

// The console logs each finding of the lists as a warning when it reads
// a batch that differs from the one it held, and a login that reads the
// same batch logs nothing again.
//
// References:
// - ADR-1205 D4, as amended on 2026-11-27: a list vtium refuses, and a line of a list it ignores, each get a finding.
#[test]
fn the_console_logs_each_finding_of_the_lists_once() {
    let dir = Scratch::new("console-lists");
    let _ = std::fs::set_permissions(&*dir, std::fs::Permissions::from_mode(0o755));
    let places = batch_places(&dir);
    let trust = Trust::owner(rustix::process::geteuid().as_raw()).below(&dir);
    let refused = places.admin_lists().join("open.conf");
    file(&refused, b"/opt/fonts/a.ttf\n", 0o666);
    let minus = places.package_lists().join("fonts.conf");
    file(&minus, b"/opt/fonts/b.ttf\n-/opt/fonts/c.ttf\n", 0o644);
    let read = || load_batch_with(&places, &trust, Rule::new(trust.clone()), Mounts::default());
    let found = read().expect("the first read logs");
    let warnings: Vec<String> = found
        .lines()
        .into_iter()
        .filter(|(level, _)| *level == log::Level::Warn)
        .map(|(_, line)| line)
        .collect();
    let minus_line = format!(
        "{}:2: This line starts with -, which turns a path off, and only batch.conf can turn a path off. Because of this, vtium ignores the line.",
        minus.display()
    );
    assert_eq!(
        warnings.iter().filter(|l| **l == minus_line).count(),
        1,
        "{warnings:#?}"
    );
    let refused_start = format!(
        "{}: vtium activates nothing from this list of active fonts, color schemes, effects and pointer themes, because ",
        refused.display()
    );
    assert_eq!(
        warnings
            .iter()
            .filter(|l| l.starts_with(&refused_start))
            .count(),
        1,
        "{warnings:#?}"
    );
    assert!(read().is_none(), "the same batch logs nothing again");
}
