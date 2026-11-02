//! The xtgettcap probe's fixture, `fixtures/kmscon.termcap`, against the
//! XTGETTCAP table vtium answers from. The C reference's build wrote its
//! fixture from its terminfo entry, named `kmscon`, and the copied probe
//! expects that name in `TN`. vtium's table is generated from vtium's own
//! entry, `crates/vtium/install/xterm-vtium.ti`, and holds more, so the
//! fixture keeps the C reference's name and holds only capabilities vtium's
//! table answers with the same value.

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
fn the_termcap_fixture_holds_vtiums_capabilities_under_the_c_references_name() {
    let path = Path::new(TOOL).join("fixtures/kmscon.termcap");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let caps = read_fixture(&text);
    Termcap::new(
        caps.iter()
            .map(|(name, value)| (name.as_slice(), value.as_deref())),
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    let table = vtium::termcap::entries();
    for (name, value) in &caps {
        let shown = String::from_utf8_lossy(name);
        if name == b"TN" || name == b"name" {
            assert_eq!(value.as_deref(), Some(&b"kmscon"[..]), "{shown}");
            continue;
        }
        let have = table
            .iter()
            .find(|(n, _)| n.as_bytes() == name.as_slice())
            .unwrap_or_else(|| panic!("{}: vtium's table lacks {shown}", path.display()));
        assert_eq!(
            have.1,
            value.as_deref(),
            "{}: {shown} differs from crates/vtium/src/termcap/table.rs: write each entry as NAME or NAME=HEX",
            path.display()
        );
    }
}
