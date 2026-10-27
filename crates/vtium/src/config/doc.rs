//! `docs/config.md` and `install/kmscon.conf.example`, generated from the
//! key table.

use std::fmt::Write as _;

use super::keys::{Default, KEYS, Key, Kind, Scope, TEXT_MAX};
use super::{FILE_MAX, LINE_MAX};

const SOURCE: &str = "crates/vtium/src/config/keys.rs";
const BLESS: &str = "VTIUM_BLESS=1 cargo test -p vtium generated_files";

fn kind(k: &Key) -> String {
    match k.kind {
        Kind::Switch => "switch".to_owned(),
        Kind::Number { unit, .. } => format!("number ({unit})"),
        Kind::Text => "text".to_owned(),
        Kind::Choice(c) => format!("one of `{}`", c.join("`, `")),
        Kind::Command => "command".to_owned(),
    }
}

fn bound(k: &Key) -> String {
    match k.kind {
        Kind::Switch => "on or off".to_owned(),
        Kind::Number { min, max, .. } => format!("{min} to {max}, clamped"),
        Kind::Text => format!("{TEXT_MAX} bytes, no control character"),
        Kind::Choice(_) => "the choices".to_owned(),
        Kind::Command => format!("{TEXT_MAX} bytes in a file"),
    }
}

fn default(k: &Key) -> String {
    match k.default {
        Default::Switch(true) => "on".to_owned(),
        Default::Switch(false) => "off".to_owned(),
        Default::Number(n) => n.to_string(),
        Default::Text(t) => format!("`{t}`"),
        Default::Command(c) => format!("`{}`", c.join(" ")),
        Default::Unset => "unset".to_owned(),
    }
}

fn option(k: &Key) -> String {
    let long = match (k.kind, k.scope) {
        (Kind::Switch, Scope::CommandLine) => format!("`--{}`", k.name),
        (Kind::Switch, _) => format!("`--{0}`, `--no-{0}`", k.name),
        (Kind::Command, _) => format!("`--{} -- COMMAND...`", k.name),
        _ => format!("`--{}=VALUE`", k.name),
    };
    match k.short {
        Some(c) => format!("{long}, `-{c}`"),
        None => long,
    }
}

/// `docs/config.md`.
pub(super) fn markdown() -> String {
    let mut out = format!(
        "# Configuration\n\
         \n\
         <!-- Generated from {SOURCE}; do not edit. `{BLESS}` rewrites it. -->\n\
         \n\
         vtium reads `kmscon.conf` in the configuration directory (`--configdir`, `/etc/kmscon` by default), then `<seat>.seat.conf` there, then its command line. Each wins over the ones before it. A file that does not exist is skipped.\n\
         \n\
         A file holds one key a line: `key=value`, or a switch's `key` to turn it on and `no-key` to turn it off. `#` starts a comment, except where it opens a `#rrggbb` colour value. Spaces and tabs around the key and the value are dropped. On the command line a key is `--key=VALUE` or `--key VALUE`, and a switch `--key` or `--no-key`.\n\
         \n\
         vtium reports, once at start, a key it does not know, however often it appears, and a value it cannot use: a number out of bounds, which it clamps, and a value it cannot read, which it ignores, keeping the value the sources before it gave. It reads at most {FILE_MAX} bytes of a file and {LINE_MAX} of a line.\n\
         \n\
         `install/kmscon.conf.example` in `crates/vtium` lists the same keys at their defaults. Source cites the C reference, kmscon at `b1f7a7c5` ([c-reference.md](c-reference.md)), or the decision a key comes from.\n\
         \n\
         | Key | Option | Type | Default | Bound | Read by | Set in | What | Source |\n\
         |---|---|---|---|---|---|---|---|---|\n"
    );
    for k in &KEYS {
        let set_in = match k.scope {
            Scope::Anywhere => "file, command line",
            Scope::CommandLine => "command line",
        };
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} | {} | {} | {set_in} | {} | {} |",
            k.name,
            option(k),
            kind(k),
            default(k),
            bound(k),
            k.reader,
            k.what,
            k.cite
        );
    }
    out
}

/// `install/kmscon.conf.example`.
pub(super) fn example() -> String {
    let mut out = format!(
        "# vtium's configuration, installed as /etc/kmscon/kmscon.conf.example.\n\
         # Generated from {SOURCE}; docs/config.md describes each key.\n\
         #\n\
         # One key a line: key=value, or a switch's key to turn it on and no-key to\n\
         # turn it off. Each key is shown at its default, commented out. The command\n\
         # line wins over this file.\n"
    );
    for k in KEYS.iter().filter(|k| k.scope == Scope::Anywhere) {
        let line = match (k.kind, k.default) {
            (Kind::Switch, Default::Switch(true)) => k.name.to_owned(),
            (Kind::Switch, _) => format!("no-{}", k.name),
            (_, Default::Number(n)) => format!("{}={n}", k.name),
            (_, Default::Text(t)) => format!("{}={t}", k.name),
            (_, Default::Command(c)) => format!("{}={}", k.name, c.join(" ")),
            _ => format!("{}=", k.name),
        };
        let _ = write!(out, "\n## {}\n#{line}\n", k.what);
    }
    out
}

#[cfg(test)]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    #[test]
    fn generated_files_match_the_table() {
        let root = env!("CARGO_MANIFEST_DIR");
        let files = [
            (format!("{root}/../../docs/config.md"), markdown()),
            (format!("{root}/install/kmscon.conf.example"), example()),
        ];
        let mut stale = Vec::new();
        for (path, want) in files {
            let have = std::fs::read_to_string(&path).unwrap_or_default();
            if have == want {
                continue;
            }
            if std::env::var_os("VTIUM_BLESS").is_some() {
                std::fs::write(&path, want).expect("rewrite a generated file");
            } else {
                stale.push(path);
            }
        }
        assert!(
            stale.is_empty(),
            "{stale:?} differ from the key table; rerun with VTIUM_BLESS=1 to rewrite them"
        );
    }
}
