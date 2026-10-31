//! The head file's bounds: one source for the constants the code uses and
//! for the table in this crate's README.
//!
//! Each bound is a [`Bound`] row holding its value, what happens past it,
//! and where its value comes from; the public constant takes its value
//! from the row. The README table is generated from [`BOUNDS`], and
//! `readme_table_matches_bounds` fails when the committed copy differs.
//! `VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config readme_table` rewrites it.

/// One bound and its account, a row of the README table.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct Bound {
    name: &'static str,
    value: u32,
    what: &'static str,
    past: &'static str,
    cite: &'static str,
}

const FILE: Bound = Bound {
    name: "FILE_MAX",
    value: 64 * 1024,
    what: "Bytes of a head file",
    past: "The file is dropped whole but for the `policy` lines within the bound",
    cite: "ADR-1401 D3, kmscon.conf's bound (`crates/vtium/src/config/mod.rs`, `FILE_MAX`)",
};

const LINE: Bound = Bound {
    name: "LINE_MAX",
    value: 8 * 1024,
    what: "Bytes of one line, its newline apart",
    past: "The file is dropped whole but for its `policy` lines, the line named",
    cite: "ADR-1401 D3, kmscon.conf's bound (`crates/vtium/src/config/mod.rs`, `LINE_MAX`)",
};

const NAME: Bound = Bound {
    name: "NAME_MAX",
    value: 64,
    what: "Characters of a head's or a profile's name, each of `A-Z a-z 0-9 . _ + -`",
    past: "The head is dropped, with every rule naming it, the line named",
    cite: "ADR-1401 D3",
};

const HEADS: Bound = Bound {
    name: "HEADS_MAX",
    value: 16,
    what: "Heads a file declares",
    past: "The file is dropped whole but for its `policy` lines, the first head past it named",
    cite: "vtium's own: a head a display, with room for a machine of many outputs",
};

const RULES: Bound = Bound {
    name: "RULES_MAX",
    value: 256,
    what: "Rules a file holds, so a device's walk reads at most this many",
    past: "The file is dropped whole but for its `policy` lines, the first rule past it named",
    cite: "vtium's own: a rule a device with room to spare",
};

const TOKENS: Bound = Bound {
    name: "TOKENS_MAX",
    value: 8,
    what: "`key:value` tokens one rule's match combines",
    past: "The rule is dropped, the line named",
    cite: "vtium's own: the strongest binding takes two (`path` and `serial`, or `connector` and `edid`)",
};

const CONSOLES: Bound = Bound {
    name: "CONSOLES_MAX",
    value: 50,
    what: "Consoles one head asks for with `consoles`",
    past: "The head gets this many, with a warning",
    cite: "a starting value, the C reference's `session-max` (`kmscon/src/config.c:1357`); ADR-1401 D10 and its probe 6 set the caps",
};

const ERRORS: Bound = Bound {
    name: "ERRORS_MAX",
    value: 64,
    what: "Errors, warnings and notes a check keeps, each kind apart",
    past: "Counted, not kept",
    cite: "kmscon.conf's `REPORTS_MAX` (`crates/vtium/src/config/mod.rs`)",
};

const QUOTE: Bound = Bound {
    name: "QUOTE_MAX",
    value: 64,
    what: "Characters of the file's text an error quotes, escaped when shown",
    past: "The quote is cut, and shown ending `...`",
    cite: "vtium's own: a name, a value or a token",
};

/// Every bound, in the README table's order.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const BOUNDS: [Bound; 9] = [
    FILE, LINE, NAME, HEADS, RULES, TOKENS, CONSOLES, ERRORS, QUOTE,
];

/// Bytes of a head file; a larger file is dropped whole but for its
/// `policy` lines.
pub const FILE_MAX: usize = FILE.value as usize;

/// Bytes of one line, its newline apart; a longer line drops the file
/// whole but for its `policy` lines.
pub const LINE_MAX: usize = LINE.value as usize;

/// Characters of a head's or a profile's name.
pub const NAME_MAX: usize = NAME.value as usize;

/// Heads a file declares.
pub const HEADS_MAX: usize = HEADS.value as usize;

/// Rules a file holds.
pub const RULES_MAX: usize = RULES.value as usize;

/// Tokens one rule's match combines.
pub const TOKENS_MAX: usize = TOKENS.value as usize;

/// Consoles a head gets at most; a head asking for more gets this many,
/// with a warning.
pub const CONSOLES_MAX: u32 = CONSOLES.value;

/// Errors, warnings and notes kept, each kind apart; past it they are
/// counted.
pub const ERRORS_MAX: usize = ERRORS.value as usize;

/// Characters of the file's text an error quotes.
pub const QUOTE_MAX: usize = QUOTE.value as usize;

#[cfg(test)]
#[expect(clippy::panic, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    const START: &str = "<!-- bounds table: generated from src/bounds.rs -->";
    const END: &str = "<!-- end of the bounds table -->";

    fn table() -> String {
        let mut out = String::from(START);
        out.push_str(
            "\n| Bound | Value | What it bounds | Past it | Source |\n|---|---|---|---|---|\n",
        );
        for b in &BOUNDS {
            out.push_str(&format!(
                "| `{}` | {} | {} | {} | {} |\n",
                b.name, b.value, b.what, b.past, b.cite
            ));
        }
        out.push_str(END);
        out
    }

    #[test]
    fn readme_table_matches_bounds() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/README.md");
        let readme = std::fs::read_to_string(path).unwrap_or_default();
        let (Some(start), Some(end)) = (readme.find(START), readme.find(END)) else {
            panic!("README.md has no bounds table between its markers");
        };
        let committed = readme
            .get(start..end.saturating_add(END.len()))
            .unwrap_or_default();
        let want = table();
        if std::env::var_os("VTIUM_CONFIG_BLESS").is_some() {
            let blessed = readme.replacen(committed, &want, 1);
            std::fs::write(path, blessed).unwrap_or_default();
            return;
        }
        assert_eq!(
            committed, want,
            "README.md's bounds table differs from src/bounds.rs"
        );
    }
}
