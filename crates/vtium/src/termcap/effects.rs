//! What each capability of vtium's terminfo entries does when vtium-term
//! reads it: each string expanded as ncurses' `tparm` expands it, with
//! the parameters a program passes, fed to a terminal, and the terminal's
//! state, replies or key reports checked. The table is generated from the
//! entries and checked against them byte for byte, so a capability edited
//! wrong in an entry and blessed into the table passes those checks; it
//! fails here, where the terminal shows what the bytes do.

#![expect(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::too_many_lines,
    reason = "test-only: a test fails by panicking, and the checks are one table"
)]

use vtium_term::{
    Attrs, Color, CursorStyle, Glyph, Mods, MouseButton, MouseEvent, MouseTracking, Rgb, Special,
    Terminal, UnderlineStyle,
};

use super::terminfo::{Cap, ENTRY, KMSCON, parse};

/// A `tparm` parameter.
#[derive(Clone, Debug)]
enum P {
    N(i64),
    S(Vec<u8>),
}

use P::N;

/// The string `cap` expanded with `params`, as ncurses' `tparm` expands it
/// for the `%` operations the entries use; `$<..>` padding is dropped, as
/// `tputs` drops it.
///
/// A string with no `%p` is in termcap's form, and ncurses expands it in
/// two ways a terminfo string never is. Before it starts, it pushes the
/// parameters the string's operations take ([`termcap_params`]), the last
/// first, so the first `%d` pops the first parameter. And `%i`, which
/// adds one to the first two parameters, also writes them into the bottom
/// two places of the stack, the first parameter into the bottom one,
/// which is where it pushed the second: `\E[%i%d;%dR` with 2 and 3 gives
/// `\E[4;3R`. `tput` and python's `curses.tparm` both give that.
fn tparm(cap: &[u8], params: &[P]) -> Vec<u8> {
    let mut params: Vec<P> = params.to_vec();
    params.resize(9, N(0));
    let mut stack: Vec<P> = Vec::new();
    let termcap = !cap.windows(2).any(|w| w == b"%p");
    if termcap {
        stack.extend(params[..termcap_params(cap)].iter().rev().cloned());
    }
    // ncurses adds one for the first %i only
    let mut incremented = false;
    let num = |p: Option<P>| match p {
        Some(N(n)) => n,
        Some(P::S(_)) | None => 0,
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < cap.len() {
        let b = cap[i];
        if b == b'$' && cap.get(i + 1) == Some(&b'<') {
            while cap[i] != b'>' {
                i += 1;
            }
            i += 1;
            continue;
        }
        if b != b'%' {
            out.push(b);
            i += 1;
            continue;
        }
        i += 1;
        let op = cap[i];
        i += 1;
        match op {
            b'%' => out.push(b'%'),
            // ncurses sends a zero as 0x80, so that no NUL goes out, and
            // keeps the low byte of anything else
            b'c' => match num(stack.pop()) {
                0 => out.push(0x80),
                n => out.push(u8::try_from(n & 0xff).expect("a byte")),
            },
            b's' => match stack.pop() {
                Some(P::S(s)) => out.extend(s),
                Some(N(n)) => out.extend(n.to_string().bytes()),
                None => {}
            },
            b'd' => out.extend(num(stack.pop()).to_string().bytes()),
            b'0'..=b'9' | b'.' | b':' => {
                // a printf form such as %2.2X
                let start = i - 1;
                while !matches!(cap[i], b'd' | b'o' | b'x' | b'X' | b's') {
                    i += 1;
                }
                let spec = std::str::from_utf8(&cap[start..i]).expect("ASCII");
                let conv = cap[i];
                i += 1;
                let width: usize = spec
                    .split('.')
                    .next()
                    .and_then(|w| w.trim_start_matches(':').parse().ok())
                    .unwrap_or(0);
                let n = num(stack.pop());
                let text = match conv {
                    b'X' => format!("{n:0width$X}"),
                    b'x' => format!("{n:0width$x}"),
                    b'o' => format!("{n:0width$o}"),
                    _ => format!("{n:width$}"),
                };
                out.extend(text.bytes());
            }
            b'p' => {
                let k = usize::from(cap[i] - b'1');
                i += 1;
                stack.push(params[k].clone());
            }
            b'{' => {
                let end = i + cap[i..].iter().position(|&c| c == b'}').expect("a }");
                let n = std::str::from_utf8(&cap[i..end])
                    .expect("ASCII")
                    .parse()
                    .expect("a number");
                stack.push(N(n));
                i = end + 1;
            }
            b'\'' => {
                stack.push(N(i64::from(cap[i])));
                i += 2;
            }
            b'i' if !incremented => {
                incremented = true;
                for (k, p) in params.iter_mut().take(2).enumerate() {
                    if let N(n) = p {
                        *n += 1;
                        if termcap && let Some(place) = stack.get_mut(k) {
                            *place = N(*n);
                        }
                    }
                }
            }
            b'i' => {}
            b'+' | b'-' | b'*' | b'/' | b'm' | b'&' | b'|' | b'^' | b'=' | b'<' | b'>' | b'A'
            | b'O' => {
                let (y, x) = (num(stack.pop()), num(stack.pop()));
                stack.push(N(match op {
                    b'+' => x + y,
                    b'-' => x - y,
                    b'*' => x * y,
                    b'/' => x / y,
                    b'm' => x % y,
                    b'&' => x & y,
                    b'|' => x | y,
                    b'^' => x ^ y,
                    b'=' => i64::from(x == y),
                    b'<' => i64::from(x < y),
                    b'>' => i64::from(x > y),
                    b'A' => i64::from(x != 0 && y != 0),
                    _ => i64::from(x != 0 || y != 0),
                }));
            }
            b'!' => {
                let x = num(stack.pop());
                stack.push(N(i64::from(x == 0)));
            }
            b'~' => {
                let x = num(stack.pop());
                stack.push(N(!x));
            }
            b'?' | b';' => {}
            b't' => {
                if num(stack.pop()) == 0 {
                    i = skip(cap, i, true);
                }
            }
            b'e' => i = skip(cap, i, false),
            op => panic!("tparm: %{}", char::from(op)),
        }
    }
    out
}

/// Where a false `%t` (`to_else`) or a taken branch's `%e` skips to: past
/// the matching `%e` or `%;` at the same depth, or past the `%;` alone.
fn skip(cap: &[u8], mut i: usize, to_else: bool) -> usize {
    let mut depth = 0;
    while i + 1 < cap.len() {
        if cap[i] == b'%' {
            match cap[i + 1] {
                b'?' => depth += 1,
                b';' if depth == 0 => return i + 2,
                b';' => depth -= 1,
                b'e' if depth == 0 && to_else => return i + 2,
                _ => {}
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    cap.len()
}

/// How many parameters ncurses pushes before it expands `cap`, a string
/// with no `%p`, as ncurses' `_nc_tparm_analyze` counts them: it follows
/// how deep the stack would be, starting one below empty, and counts each
/// operation that pops while the stack is below empty, at most two.
fn termcap_params(cap: &[u8]) -> usize {
    let (mut level, mut number) = (-1_i32, 0_usize);
    let mut i = 0;
    while i < cap.len() {
        if cap[i] != b'%' {
            i += 1;
            continue;
        }
        i += 1;
        // a printf form's flags, width and precision, as in %:-3d or %2.2X
        if cap.get(i) == Some(&b':') {
            i += 1;
            while matches!(cap.get(i), Some(b'-' | b'+' | b'#' | b' ')) {
                i += 1;
            }
        }
        while matches!(cap.get(i), Some(b'0'..=b'9' | b'.')) {
            i += 1;
        }
        let Some(&op) = cap.get(i) else { break };
        let pops = level < 0 && number < 2;
        match op {
            b'd' | b'o' | b'x' | b'X' | b'c' | b'+' | b'-' | b'*' | b'/' | b'm' | b'A' | b'O'
            | b'&' | b'|' | b'^' | b'=' | b'<' | b'>' => {
                number += usize::from(pops);
                level -= 1;
            }
            b'l' | b's' | b'!' | b'~' => number += usize::from(pops),
            b'g' => {
                level += 1;
                i += 1;
            }
            b'P' => i += 1,
            b'\'' => {
                level += 1;
                i += 2;
            }
            b'{' => {
                level += 1;
                i += 1;
                while cap.get(i).is_some_and(u8::is_ascii_digit) {
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    number
}

#[test]
fn tparm_expands_as_ncurses_does() {
    // each checked against `tput` and python's `curses.tparm`
    let cases: [(&[u8], &[P], &[u8]); 14] = [
        (b"\x1b[%i%p1%d;%p2%dH", &[N(4), N(9)], b"\x1b[5;10H"),
        (
            b"\x1b[%?%p1%{8}%<%t3%p1%d%e%p1%{16}%<%t9%p1%{8}%-%d%e38;5;%p1%d%;m",
            &[N(9)],
            b"\x1b[91m",
        ),
        (
            b"\x1b[%?%p1%{8}%<%t3%p1%d%e%p1%{16}%<%t9%p1%{8}%-%d%e38;5;%p1%d%;m",
            &[N(200)],
            b"\x1b[38;5;200m",
        ),
        (b"\x1b[?2026%?%p1%{1}%-%tl%eh%;", &[N(1)], b"\x1b[?2026h"),
        (b"\x1b[?2026%?%p1%{1}%-%tl%eh%;", &[N(2)], b"\x1b[?2026l"),
        (b"%p1%c\x1b[%p2%{1}%-%db", &[N(120), N(5)], b"x\x1b[4b"),
        (b"%p1%c", &[N(0)], b"\x80"),
        (b"%p1%c", &[N(1000)], b"\xe8"),
        // termcap's form, with no %p
        (b"\x1b[%i%d;%dR", &[N(2), N(3)], b"\x1b[4;3R"),
        (b"%d;%d", &[N(2), N(3)], b"2;3"),
        (b"%d;%d;%d", &[N(1), N(2), N(3)], b"1;2;0"),
        (b"%i%d", &[N(2), N(3)], b"3"),
        (b"%d;%i%d", &[N(2), N(3)], b"2;3"),
        (b"%i%d;%d;%d", &[N(1), N(2), N(3)], b"3;2;0"),
    ];
    for (cap, params, want) in cases {
        assert_eq!(
            String::from_utf8_lossy(&tparm(cap, params)),
            String::from_utf8_lossy(want)
        );
    }
}

// This file's tparm expands every capability of both entries that takes
// parameters as ncurses' own tparm does, with each of several parameter
// lists, so the checks below feed the terminal what a program would send.
// ncurses is what `tput` runs; without `tput` and `tic` the test says it
// skipped and passes.
#[test]
fn tparm_matches_tput_for_each_capability_with_parameters() {
    let found = |tool: &str| {
        std::process::Command::new(tool)
            .arg("-V")
            .output()
            .is_ok_and(|o| o.status.success())
    };
    if !found("tput") || !found("tic") {
        eprintln!(
            "SKIPPED tparm_matches_tput_for_each_capability_with_parameters: tput or tic (ncurses) is not installed"
        );
        return;
    }
    let dir = std::env::temp_dir().join(format!("vtium-termcap-tput-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory");
    // each list reaches other branches of setaf's, setab's and sgr's
    // conditions, and rep's %c of 0 and of more than a byte
    let lists: [[i64; 9]; 4] = [
        [1, 2, 3, 4, 5, 6, 7, 8, 9],
        [0; 9],
        [9, 15, 200, 255, 1000, 7, 3, 1, 2],
        [65, 66, 67, 68, 69, 70, 71, 72, 73],
    ];
    let mut compared = 0;
    for (term, source) in [(super::NAME, ENTRY), (super::KMSCON, KMSCON)] {
        let ti = dir.join(format!("{term}.ti"));
        std::fs::write(&ti, source).expect("the entry written");
        let tic = std::process::Command::new("tic")
            .arg("-x")
            .arg("-o")
            .arg(&dir)
            .arg(&ti)
            .output()
            .expect("tic runs");
        assert!(tic.status.success(), "tic {term}: {tic:?}");
        let (_, caps) = parse(source);
        for (name, cap) in &caps {
            let Cap::Str(value) = cap else { continue };
            // u8 is a pattern a program matches a reply against, which
            // tparm does not expand
            if !value.contains(&b'%') || value.windows(2).any(|w| w == b"%[") {
                continue;
            }
            // tput takes as many parameters as the string uses, and reads
            // any after them as further capabilities to send
            let used = (1..=9)
                .rev()
                .find(|k| value.windows(3).any(|w| w == format!("%p{k}").as_bytes()))
                .unwrap_or_else(|| termcap_params(value));
            for list in &lists {
                // a parameter the string prints with %s is a string
                let params: Vec<P> = (1..=used)
                    .zip(list)
                    .map(|(k, &n)| {
                        if value.windows(5).any(|w| w == format!("%p{k}%s").as_bytes()) {
                            P::S(format!("s{k}").into_bytes())
                        } else {
                            N(n)
                        }
                    })
                    .collect();
                let args: Vec<String> = params
                    .iter()
                    .map(|p| match p {
                        N(n) => n.to_string(),
                        P::S(s) => String::from_utf8_lossy(s).into_owned(),
                    })
                    .collect();
                let tput = std::process::Command::new("tput")
                    .env("TERMINFO", &dir)
                    .arg("-T")
                    .arg(term)
                    .arg(name)
                    .args(&args)
                    .output()
                    .expect("tput runs");
                assert!(tput.status.success(), "tput {term} {name}: {tput:?}");
                let have = tparm(value, &params);
                assert_eq!(
                    have,
                    tput.stdout,
                    "{term} {name} {args:?}: {} where tput gives {}",
                    have.escape_ascii(),
                    tput.stdout.escape_ascii()
                );
                compared += 1;
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    // both entries' capabilities with parameters, u8 aside, four lists
    // each
    assert_eq!(compared, (30 + 28) * 4, "the capabilities compared");
}

fn terminal() -> Terminal {
    let mut t = Terminal::new(80, 24, 100);
    t.screen_mut().set_scrollback_max(100);
    // vtium's default, `backspace-delete`
    t.set_backspace_sends_delete(true);
    t.set_sync_enabled(true);
    t
}

/// DECRQM's state for `mode`: 1 set, 2 reset.
fn mode(t: &mut Terminal, mode: &str) -> u8 {
    t.take_replies();
    t.input(format!("\x1b[{mode}$p").as_bytes());
    let reply = String::from_utf8(t.take_replies()).expect("ASCII");
    let state = reply
        .strip_prefix(&format!("\x1b[{mode};"))
        .and_then(|r| r.strip_suffix("$y"))
        .unwrap_or_else(|| panic!("DECRQM {mode}: {reply:?}"));
    state.parse().expect("a state")
}

/// Row `row`'s text, blanks trimmed at its end.
fn text(t: &Terminal, row: usize) -> String {
    let line = t.screen().line(row).expect("a row");
    line.cells()
        .iter()
        .map(|&c| match line.glyph(c) {
            Glyph::Char(c) => c,
            _ => ' ',
        })
        .collect::<String>()
        .trim_end()
        .to_owned()
}

/// The glyph `bytes` print at the cursor's column on row 0.
fn printed(t: &mut Terminal, bytes: &[u8]) -> char {
    t.input(b"\r");
    t.input(bytes);
    text(t, 0).chars().next().unwrap_or(' ')
}

/// The keysym and modifiers of each key capability.
fn key(name: &str) -> Option<(u32, Mods)> {
    const LEFT: u32 = 0xff51;
    const UP: u32 = 0xff52;
    const RIGHT: u32 = 0xff53;
    const DOWN: u32 = 0xff54;
    const PGUP: u32 = 0xff55;
    const PGDN: u32 = 0xff56;
    const F1: u32 = 0xffbe;
    let none = Mods::empty();
    let shift = Mods::SHIFT;
    // the modifier digit's modifiers: 3 Alt, 4 Shift Alt, 5 Ctrl, 6 Ctrl
    // Shift, 7 Ctrl Alt
    let digit = |d: char| match d {
        '3' => Mods::ALT,
        '4' => Mods::SHIFT.union(Mods::ALT),
        '5' => Mods::CTRL,
        '6' => Mods::CTRL.union(Mods::SHIFT),
        _ => Mods::CTRL.union(Mods::ALT),
    };
    let modified = |base: &str, sym: u32| {
        name.strip_prefix(base)
            .and_then(|d| d.chars().next())
            .filter(|_| name.len() == base.len() + 1)
            .map(|d| (sym, digit(d)))
    };
    let k = match name {
        "kbs" => (0xff08, none),
        "kcbt" => (0xfe20, shift),
        "kcub1" => (LEFT, none),
        "kcud1" => (DOWN, none),
        "kcuf1" => (RIGHT, none),
        "kcuu1" => (UP, none),
        "kdch1" => (0xffff, none),
        "kend" => (0xff57, none),
        "khome" => (0xff50, none),
        "kich1" => (0xff63, none),
        "knp" => (PGDN, none),
        "kpp" => (PGUP, none),
        "kLFT" => (LEFT, shift),
        "kRIT" => (RIGHT, shift),
        "kUP" | "kri" => (UP, shift),
        "kDN" | "kind" => (DOWN, shift),
        "kPRV" => (PGUP, shift),
        "kNXT" => (PGDN, shift),
        // F21 and F22 are Shift with F11 and F12
        "kf21" => (F1 + 10, shift),
        "kf22" => (F1 + 11, shift),
        _ => {
            if let Some(n) = name.strip_prefix("kf").and_then(|n| n.parse::<u32>().ok()) {
                return Some((F1 + n - 1, none));
            }
            return modified("kUP", UP)
                .or_else(|| modified("kDN", DOWN))
                .or_else(|| modified("kLFT", LEFT))
                .or_else(|| modified("kRIT", RIGHT))
                .or_else(|| modified("kPRV", PGUP))
                .or_else(|| modified("kNXT", PGDN));
        }
    };
    Some(k)
}

/// A string capability of an entry, expanded with `params`.
struct Entry {
    term: String,
    caps: Vec<(String, Cap)>,
}

impl Entry {
    fn string(&self, name: &str, params: &[P]) -> Option<Vec<u8>> {
        match self.caps.iter().find(|(n, _)| n == name) {
            Some((_, Cap::Str(v))) => Some(tparm(v, params)),
            _ => None,
        }
    }

    fn has(&self, name: &str) -> bool {
        self.caps.iter().any(|(n, _)| n == name)
    }
}

/// Checks the capability `name` of `entry`, whose text is `cap`, through a
/// terminal; panics on a capability with no check.
fn check(entry: &Entry, name: &str, cap: &Cap) {
    let s = |params: &[P]| entry.string(name, params).expect("a string");
    let mut t = terminal();
    let pen = |t: &Terminal| *t.screen().pen();
    let attr = |bytes: &[u8], attr: Attrs| {
        let mut t = terminal();
        t.input(bytes);
        assert!(pen(&t).attrs.contains(attr), "{name}: {attr:?}");
    };
    // an attribute set by `on` and cleared by this capability
    let clears = |on: &str, attr: Attrs| {
        let mut t = terminal();
        t.input(&entry.string(on, &[]).expect("its pair"));
        t.input(&s(&[]));
        assert!(!pen(&t).attrs.contains(attr), "{name}: {attr:?}");
    };
    // a mode this capability sets (`state` 1) or resets (2), from the other
    let sets = |dec: &str, state: u8, params: &[P]| {
        let mut t = terminal();
        t.input(format!("\x1b[{dec}{}", if state == 1 { 'l' } else { 'h' }).as_bytes());
        let mode_name = dec.trim_start_matches('?');
        let private = if dec.starts_with('?') { "?" } else { "" };
        t.input(&s(params));
        assert_eq!(
            mode(&mut t, &format!("{private}{mode_name}")),
            state,
            "{name}"
        );
    };
    // the cursor at (col, row) after `before` then this capability
    let moves = |before: &[u8], params: &[P], to: (usize, usize)| {
        let mut t = terminal();
        t.input(before);
        t.input(&s(params));
        assert_eq!(t.screen().cursor(), to, "{name}");
    };
    // row 0 after "abcdef", the cursor at column 1, and this capability
    let edits = |params: &[P], want: &str| {
        let mut t = terminal();
        t.input(b"abcdef\x1b[1;2H");
        t.input(&s(params));
        assert_eq!(text(&t, 0), want, "{name}");
    };
    // the rows after five lines "a" to "e" on a 5-row terminal, the cursor
    // at row 1, and this capability
    let rows = |params: &[P], want: [&str; 5]| {
        let mut t = Terminal::new(10, 5, 0);
        t.input(b"a\r\nb\r\nc\r\nd\r\ne\x1b[2;1H");
        t.input(&s(params));
        let have: Vec<String> = (0..5).map(|r| text(&t, r)).collect();
        assert_eq!(have, want, "{name}");
    };
    // the rows and the cursor after five lines "a" to "e" on a 5-row
    // terminal, then `before`, which places the cursor and may set a
    // scroll region, and this capability
    let scrolls = |before: &[u8], want: [&str; 5], cursor: (usize, usize)| {
        let mut t = Terminal::new(10, 5, 0);
        t.input(b"a\r\nb\r\nc\r\nd\r\ne");
        t.input(before);
        t.input(&s(&[]));
        let have: Vec<String> = (0..5).map(|r| text(&t, r)).collect();
        assert_eq!(
            have,
            want,
            "{name} after {:?}",
            String::from_utf8_lossy(before)
        );
        assert_eq!(t.screen().cursor(), cursor, "{name}: the cursor");
    };
    if let Some((sym, mods)) = key(name) {
        t.key(sym, 0, mods, None);
        assert_eq!(
            String::from_utf8_lossy(&t.take_replies()),
            String::from_utf8_lossy(&s(&[])),
            "{name}: the key's report"
        );
        return;
    }
    match (name, cap) {
        // booleans and numbers
        ("am", Cap::Bool) => assert_eq!(mode(&mut t, "?7"), 1, "autowrap at start"),
        ("bce", Cap::Bool) => {
            t.input(b"\x1b[44m\x1b[K");
            let line = t.screen().line(0).expect("a row");
            let cell = line.cells()[79];
            assert_eq!(line.style(cell).bg, Color::Indexed(4), "bce");
        }
        ("xenl", Cap::Bool) => {
            t.input(&[b'x'; 80]);
            assert_eq!(t.screen().cursor(), (80, 0), "a wrap pending");
            t.input(b"\r");
            assert_eq!(t.screen().cursor(), (0, 0), "no wrap taken at CR");
        }
        ("km", Cap::Bool) => {
            t.key(u32::from(b'a'), 0, Mods::ALT, Some('a'));
            assert_eq!(t.take_replies(), b"\x1ba", "Alt sends ESC");
        }
        ("mir", Cap::Bool) => {
            t.input(&entry.string("smir", &[]).unwrap_or(b"\x1b[4h".to_vec()));
            t.input(b"\x1b[3;5H");
            assert_eq!(
                t.screen().cursor(),
                (4, 2),
                "the cursor moves in insert mode"
            );
        }
        ("msgr", Cap::Bool) => {
            t.input(b"\x1b[7m\x1b[3;5H");
            assert_eq!(t.screen().cursor(), (4, 2), "the cursor moves in standout");
            assert!(pen(&t).attrs.contains(Attrs::INVERSE));
        }
        // npc: vtium has no padding character to send; nothing to drive
        ("npc", Cap::Bool) => {}
        ("ccc", Cap::Bool) => assert!(entry.has("initc"), "ccc with initc"),
        ("Tc", Cap::Bool) => assert!(entry.has("setrgbf"), "Tc with setrgbf"),
        ("Su", Cap::Bool) => assert!(entry.has("Smulx"), "Su with Smulx"),
        ("XF", Cap::Bool) => assert!(entry.has("fe") && entry.has("fd"), "XF with fe"),
        ("colors", Cap::Num(n)) => {
            t.input(b"\x1b[38;5;255m");
            assert_eq!(pen(&t).fg, Color::Indexed(255));
            assert_eq!(*n, 256);
        }
        // pairs: any foreground with any background, which ncurses bounds
        ("pairs", Cap::Num(n)) => assert!(*n >= 256, "{n}"),
        // the size a program assumes with none from the pty
        ("cols", Cap::Num(80)) | ("lines", Cap::Num(24)) => {}
        ("it", Cap::Num(n)) => {
            t.input(b"\t");
            assert_eq!(
                t.screen().cursor(),
                (usize::try_from(*n).expect("small"), 0)
            );
        }
        // characters and rendition
        ("acsc", _) => {
            let pairs = s(&[]);
            let smacs = entry.string("smacs", &[]).expect("smacs");
            let rmacs = entry.string("rmacs", &[]).expect("rmacs");
            for pair in pairs.chunks(2) {
                let (vt100, sent) = (pair[0], pair[1]);
                let mut drawn = terminal();
                let want = printed(&mut drawn, &[b"\x1b(0".as_slice(), &[vt100]].concat());
                let mut t = terminal();
                let have = printed(&mut t, &[smacs.as_slice(), &[sent]].concat());
                assert_eq!(have, want, "acsc {}", char::from(vt100));
                assert!(!have.is_ascii(), "{}", char::from(vt100));
                let back = printed(&mut t, &[rmacs.as_slice(), &[sent]].concat());
                assert_eq!(back, char::from(sent), "rmacs");
            }
        }
        ("smacs", _) => assert_eq!(printed(&mut t, &[s(&[]).as_slice(), b"q"].concat()), '─'),
        ("rmacs", _) => {
            t.input(b"\x1b(0");
            assert_eq!(printed(&mut t, &[s(&[]).as_slice(), b"q"].concat()), 'q');
        }
        ("bold", _) => attr(&s(&[]), Attrs::BOLD),
        ("dim", _) => attr(&s(&[]), Attrs::DIM),
        ("sitm", _) => attr(&s(&[]), Attrs::ITALIC),
        ("blink", _) => attr(&s(&[]), Attrs::BLINK),
        ("rev" | "smso", _) => attr(&s(&[]), Attrs::INVERSE),
        ("smxx", _) => attr(&s(&[]), Attrs::STRIKE),
        ("smul", _) => {
            t.input(&s(&[]));
            assert_eq!(pen(&t).attrs.underline(), UnderlineStyle::Single);
        }
        ("rmso", _) => clears("smso", Attrs::INVERSE),
        ("ritm", _) => clears("sitm", Attrs::ITALIC),
        ("rmxx", _) => clears("smxx", Attrs::STRIKE),
        ("rmul", _) => {
            t.input(b"\x1b[4m");
            t.input(&s(&[]));
            assert_eq!(pen(&t).attrs.underline(), UnderlineStyle::None);
        }
        ("Smulx", _) => {
            let styles = [
                UnderlineStyle::None,
                UnderlineStyle::Single,
                UnderlineStyle::Double,
                UnderlineStyle::Curly,
                UnderlineStyle::Dotted,
                UnderlineStyle::Dashed,
            ];
            for (n, style) in (0..).zip(styles) {
                t.input(&s(&[N(n)]));
                assert_eq!(pen(&t).attrs.underline(), style, "Smulx {n}");
            }
        }
        ("Setulc", _) => {
            t.input(&s(&[N(0x10_2030)]));
            assert_eq!(
                pen(&t).underline_color,
                Color::Rgb(Rgb::new(0x10, 0x20, 0x30))
            );
            assert!(!pen(&t).attrs.contains(Attrs::DIM), "no SGR 2 read from it");
        }
        ("sgr0", _) => {
            t.input(b"\x1b[1;3;4;7;31;42m\x1b(0");
            t.input(&s(&[]));
            assert_eq!(pen(&t), vtium_term::Style::DEFAULT);
            assert_eq!(printed(&mut t, b"q"), 'q', "the charset back to ASCII");
        }
        ("sgr", _) => {
            // sgr's parameters: standout, underline, reverse, blink, dim,
            // bold, invisible, protect, alternate charset
            let attrs = [
                Some(Attrs::INVERSE),
                None,
                Some(Attrs::INVERSE),
                Some(Attrs::BLINK),
                Some(Attrs::DIM),
                Some(Attrs::BOLD),
                Some(Attrs::CONCEAL),
            ];
            for (i, want) in attrs.iter().enumerate() {
                let mut params = vec![N(0); 9];
                params[i] = N(1);
                let mut t = terminal();
                t.input(b"\x1b[3m");
                t.input(&s(&params));
                let have = pen(&t).attrs;
                assert!(!have.contains(Attrs::ITALIC), "sgr resets first");
                match want {
                    // KNOWN GAP: kmscon.ti's sgr asks for SGR 8,
                    // which vtium-term does not do until the work on SGR
                    // 8 and 28 lands (vtium/vtium#37); vtium's entry
                    // leaves it out. This fails once it lands: then check
                    // it as the others.
                    Some(Attrs::CONCEAL) if entry.term == super::KMSCON => {
                        assert!(!have.contains(Attrs::CONCEAL), "SGR 8 landed");
                    }
                    // vtium's entry does not ask for it (see its sgr)
                    Some(Attrs::CONCEAL) => assert_eq!(have, Attrs::default(), "sgr p7"),
                    Some(a) => assert!(have.contains(*a), "{}: sgr p{}", entry.term, i + 1),
                    None => assert_eq!(have.underline(), UnderlineStyle::Single, "sgr p2"),
                }
            }
            let mut params = vec![N(0); 9];
            params[8] = N(1);
            assert_eq!(
                printed(&mut t, &[s(&params).as_slice(), b"q"].concat()),
                '─'
            );
        }
        ("setaf" | "setab", _) => {
            for n in [0, 7, 8, 15, 16, 200, 255] {
                t.input(&s(&[N(n)]));
                let p = pen(&t);
                let have = if name == "setaf" { p.fg } else { p.bg };
                assert_eq!(
                    have,
                    Color::Indexed(u8::try_from(n).expect("a byte")),
                    "{name} {n}"
                );
            }
        }
        ("setrgbf" | "setrgbb", _) => {
            t.input(&s(&[N(1), N(2), N(3)]));
            let p = pen(&t);
            let have = if name == "setrgbf" { p.fg } else { p.bg };
            assert_eq!(have, Color::Rgb(Rgb::new(1, 2, 3)));
            assert_eq!(p.attrs, Attrs::default(), "nothing else set");
        }
        ("op", _) => {
            t.input(b"\x1b[31;42m");
            t.input(&s(&[]));
            assert_eq!((pen(&t).fg, pen(&t).bg), (Color::Default, Color::Default));
        }
        // cursor movement
        ("cr", _) => moves(b"abc", &[], (0, 0)),
        ("ht", _) => moves(b"a", &[], (8, 0)),
        ("cbt", _) => moves(b"\x1b[1;12H", &[], (8, 0)),
        ("cub1", _) => moves(b"\x1b[3;5H", &[], (3, 2)),
        ("cuf1", _) => moves(b"\x1b[3;5H", &[], (5, 2)),
        ("cuu1", _) => moves(b"\x1b[3;5H", &[], (4, 1)),
        // terminfo leaves cud1 undefined on the bottom row, where a program
        // sends ind instead; vtium's cud1, a newline, scrolls there as
        // xterm's does. Above it, cud1 moves down and changes no row.
        ("cud1", _) => {
            scrolls(b"\x1b[3;5H", ["a", "b", "c", "d", "e"], (4, 3));
            scrolls(b"\x1b[2;4r\x1b[2;5H", ["a", "b", "c", "d", "e"], (4, 2));
        }
        // ind is defined on the bottom row, of the screen or of the scroll
        // region csr sets, where it scrolls the rows up and keeps the cursor
        ("ind", _) => {
            scrolls(b"\x1b[5;1H", ["b", "c", "d", "e", ""], (0, 4));
            scrolls(b"\x1b[2;4r\x1b[4;1H", ["a", "c", "d", "", "e"], (0, 3));
        }
        ("cub", _) => moves(b"\x1b[3;5H", &[N(3)], (1, 2)),
        ("cuf", _) => moves(b"\x1b[3;5H", &[N(3)], (7, 2)),
        ("cuu", _) => moves(b"\x1b[3;5H", &[N(2)], (4, 0)),
        ("cud", _) => moves(b"\x1b[3;5H", &[N(2)], (4, 4)),
        ("cup", _) => moves(b"", &[N(4), N(9)], (9, 4)),
        ("home", _) => moves(b"\x1b[3;5H", &[], (0, 0)),
        ("hpa", _) => moves(b"\x1b[3;5H", &[N(9)], (9, 2)),
        ("vpa", _) => moves(b"\x1b[3;5H", &[N(9)], (4, 9)),
        ("sc", _) => {
            t.input(b"\x1b[3;5H");
            t.input(&s(&[]));
            t.input(b"\x1b[H\x1b8");
            assert_eq!(t.screen().cursor(), (4, 2));
        }
        ("rc", _) => {
            t.input(b"\x1b[3;5H\x1b7\x1b[H");
            t.input(&s(&[]));
            assert_eq!(t.screen().cursor(), (4, 2));
        }
        ("hts", _) => {
            t.input(b"\x1b[3g\x1b[1;4H");
            t.input(&s(&[]));
            t.input(b"\r\t");
            assert_eq!(t.screen().cursor(), (3, 0));
        }
        ("tbc", _) => {
            t.input(&s(&[]));
            t.input(b"\r\t");
            assert_eq!(t.screen().cursor(), (79, 0), "no tab stop left");
        }
        ("csr", _) => {
            t.input(&s(&[N(2), N(10)]));
            assert_eq!(t.screen().margins(), (2, 10));
        }
        // ri is defined on the top row, of the screen or of the scroll
        // region, where it scrolls the rows down and keeps the cursor
        ("ri", _) => {
            scrolls(b"\x1b[1;1H", ["", "a", "b", "c", "d"], (0, 0));
            scrolls(b"\x1b[2;4r\x1b[2;1H", ["a", "", "b", "c", "e"], (0, 1));
        }
        // editing
        ("clear", _) => {
            t.input(b"abc\r\ndef\x1b[3;5H");
            t.input(&s(&[]));
            assert_eq!((text(&t, 0), text(&t, 1)), (String::new(), String::new()));
            assert_eq!(t.screen().cursor(), (0, 0));
        }
        ("ed", _) => rows(&[], ["a", "", "", "", ""]),
        ("el", _) => edits(&[], "a"),
        ("el1", _) => edits(&[], "  cdef"),
        ("ech", _) => edits(&[N(2)], "a  def"),
        ("dch", _) => edits(&[N(2)], "adef"),
        ("dch1", _) => edits(&[], "acdef"),
        ("ich", _) => edits(&[N(2)], "a  bcdef"),
        ("ich1" | "ch1", _) => edits(&[], "a bcdef"),
        ("dl", _) => rows(&[N(2)], ["a", "d", "e", "", ""]),
        ("dl1", _) => rows(&[], ["a", "c", "d", "e", ""]),
        ("il", _) => rows(&[N(2)], ["a", "", "", "b", "c"]),
        ("il1", _) => rows(&[], ["a", "", "b", "c", "d"]),
        ("indn", _) => rows(&[N(2)], ["c", "d", "e", "", ""]),
        ("rin", _) => rows(&[N(2)], ["", "", "a", "b", "c"]),
        ("rep", _) => {
            t.input(&s(&[N(i64::from(b'x')), N(5)]));
            assert_eq!(text(&t, 0), "xxxxx");
        }
        ("E3", _) => {
            for i in 0..30 {
                t.input(format!("{i}\r\n").as_bytes());
            }
            assert!(t.screen().scrollback().len() > 0);
            t.input(&s(&[]));
            assert_eq!(t.screen().scrollback().len(), 0);
        }
        ("bel", _) => {
            t.input(&s(&[]));
            assert!(t.take_bell());
        }
        ("flash", _) => {
            let bytes = entry.caps.iter().find(|(n, _)| n == "flash");
            let Some((_, Cap::Str(raw))) = bytes else {
                panic!("flash")
            };
            // the screen reversed before the pause, and back after it
            let before = raw.split(|&b| b == b'$').next().expect("a part");
            t.input(before);
            assert_eq!(mode(&mut t, "?5"), 1, "flash reverses the screen");
            t.input(&s(&[]));
            assert_eq!(mode(&mut t, "?5"), 2, "and restores it");
        }
        ("rs2", _) => {
            t.input(b"\x1b[?7l\x1b[1mabc");
            t.input(&s(&[]));
            assert_eq!(mode(&mut t, "?7"), 1);
            assert_eq!(text(&t, 0), "");
            assert_eq!(pen(&t), vtium_term::Style::DEFAULT);
        }
        // modes
        ("smam", _) => sets("?7", 1, &[]),
        ("rmam", _) => sets("?7", 2, &[]),
        ("smir", _) => sets("4", 1, &[]),
        ("rmir", _) => sets("4", 2, &[]),
        ("cnorm", _) => sets("?25", 1, &[]),
        ("civis", _) => sets("?25", 2, &[]),
        ("smcup", _) => sets("?1049", 1, &[]),
        ("rmcup", _) => sets("?1049", 2, &[]),
        ("fe", _) => sets("?1004", 1, &[]),
        ("fd", _) => sets("?1004", 2, &[]),
        ("BE", _) => sets("?2004", 1, &[]),
        ("BD", _) => sets("?2004", 2, &[]),
        ("XM", _) => {
            t.input(&s(&[N(1)]));
            assert_eq!(t.mouse_tracking(), MouseTracking::Normal);
            assert_eq!(mode(&mut t, "?1006"), 1);
            t.input(&s(&[N(0)]));
            assert_eq!(t.mouse_tracking(), MouseTracking::Off);
        }
        ("Sync", _) => {
            t.input(&s(&[N(1)]));
            assert!(t.synchronized(), "Sync 1 begins");
            t.input(&s(&[N(2)]));
            assert!(!t.synchronized(), "Sync 2 ends");
        }
        // reports and replies
        ("kmous", _) => {
            let on = entry
                .string("XM", &[N(1)])
                .unwrap_or_else(|| b"\x1b[?1000h".to_vec());
            t.input(&on);
            t.take_replies();
            t.mouse(
                249,
                2,
                0,
                0,
                MouseButton::Left,
                MouseEvent::Press,
                Mods::empty(),
            );
            let report = t.take_replies();
            assert!(
                report.starts_with(&s(&[])),
                "{:?}",
                String::from_utf8_lossy(&report)
            );
        }
        ("kxIN" | "kxOUT", _) => {
            t.input(b"\x1b[?1004h");
            t.take_replies();
            t.focus(name == "kxIN");
            assert_eq!(t.take_replies(), s(&[]));
        }
        ("PS" | "PE", _) => {
            t.input(b"\x1b[?2004h");
            t.take_replies();
            t.paste(b"hi");
            let pasted = t.take_replies();
            let ok = if name == "PS" {
                pasted.starts_with(&s(&[]))
            } else {
                pasted.ends_with(&s(&[]))
            };
            assert!(ok, "{:?}", String::from_utf8_lossy(&pasted));
        }
        ("u7", _) => {
            t.input(b"\x1b[3;4H");
            t.input(&s(&[]));
            assert_eq!(t.take_replies(), b"\x1b[3;4R");
        }
        ("u6", _) => {
            // the report u7 asks for, as u6 reads it: the row, then the
            // column, each counted from 1. u6 is in termcap's form, where
            // ncurses' %i hands the first two parameters out the other way
            // round (see tparm), so the column from 0 goes first to have
            // it expand to row 3, column 4.
            t.input(b"\x1b[3;4H\x1b[6n");
            let reply = t.take_replies();
            assert_eq!(reply, b"\x1b[3;4R", "row 3, column 4");
            assert_eq!(reply, s(&[N(3), N(2)]), "u6");
        }
        ("u9", _) => {
            t.input(&s(&[]));
            let reply = t.take_replies();
            assert!(reply.starts_with(b"\x1b[?") && reply.ends_with(b"c"), "DA1");
        }
        ("u8", Cap::Str(pattern)) => {
            // the DA1 reply u9 asks for, as u8 matches it
            assert_eq!(pattern.as_slice(), b"\x1b[?%[;0123456789]c");
            t.input(b"\x1b[c");
            let reply = t.take_replies();
            let body = reply
                .strip_prefix(b"\x1b[?")
                .and_then(|r| r.strip_suffix(b"c"))
                .expect("DA1");
            assert!(body.iter().all(|b| b";0123456789".contains(b)), "{body:?}");
        }
        ("XR", _) => {
            t.input(&s(&[]));
            assert!(t.take_replies().starts_with(b"\x1bP>|"), "XTVERSION");
        }
        ("RV", _) => {
            t.input(&s(&[]));
            assert!(t.take_replies().starts_with(b"\x1b[>"), "DA2");
        }
        ("Se" | "Ss", _) => {
            t.input(b"\x1b[6 q");
            if name == "Ss" {
                t.input(&s(&[N(4)]));
                assert_eq!(t.cursor_style(), CursorStyle::UnderlineSteady);
            } else {
                t.input(&s(&[]));
                assert_eq!(t.cursor_style(), CursorStyle::Default);
            }
        }
        ("Cs", _) => {
            t.input(&s(&[P::S(b"rgb:ff/00/00".to_vec())]));
            assert_eq!(
                t.palette().special(Special::Cursor),
                Some(Rgb::new(255, 0, 0))
            );
        }
        ("Cr", _) => {
            t.input(b"\x1b]12;rgb:ff/00/00\x07");
            t.input(&s(&[]));
            assert_eq!(t.palette().special(Special::Cursor), None);
        }
        ("initc", _) => {
            t.input(&s(&[N(1), N(1000), N(0), N(500)]));
            assert_eq!(t.palette().color(1), Rgb::new(0xff, 0, 0x7f));
        }
        ("oc", _) => {
            let before = t.palette().color(1);
            t.input(b"\x1b]4;1;rgb:12/34/56\x1b\\");
            assert_ne!(t.palette().color(1), before);
            t.input(&s(&[]));
            assert_eq!(t.palette().color(1), before);
        }
        ("Ms", _) => {
            t.input(&s(&[P::S(b"c".to_vec()), P::S(b"aGk=".to_vec())]));
            assert_eq!(t.take_clipboard_write().as_deref(), Some(&b"hi"[..]));
        }
        _ => panic!("{name}: no check of what it does"),
    }
}

// Every capability of each entry does what its name says when vtium-term
// reads it: a key capability is what vtium sends for that key, a mode's is
// the mode set or reset, a report's is the report vtium sends, and so on.
// covers: platform.terminfo
#[test]
fn each_capability_does_what_it_names() {
    for (term, source) in [(super::NAME, ENTRY), (super::KMSCON, KMSCON)] {
        let (name, caps) = parse(source);
        assert_eq!(name, term);
        let entry = Entry { term: name, caps };
        for (cap, value) in &entry.caps {
            check(&entry, cap, value);
        }
    }
}
