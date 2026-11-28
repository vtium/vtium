//! What check-config says of the lists of active resources in the
//! packages' folder and in root's: each finding of the lists counts
//! against the configuration, as a finished sentence that names the file
//! first.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use vtium_settings::Trust;
use vtium_settings::batch::{Batch, MERGED_BATCH_MAX, Places};

use super::resources::batch_findings;
use crate::scratch::{Scratch, batch_places, file};

/// A machine under its own scratch folder, whose files pass the owner and
/// mode check for the account the test runs as.
struct Machine {
    _dir: Scratch,
    places: Places,
    trust: Trust,
}

impl Machine {
    fn new(name: &str) -> Machine {
        let dir = Scratch::new(&format!("check-lists-{name}"));
        let _ = std::fs::set_permissions(&*dir, std::fs::Permissions::from_mode(0o755));
        let places = batch_places(&dir);
        let trust = Trust::owner(rustix::process::geteuid().as_raw()).below(&dir);
        Machine {
            _dir: dir,
            places,
            trust,
        }
    }

    /// The package's list `name`.
    fn package(&self, name: &str) -> PathBuf {
        self.places.package_lists().join(name)
    }

    /// Root's list `name`.
    fn admin(&self, name: &str) -> PathBuf {
        self.places.admin_lists().join(name)
    }

    /// What check-config counts against the configuration and notes of
    /// the batch this machine builds.
    fn findings(&self) -> (Vec<String>, Vec<String>) {
        batch_findings(&Batch::system(&self.places, &self.trust))
    }
}

/// The first finding that starts with `path`, the file it names.
fn naming<'a>(lines: &'a [String], path: &Path) -> Option<&'a str> {
    let path = path.display().to_string();
    lines
        .iter()
        .map(String::as_str)
        .find(|l| l.starts_with(&path))
}

// A list that others can write fails the owner and mode check, so vtium
// activates nothing from it, and check-config names the file and why.
//
// References:
// - ADR-1205 D4, as amended on 2026-11-27: a list that fails the trust rule activates nothing, and a finding names the file and why.
#[test]
fn check_config_names_a_list_that_fails_the_trust_rule() {
    let m = Machine::new("trust");
    let list = m.admin("extra.conf");
    file(&list, b"/opt/fonts/extra.ttf\n", 0o666);
    let (counted, notes) = m.findings();
    let finding = naming(&counted, &list).unwrap_or_default();
    assert!(
        finding.starts_with(&format!(
            "{}: vtium activates nothing from this list of active fonts, color schemes, effects and pointer themes, because ",
            list.display()
        )),
        "{counted:#?}"
    );
    assert!(finding.ends_with('.'), "{finding}");
    assert_eq!(naming(&notes, &list), None, "{notes:#?}");
}

// Root's file of a package's list's name that vtium refuses still turns
// the package's list off, and check-config says so. A link to /dev/null
// turns a package's list off on purpose, so check-config says nothing of
// it.
//
// References:
// - ADR-1205 D4, as amended on 2026-11-27: turning a package's list off from /etc, by name over every entry.
#[test]
fn check_config_names_the_package_list_a_refused_file_turns_off() {
    let m = Machine::new("masked");
    let theirs = m.package("fonts.conf");
    file(&theirs, b"/opt/fonts/a.ttf\n", 0o644);
    let ours = m.admin("fonts.conf");
    file(&ours, b"/opt/fonts/b.ttf\n", 0o666);
    let quiet = m.package("quiet.conf");
    file(&quiet, b"/opt/fonts/c.ttf\n", 0o644);
    let off = m.admin("quiet.conf");
    let _ = std::os::unix::fs::symlink("/dev/null", &off);
    let (counted, notes) = m.findings();
    let finding = naming(&counted, &ours).unwrap_or_default();
    assert!(
        finding.ends_with(&format!(
            " Its name still turns off the package's list {}.",
            theirs.display()
        )),
        "{counted:#?}"
    );
    let all: Vec<&String> = counted.iter().chain(&notes).collect();
    assert!(
        !all.iter().any(|l| l.contains("quiet.conf")),
        "a list turned off on purpose is no finding: {all:#?}"
    );
}

// A `-` line in a list turns nothing off, since only batch.conf can turn
// a path off, and check-config names the file and the line.
//
// References:
// - ADR-1205 D4, as amended on 2026-11-27: only the batch file holds `-` lines.
#[test]
fn check_config_names_a_minus_line_in_a_list() {
    let m = Machine::new("minus");
    let list = m.admin("own.conf");
    file(&list, b"/opt/fonts/c.ttf\n-/opt/fonts/d.ttf\n", 0o644);
    let (counted, _) = m.findings();
    assert_eq!(
        naming(&counted, &list),
        Some(
            format!(
                "{}:2: This line starts with -, which turns a path off, and only batch.conf can turn a path off. Because of this, vtium ignores the line.",
                list.display()
            )
            .as_str()
        ),
        "{counted:#?}"
    );
}

// The lists that would take the merged batch past its bound activate
// nothing, and check-config names each one, from the first that passes.
//
// References:
// - ADR-1205 D4, as amended on 2026-11-27: the merged batch's bound of 1 MiB, and a finding for each list left out.
#[test]
fn check_config_names_each_list_past_the_merged_bound() {
    let m = Machine::new("bound");
    let line = |list: usize, n: usize| format!("/{list:02}{n:02}{}\n", "x".repeat(994));
    let per_list = 60;
    let lists = MERGED_BATCH_MAX / (per_list * 1000) + 3;
    for list in 0..lists {
        let text: String = (0..per_list).map(|n| line(list, n)).collect();
        file(
            &m.admin(&format!("list{list:02}.conf")),
            text.as_bytes(),
            0o644,
        );
    }
    let (counted, _) = m.findings();
    let first_out = MERGED_BATCH_MAX / (per_list * 1000);
    let past = |list: usize| {
        let path = m.admin(&format!("list{list:02}.conf"));
        format!(
            "{}: vtium activates nothing from this list of active resources, because the paths of the lists before it already come close to {MERGED_BATCH_MAX} bytes, the most vtium holds, and this list would pass that. Remove the lists this machine does not need.",
            path.display()
        )
    };
    for list in first_out..lists {
        assert!(counted.contains(&past(list)), "{list}: {counted:#?}");
    }
    assert!(
        !counted.contains(&past(first_out - 1)),
        "{}: {counted:#?}",
        first_out - 1
    );
}
