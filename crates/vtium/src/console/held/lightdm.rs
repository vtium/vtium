//! The VT LightDM's configuration adds, read as LightDM reads its files.
//!
//! LightDM reads `lightdm/lightdm.conf.d/*.conf` under `/usr/share`,
//! `/usr/local/share` and `/etc/xdg`, then `/etc/lightdm/lightdm.conf.d/*.conf`,
//! then `/etc/lightdm/lightdm.conf`, a later value winning
//! (`common/configuration.c`), each as a GLib key file, whose comment lines
//! start with `#`. Its first greeter takes `[LightDM] minimum-vt`, 7 when no
//! file sets it (`src/lightdm.c`). LightDM reads a value that is no number
//! as 0, and takes 1 for any value below 1 (`src/vt.c`).

use std::path::Path;

use super::{Setting, files, read, vt};
use crate::console::text::lightdm as text;

/// The last `[LightDM] minimum-vt` LightDM's files set, with its file.
fn minimum_vt(root: &Path) -> Option<(String, String)> {
    let mut found = None;
    for file in files(
        root,
        &[
            "usr/share/lightdm/lightdm.conf.d",
            "usr/local/share/lightdm/lightdm.conf.d",
            "etc/xdg/lightdm/lightdm.conf.d",
            "etc/lightdm/lightdm.conf.d",
        ],
        Some(".conf"),
        "etc/lightdm/lightdm.conf",
    ) {
        let Some(text) = read(root, &file) else {
            continue;
        };
        let mut section = "";
        for line in text.lines().map(str::trim) {
            if line.starts_with('#') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name;
            } else if let Some((key, value)) = line.split_once('=')
                && section == "LightDM"
                && key.trim() == "minimum-vt"
            {
                found = Some((value.trim().to_owned(), format!("/{file}")));
            }
        }
    }
    found
}

/// The VT LightDM's files under `root` add: its first greeter's.
pub fn config(root: &Path) -> Option<Setting> {
    let (n, source) = match minimum_vt(root) {
        None => (7, text::DEFAULT.to_owned()),
        Some((value, file)) => match value.parse::<i64>() {
            Ok(n) => (n.max(1), format!("minimum-vt={value} in {file}")),
            Err(_) => (1, text::not_a_number(&value, &file)),
        },
    };
    let vt = vt(n)?;
    Some(Setting {
        source,
        effect: text::effect(vt),
        vt,
    })
}

#[cfg(test)]
mod tests {
    use super::super::tests::root;
    use super::*;

    #[test]
    fn lightdm_starts_on_vt_7_unless_a_file_says_otherwise() {
        let r = root("lightdm", &[]);
        assert_eq!(config(&r).map(|s| s.vt), Some(7));
        let r = root(
            "lightdm-set",
            &[
                (
                    "usr/share/lightdm/lightdm.conf.d/50-x.conf",
                    "[LightDM]\nminimum-vt=2\n",
                ),
                (
                    "etc/lightdm/lightdm.conf",
                    "[Seat:*]\nminimum-vt=9\n#[LightDM]\n",
                ),
            ],
        );
        assert_eq!(config(&r).map(|s| s.vt), Some(2));
        // LightDM reads a value that is no number as VT 1, and a VT past
        // the last is no VT.
        for (value, want) in [("seven", Some(1)), ("-3", Some(1)), ("64", None)] {
            std::fs::write(
                r.join("etc/lightdm/lightdm.conf"),
                format!("[LightDM]\nminimum-vt={value}\n"),
            )
            .ok();
            let got = config(&r).map(|s| s.vt);
            assert_eq!(got, want, "{value}");
        }
    }
}
