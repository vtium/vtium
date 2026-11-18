//! `vtium-config fonts` over the whole catalog of font sets: every set gets
//! an answer from the font helper, however many sets there are and however
//! large their fonts. Each set, with its layers in place, also names each
//! font file once.
//!
//! References:
//! - ADR-1204 D5: a set adds its layers' sources in place of its `layer=`
//!   lines.
//! - ADR-1204 D7: vtium-config names each set's missing fonts.
//! - ADR-1204 D9: the catalog's sets in their category folders.

use std::path::{Path, PathBuf};
use std::process::Command;

use vtium_testfont::ttf;

const BIN: &str = env!("CARGO_BIN_EXE_vtium-config");

/// A scratch directory of this test's own, empty.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("catalog")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the scratch directory");
    dir
}

/// `vtium-config fonts` with `frits` and the outline fonts in `fonts`: its
/// standard output.
fn fonts(frits: &Path, fonts: &Path) -> String {
    let out = Command::new(BIN)
        .arg("--no-banner")
        .arg("--frits")
        .arg(frits)
        .arg("--font-dir")
        .arg(fonts)
        .arg("fonts")
        .env("XDG_STATE_HOME", frits.join("state"))
        .output()
        .expect("vtium-config runs");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Every line that says the helper gave no answer for a set.
fn unanswered(out: &str) -> Vec<&str> {
    out.lines()
        .filter(|l| l.contains("the font helper") && l.contains("unknown"))
        .collect()
}

// Each set of the shipped catalog, put in one frits' directory with its
// layers, gets an answer, whichever of its fonts this machine has.
#[test]
fn every_catalog_set_gets_an_answer() {
    let dir = scratch("shipped");
    let frits = dir.join("frits");
    let catalog = Path::new(env!("CARGO_MANIFEST_DIR")).join("frits");
    let mut sets = 0;
    for folder in std::fs::read_dir(&catalog).expect("the catalog") {
        let folder = folder.expect("a folder").path();
        let into = match folder.file_name().and_then(|n| n.to_str()) {
            Some("layers") => frits.join("layers"),
            _ => frits.clone(),
        };
        std::fs::create_dir_all(&into).expect("the folder");
        for file in std::fs::read_dir(&folder).expect("a category") {
            let file = file.expect("a file").path();
            sets += usize::from(into == frits);
            std::fs::copy(&file, into.join(file.file_name().expect("a name"))).expect("a copy");
        }
    }
    assert_eq!(sets, 38);
    let out = fonts(&frits, &dir.join("no-fonts"));
    assert_eq!(unanswered(&out), [] as [&str; 0], "{out}");
}

// Forty sets that each name a 24 MiB font file, more than the helper's
// memory limit holds at once, each get an answer: no one helper holds
// every set's fonts.
#[test]
fn many_sets_of_large_fonts_each_get_an_answer() {
    let dir = scratch("large");
    let frits = dir.join("frits");
    let fonts_dir = dir.join("fonts");
    std::fs::create_dir_all(&frits).expect("the frits");
    std::fs::create_dir_all(&fonts_dir).expect("the fonts");
    let mut big = ttf::test_mono().build();
    big.resize(big.len() + (24 << 20), 0);
    let file = fonts_dir.join("Big-Regular.ttf");
    std::fs::write(&file, &big).expect("the font");
    for n in 0..40 {
        std::fs::write(
            frits.join(format!("big{n}.vtfrit")),
            format!("source=outline:{}\n", file.display()),
        )
        .expect("a set");
    }
    let out = fonts(&frits, &fonts_dir);
    assert_eq!(
        out.lines().filter(|l| l.starts_with("big")).count(),
        40,
        "{out}"
    );
    assert_eq!(unanswered(&out), [] as [&str; 0], "{out}");
}

// No set of the catalog, with its layers in place, names one font file in
// two sources of the same style. The helper reads, hashes and parses each
// source on its own, so a second source of the same file costs a second
// read of the file and holds its glyphs twice, and it can draw nothing the
// first one does not.
//
// References:
// - ADR-1204 D5: a set adds its layers' sources in place of its `layer=`
//   lines.
#[test]
fn no_catalog_set_names_one_font_file_twice() {
    let catalog = Path::new(env!("CARGO_MANIFEST_DIR")).join("frits");
    let layers = catalog.join("layers");
    let mut sets = 0;
    for folder in std::fs::read_dir(&catalog).expect("the catalog") {
        let folder = folder.expect("a folder").path();
        if folder == layers {
            continue;
        }
        for file in std::fs::read_dir(&folder).expect("a category") {
            let file = file.expect("a file").path();
            let frit = vtium_config::frit::Frit::load_in(&file, &layers).expect("the set reads");
            sets += 1;
            let mut seen = std::collections::BTreeSet::new();
            for source in &frit.sources {
                if source.font_name().is_some_and(|f| f.starts_with('/')) {
                    assert!(
                        seen.insert((source.spelling.clone(), format!("{:?}", source.style))),
                        "{} names {} twice",
                        file.display(),
                        source.spelling
                    );
                }
            }
        }
    }
    assert_eq!(sets, 38);
}
