//! `vtium-vitrify serve` as the console starts it: the binary under its own
//! sandbox, answering a request on stdin with frames on stdout; and a
//! source that fails as it draws, left out while the others answer.

#[path = "common/ttf.rs"]
mod ttf;

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use vtium_font::set::{Set, SetFace};
use vtium_font::{Cell, Face};
use vtium_vitrify::serve::{ASK, FRAME_DONE, FRAME_GLYPHS, Paths, ServeRequest, serve};

/// The frames in `bytes`, kind and payload.
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

#[test]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
fn the_helper_answers_under_its_sandbox_and_exits_when_the_console_closes() {
    let dir = std::env::temp_dir().join(format!("vtium-serve-bin-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let frit = dir.join("t.vtfrit");
    std::fs::write(&frit, "source=builtin:unscii-8\n").unwrap();
    let request = ServeRequest {
        kind: ASK,
        frit: frit.clone(),
        cell: Cell {
            width: 8,
            height: 16,
        },
        codepoints: vec![0x41],
        held: Vec::new(),
        console: None,
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_vtium-vitrify"))
        .arg("serve")
        .env_clear()
        .current_dir("/")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(&request.encode()).unwrap();
    drop(stdin);
    let mut out = Vec::new();
    child.stdout.take().unwrap().read_to_end(&mut out).unwrap();
    let status = child.wait().unwrap();
    let mut err = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut err)
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(status.success(), "{status}: {err}");
    let mut kinds = Vec::new();
    let mut rest = &out[..];
    while rest.len() >= 8 {
        let kind = u32::from_le_bytes(rest[..4].try_into().unwrap());
        let len = u32::from_le_bytes(rest[4..8].try_into().unwrap()) as usize;
        kinds.push(kind);
        rest = &rest[8 + len..];
    }
    assert!(rest.is_empty());
    assert_eq!(kinds.first(), Some(&FRAME_GLYPHS));
    assert_eq!(kinds.last(), Some(&FRAME_DONE));
}

// D14: a source that fails as it draws fails alone; the next source in the
// frit's order answers what it can
#[test]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
fn a_source_that_fails_as_it_draws_is_left_out_and_the_rest_answer() {
    let dir = std::env::temp_dir().join(format!("vtium-serve-fails-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // a font that maps A but gives no ascent or descent: it loads, and
    // fails when it is drawn
    let font = dir.join("flat.ttf");
    let flat = ttf::Font {
        ascent: 0,
        descent: 0,
        ..ttf::test_mono()
    };
    std::fs::write(&font, flat.build()).unwrap();
    let frit = dir.join("t.vtfrit");
    std::fs::write(
        &frit,
        format!(
            "source=outline:{}\nsource=builtin:unscii-8\n",
            font.display()
        ),
    )
    .unwrap();
    let request = ServeRequest {
        kind: ASK,
        frit,
        cell: Cell {
            width: 8,
            height: 16,
        },
        codepoints: vec![0x41],
        held: Vec::new(),
        console: None,
    };
    let mut out = Vec::new();
    let status = serve(
        std::io::Cursor::new(request.encode()),
        &mut out,
        &Paths::default(),
        Duration::from_secs(5),
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(status, 0);
    let frames = frames(&out);
    let kinds: Vec<u32> = frames.iter().map(|f| f.0).collect();
    assert_eq!(kinds, [FRAME_GLYPHS, FRAME_DONE], "{frames:?}");
    let set = Arc::new(Set::from_bytes(frames[0].1.clone()).unwrap());
    assert!(SetFace::new(set, 16).unwrap().has_glyph('A'));
}
