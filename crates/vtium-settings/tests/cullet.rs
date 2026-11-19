//! The trust rule on resource files, the mount check and the batch
//! (ADR-1205 D4, D10).
//!
//! No test runs as root, so each trusts its own user's files and root's
//! folders with [`Trust::owner`], below a fixture folder of its own with
//! [`Rule::below`]. The fixture's path is canonical, so no symbolic link
//! lies on it, wherever the target folder sits. The open calls read the
//! running system's mount table, where the fixture sits on a local
//! filesystem; the mount check's own tests feed it mount tables and sysfs
//! folders written for the test.

use std::fs;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use vtium_settings::Trust;
use vtium_settings::Untrusted;
use vtium_settings::batch::{
    BATCH_MAX, Batch, Entry, FRESH_FONTS, Finding, LineFault, Unread, vconsole,
};
use vtium_settings::cullet::{Check, Opened, Refused, Rule, open, open_in_theme};
use vtium_settings::mounts::{Mounts, Place, Unsteady};

/// A fresh fixture folder of its own for one test, mode 0755, by its
/// canonical path.
fn fixture(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("vtium-settings-cullet")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("the fixture folder");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).expect("mode 0755");
    fs::canonicalize(&dir).expect("the fixture's canonical path")
}

/// The test's own user, trusted below `dir`.
fn me(dir: &Path) -> Rule {
    let uid = fs::metadata(dir).expect("the fixture").uid();
    Rule::new(Trust::owner(uid)).below(dir)
}

fn mkdir(path: &Path, mode: u32) {
    fs::create_dir_all(path).expect("the folder");
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("its mode");
}

fn write(path: &Path, bytes: &[u8], mode: u32) {
    fs::write(path, bytes).expect("the file");
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("its mode");
}

fn system() -> Mounts {
    Mounts::system().expect("the running system's mount table")
}

fn text(opened: Opened) -> String {
    let mut s = String::new();
    let mut file = opened.file;
    file.read_to_string(&mut s).expect("the file's text");
    s
}

fn refused(result: Result<Opened, Refused>) -> Check {
    match result {
        Err(r) => r.check,
        Ok(o) => panic!("opened {:?}", o.meta),
    }
}

#[test]
fn a_root_owned_file_in_root_owned_folders_on_a_local_filesystem_passes() {
    let dir = fixture("good");
    mkdir(&dir.join("fonts"), 0o755);
    let file = dir.join("fonts/a.bdf");
    write(&file, b"STARTFONT", 0o644);
    let opened = open(&file, &me(&dir), &system()).expect("the file passes");
    assert_eq!(text(opened), "STARTFONT");
}

#[test]
fn a_file_its_group_can_write_fails_the_mode_check() {
    let dir = fixture("group-file");
    let file = dir.join("a.bdf");
    write(&file, b"x", 0o664);
    let check = refused(open(&file, &me(&dir), &system()));
    assert!(
        matches!(&check, Check::Untrusted(Untrusted::Writable { path, mode: 0o664 }) if path.raw() == file.to_str().unwrap_or_default()),
        "{check:?}"
    );
}

#[test]
fn a_folder_above_the_file_its_group_can_write_fails_the_mode_check() {
    let dir = fixture("group-folder");
    let folder = dir.join("fonts");
    mkdir(&folder, 0o775);
    let file = folder.join("a.bdf");
    write(&file, b"x", 0o644);
    let check = refused(open(&file, &me(&dir), &system()));
    assert!(
        matches!(&check, Check::Untrusted(Untrusted::Writable { path, .. }) if path.raw() == folder.to_str().unwrap_or_default()),
        "{check:?}"
    );
}

#[test]
fn a_file_another_user_owns_fails_the_owner_check() {
    let dir = fixture("owner");
    let file = dir.join("a.bdf");
    write(&file, b"x", 0o644);
    // as vtium reads it, running as root, a user's file is not root's
    let rule = Rule::new(Trust::ROOT).below(&dir);
    let check = refused(open(&file, &rule, &system()));
    assert!(
        matches!(check, Check::Untrusted(Untrusted::Owner { .. })),
        "{check:?}"
    );
}

#[test]
fn a_symbolic_link_as_the_file_fails() {
    let dir = fixture("link-file");
    let real = dir.join("real.bdf");
    write(&real, b"x", 0o644);
    let link = dir.join("a.bdf");
    symlink(&real, &link).expect("the link");
    assert_eq!(
        refused(open(&link, &me(&dir), &system())),
        Check::Link { at: link.clone() }
    );
    // the file the link names passes by its own path
    assert!(open(&real, &me(&dir), &system()).is_ok());
}

#[test]
fn a_symbolic_link_as_a_folder_on_the_path_fails() {
    let dir = fixture("link-folder");
    mkdir(&dir.join("real"), 0o755);
    write(&dir.join("real/a.bdf"), b"x", 0o644);
    let link = dir.join("fonts");
    symlink(dir.join("real"), &link).expect("the link");
    assert_eq!(
        refused(open(&link.join("a.bdf"), &me(&dir), &system())),
        Check::Link { at: link }
    );
}

#[test]
fn a_missing_file_a_relative_path_and_a_path_with_dotdot_fail() {
    let dir = fixture("plain");
    mkdir(&dir.join("fonts"), 0o755);
    write(&dir.join("a.bdf"), b"x", 0o644);
    let rule = me(&dir);
    assert_eq!(
        refused(open(&dir.join("none.bdf"), &rule, &system())),
        Check::Missing
    );
    assert_eq!(
        refused(open(Path::new("a.bdf"), &rule, &system())),
        Check::NotPlain
    );
    assert_eq!(
        refused(open(&dir.join("fonts/../a.bdf"), &rule, &system())),
        Check::NotPlain
    );
}

#[test]
fn a_fifo_or_a_folder_is_not_a_file_and_the_fifo_is_never_opened() {
    let dir = fixture("not-a-file");
    let fifo = dir.join("pipe");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo");
    assert!(made.success());
    fs::set_permissions(&fifo, fs::Permissions::from_mode(0o644)).expect("its mode");
    // a FIFO opened for reading would block until a writer comes; the
    // walk refuses it before it opens anything
    assert_eq!(
        refused(open(&fifo, &me(&dir), &system())),
        Check::NotAFile { at: fifo }
    );
    mkdir(&dir.join("fonts"), 0o755);
    assert_eq!(
        refused(open(&dir.join("fonts"), &me(&dir), &system())),
        Check::NotAFile {
            at: dir.join("fonts")
        }
    );
}

// The file read is the file checked: a file swapped in at the path after
// the open does not reach the reader, because the reader reads from the
// descriptor the checks ran on.
#[test]
fn the_file_read_is_the_file_checked_whatever_the_path_names_later() {
    let dir = fixture("swap");
    let file = dir.join("a.bdf");
    write(&file, b"checked", 0o644);
    let opened = open(&file, &me(&dir), &system()).expect("the file passes");
    let ino = fs::metadata(&file).expect("its metadata").ino();
    // whoever could write the folder swaps another file in, one the check
    // would refuse
    write(&dir.join("other"), b"swapped", 0o666);
    fs::rename(dir.join("other"), &file).expect("the swap");
    assert_eq!(opened.meta.ino(), ino);
    assert_eq!(text(opened), "checked");
}

// The mount check runs on the descriptor that was opened: a mount table
// that says the opened file's own mount is NFS refuses it.
#[test]
fn the_mount_check_reads_the_opened_files_own_mount() {
    let dir = fixture("mount-of-file");
    let file = dir.join("a.bdf");
    write(&file, b"x", 0o644);
    let real = system();
    let handle = fs::File::open(&file).expect("the file");
    let place = Place::of(&handle, &file).expect("where it sits");
    let mount = real.find(&place).expect("its mount").clone();
    let table = fs::read_to_string("/proc/self/mountinfo").expect("the mount table");
    let nfs: String = table
        .lines()
        .map(|l| {
            if l.split(' ').next() == Some(&mount.id.to_string()) {
                let (head, _) = l.split_once(" - ").expect("a mount line");
                format!("{head} - nfs4 server:/export rw\n")
            } else {
                format!("{l}\n")
            }
        })
        .collect();
    let mounts = Mounts::parse(&nfs, Path::new("/sys"));
    assert!(
        matches!(
            refused(open(&file, &me(&dir), &mounts)),
            Check::Unsteady(Unsteady::Network { .. })
        ),
        "the mount table names the file's own mount NFS"
    );
}

/// A pointer theme: `cursors/text`, a link `cursors/xterm` to `text`, a
/// link `cursors/up` to `../cursors/text`, an absolute link inside, and
/// links that leave the theme by `..` and by an absolute path.
fn theme(dir: &Path) -> PathBuf {
    let theme = dir.join("icons/Plain");
    mkdir(&theme.join("cursors"), 0o755);
    write(&theme.join("cursors/text"), b"Xcur", 0o644);
    mkdir(&dir.join("icons/Other/cursors"), 0o755);
    write(&dir.join("icons/Other/cursors/arrow"), b"Xcur", 0o644);
    let c = theme.join("cursors");
    symlink("text", c.join("xterm")).expect("xterm");
    symlink("../cursors/text", c.join("up")).expect("up");
    symlink(theme.join("cursors/text"), c.join("absolute")).expect("absolute");
    symlink("../../Other/cursors/arrow", c.join("leaves")).expect("leaves");
    symlink(dir.join("icons/Other/cursors/arrow"), c.join("away")).expect("away");
    symlink("cursors", theme.join("alias")).expect("alias");
    theme
}

#[test]
fn inside_a_pointer_theme_a_link_that_stays_inside_passes() {
    let dir = fixture("theme-inside");
    let theme = theme(&dir);
    let rule = me(&dir);
    for name in [
        "cursors/text",
        "cursors/xterm",
        "cursors/up",
        "cursors/absolute",
        "alias/xterm",
    ] {
        let opened = open_in_theme(&theme, &theme.join(name), &rule, &system())
            .unwrap_or_else(|r| panic!("{name}: {r}"));
        assert_eq!(text(opened), "Xcur", "{name}");
    }
    // the plain rule refuses the same link
    assert!(matches!(
        refused(open(&theme.join("cursors/xterm"), &rule, &system())),
        Check::Link { .. }
    ));
}

#[test]
fn inside_a_pointer_theme_a_link_that_leaves_the_theme_fails() {
    let dir = fixture("theme-leaves");
    let theme = theme(&dir);
    let rule = me(&dir);
    for name in ["cursors/leaves", "cursors/away"] {
        assert_eq!(
            refused(open_in_theme(&theme, &theme.join(name), &rule, &system())),
            Check::LeavesTheme {
                link: theme.join(name),
                theme: theme.clone()
            },
            "{name}"
        );
    }
    // a path outside the theme, and a theme folder reached by a link
    assert_eq!(
        refused(open_in_theme(
            &theme,
            &dir.join("icons/Other/cursors/arrow"),
            &rule,
            &system()
        )),
        Check::NotPlain
    );
    symlink(&theme, dir.join("icons/Linked")).expect("a linked theme");
    assert_eq!(
        refused(open_in_theme(
            &dir.join("icons/Linked"),
            &dir.join("icons/Linked/cursors/text"),
            &rule,
            &system()
        )),
        Check::Link {
            at: dir.join("icons/Linked")
        }
    );
}

#[test]
fn inside_a_pointer_theme_a_file_others_can_write_still_fails() {
    let dir = fixture("theme-mode");
    let theme = theme(&dir);
    fs::set_permissions(
        theme.join("cursors/text"),
        fs::Permissions::from_mode(0o666),
    )
    .expect("its mode");
    assert!(matches!(
        refused(open_in_theme(
            &theme,
            &theme.join("cursors/xterm"),
            &me(&dir),
            &system()
        )),
        Check::Untrusted(Untrusted::Writable { .. })
    ));
}

// The mount table of the fixtures below: an ext4 root, NFS, sshfs over
// FUSE, an NTFS disk over fuseblk, an automount point with a filesystem
// mounted below it, a removable disk's partition and a fixed disk's, and
// a mount point with a space in its name.
const MOUNTINFO: &str = "\
22 1 8:2 / / rw,relatime shared:1 - ext4 /dev/sda2 rw
30 22 0:40 / /mnt/nfs rw,relatime shared:2 - nfs4 server:/export rw,vers=4.2
31 22 0:41 / /mnt/ssh rw,nosuid shared:3 - fuse.sshfs me@host:/ rw,user_id=1000
32 22 8:33 / /mnt/ntfs rw shared:4 - fuseblk /dev/sdc1 rw
33 22 0:42 / /net rw,relatime shared:5 - autofs -hosts rw,fd=9,indirect
34 33 0:43 / /net/host/share rw shared:6 - ext4 /dev/sdd1 rw
35 22 8:17 / /media/stick rw,nosuid shared:7 - vfat /dev/sdb1 rw
36 22 8:1 / /boot rw shared:8 - vfat /dev/sda1 rw
37 22 8:18 / /media/my\\040disk rw shared:9 - ext4 /dev/sdb2 rw
";

/// A sysfs folder for the fixtures: sda, a fixed disk with partitions
/// sda1 and sda2, and sdb, a removable disk with partitions sdb1 and
/// sdb2, each reached from `dev/block/MAJ:MIN` by a link, as the kernel
/// lays sysfs out.
fn sysfs(dir: &Path) -> PathBuf {
    let sys = dir.join("sys");
    for (disk, removable, parts) in [
        ("sda", "0", [(1, "8:1"), (2, "8:2")]),
        ("sdb", "1", [(1, "8:17"), (2, "8:18")]),
    ] {
        let d = sys.join("devices/pci0000:00/usb1/block").join(disk);
        mkdir(&d, 0o755);
        fs::write(d.join("removable"), format!("{removable}\n")).expect("removable");
        for (n, dev) in parts {
            let p = d.join(format!("{disk}{n}"));
            mkdir(&p, 0o755);
            fs::write(p.join("partition"), format!("{n}\n")).expect("partition");
            mkdir(&sys.join("dev/block"), 0o755);
            symlink(&p, sys.join("dev/block").join(dev)).expect("the device link");
        }
    }
    sys
}

fn place(dev: (u32, u32), path: &str) -> Place {
    Place {
        mount_id: None,
        dev,
        path: PathBuf::from(path),
    }
}

#[test]
fn the_mount_check_refuses_network_fuse_automounted_and_removable_filesystems() {
    let dir = fixture("mounts");
    let mounts = Mounts::parse(MOUNTINFO, &sysfs(&dir));
    let judge = |dev, path| mounts.judge(&place(dev, path));
    assert_eq!(judge((8, 2), "/usr/share/fonts/a.bdf"), Ok(()));
    assert_eq!(judge((8, 1), "/boot/a.bdf"), Ok(()));
    assert_eq!(
        judge((0, 40), "/mnt/nfs/a.bdf"),
        Err(Unsteady::Network {
            point: "/mnt/nfs".into(),
            fstype: "nfs4".into()
        })
    );
    assert_eq!(
        judge((0, 41), "/mnt/ssh/a.bdf"),
        Err(Unsteady::Fuse {
            point: "/mnt/ssh".into(),
            fstype: "fuse.sshfs".into()
        })
    );
    assert_eq!(
        judge((8, 33), "/mnt/ntfs/a.bdf"),
        Err(Unsteady::Fuse {
            point: "/mnt/ntfs".into(),
            fstype: "fuseblk".into()
        })
    );
    assert_eq!(
        judge((0, 42), "/net/a.bdf"),
        Err(Unsteady::Automount {
            point: "/net".into()
        })
    );
    assert_eq!(
        judge((0, 43), "/net/host/share/a.bdf"),
        Err(Unsteady::BelowAutomount {
            point: "/net/host/share".into(),
            automount: "/net".into()
        })
    );
    assert_eq!(
        judge((8, 17), "/media/stick/a.bdf"),
        Err(Unsteady::Removable {
            point: "/media/stick".into(),
            source: "/dev/sdb1".into()
        })
    );
    assert_eq!(
        judge((8, 18), "/media/my disk/a.bdf"),
        Err(Unsteady::Removable {
            point: "/media/my disk".into(),
            source: "/dev/sdb2".into()
        })
    );
}

#[test]
fn the_root_filesystem_on_a_removable_disk_passes() {
    let dir = fixture("mounts-root");
    let table = "22 1 8:17 / / rw - ext4 /dev/sdb1 rw\n23 22 8:18 / /home rw - ext4 /dev/sdb2 rw\n";
    let mounts = Mounts::parse(table, &sysfs(&dir));
    assert_eq!(mounts.judge(&place((8, 17), "/usr/share/a")), Ok(()));
    assert!(matches!(
        mounts.judge(&place((8, 18), "/home/a")),
        Err(Unsteady::Removable { .. })
    ));
}

// btrfs reports a subvolume's own device number, which the mount table
// does not hold: vtium finds the mount by its mount ID, and without one
// by its device number, then by the longest mount point above the path
// whose own folder has the file's device number.
#[test]
fn the_mount_is_found_by_its_id_then_its_device_then_its_mount_point() {
    let dir = fixture("find");
    let meta = fs::metadata(&dir).expect("the fixture");
    let here = (rustix::fs::major(meta.dev()), rustix::fs::minor(meta.dev()));
    let table = format!(
        "\
33 2 0:30 /@ / rw - btrfs /dev/nvme0n1p2 rw
55 33 0:30 /@home /home rw - btrfs /dev/nvme0n1p2 rw
60 33 0:50 / /srv rw - nfs4 server:/srv rw
61 33 0:31 /@sub {} rw - btrfs /dev/nvme0n1p2 rw
",
        dir.display()
    );
    let mounts = Mounts::parse(&table, Path::new("/nonexistent-sys"));
    let by_id = |id, path: &str| Place {
        mount_id: Some(id),
        dev: (0, 99),
        path: path.into(),
    };
    assert_eq!(mounts.find(&by_id(55, "/home/x")).map(|m| m.id), Some(55));
    // a line whose mount point is not above the path is stale
    assert_eq!(mounts.find(&by_id(55, "/srv/x")).map(|m| m.id), None);
    // the same device mounted twice: the mount point above the path wins
    assert_eq!(
        mounts.find(&place((0, 30), "/home/a")).map(|m| m.id),
        Some(55)
    );
    assert_eq!(
        mounts.find(&place((0, 30), "/usr/a")).map(|m| m.id),
        Some(33)
    );
    // a subvolume's device number the table lacks: the mount point above
    // the path counts only when its own folder has the file's device
    let file = dir.join("a.bdf");
    let file = file.to_str().expect("a UTF-8 fixture path");
    assert_eq!(mounts.find(&place(here, file)).map(|m| m.id), Some(61));
    assert_eq!(mounts.find(&place((0, 99), file)).map(|m| m.id), None);
    assert_eq!(mounts.find(&place((0, 61), "/srv/x")).map(|m| m.id), None);
    assert_eq!(
        Mounts::parse("", Path::new("/")).judge(&place((0, 61), "/usr/x")),
        Err(Unsteady::Unknown {
            path: "/usr/x".into()
        })
    );
}

// A mount table read from a file is read again once when a file's mount is
// missing, so a mount made after the first read is judged by its own line.
#[test]
fn a_mount_missing_from_the_table_is_looked_up_again_in_the_file() {
    let dir = fixture("read-again");
    let info = dir.join("mountinfo");
    fs::write(&info, "22 1 8:2 / / rw - ext4 /dev/sda2 rw\n").expect("the table");
    let mounts = Mounts::read(&info, Path::new("/nonexistent-sys")).expect("the table");
    let at = |id| Place {
        mount_id: Some(id),
        dev: (0, 77),
        path: "/run/media/me/net/x.ttf".into(),
    };
    fs::write(
        &info,
        "22 1 8:2 / / rw - ext4 /dev/sda2 rw\n40 22 0:77 / /run/media/me/net rw - nfs4 server:/x rw\n",
    )
    .expect("the table with a newer mount");
    assert_eq!(
        mounts.judge(&at(40)),
        Err(Unsteady::Network {
            point: "/run/media/me/net".into(),
            fstype: "nfs4".into()
        })
    );
    assert_eq!(
        mounts.judge(&at(41)),
        Err(Unsteady::Unknown {
            path: "/run/media/me/net/x.ttf".into()
        })
    );
}

#[test]
fn a_refusal_names_the_file_and_the_check_it_failed() {
    let refusal = Refused {
        path: "/usr/share/fonts/a.bdf".into(),
        check: Check::Link {
            at: "/usr/share/fonts".into(),
        },
    };
    assert_eq!(
        refusal.to_string(),
        "/usr/share/fonts/a.bdf: vtium treats this file as missing, because /usr/share/fonts is a symbolic link, and vtium uses no resource reached through one."
    );
    let untrusted = Refused {
        path: "/a".into(),
        check: Check::Untrusted(Untrusted::Owner {
            path: vtium_settings::Quoted::path(Path::new("/a")),
            uid: 1000,
        }),
    };
    assert!(
        untrusted.to_string().ends_with("make root the owner."),
        "{untrusted}"
    );
    assert!(!untrusted.to_string().contains(".."), "{untrusted}");
}

#[test]
fn a_batch_file_lists_absolute_paths_with_comments_and_blank_lines() {
    let text = b"# the active fonts\n\n/usr/share/fonts/a.ttf\n  /usr/share/fonts//b.ttf  \nfonts/c.ttf\n/usr/share/../etc/d\n/usr/./share/e\n";
    let read = Batch::parse(text, Path::new("/etc/vtium/batch.conf"));
    assert_eq!(
        read.batch.entries(),
        [
            Entry::Path("/usr/share/fonts/a.ttf".into()),
            Entry::Path("/usr/share/fonts/b.ttf".into()),
            Entry::Path("/usr/share/e".into()),
        ]
    );
    let line = |line, fault| Finding::Line {
        path: "/etc/vtium/batch.conf".into(),
        line,
        fault,
    };
    assert_eq!(
        read.findings,
        [
            line(5, LineFault::NotAbsolute),
            line(6, LineFault::ParentStep)
        ]
    );
    assert!(read.batch.holds(Path::new("/usr/share/fonts/b.ttf")));
    assert!(read.batch.holds(Path::new("/usr//share/./fonts/a.ttf")));
    assert!(!read.batch.holds(Path::new("/usr/share/fonts/c.ttf")));
    assert!(!read.batch.holds(Path::new("fonts/c.ttf")));
    assert!(!read.batch.holds(Path::new("/usr/share/fonts/x/../a.ttf")));
}

#[test]
fn a_batch_line_longer_than_a_path_or_not_utf8_is_ignored() {
    let mut text = b"/a\n/".to_vec();
    text.extend(std::iter::repeat_n(b'x', 4096));
    text.extend(b"\n/b\xff\n/c\n");
    let read = Batch::parse(&text, Path::new("/b"));
    assert_eq!(
        read.batch.entries(),
        [Entry::Path("/a".into()), Entry::Path("/c".into())]
    );
    assert_eq!(read.findings.len(), 2);
    assert!(matches!(
        read.findings[0],
        Finding::Line {
            line: 2,
            fault: LineFault::TooLong,
            ..
        }
    ));
    assert!(matches!(
        read.findings[1],
        Finding::Line {
            line: 3,
            fault: LineFault::NotUtf8,
            ..
        }
    ));
}

fn fresh() -> Batch {
    Batch::new(vec![Entry::Path("/fresh".into())])
}

#[test]
fn the_batch_file_is_read_under_vtium_confs_check_and_its_bound() {
    let dir = fixture("batch-read");
    let uid = fs::metadata(&dir).expect("the fixture").uid();
    let trust = Trust::owner(uid).below(&dir);
    let file = dir.join("batch.conf");

    let missing = Batch::read(&file, &trust, fresh());
    assert_eq!(missing.batch, fresh());
    assert_eq!(missing.findings, [Finding::Missing { path: file.clone() }]);

    write(&file, b"/usr/share/fonts/a.ttf\n", 0o644);
    let good = Batch::read(&file, &trust, fresh());
    assert!(good.findings.is_empty());
    assert!(good.batch.holds(Path::new("/usr/share/fonts/a.ttf")));
    assert!(!good.batch.holds(Path::new("/fresh")));

    fs::set_permissions(&file, fs::Permissions::from_mode(0o664)).expect("its mode");
    let open = Batch::read(&file, &trust, fresh());
    assert_eq!(open.batch, fresh());
    assert!(matches!(
        &open.findings[..],
        [Finding::Refused {
            why: Unread::Untrusted(Untrusted::Writable { .. }),
            ..
        }]
    ));

    // comment lines of 64 bytes each, exactly the bound in all
    let mut long = [b"#".repeat(63), b"\n".to_vec()]
        .concat()
        .repeat(BATCH_MAX / 64);
    assert_eq!(long.len(), BATCH_MAX);
    write(&file, &long, 0o644);
    assert!(Batch::read(&file, &trust, fresh()).findings.is_empty());
    long.push(b'x');
    write(&file, &long, 0o644);
    let large = Batch::read(&file, &trust, fresh());
    assert_eq!(large.batch, fresh());
    assert_eq!(
        large.findings,
        [Finding::Refused {
            path: file.clone(),
            why: Unread::TooLarge
        }]
    );
}

#[test]
fn the_fresh_install_list_holds_the_shipped_fonts_the_console_font_and_the_shipped_palettes() {
    let dir = fixture("fresh");
    let palettes = dir.join("palettes");
    mkdir(&palettes, 0o755);
    for name in ["Kiln", "Dracula"] {
        write(&palettes.join(name), b"palette = 0=#000000\n", 0o644);
    }
    mkdir(&palettes.join("folder"), 0o755);
    let fonts = Path::new("/usr/share/vtium/fonts");
    let batch = Batch::fresh_install(fonts, &palettes, Some("ter-v16n"));
    for f in FRESH_FONTS {
        assert!(batch.holds(&fonts.join(f)), "{f}");
    }
    assert!(batch.holds(&palettes.join("Kiln")));
    assert!(batch.holds(&palettes.join("Dracula")));
    assert!(!batch.holds(&palettes.join("folder")));
    assert!(batch.holds(Path::new("/usr/share/kbd/consolefonts/ter-v16n.psf.gz")));
    assert!(batch.holds(Path::new("/usr/lib/kbd/consolefonts/ter-v16n.psfu")));
    assert!(!batch.holds(Path::new("/usr/share/kbd/consolefonts/ter-v32n.psf.gz")));
    assert!(!batch.holds(Path::new("/home/me/ter-v16n.psf.gz")));
    assert!(!batch.holds(Path::new("/usr/share/vtium/fonts/other.ttf")));
    // the palettes are listed after the fonts, sorted by name
    assert_eq!(
        &batch.entries()[FRESH_FONTS.len() + 1..],
        [
            Entry::Path(palettes.join("Dracula")),
            Entry::Path(palettes.join("Kiln"))
        ]
    );

    // a console font named by its path, no console font, no palettes
    let by_path = Batch::fresh_install(
        fonts,
        &dir.join("none"),
        Some("/usr/share/kbd/consolefonts/x.psf"),
    );
    assert!(by_path.holds(Path::new("/usr/share/kbd/consolefonts/x.psf")));
    assert_eq!(by_path.entries().len(), FRESH_FONTS.len() + 1);
    assert_eq!(
        Batch::fresh_install(fonts, &dir.join("none"), None)
            .entries()
            .len(),
        FRESH_FONTS.len()
    );
}

/// The words of a Makefile variable's value, with `$(foreach h,...,...)`
/// expanded for the one form the font list uses.
fn makefile_words(makefile: &str, name: &str) -> Vec<String> {
    let line = makefile
        .lines()
        .find_map(|l| l.strip_prefix(&format!("{name} := ")))
        .unwrap_or_else(|| panic!("the Makefile sets {name}"));
    let mut words = Vec::new();
    let mut rest = line;
    if let Some(body) = rest.strip_prefix("$(foreach h,") {
        let (sizes, after) = body.split_once(',').expect("the sizes");
        let (pattern, tail) = after.rsplit_once(')').expect("the pattern");
        for h in sizes.split_whitespace() {
            for w in pattern.split_whitespace() {
                words.push(w.replace("$(h)", h));
            }
        }
        rest = tail;
    }
    words.extend(rest.split_whitespace().map(str::to_owned));
    words
}

// The fresh-install list names the font files `make install` and `make
// install-tree` put in /usr/share/vtium/fonts, so a font the Makefile
// adds or renames fails here until the list follows it.
#[test]
fn the_fresh_fonts_are_the_files_the_makefile_installs() {
    let makefile = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Makefile"))
        .expect("the Makefile");
    let mut installed = makefile_words(&makefile, "FONT_FILE_NAMES");
    installed.extend(makefile_words(&makefile, "CULLET_MONO_FACES"));
    installed.sort();
    let mut listed: Vec<String> = FRESH_FONTS.iter().map(|s| (*s).to_owned()).collect();
    listed.sort();
    assert_eq!(listed, installed);
}

#[test]
fn vconsole_conf_is_read_as_systemd_vconsole_setup_reads_it() {
    let conf = "# comment\n; FONT=semi\nKEYMAP=us\nFONT=lat2-16\n FONT = \"ter-v16n\" \nFONT_UNIMAP='8859-2'\n";
    let v = vconsole(conf, "");
    assert_eq!(v.font.as_deref(), Some("ter-v16n"));
    assert_eq!(v.unimap.as_deref(), Some("8859-2"));
    // the kernel command line overrides the file, the last word winning
    let v = vconsole(
        conf,
        "quiet vconsole.font=a vconsole.font=ter-v32n vconsole.font_unimap=u",
    );
    assert_eq!(v.font.as_deref(), Some("ter-v32n"));
    assert_eq!(v.unimap.as_deref(), Some("u"));
    // an empty value leaves the key unset
    assert_eq!(vconsole("FONT=x\nFONT=\n", "").font, None);
    assert_eq!(vconsole("FONT=x\n", "vconsole.font=").font, None);
    assert_eq!(vconsole("#FONT=x\n", "").font, None);
}

// A mount made after vtium read the mount table, such as a USB stick that
// udisks mounts later, has a mount ID the table lacks. vtium never judges
// such a file by another line, such as `/`'s.
#[test]
fn a_mount_id_the_table_lacks_fails_closed() {
    let mounts = Mounts::parse("22 1 8:2 / / rw - ext4 /dev/sda2 rw\n", Path::new("/sys"));
    for dev in [(0, 77), (8, 17), (8, 2)] {
        let place = Place {
            mount_id: Some(99),
            dev,
            path: "/media/usb/fonts/x.ttf".into(),
        };
        assert_eq!(
            mounts.judge(&place),
            Err(Unsteady::Unknown {
                path: "/media/usb/fonts/x.ttf".into()
            }),
            "{dev:?}"
        );
    }
}
