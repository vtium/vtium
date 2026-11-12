//! `preview FONT...` (ADR-1201 D9): each font's specimen drawn by the glyph
//! helper under its own sandbox, printed as a text summary where the
//! terminal draws no pictures, and as kitty graphics' raw pixels where it
//! answers the graphics query: the console's own terminal built with
//! `images`, under the `kitty-check` feature.

#[path = "common/home.rs"]
mod home;

use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};

const BIN: &str = env!("CARGO_BIN_EXE_vtium-config");

/// A configuration directory holding `conf` as vtium.conf, if any, and the
/// shipped frits.
fn fixture(name: &str, conf: Option<&str>) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("preview")
        .join(name);
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("a fresh fixture");
    }
    std::fs::create_dir_all(&root).expect("the fixture");
    if let Some(conf) = conf {
        std::fs::write(root.join("vtium.conf"), conf).expect("vtium.conf");
    }
    root
}

fn frits() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("frits")
}

/// `preview` with `args`, its output a pipe: its status and output.
fn preview(root: &Path, args: &[&str]) -> (i32, String, String) {
    let o = home::command(BIN)
        .arg("--configdir")
        .arg(root)
        .arg("--frits")
        .arg(frits())
        .arg("--font-dir")
        .arg("/nonexistent")
        .arg("--sys")
        .arg("/nonexistent")
        .arg("preview")
        .args(args)
        .env("LANG", "C")
        .output()
        .expect("the binary runs");
    (
        o.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

// covers: sgr.script
#[test]
fn away_from_a_terminal_each_font_is_a_line_of_text() {
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped: {why}");
        return;
    }
    let root = fixture("text", None);
    let (status, out, err) = preview(&root, &["unifont", "unscii-8", "--size", "12"]);
    assert_eq!(status, 0, "{err}");
    let lines: Vec<&str> = out.lines().collect();
    // the detailed specimen's 323 characters (ADR-1201 D4): unifont draws
    // them all, and unscii-8 lacks 27, the CJK line's among them; then how
    // the helper made the superscripts and subscripts (ADR-1202 D14):
    // unifont's at its own size scaled, and unscii-8's, drawn at 2x in the
    // 16-pixel row, from unscii-8 at 1x
    assert_eq!(
        lines,
        [
            "unifont\t12 pt\tcharacter cell 8x16\tcan display 323 of the preview's 323 characters\tSuperscripts and subscripts are this font's own characters, scaled to two thirds of their size. Each pixel stays fully on or fully off, so the edges stay sharp.",
            "unscii-8\t12 pt\tcharacter cell 8x8\tcan display 296 of the preview's 323 characters\tSuperscripts and subscripts come from this font at its own size, a smaller size than its other characters, which are drawn at 2 times its size.",
        ]
    );
    assert!(!out.contains('\x1b'), "no picture to a pipe");
}

#[test]
fn the_size_is_vtium_conf_s_font_size_else_twelve_points() {
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped: {why}");
        return;
    }
    let (_, out, _) = preview(&fixture("conf-size", Some("font-size=20\n")), &["unifont"]);
    assert!(out.starts_with("unifont\t20 pt\t"), "{out}");
    let (_, out, _) = preview(&fixture("auto-size", Some("font-size=0\n")), &["unifont"]);
    assert!(out.starts_with("unifont\t12 pt\t"), "{out}");
}

#[test]
fn a_font_this_machine_lacks_fails_and_the_others_still_draw() {
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped: {why}");
        return;
    }
    let root = fixture("missing", None);
    let (status, out, _) = preview(&root, &["No Such Family Anywhere", "unifont"]);
    assert_eq!(status, 1, "{out}");
    let lines: Vec<&str> = out.lines().collect();
    assert!(
        lines[0].starts_with("failed\tNo Such Family Anywhere\t"),
        "{out}"
    );
    assert!(lines[1].starts_with("unifont\t"), "{out}");
}

/// A sysfs holding one connected display of `pixels` whose EDID states
/// `cm`, and a disconnected one before it.
fn sysfs(root: &Path, pixels: &str, cm: (u8, u8)) -> PathBuf {
    let sys = root.join("sys");
    let drm = sys.join("class/drm");
    let off = drm.join("card0-DP-1");
    std::fs::create_dir_all(&off).expect("sysfs");
    std::fs::write(off.join("status"), "disconnected\n").expect("status");
    let on = drm.join("card0-HDMI-A-1");
    std::fs::create_dir_all(&on).expect("sysfs");
    std::fs::write(on.join("status"), "connected\n").expect("status");
    std::fs::write(on.join("modes"), format!("{pixels}\n1280x720\n")).expect("modes");
    let mut edid = vec![0u8; 128];
    edid[..8].copy_from_slice(&[0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0]);
    edid[21] = cm.0;
    edid[22] = cm.1;
    let sum = edid.iter().fold(0u8, |s, &b| s.wrapping_add(b));
    edid[127] = 0u8.wrapping_sub(sum);
    std::fs::write(on.join("edid"), edid).expect("edid");
    sys
}

/// A display's EDID sets the density where vtium.conf names none, and its
/// first mode the grid a cell gives.
#[test]
fn the_display_s_edid_sets_the_density_and_its_mode_the_grid() {
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped: {why}");
        return;
    }
    let root = fixture("edid", None);
    let sys = sysfs(&root, "1920x1080", (30, 17));
    let sys = sys.to_str().expect("a path");
    // 1080 pixels over 170 mm is 161 dpi, so 12 pt is 27 pixels: the
    // outline cell the W rule gives is what a bitmap frit cannot follow,
    // so unifont keeps its 8x16 and the grid is 240x67; 27 pixels hold
    // unifont at 1x alone, so its superscripts and subscripts are scaled
    let (status, out, err) = preview(&root, &["--sys", sys, "unifont"]);
    assert_eq!(status, 0, "{err}");
    assert_eq!(
        out,
        "unifont\t12 pt\tcharacter cell 8x16, console 240x67 characters\tcan display 323 of the preview's 323 characters\tSuperscripts and subscripts are this font's own characters, scaled to two thirds of their size. Each pixel stays fully on or fully off, so the edges stay sharp.\n"
    );
}

/// The locale adds a line in its script to the specimen, which a font
/// with no glyph for it draws less of.
#[test]
fn the_locale_adds_a_line_in_its_script() {
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped: {why}");
        return;
    }
    let root = fixture("locale", None);
    let o = home::command(BIN)
        .arg("--configdir")
        .arg(&root)
        .arg("--frits")
        .arg(frits())
        .args(["--sys", "/nonexistent", "preview", "unifont", "unscii-8"])
        .env("LANG", "ru_RU.UTF-8")
        .env_remove("LC_ALL")
        .env_remove("LC_CTYPE")
        .output()
        .expect("the binary runs");
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 2, "{out}");
    // the Russian line's 18 letters the detailed specimen's Cyrillic line
    // lacks, after its 323 characters
    assert!(
        lines[0].contains("\tcan display 341 of the preview's 341 characters\t"),
        "{out}"
    );
    assert!(
        lines[1].contains("of the preview's 341 characters"),
        "{out}"
    );
}

#[test]
fn preview_s_usage_errors_exit_2() {
    let root = fixture("usage", None);
    for args in [&[][..], &["--size", "0", "unifont"], &["--bold", "unifont"]] {
        let (status, _, err) = preview(&root, args);
        assert_eq!(status, 2, "{args:?}: {err}");
        assert!(err.contains("--help"), "{args:?}: {err}");
    }
}

/// A pty's master and its slave, neither the test's controlling terminal.
fn pair() -> (File, File) {
    let master =
        openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC).expect("a pty");
    grantpt(&master).expect("grantpt");
    unlockpt(&master).expect("unlockpt");
    let name = ptsname(&master, Vec::new()).expect("ptsname");
    let slave = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(rustix::fs::OFlags::NOCTTY.bits() as i32)
        .open(name.to_str().expect("a path"))
        .expect("the slave");
    (File::from(master), slave)
}

/// `preview unifont` on a pty whose other end answers nothing, or DA1
/// alone: its status and every byte it wrote.
fn on_a_terminal(answers_da1: bool) -> (i32, String) {
    use std::io::{Read, Write};
    use std::os::fd::AsFd;
    use std::process::Stdio;
    let root = fixture(if answers_da1 { "da1-only" } else { "silent" }, None);
    let (mut master, slave) = pair();
    let io = || Stdio::from(slave.as_fd().try_clone_to_owned().expect("dup"));
    let mut child = home::command("setsid")
        .arg("--ctty")
        .arg(BIN)
        .arg("--configdir")
        .arg(&root)
        .arg("--frits")
        .arg(frits())
        .args(["--sys", "/nonexistent", "preview", "unifont"])
        .env("LANG", "C")
        .stdin(io())
        .stdout(io())
        .stderr(io())
        .spawn()
        .expect("setsid runs");
    drop(slave);
    let mut written = Vec::new();
    let mut buf = [0u8; 4096];
    while let Ok(n @ 1..) = master.read(&mut buf) {
        written.extend_from_slice(&buf[..n]);
        if answers_da1
            && buf[..n].windows(3).any(|w| w == b"\x1b[c")
            && master.write_all(b"\x1b[?62c").is_err()
        {
            break;
        }
    }
    let status = child.wait().expect("wait").code().unwrap_or(-1);
    (status, String::from_utf8_lossy(&written).into_owned())
}

/// A terminal that answers nothing, or DA1 alone, draws no kitty graphics:
/// preview asks, waits at most its second, and prints the line of text
/// alone.
#[test]
fn a_terminal_that_answers_nothing_or_da1_alone_gets_no_picture() {
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped: {why}");
        return;
    }
    for answers_da1 in [false, true] {
        let (status, written) = on_a_terminal(answers_da1);
        assert_eq!(status, 0, "{written:?}");
        assert!(
            written.contains("\x1b_Gi=31,s=1,v=1,a=q"),
            "asked: {written:?}"
        );
        assert!(
            written.contains("unifont\t12 pt\tcharacter cell 8x16"),
            "{written:?}"
        );
        assert!(!written.contains("a=T"), "no picture: {written:?}");
    }
}

#[cfg(feature = "kitty-check")]
mod pictures {
    use std::io::{Read, Write};
    use std::os::fd::AsFd;
    use std::process::Stdio;

    use vtium_term::images::{Kitty, Switches};
    use vtium_term::{ImageString, Terminal};

    use super::*;

    /// vtium's terminal with kitty graphics on, as a console built with
    /// `images` sets it up.
    fn term() -> Terminal {
        let mut t = Terminal::new(80, 25, 0);
        t.screen_mut().set_image_limit(1 << 26);
        t.screen_mut().set_image_cell(8, 16).expect("a cell");
        t.set_image_switches(Switches {
            sixel: false,
            kitty: true,
            iterm2: false,
        });
        t.set_image_handler(ImageString::Kitty, Some(Box::new(Kitty::default())));
        t
    }

    /// Each font is a line and a picture that moves the cursor past it,
    /// each its own image, which the console places.
    #[test]
    fn at_a_terminal_that_draws_kitty_graphics_each_font_is_also_a_picture() {
        if let Some(why) = vtium_testscript::qemu_without_seccomp() {
            eprintln!("skipped: {why}");
            return;
        }
        let root = fixture("pictures", None);
        let (mut master, slave) = pair();
        let io = || Stdio::from(slave.as_fd().try_clone_to_owned().expect("dup"));
        let mut child = home::command("setsid")
            .arg("--ctty")
            .arg(BIN)
            .arg("--configdir")
            .arg(&root)
            .arg("--frits")
            .arg(frits())
            .args(["preview", "unifont", "unscii-8", "--size", "12"])
            .env("LANG", "C")
            .stdin(io())
            .stdout(io())
            .stderr(io())
            .spawn()
            .expect("setsid runs");
        drop(slave);
        // the console answers until the program's end closes the pty
        let mut t = term();
        let mut written = Vec::new();
        let mut buf = [0u8; 4096];
        while let Ok(n @ 1..) = master.read(&mut buf) {
            written.extend_from_slice(&buf[..n]);
            let mut rest = &buf[..n];
            while !rest.is_empty() {
                let taken = t.input(rest);
                rest = &rest[taken..];
            }
            let replies = t.take_replies();
            if !replies.is_empty() && master.write_all(&replies).is_err() {
                break;
            }
        }
        assert!(child.wait().expect("wait").success());
        let text = String::from_utf8_lossy(&written).into_owned();
        for id in [12_010, 12_011] {
            let head = format!("\x1b_Ga=T,f=32,t=d,i={id},s=");
            let at = text
                .find(&head)
                .unwrap_or_else(|| panic!("image {id}: {text:?}"));
            let rest = &text[at..];
            let keys = &rest[..rest.find(';').expect("the keys end")];
            assert!(!keys.contains("C=1"), "the cursor moves: {keys}");
        }
        assert_eq!(t.screen().placements().len(), 2);
        assert!(
            text.contains("unifont\t12 pt\tcharacter cell 8x16"),
            "{text:?}"
        );
    }
}
