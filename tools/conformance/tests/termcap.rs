//! The xtgettcap probe's fixture, `fixtures/kmscon.termcap`, against the
//! XTGETTCAP table vtium answers from: the C reference's build wrote its
//! fixture from its terminfo entry, and this one holds vtium's table,
//! generated from vtium's entry, `crates/vtium/install/kmscon.ti`.

use std::path::Path;

use vtium_term::Termcap;

const TOOL: &str = env!("CARGO_MANIFEST_DIR");

/// A capability's name and its hex-decoded value, none for a boolean.
type Cap = (Vec<u8>, Option<Vec<u8>>);

/// The fixture's lines, `NAME` or `NAME=HEX`.
fn read_fixture(text: &str) -> Vec<Cap> {
    text.lines()
        .map(|line| match line.split_once('=') {
            Some((name, hex)) => {
                let value = (0..hex.len())
                    .step_by(2)
                    .map(|i| {
                        u8::from_str_radix(hex.get(i..i + 2).unwrap_or("?"), 16)
                            .unwrap_or_else(|e| panic!("{line}: {e}"))
                    })
                    .collect();
                (name.as_bytes().to_vec(), Some(value))
            }
            None => (line.as_bytes().to_vec(), None),
        })
        .collect()
}

#[test]
fn the_termcap_fixture_is_the_table_vtium_answers_from() {
    let path = Path::new(TOOL).join("fixtures/kmscon.termcap");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let caps = read_fixture(&text);
    let fixture = Termcap::new(
        caps.iter()
            .map(|(name, value)| (name.as_slice(), value.as_deref())),
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    let table = vtium::termcap::termcap().unwrap_or_else(|e| panic!("{e:?}"));
    assert!(
        fixture == table,
        "{} differs from crates/vtium/src/termcap/table.rs: write each entry as NAME or NAME=HEX",
        path.display()
    );
}
