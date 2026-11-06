//! vtium.conf as the console reads it from disk (ADR-1403 D2, D3): the one
//! file in the configuration directory, read only when the ownership
//! check trusts it, with the reason and the command that mends it when it
//! does not. The tests run as their own user, whose files a vtium that
//! runs unprivileged trusts, under the build's own temporary directory.

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use vtium::config::{File, Problem, keys};

#[path = "common/scratch.rs"]
mod scratch;
use scratch::Scratch;

/// A configuration directory of its own holding `text` as vtium.conf in
/// `mode`, read as the console reads it.
fn read(name: &str, text: &str, mode: u32) -> (Scratch, vtium::config::Loaded) {
    let d = Scratch::new(&format!("conf-file-{name}"));
    std::fs::set_permissions(&*d, std::fs::Permissions::from_mode(0o700)).expect("0700");
    let file = d.join("vtium.conf");
    std::fs::write(&file, text).expect("vtium.conf");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).expect("its mode");
    let args = [OsString::from(format!("--configdir={}", d.display()))];
    let loaded = vtium::config::load(&args, &mut vtium::config::read_file);
    (d, loaded)
}

const TEXT: &str = "font-size=31\nversion = 1\n[head desk]\n[rules]\ndisplay * -> desk\n";

// ADR-1403 D3: a vtium.conf the account vtium runs as owns, which no one
// else can write, is read whole: its keys and its sections
#[test]
fn a_file_its_own_user_owns_is_read_whole() {
    let (d, loaded) = read("trusted", TEXT, 0o644);
    assert_eq!(loaded.reports, []);
    assert_eq!(loaded.config.number(&keys::FONT_SIZE), Some(31));
    assert_eq!(loaded.path, d.join("vtium.conf"));
    let start = vtium::head::rules(&loaded.path, &loaded.file);
    assert_eq!(start.notice(), None);
    assert_eq!(
        start
            .rules
            .heads
            .iter()
            .map(|h| h.name.as_str())
            .collect::<Vec<_>>(),
        ["desk"]
    );
}

// ADR-1403 D3: a vtium.conf its group or others can write is read not at
// all: no top-level key applies, the shipped rules run with the skipped
// line, and the report names the path, its mode and the command that
// mends it
#[test]
fn a_file_others_can_write_applies_nothing_and_says_how_to_mend_it() {
    let (d, loaded) = read("writable", TEXT, 0o664);
    let path = d.join("vtium.conf");
    assert_eq!(
        loaded.config.number(&keys::FONT_SIZE),
        Some(0),
        "the default"
    );
    assert!(
        matches!(loaded.file, File::Untrusted(_)),
        "{:?}",
        loaded.file
    );
    let [report] = loaded.reports.as_slice() else {
        panic!("{:?}", loaded.reports);
    };
    assert!(matches!(report.problem, Problem::Untrusted(_)));
    let shown = report.to_string();
    assert!(shown.contains("(mode 664)"), "{shown}");
    assert!(
        shown.contains(&format!("Run `chmod go-w {}`", path.display())),
        "{shown}"
    );
    let start = vtium::head::rules(&loaded.path, &loaded.file);
    assert_eq!(start.rules, vtium_settings::Config::builtin());
    assert_eq!(
        start.notice().as_deref(),
        Some(
            "vtium.conf not used: running the built-in configuration.\nTo see why, run vtium check-config /etc/vtium/vtium.conf."
        )
    );
    // the command line still applies
    let args = [
        OsString::from(format!("--configdir={}", d.display())),
        OsString::from("--font-size=9"),
    ];
    let loaded = vtium::config::load(&args, &mut vtium::config::read_file);
    assert_eq!(loaded.config.number(&keys::FONT_SIZE), Some(9));
}

// ADR-1403 D3: the directory above the file gets the same check
#[test]
fn a_directory_others_can_write_refuses_the_file_below_it() {
    let (d, _) = read("dir", TEXT, 0o644);
    std::fs::set_permissions(&*d, std::fs::Permissions::from_mode(0o777)).expect("0777");
    let args = [OsString::from(format!("--configdir={}", d.display()))];
    let loaded = vtium::config::load(&args, &mut vtium::config::read_file);
    std::fs::set_permissions(&*d, std::fs::Permissions::from_mode(0o700)).expect("0700");
    assert_eq!(loaded.config.number(&keys::FONT_SIZE), Some(0));
    let shown = loaded
        .reports
        .first()
        .map(ToString::to_string)
        .unwrap_or_default();
    assert!(
        shown.contains(&format!("Run `chmod go-w {}`", d.display())),
        "{shown}"
    );
}

// ADR-1403 D2: a seat file beside vtium.conf is never read
#[test]
fn a_seat_file_beside_vtium_conf_is_not_read() {
    let (d, _) = read("seat", "font-size=31\n", 0o644);
    std::fs::write(d.join("seat0.seat.conf"), "font-size=7\n").expect("a seat file");
    let args = [OsString::from(format!("--configdir={}", d.display()))];
    let loaded = vtium::config::load(&args, &mut vtium::config::read_file);
    assert_eq!(loaded.config.number(&keys::FONT_SIZE), Some(31));
    assert!(!Path::new(&loaded.path).ends_with("seat0.seat.conf"));
}

/// The names of the heads the start runs for `loaded`'s vtium.conf.
fn heads_run(loaded: &vtium::config::Loaded) -> Vec<String> {
    vtium::head::rules(&loaded.path, &loaded.file)
        .rules
        .heads
        .iter()
        .map(|h| h.name.clone())
        .collect()
}

// ADR-1403 D3: the two parts of vtium.conf take their mistakes apart. A
// look that sets a key no look takes is dropped with its error, and a
// file with sections but no `version` line drops every section; in both
// the top level's font-size still applies. An unknown key and a value
// out of bounds at the top level are reported and leave the sections
// standing.
#[test]
fn an_error_in_a_section_leaves_the_top_level_keys_applied() {
    let bad_look = "font-size=31\nversion = 1\n[look big]\nconsoles = 2\n\
                    [head desk]\nlook = big\n[rules]\ndisplay * -> desk\n";
    let no_version = "font-size=31\n[head desk]\n[rules]\ndisplay * -> desk\n";
    for (name, text) in [("bad-look", bad_look), ("no-version", no_version)] {
        let (_d, loaded) = read(name, text, 0o644);
        assert_eq!(loaded.reports, [], "{name}: no top-level report");
        assert_eq!(
            loaded.config.number(&keys::FONT_SIZE),
            Some(31),
            "{name}: the top level applies"
        );
        let start = vtium::head::rules(&loaded.path, &loaded.file);
        assert!(
            start.notice().is_some(),
            "{name}: the sections' error shows"
        );
        assert!(
            start.rules.looks.iter().all(|l| l.name != "big"),
            "{name}: the look is dropped"
        );
    }
    let (_d, loaded) = read("no-version-heads", no_version, 0o644);
    assert_ne!(heads_run(&loaded), ["desk"], "no version, no section runs");
    let bad_top = "no-such-key=1\nsb-size=99999999999\nfont-size=31\nversion = 1\n\
                   [head desk]\n[rules]\ndisplay * -> desk\n";
    let (_d, loaded) = read("bad-top", bad_top, 0o644);
    assert_eq!(loaded.reports.len(), 2, "{:?}", loaded.reports);
    assert_eq!(loaded.config.number(&keys::FONT_SIZE), Some(31));
    assert_eq!(heads_run(&loaded), ["desk"], "the sections stand");
    assert_eq!(
        vtium::head::rules(&loaded.path, &loaded.file).notice(),
        None
    );
}
