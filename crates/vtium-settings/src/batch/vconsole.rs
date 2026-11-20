//! The console font that the system's console configuration names, read
//! as systemd-vconsole-setup reads it.
//!
//! vconsole.conf is a shell-style file of `KEY=value` lines. vtium reads
//! `FONT=`, the console font, and `FONT_UNIMAP=`, its Unicode map, and
//! then the kernel command line's `vconsole.font=` and
//! `vconsole.font_unimap=`, which override them. The last assignment of
//! each wins, a value may be quoted with `"` or `'`, and an empty value
//! leaves the key unset. A line that starts with `#` or `;` is a comment.
//!
//! References:
//! - ADR-1205 D4: the fresh-install batch holds the console font that vconsole.conf's `FONT=` names.
//! - ADR-1204 D2: the console font comes before the default font set.

/// The console font and its Unicode map, as vconsole.conf and the kernel
/// command line name them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Vconsole {
    /// `FONT=`, or `vconsole.font=` over it.
    pub font: Option<String>,
    /// `FONT_UNIMAP=`, or `vconsole.font_unimap=` over it.
    pub unimap: Option<String>,
}

/// A value with one pair of surrounding quotes removed.
fn unquote(value: &str) -> &str {
    let value = value.trim();
    ['"', '\'']
        .iter()
        .find_map(|&q| value.strip_prefix(q).and_then(|v| v.strip_suffix(q)))
        .unwrap_or(value)
}

/// Reads `conf`, vconsole.conf's text, and `cmdline`, the kernel command
/// line, as systemd-vconsole-setup does.
pub fn vconsole(conf: &str, cmdline: &str) -> Vconsole {
    let set = |v: &str| Some(v.to_owned()).filter(|v| !v.is_empty());
    let mut out = Vconsole::default();
    for line in conf.lines().map(str::trim) {
        if line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        match line.split_once('=').map(|(k, v)| (k.trim(), unquote(v))) {
            Some(("FONT", v)) => out.font = set(v),
            Some(("FONT_UNIMAP", v)) => out.unimap = set(v),
            _ => {}
        }
    }
    for word in cmdline.split_whitespace() {
        if let Some(v) = word.strip_prefix("vconsole.font=") {
            out.font = set(v);
        } else if let Some(v) = word.strip_prefix("vconsole.font_unimap=") {
            out.unimap = set(v);
        }
    }
    out
}
