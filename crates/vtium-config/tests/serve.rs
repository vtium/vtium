//! `vtium-config serve` as the console starts it: the binary under its own
//! sandbox, answering a request on stdin with frames on stdout; and a
//! source that fails as it draws, left out while the others answer.

#[path = "common/ttf.rs"]
mod ttf;

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use vtium_config::serve::{
    ASK, FILL, FRAME_DONE, FRAME_FAILED, FRAME_FILL, FRAME_FILLED, FRAME_GLYPHS, FRAME_NO_OWN_CELL,
    FRAME_OUTLINE_CELL, FRAME_OWN_CELL, FRAME_STOPPED, FritText, OWN_CELL, Paths, STOP,
    ServeRequest, serve,
};
use vtium_font::set::{Set, SetFace};
use vtium_font::{Cell, Face};

/// A directory of a test's own, `vtium-serve-<name>-<pid>` in the
/// temporary directory, removed when the test ends, whether it passes or
/// fails.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("vtium-serve-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the test's directory");
        Scratch(dir)
    }
}

impl std::ops::Deref for Scratch {
    type Target = std::path::Path;

    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

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
    let request = ServeRequest {
        kind: ASK,
        frit: FritText {
            name: "t".to_owned(),
            text: "source=builtin:unscii-8\n".to_owned(),
        },
        cell: Cell {
            width: 8,
            height: 16,
        },
        codepoints: vec![0x41],
        clusters: Vec::new(),
        held: Vec::new(),
        console: None,
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_vtium-config"))
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
    let dir = Scratch::new("fails");
    // a font that maps A but gives no ascent or descent: it loads, and
    // fails when it is drawn
    let font = dir.join("flat.ttf");
    let flat = ttf::Font {
        ascent: 0,
        descent: 0,
        ..ttf::test_mono()
    };
    std::fs::write(&font, flat.build()).unwrap();
    let request = ServeRequest {
        kind: ASK,
        frit: FritText {
            name: "t".to_owned(),
            text: format!(
                "source=outline:{}\nsource=builtin:unscii-8\n",
                font.display()
            ),
        },
        cell: Cell {
            width: 8,
            height: 16,
        },
        codepoints: vec![0x41],
        clusters: Vec::new(),
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
    assert_eq!(status, 0);
    let frames = frames(&out);
    let kinds: Vec<u32> = frames.iter().map(|f| f.0).collect();
    assert_eq!(kinds, [FRAME_GLYPHS, FRAME_DONE], "{frames:?}");
    let set = Arc::new(Set::from_bytes(frames[0].1.clone()).unwrap());
    assert!(SetFace::new(set, 16).unwrap().has_glyph('A'));
}

/// Reads frames from `r` up to and including the first of `last`.
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn frames_until(r: &mut impl Read, last: &[u32]) -> Vec<(u32, Vec<u8>)> {
    let mut out = Vec::new();
    loop {
        let mut head = [0u8; 8];
        r.read_exact(&mut head).unwrap();
        let [k0, k1, k2, k3, n0, n1, n2, n3] = head;
        let kind = u32::from_le_bytes([k0, k1, k2, k3]);
        let mut payload = vec![0u8; u32::from_le_bytes([n0, n1, n2, n3]) as usize];
        r.read_exact(&mut payload).unwrap();
        out.push((kind, payload));
        if last.contains(&kind) {
            return out;
        }
    }
}

// a zoom away and back inside the helper's idle interval: the stop the
// console sends as it drops the cell's glyphs is answered, and makes the
// helper send them again, so the fill back at the cell draws the frit, not
// a failure
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn a_cell_stopped_and_filled_again_is_filled_again() {
    let request = |kind| ServeRequest {
        kind,
        frit: FritText {
            name: "t".to_owned(),
            text: "source=builtin:unscii-8\n".to_owned(),
        },
        cell: Cell {
            width: 8,
            height: 16,
        },
        codepoints: vec![0x41],
        clusters: Vec::new(),
        held: Vec::new(),
        console: None,
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_vtium-config"))
        .arg("serve")
        .env_clear()
        .current_dir("/")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let ends = [FRAME_FILLED, FRAME_FAILED];
    stdin.write_all(&request(FILL).encode()).unwrap();
    let first = frames_until(&mut stdout, &ends);
    assert_eq!(first.last().map(|f| f.0), Some(FRAME_FILLED));
    // the zoom away, then back with nothing held
    stdin.write_all(&request(STOP).encode()).unwrap();
    stdin.write_all(&request(FILL).encode()).unwrap();
    let again = frames_until(&mut stdout, &ends);
    drop(stdin);
    assert!(child.wait().unwrap().success());
    let mut cell = 8u32.to_le_bytes().to_vec();
    cell.extend_from_slice(&16u32.to_le_bytes());
    let (stopped, again) = again.split_first().unwrap();
    assert_eq!(*stopped, (FRAME_STOPPED, cell));
    let kinds = |f: &[(u32, Vec<u8>)]| f.iter().map(|f| f.0).collect::<Vec<_>>();
    assert_eq!(kinds(again), kinds(&first), "{:?}", again.last());
    let a = |f: &[(u32, Vec<u8>)]| {
        let set = Arc::new(Set::from_bytes(f[0].1.clone()).unwrap());
        SetFace::new(set, 16).unwrap().has_glyph('A')
    };
    assert!(a(&first) && a(again));
}

/// Each codepoint the fill frames among `frames` hold, with how many
/// frames hold it.
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn filled(frames: &[(u32, Vec<u8>)]) -> std::collections::BTreeMap<char, usize> {
    let faces: Vec<SetFace> = frames
        .iter()
        .filter(|f| f.0 == FRAME_FILL)
        .map(|f| SetFace::new(Arc::new(Set::from_bytes(f.1.clone()).unwrap()), 16).unwrap())
        .collect();
    let mut held = std::collections::BTreeMap::new();
    for c in (0..0x2_0000).filter_map(char::from_u32) {
        let n = faces.iter().filter(|f| f.has_glyph(c)).count();
        if n > 0 {
            held.insert(c, n);
        }
    }
    held
}

// D18: the helper skips repeats within one request only; the console's
// store is the one record of what it holds, so a second fill of a cell
// sends its coverage in full again, each glyph once
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn each_fill_sends_the_cells_coverage_in_full_once() {
    let fill = ServeRequest {
        kind: FILL,
        frit: FritText {
            name: "t".to_owned(),
            text: "source=builtin:unscii-8\n".to_owned(),
        },
        cell: Cell {
            width: 8,
            height: 16,
        },
        codepoints: vec![0x41],
        clusters: Vec::new(),
        held: Vec::new(),
        console: None,
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_vtium-config"))
        .arg("serve")
        .env_clear()
        .current_dir("/")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let ends = [FRAME_FILLED, FRAME_FAILED];
    stdin.write_all(&fill.encode()).unwrap();
    let first = frames_until(&mut stdout, &ends);
    stdin.write_all(&fill.encode()).unwrap();
    let again = frames_until(&mut stdout, &ends);
    drop(stdin);
    assert!(child.wait().unwrap().success());
    for frames in [&first, &again] {
        assert_eq!(frames.last().map(|f| f.0), Some(FRAME_FILLED));
    }
    let (first, again) = (filled(&first), filled(&again));
    assert!(first.len() > 100, "{}", first.len());
    assert!(first.values().all(|&n| n == 1));
    assert_eq!(again, first);
}

// ADR-1200 D17 (the maintainer, 2026-11-01): an outline frit's cell query
// names the height it is drawn at, and is answered with the W rule's cell
// at it; with no height it is refused, naming why. After a bitmap first
// source the bitmap's own cell is answered, the outline source drawing
// into it
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn an_outline_frits_cell_query_answers_the_w_rule_at_its_height() {
    let dir = Scratch::new("outline-cell");
    let font = dir.join("mono.ttf");
    std::fs::write(&font, ttf::test_mono().build()).unwrap();
    let query = |text: String, height: u32| ServeRequest {
        kind: OWN_CELL,
        frit: FritText {
            name: "t".to_owned(),
            text,
        },
        cell: Cell { width: 0, height },
        codepoints: Vec::new(),
        clusters: Vec::new(),
        held: Vec::new(),
        console: None,
    };
    let outline = format!("source=outline:{}\n", font.display());
    let requests = [
        query(outline.clone(), 0),
        query(outline.clone(), 20),
        query(format!("source=builtin:unscii-8\n{outline}"), 20),
    ];
    let bytes: Vec<u8> = requests.iter().flat_map(ServeRequest::encode).collect();
    let mut out = Vec::new();
    let status = serve(
        std::io::Cursor::new(bytes),
        &mut out,
        &Paths::default(),
        Duration::from_secs(5),
    );
    assert_eq!(status, 0);
    let frames = frames(&out);
    assert_eq!(
        frames.iter().map(|f| f.0).collect::<Vec<_>>(),
        [FRAME_NO_OWN_CELL, FRAME_OUTLINE_CELL, FRAME_OWN_CELL],
        "{frames:?}"
    );
    let why = String::from_utf8_lossy(&frames[0].1);
    assert!(
        why.contains("is an outline font, whose cell needs the height"),
        "{why}"
    );
    let cell = |w: u32, h: u32| [w.to_le_bytes(), h.to_le_bytes()].concat();
    // 19.5 pixels to the em: Test Mono's W advances 11.7, inked over 12
    assert_eq!(frames[1].1, cell(12, 20));
    assert_eq!(frames[2].1, cell(8, 8));
}

// ADR-1200 D21, ADR-1301 D1: an ask's clusters are shaped with HarfRust
// and drawn whole into the frame's cluster map, keyed by their text; one
// the font does not shape as its kind requires is left out, and the ask
// is answered whole
#[test]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
fn an_asks_clusters_are_shaped_and_drawn_whole() {
    let dir = Scratch::new("clusters");
    let font = dir.join("shaping.ttf");
    std::fs::write(&font, ttf::test_shaping().build()).unwrap();
    let key = |cs: &[char]| cs.iter().map(|&c| u32::from(c)).collect::<Vec<u32>>();
    let acute = ['e', '\u{301}'];
    // the font has no q, and no ligature of man and girl
    let q_acute = ['q', '\u{301}'];
    let man_girl = ['\u{1f468}', '\u{200d}', '\u{1f467}'];
    let request = ServeRequest {
        kind: ASK,
        frit: FritText {
            name: "t".to_owned(),
            text: format!("source=outline:{}\n", font.display()),
        },
        cell: Cell {
            width: 12,
            height: 20,
        },
        codepoints: vec![0xe9],
        clusters: vec![
            key(&acute),
            key(&ttf::FAMILY),
            key(&ttf::FLAG),
            key(&q_acute),
            key(&man_girl),
        ],
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
    assert_eq!(status, 0);
    let frames = frames(&out);
    let kinds: Vec<u32> = frames.iter().map(|f| f.0).collect();
    assert_eq!(kinds, [FRAME_GLYPHS, FRAME_GLYPHS, FRAME_DONE], "{kinds:?}");
    let sets: Vec<Arc<Set>> = frames[..2]
        .iter()
        .map(|f| Arc::new(Set::from_bytes(f.1.clone()).unwrap()))
        .collect();
    // the codepoints' frame maps no cluster, the clusters' frame maps
    // only the three the font shapes
    assert_eq!(sets[0].clusters().count(), 0);
    let mut shaped: Vec<Vec<char>> = sets[1].clusters().map(|(_, _, k)| k).collect();
    shaped.sort();
    let mut want = vec![acute.to_vec(), ttf::FAMILY.to_vec(), ttf::FLAG.to_vec()];
    want.sort();
    assert_eq!(shaped, want);
    let codepoints = SetFace::new(Arc::clone(&sets[0]), 20).unwrap();
    let clusters = SetFace::new(Arc::clone(&sets[1]), 20).unwrap();
    let regular = vtium_font::Style::default();
    // e and U+0301 compose to the font's é
    assert_eq!(
        clusters.cluster(&acute, regular).unwrap().pixels(),
        codepoints.draw(&['\u{e9}'], regular).unwrap().pixels()
    );
    let family = clusters.cluster(&ttf::FAMILY, regular).unwrap();
    assert_eq!(family.cells(), vtium_font::Width::Wide);
    assert_eq!(family.format(), vtium_font::Format::Rgba);
}
