//! OSC 52: a program sets the selection, which the host takes with
//! `Terminal::take_clipboard_write`. The data is base64, decoded strictly
//! and held to `CLIPBOARD_MAX`; the query gets no reply (ADR-1000,
//! osc.52.read, refused); and a string the parser truncated or cancelled
//! sets nothing.

mod common;

use common::term;
use vtium_parse::OSC_MAX;
use vtium_term::{CLIPBOARD_MAX, Terminal};

/// RFC 4648's base64, padded, written here so the tests do not lean on the
/// decoder they test.
fn base64(data: &[u8]) -> Vec<u8> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(data.len().div_ceil(3) * 4);
    for group in data.chunks(3) {
        let b = [
            group[0],
            *group.get(1).unwrap_or(&0),
            *group.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= group.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]);
            } else {
                out.push(b'=');
            }
        }
    }
    out
}

/// `OSC 52 ; targets ; data ST`.
fn osc52(targets: &str, data: &[u8]) -> Vec<u8> {
    let mut seq = format!("\x1b]52;{targets};").into_bytes();
    seq.extend_from_slice(data);
    seq.extend_from_slice(b"\x1b\\");
    seq
}

fn set(t: &mut Terminal, seq: &[u8]) -> Option<Vec<u8>> {
    t.input(seq);
    assert_eq!(t.take_replies(), b"", "OSC 52 replied");
    t.take_clipboard_write()
}

// ctlseqs, OSC 52: "The second parameter, Pd, gives the selection data
// [...] encoded using base64"; the host takes the decoded bytes once.
// covers: osc.52.write
#[test]
fn osc_52_sets_the_selection() {
    let mut t = term(8, 2, b"");
    assert_eq!(t.take_clipboard_write(), None);
    assert_eq!(
        set(&mut t, b"\x1b]52;c;aGVsbG8=\x07"),
        Some(b"hello".to_vec())
    );
    assert_eq!(t.take_clipboard_write(), None, "taken once");
    assert_eq!(
        set(&mut t, &osc52("c", &base64("é\u{1F600}\n".as_bytes()))),
        Some("é\u{1F600}\n".as_bytes().to_vec())
    );
}

// RFC 4648, section 10's test vectors, padded and unpadded; ctlseqs: an
// empty Pd is the empty selection, which clears it.
#[test]
fn osc_52_decodes_rfc_4648s_vectors() {
    let vectors: [(&[u8], &[u8], &[u8]); 7] = [
        (b"", b"", b""),
        (b"Zg==", b"Zg", b"f"),
        (b"Zm8=", b"Zm8", b"fo"),
        (b"Zm9v", b"Zm9v", b"foo"),
        (b"Zm9vYg==", b"Zm9vYg", b"foob"),
        (b"Zm9vYmE=", b"Zm9vYmE", b"fooba"),
        (b"Zm9vYmFy", b"Zm9vYmFy", b"foobar"),
    ];
    let mut t = term(8, 2, b"");
    for (padded, unpadded, want) in vectors {
        assert_eq!(set(&mut t, &osc52("c", padded)), Some(want.to_vec()));
        assert_eq!(set(&mut t, &osc52("c", unpadded)), Some(want.to_vec()));
    }
    let every_byte: Vec<u8> = (0..=255).collect();
    assert_eq!(
        set(&mut t, &osc52("c", &base64(&every_byte))),
        Some(every_byte)
    );
}

// RFC 4648, section 3.3: "Implementations MUST reject the encoded data if
// it contains characters outside the base alphabet"; invalid data is
// dropped and sets nothing, where ctlseqs clears the selection. A set
// waiting for the host stays.
// covers: osc.52.write
#[test]
fn osc_52_with_invalid_base64_is_dropped() {
    let invalid: [&[u8]; 16] = [
        b"aGVs bG8=",
        b"aGVsbG8=\x7f",
        b"aGVsbG8*",
        b"aGVs-G8_",
        b"aGVsbG8=x",
        b"aGVsbG8==",
        b"aG=sbG8=",
        b"Zg=",
        b"Zg===",
        b"Z",
        b"Zm9vY",
        b"=",
        b"====",
        b"Zh==",
        b"Zm9=",
        "aGVsbG8\u{e9}".as_bytes(),
    ];
    let mut t = term(8, 2, b"");
    for data in invalid {
        assert_eq!(set(&mut t, &osc52("c", data)), None, "{data:?} set");
        t.input(&osc52("c", b"aGk="));
        t.input(&osc52("c", data));
        assert_eq!(t.take_clipboard_write(), Some(b"hi".to_vec()), "{data:?}");
    }
}

// ctlseqs, OSC 52: Pc names "clipboard, primary, secondary, select, or
// cut-buffers 0 through 7", and an empty Pc is "s 0". vtium has one
// selection, which c, p and s name; q and the cut buffers name none of
// vtium's, and a character outside the set is skipped, as xterm skips it.
// covers: osc.52.write
#[test]
fn osc_52_sets_the_selection_for_c_p_and_s() {
    let mut t = term(8, 2, b"");
    for targets in ["", "c", "p", "s", "s0", "cps", "q7c", "pc", "xc", "cc"] {
        assert_eq!(
            set(&mut t, &osc52(targets, b"aGk=")),
            Some(b"hi".to_vec()),
            "{targets:?}"
        );
    }
    for targets in ["q", "0", "7", "q01234567", "x", "C", ";"] {
        assert_eq!(set(&mut t, &osc52(targets, b"aGk=")), None, "{targets:?}");
    }
    // no second `;`: neither targets nor data
    assert_eq!(set(&mut t, b"\x1b]52;aGk=\x1b\\"), None);
    assert_eq!(set(&mut t, b"\x1b]52\x1b\\"), None);
}

// ADR-1000, osc.52.read (refused): the query would hand the selection to
// whatever reads the pty, so it gets no reply, by BEL or by ST, and sets
// nothing.
// covers: osc.52.read
#[test]
fn osc_52_query_gets_no_reply() {
    let mut t = term(8, 2, b"");
    t.input(&osc52("c", b"aGk="));
    for query in [
        &b"\x1b]52;c;?\x07"[..],
        b"\x1b]52;c;?\x1b\\",
        b"\x1b]52;;?\x07",
        b"\x1b]52;p;?\x07",
        b"\x1b]52;s0;?\x9c",
    ] {
        t.input(query);
        assert_eq!(t.take_replies(), b"", "{query:?} replied");
    }
    assert_eq!(t.take_clipboard_write(), Some(b"hi".to_vec()));
}

// The cap (src/bounds.rs, CLIPBOARD_MAX): a set of exactly CLIPBOARD_MAX
// bytes reaches the host, and one byte more is dropped whole, though both
// take 699,052 bytes of base64 and fit the OSC bound.
// covers: osc.52.write
#[test]
fn osc_52_at_the_cap_sets_and_one_byte_over_is_dropped() {
    assert_eq!(CLIPBOARD_MAX, 512 * 1024);
    let at = vec![b'x'; CLIPBOARD_MAX];
    let over = vec![b'x'; CLIPBOARD_MAX + 1];
    assert_eq!(base64(&at).len(), 699_052);
    assert_eq!(base64(&over).len(), 699_052);
    let mut t = term(8, 2, b"");
    assert_eq!(set(&mut t, &osc52("c", &base64(&at))), Some(at));
    assert_eq!(set(&mut t, &osc52("c", &base64(&over))), None);
}

// vtium-parse's README, "Notes for vtium-term": a string past the OSC bound
// is truncated, and a truncated OSC 52 sets nothing, never a part of the
// selection. The data is base64 throughout. Its kept prefix also decodes
// past CLIPBOARD_MAX, as any prefix at the bound does, so the cap refuses it
// too; `a_truncated_osc_52_sets_nothing` in src/terminal/clipboard.rs shows
// the truncation rule alone, and `a_truncated_osc_4_list_gets_no_reply`
// shows it through the parser.
#[test]
fn osc_52_past_the_osc_bound_is_dropped() {
    let data = b"QUFB".repeat(OSC_MAX / 4 + 1);
    let mut t = term(8, 2, b"");
    assert_eq!(set(&mut t, &osc52("c", &data)), None);
}

// The selection waits for the host in the terminal's state, and `{:?}` of
// the terminal shows its length, never its bytes.
#[test]
fn debug_output_shows_no_selection() {
    let mut t = term(8, 2, b"");
    t.input(&osc52("c", &base64(b"secret")));
    let shown = format!("{t:?}");
    assert!(!shown.contains("[115, 101, 99, 114, 101, 116]"), "{shown}");
    assert!(!shown.contains("c2VjcmV0"), "{shown}");
    assert!(shown.contains("6 bytes"), "{shown}");
}

// vtium-parse's README, "A string acts only when its terminator completes
// it": CAN, SUB and an ESC that starts another sequence cancel an OSC 52.
#[test]
fn a_cancelled_osc_52_sets_nothing() {
    let mut t = term(8, 2, b"");
    for end in [&b"\x18"[..], b"\x1a", b"\x1b[m", b"\x1bx"] {
        let mut seq = b"\x1b]52;c;aGVsbG8=".to_vec();
        seq.extend_from_slice(end);
        assert_eq!(set(&mut t, &seq), None, "{end:?}");
    }
}

// A second set before the host takes the first replaces it, so what waits
// is at most one cap's worth.
#[test]
fn a_later_set_replaces_an_earlier_one() {
    let mut t = term(8, 2, b"");
    t.input(&osc52("c", b"b25l"));
    t.input(&osc52("p", b"dHdv"));
    assert_eq!(t.take_clipboard_write(), Some(b"two".to_vec()));
}

/// What a stream sets, fed in `pieces`.
fn fed(pieces: &[&[u8]]) -> Option<Vec<u8>> {
    let mut t = Terminal::new(8, 2, 0);
    for piece in pieces {
        t.input(piece);
    }
    assert_eq!(t.take_replies(), b"");
    t.take_clipboard_write()
}

// docs/architecture.md, "Determinism": an OSC 52 cut into two input calls
// at every offset, or three at every pair, sets what it sets whole.
// covers: osc.52.write
#[test]
fn osc_52_split_at_every_offset() {
    for (seq, want) in [
        (
            b"\x1b]52;c;aGVsbG8=\x1b\\".to_vec(),
            Some(b"hello".to_vec()),
        ),
        (b"\x1b]52;c;aGVsbG8=\x07".to_vec(), Some(b"hello".to_vec())),
        (b"\x1b]52;c;aGVs*G8=\x1b\\".to_vec(), None),
        (b"\x1b]52;c;?\x1b\\".to_vec(), None),
    ] {
        assert_eq!(fed(&[&seq]), want);
        for i in 0..=seq.len() {
            let (a, rest) = seq.split_at(i);
            assert_eq!(fed(&[a, rest]), want, "cut at {i} of {seq:?}");
            for j in 0..=rest.len() {
                let (b, c) = rest.split_at(j);
                assert_eq!(fed(&[a, b, c]), want, "cut at {i}, {j} of {seq:?}");
            }
        }
    }
}

// The same at the cap and one byte over, cut where the state changes: in
// the `52;c;` header, around the last base64 group, and in the ST.
#[test]
fn osc_52_at_the_cap_split_where_its_state_changes() {
    for (len, sets) in [(CLIPBOARD_MAX, true), (CLIPBOARD_MAX + 1, false)] {
        let data = vec![b'y'; len];
        let seq = osc52("c", &base64(&data));
        let n = seq.len();
        let want = sets.then(|| data.clone());
        for cut in (0..=10).chain(n - 12..=n) {
            let (a, b) = seq.split_at(cut);
            assert_eq!(fed(&[a, b]), want, "cut at {cut} of {n}");
        }
        let pieces: Vec<&[u8]> = seq.chunks(4093).collect();
        assert_eq!(fed(&pieces), want, "in pieces");
    }
}
