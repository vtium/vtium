//! The font helper describes font files for vtium-config's listing of the
//! fonts it finds: the real binary as `serve`, under its own sandbox,
//! answering each description request through its gate.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "a test fails by panicking"
)]

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Stdio;

use vtium_config::describe::{Format, Group};
use vtium_config::discover::{Described, describe};
use vtium_config::serve::{ActiveAsk, DescribeAsk, FRAME_NO_DESCRIPTION, FRAME_SANDBOX};
use vtium_config::specimen::Helper;
use vtium_testfont::ttf;

/// A directory of a test's own, removed when the test ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir =
            std::env::temp_dir().join(format!("vtium-discover-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn helper() -> Helper {
    Helper::at(env!("CARGO_BIN_EXE_vtium-config"))
}

#[test]
fn the_helper_describes_each_font_file_and_refuses_what_is_no_font() {
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped: {why}");
        return;
    }
    let s = Scratch::new("each");
    let font = s.0.join("TestMono.ttf");
    std::fs::write(&font, ttf::test_mono().build_with_version("Version 3.1")).unwrap();
    let notes = s.0.join("notes.ttf");
    std::fs::write(&notes, b"not a font at all").unwrap();
    let missing = s.0.join("gone.ttf");
    let got = describe(&helper(), &[font, notes, missing, s.0.join("TestMono.ttf")]);
    assert_eq!(got.len(), 4);
    let Described::Font(d) = &got[0] else {
        panic!("{:?}", got[0]);
    };
    assert_eq!(d.format, Format::OpenType);
    assert_eq!(d.faces[0].family, "Test Mono");
    assert_eq!(d.faces[0].version.as_deref(), Some("Version 3.1"));
    assert!(d.faces[0].groups.contains(&Group::ColorEmoji));
    assert!(
        matches!(&got[1], Described::NotAFont(why) if why.contains("not a font that vtium reads")),
        "{:?}",
        got[1]
    );
    assert!(
        matches!(&got[2], Described::NotAFont(why) if why.contains("gone.ttf")),
        "{:?}",
        got[2]
    );
    // one helper answers every request in order
    assert_eq!(got[3], got[0]);
}

#[test]
fn a_helper_that_cannot_start_leaves_every_file_unknown() {
    let got = describe(
        &Helper::at("/nonexistent/vtium-config"),
        &[PathBuf::from("/a.ttf"), PathBuf::from("/b.ttf")],
    );
    assert_eq!(got.len(), 2);
    assert!(
        got.iter()
            .all(|d| matches!(d, Described::Unknown(why) if why.contains("cannot start")))
    );
}

/// The console's helper holds the active fonts, and describes no other
/// file: a description request reads through the same gate as every frit.
#[test]
fn a_helper_that_holds_the_active_fonts_describes_no_other_file() {
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped: {why}");
        return;
    }
    let s = Scratch::new("gate");
    let font = s.0.join("TestMono.ttf");
    std::fs::write(&font, ttf::test_mono().build()).unwrap();
    let euid = rustix::process::geteuid().as_raw();
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_vtium-config"))
        .arg("serve")
        .env_clear()
        .current_dir("/")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let active = ActiveAsk {
        owner: euid,
        paths: Vec::new(),
    };
    stdin.write_all(&active.encode()).unwrap();
    stdin
        .write_all(&DescribeAsk { path: font.clone() }.encode())
        .unwrap();
    drop(stdin);
    let mut out = Vec::new();
    child.stdout.take().unwrap().read_to_end(&mut out).unwrap();
    let _ = child.wait();
    let frames = frames(&out);
    let kinds: Vec<u32> = frames.iter().map(|f| f.0).collect();
    assert_eq!(kinds, [FRAME_SANDBOX, FRAME_NO_DESCRIPTION], "{kinds:?}");
    let why = String::from_utf8_lossy(&frames[1].1);
    assert!(
        why.contains(&*font.to_string_lossy()) && why.contains("active"),
        "{why}"
    );
}

fn frames(mut bytes: &[u8]) -> Vec<(u32, Vec<u8>)> {
    let mut out = Vec::new();
    while bytes.len() >= 8 {
        let (head, rest) = bytes.split_at(8);
        let kind = u32::from_le_bytes(head[..4].try_into().unwrap());
        let len = u32::from_le_bytes(head[4..].try_into().unwrap()) as usize;
        out.push((kind, rest[..len].to_vec()));
        bytes = &rest[len..];
    }
    assert!(bytes.is_empty());
    out
}
