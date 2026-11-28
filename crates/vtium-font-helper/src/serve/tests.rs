use super::engine::draw::{halved, halved_ink};
use super::engine::{Handed, outline_coverage, own_regular, ranges};
use super::sandbox::{fall_back, lock_out, refuse_root, refuse_sockets, sandbox, status_ids};
use super::*;
use crate::frit::{Cover, Hinting};
use crate::outline::{OutlineRequest, rasterise};
use crate::scratch::Scratch;
use crate::sources::{Host, SourceError};
use landlock::LandlockStatus;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use vtium_font::Face;
use vtium_font::Format;
use vtium_font::set::{Set, SetFace};
use vtium_font::{
    CELL_SIDE_MAX, CLUSTER_SCALARS_MAX, Decorations, Glyph, Script, Style, Width, spacing,
};
use vtium_platform::vt::font::UniPair;
use vtium_render::POINTER_SVG_BYTES_MAX;
use vtium_settings::batch::{Batch, Entry};
use vtium_settings::cullet::{self, Rule};
use vtium_term::{ClusterWidth, PointerShape};

/// A directory of a test's own, `vtium-serve-<name>-<pid>` in the
/// temporary directory, removed when the test ends, whether it passes
/// or fails.
fn scratch(name: &str) -> Scratch {
    Scratch::new(&format!("serve-{name}"))
}

/// Each frame in `bytes`, a glyph frame's payload without the band in
/// front of its set, which [`bands`] gives.
fn frames(bytes: &[u8]) -> Vec<(u32, Vec<u8>)> {
    raw_frames(bytes)
        .into_iter()
        .map(|(kind, payload)| match kind {
            FRAME_GLYPHS | FRAME_FILL => (kind, payload[8..].to_vec()),
            _ => (kind, payload),
        })
        .collect()
}

/// Each frame in `bytes`, as the helper wrote it.
fn raw_frames(mut bytes: &[u8]) -> Vec<(u32, Vec<u8>)> {
    let mut out = Vec::new();
    while !bytes.is_empty() {
        let kind = u32::from_le_bytes(bytes[..4].try_into().unwrap());
        let len = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        out.push((kind, bytes[8..8 + len].to_vec()));
        bytes = &bytes[8 + len..];
    }
    out
}

/// The band each glyph frame in `bytes` names, in order.
fn bands(bytes: &[u8]) -> Vec<Band> {
    raw_frames(bytes)
        .into_iter()
        .filter(|f| matches!(f.0, FRAME_GLYPHS | FRAME_FILL))
        .map(|(_, p)| Band {
            above: u32::from_le_bytes(p[..4].try_into().unwrap()),
            below: u32::from_le_bytes(p[4..8].try_into().unwrap()),
        })
        .collect()
}

/// The frit `t` of `text`, handed over with no file.
fn frit(text: &str) -> FritText {
    FritText {
        name: "t".to_owned(),
        text: text.to_owned(),
    }
}

const CELL: Cell = Cell {
    width: 8,
    height: 16,
};

fn request(kind: u32, frit: FritText, codepoints: &[char]) -> ServeRequest {
    ServeRequest {
        cluster_width: Default::default(),
        kind,
        frit,
        cell: CELL,
        band: Band::NONE,
        style: Style::default(),
        script: Script::Baseline,
        doubled: None,
        codepoints: codepoints.iter().map(|&c| u32::from(c)).collect(),
        clusters: Vec::new(),
        held: Vec::new(),
        console: None,
    }
}

fn run(input: &[ServeRequest]) -> Vec<(u32, Vec<u8>)> {
    frames(&run_bytes(input))
}

/// What the helper writes for `input`, as it writes it.
fn run_bytes(input: &[ServeRequest]) -> Vec<u8> {
    let bytes: Vec<u8> = input.iter().flat_map(ServeRequest::encode).collect();
    let mut out = Vec::new();
    assert_eq!(
        serve(
            std::io::Cursor::new(bytes),
            &mut out,
            &Paths::default(),
            Duration::from_secs(5)
        ),
        0
    );
    out
}

/// Each glyph frame's set as a face at the 8x16 cell, with its kind.
fn faces(frames: &[(u32, Vec<u8>)]) -> Vec<(u32, SetFace)> {
    frames
        .iter()
        .filter(|f| matches!(f.0, FRAME_GLYPHS | FRAME_FILL))
        .map(|f| {
            let set = Arc::new(Set::from_bytes(f.1.clone()).unwrap());
            assert_eq!(set.cells(), vec![CELL]);
            (f.0, SetFace::new(set, 16).unwrap())
        })
        .collect()
}

// ADR-1200 D14: the console names the sources the helper leaves out,
// after one of them made a helper crash; the helper reads the frit
// without them, and a frit with none left draws nothing
#[test]
fn the_sources_the_console_names_are_left_out() {
    let text = "source=builtin:unifont\ncover=U+0041\nsource=builtin:unifont\ncover=U+0042\nsource=builtin:unifont\ncover=U+0043\n";
    let ask = request(ASK, frit(text), &['A', 'B', 'C']).encode();
    let answer = |mask: Option<u32>| {
        let mut bytes = mask.map(leave_out_request).unwrap_or_default();
        bytes.extend_from_slice(&ask);
        let mut out = Vec::new();
        let status = serve(
            std::io::Cursor::new(bytes),
            &mut out,
            &Paths::default(),
            Duration::from_secs(5),
        );
        (status, frames(&out))
    };
    let drawn = |frames: &[(u32, Vec<u8>)]| -> Vec<char> {
        let faces = faces(frames);
        ['A', 'B', 'C']
            .into_iter()
            .filter(|&c| faces.iter().any(|(_, f)| f.has_glyph(c)))
            .collect()
    };
    let (status, all) = answer(None);
    assert_eq!(status, 0);
    assert_eq!(drawn(&all), ['A', 'B', 'C']);
    // the second source left out: its B is not drawn, the others are,
    // and no warning names it
    let (status, two) = answer(Some(0b010));
    assert_eq!(status, 0);
    assert_eq!(drawn(&two), ['A', 'C']);
    assert!(two.iter().all(|f| f.0 != FRAME_WARNING), "{two:?}");
    assert_eq!(two.last().map(|f| f.0), Some(FRAME_DONE));
    let (status, first) = answer(Some(0b110));
    assert_eq!(status, 0);
    assert_eq!(drawn(&first), ['A']);
    // every source left out: the ask fails, and the helper goes on
    let (status, none) = answer(Some(0b111));
    assert_eq!(status, 0);
    assert_eq!(none.iter().map(|f| f.0).collect::<Vec<_>>(), [FRAME_FAILED]);
}

// ADR-1200 D14: a text font the console left out, after it made a
// helper fail, gives no cell, and the reason says that vtium left it
// out, not that it is missing; the built-in font draws the text in its
// place
#[test]
fn a_text_font_the_console_left_out_gives_no_cell_and_says_why() {
    let text = "source=builtin:unscii-8\nsource=builtin:unifont\n";
    let mut bytes = leave_out_request(0b01);
    bytes.extend_from_slice(&query_at(frit(text), 16).encode());
    let mut out = Vec::new();
    let status = serve(
        std::io::Cursor::new(bytes),
        &mut out,
        &Paths::default(),
        Duration::from_secs(5),
    );
    assert_eq!(status, 0);
    let frames: Vec<(u32, Vec<u8>)> = frames(&out)
        .into_iter()
        .filter(|f| f.0 != FRAME_WARNING)
        .collect();
    assert_eq!(
        frames.iter().map(|f| f.0).collect::<Vec<_>>(),
        [FRAME_NO_OWN_CELL]
    );
    assert_eq!(
        String::from_utf8_lossy(&frames[0].1),
        "vtium left out the text font source=builtin:unscii-8 after it made the font helper fail, so the built-in font draws the text in its place"
    );
}

// a leave-out request reads back, and one with a bit past the sixteen
// sources a frit holds is refused before anything else is read
#[test]
fn a_leave_out_request_reads_back_and_names_no_source_past_sixteen() {
    for mask in [0, 1, 0b1010, 0xffff] {
        assert_eq!(
            read_request(&mut &leave_out_request(mask)[..]),
            Ok(Some(Request::LeaveOut(mask)))
        );
    }
    for mask in [0x1_0000, 0x8000_0000] {
        assert_eq!(
            read_request(&mut &leave_out_request(mask)[..]),
            Err("sources left out past the 16 a frit holds".to_owned())
        );
    }
    let mut cut = leave_out_request(1);
    cut.pop();
    assert!(read_request(&mut &cut[..]).is_err());
}

#[test]
fn a_request_reads_back() {
    let r = ServeRequest {
        cluster_width: Default::default(),
        held: vec![(0x41, 0x5a), (0x4e00, 0x4e00)],
        console: Some(ConsoleFont {
            width: 8,
            height: 2,
            charcount: 2,
            data: vec![1, 2, 3, 4],
            unimap: vec![UniPair {
                unicode: 0x41,
                fontpos: 1,
            }],
        }),
        ..request(FILL, frit("name=a\nsource=builtin:unifont\n"), &['A', '中'])
    };
    let bytes = r.encode();
    assert_eq!(ServeRequest::read(&mut &bytes[..]), Ok(Some(r)));
    assert_eq!(ServeRequest::read(&mut &b""[..]), Ok(None));
}

// ADR-1202 D14: an ask that says how its glyphs were made reads back,
// and its answer holds a made frame just before its done frame, naming
// each script glyph the helper drew and how; a plain ask's holds none
// covers: sgr.script
#[test]
fn an_ask_made_says_how_each_script_glyph_was_made() {
    let sup = ServeRequest {
        cluster_width: Default::default(),
        script: Script::Super,
        ..request(ASK_MADE, frit("source=builtin:unifont\n"), &['x', '2'])
    };
    assert_eq!(
        ServeRequest::read(&mut &sup.encode()[..]),
        Ok(Some(sup.clone()))
    );
    let answer = run(std::slice::from_ref(&sup));
    let kinds: Vec<u32> = answer.iter().map(|f| f.0).collect();
    assert_eq!(kinds.first(), Some(&FRAME_GLYPHS), "{kinds:?}");
    assert_eq!(kinds[kinds.len() - 2..], [FRAME_MADE, FRAME_DONE]);
    assert_eq!(
        Made::decode_all(&answer[kinds.len() - 2].1),
        Some(vec![(0x32, Made::Scaled), (0x78, Made::Scaled)])
    );
    let plain = ServeRequest { kind: ASK, ..sup };
    assert!(!run(&[plain]).iter().any(|f| f.0 == FRAME_MADE));
}

const ITALIC: Style = Style {
    bold: false,
    italic: true,
};
const BOLD_ITALIC: Style = Style {
    bold: true,
    italic: true,
};

// the line size of a line of double size moved the magic from
// VTVSREQ9 to VTVSREQA, as ADR-1006 D8's cluster width moved it from
// VTVSREQ8 to VTVSREQ9, as ADR-1401 D7's coverage query and ADR-1302 D5's color
// emoji's box moved it from VTVSREQ7 to VTVSREQ8, as ADR-1200 D15's report
// of the sandbox moved it from VTVSREQ6 to VTVSREQ7, as ADR-1202 D11's script moved it from VTVSREQ5 to VTVSREQ6, as
// ADR-1302 D13's band moved it from VTVSREQ4 to VTVSREQ5 and the style
// from VTVSREQ3 to VTVSREQ4: a request of an older layout
// is refused whole, with the reason a person can act on, and one of no
// layout of this format as not a request; an ask alone names a style
// (ADR-1200 D21)
#[test]
fn the_old_magic_and_a_style_off_an_ask_are_refused() {
    let ask = ServeRequest {
        cluster_width: Default::default(),
        style: BOLD_ITALIC,
        script: Script::Baseline,
        ..request(ASK, frit("source=builtin:unifont\n"), &['A'])
    };
    let bytes = ask.encode();
    assert_eq!(ServeRequest::read(&mut &bytes[..]), Ok(Some(ask)));
    for magic in [
        b"VTVSREQ3",
        b"VTVSREQ4",
        b"VTVSREQ5",
        b"VTVSREQ6",
        b"VTVSREQ7",
        b"VTVSREQ8",
        b"VTVSREQ9",
    ] {
        let mut old = bytes.clone();
        old[..8].copy_from_slice(magic);
        assert_eq!(
            ServeRequest::read(&mut &old[..]),
            Err(OTHER_BUILD.to_owned())
        );
    }
    let mut other = bytes.clone();
    other[..8].copy_from_slice(b"GARBAGE!");
    assert_eq!(
        ServeRequest::read(&mut &other[..]),
        Err("not a serve request".to_owned())
    );
    for kind in [FILL, STOP, OWN_CELL, SOURCES] {
        let query = matches!(kind, OWN_CELL | SOURCES);
        let r = ServeRequest {
            cluster_width: Default::default(),
            style: ITALIC,
            script: Script::Baseline,
            cell: if query {
                Cell {
                    width: 0,
                    height: 0,
                }
            } else {
                CELL
            },
            band: Band::NONE,
            ..request(kind, frit("source=builtin:unifont\n"), &[])
        };
        let e = ServeRequest::read(&mut &r.encode()[..]).unwrap_err();
        assert!(e.contains("style"), "{kind}: {e}");
        let regular = ServeRequest {
            cluster_width: Default::default(),
            style: REGULAR,
            script: Script::Baseline,
            ..r
        };
        assert!(
            ServeRequest::read(&mut &regular.encode()[..]).is_ok(),
            "{kind}"
        );
    }
}

#[test]
fn a_malformed_request_is_refused_before_allocation() {
    // the frit `a` with no text: its cell at 21, its line spacing at 29,
    // its style at 37, its script at 41, its cluster width at 45, its
    // line size at 49, its capabilities at 53, its codepoint at 61
    let a = FritText {
        name: "a".to_owned(),
        text: String::new(),
    };
    let good = request(ASK, a.clone(), &['A']).encode();
    assert_eq!(ServeRequest::read(&mut &good[..]).unwrap().unwrap().frit, a);
    // the name's and the text's lengths past their bounds, then
    // nothing: refused on the count, before a buffer of that size is
    // made
    let head = |n: &[u32]| {
        let mut b = REQUEST_MAGIC.to_vec();
        b.extend_from_slice(&ASK.to_le_bytes());
        for v in n {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b
    };
    for huge in [head(&[u32::MAX]), {
        let mut b = head(&[1]);
        b.push(b'a');
        b.extend_from_slice(&u32::MAX.to_le_bytes());
        b
    }] {
        let e = ServeRequest::read(&mut &huge[..]).unwrap_err();
        assert!(e.contains("past its bound"), "{e}");
    }
    let mut many = head(&[1]);
    many.push(b'a');
    for v in [0u32, 8, 16, 0, 0, 0, 0, 0, 0, 0, u32::MAX] {
        many.extend_from_slice(&v.to_le_bytes());
    }
    let e = ServeRequest::read(&mut &many[..]).unwrap_err();
    assert!(e.contains("past its bound"), "{e}");
    // the kind, the cell's width and height, line spacing above and
    // below that leaves no text band, a style past bold italic, a
    // script past subscript, a cluster width past single, a line size
    // past double height, capabilities not yet read, the codepoint, the
    // cluster count
    for (at, value) in [
        (8, 9u32),
        (21, 0),
        (25, CELL_SIDE_MAX + 1),
        (29, 16),
        (33, 16),
        (37, 4),
        (41, 3),
        (45, 2),
        (49, 3),
        (53, 1),
        (61, 0xd800),
        (65, u32::MAX),
    ] {
        let mut bad = good.clone();
        bad[at..at + 4].copy_from_slice(&value.to_le_bytes());
        assert!(ServeRequest::read(&mut &bad[..]).is_err(), "{at}");
    }
    // ADR-1006 D8: an ask carries its cluster width, and a request
    // that is not an ask carries `double`
    let single = ServeRequest {
        cluster_width: ClusterWidth::Single,
        ..request(ASK, a.clone(), &['A'])
    };
    let read = ServeRequest::read(&mut &single.encode()[..])
        .unwrap()
        .unwrap();
    assert_eq!(read.cluster_width, ClusterWidth::Single);
    let fill = ServeRequest {
        cluster_width: ClusterWidth::Single,
        ..request(FILL, a.clone(), &[])
    };
    let e = ServeRequest::read(&mut &fill.encode()[..]).unwrap_err();
    assert!(e.contains("cluster width"), "{e}");
    let backwards = ServeRequest {
        cluster_width: Default::default(),
        held: vec![(0x42, 0x41)],
        ..request(FILL, a, &[])
    };
    assert!(ServeRequest::read(&mut &backwards.encode()[..]).is_err());
    for name in ["", "a\nb"] {
        let named = ServeRequest {
            cluster_width: Default::default(),
            frit: FritText {
                name: name.to_owned(),
                text: String::new(),
            },
            ..request(ASK, frit(""), &[])
        };
        assert!(
            ServeRequest::read(&mut &named.encode()[..]).is_err(),
            "{name:?}"
        );
    }
    let mut not_utf8 = good.clone();
    not_utf8[16] = 0xff;
    assert!(ServeRequest::read(&mut &not_utf8[..]).is_err());
    assert!(ServeRequest::read(&mut &good[..good.len() - 1]).is_err());
}

/// A sources query is answered for every frit: the sources left out of
/// one that reads, those of one that reads and has no source on this
/// machine after a line that says it draws nothing, and for one that
/// does not read a frame of its own, never the failed frame the helper
/// sends for its own failure.
#[test]
fn a_frit_that_draws_nothing_has_an_answer_of_its_own() {
    let ask = |text: &str| ServeRequest {
        cluster_width: Default::default(),
        cell: Cell {
            width: 0,
            height: 0,
        },
        band: Band::NONE,
        ..request(SOURCES, frit(text), &[])
    };
    let frames: Vec<(u32, Vec<u8>)> = run(&[
        ask("source=builtin:unifont\n"),
        ask("source=file:/nonexistent/vtium.psf\n"),
        ask("no-such-key=1\n"),
    ])
    .into_iter()
    .filter(|f| f.0 != FRAME_WARNING)
    .collect();
    assert_eq!(
        frames.iter().map(|f| f.0).collect::<Vec<_>>(),
        [FRAME_SOURCES, FRAME_SOURCES, FRAME_DRAWS_NOTHING]
    );
    let nothing = String::from_utf8_lossy(&frames[1].1);
    assert_eq!(
        DrawsNothing::parse(&nothing).as_deref(),
        Some("source=file:/nonexistent/vtium.psf: /nonexistent/vtium.psf is not on this machine"),
        "{nothing}"
    );
}

/// A sources query carries a cell of 0 by 0 and names nothing: one
/// at a width or a height, or naming a codepoint or a held range, is
/// refused as it is read.
#[test]
fn a_sources_query_naming_anything_is_refused() {
    let empty = Cell {
        width: 0,
        height: 0,
    };
    let query = ServeRequest {
        cluster_width: Default::default(),
        cell: empty,
        band: Band::NONE,
        ..request(SOURCES, frit("source=builtin:unifont\n"), &[])
    };
    assert_eq!(
        ServeRequest::read(&mut &query.encode()[..]),
        Ok(Some(query.clone()))
    );
    for (what, bad) in [
        (
            "a width",
            ServeRequest {
                cluster_width: Default::default(),
                cell: Cell {
                    width: 1,
                    height: 0,
                },
                band: Band::NONE,
                ..query.clone()
            },
        ),
        (
            "a height",
            ServeRequest {
                cluster_width: Default::default(),
                cell: Cell {
                    width: 0,
                    height: 16,
                },
                band: Band::NONE,
                ..query.clone()
            },
        ),
        (
            "a codepoint",
            ServeRequest {
                cluster_width: Default::default(),
                codepoints: vec![0x41],
                ..query.clone()
            },
        ),
        (
            "a held range",
            ServeRequest {
                cluster_width: Default::default(),
                held: vec![(0x41, 0x42)],
                ..query.clone()
            },
        ),
    ] {
        assert!(
            ServeRequest::read(&mut &bad.encode()[..]).is_err(),
            "{what}"
        );
    }
}

// ADR-1200 D21: an ask names clusters whole, each checked as it is
// read, and no other request names one
#[test]
fn an_asks_clusters_read_back_and_a_malformed_one_is_refused() {
    let acute = vec![0x65, 0x301];
    let family = vec![0x1f468, 0x200d, 0x1f469, 0x200d, 0x1f467];
    let ask = ServeRequest {
        cluster_width: Default::default(),
        clusters: vec![acute.clone(), family],
        ..request(ASK, frit("source=builtin:unifont\n"), &['A'])
    };
    let bytes = ask.encode();
    assert_eq!(ServeRequest::read(&mut &bytes[..]), Ok(Some(ask.clone())));
    let longest = CLUSTER_SCALARS_MAX as usize;
    for bad in [vec![], vec![0x41; longest + 1], vec![0x65, 0xd800]] {
        let r = ServeRequest {
            cluster_width: Default::default(),
            clusters: vec![bad.clone()],
            ..ask.clone()
        };
        assert!(
            ServeRequest::read(&mut &r.encode()[..]).is_err(),
            "{bad:x?}"
        );
    }
    let longest_ok = ServeRequest {
        cluster_width: Default::default(),
        clusters: vec![vec![0x41; longest]],
        ..ask.clone()
    };
    assert!(ServeRequest::read(&mut &longest_ok.encode()[..]).is_ok());
    for kind in [FILL, STOP] {
        let r = ServeRequest {
            cluster_width: Default::default(),
            clusters: vec![acute.clone()],
            ..request(kind, frit("source=builtin:unifont\n"), &[])
        };
        let e = ServeRequest::read(&mut &r.encode()[..]).unwrap_err();
        assert!(e.contains("past its bound"), "{e}");
    }
}

// ADR-1200 D1, S5: a frit's warnings, such as compress='s, are sent
// as the helper reads the frit, once for its life, so the console logs
// them
#[test]
fn a_frits_warnings_are_sent_once_as_it_is_read() {
    let warned = frit("source=builtin:unifont\ncompress=yes\n");
    let ask = request(ASK, warned.clone(), &['A']);
    let frames = run(&[ask.clone(), request(FILL, warned, &[]), ask]);
    let warnings: Vec<String> = frames
        .iter()
        .filter(|(k, _)| *k == FRAME_WARNING)
        .map(|(_, p)| String::from_utf8(p.clone()).unwrap())
        .collect();
    assert_eq!(
        warnings,
        ["t:2: compress= has no effect here, and is ignored"]
    );
    assert_eq!(frames.first().map(|f| f.0), Some(FRAME_WARNING));
    let quiet = run(&[request(ASK, frit("source=builtin:unifont\n"), &['A'])]);
    assert!(quiet.iter().all(|(k, _)| *k != FRAME_WARNING));
}

// a bdf: source is parsed where the other fonts are, not as it is read
#[test]
fn a_bdf_source_is_parsed_when_the_frit_loads() {
    let dir = scratch("bdf");
    let (good, bad) = (dir.join("good.bdf"), dir.join("bad.bdf"));
    std::fs::write(&good, crate::bdf::tests::bdf("")).unwrap();
    std::fs::write(&bad, "STARTFONT 2.1\nCHARS 1\n").unwrap();
    let load = |p: &std::path::Path| {
        Engine::load(
            &frit(&format!("source=bdf:{}\n", p.display())),
            &Paths::default(),
            None,
        )
    };
    let (good, bad) = (load(&good), load(&bad));
    assert!(good.unwrap().coverage().contains(&0x41));
    let e = bad.err().unwrap();
    assert!(e.contains("CHARSET_REGISTRY"), "{e}");
}

// A source on this machine that the helper refuses, such as a BDF
// file its reader refuses, is a warning the console logs when the
// frit loads without it. A source this machine does not have is a
// warning too, which names the set and the font, so the console logs
// one line for each missing font.
//
// References:
// - ADR-1204 D7: the console's line for each missing font.
#[test]
fn a_source_refused_and_a_missing_one_are_each_a_warning() {
    let dir = scratch("refused");
    let bad = dir.join("bad.bdf");
    std::fs::write(&bad, "STARTFONT 2.1\nCHARS 1\n").unwrap();
    let missing = dir.join("missing.bdf");
    let text = format!(
        "source=builtin:unifont\nsource=bdf:{}\nsource=bdf:{}\nversion=1.2\n",
        bad.display(),
        missing.display()
    );
    let engine = Engine::load(&frit(&text), &Paths::default(), None);
    let frames = run(&[request(ASK, frit(&text), &['A'])]);
    let warnings = engine.unwrap().warnings().to_vec();
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    let w = &warnings[0];
    assert!(
        w.starts_with(&format!("t:2: source=bdf:{}: ", bad.display())),
        "{w}"
    );
    assert!(w.contains("CHARSET_REGISTRY"), "{w}");
    assert!(w.ends_with("; the source is left out"), "{w}");
    assert_eq!(
        warnings[1],
        format!(
            "t needs the font {} 1.2 or later, which is not on this machine, so vtium draws its characters from the next font that has them",
            missing.display()
        )
    );
    let sent: Vec<String> = frames
        .iter()
        .filter(|(k, _)| *k == FRAME_WARNING)
        .map(|(_, p)| String::from_utf8(p.clone()).unwrap())
        .collect();
    assert_eq!(sent, warnings, "sent to the console as the frit loads");
}

// A font that lacks a character its check= line names is a different
// font of a similar name. The helper leaves it out as missing, the
// next source gives its characters, the console's line names the
// character, and a sources query answers both that the source is
// missing and which character the font lacks. A font that holds every
// character check= names draws.
//
// References:
// - ADR-1204 D7: a font that fails check= counts as missing.
#[test]
fn a_font_that_fails_its_check_is_missing_and_the_next_source_draws() {
    let dir = scratch("check");
    let file = dir.join("t.bdf");
    std::fs::write(&file, crate::bdf::tests::bdf("")).unwrap();
    let named = |check: &str| {
        format!(
            "source=bdf:{}\n{check}\nsource=builtin:unscii-8\n",
            file.display()
        )
    };
    // the font holds A and U+4E2D, and lacks U+E0B0
    let held = Engine::load(
        &frit(&named("check=U+0041\ncheck=U+4E2D")),
        &Paths::default(),
        None,
    )
    .unwrap();
    assert_eq!(held.left_out(), []);
    assert_eq!(held.differs(), []);
    assert_eq!(held.warnings(), [] as [String; 0]);
    assert_eq!(held.sources.len(), 2);
    let text = named("check=U+0041-U+0042");
    let lacked = Engine::load(&frit(&text), &Paths::default(), None).unwrap();
    assert_eq!(lacked.sources.len(), 1, "the font is left out");
    assert!(matches!(
        &lacked.sources[0].0.origin,
        crate::frit::Origin::Builtin(_)
    ));
    assert_eq!(lacked.left_out().len(), 1);
    assert!(lacked.left_out()[0].missing);
    assert_eq!(lacked.left_out()[0].line, 1);
    assert_eq!(
        lacked.differs(),
        [crate::needs::Differs {
            line: 1,
            spelling: format!("bdf:{}", file.display()),
            what: crate::needs::Differ::Lacks(0x42),
        }]
    );
    assert_eq!(
        lacked.warnings(),
        [format!(
            "t needs the font {}, and the font of that name on this machine lacks U+0042, so vtium counts it as missing and draws its characters from the next font that has them",
            file.display()
        )]
    );
    let frames = run(&[sources_query(frit(&text))]);
    let answer = frames
        .iter()
        .find(|(k, _)| *k == FRAME_SOURCES)
        .map(|(_, p)| String::from_utf8(p.clone()).unwrap())
        .unwrap();
    let lines: Vec<&str> = answer.lines().collect();
    assert_eq!(lines.len(), 2, "{answer}");
    assert!(
        lines[0].starts_with(&format!("missing\t1\tbdf:{}\t", file.display())),
        "{answer}"
    );
    assert_eq!(
        lines[1],
        format!("lacks\t1\tbdf:{}\tU+0042", file.display())
    );
}

// A set whose every font is missing draws nothing, and its read still
// finds what a set with one font left finds: the sources query answers
// a line that says the set draws nothing and why, then a line for each
// missing font, and the console is sent the warning for each missing
// font, so it logs them.
//
// References:
// - ADR-1204 D7: the console logs each missing font, and vtium-config
//   names each one.
#[test]
fn a_set_whose_every_font_is_missing_still_names_each_font() {
    let text =
        "source=font:No Such Family Anywhere\nversion=2.304\nsource=bdf:/nonesuch/gone.bdf\n";
    let frames = run(&[sources_query(frit(text))]);
    let warnings: Vec<String> = frames
        .iter()
        .filter(|(k, _)| *k == FRAME_WARNING)
        .map(|(_, p)| String::from_utf8(p.clone()).unwrap())
        .collect();
    assert_eq!(
        warnings,
        [
            "t needs the font No Such Family Anywhere 2.304 or later, which is not on this machine, so vtium draws its characters from the next font that has them",
            "t needs the font /nonesuch/gone.bdf, which is not on this machine, so vtium draws its characters from the next font that has them",
        ],
        "{frames:?}"
    );
    let answer = frames
        .iter()
        .find(|(k, _)| *k == FRAME_SOURCES)
        .map(|(_, p)| String::from_utf8(p.clone()).unwrap())
        .unwrap();
    let lines: Vec<&str> = answer.lines().collect();
    assert_eq!(lines.len(), 3, "{answer}");
    assert!(
        lines[0].starts_with("nothing\t0\t\tsource=font:No Such Family Anywhere: "),
        "{answer}"
    );
    assert!(
        lines[1].starts_with("missing\t1\tfont:No Such Family Anywhere\t"),
        "{answer}"
    );
    assert!(
        lines[2].starts_with("missing\t3\tbdf:/nonesuch/gone.bdf\t"),
        "{answer}"
    );
    assert!(frames.iter().all(|(k, _)| *k != FRAME_DRAWS_NOTHING));
    // a frit that does not read has no sources to name, and answers
    // with the frame of its own
    let frames = run(&[sources_query(frit("source=\n"))]);
    assert_eq!(
        frames.iter().map(|f| f.0).collect::<Vec<_>>(),
        [FRAME_DRAWS_NOTHING],
        "{frames:?}"
    );
}

// An outline font's version comes from its name table's version
// string, and a lower one than version= asks for is reported while the
// font still draws. A font whose name table states no version is not
// compared.
//
// References:
// - ADR-1204 D7: a font of a lower version still draws, and is
//   reported.
#[test]
fn an_outline_fonts_version_comes_from_its_name_table() {
    let dir = scratch("outline-version");
    let font = ttf::test_mono();
    let differs = |bytes: Vec<u8>| {
        let file = dir.join("t.ttf");
        std::fs::write(&file, bytes).unwrap();
        let text = format!("source=outline:{}\nversion=2.304\n", file.display());
        let engine = Engine::load(&frit(&text), &Paths::default(), None).unwrap();
        assert_eq!(engine.sources.len(), 1, "the font draws");
        engine
            .differs()
            .iter()
            .map(|d| d.what.clone())
            .collect::<Vec<_>>()
    };
    let older = font.build_with_version("Version 2.242; ttfautohint (v1.8.4)");
    assert_eq!(
        outline_version(&older, "").as_deref(),
        Some("Version 2.242; ttfautohint (v1.8.4)")
    );
    assert_eq!(
        differs(older),
        [crate::needs::Differ::Older("2.242".into())]
    );
    assert_eq!(differs(font.build_with_version("Version 2.304")), []);
    assert_eq!(outline_version(&font.build(), ""), None);
    assert_eq!(differs(font.build()), []);
}

// The version reader reads a font's name table with read-fonts, which
// checks every offset, so a file whose table directory points past its
// end has no version and never panics. The bytes are the fuzz input
// that found swash's range check subtracting past zero, as the names
// target ran outline_version.
//
// References:
// - ADR-1204 D7: the helper reads an outline font's version from its
//   name table.
#[test]
fn a_font_whose_tables_point_past_its_end_has_no_version() {
    let cut: &[u8] = &[
        0x4f, 0x54, 0x54, 0x4f, 0x00, 0x01, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x4d, 0x6e, 0x61, 0x6d,
        0x65, 0x54, 0x56, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00,
    ];
    assert_eq!(outline_version(cut, ""), None);
    assert_eq!(outline_version(cut, "Mname"), None);
}

// The same bytes reach swash on every other path the helper takes:
// the coverage of a font, its description, and the drawing of a frit
// that names it. vtium-config checks the font's tables before swash
// reads them, so each path refuses the font with a message, in a debug
// build too, where swash's subtraction past zero would panic. The test
// runs in the test profile, which checks overflow as a debug build does.
#[test]
fn a_font_whose_tables_point_past_its_end_is_refused_on_every_path() {
    let cut: &[u8] = &[
        0x4f, 0x54, 0x54, 0x4f, 0x00, 0x01, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x4d, 0x6e, 0x61, 0x6d,
        0x65, 0x54, 0x56, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00,
    ];
    let damaged = "the font is damaged: its name table points past its own end";
    assert_eq!(outline_coverage(cut, "").unwrap_err(), damaged);
    assert_eq!(outline_coverage(cut, "Mname").unwrap_err(), damaged);
    assert_eq!(
        crate::describe::describe(cut, "font.ttf").unwrap_err(),
        damaged
    );
    let dir = scratch("cut-font");
    let file = dir.join("cut.otf");
    std::fs::write(&file, cut).unwrap();
    let text = format!(
        "source=outline:{}\nsource=builtin:unifont\n",
        file.display()
    );
    let engine = Engine::load(&frit(&text), &Paths::default(), None).unwrap();
    let left: Vec<(usize, bool, &str)> = engine
        .left_out()
        .iter()
        .map(|l| (l.line, l.missing, l.why.as_str()))
        .collect();
    assert_eq!(left, [(1, false, damaged)]);
}

// A font of a lower version than version= asks for, and a file whose
// SHA-256 is not the one sha256= gives, each still draw. The helper
// reports each in its answer to a sources query, and the console logs
// neither, because neither is missing.
//
// References:
// - ADR-1204 D7: a lower version or a different hash still draws, and
//   is reported.
#[test]
fn a_lower_version_and_another_hash_still_draw_and_are_reported() {
    let dir = scratch("versions");
    let file = dir.join("t.bdf");
    let bdf = crate::bdf::tests::bdf("").replace(
        "STARTPROPERTIES 3\n",
        "STARTPROPERTIES 4\nFONT_VERSION \"3.000\"\n",
    );
    std::fs::write(&file, &bdf).unwrap();
    let hash = vtium_font::sha256_hex(&vtium_font::Sha256::digest(bdf.as_bytes()));
    let load = |keys: &str| {
        let text = format!("source=bdf:{}\n{keys}\n", file.display());
        let engine = Engine::load(&frit(&text), &Paths::default(), None).unwrap();
        assert_eq!(engine.sources.len(), 1, "the font draws");
        assert_eq!(engine.left_out(), []);
        assert_eq!(engine.warnings(), [] as [String; 0]);
        engine
            .differs()
            .iter()
            .map(|d| d.what.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(load(&format!("version=3\nsha256={hash}")), []);
    assert_eq!(load("version=2.9"), []);
    assert_eq!(
        load("version=3.1"),
        [crate::needs::Differ::Older("3.000".into())]
    );
    let other = "0".repeat(64);
    assert_eq!(
        load(&format!("version=3.0.1\nsha256={other}")),
        [
            crate::needs::Differ::Older("3.000".into()),
            crate::needs::Differ::OtherHash(hash.clone()),
        ]
    );
    let text = format!("source=bdf:{}\nsha256={other}\n", file.display());
    let frames = run(&[sources_query(frit(&text))]);
    let answer = frames
        .iter()
        .find(|(k, _)| *k == FRAME_SOURCES)
        .map(|(_, p)| String::from_utf8(p.clone()).unwrap())
        .unwrap();
    assert_eq!(answer, format!("sha256\t1\tbdf:{}\t{hash}", file.display()));
}

// A hex: source draws its glyphs in its 8×16 cell, a 16×16 glyph across
// two cells, and a file whose SHA-256 is not the one sha256= gives
// still draws and is reported. A .hex font states no version, so
// version= finds nothing to compare. A gzipped .hex file is refused
// with a warning that names its line, and the frit draws without it.
//
// References:
// - ADR-1204 D5: the hex: source, read uncompressed.
// - ADR-1204 D7: a font that states no version is checked by its
//   sha256= line.
#[test]
fn a_hex_source_draws_and_is_checked_by_its_hash() {
    let dir = scratch("hex");
    let file = dir.join("u.hex");
    let text = format!(
        "0041:{}\n4E00:{}\n",
        "00000000182424427E42424242000000",
        "FF".repeat(32)
    );
    std::fs::write(&file, &text).unwrap();
    let hash = vtium_font::sha256_hex(&vtium_font::Sha256::digest(text.as_bytes()));
    let load = |keys: &str| {
        let text = format!("source=hex:{}\n{keys}\n", file.display());
        Engine::load(&frit(&text), &Paths::default(), None).unwrap()
    };
    let engine = load(&format!("version=17.0.03\nsha256={hash}"));
    assert_eq!(engine.differs(), []);
    assert_eq!(engine.coverage(), [0x41, 0x4e00]);
    assert!(matches!(
        engine.own_cell(16),
        Ok(OwnCell::Bitmap(Cell {
            width: 8,
            height: 16
        }))
    ));
    let (rows, _) = spaced_rows(
        &frit(&format!("source=hex:{}\n", file.display())),
        CELL,
        Band::NONE,
        &['A', '\u{4e00}'],
    );
    let a: Vec<u8> = [
        0, 0, 0, 0, 0x18, 0x24, 0x24, 0x42, 0x7e, 0x42, 0x42, 0x42, 0x42, 0, 0, 0,
    ]
    .to_vec();
    let lit = |r: &Vec<u8>| r.iter().fold(0u8, |b, &p| (b << 1) | u8::from(p != 0));
    assert_eq!(rows[0].iter().map(lit).collect::<Vec<_>>(), a);
    assert!(
        rows[1]
            .iter()
            .all(|r| r.len() == 16 && r.iter().all(|&p| p == 255))
    );
    let other = "0".repeat(64);
    let engine = load(&format!("sha256={other}"));
    assert_eq!(
        engine
            .differs()
            .iter()
            .map(|d| d.what.clone())
            .collect::<Vec<_>>(),
        [crate::needs::Differ::OtherHash(hash)]
    );
    let gz = dir.join("u.hex.gz");
    std::fs::write(&gz, crate::decode::tests::gzip(text.as_bytes())).unwrap();
    let both = format!(
        "source=hex:{}\nsource=hex:{}\n",
        gz.display(),
        file.display()
    );
    let engine = Engine::load(&frit(&both), &Paths::default(), None).unwrap();
    assert_eq!(engine.sources.len(), 1);
    assert!(
        engine
            .warnings()
            .iter()
            .any(|w| w.contains(":1: source=hex:") && w.contains("is gzipped")),
        "{:?}",
        engine.warnings()
    );
}

// A hex: source draws only the characters its cover= lines let it
// give. With the unifont layer's two cover= lines, the source draws
// A and leaves out its glyphs in both private-use areas, where Nerd
// Fonts puts its icons, so an icon that no other source holds draws
// nothing from it. The test reads what is drawn, not the coverage, so
// it fails when the drawing path stops checking cover=.
//
// References:
// - ADR-1204 D5: the unifont layer's two cover= lines leave both
//   private-use areas out.
#[test]
fn a_hex_source_draws_only_what_its_cover_lines_give() {
    let dir = scratch("hex-cover");
    let file = dir.join("u.hex");
    let glyph = "00000000182424427E42424242000000";
    let text = format!("0041:{glyph}\nE0B0:{glyph}\nF0001:{glyph}\n");
    std::fs::write(&file, &text).unwrap();
    let chars = ['A', '\u{e0b0}', '\u{f0001}'];
    let bare = frit(&format!("source=hex:{}\n", file.display()));
    assert_eq!(drawn_chars(&bare, CELL, &chars), chars);
    let covered = frit(&format!(
        "source=hex:{}\ncover=U+0000-U+DFFF\ncover=U+F900-U+EFFFF\n",
        file.display()
    ));
    assert_eq!(drawn_chars(&covered, CELL, &chars), ['A']);
}

// A zstd font whose window is past ZSTD_WINDOW_MAX is refused with a
// warning that names its window, the 16 MiB limit and what to do
#[test]
fn a_zstd_font_with_too_large_a_window_warns_how_to_fix_it() {
    let dir = scratch("window");
    let wide = dir.join("wide.bdf.zst");
    let bdf = crate::bdf::tests::bdf("");
    std::fs::write(
        &wide,
        crate::decode::tests::zstd_frame(0x78, 0, bdf.as_bytes()),
    )
    .unwrap();
    let text = format!("source=builtin:unifont\nsource=bdf:{}\n", wide.display());
    let engine = Engine::load(&frit(&text), &Paths::default(), None);
    let warnings = engine.unwrap().warnings().to_vec();
    assert_eq!(
        warnings,
        [format!(
            "t:2: source=bdf:{}: zstd: the font is compressed with a 32 MiB window and \
                 vtium-config reads at most 16 MiB: recompress it with a window of 16 MiB or \
                 less; the source is left out",
            wide.display()
        )]
    );
}

// `Frit::parse` refuses a name that is not valid, so no engine is
// named anything but the frit's own name
#[test]
fn a_frit_with_an_invalid_name_does_not_load() {
    let bad = FritText {
        name: "a b".to_owned(),
        text: "source=builtin:unifont\n".to_owned(),
    };
    let e = Engine::load(&bad, &Paths::default(), None).err().unwrap();
    assert!(e.contains("cannot be a font set's name"), "{e}");
}

// a set past its frame's bound, of one codepoint and a cluster, is cut
// into the codepoint's and the cluster's, each sent, and ends
#[test]
fn an_oversize_set_of_a_codepoint_and_clusters_is_cut_and_ends() {
    let dir = scratch("cut");
    let font = dir.join("shaping.ttf");
    std::fs::write(&font, super::ttf::test_shaping().build()).unwrap();
    let engine = Engine::load(
        &frit(&format!("source=outline:{}\n", font.display())),
        &Paths::default(),
        None,
    );
    let engine = engine.unwrap();
    let cell = Cell {
        width: 12,
        height: 20,
    };
    let (want, clusters) = (vec![0xe9], vec![vec![0x65, 0x301]]);
    let whole = engine
        .glyphs(
            cell,
            Band::NONE,
            (REGULAR.into(), None),
            &want,
            (&clusters, false),
            &mut BTreeMap::new(),
        )
        .unwrap()
        .unwrap();
    let mut out = Vec::new();
    let sent = send_glyphs(
        &engine,
        (cell, Band::NONE, REGULAR.into(), None),
        (&want, &clusters, false),
        FRAME_GLYPHS,
        (&mut out, &mut BTreeMap::new()),
        whole.len() - 1,
    )
    .unwrap();
    assert_eq!(sent, Ok(true));
    let sets: Vec<Set> = frames(&out)
        .into_iter()
        .map(|(k, p)| {
            assert_eq!(k, FRAME_GLYPHS);
            Set::from_bytes(p).unwrap()
        })
        .collect();
    assert_eq!(sets.len(), 2);
    assert_eq!(sets[0].clusters().count(), 0);
    assert_eq!(
        sets[1].clusters().map(|(_, _, k)| k).collect::<Vec<_>>(),
        [vec!['e', '\u{301}']]
    );
}

// an 8x16 bitmap font scaled 33 times: each glyph takes 17,424 bytes cut
// to its cell of 264x528, past a set's 16 KiB chunk, and the answer
// carries every glyph, each in a chunk of its own, never a failure
// (issue 272, ADR-1302 D6)
#[test]
fn a_bitmap_font_scaled_33_times_is_answered_whole() {
    let dir = scratch("huge");
    let path = dir.join("8x16.psf");
    let font = crate::decode::tests::psf2(8, 16, 3, Some(&[(1, 'A'), (2, 'B')]));
    std::fs::write(&path, font).unwrap();
    let frit = frit(&format!("source=file:{}\n", path.display()));
    let huge = Cell {
        width: 264,
        height: 528,
    };
    let asked = run(&[ServeRequest {
        cluster_width: Default::default(),
        cell: huge,
        band: Band::NONE,
        ..request(ASK, frit.clone(), &['A', 'B'])
    }]);
    let kinds: Vec<u32> = asked.iter().map(|f| f.0).collect();
    assert_eq!(kinds, [FRAME_GLYPHS, FRAME_DONE]);
    let set = Arc::new(Set::from_bytes(asked[0].1.clone()).unwrap());
    assert_eq!(set.cells(), vec![huge]);
    let face = SetFace::new(set, huge.height).unwrap();
    // glyph 1's rows are 0x01, its last column lit, and glyph 2's 0x02;
    // each scaled 33 times and placed across the cell's full width
    for (c, column) in [('A', 7), ('B', 6)] {
        let g = face.draw(&[c], Style::default()).unwrap();
        assert_eq!((g.width(), g.height()), (264, 528));
        for y in [0, 263, 527] {
            for x in 0..264 {
                let lit = g.pixel(x, y).unwrap()[0] != 0;
                assert_eq!(lit, x / 33 == column, "{c:?} at {x},{y}");
            }
        }
    }
    // a fill at that cell is answered too
    let filled = run(&[ServeRequest {
        cluster_width: Default::default(),
        cell: huge,
        band: Band::NONE,
        ..request(FILL, frit, &[])
    }]);
    assert!(
        filled.iter().all(|f| f.0 != FRAME_FAILED),
        "{:?}",
        filled.iter().map(|f| f.0).collect::<Vec<_>>()
    );
    assert!(filled.iter().any(|f| f.0 == FRAME_FILL));
}

// a batch is drawn no larger than a frame holds at its glyphs' largest:
// with room for one 264x528 glyph a frame, each glyph is drawn and sent
// alone
#[test]
fn a_batch_of_large_glyphs_is_drawn_a_frame_at_a_time() {
    let dir = scratch("batch");
    let path = dir.join("8x16.psf");
    let font = crate::decode::tests::psf2(8, 16, 3, Some(&[(1, 'A'), (2, 'B')]));
    std::fs::write(&path, font).unwrap();
    let engine = Engine::load(
        &frit(&format!("source=file:{}\n", path.display())),
        &Paths::default(),
        None,
    )
    .unwrap();
    let huge = Cell {
        width: 264,
        height: 528,
    };
    assert_eq!(engine.glyph_bytes_max(huge), 66 * 528);
    let mut out = Vec::new();
    let sent = send_glyphs(
        &engine,
        (huge, Band::NONE, REGULAR.into(), None),
        (&[u32::from('A'), u32::from('B')], &[], false),
        FRAME_GLYPHS,
        (&mut out, &mut BTreeMap::new()),
        66 * 528 + 4096,
    )
    .unwrap();
    assert_eq!(sent, Ok(true));
    let sets: Vec<Set> = frames(&out)
        .into_iter()
        .map(|(_, p)| Set::from_bytes(p).unwrap())
        .collect();
    assert_eq!(sets.len(), 2);
}

// issue #61: a line of double width draws its color glyphs and its
// clusters at double height as well, so a batch of its glyphs counts
// each at both sizes. Test Mono's glyphs at an 8×16 cell take at most
// 2,048 bytes on a line of double width, 16×16 two cells wide in RGBA,
// and 4,096 at double height. With room for 5,120 bytes a frame, the
// line draws its glyphs one at a time, where a line of double width
// alone would draw two
#[test]
fn a_batch_on_a_line_of_double_width_counts_its_glyphs_at_both_sizes() {
    let dir = scratch("batch-wide");
    let path = dir.join("TestMono.ttf");
    std::fs::write(&path, super::ttf::test_mono().build()).unwrap();
    let engine = Engine::load(
        &frit(&format!("source=outline:{}\n", path.display())),
        &Paths::default(),
        None,
    )
    .unwrap();
    let wide = Doubled::Width.cell(CELL).unwrap();
    let tall = Doubled::Height.cell(CELL).unwrap();
    assert_eq!(engine.glyph_bytes_max(wide), 2048);
    assert_eq!(engine.glyph_bytes_max(tall), 4096);
    let sets = |doubled| {
        let mut out = Vec::new();
        let sent = send_glyphs(
            &engine,
            (CELL, Band::NONE, REGULAR.into(), doubled),
            (&[u32::from('A'), u32::from('W')], &[], false),
            FRAME_GLYPHS,
            (&mut out, &mut BTreeMap::new()),
            5120,
        )
        .unwrap();
        assert_eq!(sent, Ok(true));
        frames(&out).len()
    };
    assert_eq!(sets(Some(Doubled::Width)), 2, "one glyph a frame");
    assert_eq!(sets(None), 1, "both glyphs in one frame");
}

// issue #61: a line of double width draws each color glyph once, at
// double height, and halves it. The rasteriser stretches each outline
// sideways for the line, which a color glyph cannot take, so it leaves
// the color glyph out and names it rather than drawing it for nothing.
// The rasteriser draws a color glyph again, smaller, until it fits its
// cells, so the test compares counts: the line of double width draws
// Test Mono's COLR U+1F600 exactly as often as a pair of double-height
// lines does
#[test]
fn a_double_width_ask_draws_each_color_glyph_once() {
    let dir = scratch("doubled-wide-color-once");
    let path = dir.join("TestMono.ttf");
    std::fs::write(&path, super::ttf::test_mono().build()).unwrap();
    let engine = Engine::load(
        &frit(&format!("source=outline:{}\n", path.display())),
        &Paths::default(),
        None,
    )
    .unwrap();
    let drawn = |doubled| {
        let draws = || crate::outline::raster::COLOR_DRAWS.with(core::cell::Cell::get);
        let before = draws();
        let set = engine
            .glyphs(
                CELL,
                Band::NONE,
                (REGULAR.into(), doubled),
                &[0x1f600],
                (&[], false),
                &mut BTreeMap::new(),
            )
            .unwrap();
        assert!(set.is_some(), "the emoji is answered at {doubled:?}");
        draws() - before
    };
    let tall = drawn(Some(Doubled::Height));
    assert!(tall >= 1, "drawn at double height");
    assert_eq!(
        drawn(Some(Doubled::Width)),
        tall,
        "on a line of double width"
    );
}

/// A sources query for `frit`, as vtium-config asks it.
fn sources_query(frit: FritText) -> ServeRequest {
    ServeRequest {
        kind: SOURCES,
        ..query(frit)
    }
}

/// A cell query for `frit`: at no cell, naming nothing.
fn query(frit: FritText) -> ServeRequest {
    ServeRequest {
        cluster_width: Default::default(),
        cell: Cell {
            width: 0,
            height: 0,
        },
        band: Band::NONE,
        ..request(OWN_CELL, frit, &[])
    }
}

/// The frit `t` whose sources are `sources`, each a PSF2 font written
/// to a file of its own as `(width, height)`, or a source's spelling.
fn psf_frit(name: &str, sources: &[Result<(u32, u32), &str>]) -> (FritText, Scratch) {
    let dir = scratch(name);
    let mut text = String::new();
    for (i, source) in sources.iter().enumerate() {
        match source {
            Ok((w, h)) => {
                let path = dir.join(format!("{i}.psf"));
                let font = crate::decode::tests::psf2(*w, *h, 1, Some(&[(0, 'A')]));
                std::fs::write(&path, font).unwrap();
                text.push_str(&format!("source=file:{}\n", path.display()));
            }
            Err(spelling) => text.push_str(&format!("source={spelling}\n")),
        }
    }
    (frit(&text), dir)
}

fn cell_bytes(width: u32, height: u32) -> Vec<u8> {
    [width.to_le_bytes(), height.to_le_bytes()].concat()
}

// the operator's decision on #145, 2026-10-28: a frit of bitmap sources
// with no cell= takes its first source's own cell, asked of the helper
#[test]
fn a_cell_query_answers_the_frits_own_cell() {
    let (psf, _dir) = psf_frit("own-psf", &[Ok((8, 16))]);
    let (mixed, _mixed_dir) = psf_frit("own-mixed", &[Ok((8, 16)), Err("builtin:unscii-8")]);
    let frames = run(&[query(psf), query(mixed)]);
    assert_eq!(
        frames,
        [
            (FRAME_OWN_CELL, cell_bytes(8, 16)),
            (FRAME_OWN_CELL, cell_bytes(8, 16))
        ]
    );
    let frames = run(&[query(frit(
        "source=builtin:unscii-8\nsource=builtin:unscii-8\n",
    ))]);
    assert_eq!(frames, [(FRAME_OWN_CELL, cell_bytes(8, 8))]);
}

// a later source whose cell does not fit the first's leaves the frit
// with no own cell, and the reason names it
#[test]
fn a_frit_whose_sources_do_not_fit_its_first_has_no_own_cell() {
    let (frit, _dir) = psf_frit("own-unfit", &[Ok((8, 8)), Err("builtin:unifont")]);
    let frames: Vec<(u32, Vec<u8>)> = run(&[
        query(frit),
        query(self::frit("source=file:/nonexistent/x.psf\n")),
    ])
    .into_iter()
    .filter(|f| f.0 != FRAME_WARNING)
    .collect();
    assert_eq!(
        frames.iter().map(|f| f.0).collect::<Vec<_>>(),
        [FRAME_NO_OWN_CELL, FRAME_NO_OWN_CELL]
    );
    let why = String::from_utf8_lossy(&frames[0].1);
    assert!(
        why.contains("source=builtin:unifont's 8x16 cell does not fit"),
        "{why}"
    );
    assert!(String::from_utf8_lossy(&frames[1].1).contains("not on this machine"));
}

/// A cell query for `frit` at a cell `height` pixels high, the height
/// an outline first source is drawn at.
fn query_at(frit: FritText, height: u32) -> ServeRequest {
    ServeRequest {
        cluster_width: Default::default(),
        cell: Cell { width: 0, height },
        band: Band::NONE,
        ..query(frit)
    }
}

/// The frit of `text` and then a `source=outline:` line for each of
/// `fonts`, each written to a file of its own in a directory named
/// for `name`, or a source's spelling.
fn outline_frit(
    name: &str,
    text: &str,
    fonts: &[Result<super::ttf::Font, &str>],
) -> (FritText, Scratch) {
    let dir = scratch(name);
    let mut text = text.to_owned();
    for (i, font) in fonts.iter().enumerate() {
        match font {
            Ok(font) => {
                let path = dir.join(format!("{i}.ttf"));
                std::fs::write(&path, font.build()).unwrap();
                text.push_str(&format!("source=outline:{}\n", path.display()));
            }
            Err(spelling) => text.push_str(&format!("source={spelling}\n")),
        }
    }
    (frit(&text), dir)
}

// ADR-1200 D17 (the maintainer, 2026-11-01): an outline frit that
// states no cell takes its fonts' own, the W rule's width at the
// height the console asks at. Test Mono's W advances 600 units of
// 1000 and its ink fills the advance: 15.5 pixels to the em at 16
// give 9.3, inked over 10 pixels, and 19.5 at 20 give 11.7, over 12
#[test]
fn an_outline_frits_cell_query_answers_the_w_rule_at_the_height_asked() {
    let (frit, _dir) = outline_frit("own-outline", "", &[Ok(super::ttf::test_mono())]);
    let frames = run(&[
        query_at(frit.clone(), 16),
        query_at(frit.clone(), 20),
        query(frit),
    ]);
    assert_eq!(
        frames[..2],
        [
            (FRAME_OUTLINE_CELL, cell_bytes(10, 16)),
            (FRAME_OUTLINE_CELL, cell_bytes(12, 20)),
        ]
    );
    // with no height there is no outline cell to give
    assert_eq!(frames[2].0, FRAME_NO_OWN_CELL);
    let why = String::from_utf8_lossy(&frames[2].1);
    assert!(why.contains("needs the height"), "{why}");
    assert_eq!(frames.len(), 3);
}

// ADR-1200 D17: the first source sets the cell, and later sources draw
// into it. A bitmap first source gives its own cell whatever outline
// source follows, and an outline first source the W rule's, a later
// bitmap source refusing nothing
#[test]
fn the_first_source_sets_the_cell_whatever_kind_follows() {
    let (psf, psf_dir) = psf_frit("own-first-psf", &[Ok((8, 16))]);
    let font = |dir: &Path| {
        let path = dir.join("later.ttf");
        std::fs::write(&path, super::ttf::test_mono().build()).unwrap();
        path
    };
    let bitmap_first = frit(&format!(
        "{}source=outline:{}\n",
        psf.text,
        font(&psf_dir).display()
    ));
    let (outline_first, _dir) = outline_frit(
        "own-first-outline",
        "",
        &[Ok(super::ttf::test_mono()), Err("builtin:unifont")],
    );
    let frames = run(&[
        query_at(bitmap_first.clone(), 20),
        query(bitmap_first),
        query_at(outline_first, 10),
    ]);
    assert_eq!(
        frames,
        [
            (FRAME_OWN_CELL, cell_bytes(8, 16)),
            (FRAME_OWN_CELL, cell_bytes(8, 16)),
            // 9.5 pixels to the em: W advances 5.7, inked over 6; unifont's
            // 8x16 does not fit and fills nothing
            (FRAME_OUTLINE_CELL, cell_bytes(6, 10)),
        ]
    );
}

/// The frit `text` with the layer `name`, whose text is `layer`, in
/// place, as the console hands it over.
fn layered(text: &str, name: &str, layer: &str) -> FritText {
    let mut read = |n: &str| Ok((n == name).then(|| layer.to_owned()));
    frit(&vtium_font::set::layer::expand(text, &mut read).text)
}

/// The characters of `chars` that the helper draws for `frit` at
/// `cell`, in an ask; the rest it leaves to the console.
fn drawn_chars(frit: &FritText, cell: Cell, chars: &[char]) -> Vec<char> {
    let frames = run(&[ServeRequest {
        cluster_width: Default::default(),
        cell,
        band: Band::NONE,
        ..request(ASK, frit.clone(), chars)
    }]);
    let faces: Vec<SetFace> = frames
        .iter()
        .filter(|f| f.0 == FRAME_GLYPHS)
        .filter_map(|f| SetFace::new(Arc::new(Set::from_bytes(f.1.clone()).ok()?), cell.height))
        .collect();
    chars
        .iter()
        .copied()
        .filter(|&c| faces.iter().any(|face| face.has_glyph(c)))
        .collect()
}

// When the set's text font, its own first source, is missing or
// refused, or the set has no source of its own, the built-in font
// takes the text font's place. The frit gives no own cell, so the
// console takes the cell of its configuration, and no later source,
// such as a layer's font, sets the cell.
//
// References:
// - ADR-1204 D5: the cell rule for a missing text font.
#[test]
fn a_missing_text_font_leaves_the_cell_to_the_configuration() {
    let (layered_text, dir) = outline_frit(
        "own-missing-text",
        "source=font:Nonesuch Mono\n",
        &[Ok(super::ttf::test_mono())],
    );
    let broken = dir.join("broken.psf");
    std::fs::write(&broken, b"not a font").unwrap();
    let refused = frit(&format!(
        "source=file:{}\nsource=builtin:unscii-8\n",
        broken.display()
    ));
    let missing_bitmap = frit("source=kbd:nonesuch\nsource=builtin:unscii-8\n");
    let only_layers = layered(
        "layer=small\n",
        "small",
        "role=symbols\nsource=builtin:unscii-8\n",
    );
    let frames: Vec<(u32, Vec<u8>)> = run(&[
        query_at(layered_text, 16),
        query(refused),
        query(missing_bitmap),
        query(only_layers),
    ])
    .into_iter()
    .filter(|f| f.0 != FRAME_WARNING)
    .collect();
    assert_eq!(
        frames.iter().map(|f| f.0).collect::<Vec<_>>(),
        [FRAME_NO_OWN_CELL; 4]
    );
    let why = |i: usize| String::from_utf8_lossy(&frames[i].1).into_owned();
    // the reason names the folders this machine looked in
    assert!(
        why(0).starts_with("the text font source=font:Nonesuch Mono is missing: no font of family Nonesuch Mono: ")
            && why(0).ends_with(". The built-in font draws the text in its place, and the character cell comes from the configuration"),
        "{}",
        why(0)
    );
    assert_eq!(
        why(3),
        "the font set has no font of its own, only layers, so the built-in font draws the text, and the cell comes from the configuration"
    );
}

// A text font that fails as it draws leaves the cell to the frit's next
// own source. A layer's source never sets it, so with no own source
// left the frit has no own cell.
//
// References:
// - ADR-1204 D5: a layer's font never sets the cell.
#[test]
fn a_layers_source_never_sets_the_cell() {
    let dir = scratch("own-layer-cell");
    let ttf = dir.join("mono.ttf");
    std::fs::write(&ttf, super::ttf::test_mono().build()).unwrap();
    let set = layered(
        &format!("source=outline:{}\nlayer=small\n", ttf.display()),
        "small",
        "role=symbols\nsource=builtin:unscii-8\n",
    );
    let engine = Engine::load(&set, &Paths::default(), None).unwrap();
    assert!(matches!(engine.own_cell(16), Ok(OwnCell::Outline(_))));
    engine
        .failed
        .borrow_mut()
        .insert(0, "failed as it drew".to_owned());
    let why = engine.own_cell(16).unwrap_err();
    assert!(why.contains("no source of its own left"), "{why}");
}

// With the text font missing, the built-in font draws the text in the
// text font's place. The helper leaves each character the built-in
// font holds to the console, which draws it from the built-in font, and
// the later sources, here a layer's font, draw only the characters the
// built-in font lacks. With the text font there, it draws them all.
//
// References:
// - ADR-1204 D5 and D7: the built-in font draws the text when the
//   set's text font is missing.
#[test]
fn the_built_in_font_draws_the_text_in_place_of_a_missing_text_font() {
    use vtium_font::Face;
    let dir = scratch("built-in-text");
    let ttf = dir.join("mono.ttf");
    std::fs::write(&ttf, super::ttf::test_mono().build()).unwrap();
    let symbols = format!("role=symbols\nsource=outline:{}\n", ttf.display());
    // an 8-pixel cell draws the built-in Unscii-8, which holds no Han
    let cell = Cell {
        width: 8,
        height: 8,
    };
    let built_in = SetFace::new(
        vtium_font::set::builtin(vtium_font::set::builtin_name(cell.height, "")).unwrap(),
        cell.height,
    )
    .unwrap();
    let chars = ['A', 'W', '\u{4e00}'];
    assert_eq!(chars.map(|c| built_in.has_glyph(c)), [true, true, false]);
    let missing = layered("source=font:Nonesuch Mono\nlayer=sym\n", "sym", &symbols);
    assert_eq!(drawn_chars(&missing, cell, &chars), ['\u{4e00}']);
    let only_layers = layered("layer=sym\n", "sym", &symbols);
    assert_eq!(drawn_chars(&only_layers, cell, &chars), ['\u{4e00}']);
    let present = layered(
        &format!("source=outline:{}\nlayer=sym\n", ttf.display()),
        "sym",
        "role=symbols\nsource=builtin:unscii-8\n",
    );
    assert_eq!(drawn_chars(&present, cell, &chars), chars);
}

// The text font, the frit's own first regular source, sets the baseline
// that the sources drawing at its size stand on, even where a layer's
// regular source comes first in the frit. Test Mono stands on row 13 of
// a 16-pixel cell; Test Low, the layer's font, with its ascent and
// descent at 500 units each, stands on row 8.
//
// References:
// - ADR-1204 D5: a layer's font never sets the cell or its baseline.
// - ADR-1200 D17: the sources at the text font's size stand on its
//   baseline.
#[test]
fn a_layers_regular_source_never_sets_the_baseline() {
    let dir = scratch("layer-baseline");
    let mono = dir.join("mono.ttf");
    std::fs::write(&mono, super::ttf::test_mono().build()).unwrap();
    let mut low = super::ttf::test_mono();
    low.family = "Test Low".to_owned();
    (low.ascent, low.descent) = (500, -500);
    let low_path = dir.join("low.ttf");
    std::fs::write(&low_path, low.build()).unwrap();
    let cell = Cell {
        width: 8,
        height: 16,
    };
    let alone = frit(&format!("source=outline:{}\n", mono.display()));
    let alone = Engine::load(&alone, &Paths::default(), None).unwrap();
    let own = alone.metrics_at(cell).0;
    let low_alone = frit(&format!("source=outline:{}\n", low_path.display()));
    let low_alone = Engine::load(&low_alone, &Paths::default(), None).unwrap();
    assert_ne!(
        low_alone.metrics_at(cell).0,
        own,
        "the two fonts stand apart"
    );
    let text = layered(
        &format!("layer=low\nsource=outline:{}\n", mono.display()),
        "low",
        &format!("role=symbols\nsource=outline:{}\n", low_path.display()),
    );
    let engine = Engine::load(&text, &Paths::default(), None).unwrap();
    assert!(
        engine
            .sources
            .first()
            .is_some_and(|(s, _)| s.layer.is_some()),
        "the layer's source comes first"
    );
    assert_eq!(engine.metrics_at(cell).0, own);
    let at = engine
        .sources
        .iter()
        .position(|(s, _)| own_regular(s))
        .unwrap();
    assert_eq!(engine.baseline(at, cell.height), Some(own));
    assert_eq!(engine.own_baseline(at, cell.height), Some(own));
}

// A layer's bold source is never the frit's bold face, so it never
// joins the W rule that sets the cell's width. Test Wide, the layer's
// bold font, has advances of 900 units where Test Mono's are 600.
//
// References:
// - ADR-1204 D5: a layer's font never sets the cell.
// - ADR-1200 D17: the W rule measures the frit's bold face beside its
//   regular one.
#[test]
fn a_layers_bold_source_never_widens_the_cell() {
    let dir = scratch("layer-bold-width");
    let mono = dir.join("mono.ttf");
    std::fs::write(&mono, super::ttf::test_mono().build()).unwrap();
    let mut wide = super::ttf::test_mono();
    wide.family = "Test Wide".to_owned();
    for glyph in &mut wide.glyphs {
        glyph.advance = 900;
    }
    let wide_path = dir.join("wide.ttf");
    std::fs::write(&wide_path, wide.build()).unwrap();
    let own_cell = |text: &FritText| {
        Engine::load(text, &Paths::default(), None)
            .unwrap()
            .own_cell(16)
    };
    let alone = own_cell(&frit(&format!("source=outline:{}\n", mono.display())));
    let own_bold = own_cell(&frit(&format!(
        "source=outline:{}\nsource=outline:{}\nstyle=bold\n",
        mono.display(),
        wide_path.display()
    )));
    assert!(
        matches!(
            (&own_bold, &alone),
            (Ok(OwnCell::Outline(bold)), Ok(OwnCell::Outline(plain))) if bold.width > plain.width
        ),
        "the frit's own bold face widens the cell: {own_bold:?} against {alone:?}"
    );
    let text = layered(
        &format!("source=outline:{}\nlayer=wide\n", mono.display()),
        "wide",
        &format!(
            "role=script\nsource=outline:{}\nstyle=bold\n",
            wide_path.display()
        ),
    );
    assert_eq!(own_cell(&text), alone);
}

// The built-in font takes the missing text font's place in the set's
// order, so a layer placed before the text font's line draws first:
// Test Mono, the layer's font, draws A, W and U+4E00, and the built-in
// font draws none of them. A layer after the text font's line draws
// only what the built-in font lacks.
//
// References:
// - ADR-1204 D5: the layers are spliced in place, and the built-in font
//   draws the text in the missing text font's place.
#[test]
fn a_layer_before_a_missing_text_font_draws_before_the_built_in_font() {
    let dir = scratch("built-in-after-layer");
    let ttf = dir.join("mono.ttf");
    std::fs::write(&ttf, super::ttf::test_mono().build()).unwrap();
    let symbols = format!("role=symbols\nsource=outline:{}\n", ttf.display());
    let cell = Cell {
        width: 8,
        height: 8,
    };
    let chars = ['A', 'W', '\u{4e00}'];
    let before = layered("layer=sym\nsource=font:Nonesuch Mono\n", "sym", &symbols);
    assert_eq!(drawn_chars(&before, cell, &chars), chars);
    let after = layered("source=font:Nonesuch Mono\nlayer=sym\n", "sym", &symbols);
    assert_eq!(drawn_chars(&after, cell, &chars), ['\u{4e00}']);
}

// Whatever the order, a character the built-in font holds and no
// source before it holds is left to the console, which draws it from
// the built-in font. A frit whose only source is a missing text font
// draws nothing in the helper: the ask fails, and the console draws
// the text in the built-in font. With the text font missing and a
// layer that holds no Latin, after the text font's line or before it,
// the helper draws U+4E00 from the layer and leaves A to the console.
//
// References:
// - ADR-1204 D5 and D7: the built-in font draws the text when the
//   set's text font is missing.
#[test]
fn a_missing_text_font_leaves_a_to_the_built_in_font_in_any_order() {
    let dir = scratch("built-in-lone");
    let mut han = super::ttf::test_mono();
    han.family = "Test Han".to_owned();
    han.cmap.retain(|&(c, _)| c == 0x4e00);
    let ttf = dir.join("han.ttf");
    std::fs::write(&ttf, han.build()).unwrap();
    let script = format!("role=script\nsource=outline:{}\n", ttf.display());
    let cell = Cell {
        width: 8,
        height: 8,
    };
    let chars = ['A', '\u{4e00}'];
    let lone = frit("source=font:Nonesuch Mono\n");
    let frames = run(&[ServeRequest {
        cluster_width: Default::default(),
        cell,
        band: Band::NONE,
        ..request(ASK, lone, &chars)
    }]);
    // the warning for the missing font comes first, so the console logs
    // it, then the failure
    assert_eq!(
        frames.iter().map(|f| f.0).collect::<Vec<_>>(),
        [FRAME_WARNING, FRAME_FAILED],
        "no glyph from a frit with no source here"
    );
    for text in [
        "source=font:Nonesuch Mono\nlayer=han\n",
        "layer=han\nsource=font:Nonesuch Mono\n",
    ] {
        let layered = layered(text, "han", &script);
        assert_eq!(drawn_chars(&layered, cell, &chars), ['\u{4e00}'], "{text}");
    }
}

// In a zh, ja or ko locale the console draws Unifont as its built-in
// font at every height, so a set whose text font is missing leaves Han
// to the console even in an 8-pixel cell, where Unscii-8, which holds
// no Han, would otherwise be the built-in font. The helper picks the
// built-in font from the locale the console passes it.
//
// References:
// - ADR-1200 D7: the built-in font for a cell height and locale.
// - ADR-1204 D5: the built-in font draws the text when the set's text
//   font is missing.
#[test]
fn the_built_in_font_follows_the_consoles_locale() {
    let dir = scratch("built-in-locale");
    let ttf = dir.join("mono.ttf");
    std::fs::write(&ttf, super::ttf::test_mono().build()).unwrap();
    let symbols = format!("role=symbols\nsource=outline:{}\n", ttf.display());
    let missing = layered("source=font:Nonesuch Mono\nlayer=sym\n", "sym", &symbols);
    let cell = Cell {
        width: 8,
        height: 8,
    };
    let chars = ['A', '\u{4e00}'];
    let drawn = |locale: &str| {
        let paths = Paths {
            locale: locale.to_owned(),
            ..Paths::default()
        };
        let engine = Engine::load(&missing, &paths, None).unwrap();
        let want: Vec<u32> = chars.iter().map(|&c| u32::from(c)).collect();
        let Some(bytes) = engine.draw(cell, &want).unwrap() else {
            return Vec::new();
        };
        let face = SetFace::new(Arc::new(Set::from_bytes(bytes).unwrap()), cell.height).unwrap();
        chars
            .iter()
            .copied()
            .filter(|&c| face.has_glyph(c))
            .collect::<Vec<char>>()
    };
    assert_eq!(drawn("C"), ['\u{4e00}']);
    assert_eq!(drawn("en_US.UTF-8"), ['\u{4e00}']);
    for cjk in ["ja_JP.UTF-8", "zh_CN.UTF-8", "ko_KR.UTF-8"] {
        assert!(drawn(cjk).is_empty(), "{cjk}");
    }
}

// The helper is handed a frit with its layers in place, and each of its
// messages names the file and line a line came from: a layer's file
// and the layer's own line for a layer's line, and the frit's own line
// for the frit's.
//
// References:
// - ADR-1204 D5: the splice the console makes.
#[test]
fn the_helpers_messages_name_a_layers_file_and_line() {
    let refused = layered(
        "source=kbd:x\nlayer=sym\n",
        "sym",
        "role=symbols\n# c\n# c\nsource=font:S\ncover=nowhere\n",
    );
    let e = Engine::load(&refused, &Paths::default(), None)
        .err()
        .unwrap();
    assert!(
        e.starts_with("layers/sym.vtlayer:5: cover=nowhere is not one of"),
        "{e}"
    );
    let dir = scratch("layer-messages");
    let broken = dir.join("broken.psf");
    std::fs::write(&broken, b"not a font").unwrap();
    let set = layered(
        "source=builtin:unscii-8\nlayer=sym\nsize=12\nsource=file:/nonesuch.psf\n",
        "sym",
        &format!("role=symbols\n\nsource=file:{}\n", broken.display()),
    );
    let engine = Engine::load(&set, &Paths::default(), None).unwrap();
    assert_eq!(engine.warnings().len(), 2, "{:?}", engine.warnings());
    assert!(
        engine.warnings()[0].starts_with(&format!(
            "layers/sym.vtlayer:3: source=file:{}: ",
            broken.display()
        )),
        "{:?}",
        engine.warnings()
    );
    // a missing font's line names the set, as the console logs it
    assert!(
        engine.warnings()[1].starts_with("t needs the font /nonesuch.psf, which"),
        "{:?}",
        engine.warnings()
    );
    let lines: Vec<(usize, &str)> = engine
        .left_out()
        .iter()
        .map(|l| (l.line, l.spelling.as_str()))
        .collect();
    assert_eq!(
        lines,
        [
            (2, format!("file:{}", broken.display()).as_str()),
            (4, "file:/nonesuch.psf")
        ]
    );
}

// A source that has failed is left out of the cell as it is out of the
// glyphs, and the next source sets the cell: one whose W rule fails, a
// font giving no ascent or descent, and one that failed as it drew
#[test]
fn a_failed_first_source_leaves_the_cell_to_the_next() {
    let flat = super::ttf::Font {
        ascent: 0,
        descent: 0,
        ..super::ttf::test_mono()
    };
    let (broken, _dir) = outline_frit("own-broken", "", &[Ok(flat)]);
    let broken = frit(&format!("{}source=builtin:unscii-8\n", broken.text));
    let frames = run(&[query_at(broken.clone(), 16), query_at(broken, 16)]);
    assert_eq!(
        frames,
        [
            (FRAME_OWN_CELL, cell_bytes(8, 8)),
            (FRAME_OWN_CELL, cell_bytes(8, 8)),
        ]
    );
    let (good, _dir) = outline_frit(
        "own-failed",
        "",
        &[Ok(super::ttf::test_mono()), Err("builtin:unscii-8")],
    );
    let engine = Engine::load(&good, &Paths::default(), None).unwrap();
    assert_eq!(
        engine.own_cell(16),
        Ok(OwnCell::Outline(Cell {
            width: 10,
            height: 16
        }))
    );
    engine
        .failed
        .borrow_mut()
        .insert(0, "failed as it drew".to_owned());
    assert_eq!(
        engine.own_cell(16),
        Ok(OwnCell::Bitmap(Cell {
            width: 8,
            height: 8
        }))
    );
    // with no source left, the reason names each one's failure
    engine.failed.borrow_mut().insert(1, "planted".to_owned());
    let why = engine.own_cell(16).unwrap_err();
    assert!(
        why.contains("failed as it drew") && why.contains("no source of its own left"),
        "{why}"
    );
}

// ADR-1302 D6: the helper sends a glyph's ink box in the set beside its
// pixels cut to its cell, and the console's reader gives it back. Test
// Mono's L here spans -2.9 to 8.8 pixels of a 12-pixel cell
// covers: text.overhang
#[test]
fn an_asks_glyph_carries_its_ink_box() {
    let mut font = super::ttf::test_mono();
    font.glyphs
        .push(super::ttf::Glyph::rect(600, -150, 0, 450, 700));
    font.cmap.push((0x4c, 8));
    font.cmap.sort_unstable();
    let (frit, _dir) = outline_frit("ink", "", &[Ok(font)]);
    let cell = Cell {
        width: 12,
        height: 20,
    };
    let frames = run(&[ServeRequest {
        cluster_width: Default::default(),
        cell,
        band: Band::NONE,
        ..request(ASK, frit, &['L', 'A'])
    }]);
    assert_eq!(frames[0].0, FRAME_GLYPHS);
    let set = Arc::new(Set::from_bytes(frames[0].1.clone()).unwrap());
    let face = SetFace::new(set, 20).unwrap();
    let l = face.draw(&['L'], Style::default()).unwrap();
    let ink = l.ink().unwrap();
    assert_eq!((ink.left(), ink.width()), (-3, 15));
    assert!(face.draw(&['A'], Style::default()).unwrap().ink().is_none());
}

/// A tall main font, whose ascent and descent span 1.5 em, and a short
/// symbol font, whose span 1 em, merged by `cover=` for the Private
/// Use Area and Cyrillic: the symbol font's U+E0B0, a bar from its
/// descent to its ascent, and U+0416, the same bar as a letter.
fn merged_frit(name: &str) -> (FritText, Scratch) {
    let dir = scratch(name);
    let main = super::ttf::Font {
        family: "Tall Mono".to_owned(),
        ascent: 1100,
        descent: -400,
        ..super::ttf::test_mono()
    };
    let symbols = super::ttf::Font {
        family: "Short Symbols".to_owned(),
        ascent: 800,
        descent: -200,
        glyphs: vec![
            super::ttf::Glyph::empty(400),
            super::ttf::Glyph::rect(400, 0, -200, 400, 800),
        ],
        cmap: vec![(0x0416, 1), (0xe0b0, 1)],
        colr: Vec::new(),
        palette: Vec::new(),
        ..super::ttf::test_mono()
    };
    let (main_path, symbols_path) = (dir.join("main.ttf"), dir.join("symbols.ttf"));
    std::fs::write(&main_path, main.build()).unwrap();
    std::fs::write(&symbols_path, symbols.build()).unwrap();
    let text = format!(
        "source=outline:{}\nsource=outline:{}\ncover=U+E000-U+F8FF\ncover=U+0400-U+04FF\n",
        main_path.display(),
        symbols_path.display()
    );
    (frit(&text), dir)
}

// #64: a merged symbol font's private-use glyph is fitted to the main
// font's cell. It draws at the size at which the symbol font's own
// ascent and descent span the cell, so a bar the font draws from its
// descent to its ascent, as a Powerline separator runs, fills the
// cell from its top row to its bottom row. At the main font's size it
// would span only two thirds of the cell. A letter the same font gives
// through cover= keeps the main font's size, on its baseline, as every
// later source's letters do (ADR-1200 D17)
// covers: text.font-merge
#[test]
fn a_merged_symbol_fills_the_main_fonts_cell() {
    let (frit, _dir) = merged_frit("merged-fit");
    let cell = Cell {
        width: 12,
        height: 30,
    };
    let frames = run(&[ServeRequest {
        cluster_width: Default::default(),
        cell,
        band: Band::NONE,
        ..request(ASK, frit, &['\u{e0b0}', '\u{416}'])
    }]);
    assert_eq!(frames[0].0, FRAME_GLYPHS);
    let set = Arc::new(Set::from_bytes(frames[0].1.clone()).unwrap());
    let face = SetFace::new(set, 30).unwrap();
    let rows = |c: char| -> Vec<u32> {
        let g = face.draw(&[c], Style::default()).unwrap();
        (0..g.height())
            .filter(|&y| (0..g.width()).any(|x| g.pixel(x, y).unwrap()[0] > 0))
            .collect()
    };
    assert_eq!(
        rows('\u{e0b0}'),
        (0..30).collect::<Vec<_>>(),
        "the separator"
    );
    let letter = rows('\u{416}');
    assert!(
        letter.len() <= 21,
        "the letter at the main font's size: {letter:?}"
    );
}

// ADR-1006 D8: an ask under `cluster-width = single` has the conjunct
// ka, virama, ssa drawn in one cell, as the console's terminal gives it,
// and the same ask under `double` in two
#[test]
fn an_ask_places_its_clusters_by_its_cluster_width() {
    let mut font = super::ttf::test_mono();
    let first = u16::try_from(font.glyphs.len()).unwrap();
    for (i, c) in [0x915u32, 0x94d, 0x937].into_iter().enumerate() {
        font.glyphs
            .push(super::ttf::Glyph::rect(900, 0, 0, 900, 500));
        font.cmap.push((c, first + u16::try_from(i).unwrap()));
    }
    font.cmap.sort_unstable();
    let (frit, _dir) = outline_frit("cluster-width", "", &[Ok(font)]);
    let key = vec![0x915, 0x94d, 0x937];
    let cells = |rule: ClusterWidth| {
        let frames = run(&[ServeRequest {
            cluster_width: rule,
            clusters: vec![key.clone()],
            ..request(ASK, frit.clone(), &[])
        }]);
        let set = frames
            .iter()
            .find(|(k, _)| *k == FRAME_GLYPHS)
            .map(|(_, p)| Arc::new(Set::from_bytes(p.clone()).unwrap()))
            .unwrap();
        let face = SetFace::new(set, 16).unwrap();
        face.cluster(&['\u{915}', '\u{94d}', '\u{937}'], Style::default())
            .unwrap()
            .cells()
    };
    assert_eq!(cells(ClusterWidth::Single), Width::Narrow);
    assert_eq!(cells(ClusterWidth::Double), Width::Wide);
}

// ADR-1302 D3: every request draws a font on one baseline, found from
// its printable ASCII, whatever the request holds, and the strike
// reports it with lines that follow it. Test Mono's underscore passes
// a 12-pixel cell by a row, so its baseline rises from 10 to 9
// covers: text.overhang
#[test]
fn every_request_draws_a_font_on_one_baseline() {
    let mut font = super::ttf::test_mono();
    font.glyphs
        .push(super::ttf::Glyph::rect(600, 0, -200, 600, -100));
    font.glyphs
        .push(super::ttf::Glyph::rect(600, 100, 0, 500, 500));
    font.cmap.push((0x5f, 8));
    font.cmap.push((0x61, 9));
    font.cmap.sort_unstable();
    let (frit, _dir) = outline_frit("baseline", "", &[Ok(font)]);
    let cell = Cell {
        width: 7,
        height: 12,
    };
    let ask = |c: char| ServeRequest {
        cluster_width: Default::default(),
        cell,
        band: Band::NONE,
        ..request(ASK, frit.clone(), &[c])
    };
    // each in a helper of its own, so no earlier request found it
    let sets: Vec<Set> = ['a', '_', '\u{4e00}']
        .iter()
        .map(|&c| Set::from_bytes(run(&[ask(c)])[0].1.clone()).unwrap())
        .collect();
    for set in &sets {
        let strike = set.strikes().next().unwrap();
        assert_eq!(strike.baseline, 9, "{:?}", set.clusters().count());
        let d = strike.decorations;
        assert!(
            d.underline > strike.baseline,
            "the underline below the baseline"
        );
        assert!(d.strike < strike.baseline, "the strike-through above it");
    }
    let face = SetFace::new(Arc::new(sets.into_iter().next().unwrap()), 12).unwrap();
    let a = face.draw(&['a'], Style::default()).unwrap();
    let inked = |y: usize| a.pixels()[y * 7..(y + 1) * 7].iter().any(|&p| p > 0);
    assert!(inked(8) && !inked(9), "a stands on row 9 asked alone");
}

// ADR-1200 D1: a frit's `hinting=` reaches the glyphs the helper draws
// and the baseline it finds, light where the frit says nothing. In
// DejaVu Sans Mono at 16 pixels, light hinting puts the underscore a
// row past the cell at the ascent's baseline, so the baseline rises
// from 13 to 12, and full hinting keeps it at 13
#[test]
fn a_frits_hinting_reaches_its_outline_glyphs() {
    let font = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/DejaVuSansMono-ascii.ttf"
    );
    let cell = Cell {
        width: 9,
        height: 16,
    };
    for (key, hinting, base) in [
        ("", Hinting::Light, 12),
        ("hinting=light\n", Hinting::Light, 12),
        ("hinting=full\n", Hinting::Full, 13),
    ] {
        let text = format!("{key}source=outline:{font}\n");
        let frames = run(&[ServeRequest {
            cluster_width: Default::default(),
            cell,
            band: Band::NONE,
            ..request(ASK, frit(&text), &['H'])
        }]);
        assert_eq!(frames[0].0, FRAME_GLYPHS, "{key:?}");
        let set = Set::from_bytes(frames[0].1.clone()).unwrap();
        assert_eq!(set.strikes().next().unwrap().baseline, base, "{key:?}");
        let face = SetFace::new(Arc::new(set), 16).unwrap();
        let drawn = face.draw(&['H'], Style::default()).unwrap();
        let want = rasterise(&OutlineRequest {
            cluster_width: Default::default(),
            font: std::fs::read(font).unwrap().into(),
            cells: vec![(9, 16)],
            cover: vec![(0x48, 0x48)],
            hinting,
            ..OutlineRequest::default()
        })
        .unwrap();
        let s = &want.strikes[0];
        assert_eq!(s.baseline, base, "{key:?}");
        assert_eq!(drawn.pixels(), &s.glyphs[0].pixels[..], "{key:?}");
    }
}

// Outline glyphs behind a bitmap first source draw 1-bit, so they look
// like the bitmap font's own; render=smooth draws them antialiased, and
// render=bitmap draws even a first outline source 1-bit. The frit's
// first source is an 8x16 PSF font that holds only A, and DejaVu Sans
// Mono fills a, g and @; a glyph drawn 1-bit holds only fully lit and
// fully dark pixels, and some of them lit. A font: source that turns
// out to be a console font draws 1-bit whatever its render= says, so
// the helper warns that the line does nothing.
//
// References:
// - ADR-1200 D1, the render= row: the default and its override.
// covers: text.outline
#[test]
fn outline_glyphs_behind_a_bitmap_first_source_draw_one_bit() {
    let dejavu = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/DejaVuSansMono-ascii.ttf"
    );
    let (psf, _dir) = psf_frit("render-psf", &[Ok((8, 16))]);
    let outline = format!("source=outline:{dejavu}\n");
    let cases = [
        (format!("{}{outline}", psf.text), true),
        (format!("{}{outline}render=smooth\n", psf.text), false),
        (format!("{}{outline}render=bitmap\n", psf.text), true),
        (outline.clone(), false),
        (format!("{outline}render=bitmap\n"), true),
        (format!("{outline}{}", psf.text), false),
    ];
    let cell = Cell {
        width: 8,
        height: 16,
    };
    for (text, one_bit) in cases {
        let frames = run(&[ServeRequest {
            cell,
            ..request(ASK, frit(&text), &['a', 'g', '@'])
        }]);
        assert_eq!(frames[0].0, FRAME_GLYPHS, "{text}");
        let set = Set::from_bytes(frames[0].1.clone()).unwrap();
        let face = SetFace::new(Arc::new(set), 16).unwrap();
        let mut partial = false;
        for c in ['a', 'g', '@'] {
            let drawn = face.draw(&[c], Style::default()).unwrap();
            let px = drawn.pixels();
            assert!(px.iter().any(|&p| p > 0), "{c:?} has ink: {text}");
            partial |= px.iter().any(|&p| p != 0 && p != 255);
        }
        assert_eq!(partial, !one_bit, "{text}");
    }
    let kbd = scratch("render-kbd");
    std::fs::create_dir_all(kbd.join("consolefonts")).unwrap();
    let font = crate::decode::tests::psf2(8, 16, 1, Some(&[(0, 'A')]));
    std::fs::write(kbd.join("consolefonts/x.psf"), font).unwrap();
    let paths = Paths {
        kbd_dirs: vec![kbd.to_path_buf()],
        font_dirs: Vec::new(),
        ..Paths::default()
    };
    let engine = Engine::load(&frit("source=font:x\nrender=bitmap\n"), &paths, None).unwrap();
    assert_eq!(
        engine.warnings(),
        [
            "t:2: render= is ignored, because font:x is a bitmap font, whose characters always have sharp edges"
        ]
    );
}

// An outline source that draws 1-bit is hinted fully, whatever the
// frit's hinting= says, so its baseline and lines are measured with full
// hinting too. DejaVu Sans Mono's lowest ASCII ink lies on a different
// row under light and under full hinting at 16 and 57 pixels, so a
// baseline measured with light hinting would set the 1-bit glyphs a row
// too high, and at 57 pixels the underscore's ink would pass the cell's
// bottom. This holds for a source behind a bitmap first source, for a
// first source with render=bitmap and its lines, and for a later source
// with render=bitmap that draws on its own baseline.
//
// References:
// - ADR-1200 D1, the render= row: full hinting for a source that draws
//   1-bit.
// - ADR-1302 D3: the baseline from the lowest ASCII ink, hinted as it
//   is drawn.
#[test]
fn a_source_that_draws_one_bit_is_measured_as_it_draws() {
    let dejavu = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/DejaVuSansMono-ascii.ttf"
    );
    let font = std::fs::read(dejavu).unwrap();
    let measured = |h, hinting| crate::outline::baseline(&font, "", h, None, hinting).unwrap();
    for h in [16, 57] {
        assert_ne!(measured(h, Hinting::Light), measured(h, Hinting::Bitmap));
    }
    let load = |text: &str| Engine::load(&frit(text), &Paths::default(), None).unwrap();
    let (psf, _dir) = psf_frit("measured-psf", &[Ok((8, 16))]);
    let outline = format!("source=outline:{dejavu}\n");
    let behind = load(&format!("{}{outline}", psf.text));
    for h in [16, 57] {
        assert_eq!(behind.baseline(1, h), Some(measured(h, Hinting::Bitmap)));
    }
    let smooth = load(&format!("{}{outline}render=smooth\n", psf.text));
    assert_eq!(smooth.baseline(1, 16), Some(measured(16, Hinting::Light)));
    let first = load(&format!("{outline}render=bitmap\n"));
    let base = measured(16, Hinting::Bitmap);
    let lines = crate::outline::decorations(&font, "", (16, base), None, Hinting::Bitmap).unwrap();
    assert_eq!(first.metrics_at(CELL), (base, lines));
    let later = load(&format!("{outline}{outline}cover=ascii\nrender=bitmap\n"));
    assert_eq!(later.baseline(1, 16), Some(measured(16, Hinting::Light)));
    assert_eq!(
        later.own_baseline(1, 16),
        Some(measured(16, Hinting::Bitmap))
    );
}

// The frit's first source, whose kind decides how the outline sources
// with no render= line draw, is its first regular source the engine can
// still draw from. A bold bitmap font in front of a regular outline
// font does not make the outline font draw 1-bit. An outline first
// source that the W rule refuses hands the cell to the bitmap font
// behind it, and the outline font after that draws 1-bit.
//
// References:
// - ADR-1200 D1, the render= row: the default follows the first source.
// - ADR-1200 D17: the first regular source sets the cell.
#[test]
fn the_first_source_is_the_first_regular_one_left() {
    let dejavu = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/DejaVuSansMono-ascii.ttf"
    );
    let (psf, _dir) = psf_frit("first-bold", &[Ok((8, 16))]);
    let bold = format!("{}style=bold\nsource=outline:{dejavu}\n", psf.text);
    let engine = Engine::load(&frit(&bold), &Paths::default(), None).unwrap();
    assert!(!engine.draws_as_bitmap(1), "{bold}");
    let flat = super::ttf::Font {
        ascent: 0,
        descent: 0,
        ..super::ttf::test_mono()
    };
    let (refused, _flat) = outline_frit("first-refused", "", &[Ok(flat)]);
    let text = format!("{}{}source=outline:{dejavu}\n", refused.text, psf.text);
    let engine = Engine::load(&frit(&text), &Paths::default(), None).unwrap();
    assert!(matches!(engine.own_cell(16), Ok(OwnCell::Bitmap(_))));
    assert!(engine.draws_as_bitmap(2), "{text}");
    // an outline first source that failed before anything was decided
    let outline = format!("source=outline:{dejavu}\n");
    let failed = format!("{outline}{}{outline}", psf.text);
    let engine = Engine::load(&frit(&failed), &Paths::default(), None).unwrap();
    engine
        .failed
        .borrow_mut()
        .insert(0, "failed as it drew".to_owned());
    assert!(engine.draws_as_bitmap(2), "{failed}");
}

// The engine decides once whether its outline sources draw 1-bit by
// default. When the frit's outline first source fails as it draws
// later in the session, the bitmap font behind it becomes the first
// source, but the outline font after both keeps drawing antialiased and
// keeps its baseline, so its glyphs never mix the two kinds.
#[test]
fn a_failing_first_source_does_not_change_how_the_others_draw() {
    let dejavu = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/DejaVuSansMono-ascii.ttf"
    );
    let (psf, _dir) = psf_frit("first-fails", &[Ok((8, 16))]);
    let outline = format!("source=outline:{dejavu}\n");
    let text = format!("{outline}{}{outline}", psf.text);
    let engine = Engine::load(&frit(&text), &Paths::default(), None).unwrap();
    assert!(!engine.draws_as_bitmap(2));
    let base = engine.baseline(2, 16);
    engine
        .failed
        .borrow_mut()
        .insert(0, "failed as it drew".to_owned());
    assert!(!engine.draws_as_bitmap(2), "{text}");
    assert_eq!(engine.baseline(2, 16), base);
    // decided by own_cell before any draw, as the console asks for it
    let engine = Engine::load(&frit(&text), &Paths::default(), None).unwrap();
    assert!(matches!(engine.own_cell(16), Ok(OwnCell::Outline(_))));
    engine
        .failed
        .borrow_mut()
        .insert(0, "failed as it drew".to_owned());
    assert!(!engine.draws_as_bitmap(2), "{text}");
}

// one helper keeps a source's baseline for each height apart (review
// finding A7): Test Mono's underscore leaves its baseline at 8 in a
// 10-pixel cell and raises it from 10 to 9 in a 12-pixel one
#[test]
fn the_baseline_is_kept_for_each_height() {
    let mut font = super::ttf::test_mono();
    font.glyphs
        .push(super::ttf::Glyph::rect(600, 0, -200, 600, -100));
    font.cmap.push((0x5f, 8));
    font.cmap.sort_unstable();
    let (frit, _dir) = outline_frit("two-heights", "", &[Ok(font)]);
    let at = |h| ServeRequest {
        cluster_width: Default::default(),
        cell: Cell {
            width: 7,
            height: h,
        },
        band: Band::NONE,
        ..request(ASK, frit.clone(), &['A'])
    };
    let frames = run(&[at(10), at(12), at(10)]);
    let bases: Vec<u32> = frames
        .iter()
        .filter(|f| f.0 == FRAME_GLYPHS)
        .map(|f| {
            Set::from_bytes(f.1.clone())
                .unwrap()
                .strikes()
                .next()
                .unwrap()
                .baseline
        })
        .collect();
    assert_eq!(bases, [8, 9, 8]);
}

// a strike reports its frit's first source's baseline even once that
// source has failed as it drew, so the answers at one cell never change
// mid-session: Test Mono with an underscore that raises its baseline at
// 12 pixels from 10 to 9
#[test]
fn a_strike_keeps_its_first_sources_baseline_after_it_fails() {
    let mut font = super::ttf::test_mono();
    font.glyphs
        .push(super::ttf::Glyph::rect(600, 0, -200, 600, -100));
    font.cmap.push((0x5f, 8));
    font.cmap.sort_unstable();
    let (frit, _dir) = outline_frit("first-baseline", "", &[Ok(font), Err("builtin:unscii-8")]);
    let engine = Engine::load(&frit, &Paths::default(), None).unwrap();
    let cell = Cell {
        width: 7,
        height: 12,
    };
    assert_eq!(engine.metrics_at(cell).0, 9);
    engine
        .failed
        .borrow_mut()
        .insert(0, "failed as it drew".to_owned());
    assert_eq!(engine.metrics_at(cell).0, 9, "after the source fails");
}

// ADR-1200 D17: a frit that states its cell is drawn at it, the W
/// The glyphs an ask for `chars` draws from `frit` at `cell`, whose
/// line spacing adds `band`, as rows, each glyph's in order, with the
/// first strike's lines and baseline.
fn spaced_rows(
    frit: &FritText,
    cell: Cell,
    band: Band,
    chars: &[char],
) -> (Vec<Vec<Vec<u8>>>, (Decorations, u32)) {
    let bytes = run_bytes(&[ServeRequest {
        cluster_width: Default::default(),
        cell,
        band,
        ..request(ASK, frit.clone(), chars)
    }]);
    // every glyph frame names the band its request named
    assert!(
        bands(&bytes).iter().all(|&b| b == band),
        "{:?}",
        bands(&bytes)
    );
    let frames = frames(&bytes);
    let faces: Vec<SetFace> = frames
        .iter()
        .filter(|f| f.0 == FRAME_GLYPHS)
        .map(|f| {
            SetFace::new(Arc::new(Set::from_bytes(f.1.clone()).unwrap()), cell.height).unwrap()
        })
        .collect();
    let glyph = |c: char| {
        faces
            .iter()
            .find_map(|face| face.draw(&[c], Style::default()))
            .unwrap()
    };
    let rows = chars
        .iter()
        .map(|&c| {
            let g = glyph(c);
            assert_eq!(g.height(), cell.height, "{c:?}");
            g.rows().map(<[u8]>::to_vec).collect()
        })
        .collect();
    let baseline = faces[0].set().strikes().next().unwrap().baseline;
    (rows, (faces[0].decorations(), baseline))
}

/// `rows` with `above` blank rows over them and `below` under them.
fn padded(rows: &[Vec<u8>], above: usize, below: usize) -> Vec<Vec<u8>> {
    let blank = vec![0u8; rows[0].len()];
    let mut out = vec![blank.clone(); above];
    out.extend(rows.iter().cloned());
    out.extend(vec![blank; below]);
    out
}

// An outline source's strike= lines name bitmap files of its design. A
// cell up to the tallest strike's height draws the largest strike that
// fits it, placed on the cell's bottom row as a bitmap source is
// placed. Above that height, and in a cell no strike fits, the outline
// draws. A character the strikes lack, W here, draws from the outline
// at every height. The 12-pixel strike draws its A in the cell's last
// column and the 16-pixel strike in the one before it.
//
// References:
// - ADR-1204 D2: the strikes and the switch at the tallest strike.
#[test]
fn an_outline_sources_strikes_draw_up_to_the_tallest_strikes_height() {
    let dir = scratch("strikes");
    let outline = dir.join("mono.ttf");
    std::fs::write(&outline, super::ttf::test_mono().build()).unwrap();
    let strike = |height: u32, glyph: u32| {
        let path = dir.join(format!("{height}.psf"));
        let font = crate::decode::tests::psf2(8, height, 3, Some(&[(glyph, 'A')]));
        std::fs::write(&path, font).unwrap();
        path
    };
    let (twelve, sixteen) = (strike(12, 1), strike(16, 2));
    let plain = frit(&format!("source=outline:{}\n", outline.display()));
    let struck = frit(&format!(
        "source=outline:{}\nstrike=file:{}\nstrike=file:{}\n",
        outline.display(),
        sixteen.display(),
        twelve.display()
    ));
    // the strike's A: its column lit on its rows, at the cell's bottom
    let strike_a = |height: u32, tall: u32, column: usize| -> Vec<Vec<u8>> {
        (0..height)
            .map(|y| {
                let mut row = vec![0u8; 8];
                if y >= height.saturating_sub(tall) {
                    row[column] = 255;
                }
                row
            })
            .collect()
    };
    for height in [10, 12, 14, 15, 16, 17, 20] {
        let cell = Cell { width: 8, height };
        let (drawn, _) = spaced_rows(&struck, cell, Band::NONE, &['A', 'W']);
        let (outline_drawn, _) = spaced_rows(&plain, cell, Band::NONE, &['A', 'W']);
        let want_a = match height {
            12..=15 => strike_a(height, 12, 7),
            16 => strike_a(height, 16, 6),
            _ => outline_drawn[0].clone(),
        };
        assert_ne!(strike_a(height, 12, 7), outline_drawn[0], "{height}");
        assert_eq!(drawn[0], want_a, "A at {height} pixels");
        assert_eq!(drawn[1], outline_drawn[1], "W at {height} pixels");
    }
}

/// A BDF font of `width`×`height` cells, one row of descent, whose `A`
/// lights column `column` on every row, and which holds `B` the same
/// way when `b` says so, so a test tells which font drew a glyph.
fn marked_bdf(width: u32, height: u32, column: u32, b: bool) -> String {
    // a row of `width` pixels is whole bytes of hexadecimal digits
    let bytes = width.div_ceil(8);
    let row = |c: u32| {
        let bits = 1u64 << (bytes * 8 - 1 - c);
        format!("{bits:0digits$X}\n", digits = (bytes * 2) as usize)
    };
    let glyph = |name: &str, code: u32| {
        format!(
            "STARTCHAR {name}\nENCODING {code}\nDWIDTH {width} 0\nBBX {width} {height} 0 -1\nBITMAP\n{}ENDCHAR\n",
            row(column).repeat(height as usize)
        )
    };
    let b = if b { glyph("B", 66) } else { String::new() };
    format!(
        "STARTFONT 2.1\nFONT -test-\nFONTBOUNDINGBOX {width} {height} 0 -1\n\
             STARTPROPERTIES 3\nFONT_ASCENT {}\nFONT_DESCENT 1\n\
             CHARSET_REGISTRY \"ISO10646\"\nENDPROPERTIES\nCHARS {}\n{}{b}ENDFONT\n",
        height - 1,
        if b.is_empty() { 1 } else { 2 },
        glyph("A", 65),
    )
}

/// A frit of a bitmap source of 8×16 cells whose `A` lights column 2,
/// with strikes of 6×12 (column 1) and 16×32 (column 3), each file
/// written to `dir`; only the 8×16 font holds `B`. `tail` follows the
/// strikes.
fn struck_frit(dir: &Path, tail: &str) -> FritText {
    let write = |name: &str, text: String| {
        let path = dir.join(name);
        std::fs::write(&path, text).unwrap();
        path
    };
    let own = write("16.bdf", marked_bdf(8, 16, 2, true));
    let twelve = write("12.bdf", marked_bdf(6, 12, 1, false));
    let thirty_two = write("32.bdf", marked_bdf(16, 32, 3, false));
    frit(&format!(
        "source=bdf:{}\nstrike=bdf:{}\nstrike=bdf:{}\n{tail}",
        own.display(),
        thirty_two.display(),
        twelve.display()
    ))
}

// A bitmap source and its strikes are one font at several heights.
// When the set states no cell, the helper answers the cell of the
// tallest of them no taller than the height the console asks at: no
// cell under 12 pixels, where the built-in font draws the text, 6×12
// from 12, 8×16 from 16, and 16×32 from 32, of which the console takes
// whole multiples. A later bitmap source whose cell is larger than the
// strike's, such as an 8×16 fallback in a 12-pixel cell, does not stop
// the answer, and draws nothing in that cell. With no height, the
// answer is the source's own cell.
//
// References:
// - ADR-1204 D2: the cell of the tallest strike no taller than the
//   automatic rule's height, and a whole multiple of the 32-pixel
//   strike's cell above it.
#[test]
fn a_bitmap_source_with_strikes_gives_the_cell_of_its_tallest_strike_that_fits() {
    let dir = scratch("bitmap-strikes-cell");
    let fallback = dir.join("fallback.bdf");
    std::fs::write(&fallback, marked_bdf(8, 16, 7, true)).unwrap();
    let struck = struck_frit(&dir, &format!("source=bdf:{}\n", fallback.display()));
    let engine = Engine::load(&struck, &Paths::default(), None).unwrap();
    assert_eq!(engine.warnings(), [] as [String; 0]);
    // an outline answer reads as no cell, which no row below expects
    let at = |height: u32| match engine.own_cell(height) {
        Ok(OwnCell::Bitmap(c)) => Ok((c.width, c.height)),
        Ok(OwnCell::Outline(c)) => Err(format!("an outline cell {c:?}")),
        Err(why) => Err(why),
    };
    let why = at(8).unwrap_err();
    assert!(
        why.contains("12 pixels high, taller than the 8-pixel cell")
            && why.contains("built-in font"),
        "{why}"
    );
    assert!(at(11).is_err());
    for (height, cell) in [
        (12, (6, 12)),
        (15, (6, 12)),
        (16, (8, 16)),
        (24, (8, 16)),
        (31, (8, 16)),
        (32, (16, 32)),
        (40, (16, 32)),
        (64, (16, 32)),
    ] {
        assert_eq!(at(height), Ok(cell), "at {height} pixels");
    }
    assert_eq!(at(0), Ok((8, 16)), "no height: the source's own cell");
    // the fallback's 8×16 cell does not fit 6×12, so it draws nothing
    let small = Cell {
        width: 6,
        height: 12,
    };
    assert_eq!(drawn_chars(&struck, small, &['A', 'B']), ['A']);
    // a bitmap source with no strikes and the same fallback answers
    // as before, its own cell, at any height
    let plain = frit(&format!(
        "source=bdf:{}\nsource=bdf:{}\n",
        dir.join("12.bdf").display(),
        fallback.display()
    ));
    let engine = Engine::load(&plain, &Paths::default(), None).unwrap();
    assert!(engine.own_cell(12).is_err(), "8×16 does not fit 6×12");
}

/// The rows a glyph of a [`marked_bdf`] font draws in `cell`: column
/// `column` of a `width`×`height` font at `scale`, centred across, with
/// its top row at `top`.
fn marked_rows(
    cell: Cell,
    (width, height): (u32, u32),
    column: u32,
    (scale, top): (u32, u32),
) -> Vec<Vec<u8>> {
    let x0 = (cell.width - width * scale) / 2 + column * scale;
    (0..cell.height)
        .map(|y| {
            (0..cell.width)
                .map(|x| {
                    let lit =
                        (top..top + height * scale).contains(&y) && (x0..x0 + scale).contains(&x);
                    if lit { 255 } else { 0 }
                })
                .collect()
        })
        .collect()
}

// Each cell draws the tallest of the source and its strikes that fits
// it, and that font alone: a character it lacks draws from the next
// source, never from another strike. A cell between two strikes'
// heights draws the shorter strike, centred in the cell. A cell taller
// than every strike draws the tallest at the largest whole factor that
// fits, centred. A cell that no strike fits draws nothing from the
// source.
//
// References:
// - ADR-1204 D2: the next smaller strike centred in a cell between two
//   strikes, and the 32-pixel strike at the largest whole factor above
//   it.
#[test]
fn a_bitmap_source_draws_its_strike_for_each_cell_centred() {
    let dir = scratch("bitmap-strikes-draw");
    let struck = struck_frit(&dir, "");
    let cell = |width, height| Cell { width, height };
    for (at, font, column, placed) in [
        (cell(6, 12), (6, 12), 1, (1, 0)),
        (cell(7, 14), (6, 12), 1, (1, 1)),
        (cell(8, 16), (8, 16), 2, (1, 0)),
        (cell(10, 20), (8, 16), 2, (1, 2)),
        (cell(14, 31), (8, 16), 2, (1, 7)),
        (cell(16, 32), (16, 32), 3, (1, 0)),
        (cell(16, 40), (16, 32), 3, (1, 4)),
        (cell(32, 64), (16, 32), 3, (2, 0)),
        (cell(33, 70), (16, 32), 3, (2, 3)),
    ] {
        let (rows, _) = spaced_rows(&struck, at, Band::NONE, &['A']);
        assert_eq!(
            rows[0],
            marked_rows(at, font, column, placed),
            "A in {at:?}"
        );
    }
    // B is the 8×16 font's alone, so the strikes' cells lack it
    assert_eq!(drawn_chars(&struck, cell(6, 12), &['A', 'B']), ['A']);
    assert_eq!(drawn_chars(&struck, cell(8, 16), &['A', 'B']), ['A', 'B']);
    assert_eq!(drawn_chars(&struck, cell(16, 32), &['A', 'B']), ['A']);
    assert!(drawn_chars(&struck, cell(8, 8), &['A']).is_empty());
}

// A strike whose file has another SHA-256 than the sha256= line after
// it gives still draws, and the helper reports it under the strike's
// own line and spelling. A strike whose hash matches is not reported.
//
// References:
// - ADR-1204 D7: a file with another hash still draws, and is
//   reported.
#[test]
fn a_strike_with_another_hash_draws_and_is_reported() {
    let dir = scratch("bitmap-strikes-hash");
    let twelve = dir.join("12.bdf");
    let text = marked_bdf(6, 12, 1, false);
    std::fs::write(&twelve, &text).unwrap();
    let hash = vtium_font::sha256_hex(&vtium_font::Sha256::digest(text.as_bytes()));
    let own = dir.join("16.bdf");
    std::fs::write(&own, marked_bdf(8, 16, 2, true)).unwrap();
    let load = |sha: &str| {
        let text = format!(
            "source=bdf:{}\nstrike=bdf:{}\nsha256={sha}\n",
            own.display(),
            twelve.display()
        );
        Engine::load(&frit(&text), &Paths::default(), None).unwrap()
    };
    assert_eq!(load(&hash).differs(), []);
    let engine = load(&"0".repeat(64));
    assert_eq!(engine.strikes[0].len(), 1, "the strike still draws");
    assert_eq!(
        engine.differs(),
        [crate::needs::Differs {
            line: 2,
            spelling: format!("bdf:{}", twelve.display()),
            what: crate::needs::Differ::OtherHash(hash),
        }]
    );
}

// The frit's coverage holds what it draws at every height, so a
// character that only a strike holds is not in it, though a cell up to
// the strike's height draws it.
//
// References:
// - ADR-1204 D2: the strikes draw only up to the tallest strike's
//   height.
#[test]
fn a_character_only_a_strike_holds_is_not_in_the_coverage() {
    let dir = scratch("strike-coverage");
    let outline = dir.join("mono.ttf");
    std::fs::write(&outline, super::ttf::test_mono().build()).unwrap();
    let strike = dir.join("12.psf");
    std::fs::write(
        &strike,
        crate::decode::tests::psf2(8, 12, 3, Some(&[(1, 'Z')])),
    )
    .unwrap();
    let struck = frit(&format!(
        "source=outline:{}\nstrike=file:{}\n",
        outline.display(),
        strike.display()
    ));
    let engine = Engine::load(&struck, &Paths::default(), None).unwrap();
    assert!(engine.coverage().contains(&u32::from('A')));
    assert!(!engine.coverage().contains(&u32::from('Z')));
    let small = Cell {
        width: 8,
        height: 12,
    };
    assert_eq!(drawn_chars(&struck, small, &['Z']), ['Z']);
    assert!(drawn_chars(&struck, CELL, &['Z']).is_empty());
}

// A strike this machine lacks is reported as a missing font is: the
// console's line that names the set, the strike's font and the font
// it is a size of, and a missing font under the strike's own line in
// the answer to a sources query. A strike that is here and refused is
// a warning naming its line.
//
// References:
// - ADR-1204 D2: the strike= key.
// - ADR-1204 D7: a missing font is logged and reported.
#[test]
fn a_strike_this_machine_lacks_is_reported_and_a_bad_one_warns() {
    let dir = scratch("strikes-bad");
    let outline = dir.join("mono.ttf");
    std::fs::write(&outline, super::ttf::test_mono().build()).unwrap();
    let broken = dir.join("broken.psf");
    std::fs::write(&broken, b"not a font").unwrap();
    let gone = dir.join("gone.psf");
    let text = format!(
        "source=outline:{}\nstrike=file:{}\nstrike=file:{}\n",
        outline.display(),
        gone.display(),
        broken.display()
    );
    let engine = Engine::load(&frit(&text), &Paths::default(), None).unwrap();
    assert_eq!(engine.warnings().len(), 2, "{:?}", engine.warnings());
    assert_eq!(
        engine.warnings()[0],
        format!(
            "t needs the font {}, which is not on this machine. That font is {} at one size, so vtium draws {} at that size without it",
            gone.display(),
            outline.display(),
            outline.display()
        )
    );
    assert!(
        engine.warnings()[1].starts_with(&format!("t:3: strike=file:{}: ", broken.display())),
        "{:?}",
        engine.warnings()
    );
    assert!(
        engine.warnings()[1].ends_with(". Because of this error, vtium leaves the strike out.")
    );
    assert_eq!(
        engine.left_out,
        [LeftOut {
            missing: true,
            line: 2,
            spelling: format!("file:{}", gone.display()),
            why: format!("{} is not on this machine", gone.display()),
        }]
    );
    assert!(engine.strikes.iter().all(Vec::is_empty));
}

// ADR-1302 D13: where line spacing adds rows to the cell, the helper
// draws each glyph in the text band and places it in the whole cell.
// A bitmap letter keeps its rows with blank rows above and below, a
// box-drawing line stretches by repeating its top and bottom rows, and
// the strike's lines move down by the rows added above. A frit's
// `fit=natural` keeps the line at its size too.
#[test]
fn a_spaced_bitmap_glyph_is_padded_or_stretched_by_its_fit() {
    let unscii = frit("source=builtin:unscii-8\n");
    let text = Cell {
        width: 8,
        height: 8,
    };
    let cell = Cell {
        width: 8,
        height: 11,
    };
    let band = Band { above: 2, below: 1 };
    let chars = ['A', '\u{2502}', '\u{2588}'];
    let (plain, (plain_lines, plain_base)) = spaced_rows(&unscii, text, Band::NONE, &chars);
    let (rows, (lines, base)) = spaced_rows(&unscii, cell, band, &chars);
    assert_eq!(rows[0], padded(&plain[0], 2, 1), "A keeps its size");
    for (i, c) in [(1, "a box line"), (2, "a full block")] {
        let p = &plain[i];
        let mut want = vec![p[0].clone(), p[0].clone()];
        want.extend(p.iter().cloned());
        want.push(p[7].clone());
        assert_eq!(rows[i], want, "{c} fills the cell");
    }
    assert_eq!(lines.underline, plain_lines.underline + 2);
    assert_eq!(lines.strike, plain_lines.strike + 2);
    // the baseline moves down with the text band, so a bitmap glyph
    // sheared for italic about it leans as it does with no spacing
    assert_eq!(base, plain_base + 2);
    let lean = |cell: Cell, band: Band| -> Vec<Vec<u8>> {
        let frames = run(&[ServeRequest {
            cluster_width: Default::default(),
            cell,
            band,
            ..request(ASK, unscii.clone(), &['A'])
        }]);
        let set = Arc::new(Set::from_bytes(frames[0].1.clone()).unwrap());
        let face = SetFace::new(set, cell.height).unwrap();
        let italic = Style {
            bold: false,
            italic: true,
        };
        let g = face.draw(&['A'], italic).unwrap();
        g.rows().map(<[u8]>::to_vec).collect()
    };
    let (plain_italic, spaced_italic) = (lean(text, Band::NONE), lean(cell, band));
    assert_eq!(
        spaced_italic,
        padded(&plain_italic, 2, 1),
        "the italic A leans alike"
    );
    let natural = frit("source=builtin:unscii-8\nfit=natural\n");
    let (rows, _) = spaced_rows(&natural, cell, band, &chars);
    assert_eq!(
        rows[1],
        padded(&plain[1], 2, 1),
        "fit=natural keeps the line's size"
    );
}

// ADR-1302 D13: an outline letter keeps its size in the text band; the
// same glyph mapped from a Powerline codepoint, which is `fill`, scales
// vertically to the whole cell; a color glyph keeps its size whatever
// `fit=` says
#[test]
fn a_spaced_outline_glyph_is_padded_scaled_or_kept_by_its_fit() {
    let (mono, _dir) = outline_frit("spaced", "", &[Ok(super::ttf::test_mono())]);
    let text = Cell {
        width: 12,
        height: 20,
    };
    let cell = Cell {
        width: 12,
        height: 26,
    };
    let band = Band { above: 3, below: 3 };
    let chars = ['A', '\u{e0a0}', '\u{1f600}'];
    let (plain, (_, plain_base)) = spaced_rows(&mono, text, Band::NONE, &chars);
    let (rows, (_, base)) = spaced_rows(&mono, cell, band, &chars);
    assert_eq!(
        base,
        plain_base + 3,
        "the baseline moves down with the text band"
    );
    assert_eq!(rows[0], padded(&plain[0], 3, 3), "A keeps its size");
    assert_ne!(
        rows[1],
        padded(&plain[1], 3, 3),
        "the Powerline glyph is stretched"
    );
    assert_eq!(
        spacing::scale_rows(&plain[1].concat(), 12, band).unwrap(),
        rows[1].concat(),
        "the Powerline glyph is scaled vertically"
    );
    assert_eq!(rows[2], padded(&plain[2], 3, 3), "the emoji keeps its size");
    let (fill, _dir) = outline_frit("spaced-fill", "", &[Ok(super::ttf::test_mono())]);
    let fill = frit(&format!("{}fit=fill\n", fill.text));
    let (rows, _) = spaced_rows(&fill, cell, band, &chars);
    assert_eq!(
        rows[2],
        padded(&plain[2], 3, 3),
        "fit=fill does not stretch a color glyph"
    );
    assert_eq!(rows[0], rows[1], "fit=fill stretches the letter");
}

// ADR-1306 D8: under line spacing a natural glyph's ink may reach past
// its text band through the rows the band adds and then a row of the
// whole cell. Test Mono's U+2191 here is a bar from 1400 to 2200 units
// above the baseline: at 19.5 pixels to the em in the 12x20 text band,
// baseline 16, it reaches 42.9 pixels up, 26.9 over the text band's top,
// more than the whole cell's 26 rows but less than the 3 added rows and
// those 26. Drawn with a band of 3 above and 3 below, its box starts 24
// rows over the 26-row cell's top, where a reach of a row alone, without
// the added rows, would have cut it at 23
#[test]
fn a_spaced_glyphs_ink_reaches_through_the_added_rows_and_a_row_more() {
    let mut font = super::ttf::test_mono();
    font.glyphs
        .push(super::ttf::Glyph::rect(600, 100, 1400, 500, 2200));
    let bar = u16::try_from(font.glyphs.len() - 1).unwrap();
    font.cmap.push((0x2191, bar));
    font.cmap.sort_unstable();
    let (mono, _dir) = outline_frit("spaced-reach", "", &[Ok(font)]);
    let cell = Cell {
        width: 12,
        height: 26,
    };
    let band = Band { above: 3, below: 3 };
    let bytes = run_bytes(&[ServeRequest {
        cluster_width: Default::default(),
        cell,
        band,
        ..request(ASK, mono, &['\u{2191}'])
    }]);
    let g = frames(&bytes)
        .iter()
        .filter(|f| f.0 == FRAME_GLYPHS)
        .find_map(|f| {
            let set = Set::from_bytes(f.1.clone()).unwrap();
            SetFace::new(Arc::new(set), cell.height)
                .unwrap()
                .draw(&['\u{2191}'], Style::default())
        })
        .unwrap();
    // the bar's ink over the cell
    let ink = g.ink().unwrap();
    assert_eq!((ink.left(), ink.top(), ink.width()), (0, -24, 12));
    let row = |y: usize| &ink.pixels()[y * 12..(y + 1) * 12];
    assert!(row(0).iter().any(|&p| p > 0), "the bar's top row");
    assert!(row(1)[4] == 255, "the bar under its top row");
}

// ADR-1302 D13: two fills at one cell size with two bands are two
// fills, each drawn at its band, each ending with a frame that names
// its band; neither replaces the other
#[test]
fn one_cell_size_with_two_bands_fills_twice() {
    let unscii = frit("source=builtin:unscii-8\n");
    let band = Band { above: 2, below: 2 };
    let fill = |band| ServeRequest {
        cluster_width: Default::default(),
        band,
        ..request(FILL, unscii.clone(), &['A'])
    };
    let bytes = run_bytes(&[fill(Band::NONE), fill(band)]);
    let ends: Vec<Vec<u8>> = raw_frames(&bytes)
        .into_iter()
        .filter(|f| f.0 == FRAME_FILLED)
        .map(|f| f.1)
        .collect();
    assert_eq!(
        ends,
        [spaced_payload(CELL, Band::NONE), spaced_payload(CELL, band)]
    );
    let bands = bands(&bytes);
    assert!(
        bands.contains(&Band::NONE) && bands.contains(&band),
        "{bands:?}"
    );
}

// ADR-1302 D13: a stop at a cell with a band ends that band's fill
// alone, and the helper's word that it took it names the band
#[test]
fn a_stop_at_a_band_is_taken_naming_the_band() {
    let unscii = frit("source=builtin:unscii-8\n");
    let band = Band { above: 2, below: 1 };
    let at = |kind| ServeRequest {
        cluster_width: Default::default(),
        band,
        ..request(kind, unscii.clone(), &['A'])
    };
    let frames = run(&[at(FILL), at(STOP)]);
    let stops: Vec<&Vec<u8>> = frames
        .iter()
        .filter(|f| f.0 == FRAME_STOPPED)
        .map(|f| &f.1)
        .collect();
    assert_eq!(stops, [&spaced_payload(CELL, band)]);
}

// ADR-1302 D13: a request whose line spacing leaves its cell no text
// band, or a query that names any, is refused
#[test]
fn line_spacing_that_leaves_no_text_band_is_refused() {
    let (unscii, base) = (
        frit("source=builtin:unscii-8\n"),
        request(ASK, frit("source=builtin:unscii-8\n"), &['A']),
    );
    for (cell_h, above, below, ok) in [
        (11, 2, 1, true),
        (11, 6, 5, false),
        (11, 11, 0, false),
        (11, 0, 11, false),
    ] {
        let r = ServeRequest {
            cluster_width: Default::default(),
            cell: Cell {
                width: 8,
                height: cell_h,
            },
            band: Band { above, below },
            ..base.clone()
        };
        let read = ServeRequest::read(&mut std::io::Cursor::new(r.encode()));
        assert_eq!(read.is_ok(), ok, "{above}+{below} of {cell_h}");
    }
    let query = ServeRequest {
        cluster_width: Default::default(),
        band: Band { above: 1, below: 0 },
        ..super::tests::query(unscii)
    };
    assert!(ServeRequest::read(&mut std::io::Cursor::new(query.encode())).is_err());
}

// rule never widening it; the console reads cell= itself and asks the
// helper only where D17 has it ask, so a cell query answers the fonts'
// own cell whatever cell= says
#[test]
fn a_frit_that_states_its_cell_is_drawn_at_it() {
    let (frit, _dir) = outline_frit("own-stated", "cell=10x20\n", &[Ok(super::ttf::test_mono())]);
    let stated = Cell {
        width: 10,
        height: 20,
    };
    let asked = run(&[ServeRequest {
        cluster_width: Default::default(),
        cell: stated,
        band: Band::NONE,
        ..request(ASK, frit.clone(), &['W'])
    }]);
    let queried = run(&[query_at(frit, 20)]);
    assert_eq!(
        asked.iter().map(|f| f.0).collect::<Vec<_>>(),
        [FRAME_GLYPHS, FRAME_DONE]
    );
    let set = Set::from_bytes(asked[0].1.clone()).unwrap();
    assert_eq!(set.cells(), vec![stated]);
    assert_eq!(queried, [(FRAME_OUTLINE_CELL, cell_bytes(12, 20))]);
}

// a cell query names no cell, no codepoint and no range
#[test]
fn a_cell_query_at_a_cell_or_naming_codepoints_is_refused() {
    let good = query(frit("")).encode();
    assert!(ServeRequest::read(&mut &good[..]).unwrap().is_some());
    let high = query_at(frit(""), CELL_SIDE_MAX).encode();
    let read = ServeRequest::read(&mut &high[..]).unwrap().unwrap();
    assert_eq!((read.cell.width, read.cell.height), (0, CELL_SIDE_MAX));
    for bad in [
        ServeRequest {
            cluster_width: Default::default(),
            cell: CELL,
            band: Band::NONE,
            ..query(frit(""))
        },
        ServeRequest {
            cluster_width: Default::default(),
            cell: Cell {
                width: 8,
                height: 0,
            },
            band: Band::NONE,
            ..query(frit(""))
        },
        query_at(frit(""), CELL_SIDE_MAX + 1),
        ServeRequest {
            cluster_width: Default::default(),
            codepoints: vec![0x41],
            clusters: Vec::new(),
            ..query(frit(""))
        },
        ServeRequest {
            cluster_width: Default::default(),
            held: vec![(0x41, 0x41)],
            ..query(frit(""))
        },
    ] {
        assert!(
            ServeRequest::read(&mut &bad.encode()[..]).is_err(),
            "{bad:?}"
        );
    }
}

#[test]
fn a_frit_name_outside_d1s_set_is_refused() {
    let named = |name: &str| {
        ServeRequest {
            cluster_width: Default::default(),
            frit: FritText {
                name: name.to_owned(),
                text: String::new(),
            },
            ..request(ASK, frit(""), &[])
        }
        .encode()
    };
    for name in ["a/b", "../a", ".a", "a b", "é", "a\u{7f}"] {
        let e = ServeRequest::read(&mut &named(name)[..]).unwrap_err();
        assert!(e.contains("a frit's name"), "{name:?}: {e}");
    }
    let fine = "Ter-v16n_1.2+x";
    let read = ServeRequest::read(&mut &named(fine)[..]).unwrap().unwrap();
    assert_eq!(read.frit.name, fine);
    let longest = "a".repeat(NAME_BYTES_MAX);
    assert!(ServeRequest::read(&mut &named(&longest)[..]).is_ok());
}

#[test]
fn an_ask_is_answered_whole_with_only_what_it_names() {
    let frit = frit("source=builtin:unscii-8\n");
    let frames = run(&[request(ASK, frit, &['Z', 'A', '\u{10fffd}'])]);
    assert_eq!(
        frames.iter().map(|f| f.0).collect::<Vec<_>>(),
        [FRAME_GLYPHS, FRAME_DONE]
    );
    let faces = faces(&frames);
    let f = &faces[0].1;
    assert!(f.has_glyph('A') && f.has_glyph('Z') && !f.has_glyph('B'));
    // Unscii-8's 8x8 sits on the bottom half of an 8x16 cell
    let a = f.draw(&['A'], Style::default()).unwrap();
    assert_eq!((a.width(), a.height()), (8, 16));
    assert!((0..8).all(|y| (0..8).all(|x| a.pixel(x, y).unwrap()[0] == 0)));
}

#[test]
fn a_fill_draws_the_screen_then_latin_and_box_then_the_rest_once() {
    let frit = frit("source=builtin:unscii-8\n");
    let fill = ServeRequest {
        cluster_width: Default::default(),
        held: vec![(0x42, 0x42)],
        ..request(FILL, frit, &['α'])
    };
    let frames = run(&[fill]);
    assert_eq!(frames.last().map(|f| f.0), Some(FRAME_FILLED));
    assert_eq!(
        frames.last().unwrap().1,
        [8, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    let faces = faces(&frames);
    assert!(faces.len() > 1, "{}", faces.len());
    assert!(faces.iter().all(|f| f.0 == FRAME_FILL));
    let first = &faces[0].1;
    // the screen's α, then Latin and box drawing, then the rest in
    // codepoint order: Unscii-8's last, U+1FBF9, comes later
    for c in ['α', 'A', 'é', '─', '█'] {
        assert!(first.has_glyph(c), "{c}");
    }
    assert!(!first.has_glyph('\u{1fbf9}'));
    // every glyph once, and none the console holds
    for c in ['A', 'α', 'β', '─', '\u{1fbf9}'] {
        assert_eq!(faces.iter().filter(|f| f.1.has_glyph(c)).count(), 1, "{c}");
    }
    assert!(faces.iter().all(|f| !f.1.has_glyph('B')));
}

// The fill's first batch holds General Punctuation, Arrows and
// Geometric Shapes beside printable Latin and box drawing, so a
// program's symbols arrive with its text
//
// References:
// - ADR-1200 D18, note of 2026-11-25: the ranges the fill's second
//   tier gains.
#[test]
fn a_fills_first_batch_holds_punctuation_arrows_and_geometric_shapes() {
    let frit = frit("source=builtin:unscii-8\n");
    let frames = run(&[request(FILL, frit, &[])]);
    assert_eq!(frames.last().map(|f| f.0), Some(FRAME_FILLED));
    let faces = faces(&frames);
    assert!(faces.len() > 1, "{}", faces.len());
    let first = &faces[0].1;
    let wider = [
        (0x2010, 0x2027),
        (0x2030, 0x205e),
        (0x2190, 0x21ff),
        (0x25a0, 0x25ff),
    ];
    let covered: Vec<char> = wider
        .iter()
        .flat_map(|&(a, b)| a..=b)
        .filter_map(char::from_u32)
        .filter(|&c| faces.iter().any(|f| f.1.has_glyph(c)))
        .collect();
    // Unscii-8 draws many characters of each range
    for c in ['—', '…', '‰', '←', '■', '▶', '●'] {
        assert!(covered.contains(&c), "{c}");
    }
    for c in covered {
        assert!(
            first.has_glyph(c),
            "U+{:04X} in the first batch",
            u32::from(c)
        );
    }
}

#[test]
fn an_ask_waits_at_most_one_batch_of_a_fill() {
    // unifont's coverage is some sixty batches; an ask sent behind the
    // fill is answered after at most one of them
    let frit = frit("source=builtin:unifont\n");
    let frames = run(&[
        request(FILL, frit.clone(), &[]),
        request(ASK, frit, &['中']),
    ]);
    let kinds: Vec<u32> = frames.iter().map(|f| f.0).collect();
    let asked = kinds.iter().position(|&k| k == FRAME_GLYPHS).unwrap();
    assert!(asked <= 1, "{:?}", &kinds[..asked + 1]);
    assert_eq!(kinds[asked + 1], FRAME_DONE);
    assert!(kinds.iter().filter(|&&k| k == FRAME_FILL).count() > 10);
    assert_eq!(kinds.last(), Some(&FRAME_FILLED));
    // the ask and the fill are two requests: each sends 中 once, and
    // the console drops the fill's
    let faces = faces(&frames);
    let sent = |kind| {
        faces
            .iter()
            .filter(|f| f.0 == kind && f.1.has_glyph('中'))
            .count()
    };
    assert_eq!((sent(FRAME_GLYPHS), sent(FRAME_FILL)), (1, 1));
}

#[test]
fn a_stop_ends_its_cells_fill_and_a_fill_replaces_it() {
    let frit = frit("source=builtin:unscii-8\n");
    let paths = Paths::default();
    let mut work = Work {
        paths: &paths,
        gate: Gate::Open,
        engines: BTreeMap::new(),
        left_out: 0,
        console: None,
        asks: VecDeque::new(),
        fills: VecDeque::new(),
    };
    let mut out = Vec::new();
    work.take(request(FILL, frit.clone(), &[]), &mut out)
        .unwrap();
    work.take(request(FILL, frit.clone(), &['α']), &mut out)
        .unwrap();
    assert_eq!(work.fills.len(), 1);
    assert_eq!(work.fills[0].order[0], u32::from('α'));
    assert!(out.is_empty());
    work.take(request(STOP, frit, &[]), &mut out).unwrap();
    assert!(work.fills.is_empty());
    assert_eq!(
        frames(&out),
        [(FRAME_STOPPED, spaced_payload(CELL, Band::NONE))]
    );
}

// a stop is answered as it is taken, between the stopped fill's last
// frame and the next fill's first, and answered with no fill too
#[test]
fn a_stop_is_answered_between_the_fill_it_ends_and_the_next() {
    let frit = frit("source=builtin:unscii-8\n");
    let frames = run(&[
        request(STOP, frit.clone(), &[]),
        request(FILL, frit.clone(), &['A']),
        request(STOP, frit.clone(), &[]),
        request(FILL, frit, &['A']),
    ]);
    let kinds: Vec<u32> = frames.iter().map(|f| f.0).collect();
    let stops: Vec<usize> = (0..kinds.len())
        .filter(|&i| kinds[i] == FRAME_STOPPED)
        .collect();
    assert_eq!(stops.len(), 2, "{kinds:?}");
    assert!(
        stops
            .iter()
            .all(|&i| frames[i].1 == spaced_payload(CELL, Band::NONE))
    );
    // the first stop, with no fill, comes first; the first fill's
    // frames, and its end if it reached it, come before the second,
    // and the second fill runs whole after it
    assert_eq!(stops[0], 0);
    assert!(
        kinds[..stops[1]]
            .iter()
            .filter(|&&k| k == FRAME_FILLED)
            .count()
            <= 1
    );
    let after = &kinds[stops[1] + 1..];
    assert_eq!(after.first(), Some(&FRAME_FILL), "{kinds:?}");
    assert_eq!(after.last(), Some(&FRAME_FILLED), "{kinds:?}");
    assert_eq!(after.iter().filter(|&&k| k == FRAME_FILLED).count(), 1);
}

// a frit is its name and its text: the same name with new text is read
// again
#[test]
fn a_frit_whose_text_changes_is_read_again() {
    let frames = run(&[
        request(ASK, frit("source=builtin:unscii-8\n"), &['中']),
        request(ASK, frit("source=builtin:unifont\n"), &['中']),
    ]);
    let kinds: Vec<u32> = frames.iter().map(|f| f.0).collect();
    // Unscii-8 lacks 中 and unifont has it
    assert_eq!(kinds, [FRAME_DONE, FRAME_GLYPHS, FRAME_DONE]);
}

#[test]
fn a_frit_with_no_source_here_fails_each_request() {
    let frit = frit("source=file:/nonexistent/x.psf\n");
    let frames: Vec<(u32, Vec<u8>)> = run(&[
        request(ASK, frit.clone(), &['A']),
        request(FILL, frit, &['B']),
    ])
    .into_iter()
    .filter(|f| f.0 != FRAME_WARNING)
    .collect();
    assert_eq!(frames.len(), 2);
    assert!(frames.iter().all(|f| f.0 == FRAME_FAILED));
    assert!(String::from_utf8_lossy(&frames[0].1).contains("not on this machine"));
}

#[test]
fn a_fill_no_source_can_draw_fails_naming_why() {
    // unifont's 8x16 does not fit a 4x8 cell: an ask is answered with
    // nothing, and a fill fails with the reason
    let frit = frit("source=builtin:unifont\n");
    let small = Cell {
        width: 4,
        height: 8,
    };
    let frames = run(&[
        ServeRequest {
            cluster_width: Default::default(),
            cell: small,
            band: Band::NONE,
            ..request(ASK, frit.clone(), &['A'])
        },
        ServeRequest {
            cluster_width: Default::default(),
            cell: small,
            band: Band::NONE,
            ..request(FILL, frit, &['A'])
        },
    ]);
    assert_eq!(
        frames.iter().map(|f| f.0).collect::<Vec<_>>(),
        [FRAME_DONE, FRAME_FAILED]
    );
    let why = String::from_utf8_lossy(&frames[1].1);
    assert!(why.contains("no glyph at 4x8"), "{why}");
    assert!(why.contains("8x16"), "{why}");
}

#[test]
fn a_malformed_request_ends_the_helper_with_a_failure() {
    let mut out = Vec::new();
    let status = serve(
        std::io::Cursor::new(b"VTVSREQ1\xff\xff\xff\xff".to_vec()),
        &mut out,
        &Paths::default(),
        Duration::from_secs(5),
    );
    assert_eq!(status, 1);
    let frames = frames(&out);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].0, FRAME_FAILED);
}

/// An SVG pointer file with a white 8 × 16 `default` symbol, its
/// hotspot at its corner.
fn pointer_svg() -> Vec<u8> {
    b"<svg xmlns=\"http://www.w3.org/2000/svg\"><symbol id=\"default\" viewBox=\"0 0 8 16\" data-hotspot=\"0 0\"><rect width=\"8\" height=\"16\" fill=\"#fff\"/></symbol></svg>".to_vec()
}

fn svg_ask(shape: PointerShape, heights: Vec<u32>) -> PointerAsk {
    PointerAsk {
        file: PointerFile::Svg,
        shape,
        size: None,
        heights,
        bytes: pointer_svg(),
    }
}

// A pointer ask reads back as it was encoded, and the glyph requests'
// reader refuses it as a kind it does not know.
#[test]
fn a_pointer_ask_reads_back_and_glyph_reading_refuses_it() {
    let ask = svg_ask(PointerShape::Text, vec![16, 32]);
    let bytes = ask.encode();
    assert_eq!(
        read_request(&mut &bytes[..]),
        Ok(Some(Request::Pointer(ask.clone())))
    );
    assert_eq!(PointerAsk::read(&mut &bytes[..]), Ok(Some(ask)));
    assert!(ServeRequest::read(&mut &bytes[..]).is_err());
    let x = PointerAsk {
        file: PointerFile::Xcursor,
        shape: PointerShape::NsResize,
        size: Some(24),
        heights: Vec::new(),
        bytes: b"Xcur".to_vec(),
    };
    assert_eq!(
        read_request(&mut &x.encode()[..]),
        Ok(Some(Request::Pointer(x)))
    );
}

// Each field of a pointer ask is checked before it is trusted.
#[test]
fn a_pointer_ask_refuses_each_field_out_of_bounds() {
    let good = svg_ask(PointerShape::Default, vec![16]);
    let svg_sized = PointerAsk {
        size: Some(24),
        ..good.clone()
    };
    let no_height = svg_ask(PointerShape::Default, Vec::new());
    let many = svg_ask(PointerShape::Default, vec![16; POINTER_HEIGHTS_MAX + 1]);
    let zero = svg_ask(PointerShape::Default, vec![0]);
    let tall = svg_ask(PointerShape::Default, vec![CELL_SIDE_MAX + 1]);
    let xcursor_height = PointerAsk {
        file: PointerFile::Xcursor,
        ..good.clone()
    };
    let small = PointerAsk {
        file: PointerFile::Xcursor,
        heights: Vec::new(),
        size: Some(7),
        ..good.clone()
    };
    let large_svg = PointerAsk {
        bytes: vec![b' '; POINTER_SVG_BYTES_MAX + 1],
        ..good.clone()
    };
    for (what, ask) in [
        ("size on SVG", svg_sized),
        ("no height", no_height),
        ("too many heights", many),
        ("height 0", zero),
        ("height too tall", tall),
        ("height on Xcursor", xcursor_height),
        ("size under 8", small),
        ("SVG too large", large_svg),
    ] {
        assert!(read_request(&mut &ask.encode()[..]).is_err(), "{what}");
    }
    let mut file_kind = good.encode();
    file_kind[12] = 3;
    assert!(read_request(&mut &file_kind[..]).is_err());
    let mut shape = good.encode();
    shape[16] = 10;
    assert!(read_request(&mut &shape[..]).is_err());
    let bytes = good.encode();
    assert!(read_request(&mut &bytes[..bytes.len() - 1]).is_err());
}

// The helper answers a pointer ask with a picture at each height
// asked, then done; a shape the file lacks with a frame that says so;
// and a cursor file it cannot read with the reason.
#[test]
fn the_helper_answers_a_pointer_ask_with_its_pictures() {
    let mut input = svg_ask(PointerShape::Default, vec![20, 16, 16]).encode();
    input.extend(svg_ask(PointerShape::Wait, vec![16]).encode());
    input.extend(
        PointerAsk {
            file: PointerFile::Xcursor,
            shape: PointerShape::Text,
            size: None,
            heights: Vec::new(),
            bytes: b"not a cursor".to_vec(),
        }
        .encode(),
    );
    let mut out = Vec::new();
    let status = serve(
        std::io::Cursor::new(input),
        &mut out,
        &Paths::default(),
        Duration::from_secs(5),
    );
    assert_eq!(status, 0);
    let frames = raw_frames(&out);
    let kinds: Vec<u32> = frames.iter().map(|f| f.0).collect();
    assert_eq!(
        kinds,
        [
            FRAME_POINTER,
            FRAME_POINTER,
            FRAME_DONE,
            FRAME_NO_POINTER,
            FRAME_DONE,
            FRAME_NO_POINTER,
            FRAME_DONE
        ]
    );
    let word = |b: &[u8], i: usize| u32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap());
    let first = &frames[0].1;
    // shape 0, drawn for 16, 8 × 16, hotspot at 0, 0
    assert_eq!(
        (0..6).map(|i| word(first, i)).collect::<Vec<_>>(),
        [0, 16, 8, 16, 0, 0]
    );
    assert_eq!(first.len(), 24 + 8 * 16 * 4);
    assert_eq!(word(&frames[1].1, 1), 20);
    assert_eq!(
        (word(&frames[3].1, 0), word(&frames[3].1, 1)),
        (shape_code(PointerShape::Wait), NO_POINTER_MISSING)
    );
    let why = &frames[5].1;
    assert_eq!(
        (word(why, 0), word(why, 1)),
        (shape_code(PointerShape::Text), NO_POINTER_REFUSED)
    );
    assert!(String::from_utf8_lossy(&why[8..]).contains("Xcur"));
}

/// Input that never ends and never gives a byte.
struct Silent;

impl Read for Silent {
    #[expect(clippy::disallowed_methods, reason = "input that never comes")]
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        std::thread::sleep(Duration::from_secs(3600));
        Ok(0)
    }
}

#[test]
#[expect(clippy::disallowed_methods, reason = "the idle exit is timed")]
fn the_helper_exits_after_its_idle_interval() {
    let started = Instant::now();
    let mut out = Vec::new();
    let status = serve(
        Silent,
        &mut out,
        &Paths::default(),
        Duration::from_millis(200),
    );
    let took = started.elapsed();
    assert_eq!(status, 0);
    assert!(out.is_empty());
    assert!(
        took >= Duration::from_millis(200) && took < Duration::from_secs(5),
        "{took:?}"
    );
    assert_eq!(HELPER_IDLE_MS, 60_000);
}

/// The outline font in the crate's test fixtures, in the checkout,
/// whose folders the test's own account owns and nobody else can
/// write, on the filesystem the checkout is on.
fn fixture_font() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DejaVuSansMono-W-M-0.ttf")
}

fn me() -> u32 {
    rustix::process::geteuid().as_raw()
}

// The active fonts read back as they were encoded, and
// each field is checked before it is trusted
//
// References:
// - ADR-1205 D4: the console tells each font helper the active fonts.
#[test]
fn an_active_ask_reads_back_and_refuses_each_field_out_of_bounds() {
    let ask = ActiveAsk {
        owner: 0,
        paths: vec![PathBuf::from("/usr/share/vtium/fonts/ter-u16n.bdf.gz")],
    };
    let bytes = ask.encode();
    assert_eq!(
        read_request(&mut &bytes[..]),
        Ok(Some(Request::Active(ask.clone())))
    );
    assert!(PointerAsk::read(&mut &bytes[..]).is_err());
    assert!(ServeRequest::read(&mut &bytes[..]).is_err());
    let refused = |paths: Vec<PathBuf>| {
        let bytes = ActiveAsk { owner: 0, paths }.encode();
        read_request(&mut &bytes[..]).unwrap_err()
    };
    assert!(refused(vec![PathBuf::from("fonts/x.ttf")]).contains("absolute"));
    assert!(refused(vec![PathBuf::from("/a\0b")]).contains("absolute"));
    assert!(refused(vec![PathBuf::from("")]).contains("absolute"));
    let long = format!("/{}", "x".repeat(ACTIVE_PATH_MAX));
    assert!(refused(vec![PathBuf::from(long)]).contains("past its bound"));
    let many = vec![PathBuf::from(format!("/{}", "x".repeat(1023))); ACTIVE_BYTES_MAX / 1024 + 1];
    assert!(refused(many).contains("past their bound"));
    let mut odd = ActiveAsk {
        owner: 0,
        paths: vec![PathBuf::from("/ab")],
    }
    .encode();
    let at = odd.len() - 2;
    odd[at] = 0xff;
    assert!(read_request(&mut &odd[..]).unwrap_err().contains("UTF-8"));
}

/// What the helper answers a sources query for the frit `text`, each
/// request read in turn, starting from `gate`, with `active` sent first
/// when it is given.
fn sources_through(gate: Gate, active: Option<ActiveAsk>, text: &str) -> (u8, Vec<(u32, Vec<u8>)>) {
    let mut bytes = active.map(|a| a.encode()).unwrap_or_default();
    bytes.extend(sources_query(frit(text)).encode());
    let mut out = Vec::new();
    let status = answer_requests(
        &read_requests(std::io::Cursor::new(bytes)),
        &mut out,
        &Paths::default(),
        Duration::from_secs(5),
        None,
        gate,
    );
    (status, frames(&out))
}

// Once the console names the active fonts, the helper
// reads only the font files they list, and logs one line for each it
// refuses; vtium-config's own previews, which name none, read any
//
// References:
// - ADR-1205 D4: the console tells each font helper the active fonts.
#[test]
fn the_console_s_active_fonts_gate_every_font_file_the_helper_reads() {
    let font = fixture_font();
    let text = format!(
        "source=outline:{}\nsource=builtin:unscii-8\n",
        font.display()
    );
    let (status, open) = sources_through(Gate::Open, None, &text);
    assert_eq!(status, 0);
    assert_eq!(open.len(), 1, "{open:?}");
    assert_eq!((open[0].0, open[0].1.as_slice()), (FRAME_SOURCES, &b""[..]));
    let none = ActiveAsk {
        owner: me(),
        paths: Vec::new(),
    };
    let (status, closed) = sources_through(Gate::Open, Some(none), &text);
    assert_eq!(status, 0);
    let kinds: Vec<u32> = closed.iter().map(|f| f.0).collect();
    assert_eq!(kinds, [FRAME_WARNING, FRAME_SOURCES]);
    // the line the console logs once a boot names the file and why
    assert_eq!(
        String::from_utf8_lossy(&closed[0].1),
        format!(
            "t needs the font file {}, which vtium treats as missing, because it is not among the active fonts. vtium draws the font's characters from the next font that has them.",
            font.display()
        )
    );
    assert_eq!(
        String::from_utf8_lossy(&closed[1].1),
        format!(
            "missing\t1\toutline:{0}\t{0}: vtium treats this file as missing, because it is not among the active fonts.",
            font.display()
        )
    );
    let named = ActiveAsk {
        owner: me(),
        paths: vec![font.clone()],
    };
    let (status, listed) = sources_through(Gate::Open, Some(named), &text);
    assert_eq!(status, 0);
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(
        (listed[0].0, listed[0].1.as_slice()),
        (FRAME_SOURCES, &b""[..])
    );
    // a font from a layer is logged with its layer's name
    let symbols = format!("role=symbols\nsource=outline:{}\n", font.display());
    let spliced = layered("source=builtin:unscii-8\nlayer=sym\n", "sym", &symbols);
    let none = ActiveAsk {
        owner: me(),
        paths: Vec::new(),
    };
    let (_, layer) = sources_through(Gate::Open, Some(none), &spliced.text);
    assert_eq!(layer[0].0, FRAME_WARNING);
    assert!(
        String::from_utf8_lossy(&layer[0].1).starts_with(&format!(
            "t needs the font file {} from the layer sym, which vtium treats as missing",
            font.display()
        )),
        "{layer:?}"
    );
}

/// The font helper's answer to a cell query at 16 pixels for the frit
/// `text`, its fonts found in `paths`, with `active` sent first when it
/// is given: the reason it gives for the frit's having no character cell
/// of its own, `None` when it gives a cell.
fn no_cell_reason(paths: &Paths, active: Option<ActiveAsk>, text: &str) -> Option<String> {
    let mut bytes = active.map(|a| a.encode()).unwrap_or_default();
    bytes.extend(query_at(frit(text), 16).encode());
    let mut out = Vec::new();
    let status = answer_requests(
        &read_requests(std::io::Cursor::new(bytes)),
        &mut out,
        paths,
        Duration::from_secs(5),
        None,
        Gate::Open,
    );
    assert_eq!(status, 0);
    frames(&out)
        .into_iter()
        .find(|f| f.0 == FRAME_NO_OWN_CELL)
        .map(|f| String::from_utf8_lossy(&f.1).into_owned())
}

// When the font helper leaves a font set's text font out, and a later
// font of the set is left to draw, its answer to the console's cell
// query says why the text font is out: a file that is not among the active
// fonts, with the command that activates it, a file that does not exist,
// a file that cannot be read, and a file that fails the trust rule. Each
// reason names the text font and its file. The console logs the reason
// as the helper sends it.
//
// References:
// - ADR-1204 D5: a missing text font leaves the cell to the configuration.
// - ADR-1205 D4: a font draws only while the batch lists it and it passes the trust rule.
#[test]
fn a_left_out_text_font_s_cell_reason_says_why_the_helper_left_it_out() {
    let none = || {
        Some(ActiveAsk {
            owner: me(),
            paths: Vec::new(),
        })
    };
    let system = Paths::default();
    let tail = "The built-in font draws the text in its place, and the character cell comes from the configuration";
    let font = fixture_font();
    let inactive = no_cell_reason(
        &system,
        none(),
        &format!(
            "source=outline:{}\nsource=builtin:unscii-8\n",
            font.display()
        ),
    );
    assert_eq!(
        inactive.as_deref(),
        Some(
            format!(
                "the text font source=outline:{0} is not among the active fonts. {tail}. To activate the font file, run vtium-config activate {0} --install",
                font.display()
            )
            .as_str()
        )
    );
    let missing = no_cell_reason(
        &system,
        none(),
        "source=outline:/nonesuch/mono.ttf\nsource=builtin:unscii-8\n",
    );
    assert_eq!(
        missing.as_deref(),
        Some(
            format!(
                "the text font source=outline:/nonesuch/mono.ttf is missing: /nonesuch/mono.ttf is not on this machine. {tail}"
            )
            .as_str()
        )
    );
    let dir = scratch("reason-unread");
    let junk = dir.join("junk.ttf");
    std::fs::write(&junk, b"not a font").unwrap();
    let unread = no_cell_reason(
        &system,
        None,
        &format!(
            "source=outline:{}\nsource=builtin:unscii-8\n",
            junk.display()
        ),
    )
    .unwrap();
    assert!(
        unread.starts_with(&format!(
            "the text font source=outline:{} cannot be read: ",
            junk.display()
        )) && unread.ends_with(&format!(". {tail}")),
        "{unread}"
    );
    // a font file that others can write fails the trust rule, and the
    // reason gives the command that mends the file or its folder
    let open = dir.join("open.ttf");
    std::fs::copy(&font, &open).unwrap();
    std::fs::set_permissions(&open, std::os::unix::fs::PermissionsExt::from_mode(0o666)).unwrap();
    let listed = ActiveAsk {
        owner: me(),
        paths: vec![open.clone()],
    };
    let untrusted = no_cell_reason(
        &system,
        Some(listed),
        &format!(
            "source=outline:{}\nsource=builtin:unscii-8\n",
            open.display()
        ),
    )
    .unwrap();
    assert!(
        untrusted.starts_with(&format!(
            "the text font source=outline:{} counts as missing, because ",
            open.display()
        )) && untrusted.contains("chmod go-w")
            && untrusted.ends_with(&format!(". {tail}")),
        "{untrusted}"
    );
    // a font: source that names a family names the file it found too
    let named = dir.join("NonesuchMono-Regular.ttf");
    std::fs::copy(&font, &named).unwrap();
    let dirs = Paths {
        kbd_dirs: Vec::new(),
        font_dirs: vec![dir.to_path_buf()],
        ..Paths::default()
    };
    let family = no_cell_reason(
        &dirs,
        none(),
        "source=font:Nonesuch Mono\nsource=builtin:unscii-8\n",
    );
    assert_eq!(
        family.as_deref(),
        Some(
            format!(
                "the text font source=font:Nonesuch Mono, the font file {0}, is not among the active fonts. {tail}. To activate the font file, run vtium-config activate {0} --install",
                named.display()
            )
            .as_str()
        )
    );
}

// The console names root's account, or its own, which
// the helper then runs as; any other account is refused
//
// References:
// - ADR-1205 D4: the console tells each font helper the active fonts.
#[test]
fn active_fonts_of_another_account_end_the_helper() {
    let ask = ActiveAsk {
        owner: me().wrapping_add(1).max(1),
        paths: Vec::new(),
    };
    let (status, frames) = sources_through(Gate::Open, Some(ask), "source=builtin:unscii-8\n");
    assert_eq!(status, 1);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].0, FRAME_FAILED);
    let why = String::from_utf8_lossy(&frames[0].1);
    assert!(
        why.contains("neither root nor the font helper's own account"),
        "{why}"
    );
}

/// Opens the fixture font through an active gate on a thread that
/// `enter` sandboxes first, as the helper's main thread is sandboxed,
/// and says whether the mount table read and the font opened; `None`
/// when `enter` puts no sandbox in force on this kernel or machine.
fn opened_in_sandbox(
    enter: fn() -> Result<bool, String>,
) -> Option<(bool, Result<(), SourceError>)> {
    let font = fixture_font();
    std::thread::spawn(move || {
        if !enter().ok()? {
            return None;
        }
        let table = vtium_settings::mounts::Mounts::system().is_ok_and(|m| !m.list().is_empty());
        let gate = Gate::Active(cullet::Gate::new(
            Rule::running_as(me()),
            None,
            Batch::new(vec![Entry::Path(font.clone())]),
        ));
        let opened = crate::sources::resolve(
            &crate::frit::Frit::parse(
                format!("source=outline:{}\n", font.display()).as_bytes(),
                Path::new("t"),
                "t",
            )
            .unwrap()
            .sources[0],
            &Host {
                kbd_dirs: Vec::new(),
                font_dirs: Vec::new(),
                vconsole_conf: PathBuf::from("/nonexistent"),
                kernel_cmdline: PathBuf::from("/nonexistent"),
                consoles: &Handed(None),
                gate: &gate,
            },
        )
        .map(drop);
        Some((table, opened))
    })
    .join()
    .unwrap()
}

// Inside the helper's real sandbox, the
// trust rule reads the mount table and sysfs, so an active font on a
// filesystem that stays mounted opens; were the sandbox to hide them,
// the rule would refuse every font. Under the landlock domain, and
// under the fallback filter that stands in for it
//
// References:
// - ADR-1205 D4: a font draws only while the batch lists it and it passes the trust rule.
// - ADR-1200 D15: the font helper runs in a sandbox.
#[test]
fn the_trust_rule_reads_the_mount_table_and_sysfs_inside_the_sandbox() {
    fn fallback() -> Result<bool, String> {
        if refuse_sockets()?.is_some() {
            return Ok(false);
        }
        fall_back()?;
        Ok(true)
    }
    let mut judged = 0;
    for (name, enter) in [
        ("landlock", sandbox_with_domain as fn() -> _),
        ("fallback", fallback),
    ] {
        match opened_in_sandbox(enter) {
            None => eprintln!("skipped: no {name} sandbox on this kernel or machine"),
            Some((table, opened)) => {
                assert!(table, "{name}: the mount table did not read");
                assert_eq!(opened, Ok(()), "{name}");
                judged += 1;
            }
        }
    }
    if std::env::var_os("VTIUM_QEMU_USER").is_none() {
        assert!(judged > 0, "neither sandbox was in force");
    }
}

/// The helper's sandbox, when its landlock domain is in force.
fn sandbox_with_domain() -> Result<bool, String> {
    Ok(sandbox(true)?.parts & SANDBOX_DOMAIN != 0)
}

/// What a thread of the test can do once `sandbox(on)` has run in it:
/// the report, the seccomp filters it runs under beyond the test's
/// own, whether it can make a UDP socket, and whether it can create a
/// file in `dir`.
fn sandboxed_thread(on: bool, dir: PathBuf) -> (Sandboxed, Option<u32>, bool, bool) {
    fn filters() -> Option<u32> {
        std::fs::read_to_string("/proc/thread-self/status")
            .ok()?
            .lines()
            .find_map(|l| l.strip_prefix("Seccomp_filters:"))
            .and_then(|n| n.trim().parse().ok())
    }
    std::thread::spawn(move || {
        let before = filters();
        let report = sandbox(on).unwrap();
        let added = filters().zip(before).map(|(after, before)| after - before);
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").is_ok();
        let created = std::fs::write(dir.join(format!("made-{on}")), b"x").is_ok();
        (report, added, socket, created)
    })
    .join()
    .unwrap()
}

// helper-sandbox=off reaches the helper as `--sandbox off`, and the
// helper then installs neither its socket filter nor its fallback
// filter and enters no landlock domain: a thread sandboxed so still
// makes a socket and creates a file, and its report gives the setting
// as the reason for both. With the sandbox on, the same thread runs
// under the socket filter and the domain or the fallback filter.
//
// References:
// - ADR-1200 D15, "A setting turns the sandbox off": what the helper
//   leaves out under off, and that its report says why.
#[test]
fn the_sandbox_off_installs_no_filter_and_enters_no_domain() {
    let dir = scratch("sandbox-off");
    let (report, added, socket, created) = sandboxed_thread(false, dir.to_path_buf());
    assert_eq!(
        report,
        Sandboxed {
            parts: 0,
            seccomp: Some(Missing::Off),
            domain: Some(NoDomain::Off),
        }
    );
    assert_eq!(report.word(), 1 << 8 | 1 << 16);
    assert!(matches!(added, None | Some(0)), "{added:?}");
    assert!(socket, "the thread could not make a socket");
    assert!(created, "the thread could not create a file");
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped the sandbox on: {why}");
        return;
    }
    if !crate::seccomp::BUILT {
        eprintln!("skipped the sandbox on: vtium has no seccomp filter for this architecture");
        return;
    }
    let (report, added, socket, created) = sandboxed_thread(true, dir.to_path_buf());
    assert_eq!(report.seccomp, None, "{report:?}");
    assert_ne!(report.parts & SANDBOX_SOCKETS, 0, "{report:?}");
    assert_ne!(
        report.parts & (SANDBOX_DOMAIN | SANDBOX_FALLBACK),
        0,
        "{report:?}"
    );
    assert_eq!(report.domain.is_none(), report.parts & SANDBOX_DOMAIN != 0);
    // the socket filter, and the fallback filter where it stands in
    let want = if report.parts & SANDBOX_FALLBACK != 0 {
        2
    } else {
        1
    };
    assert!(added.is_none() || added == Some(want), "{added:?}");
    assert!(!socket, "a socket made under the socket filter");
    assert!(!created, "a file created inside the sandbox");
}

// Why the landlock domain is not in force, from the landlock query's
// answer and the kernel's release: ENOSYS before 5.13 is a kernel
// older than landlock, and from 5.13 one built without it; EOPNOTSUPP
// is landlock left off at boot.
//
// References:
// - ADR-1200 D15, "Nothing on screen, and the log says why": the
//   landlock reasons.
#[test]
fn the_missing_domain_reads_as_its_kernels_reason() {
    let not_built = LandlockStatus::NotImplemented;
    assert_eq!(no_domain(not_built, Some((5, 12))), NoDomain::TooOld);
    assert_eq!(no_domain(not_built, Some((4, 19))), NoDomain::TooOld);
    assert_eq!(no_domain(not_built, Some((5, 13))), NoDomain::NotBuilt);
    assert_eq!(no_domain(not_built, Some((6, 7))), NoDomain::NotBuilt);
    let off = LandlockStatus::NotEnabled;
    assert_eq!(no_domain(off, Some((6, 7))), NoDomain::NotEnabled);
    assert_eq!(no_domain(off, None), NoDomain::NotEnabled);
    // the report carries both reasons above the parts
    let report = Sandboxed {
        parts: SANDBOX_SOCKETS | SANDBOX_FALLBACK,
        seccomp: None,
        domain: Some(NoDomain::NotEnabled),
    };
    assert_eq!(report.word(), 0b110 | 4 << 16);
    let report = Sandboxed {
        parts: SANDBOX_DOMAIN,
        seccomp: Some(Missing::NoFilters),
        domain: None,
    };
    assert_eq!(report.word(), 0b001 | 4 << 8);
}

// the helper's landlock domain refuses every file write and every TCP
// bind and connect, and an abstract Unix socket made outside it, and
// leaves reads open (ADR-1200 D15, the maintainer, 2026-11-04); a
// thread of its own enters it, as the helper's main thread does, so the
// rest of the test process stays outside it
#[test]
fn the_helpers_landlock_domain_refuses_writes_and_the_network() {
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
    let dir = scratch("landlock");
    let existing = dir.join("existing");
    std::fs::write(&existing, b"font").unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let tcp = listener.local_addr().unwrap();
    let name = format!("vtium-landlock-{}", std::process::id());
    let abstract_addr = SocketAddr::from_abstract_name(name.as_bytes()).unwrap();
    let _unix = UnixListener::bind_addr(&abstract_addr).unwrap();
    let (existing_in, dir_in) = (existing.clone(), dir.to_path_buf());
    let locked = std::thread::spawn(move || {
        let enforced = lock_out().unwrap().is_none();
        if !enforced {
            return None;
        }
        let read = std::fs::read(&existing_in).ok();
        let created = std::fs::write(dir_in.join("new"), b"x").is_ok();
        let opened = std::fs::OpenOptions::new()
            .write(true)
            .open(&existing_in)
            .is_ok();
        let removed = std::fs::remove_file(&existing_in).is_ok();
        let bound = std::net::TcpListener::bind("127.0.0.1:0").is_ok();
        let connected = std::net::TcpStream::connect(tcp).is_ok();
        let reached = UnixStream::connect_addr(&abstract_addr).is_ok();
        Some((read, created, opened, removed, bound, connected, reached))
    })
    .join()
    .unwrap();
    let Some((read, created, opened, removed, bound, connected, reached)) = locked else {
        eprintln!("skipped: this kernel offers no landlock, so the helper runs without its domain");
        return;
    };
    assert_eq!(read.as_deref(), Some(&b"font"[..]), "reads stay open");
    assert!(!created, "a file created");
    assert!(!opened, "a file opened for writing");
    assert!(!removed, "a file removed");
    assert!(!bound, "a TCP socket bound");
    assert!(!connected, "a TCP socket connected");
    assert!(
        !reached,
        "an abstract Unix socket made outside the domain reached"
    );
    // the test's own threads are outside the domain
    assert!(std::fs::write(dir.join("outside"), b"x").is_ok());
}

// the helper's socket filter refuses every new socket on any kernel,
// UDP and Unix ones included, which landlock leaves open, and lets
// every other call through (ADR-1200 D15, the maintainer, 2026-11-04);
// a thread of its own installs it, as the helper's main thread does
#[test]
fn the_helpers_socket_filter_refuses_every_new_socket() {
    use std::os::unix::net::{UnixDatagram, UnixStream};
    let dir = scratch("sockets");
    // qemu-user implements no seccomp, since a filter would bind qemu
    // itself, so there the test cannot judge the filter, and it skips
    // and says so
    let enosys = std::io::Error::from_raw_os_error(libc::ENOSYS).to_string();
    let qemu = std::env::var_os("VTIUM_QEMU_USER");
    let made = std::thread::spawn(move || {
        match refuse_sockets() {
            Ok(None) => {}
            Ok(Some(_)) => return None,
            Err(e) => {
                assert!(qemu.is_some() && e.contains(&enosys), "{e}");
                eprintln!(
                    "skipped: {} implements no seccomp: {e}",
                    qemu.unwrap_or_default().display()
                );
                return None;
            }
        }
        let read = std::fs::read("/proc/self/status").is_ok();
        let written = std::fs::write(dir.join("allowed"), b"x").is_ok();
        Some((
            read && written,
            std::net::UdpSocket::bind("127.0.0.1:0").is_ok(),
            std::net::TcpListener::bind("127.0.0.1:0").is_ok(),
            UnixDatagram::unbound().is_ok(),
            UnixStream::pair().is_ok(),
        ))
    })
    .join()
    .unwrap();
    let Some((others, udp, tcp, unix, pair)) = made else {
        eprintln!("skipped: no socket filter on this architecture or under qemu-user");
        return;
    };
    assert!(others, "a call other than socket refused");
    assert!(!udp, "a UDP socket made");
    assert!(!tcp, "a TCP socket made");
    assert!(!unix, "a Unix socket made");
    assert!(!pair, "a socket pair made");
    // the test's own threads are outside the filter
    assert!(std::net::UdpSocket::bind("127.0.0.1:0").is_ok());
}

// the socket filter refuses io_uring, whose IORING_OP_SOCKET makes a
// socket without calling socket (Linux 5.19). The test cannot make the
// calls itself, since the crate forbids unsafe code, and std cannot
// start a process under the filter, so it hands the compiled filter to
// a python3, which installs it through ctypes, as the helper's
// seccompiler does, and then makes each io_uring call with arguments
// that fail on their own (EFAULT or EBADF) and prints its errno. The
// same python3 without the filter shows that the kernel offers
// io_uring, else the test skips and says so
#[cfg(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "riscv64"
))]
#[test]
fn the_helpers_socket_filter_refuses_io_uring() {
    const SCRIPT: &str = "import ctypes, struct, sys\n\
            libc = ctypes.CDLL(None, use_errno=True)\n\
            code = bytes.fromhex(sys.argv[4])\n\
            if code:\n\
            \x20   class Prog(ctypes.Structure):\n\
            \x20       _fields_ = [('len', ctypes.c_ushort), ('filter', ctypes.c_char_p)]\n\
            \x20   prog = Prog(len(code) // 8, code)\n\
            \x20   assert libc.prctl(38, 1, 0, 0, 0) == 0\n\
            \x20   assert libc.prctl(22, 2, ctypes.byref(prog), 0, 0) == 0\n\
            out = []\n\
            for call, args in ((sys.argv[1], (1, None)), (sys.argv[2], (-1, 0, 0, 0, None, 0)), (sys.argv[3], (-1, 0, None, 0))):\n\
            \x20   ctypes.set_errno(0)\n\
            \x20   r = libc.syscall(ctypes.c_long(int(call)), *[ctypes.c_long(a) if isinstance(a, int) else a for a in args])\n\
            \x20   out.append(str(ctypes.get_errno() if r < 0 else 0))\n\
            print(' '.join(out))\n";
    let errnos = |filter: &[seccompiler::sock_filter]| -> Option<Vec<i32>> {
        let code: String = filter
            .iter()
            .flat_map(|f| {
                let mut b = f.code.to_ne_bytes().to_vec();
                b.extend([f.jt, f.jf]);
                b.extend(f.k.to_ne_bytes());
                b
            })
            .map(|b| format!("{b:02x}"))
            .collect();
        let calls = [
            libc::SYS_io_uring_setup,
            libc::SYS_io_uring_enter,
            libc::SYS_io_uring_register,
        ];
        let out = std::process::Command::new("python3")
            .args(["-I", "-c", SCRIPT])
            .args(calls.map(|n| n.to_string()))
            .arg(code)
            .env_clear()
            .output()
            .ok()?;
        String::from_utf8(out.stdout)
            .ok()?
            .split_whitespace()
            .map(|e| e.parse().ok())
            .collect::<Option<Vec<i32>>>()
            .filter(|e| e.len() == 3)
    };
    let Some(outside) = errnos(&[]) else {
        eprintln!("skipped: no python3 to make the io_uring calls with");
        return;
    };
    // Under qemu-user, as make arm-test runs the tests, python3 is the
    // host's, of another architecture than the filter, which then
    // matches none of its calls.
    let machine = std::process::Command::new("python3")
        .args(["-I", "-c", "import platform; print(platform.machine())"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|m| m.trim().to_owned())
        .unwrap_or_default();
    if machine != std::env::consts::ARCH {
        eprintln!(
            "skipped: python3 runs on {machine}, and the filter is for {}",
            std::env::consts::ARCH
        );
        return;
    }
    if outside.contains(&libc::EPERM) || outside.contains(&libc::ENOSYS) {
        eprintln!("skipped: this kernel refuses io_uring already: {outside:?}");
        return;
    }
    let inside = errnos(&crate::seccomp::socket_filter().unwrap());
    assert_eq!(
        inside,
        Some(vec![libc::EPERM; 3]),
        "outside the filter: {outside:?}"
    );
}

#[test]
fn the_helper_refuses_to_run_as_root() {
    let status = |uid: &str, gid: &str, groups: &str| {
        format!("Name:\tvtium-config\nUid:\t{uid}\nGid:\t{gid}\nGroups:\t{groups}\nNgid:\t0\n")
    };
    let nobody = "65534\t65534\t65534\t65534";
    assert_eq!(refuse_root(&status(nobody, nobody, "")), Ok(()));
    assert_eq!(refuse_root(&status(nobody, nobody, "1000 998 ")), Ok(()));
    // root in any of the four uids, its saved uid included, which would
    // let the helper take root's id back
    for uid in [
        "0\t0\t0\t0",
        "65534\t0\t65534\t65534",
        "65534\t65534\t0\t65534",
        "65534\t65534\t65534\t0",
    ] {
        let why = refuse_root(&status(uid, nobody, "")).unwrap_err();
        assert!(why.contains("refuses to run as root"), "{why}");
    }
    // group 0 as any of the four gids, or among the supplementary groups
    for (gid, groups) in [
        ("0\t0\t0\t0", ""),
        ("65534\t65534\t0\t65534", ""),
        (nobody, "0"),
        (nobody, "1000 0 998"),
    ] {
        let why = refuse_root(&status(nobody, gid, groups)).unwrap_err();
        assert!(why.contains("group 0"), "{why}");
    }
    // a status it cannot read is refused
    for broken in [
        String::new(),
        status("65534\t65534", nobody, ""),
        status(nobody, "x\t65534\t65534\t65534", ""),
        format!("Uid:\t{nobody}\nGid:\t{nobody}\n"),
    ] {
        let why = refuse_root(&broken).unwrap_err();
        assert!(why.contains("cannot read its ids"), "{why}");
    }
    // the helper's own status reads, as the test's account
    let own = std::fs::read_to_string("/proc/self/status").unwrap();
    assert!(status_ids(&own, "Uid:").is_some_and(|ids| ids.len() == 4));
    assert!(status_ids(&own, "Groups:").is_some());
}

/// DejaVu Sans Mono's ASCII, whose cell at 16 pixels is 9x16.
const DEJAVU: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/DejaVuSansMono-ascii.ttf"
);
const DEJAVU_CELL: Cell = Cell {
    width: 9,
    height: 16,
};

/// An ask of `frit` at DejaVu Sans Mono's cell for `chars`, drawn at
/// `doubled`.
fn doubled_ask(frit: FritText, chars: &[char], doubled: Option<Doubled>) -> ServeRequest {
    ServeRequest {
        cell: DEJAVU_CELL,
        doubled,
        ..request(ASK, frit, chars)
    }
}

/// The set of each glyph frame `frames` holds, and the cell of each of
/// its strikes.
fn answered(frames: &[(u32, Vec<u8>)]) -> Vec<(Arc<Set>, Vec<Cell>)> {
    frames
        .iter()
        .filter(|f| f.0 == FRAME_GLYPHS)
        .map(|f| {
            let set = Arc::new(Set::from_bytes(f.1.clone()).unwrap());
            let cells = set.cells();
            (set, cells)
        })
        .collect()
}

/// Where a request about `frit` holds its line size: after the magic,
/// the kind, the frit's name and text with their lengths, the cell, the
/// rows line spacing adds, the style, the script and the cluster width.
fn line_size_at(frit: &FritText) -> usize {
    8 + 4 + 4 + frit.name.len() + 4 + frit.text.len() + 8 + 8 + 4 + 4 + 4
}

/// The one glyph frame `frames` holds, as [`answered`] reads it; fails,
/// saying `what`, unless there is exactly one.
fn one_answer(frames: &[(u32, Vec<u8>)], what: &str) -> (Arc<Set>, Vec<Cell>) {
    let mut all = answered(frames);
    assert_eq!(all.len(), 1, "{what}");
    all.remove(0)
}

/// The coverage in each of `g`'s rows, added up.
fn row_ink(g: &Glyph) -> Vec<u64> {
    g.pixels()
        .chunks(g.width() as usize)
        .map(|row| row.iter().map(|&p| u64::from(p)).sum())
        .collect()
}

// an ask's line size reads back; a double size on any other request,
// a superscript or subscript at a double size, a line size past double
// height's 2, and a cell whose double passes CELL_SIDE_MAX are refused
#[test]
fn a_double_ask_reads_back_and_its_bounds_are_checked() {
    let font = frit(&format!("source=outline:{DEJAVU}\n"));
    for doubled in [Doubled::Width, Doubled::Height] {
        let ask = ServeRequest {
            style: BOLD_ITALIC,
            ..doubled_ask(font.clone(), &['H'], Some(doubled))
        };
        let bytes = ask.encode();
        let at = line_size_at(&font);
        assert_eq!(
            &bytes[at..at + 4],
            &doubled_code(Some(doubled)).to_le_bytes()
        );
        assert_eq!(ServeRequest::read(&mut &bytes[..]), Ok(Some(ask)));
    }
    let fill = ServeRequest {
        doubled: Some(Doubled::Width),
        ..request(FILL, font.clone(), &['H'])
    };
    let e = ServeRequest::read(&mut &fill.encode()[..]).unwrap_err();
    assert!(
        e.contains("double size on a request that is not an ask"),
        "{e}"
    );
    let sup = ServeRequest {
        script: Script::Super,
        ..doubled_ask(font.clone(), &['H'], Some(Doubled::Height))
    };
    let e = ServeRequest::read(&mut &sup.encode()[..]).unwrap_err();
    assert!(
        e.contains("superscript or subscript at a double size"),
        "{e}"
    );
    let mut past = doubled_ask(font.clone(), &['H'], None).encode();
    let at = line_size_at(&font);
    past[at..at + 4].copy_from_slice(&3u32.to_le_bytes());
    assert_eq!(
        ServeRequest::read(&mut &past[..]),
        Err("an unknown line size".to_owned())
    );
    let tall = Cell {
        width: 9,
        height: CELL_SIDE_MAX / 2 + 1,
    };
    let height = ServeRequest {
        cell: tall,
        ..doubled_ask(font.clone(), &['H'], Some(Doubled::Height))
    };
    let e = ServeRequest::read(&mut &height.encode()[..]).unwrap_err();
    assert!(e.contains("double size whose cell"), "{e}");
    // the same cell doubles across at double width, which keeps its height
    let width = ServeRequest {
        cell: tall,
        doubled: Some(Doubled::Width),
        ..height
    };
    assert!(ServeRequest::read(&mut &width.encode()[..]).is_ok());
}

// issue #61, the maintainer's direction of 2026-11-14: an outline font
// draws a line of double size at true double size. A double-width
// ask's glyph is drawn in a cell twice as wide, from the outline
// stretched to twice its width at the normal size: each row holds
// twice the normal row's coverage, the rows unchanged, and the glyph is
// not the normal one with each pixel doubled. A double-height ask's
// glyph is drawn at twice the size, in a cell twice as wide and twice
// as tall, its ink over twice the rows and four times the coverage,
// and it too is not the normal glyph with its pixels doubled
#[test]
fn a_double_ask_renders_its_outline_glyphs_at_double_size() {
    let font = frit(&format!("source=outline:{DEJAVU}\n"));
    let chars = ['H', 'g', 'o'];
    let normal = run(&[doubled_ask(font.clone(), &chars, None)]);
    let (normal, _) = &one_answer(&normal, "one glyph frame at the normal size");
    let normal = SetFace::new(Arc::clone(normal), 16).unwrap();
    let wide = run(&[doubled_ask(font.clone(), &chars, Some(Doubled::Width))]);
    let (wide, cells) = &one_answer(&wide, "one glyph frame at double width");
    assert_eq!(
        cells,
        &[Cell {
            width: 18,
            height: 16
        }]
    );
    let wide = SetFace::new(Arc::clone(wide), 16).unwrap();
    let tall = run(&[doubled_ask(font, &chars, Some(Doubled::Height))]);
    let (tall, cells) = &one_answer(&tall, "one glyph frame at double height");
    assert_eq!(
        cells,
        &[Cell {
            width: 18,
            height: 32
        }]
    );
    let tall = SetFace::new(Arc::clone(tall), 32).unwrap();
    let ink_rows = |g: &Glyph| {
        let rows = row_ink(g);
        let first = rows.iter().position(|&n| n > 0).unwrap();
        let last = rows.iter().rposition(|&n| n > 0).unwrap();
        (first, last)
    };
    for c in chars {
        let n = normal.draw(&[c], Style::default()).unwrap();
        let w = wide.draw(&[c], Style::default()).unwrap();
        let t = tall.draw(&[c], Style::default()).unwrap();
        assert_eq!((w.width(), w.height()), (18, 16), "{c}");
        assert_eq!((t.width(), t.height()), (18, 32), "{c}");
        for (y, (&a, &b)) in row_ink(&n).iter().zip(&row_ink(&w)).enumerate() {
            // a row's ink moves by a fraction of a pixel as the pen
            // is placed in the wider cell, which changes its sum by a
            // few per cent where the glyph meets its cell's edges
            assert!(
                b.abs_diff(2 * a) <= 2 * a / 20 + 18,
                "{c} row {y}: {b} against twice {a}"
            );
        }
        let doubled_pairs = w.pixels().chunks(2).all(|p| p[0] == p[1]);
        assert!(!doubled_pairs, "{c}: each pixel doubled across");
        let (top, bottom) = ink_rows(&n);
        let (t_top, t_bottom) = ink_rows(&t);
        assert!(t_top.abs_diff(2 * top) <= 1, "{c}: {t_top} against {top}");
        assert!(
            t_bottom.abs_diff(2 * bottom + 1) <= 1,
            "{c}: {t_bottom} against {bottom}"
        );
        let (sum_n, sum_t): (u64, u64) = (row_ink(&n).iter().sum(), row_ink(&t).iter().sum());
        assert!(
            sum_t.abs_diff(4 * sum_n) <= 4 * sum_n / 10,
            "{c}: {sum_t} against four times {sum_n}"
        );
        let blocks = (0..16).all(|y| {
            (0..9).all(|x| {
                let at = |dx: usize, dy: usize| t.pixels()[(2 * y + dy) * 18 + 2 * x + dx];
                at(0, 0) == at(1, 0) && at(0, 0) == at(0, 1) && at(0, 0) == at(1, 1)
            })
        });
        assert!(!blocks, "{c}: each pixel doubled across and down");
    }
}

// a double ask leaves to the console what a bitmap source draws, even
// where a later outline source covers it too, so the console doubles
// its pixels as the DEC terminals did; the outline source draws the
// rest at double size
#[test]
fn a_double_ask_leaves_a_bitmap_sources_glyphs_to_the_console() {
    let dir = scratch("doubled-bitmap");
    let path = dir.join("9x16.psf");
    let psf = crate::decode::tests::psf2(9, 16, 2, Some(&[(1, 'A')]));
    std::fs::write(&path, psf).unwrap();
    let font = frit(&format!(
        "source=file:{}\nsource=outline:{DEJAVU}\n",
        path.display()
    ));
    let normal = run(&[doubled_ask(font.clone(), &['A', 'B'], None)]);
    let (normal, _) = &one_answer(&normal, "one glyph frame at the normal size");
    let normal = SetFace::new(Arc::clone(normal), 16).unwrap();
    assert!(normal.has_glyph('A') && normal.has_glyph('B'));
    let frames = run(&[doubled_ask(font, &['A', 'B'], Some(Doubled::Width))]);
    let (wide, _) = &one_answer(&frames, "one glyph frame at double width");
    let wide = SetFace::new(Arc::clone(wide), 16).unwrap();
    assert!(!wide.has_glyph('A'));
    assert!(wide.has_glyph('B'));
    assert_eq!(frames.last().map(|f| f.0), Some(FRAME_DONE));
    // a frit of bitmap fonts alone answers a double ask with nothing
    let bitmap = frit(&format!("source=file:{}\n", path.display()));
    let frames = run(&[doubled_ask(bitmap, &['A'], Some(Doubled::Height))]);
    assert_eq!(frames.iter().map(|f| f.0).collect::<Vec<_>>(), [FRAME_DONE]);
}

/// The face of the one glyph set `frames` answer with, at `height`.
fn answered_face(frames: &[(u32, Vec<u8>)], what: &str) -> (SetFace, Vec<Cell>) {
    let (set, cells) = one_answer(frames, what);
    let height = cells[0].height;
    (SetFace::new(set, height).unwrap(), cells)
}

// covers: text.line-size
// issue #61: a color glyph on a line of double size comes from the
// helper at that size, never with its pixels doubled. Test Mono's
// U+1F600 is a wide COLR glyph, a red block under a half-clear blue
// one. On a double-height pair the helper draws it in the doubled cell,
// 32 by 32 for two cells of 8 by 16. On a line of double width it is
// that glyph made half as tall, 32 by 16, each pair of rows averaged:
// the normal glyph at twice its width
#[test]
fn a_double_ask_draws_a_color_glyph_at_its_size() {
    let dir = scratch("doubled-color");
    let path = dir.join("TestMono.ttf");
    std::fs::write(&path, super::ttf::test_mono().build()).unwrap();
    let font = frit(&format!("source=outline:{}\n", path.display()));
    let emoji = ['\u{1f600}'];
    let ask = |doubled| ServeRequest {
        doubled,
        ..request(ASK, font.clone(), &emoji)
    };
    let (normal, _) = answered_face(&run(&[ask(None)]), "at the normal size");
    let (tall, cells) = answered_face(&run(&[ask(Some(Doubled::Height))]), "double height");
    assert_eq!(
        cells,
        [Cell {
            width: 16,
            height: 32
        }]
    );
    let (wide, cells) = answered_face(&run(&[ask(Some(Doubled::Width))]), "double width");
    assert_eq!(
        cells,
        [Cell {
            width: 16,
            height: 16
        }]
    );
    let n = normal.draw(&emoji, Style::default()).unwrap();
    let t = tall.draw(&emoji, Style::default()).unwrap();
    let w = wide.draw(&emoji, Style::default()).unwrap();
    assert_eq!((n.format(), n.width(), n.height()), (Format::Rgba, 16, 16));
    assert_eq!(
        (t.format(), t.cells(), t.width(), t.height()),
        (Format::Rgba, Width::Wide, 32, 32)
    );
    assert_eq!(
        (w.format(), w.cells(), w.width(), w.height()),
        (Format::Rgba, Width::Wide, 32, 16)
    );
    assert_eq!(Some(w.pixels().to_vec()), halved(&t));
    // the double-height glyph holds the normal one's red over at least
    // three times as many pixels, as a glyph drawn at twice the size
    // does; each fits its cells by its own rounding, so the ratio is
    // not exactly four
    let red = |g: &Glyph| {
        g.pixels()
            .chunks(4)
            .filter(|p| p[0] > 200 && p[2] == 0)
            .count()
    };
    assert!(red(&t) >= 3 * red(&n), "{} against {}", red(&t), red(&n));
}

// issue #61: a line of double width draws its color glyphs from a
// double-height cell. Where that cell passes CELL_SIDE_MAX, as 640×1100
// does at 1280×2200 while its double width is 1280×1100, the line
// leaves the color glyph to the console, which doubles the pixels of
// the glyph at the normal size, and still draws its other glyphs. The
// ask does not fail, so the helper is not given up
#[test]
fn a_double_width_ask_whose_double_height_is_too_large_leaves_its_color_glyphs() {
    let dir = scratch("doubled-wide-tall");
    let path = dir.join("TestMono.ttf");
    std::fs::write(&path, super::ttf::test_mono().build()).unwrap();
    let font = frit(&format!("source=outline:{}\n", path.display()));
    let cell = Cell {
        width: 640,
        height: 1100,
    };
    assert!(Doubled::Height.cell(cell).is_none());
    let ask = ServeRequest {
        cell,
        doubled: Some(Doubled::Width),
        ..request(ASK, font, &['A', '\u{1f600}'])
    };
    let frames = run(&[ask]);
    assert!(frames.iter().all(|f| f.0 != FRAME_FAILED), "the ask failed");
    let (wide, cells) = answered_face(&frames, "double width");
    assert_eq!(
        cells,
        [Cell {
            width: 1280,
            height: 1100
        }]
    );
    assert!(wide.draw(&['A'], Style::default()).is_some(), "A is drawn");
    assert!(
        wide.draw(&['\u{1f600}'], Style::default()).is_none(),
        "the color glyph is left to the console"
    );
}

// covers: text.line-size
// issue #61: a character that presents as text by default, whose only
// glyph is a color glyph one cell wide, waits for a later source's text
// glyph, and takes the color glyph once no source has one. On a pair of
// double-height lines that color glyph is placed in the doubled cell,
// with the doubled rows that line spacing adds, like every other glyph
// of the answer. Test Mono here draws U+263A, which presents as text by
// default, as one opaque red layer of its `A` bar. A cell of 8 by 20
// with 2 rows of spacing above and below the text doubles to 16 by 40
// with 4 rows above and below, and those rows stay clear
#[test]
fn a_double_ask_places_a_color_glyph_held_for_text_in_the_doubled_cell() {
    let dir = scratch("doubled-held-color");
    let path = dir.join("TestMono.ttf");
    let mut mono = super::ttf::test_mono();
    mono.glyphs
        .push(super::ttf::Glyph::rect(600, 100, 0, 500, 700));
    let held = mono.glyphs.len() as u16 - 1;
    mono.cmap.push((0x263a, held));
    mono.cmap.sort_unstable();
    mono.colr.push((held, vec![(2, 0)]));
    std::fs::write(&path, mono.build()).unwrap();
    let font = frit(&format!("source=outline:{}\n", path.display()));
    let smiley = ['\u{263a}'];
    assert!(!crate::shaping::emoji_by_default(0x263a));
    let ask = ServeRequest {
        cell: Cell {
            width: 8,
            height: 20,
        },
        band: Band { above: 2, below: 2 },
        doubled: Some(Doubled::Height),
        ..request(ASK, font, &smiley)
    };
    let (tall, cells) = answered_face(&run(&[ask]), "double height");
    assert_eq!(
        cells,
        [Cell {
            width: 16,
            height: 40
        }]
    );
    let t = tall.draw(&smiley, Style::default()).unwrap();
    assert_eq!(
        (t.format(), t.cells(), t.width(), t.height()),
        (Format::Rgba, Width::Narrow, 16, 40)
    );
    let red_rows: Vec<usize> = t
        .pixels()
        .chunks(16 * 4)
        .enumerate()
        .filter(|(_, row)| row.chunks(4).any(|p| p[0] > 200 && p[3] > 200))
        .map(|(y, _)| y)
        .collect();
    assert!(!red_rows.is_empty(), "the glyph holds its red layer");
    assert!(
        red_rows.iter().all(|y| (4..36).contains(y)),
        "red outside the text band: {red_rows:?}"
    );
}

// covers: text.line-size
// issue #61: a cluster on a line of double size comes from the helper
// at that size. E with a grave accent, decomposed, is shaped whole in
// the doubled cell of a double-height pair, 18 by 32, and on a line of
// double width it is that glyph made half as tall, 18 by 16. The tall
// glyph's ink spans about twice the rows of the normal glyph's
#[test]
fn a_double_ask_shapes_a_cluster_at_its_size() {
    let marks = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/DejaVuSansMono-marks.ttf"
    );
    let font = frit(&format!("source=outline:{marks}\n"));
    let key = ['E', '\u{300}'];
    let ask = |doubled| ServeRequest {
        doubled,
        clusters: vec![key.iter().map(|&c| u32::from(c)).collect()],
        ..doubled_ask(font.clone(), &[], doubled)
    };
    let (normal, _) = answered_face(&run(&[ask(None)]), "at the normal size");
    let (tall, cells) = answered_face(&run(&[ask(Some(Doubled::Height))]), "double height");
    assert_eq!(
        cells,
        [Cell {
            width: 18,
            height: 32
        }]
    );
    let (wide, cells) = answered_face(&run(&[ask(Some(Doubled::Width))]), "double width");
    assert_eq!(
        cells,
        [Cell {
            width: 18,
            height: 16
        }]
    );
    let n = normal.cluster(&key, Style::default()).unwrap();
    let t = tall.cluster(&key, Style::default()).unwrap();
    let w = wide.cluster(&key, Style::default()).unwrap();
    assert_eq!((t.width(), t.height()), (18, 32));
    assert_eq!((w.width(), w.height()), (18, 16));
    assert_eq!(Some(w.pixels().to_vec()), halved(&t));
    let span = |g: &Glyph| {
        let rows = row_ink(g);
        let first = rows.iter().position(|&n| n > 0).unwrap();
        rows.iter().rposition(|&n| n > 0).unwrap() - first + 1
    };
    assert!(
        span(&t).abs_diff(2 * span(&n)) <= 2,
        "{} against {}",
        span(&t),
        span(&n)
    );
}

// halving a glyph averages each pair of rows, rounded to the nearest
#[test]
fn halving_a_glyph_averages_each_pair_of_rows() {
    let g = Glyph::from_pixels(2, 4, Format::Mask, vec![0, 255, 0, 254, 10, 20, 11, 20]).unwrap();
    assert_eq!(halved(&g), Some(vec![0, 255, 11, 20]));
    let odd = Glyph::from_pixels(1, 3, Format::Mask, vec![1, 2, 3]).unwrap();
    assert_eq!(halved(&odd), None);
}

// A glyph halved for a line of double width keeps its ink box, halved
// by the same pairs of rows from the glyph's top: a mark 1 row above
// the tall glyph pairs with an empty row, so it keeps a row of its own
// above the halved glyph at half its coverage
#[test]
fn halving_a_glyph_keeps_its_ink_box() {
    let own = vec![255, 0, 0, 0, 0, 0, 0, 255];
    let mut ink = vec![200, 200];
    ink.extend(&own);
    let g = Glyph::from_pixels(2, 4, Format::Mask, own)
        .unwrap()
        .with_ink((0, -1), (2, 5), ink)
        .unwrap();
    assert_eq!(
        halved_ink(&g),
        Some(((0, -1), (2, 3), vec![100, 100, 128, 0, 0, 128]))
    );
}

#[test]
fn ranges_join_runs() {
    assert_eq!(ranges(&[1, 2, 3, 5, 7, 8]), vec![(1, 3), (5, 5), (7, 8)]);
    assert_eq!(ranges(&[]), Vec::<Cover>::new());
}
