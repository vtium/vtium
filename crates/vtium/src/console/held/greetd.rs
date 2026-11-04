//! The VT greetd's configuration adds, read as greetd reads it
//! (`greetd/src/config/mod.rs`).
//!
//! greetd reads the file its `--config` names, or else
//! `/etc/greetd/greetd.conf`, and `/etc/greetd/config.toml` when that is
//! missing. It skips a line that starts with `#`, takes `[name]` as a
//! section, and `key = value` as a setting, a value in quotes losing them
//! (`inish/src/lib.rs`). Its `[terminal] vt` is a VT's number, which holds
//! that VT; `current`, the VT shown when greetd starts, which is VT 1 at
//! boot, so vtium holds VT 1; or `next` or `none`, which name no VT. A
//! `--vt` on its command line stands for the file's `vt`. vtium reads both
//! options from the `ExecStart=` of greetd's unit.

use std::path::Path;

use super::{Setting, read, vt};
use crate::console::text::greetd as text;

/// A setting's value without the quotes around it.
fn unquote(value: &str) -> &str {
    for q in ['"', '\''] {
        if let Some(inner) = value.strip_prefix(q).and_then(|v| v.strip_suffix(q)) {
            return inner;
        }
    }
    value
}

/// The value of the option `long` or `short` in a command line's words.
fn option<'a>(words: &[&'a str], long: &str, short: Option<&str>) -> Option<&'a str> {
    let mut found = None;
    let mut words = words.iter();
    while let Some(word) = words.next() {
        if *word == long || Some(*word) == short {
            found = words.next().copied();
        } else if let Some(value) = word
            .strip_prefix(long)
            .and_then(|rest| rest.strip_prefix('='))
        {
            found = Some(value);
        }
    }
    found
}

/// The `[terminal] vt` of greetd's configuration text.
fn terminal_vt(text: &str) -> Option<String> {
    let mut section = "";
    let mut vt = None;
    for line in text.lines().map(str::trim) {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.trim();
        } else if let Some((key, value)) = line.split_once('=')
            && section == "terminal"
            && key.trim() == "vt"
        {
            vt = Some(unquote(value.trim()).to_owned());
        }
    }
    vt
}

/// The VT greetd's configuration under `root` adds, `exec` its unit's
/// `ExecStart=` and `unit` the unit's name.
pub fn config(root: &Path, unit: &str, exec: Option<&str>) -> Option<Setting> {
    let words: Vec<&str> = exec
        .unwrap_or_default()
        .trim_start_matches(['-', '@', ':', '+', '!', '|'])
        .split_whitespace()
        .collect();
    let files: Vec<String> = match option(&words, "--config", Some("-c")) {
        Some(path) => vec![path.trim_start_matches('/').to_owned()],
        None => vec![
            "etc/greetd/greetd.conf".to_owned(),
            "etc/greetd/config.toml".to_owned(),
        ],
    };
    // greetd starts with no file, and so on no VT.
    let (file, text) = files
        .iter()
        .find_map(|f| read(root, f).map(|text| (format!("/{f}"), text)))?;
    let (value, source) = match (option(&words, "--vt", None), terminal_vt(&text)) {
        (Some(value), _) => (value.to_owned(), text::command_line(value, unit)),
        (None, Some(value)) => (value.clone(), format!("vt = {value} in {file}")),
        (None, None) => return None,
    };
    let n = match value.as_str() {
        "current" => 1,
        other => vt(other.parse().ok()?)?,
    };
    Some(Setting {
        effect: if value == "current" {
            text::current()
        } else {
            text::effect(n)
        },
        source,
        vt: n,
    })
}

#[cfg(test)]
mod tests {
    use super::super::tests::root;
    use super::*;

    fn vt_of(config: Option<Setting>) -> Option<u16> {
        config.map(|s| s.vt)
    }

    #[test]
    fn greetd_reads_greetd_conf_before_config_toml_and_unquotes() {
        let r = root(
            "greetd",
            &[
                ("etc/greetd/config.toml", "[terminal]\nvt = 1\n"),
                (
                    "etc/greetd/greetd.conf",
                    "# The VT\n[terminal]\nvt = \"2\"\n[default_session]\ncommand = \"agreety\"\n",
                ),
            ],
        );
        assert_eq!(vt_of(config(&r, "greetd.service", Some("greetd"))), Some(2));
        std::fs::remove_file(r.join("etc/greetd/greetd.conf")).ok();
        assert_eq!(vt_of(config(&r, "greetd.service", Some("greetd"))), Some(1));
        // current is the VT shown when greetd starts, VT 1 at boot; next
        // and none name no VT, and neither does a VT past the last.
        for (value, want) in [
            ("current", Some(1)),
            ("'next'", None),
            ("none", None),
            ("64", None),
            ("1 # first", None),
        ] {
            std::fs::write(
                r.join("etc/greetd/config.toml"),
                format!("[terminal]\nvt = {value}\n"),
            )
            .ok();
            assert_eq!(vt_of(config(&r, "greetd.service", None)), want, "{value}");
        }
        let _ = std::fs::remove_dir_all(r);
    }

    #[test]
    fn greetd_takes_vt_and_config_from_its_command_line() {
        let r = root(
            "greetd-exec",
            &[
                ("etc/greetd/config.toml", "[terminal]\nvt = 1\n"),
                ("etc/greetd/other.toml", "[terminal]\nvt = 4\n"),
            ],
        );
        let exec = |line| vt_of(config(&r, "greetd.service", Some(line)));
        assert_eq!(exec("/usr/bin/greetd --vt 3"), Some(3));
        assert_eq!(exec("-greetd --vt=5"), Some(5));
        assert_eq!(exec("greetd --config /etc/greetd/other.toml"), Some(4));
        assert_eq!(exec("greetd -c /etc/greetd/other.toml --vt next"), None);
        assert_eq!(exec("greetd --config /etc/greetd/missing.toml"), None);
        let _ = std::fs::remove_dir_all(r);
    }
}
