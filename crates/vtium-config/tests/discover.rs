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
    assert_eq!(d.faces[0].version.as_deref(), Some("3.1"));
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

/// `font`, an sfnt file, with its `cmap` table replaced by `cmap`: the
/// table directory written again, each table at a four-byte boundary.
fn with_cmap(font: &[u8], cmap: &[u8]) -> Vec<u8> {
    let be16 = |at: usize| usize::from(u16::from_be_bytes([font[at], font[at + 1]]));
    let be32 = |at: usize| u32::from_be_bytes(font[at..at + 4].try_into().unwrap()) as usize;
    let count = be16(4);
    let mut tables: Vec<([u8; 4], Vec<u8>)> = (0..count)
        .map(|i| {
            let rec = 12 + 16 * i;
            let tag: [u8; 4] = font[rec..rec + 4].try_into().unwrap();
            let (offset, len) = (be32(rec + 8), be32(rec + 12));
            let data = if &tag == b"cmap" {
                cmap.to_vec()
            } else {
                font[offset..offset + len].to_vec()
            };
            (tag, data)
        })
        .collect();
    tables.sort_by_key(|t| t.0);
    let mut out = font[..12].to_vec();
    let mut offset = 12 + 16 * count;
    let mut body = Vec::new();
    for (tag, data) in &tables {
        out.extend_from_slice(tag);
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&u32::try_from(offset + body.len()).unwrap().to_be_bytes());
        out.extend_from_slice(&u32::try_from(data.len()).unwrap().to_be_bytes());
        body.extend_from_slice(data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
    }
    offset += body.len();
    out.extend_from_slice(&body);
    assert_eq!(out.len(), offset);
    out
}

/// A `cmap` table of one format 12 subtable whose `groups` groups each
/// map every codepoint from U+0020 to U+10FFFF, one over the other.
fn overlapping_cmap(groups: u32) -> Vec<u8> {
    let mut sub = Vec::new();
    sub.extend_from_slice(&12u16.to_be_bytes());
    sub.extend_from_slice(&0u16.to_be_bytes());
    sub.extend_from_slice(&(16 + 12 * groups).to_be_bytes());
    sub.extend_from_slice(&0u32.to_be_bytes());
    sub.extend_from_slice(&groups.to_be_bytes());
    for _ in 0..groups {
        for v in [0x20u32, 0x10_ffff, 1] {
            sub.extend_from_slice(&v.to_be_bytes());
        }
    }
    let mut cmap = Vec::new();
    for v in [0u16, 1, 3, 10] {
        cmap.extend_from_slice(&v.to_be_bytes());
    }
    cmap.extend_from_slice(&12u32.to_be_bytes());
    cmap.extend_from_slice(&sub);
    cmap
}

/// A font of a few kilobytes can list the same codepoints many times over
/// in overlapping groups of its character map. The helper keeps each
/// codepoint once, in a fixed set, so such a font fits in the helper's
/// memory limit and is described like any other.
#[test]
fn a_font_that_maps_every_codepoint_many_times_over_is_described() {
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped: {why}");
        return;
    }
    let s = Scratch::new("overlap");
    let font = s.0.join("Overlap.ttf");
    std::fs::write(
        &font,
        with_cmap(&ttf::test_mono().build(), &overlapping_cmap(64)),
    )
    .unwrap();
    assert!(std::fs::metadata(&font).unwrap().len() < 8192);
    let got = describe(&helper(), &[font]);
    let Described::Font(d) = &got[0] else {
        panic!("{:?}", got[0]);
    };
    // every character from U+0020 on, less DEL, the C1 controls and the
    // surrogates, and less the 16 codepoints whose 16-bit glyph number
    // comes round to glyph 0, U+1001F and each 65,536 codepoints after it
    let all = 0x10_ffff - 0x20 + 1 - (0xa0 - 0x7f) - (0xe000 - 0xd800) - 16;
    assert_eq!(all, 1_111_983);
    assert_eq!(d.faces[0].codepoints, all);
}
