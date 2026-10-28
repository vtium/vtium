//! `vtium-vitrify serve` as the console starts it: the binary under its own
//! sandbox, answering a request on stdin with frames on stdout.

use std::io::{Read, Write};
use std::process::{Command, Stdio};

use vtium_font::Cell;
use vtium_vitrify::serve::{ASK, FRAME_DONE, FRAME_GLYPHS, ServeRequest};

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
