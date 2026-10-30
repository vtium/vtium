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
        Kind::Number { unit, word, .. } => format!("number ({unit}){}", or(word)),
        Kind::Text => "text".to_owned(),
        Kind::Choice(c) => format!("one of `{}`", c.join("`, `")),
        Kind::Command => "command".to_owned(),
        Kind::Rgb => "color, `r,g,b`".to_owned(),
        Kind::Mode => "mode, a line each".to_owned(),
        Kind::Grab => "chords, `<Mod>...key`, separated by commas".to_owned(),
        Kind::DecoColor => "color, `fg`, a palette index or `#rrggbb`".to_owned(),
        Kind::Char => "character".to_owned(),
    }
}

/// A number key's word, as the tables give it after the number.
fn or(word: Option<&str>) -> String {
    word.map(|w| format!(", or `{w}`")).unwrap_or_default()
}

fn bound(k: &Key) -> String {
    match k.kind {
        Kind::Switch => "on or off".to_owned(),
        Kind::Number { min, max, word, .. } => format!("{min} to {max}, clamped{}", or(word)),
        Kind::Text => format!("{TEXT_MAX} bytes, no control character"),
        Kind::Choice(_) => "the choices".to_owned(),
        Kind::Command => format!("{TEXT_MAX} bytes in a file"),
        Kind::Rgb => "each 0 to 255".to_owned(),
        Kind::Mode => "names of 63 bytes, each side 1 to 65535".to_owned(),
        Kind::Grab => format!(
            "{TEXT_MAX} bytes; modifiers Shift, Lock, Control, Alt, Logo; keys as xkb names them"
        ),
        Kind::DecoColor => "an index 0 to 255, or six hex digits".to_owned(),
        Kind::Char => "one character, no control character or space".to_owned(),
    }
}

fn default(k: &Key) -> String {
    match k.default {
        Default::Switch(true) => "on".to_owned(),
        Default::Switch(false) => "off".to_owned(),
        Default::Number(n) => n.to_string(),
        Default::Text(t) => format!("`{t}`"),
        Default::Command(c) => format!("`{}`", c.join(" ")),
        Default::Rgb([r, g, b]) => format!("`{r},{g},{b}`"),
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
         vtium reads `kmscon.conf` in the configuration directory (`--configdir`, `/etc/kmscon` by default), then `<seat>.seat.conf` there, then its command line. Each wins over the ones before it. A file that does not exist is skipped. The files are read once, at start: a change applies when vtium starts again, as at a restart of its service. `vtium check-config [FILE]` reads them as the start would, with the head file, `/etc/vtium/heads.conf`, or FILE in its place to check it before it is installed, through the start's parser and evaluator (ADR-1401 D7). It applies nothing, exits 1 on any warning and on each of the head file's errors and warnings, each named with its line, and notes on standard output what it does not count: the density points convert at, and of the head file each guard turned off, each rule sending input to a head by `id` or `name` alone, where no udev daemon runs, each rule using a key that reads udev's database, each rule using `connector-path:`, which this build matches to nothing (vtium/vtium#157), and what the devices present find: each rule matching none of them, a head claiming pointers while every keyboard is claimed elsewhere, every keyboard ignored, and a head whose only keyboard is one device (ADR-1401 D9). It reads the devices from sysfs and udev's database, `/run/udev/data`, and opens nothing under `/dev`. A FILE that root does not own, or that others can write, is checked all the same, with a note, since installing it makes it root's.\n\
         \n\
         A file holds one key a line: `key=value`, or a switch's `key` to turn it on and `no-key` to turn it off. `#` starts a comment, except where it opens a `#rrggbb` color value. Spaces and tabs around the key and the value are dropped. On the command line a key is `--key=VALUE` or `--key VALUE`, and a switch `--key` or `--no-key`.\n\
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
            (_, Default::Rgb([r, g, b])) => format!("{}={r},{g},{b}", k.name),
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
