//! Whether vtium's drawing process can read vtium.conf.
//!
//! A console built with the feature `split` starts as root and runs its
//! drawing process as the system account `vtium`. The drawing process
//! reads vtium.conf itself, so a vtium.conf that root owns with mode 0600
//! passes the start's ownership check and still cannot be read by that
//! account. The drawing process then starts without the file's settings,
//! while the root process, which reads the login command and the power keys
//! from the same file, does not. check-config warns of such a file.
//!
//! The check reads the file's owner, group and mode, and for the installed
//! file the folders above it, and judges them as the kernel judges an
//! account's access: the owner's bits for the owner, the group's bits for
//! a member of the file's group, and the others' bits for anyone else. It
//! reads no access control list. Only a build with the feature `split`
//! makes the check, since only that build has a drawing process.
//!
//! References:
//! - ADR-1400 D4, "vtium.conf under the split": the drawer reads vtium.conf
//!   itself, and check-config warns when its account cannot read the file.

#[cfg(feature = "split")]
use std::os::unix::fs::MetadataExt;
#[cfg(feature = "split")]
use std::path::Path;

/// The account vtium's drawing process runs as, as the name service gives
/// it: its name, its user id, its group id and its supplementary groups.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reader {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
    pub groups: Vec<u32>,
}

/// The permission bit an account needs on a file or folder.
#[cfg(feature = "split")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Need {
    /// Read a file.
    Read,
    /// Open the entries of a folder.
    Search,
}

#[cfg(feature = "split")]
impl Reader {
    /// Whether this account may do `need` to a file or folder of owner
    /// `uid`, group `gid` and permission bits `mode`. The kernel uses only
    /// the owner's bits for the owner, and only the group's bits for a
    /// member of the group, so an owner whose own bits refuse is refused
    /// even where the others' bits allow.
    fn may(&self, uid: u32, gid: u32, mode: u32, need: Need) -> bool {
        let bit = match need {
            Need::Read => 0o4,
            Need::Search => 0o1,
        };
        let shift = if self.uid == uid {
            6
        } else if self.gid == gid || self.groups.contains(&gid) {
            3
        } else {
            0
        };
        (mode >> shift) & bit != 0
    }
}

/// The warning on `path` when the account `reader` cannot read it, or
/// `None` when it can, or when the file cannot be read at all, which
/// check-config reports apart. For the `installed` file, the one vtium
/// reads at start, each folder above it must let the account in as well. A
/// candidate is judged by its own owner, group and mode alone, since its
/// folder is not where vtium reads it.
#[cfg(feature = "split")]
pub fn finding(path: &Path, installed: bool, reader: &Reader) -> Option<String> {
    let f = path.display();
    let name = &reader.name;
    let file = std::fs::metadata(path).ok()?;
    if installed {
        let real = std::fs::canonicalize(path).ok()?;
        for folder in real.ancestors().skip(1) {
            let meta = std::fs::metadata(folder).ok()?;
            if !reader.may(meta.uid(), meta.gid(), meta.mode(), Need::Search) {
                return Some(format!(
                    "{f}: vtium's drawing process cannot read this file, so it starts without the file's settings. The drawing process runs as the account {name}, and the folder {} has mode {:04o}, which does not let that account open the files in it. Run `chmod 755 {}` to let every account open them.",
                    folder.display(),
                    meta.mode() & 0o7777,
                    folder.display(),
                ));
            }
        }
    }
    if reader.may(file.uid(), file.gid(), file.mode(), Need::Read) {
        return None;
    }
    let mode = file.mode() & 0o7777;
    Some(if installed {
        format!(
            "{f}: vtium's drawing process cannot read this file, so it starts without the file's settings. The drawing process runs as the account {name}, and the file's mode {mode:04o} does not let that account read it. Run `chmod 644 {f}` to let every account read the file."
        )
    } else {
        format!(
            "{f}: vtium's drawing process could not read this file once it is installed. The drawing process runs as the account {name}, and the file's mode {mode:04o} does not let that account read it. Install the file with mode 644 so that every account can read it."
        )
    })
}

#[cfg(all(test, feature = "split"))]
mod tests {
    use super::*;

    fn reader(uid: u32, gid: u32, groups: &[u32]) -> Reader {
        Reader {
            name: "vtium".to_owned(),
            uid,
            gid,
            groups: groups.to_vec(),
        }
    }

    // The account's access follows the kernel's order: the owner's bits for
    // the owner, the group's for a member, the others' for the rest.
    #[test]
    fn an_account_is_judged_by_the_owners_the_groups_or_the_others_bits() {
        let vtium = reader(990, 990, &[990, 5]);
        // root's file, mode 0600 and 0640: no
        assert!(!vtium.may(0, 0, 0o600, Need::Read));
        assert!(!vtium.may(0, 0, 0o640, Need::Read));
        // mode 0644: through the others' bits
        assert!(vtium.may(0, 0, 0o644, Need::Read));
        // mode 0640 with a group the account belongs to
        assert!(vtium.may(0, 5, 0o640, Need::Read));
        assert!(vtium.may(0, 990, 0o640, Need::Read));
        // its own file whose owner's bits refuse, though the others' allow
        assert!(!vtium.may(990, 0, 0o044, Need::Read));
        // a member whose group's bits refuse, though the others' allow
        assert!(!vtium.may(0, 5, 0o604, Need::Read));
        // folders: the search bit
        assert!(vtium.may(0, 0, 0o755, Need::Search));
        assert!(!vtium.may(0, 0, 0o750, Need::Search));
        assert!(!vtium.may(0, 0, 0o744, Need::Search));
    }

    // The installed file is judged with each folder above it: a file every
    // account can read, in a folder only its owner can open, is warned of,
    // and so is a file of mode 0600 in an open folder.
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn the_installed_file_and_each_folder_above_it_must_let_the_account_in() {
        use std::os::unix::fs::PermissionsExt;
        let me = rustix::process::geteuid().as_raw();
        let other = if me == 990 { 991 } else { 990 };
        let vtium = reader(other, other, &[other]);
        let dir = crate::scratch::Scratch::new("check-drawer-folders");
        let folder = dir.join("etc-vtium");
        std::fs::create_dir(&folder).unwrap();
        let file = folder.join("vtium.conf");
        std::fs::write(&file, "version = 1\n").unwrap();
        let mode = |p: &Path, m: u32| {
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(m)).unwrap();
        };
        mode(&dir, 0o755);
        mode(&file, 0o644);
        mode(&folder, 0o700);
        let said = finding(&file, true, &vtium).unwrap_or_default();
        assert!(
            said.contains(&format!(
                "the folder {} has mode 0700, which does not let that account open the files in it. Run `chmod 755 {}`",
                folder.display(),
                folder.display()
            )),
            "{said}"
        );
        // a candidate's folder is not where vtium reads it
        assert_eq!(finding(&file, false, &vtium), None);
        mode(&folder, 0o755);
        assert_eq!(finding(&file, true, &vtium), None);
        mode(&file, 0o600);
        let f = file.display();
        assert_eq!(
            finding(&file, true, &vtium),
            Some(format!(
                "{f}: vtium's drawing process cannot read this file, so it starts without the file's settings. The drawing process runs as the account vtium, and the file's mode 0600 does not let that account read it. Run `chmod 644 {f}` to let every account read the file."
            ))
        );
        // the account that owns the file reads it through the owner's bits
        assert_eq!(finding(&file, true, &reader(me, me, &[])), None);
        mode(&file, 0o644);
    }
}
