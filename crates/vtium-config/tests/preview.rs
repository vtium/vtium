//! `preview FONT...` (ADR-1201 D9): each font's specimen drawn by the glyph
//! helper under its own sandbox, printed as a text summary where the
//! terminal draws no pictures, and as kitty graphics' raw pixels where it
//! answers the graphics query: the console's own terminal built with
//! `images`, under the `kitty-check` feature.

use std::path::{Path, PathBuf};
use std::process::Command;

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
    let o = Command::new(BIN)
        .arg("--configdir")
        .arg(root)
        .arg("--frits")
        .arg(frits())
        .arg("--font-dir")
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

#[test]
fn away_from_a_terminal_each_font_is_a_line_of_text() {
    let root = fixture("text", None);
    let (status, out, err) = preview(&root, &["unifont", "unscii-8", "--size", "12"]);
    assert_eq!(status, 0, "{err}");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines,
        [
            "unifont\t12 pt\tcell 8x16\tdraws 51 of the specimen's 51 characters",
            "unscii-8\t12 pt\tcell 8x8\tdraws 51 of the specimen's 51 characters",
        ]
    );
    assert!(!out.contains('\x1b'), "no picture to a pipe");
}

#[test]
fn the_size_is_vtium_conf_s_font_size_else_twelve_points() {
    let (_, out, _) = preview(&fixture("conf-size", Some("font-size=20\n")), &["unifont"]);
    assert!(out.starts_with("unifont\t20 pt\t"), "{out}");
    let (_, out, _) = preview(&fixture("auto-size", Some("font-size=0\n")), &["unifont"]);
    assert!(out.starts_with("unifont\t12 pt\t"), "{out}");
}

#[test]
fn a_font_this_machine_lacks_fails_and_the_others_still_draw() {
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

#[test]
fn preview_s_usage_errors_exit_2() {
    let root = fixture("usage", None);
    for args in [&[][..], &["--size", "0", "unifont"], &["--bold", "unifont"]] {
        let (status, _, err) = preview(&root, args);
        assert_eq!(status, 2, "{args:?}: {err}");
        assert!(err.contains("--help"), "{args:?}: {err}");
    }
}

#[cfg(feature = "kitty-check")]
mod pictures {
    use std::fs::{File, OpenOptions};
    use std::io::{Read, Write};
    use std::os::fd::AsFd;
    use std::os::unix::fs::OpenOptionsExt;
    use std::process::Stdio;

    use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
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

    /// Each font is a line and a picture that moves the cursor past it,
    /// each its own image, which the console places.
    #[test]
    fn at_a_terminal_that_draws_kitty_graphics_each_font_is_also_a_picture() {
        let root = fixture("pictures", None);
        let (mut master, slave) = pair();
        let io = || Stdio::from(slave.as_fd().try_clone_to_owned().expect("dup"));
        let mut child = Command::new("setsid")
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
        assert!(text.contains("unifont\t12 pt\tcell 8x16"), "{text:?}");
    }
}
