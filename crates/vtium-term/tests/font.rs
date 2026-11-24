//! OSC 50 (ADR-1200 D5): a program asks for a font set by name, which the
//! host takes with `Terminal::take_font_request`, or asks which set is in
//! use. Both are ignored until the host turns font operations on.

use vtium_term::Terminal;

/// A terminal whose host takes OSC 50 and draws `unifont`.
fn on() -> Terminal {
    let mut t = Terminal::new(10, 5, 10);
    t.set_font_ops(true);
    assert!(t.set_font_name("unifont"));
    t
}

/// Feeds `bytes` whole, as the host does after each return.
fn feed(t: &mut Terminal, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        bytes = &bytes[t.input(bytes)..];
    }
}

#[test]
fn a_set_asked_for_waits_for_the_host() {
    let mut t = on();
    t.input(b"\x1b]50;ter-v16n\x1b\\");
    assert_eq!(t.take_font_request().as_deref(), Some("ter-v16n"));
    assert_eq!(t.take_font_request(), None, "taken once");
    // A later request replaces an earlier one, and BEL ends it too.
    feed(&mut t, b"\x1b]50;Lat2-Terminus16\x07\x1b]50;unscii-8\x07");
    assert_eq!(t.take_font_request().as_deref(), Some("unscii-8"));
    assert_eq!(t.take_replies(), b"", "a set asked for gets no reply");
}

#[test]
fn the_query_names_the_hosts_set_ended_as_it_was_asked() {
    let mut t = on();
    t.input(b"\x1b]50;?\x1b\\");
    assert_eq!(t.take_replies(), b"\x1b]50;unifont\x1b\\");
    t.input(b"\x1b]50;?\x07");
    assert_eq!(t.take_replies(), b"\x1b]50;unifont\x07");
    // A request changes the answer only once the host switches.
    feed(&mut t, b"\x1b]50;other\x07\x1b]50;?\x07");
    assert_eq!(t.take_replies(), b"\x1b]50;unifont\x07");
    assert!(t.set_font_name("other"));
    t.input(b"\x1b]50;?\x07");
    assert_eq!(t.take_replies(), b"\x1b]50;other\x07");
}

#[test]
fn what_cannot_name_a_set_is_ignored() {
    let mut t = on();
    let long = format!("\x1b]50;{}\x07", "a".repeat(65));
    for seq in [
        b"\x1b]50;-misc-fixed-medium-r-normal--13-*\x07".as_slice(),
        b"\x1b]50;#+1\x07",
        b"\x1b]50;#\x07",
        b"\x1b]50;.hidden\x07",
        b"\x1b]50;\x07",
        b"\x1b]50;a b\x07",
        long.as_bytes(),
    ] {
        t.input(seq);
        assert_eq!(
            t.take_font_request(),
            None,
            "{:?}",
            String::from_utf8_lossy(seq)
        );
        assert_eq!(t.take_replies(), b"");
    }
    let longest = format!("\x1b]50;{}\x07", "a".repeat(64));
    t.input(longest.as_bytes());
    assert_eq!(t.take_font_request(), Some("a".repeat(64)));
}

// ADR-1200 D5: a Pt that holds `/` is a font file's path, which waits for
// the host to check it. A relative path waits too, so that the host can
// log why it refuses one.
#[test]
fn a_font_files_path_waits_for_the_host() {
    let mut t = on();
    let longest = format!("/{}", "a".repeat(4095));
    for path in [
        "/usr/share/kbd/consolefonts/ter-v16n.psf.gz",
        "/usr/share/fonts/TTF/DejaVu Sans Mono.ttf",
        "fonts/a.ttf",
        longest.as_str(),
    ] {
        feed(&mut t, format!("\x1b]50;{path}\x07").as_bytes());
        assert_eq!(t.take_font_request().as_deref(), Some(path));
        assert_eq!(t.take_replies(), b"", "a path asked for gets no reply");
    }
}

// ADR-1200 D5: a path longer than PATH_MAX, 4096 bytes, or one whose bytes
// are not UTF-8 is dropped with no reply, as a bad name is. The parser
// gives each byte that is not UTF-8 as U+FFFD, so that mark drops a path.
#[test]
fn a_path_too_long_or_not_utf8_is_dropped_with_no_reply() {
    let mut t = on();
    let long = format!("\x1b]50;/{}\x07", "a".repeat(4096));
    for seq in [
        long.as_bytes(),
        b"\x1b]50;/usr/share/fonts/a\xff.ttf\x07",
        b"\x1b]50;/usr/share/fonts/a\xc3.ttf\x1b\\",
        "\x1b]50;/usr/share/fonts/a\u{fffd}.ttf\x07".as_bytes(),
    ] {
        feed(&mut t, seq);
        assert_eq!(
            t.take_font_request(),
            None,
            "{:?}",
            String::from_utf8_lossy(seq)
        );
        assert_eq!(t.take_replies(), b"");
    }
}

// ADR-1200 D5: once the host switches to a font file by its path, the
// query reports that path, which a switch takes back. The host names only
// an absolute path, of at most 4096 bytes, with no control character.
#[test]
fn the_query_names_the_font_file_the_host_switched_to() {
    let mut t = on();
    let path = "/usr/share/fonts/TTF/DejaVu Sans Mono.ttf";
    assert!(t.set_font_name(path));
    t.input(b"\x1b]50;?\x07");
    assert_eq!(t.take_replies(), format!("\x1b]50;{path}\x07").as_bytes());
    let long = format!("/{}", "a".repeat(4096));
    for bad in ["fonts/a.ttf", "/a\x1b]0;evil", "/a\nb", long.as_str()] {
        assert!(!t.set_font_name(bad), "{bad:?}");
    }
    assert!(t.set_font_name(&long[..4096]), "4096 bytes");
    assert!(t.set_font_name(path));
    t.input(b"\x1b]50;?\x1b\\");
    assert_eq!(
        t.take_replies(),
        format!("\x1b]50;{path}\x1b\\").as_bytes(),
        "the name before each refusal"
    );
}

#[test]
fn off_osc_50_and_its_query_are_ignored() {
    // As until the host turns font operations on.
    let mut t = Terminal::new(10, 5, 10);
    assert!(t.set_font_name("unifont"));
    feed(&mut t, b"\x1b]50;ter-v16n\x07\x1b]50;?\x07");
    assert_eq!(t.take_font_request(), None);
    assert_eq!(t.take_replies(), b"");
    // Turned off, a request still waiting is dropped.
    let mut t = on();
    t.input(b"\x1b]50;ter-v16n\x07");
    t.set_font_ops(false);
    assert_eq!(t.take_font_request(), None);
    feed(&mut t, b"\x1b]50;ter-v16n\x07\x1b]50;?\x07");
    assert_eq!(t.take_font_request(), None);
    assert_eq!(t.take_replies(), b"");
}

#[test]
fn the_host_names_only_a_set_name() {
    let mut t = on();
    assert!(!t.set_font_name("a\x1b]0;evil"));
    assert!(!t.set_font_name(""));
    t.input(b"\x1b]50;?\x07");
    assert_eq!(t.take_replies(), b"\x1b]50;unifont\x07", "the name before");
}

// vtium/vtium#384: a font with no set's name, as a font file's path or a
// family has, is shown after a set was: the query then names none, never
// the set no longer shown
#[test]
fn a_query_after_the_host_shows_a_font_with_no_sets_name_names_none() {
    let mut t = on();
    t.clear_font_name();
    t.input(b"\x1b]50;?\x07");
    assert_eq!(t.take_replies(), b"\x1b]50;\x07");
    assert!(t.set_font_name("ter-v16n"));
    t.input(b"\x1b]50;?\x07");
    assert_eq!(t.take_replies(), b"\x1b]50;ter-v16n\x07");
}

#[test]
fn a_query_before_the_host_names_a_set_names_none() {
    let mut t = Terminal::new(10, 5, 10);
    t.set_font_ops(true);
    t.input(b"\x1b]50;?\x07");
    assert_eq!(t.take_replies(), b"\x1b]50;\x07");
}

#[test]
fn the_input_returns_after_a_set_asked_for_so_a_query_names_the_new_set() {
    let mut t = on();
    let set = b"\x1b]50;ter-v16n\x07";
    let both = [&set[..], b"\x1b]50;?\x07"].concat();
    assert_eq!(t.input(&both), set.len(), "the input returns after the set");
    // The host switches, then feeds the rest.
    assert!(t.font_requested());
    assert_eq!(t.take_font_request().as_deref(), Some("ter-v16n"));
    assert!(!t.font_requested());
    assert!(t.set_font_name("ter-v16n"));
    assert_eq!(t.input(&both[set.len()..]), both.len() - set.len());
    assert_eq!(t.take_replies(), b"\x1b]50;ter-v16n\x07");
}
