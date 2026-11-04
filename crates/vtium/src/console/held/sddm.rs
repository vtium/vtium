//! The VT SDDM's configuration adds, read as SDDM 0.21 reads its files.
//!
//! SDDM reads every file in `/usr/lib/sddm/sddm.conf.d`, then in
//! `/etc/sddm.conf.d`, each in name order, then `/etc/sddm.conf`, a later
//! value winning (`src/common/ConfigReader.cpp`). On each line it drops
//! everything from a `#` on, then trims the rest, so a `#` ends a section's
//! header too, and it reads the section `[XDisplay]`, which SDDM 0.14
//! renamed, as `[X11]`. Only `[X11] MinimumVT=`, which SDDM before 0.20 read
//! and sddm.conf(5) says SDDM 0.20 dropped, names a VT; vtium holds that VT
//! too, since it cannot tell which SDDM is installed. SDDM's other greeters
//! start on the VT its unit's `Conflicts=` names, or on the first VT no
//! program has open, which no file names.

use std::path::Path;

use super::{Setting, files, read, vt};
use crate::console::text::sddm as text;

/// The last `[X11] MinimumVT=` SDDM's files set, with its file.
fn minimum_vt(root: &Path) -> Option<(String, String)> {
    let mut found = None;
    for file in files(
        root,
        &["usr/lib/sddm/sddm.conf.d", "etc/sddm.conf.d"],
        None,
        "etc/sddm.conf",
    ) {
        let Some(text) = read(root, &file) else {
            continue;
        };
        let mut section = String::new();
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or_default().trim();
            if section == "XDisplay" {
                section = "X11".to_owned();
            }
            if let Some((key, value)) = line.split_once('=') {
                if section == "X11" && key.trim() == "MinimumVT" {
                    found = Some((value.trim().to_owned(), format!("/{file}")));
                }
            } else if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name.to_owned();
            }
        }
    }
    found
}

/// The VT SDDM's files under `root` add.
pub fn config(root: &Path) -> Option<Setting> {
    let (value, file) = minimum_vt(root)?;
    let n = vt(value.parse().ok()?)?;
    Some(Setting {
        source: format!("MinimumVT={n} in {file}"),
        effect: text::minimum_vt(n),
        vt: n,
    })
}

#[cfg(test)]
mod tests {
    use super::super::tests::root;
    use super::*;

    #[test]
    fn sddm_adds_the_vt_its_minimum_vt_names_as_sddm_reads_it() {
        let r = root(
            "sddm",
            &[
                (
                    "usr/lib/sddm/sddm.conf.d/default.conf",
                    "[General]\nDisplayServer=x11\n[X11]\nMinimumVT=2\n",
                ),
                (
                    "etc/sddm.conf.d/10.conf",
                    "[X11] # the server\n  MinimumVT = 3 # three\n",
                ),
            ],
        );
        let vt = |r: &Path| config(r).map(|s| s.vt);
        assert_eq!(vt(&r), Some(3));
        // [XDisplay] is [X11], and /etc/sddm.conf comes last.
        std::fs::write(r.join("etc/sddm.conf"), "[XDisplay]\nMinimumVT=4\n").ok();
        assert_eq!(vt(&r), Some(4));
        // A VT out of range, or in another section, names none.
        std::fs::write(r.join("etc/sddm.conf"), "[X11]\nMinimumVT=64\n").ok();
        assert_eq!(vt(&r), None);
        std::fs::write(
            r.join("etc/sddm.conf.d/10.conf"),
            "[General]\nMinimumVT=5\n",
        )
        .ok();
        std::fs::remove_file(r.join("etc/sddm.conf")).ok();
        assert_eq!(vt(&r), Some(2));
        let _ = std::fs::remove_dir_all(r);
    }
}
