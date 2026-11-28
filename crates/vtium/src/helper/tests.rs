use super::launcher::{active_request, holds_account, refusal, status_ids};
use super::sandbox::{
    SANDBOX_DOMAIN, SANDBOX_DOMAIN_SHIFT, SANDBOX_FALLBACK, SANDBOX_SECCOMP_SHIFT, SANDBOX_SOCKETS,
    log_sandbox_once, sandbox_notice,
};
use super::search::leave_out_request;
use super::wire::{
    ACTIVE, ACTIVE_BYTES_MAX, FRAME_COVERAGE, FRAME_DONE, FRAME_DRAWS_NOTHING, FRAME_FAILED,
    FRAME_FILL, FRAME_FILLED, FRAME_GLYPHS, FRAME_NO_OWN_CELL, FRAME_OUTLINE_CELL, FRAME_OWN_CELL,
    FRAME_SANDBOX, FRAME_SOURCES, FRAME_STOPPED, FRAME_WARNING, OTHER_BUILD, encode, len32,
};
use super::*;
use crate::pointer_file::FRAME_POINTER;
use rustix::fs::{FileType, Mode};
use std::process::Command as Process;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use vtium_font::Doubled;
use vtium_font::set::Set;
use vtium_settings::batch::{BATCH_MAX, Batch, Entry, MERGED_BATCH_MAX};

/// Not a test of the helper: run by a trap of [`undumpable`] with
/// [`DIE_BY`] set, it turns off this process's core dumps and ends the
/// process by the signal [`DIE_BY`] names; run by the test harness, it
/// does nothing. The standard library catches SIGSEGV to report a
/// stack overflow, and its handler puts back the default action and
/// returns. So the first SIGSEGV can end in that handler, and the
/// signal is sent until one ends the process.
#[test]
fn the_stand_in_dies_without_a_core_dump() {
    let Ok(name) = std::env::var(DIE_BY) else {
        return;
    };
    assert!(["SEGV", "XCPU"].contains(&name.as_str()), "{name}");
    let signal = if name == "SEGV" {
        rustix::process::Signal::SEGV
    } else {
        rustix::process::Signal::XCPU
    };
    rustix::process::set_dumpable_behavior(rustix::process::DumpableBehavior::NotDumpable).unwrap();
    loop {
        let _ = rustix::process::kill_process(rustix::process::getpid(), signal);
        std::thread::yield_now();
    }
}

// vtium/vtium#451: a stand-in shell that a script ends by SIGSEGV or
// SIGXCPU ends by that signal and leaves no core dump. The kernel says
// in the wait status whether it wrote one, a pipe to systemd-coredump
// included.
#[test]
fn a_stand_in_ended_by_a_signal_leaves_no_core_dump() {
    use std::os::unix::process::ExitStatusExt;
    for (signal, number) in [("SEGV", 11), ("XCPU", 24)] {
        let status = std::process::Command::new(stand_in_shell())
            .arg("-c")
            .arg(undumpable(&format!("kill -{signal} $$; sleep 5")))
            .status()
            .unwrap();
        assert_eq!(status.signal(), Some(number), "{signal}: {status:?}");
        assert!(!status.core_dumped(), "{signal}: a core dump was written");
    }
}
use crate::scratch::Scratch;
use vtium_font::set::{PixelFormat, SetBuilder, StrikeBuilder};
use vtium_font::{Face, Style, Width};

/// [`CELL`] with no line spacing.
const AT: SpacedCell = SpacedCell::plain(CELL);

const CELL: Cell = Cell {
    width: 8,
    height: 16,
};

/// A set of `chars`, each solid, at `cell`, its chunks raw as the
/// helper writes them.
fn set(cell: Cell, chars: &[char]) -> Vec<u8> {
    let mut s = StrikeBuilder::new(PixelFormat::R8, cell, Style::default()).compress(false);
    let g = s.glyph(
        Width::Narrow,
        vec![255; (cell.width * cell.height) as usize],
    );
    for &c in chars {
        s.map(c, g);
    }
    SetBuilder::new("t").strike(s).write().unwrap()
}

/// A frame of `kind` holding `payload`; a glyph frame's set at no line
/// spacing, its band of none in front.
fn frame_bytes(kind: u32, payload: &[u8]) -> Vec<u8> {
    match kind {
        FRAME_GLYPHS | FRAME_FILL => spaced_frame(kind, Band::NONE, payload),
        _ => raw_frame(kind, payload),
    }
}

/// A glyph frame of `kind` holding `set`, whose cell line spacing
/// makes taller by `band`.
fn spaced_frame(kind: u32, band: Band, set: &[u8]) -> Vec<u8> {
    let mut payload = band.above.to_le_bytes().to_vec();
    payload.extend_from_slice(&band.below.to_le_bytes());
    payload.extend_from_slice(set);
    raw_frame(kind, &payload)
}

fn raw_frame(kind: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = kind.to_le_bytes().to_vec();
    out.extend_from_slice(&len32(payload.len()).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// A fill's end or a stop of `kind` naming `cell` at no line spacing.
fn fill_frame(kind: u32, cell: Cell) -> Vec<u8> {
    spaced_end(kind, SpacedCell::plain(cell))
}

/// A fill's end or a stop of `kind` naming `at`, a cell and its band:
/// the width, the height, the rows above and the rows below.
fn spaced_end(kind: u32, at: SpacedCell) -> Vec<u8> {
    let words = [at.cell.width, at.cell.height, at.band.above, at.band.below];
    let c: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    raw_frame(kind, &c)
}

fn filled(cell: Cell) -> Vec<u8> {
    fill_frame(FRAME_FILLED, cell)
}

/// The helper's word that it took a stop at `cell`, as it answers
/// each.
fn stopped(cell: Cell) -> Vec<u8> {
    fill_frame(FRAME_STOPPED, cell)
}

/// A helper whose answers are `script`, run by the shell after it
/// writes `fixture` where the script finds it as `$1`.
struct StandIn {
    helper: Helper,
    woke: mpsc::Receiver<()>,
    dir: PathBuf,
}

/// Answers with the fixture, then reads every request until the client
/// closes its input, as the real helper runs until then. cat runs in
/// the shell's own place: a shell gives a command it runs in the
/// background /dev/null for its standard input, so a background cat
/// would read none of the requests.
const ANSWERS: &str = "cat \"$1\"; exec cat >/dev/null";

/// As [`ANSWERS`], but the fixture waits until the test creates the
/// file `$1.go`, so the answers come late.
const HELD: &str = "while [ ! -e \"$1.go\" ]; do sleep 0.01; done; cat \"$1\"; exec cat >/dev/null";

impl StandIn {
    fn new(name: &str, script: &str, fixture: &[u8]) -> StandIn {
        StandIn::limited(name, script, fixture, 64 << 20)
    }

    fn limited(name: &str, script: &str, fixture: &[u8], limit: u64) -> StandIn {
        StandIn::of(name, script, fixture, (limit, "source=builtin:unscii-8\n"))
    }

    /// [`StandIn::limited`] for the frit whose text is `text`.
    fn of(name: &str, script: &str, fixture: &[u8], (limit, text): (u64, &str)) -> StandIn {
        let dir = std::env::temp_dir().join(format!("vtium-helper-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fixture_path = dir.join("answer");
        std::fs::write(&fixture_path, fixture).unwrap();
        let (tx, woke) = mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let command = Command {
            program: stand_in_shell().to_path_buf(),
            args: vec![
                "-c".into(),
                undumpable(script).into(),
                "stand-in".into(),
                fixture_path.into(),
            ],
            account: None,
        };
        let wake = Arc::new(move || {
            let _ = tx.lock().map(|t| t.send(()));
        });
        let mut helper = Helper::new(
            Launcher::open(command).unwrap(),
            wake,
            t0(),
            frit(text),
            None,
            Shared::default(),
            limit,
        );
        // a stand-in's answers are written out in full, and the early
        // bold ask would wait for one the fixture does not hold; the
        // tests of that ask turn it on
        helper.asks_bold_early = false;
        StandIn { helper, woke, dir }
    }

    /// Polls as the loop does, each time the reader wakes it, until the
    /// helper has nothing outstanding, has gone, or is given up; the
    /// answers taken.
    fn run(&mut self, now: Instant) -> Vec<Answer> {
        let mut out = Vec::new();
        for _ in 0..50 {
            out.extend(self.helper.poll(now));
            if !self.helper.busy() || self.helper.failed().is_some() {
                break;
            }
            let _ = self.woke.recv_timeout(Duration::from_secs(10));
        }
        out
    }

    /// Polls as the loop does, each time the reader wakes it, until
    /// `done` holds for the helper or the reader stays quiet for 10 s.
    fn until(&mut self, now: Instant, done: impl Fn(&Helper) -> bool) {
        loop {
            let _ = self.helper.poll(now);
            if done(&self.helper) || self.woke.recv_timeout(Duration::from_secs(10)).is_err() {
                break;
            }
        }
    }

    /// Drops the helper and makes a new one for the same frit, as the
    /// console does when no head draws the frit and then one draws it
    /// again. The new helper takes the record of what the dropped
    /// one used up, as the console's [`Failures`] record gives it.
    fn again(&mut self) {
        let record = Rc::clone(&self.helper.failures);
        let fresh = Helper::new(
            self.helper.launcher.clone(),
            Arc::clone(&self.helper.wake),
            t0(),
            self.helper.frit.clone(),
            None,
            Shared::default(),
            self.helper.limit,
        );
        self.helper = fresh.remembering(record);
        self.helper.asks_bold_early = false;
    }

    /// Polls until the helper's process is gone.
    fn until_gone(&mut self, now: Instant) {
        while self.helper.running.is_some()
            && self.woke.recv_timeout(Duration::from_secs(10)).is_ok()
        {
            let _ = self.helper.poll(now);
        }
    }

    fn held(&self, c: char) -> bool {
        self.helper
            .cells
            .iter()
            .any(|s| s.at == AT && s.held.has(u32::from(c)))
    }
}

impl Drop for StandIn {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The stand-ins' clock: a fixed instant, moved on by hand.
#[expect(clippy::disallowed_methods, reason = "the tests' fixed instant")]
fn t0() -> Instant {
    Instant::now()
}

/// The screen showing `chars`, no cluster.
fn shown(chars: &[char]) -> Shown {
    Shown {
        cluster_width: Default::default(),
        doubled: None,
        variant: Variant::default(),
        chars: chars.to_vec(),
        clusters: Vec::new(),
    }
}

/// The frit `t` of `text`.
fn frit(text: &str) -> FritText {
    FritText {
        name: "t".to_owned(),
        text: text.to_owned(),
    }
}

// A test that holds a CellWait sets the wait of the cell queries on
// its thread for as long as it holds it, and no longer, so a later test
// on the same thread, which may judge the console's own wait, gets
// that wait back.
#[test]
fn a_cell_wait_holds_only_while_its_guard_lives() {
    let wait = || CELL_WAIT.with(std::cell::Cell::get);
    let (long, short) = (Duration::from_secs(30), Duration::from_secs(5));
    assert_eq!(wait(), None, "the console's wait");
    {
        let _outer = CellWait::new(long);
        assert_eq!(wait(), Some(long));
        {
            let _inner = CellWait::new(short);
            assert_eq!(wait(), Some(short));
        }
        assert_eq!(wait(), Some(long), "the outer guard's wait is back");
    }
    assert_eq!(wait(), None, "the console's wait is back");
}

#[test]
fn a_request_is_the_helpers_format() {
    let both = Style {
        bold: true,
        italic: true,
    };
    let bytes = encode(
        &frit("x=y\n"),
        None,
        (
            ASK,
            CELL,
            Band { above: 2, below: 1 },
            (both.into(), None),
            ClusterWidth::Single,
        ),
        (&[0x41], &[vec!['e', '\u{301}']]),
        &[(0x42, 0x43)],
    );
    let mut want = b"VTVSREQA".to_vec();
    want.extend_from_slice(&ASK.to_le_bytes());
    want.extend_from_slice(&1u32.to_le_bytes());
    want.extend_from_slice(b"t");
    want.extend_from_slice(&4u32.to_le_bytes());
    want.extend_from_slice(b"x=y\n");
    // the cell, the rows line spacing adds above and below it, the
    // style, bold 1 and italic 2, the script, normal 0, the cluster
    // width, single 1, the line size, normal 0, the reserved
    // capabilities, the codepoint, the cluster of two, the held range,
    // no console font
    for v in [
        8u32, 16, 2, 1, 3, 0, 1, 0, 0, 1, 0x41, 1, 2, 0x65, 0x301, 1, 0x42, 0x43, 0,
    ] {
        want.extend_from_slice(&v.to_le_bytes());
    }
    assert_eq!(bytes, want);
}

const ITALIC: Style = Style {
    bold: false,
    italic: true,
};

/// A set of `chars`, each solid, at `cell`, in one strike of `style`.
fn styled(cell: Cell, style: Style, chars: &[char]) -> Vec<u8> {
    let mut s = StrikeBuilder::new(PixelFormat::R8, cell, style).compress(false);
    let g = s.glyph(
        Width::Narrow,
        vec![200; (cell.width * cell.height) as usize],
    );
    for &c in chars {
        s.map(c, g);
    }
    SetBuilder::new("t").strike(s).write().unwrap()
}

// ADR-1200 D21: italic text is asked for in an ask of its own that
// names the style, and its glyphs go into the store as italic, where
// the regular style does not find them
// covers: sgr.italic
#[test]
fn an_italic_ask_names_its_style_and_its_glyphs_are_held_as_italic() {
    let ask = encode(
        &frit("source=builtin:unscii-8\n"),
        None,
        (
            ASK,
            CELL,
            Band::NONE,
            (ITALIC.into(), None),
            ClusterWidth::Double,
        ),
        (&[0x41], &[]),
        &[],
    );
    let mut fixture = frame_bytes(FRAME_GLYPHS, &styled(CELL, ITALIC, &['A']));
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    let script = format!(
        "head -c {} >\"$1.ask\"; cat \"$1\"; cat >/dev/null",
        ask.len()
    );
    let mut s = StandIn::new("italic", &script, &fixture);
    let now = t0();
    let italic = Shown {
        cluster_width: Default::default(),
        doubled: None,
        variant: ITALIC.into(),
        chars: vec!['A'],
        clusters: Vec::new(),
    };
    s.helper.ask(CELL, Band::NONE, &italic, now);
    let answers = s.run(now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(answers.len(), 1);
    assert_eq!(std::fs::read(s.dir.join("answer.ask")).unwrap(), ask);
    let held = |style| {
        s.helper
            .cells
            .iter()
            .any(|c| (c.at, c.variant) == (AT, Variant::from(style)) && c.held.has(0x41))
    };
    assert!(held(ITALIC), "held as italic");
    assert!(!held(Style::default()), "and not as regular");
    // asked again in italic, it is held; in regular, it is not
    s.helper.ask(CELL, Band::NONE, &italic, now);
    assert!(!s.helper.busy());
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    assert!(s.helper.busy());
}

// Each helper the console starts is told the active fonts
// first, before any request reads a frit: the account whose files pass
// the trust rule and each path the batch lists, in the helper's layout
//
// References:
// - ADR-1205 D4: the console tells each font helper the active fonts.
#[test]
fn each_helper_is_told_the_active_fonts_first() {
    let paths = [
        "/usr/share/vtium/fonts/ter-u16n.bdf.gz",
        "/usr/share/kbd/consolefonts/ter-v16n.psf.gz",
    ];
    let batch = Batch::new(
        paths
            .iter()
            .map(|p| Entry::Path(PathBuf::from(p)))
            .collect(),
    );
    let active = active_request(1000, &batch);
    let mut want = b"VTVSREQA".to_vec();
    for word in [ACTIVE, 1000, 2] {
        want.extend_from_slice(&word.to_le_bytes());
    }
    for path in paths {
        want.extend_from_slice(&len32(path.len()).to_le_bytes());
        want.extend_from_slice(path.as_bytes());
    }
    assert_eq!(active, want);
    let ask = encode(
        &frit("source=builtin:unscii-8\n"),
        None,
        (
            ASK,
            CELL,
            Band::NONE,
            (Variant::default(), None),
            ClusterWidth::Double,
        ),
        (&[0x41], &[]),
        &[],
    );
    let mut fixture = frame_bytes(FRAME_GLYPHS, &styled(CELL, Style::default(), &['A']));
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    let script = format!(
        "head -c {} >\"$1.ask\"; cat \"$1\"; cat >/dev/null",
        active.len() + ask.len()
    );
    let mut s = StandIn::new("active-first", &script, &fixture);
    s.helper.launcher = s.helper.launcher.clone().with_active(1000, &batch);
    let now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    let answers = s.run(now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(answers.len(), 1);
    assert_eq!(
        std::fs::read(s.dir.join("answer.ask")).unwrap(),
        [active, ask].concat()
    );
}

// A new login that finds the batch changed tells a running helper the
// new active fonts at once, after what it was already sent, and keeps
// the request for a helper started after this one ends.
//
// References:
// - ADR-1205 D4, its note of 2026-11-19: an activation takes effect at the next login, with no restart.
#[test]
fn a_running_helper_is_told_the_active_fonts_of_a_new_login() {
    let first = Batch::new(vec![Entry::Path(PathBuf::from("/first.bdf"))]);
    let second = Batch::new(vec![Entry::Path(PathBuf::from("/second.bdf"))]);
    let (before, after) = (active_request(1000, &first), active_request(1000, &second));
    let ask = encode(
        &frit("source=builtin:unscii-8\n"),
        None,
        (
            ASK,
            CELL,
            Band::NONE,
            (Variant::default(), None),
            ClusterWidth::Double,
        ),
        (&[0x41], &[]),
        &[],
    );
    let mut fixture = frame_bytes(FRAME_GLYPHS, &styled(CELL, Style::default(), &['A']));
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    let script = format!(
        "head -c {} >\"$1.ask\"; cat \"$1\"; cat >/dev/null",
        before.len() + ask.len() + after.len()
    );
    let mut s = StandIn::new("active-again", &script, &fixture);
    s.helper.launcher = s.helper.launcher.clone().with_active(1000, &first);
    let now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    let next = s.helper.launcher.clone().with_active(1000, &second);
    s.helper.activate(&next);
    // the stand-in answers once it has read the three requests; a
    // request never sent leaves it waiting, and the reader quiet
    s.until(now, |h| !h.busy());
    assert_eq!(s.helper.failed(), None);
    assert_eq!(
        std::fs::read(s.dir.join("answer.ask")).unwrap(),
        [before, ask, after.clone()].concat()
    );
    assert_eq!(s.helper.launcher.active(), Some(after.as_slice()));
}

// A helper begun again at a new login whose batch changed takes no
// answer to an ask sent before the login, and the helper that answers
// the next ask reads the new active fonts before that ask. Both asks
// name A and Z. A stand-in that reads the old active fonts first holds
// its answer, which maps both, until the test lets it go after the
// login. One that reads the new active fonts first answers with A
// alone, as a font the new batch no longer holds would leave Z out.
//
// References:
// - ADR-1205 D4, its note of 2026-11-19: a deactivation takes effect at the next login.
#[test]
fn a_helper_begun_again_takes_no_answer_to_an_earlier_ask() {
    let first = Batch::new(vec![Entry::Path(PathBuf::from("/one.bdf"))]);
    let second = Batch::new(vec![Entry::Path(PathBuf::from("/two.bdf"))]);
    let (before, after) = (active_request(1000, &first), active_request(1000, &second));
    assert_eq!(before.len(), after.len());
    let mut old = Vec::new();
    for _ in 0..2 {
        old.extend(frame_bytes(
            FRAME_GLYPHS,
            &styled(CELL, Style::default(), &['A', 'Z']),
        ));
        old.extend(frame_bytes(FRAME_DONE, &[]));
    }
    let mut new = frame_bytes(FRAME_GLYPHS, &styled(CELL, Style::default(), &['A']));
    new.extend(frame_bytes(FRAME_DONE, &[]));
    let script = format!(
        "head -c {} >\"$1.$$\"; if cmp -s \"$1.$$\" \"$1.after\"; then cat \"$1.new\"; else while [ ! -e \"$1.go\" ]; do sleep 0.01; done; cat \"$1\"; fi; exec cat >/dev/null",
        after.len()
    );
    let mut s = StandIn::new("begun-again", &script, &old);
    std::fs::write(s.dir.join("answer.new"), &new).unwrap();
    std::fs::write(s.dir.join("answer.after"), &after).unwrap();
    s.helper.launcher = s.helper.launcher.clone().with_active(1000, &first);
    let now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A', 'Z']), now);
    let next = s.helper.launcher.clone().with_active(1000, &second);
    s.helper.begin_again(&next);
    std::fs::write(s.dir.join("answer.go"), b"").unwrap();
    s.helper.ask(CELL, Band::NONE, &shown(&['A', 'Z']), now);
    let answers = s.run(now);
    assert_eq!(s.helper.failed(), None);
    let chars: Vec<Vec<char>> = answers.into_iter().map(|a| a.chars).collect();
    assert_eq!(
        chars,
        [vec!['A']],
        "the answer of a helper that read the new active fonts first, alone"
    );
}

// The active fonts' paths stop before they pass the
// helper's bound, so the helper never refuses the request
//
// References:
// - ADR-1205 D4: the console tells each font helper the active fonts.
#[test]
fn the_active_fonts_stop_at_the_helpers_bound() {
    let long = |i: usize| Entry::Path(PathBuf::from(format!("/{i:04}{}", "x".repeat(995))));
    let batch = Batch::new((0..MERGED_BATCH_MAX / 1000 + 10).map(long).collect());
    let bytes = active_request(0, &batch);
    let count = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    assert_eq!(count as usize, ACTIVE_BYTES_MAX / 1000);
    assert!(bytes.len() - 20 <= ACTIVE_BYTES_MAX + 4 * count as usize);
}

// The helper's bound is the merged batch's, so every path of a batch
// that the packages' lists take past batch.conf's own bound reaches the
// font helper
//
// References:
// - ADR-1205 D4, as amended on 2026-11-27: the merged batch's bound of 1 MiB bounds the request of active fonts.
#[test]
fn the_active_fonts_carry_the_whole_merged_batch() {
    let long = |i: usize| Entry::Path(PathBuf::from(format!("/{i:03}{}", "x".repeat(996))));
    let paths = BATCH_MAX / 1000 + 10;
    let batch = Batch::new((0..paths).map(long).collect());
    let bytes = active_request(0, &batch);
    let count = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    assert_eq!(count as usize, paths);
}

// A batch path that the helper would refuse, one holding a NUL byte or
// one that is not absolute, is left out of the active fonts' request.
// The helper refuses the whole request over one such path and exits,
// so one bad line would otherwise stop every helper from starting. The
// test reads the request with the helper's own parser.
//
// References:
// - ADR-1205 D4: the console tells each font helper the active fonts.
#[test]
fn a_path_the_helper_would_refuse_is_left_out_of_the_active_fonts() {
    let batch = Batch::new(
        ["/usr/share/fonts/a\0b.ttf", "relative.bdf", "/ok.bdf"]
            .iter()
            .map(|p| Entry::Path(PathBuf::from(p)))
            .collect(),
    );
    let bytes = active_request(0, &batch);
    let read = vtium_config::serve::read_request(&mut bytes.as_slice());
    assert!(
        matches!(
            &read,
            Ok(Some(vtium_config::serve::Request::Active(ask)))
                if ask.paths == [PathBuf::from("/ok.bdf")]
        ),
        "the helper reads the request as the one good path: {read:?}"
    );
}

// ADR-1200 D19, D21: an italic glyph an ask past the store's limit
// evicts is forgotten in its own style's record, so it is asked for
// again when the screen shows it
#[test]
fn an_evicted_italic_glyph_is_asked_for_again() {
    let a = styled(CELL, ITALIC, &['A']);
    let b = styled(CELL, Style::default(), &['B']);
    let limit = (a.len() + b.len() - 1) as u64;
    let one = encode(
        &frit("source=builtin:unscii-8\n"),
        None,
        (
            ASK,
            CELL,
            Band::NONE,
            (ITALIC.into(), None),
            ClusterWidth::Double,
        ),
        (&[0x41], &[]),
        &[],
    )
    .len();
    let script = format!(
        "head -c {one} >/dev/null; cat \"$1\"; head -c {one} >/dev/null; cat \"$1.b\"; cat >/dev/null"
    );
    let mut first = frame_bytes(FRAME_GLYPHS, &a);
    first.extend(frame_bytes(FRAME_DONE, &[]));
    let mut s = StandIn::limited("evict-italic", &script, &first, limit);
    let mut second = frame_bytes(FRAME_GLYPHS, &b);
    second.extend(frame_bytes(FRAME_DONE, &[]));
    std::fs::write(s.dir.join("answer.b"), second).unwrap();
    let now = t0();
    let italic = Shown {
        cluster_width: Default::default(),
        doubled: None,
        variant: ITALIC.into(),
        chars: vec!['A'],
        clusters: Vec::new(),
    };
    s.helper.ask(CELL, Band::NONE, &italic, now);
    assert_eq!(s.run(now).len(), 1);
    s.helper.ask(CELL, Band::NONE, &shown(&['B']), now);
    assert_eq!(s.run(now).len(), 1);
    assert_eq!(s.helper.failed(), None);
    let held = |s: &StandIn, style, c: char| {
        s.helper
            .cells
            .iter()
            .any(|r| (r.at, r.variant) == (AT, Variant::from(style)) && r.held.has(u32::from(c)))
    };
    assert!(held(&s, Style::default(), 'B'));
    assert!(!held(&s, ITALIC, 'A'), "italic A left the store");
    s.helper.ask(CELL, Band::NONE, &italic, now);
    assert!(s.helper.busy(), "and is asked for again");
}

const BOLD: Style = Style {
    bold: true,
    italic: false,
};

/// The characters of the early bold ask, printable ASCII.
fn bold_ascii() -> Vec<char> {
    (BOLD_EARLY.0..=BOLD_EARLY.1).collect()
}

/// The bytes of a request of `kind` at [`CELL`] in `style` that names
/// `chars`, as the console sends it for the stand-ins' frit.
fn request_of(kind: u32, style: Style, chars: &[char]) -> Vec<u8> {
    let codepoints: Vec<u32> = chars.iter().map(|&c| u32::from(c)).collect();
    encode(
        &frit("source=builtin:unscii-8\n"),
        None,
        (
            kind,
            CELL,
            Band::NONE,
            (style.into(), None),
            ClusterWidth::Double,
        ),
        (&codepoints, &[]),
        &[],
    )
}

/// The helper's answer to an ask: a set of `chars` in `style`, then
/// the ask's end.
fn answered(style: Style, chars: &[char]) -> Vec<u8> {
    let mut out = frame_bytes(FRAME_GLYPHS, &styled(CELL, style, chars));
    out.extend(frame_bytes(FRAME_DONE, &[]));
    out
}

/// Whether the helper's record of [`AT`] in `style` holds `c`.
fn held_in(helper: &Helper, style: Style, c: char) -> bool {
    helper
        .cells
        .iter()
        .any(|r| (r.at, r.variant) == (AT, Variant::from(style)) && r.held.has(u32::from(c)))
}

// Once the helper has answered the first ask at a cell, the console
// asks once for bold printable ASCII there, so a bold screen that
// follows finds every one of its glyphs held
//
// References:
// - ADR-1200 D18, note of 2026-11-25: the one ask for glyphs the
//   screen does not show yet.
#[test]
fn bold_printable_ascii_is_held_before_a_bold_cell_is_shown() {
    let first = request_of(ASK, Style::default(), &['A']).len();
    let bold = request_of(ASK, BOLD, &bold_ascii());
    let script = format!(
        "head -c {first} >/dev/null; cat \"$1\"; head -c {} >\"$1.bold\"; cat \"$1.b\"; cat >/dev/null",
        bold.len()
    );
    let mut s = StandIn::new("bold-early", &script, &answered(Style::default(), &['A']));
    s.helper.asks_bold_early = true;
    std::fs::write(s.dir.join("answer.b"), answered(BOLD, &bold_ascii())).unwrap();
    let now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    let answers = s.run(now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(answers.len(), 2, "the screen's glyph, then the bold ones");
    assert_eq!(std::fs::read(s.dir.join("answer.bold")).unwrap(), bold);
    for c in bold_ascii() {
        assert!(held_in(&s.helper, BOLD, c), "bold {c} is held");
    }
    // a bold screen then asks for nothing
    let bold_screen = Shown {
        variant: BOLD.into(),
        ..shown(&['H', 'i', '!'])
    };
    s.helper.ask(CELL, Band::NONE, &bold_screen, now);
    assert!(!s.helper.busy());
}

// The early bold ask goes out only while the store is under its
// limit, so the console never asks for glyphs the screen does not show
// into a store that is full
//
// References:
// - ADR-1200 D18, note of 2026-11-25: only while the store is under
//   its limit.
#[test]
fn no_bold_ask_goes_out_while_the_store_is_at_its_limit() {
    let fixture = answered(Style::default(), &['A']);
    let first = request_of(ASK, Style::default(), &['A']).len();
    let script = format!("head -c {first} >/dev/null; cat \"$1\"; cat >\"$1.rest\"");
    // the limit is the bytes of the answer's set, so the store is at
    // its limit once it holds the answer
    let limit = styled(CELL, Style::default(), &['A']).len() as u64;
    let mut s = StandIn::limited("bold-full", &script, &fixture, limit);
    s.helper.asks_bold_early = true;
    let now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    s.until(now, |h| !h.busy());
    assert_eq!(s.helper.failed(), None);
    assert!(held_in(&s.helper, Style::default(), 'A'));
    assert_eq!(crate::deferred::lock(&s.helper.store).bytes(), limit);
    assert!(s.helper.asks.is_empty(), "no bold ask waits");
    assert!(!s.helper.busy());
}

// The console asks for bold printable ASCII once at a cell. A later
// ask at the cell whose answer evicts those bold glyphs, which the
// screen never showed, sends no second bold ask for them, so the early
// ask never churns the store
//
// References:
// - ADR-1200 D18, note of 2026-11-25: the console decides this once
//   for each cell, and its glyphs count in eviction as any glyphs do.
#[test]
fn the_bold_ask_goes_out_once_at_a_cell() {
    let first = request_of(ASK, Style::default(), &['A']).len();
    let bold = request_of(ASK, BOLD, &bold_ascii()).len();
    let second = request_of(ASK, Style::default(), &['B']).len();
    let script = format!(
        "head -c {first} >/dev/null; cat \"$1\"; head -c {bold} >/dev/null; cat \"$1.b\"; head -c {second} >/dev/null; cat \"$1.c\"; cat >\"$1.rest\""
    );
    // the store holds the first answer and the bold one, so the third
    // answer evicts what was shown least recently: the bold glyphs
    let limit = (styled(CELL, Style::default(), &['A']).len()
        + styled(CELL, BOLD, &bold_ascii()).len()) as u64;
    let mut s = StandIn::limited(
        "bold-once",
        &script,
        &answered(Style::default(), &['A']),
        limit,
    );
    s.helper.asks_bold_early = true;
    std::fs::write(s.dir.join("answer.b"), answered(BOLD, &bold_ascii())).unwrap();
    std::fs::write(s.dir.join("answer.c"), answered(Style::default(), &['B'])).unwrap();
    let now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    assert_eq!(s.run(now).len(), 2);
    assert!(held_in(&s.helper, BOLD, '!'));
    crate::deferred::lock(&s.helper.store).touch(AT, &shown(&['A']));
    s.helper.ask(CELL, Band::NONE, &shown(&['B']), now);
    s.until(now, |h| !h.busy());
    assert_eq!(s.helper.failed(), None);
    assert!(held_in(&s.helper, Style::default(), 'A'));
    assert!(held_in(&s.helper, Style::default(), 'B'));
    assert!(
        bold_ascii()
            .into_iter()
            .any(|c| !held_in(&s.helper, BOLD, c)),
        "the answer evicted bold glyphs"
    );
    assert!(s.helper.asks.is_empty(), "no second bold ask waits");
    assert!(!s.helper.busy());
}

// The first frame's ask goes out first and alone, the fill after it,
// and the bold ask only once the helper has answered the first ask,
// so the bold ask never delays the first frame's glyphs
//
// References:
// - ADR-1200 D18: asks come first, and the helper answers an ask whole
//   ahead of anything else.
// - ADR-1200 D18, note of 2026-11-25: the bold ask follows the cell's
//   first asks.
#[test]
fn the_bold_ask_follows_the_first_frames_ask_and_its_fill() {
    let ask = request_of(ASK, Style::default(), &['A']);
    let fill = request_of(FILL, Style::default(), &['A']);
    let bold = request_of(ASK, BOLD, &bold_ascii());
    let script = format!(
        "head -c {} >\"$1.1\"; head -c {} >\"$1.2\"; cat \"$1\"; head -c {} >\"$1.3\"; cat \"$1.b\"; cat >/dev/null",
        ask.len(),
        fill.len(),
        bold.len()
    );
    let mut s = StandIn::new("bold-order", &script, &answered(Style::default(), &['A']));
    s.helper.asks_bold_early = true;
    let mut rest = answered(BOLD, &bold_ascii());
    rest.extend(filled(CELL));
    std::fs::write(s.dir.join("answer.b"), rest).unwrap();
    let now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    s.helper.fill(CELL, Band::NONE, &['A'], now);
    assert_eq!(s.helper.asks.len(), 1, "the first frame's ask waits alone");
    s.until(now, |h| !h.busy());
    assert_eq!(s.helper.failed(), None);
    let sent = |n: &str| std::fs::read(s.dir.join(format!("answer.{n}"))).unwrap();
    assert_eq!(sent("1"), ask, "the first frame's ask goes out first");
    assert_eq!(sent("2"), fill, "then the fill");
    assert_eq!(sent("3"), bold, "then the bold ask");
    assert!(held_in(&s.helper, BOLD, '~'));
}

/// Whether an ask for bold printable ASCII waits at [`AT`].
fn bold_ask_waits(helper: &Helper) -> bool {
    helper.asks.iter().any(|a| {
        a.at == AT && a.variant == Variant::from(BOLD) && a.want.len() == bold_ascii().len()
    })
}

// The console can decide on the early bold ask while ASKS_MAX asks
// wait, as it does when a search ends. The bound then refuses the ask,
// so the console has not decided: the cell's next answer sends it.
//
// References:
// - ADR-1200 D18, note of 2026-11-25: the console asks once for bold
//   printable ASCII at each cell.
#[test]
fn a_bold_ask_that_the_asks_bound_refuses_goes_out_with_the_next_answer() {
    let mut s = StandIn::new("bold-bound", "cat >/dev/null", b"");
    s.helper.asks_bold_early = true;
    let now = t0();
    let other = Cell {
        width: 16,
        height: 32,
    };
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    for c in ('\u{4e00}'..).take(ASKS_MAX - 1) {
        s.helper.ask(other, Band::NONE, &shown(&[c]), now);
    }
    assert_eq!(s.helper.asks.len(), ASKS_MAX);
    s.helper.ask_bold_early(AT, now);
    assert_eq!(s.helper.asks.len(), ASKS_MAX, "the bound refused the ask");
    assert!(!bold_ask_waits(&s.helper));
    // the helper answers the ask at the cell, which frees room
    s.helper.done(now).unwrap();
    assert!(bold_ask_waits(&s.helper), "the next answer sends it");
}

// An ask whose cell the console dropped before the answer came sends
// no bold ask, and does not make the cell's record again
//
// References:
// - ADR-1200 D18, note of 2026-11-25: the console decides again only
//   for a cell dropped and made again.
#[test]
fn an_ask_answered_after_its_cell_is_dropped_sends_no_bold_ask() {
    let mut s = StandIn::new("bold-dropped", "cat >/dev/null", b"");
    s.helper.asks_bold_early = true;
    let now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    s.helper.retain(&[], now);
    s.helper.done(now).unwrap();
    assert!(s.helper.asks.is_empty(), "no bold ask waits");
    assert!(!s.helper.cells.iter().any(|c| c.at == AT));
}

// With two asks waiting at a cell, the first answer sends no bold
// ask; the second, the cell's last, sends it
//
// References:
// - ADR-1200 D18, note of 2026-11-25: the bold ask follows the cell's
//   first asks.
#[test]
fn the_bold_ask_waits_for_every_ask_at_the_cell() {
    let mut s = StandIn::new("bold-two", "cat >/dev/null", b"");
    s.helper.asks_bold_early = true;
    let now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    s.helper.ask(CELL, Band::NONE, &shown(&['B']), now);
    assert_eq!(s.helper.asks.len(), 2);
    s.helper.done(now).unwrap();
    assert_eq!(s.helper.asks.len(), 1, "the second ask waits alone");
    assert!(!bold_ask_waits(&s.helper));
    s.helper.done(now).unwrap();
    assert!(bold_ask_waits(&s.helper), "the last answer sends it");
}

// ADR-1200 D21: the console refuses a strike in a style other than the
// one it asked for, an ask's or a fill's, and gives the helper up
#[test]
fn a_strike_in_a_style_not_asked_for_gives_the_helper_up() {
    for (name, set, fill, style) in [
        (
            "regular-for-italic",
            styled(CELL, Style::default(), &['A']),
            false,
            ITALIC,
        ),
        (
            "italic-for-regular",
            styled(CELL, ITALIC, &['A']),
            false,
            Style::default(),
        ),
        (
            "italic-fill",
            styled(CELL, ITALIC, &['A']),
            true,
            Style::default(),
        ),
    ] {
        let kind = if fill { FRAME_FILL } else { FRAME_GLYPHS };
        let mut s = StandIn::new(name, ANSWERS, &frame_bytes(kind, &set));
        let now = t0();
        if fill {
            s.helper.fill(CELL, Band::NONE, &[], now);
        } else {
            let ask = Shown {
                cluster_width: Default::default(),
                doubled: None,
                variant: style.into(),
                chars: vec!['A'],
                clusters: Vec::new(),
            };
            s.helper.ask(CELL, Band::NONE, &ask, now);
        }
        assert!(s.run(now).is_empty(), "{name}");
        let why = s.helper.failed().unwrap_or_default();
        assert!(why.contains("style"), "{name}: {why}");
    }
}

/// The superscript variant of the regular style.
const SUPER: Variant = Variant {
    style: Style {
        bold: false,
        italic: false,
    },
    script: Script::Super,
};

/// A set of `chars`, each solid, at `cell`, in one strike of
/// `variant`'s style and script.
fn scripted(cell: Cell, variant: Variant, chars: &[char]) -> Vec<u8> {
    let mut s = StrikeBuilder::new(PixelFormat::R8, cell, variant.style)
        .script(variant.script)
        .compress(false);
    let g = s.glyph(
        Width::Narrow,
        vec![200; (cell.width * cell.height) as usize],
    );
    for &c in chars {
        s.map(c, g);
    }
    SetBuilder::new("t").strike(s).write().unwrap()
}

// ADR-1202 D11: superscript text is asked for in an ask of its own
// that names the script after the style, and its glyphs go into the
// store as superscript, where the normal glyphs do not find them
// covers: sgr.script
#[test]
fn a_superscript_ask_names_its_script_and_its_glyphs_are_held_in_it() {
    let ask = encode(
        &frit("source=builtin:unscii-8\n"),
        None,
        (ASK, CELL, Band::NONE, (SUPER, None), ClusterWidth::Double),
        (&[0x41], &[]),
        &[],
    );
    // the script's field follows the style's, after the magic, the
    // kind, the frit's name and text, the cell and the band: 1 for
    // superscript
    let field = 8 + 4 + 4 + 1 + 4 + "source=builtin:unscii-8\n".len() + 4 * 5;
    assert_eq!(&ask[field..field + 4], &1u32.to_le_bytes(), "the script");
    let mut fixture = frame_bytes(FRAME_GLYPHS, &scripted(CELL, SUPER, &['A']));
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    let script = format!(
        "head -c {} >\"$1.ask\"; cat \"$1\"; cat >/dev/null",
        ask.len()
    );
    let mut s = StandIn::new("superscript", &script, &fixture);
    let now = t0();
    let sup = Shown {
        cluster_width: Default::default(),
        doubled: None,
        variant: SUPER,
        chars: vec!['A'],
        clusters: Vec::new(),
    };
    s.helper.ask(CELL, Band::NONE, &sup, now);
    let answers = s.run(now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(answers.len(), 1);
    assert_eq!(std::fs::read(s.dir.join("answer.ask")).unwrap(), ask);
    let held = |variant: Variant| {
        s.helper
            .cells
            .iter()
            .any(|c| (c.at, c.variant) == (AT, variant) && c.held.has(0x41))
    };
    assert!(held(SUPER), "held as superscript");
    assert!(!held(Variant::default()), "and not as normal");
    let store = crate::deferred::lock(&s.helper.store);
    assert!(store.holds_variant(AT, SUPER, &['A']), "the store's key");
    assert!(!store.holds_variant(AT, Variant::default(), &['A']));
    drop(store);
    // asked again in superscript, it is held; normal, it is not
    s.helper.ask(CELL, Band::NONE, &sup, now);
    assert!(!s.helper.busy());
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    assert!(s.helper.busy());
}

/// A shown character `A` at the double size `doubled`, regular.
fn doubled_a(doubled: Option<Doubled>) -> Shown {
    Shown {
        doubled,
        chars: vec!['A'],
        ..Shown::default()
    }
}

// issue #61: a line of double size asks for its characters in an ask of
// its own, whose line size field follows the cluster width: 1 for
// double width, 2 for double height. The helper's answer at the cell
// the size gives goes into the store at that size, apart from the
// glyphs of the normal size, and the face draws it as the doubled
// glyph
#[test]
fn a_double_ask_names_its_line_size_and_its_glyphs_are_held_at_it() {
    for (doubled, code) in [(Doubled::Width, 1u32), (Doubled::Height, 2)] {
        let ask = encode(
            &frit("source=builtin:unscii-8\n"),
            None,
            (
                ASK,
                CELL,
                Band::NONE,
                (Variant::default(), Some(doubled)),
                ClusterWidth::Double,
            ),
            (&[0x41], &[]),
            &[],
        );
        // after the magic, the kind, the frit's name and text, the
        // cell, the band, the style, the script and the cluster width
        let field = 8 + 4 + 4 + 1 + 4 + "source=builtin:unscii-8\n".len() + 4 * 7;
        assert_eq!(&ask[field..field + 4], &code.to_le_bytes(), "{doubled:?}");
        let drawn = doubled.cell(CELL).unwrap();
        let mut fixture = frame_bytes(FRAME_GLYPHS, &set(drawn, &['A']));
        fixture.extend(frame_bytes(FRAME_DONE, &[]));
        let script = format!(
            "head -c {} >\"$1.ask\"; cat \"$1\"; cat >/dev/null",
            ask.len()
        );
        let mut s = StandIn::new(&format!("doubled-{code}"), &script, &fixture);
        let now = t0();
        s.helper
            .ask(CELL, Band::NONE, &doubled_a(Some(doubled)), now);
        let answers = s.run(now);
        assert_eq!(s.helper.failed(), None, "{doubled:?}");
        assert_eq!(answers.len(), 1, "{doubled:?}");
        assert_eq!(answers[0].at, AT, "{doubled:?}: the answer's cell");
        assert_eq!(std::fs::read(s.dir.join("answer.ask")).unwrap(), ask);
        let store = crate::deferred::lock(&s.helper.store);
        assert!(!store.holds_variant(AT, Variant::default(), &['A']));
        drop(store);
        let face = crate::deferred::DeferredFace::new(
            Box::new(vtium_font::Unifont::new(16)),
            CELL,
            16,
            Arc::clone(&s.helper.store),
        );
        let g = face.doubled(&['A'], Variant::default(), doubled).unwrap();
        assert_eq!((g.width(), g.height()), (drawn.width, drawn.height));
        assert!(g.pixels().iter().all(|&p| p == 255), "{doubled:?}");
        // asked again at that size, it is held; at the normal size, it
        // is not
        s.helper
            .ask(CELL, Band::NONE, &doubled_a(Some(doubled)), now);
        assert!(!s.helper.busy(), "{doubled:?}");
        s.helper.ask(CELL, Band::NONE, &doubled_a(None), now);
        assert!(s.helper.busy(), "{doubled:?}");
    }
}

// issue #61: a line of double size asks for its clusters at its size
// too, so a letter with its marks, an emoji sequence or a syllable's
// piece draws from the helper's glyph at that size
#[test]
fn a_double_ask_names_its_clusters() {
    let key: Text = vec!['A', '\u{301}'];
    let ask = encode(
        &frit("source=builtin:unscii-8\n"),
        None,
        (
            ASK,
            CELL,
            Band::NONE,
            (Variant::default(), Some(Doubled::Height)),
            ClusterWidth::Double,
        ),
        (&[], std::slice::from_ref(&key)),
        &[],
    );
    let fixture = frame_bytes(FRAME_DONE, &[]);
    let script = format!(
        "head -c {} >\"$1.ask\"; cat \"$1\"; cat >/dev/null",
        ask.len()
    );
    let mut s = StandIn::new("doubled-cluster", &script, &fixture);
    let now = t0();
    let shown = Shown {
        doubled: Some(Doubled::Height),
        clusters: vec![key],
        ..Shown::default()
    };
    s.helper.ask(CELL, Band::NONE, &shown, now);
    assert!(s.helper.busy(), "the cluster is asked for");
    s.run(now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(std::fs::read(s.dir.join("answer.ask")).unwrap(), ask);
}

// the console refuses a double ask's answer at any cell but the one its
// size gives, the normal cell among them, and gives the helper up; it
// asks no superscript at a double size, and no double size whose cell
// passes the largest cell
#[test]
fn a_double_asks_answer_at_another_cell_gives_the_helper_up() {
    let wide = Doubled::Width.cell(CELL).unwrap();
    let tall = Doubled::Height.cell(CELL).unwrap();
    for (name, cell, doubled) in [
        ("normal-for-wide", CELL, Doubled::Width),
        ("tall-for-wide", tall, Doubled::Width),
        ("wide-for-tall", wide, Doubled::Height),
    ] {
        let mut s = StandIn::new(
            name,
            ANSWERS,
            &frame_bytes(FRAME_GLYPHS, &set(cell, &['A'])),
        );
        let now = t0();
        s.helper
            .ask(CELL, Band::NONE, &doubled_a(Some(doubled)), now);
        assert!(s.run(now).is_empty(), "{name}");
        let why = s.helper.failed().unwrap_or_default();
        assert!(why.contains("not asked"), "{name}: {why}");
    }
    let mut s = StandIn::new("doubled-unasked", ANSWERS, &[]);
    let now = t0();
    let sup = Shown {
        variant: SUPER,
        ..doubled_a(Some(Doubled::Height))
    };
    s.helper.ask(CELL, Band::NONE, &sup, now);
    let huge = Cell {
        width: 8,
        height: vtium_font::CELL_SIDE_MAX / 2 + 1,
    };
    s.helper
        .ask(huge, Band::NONE, &doubled_a(Some(Doubled::Height)), now);
    assert!(!s.helper.busy(), "nothing is asked");
}

// ADR-1202 D11: the console refuses a strike in another script than
// the one it asked for, an ask's or a fill's, and gives the helper up
#[test]
fn a_strike_in_a_script_not_asked_for_gives_the_helper_up() {
    let sub = Variant {
        script: Script::Sub,
        ..SUPER
    };
    for (name, set, fill, variant) in [
        (
            "normal-for-super",
            scripted(CELL, Variant::default(), &['A']),
            false,
            SUPER,
        ),
        ("sub-for-super", scripted(CELL, sub, &['A']), false, SUPER),
        (
            "super-for-normal",
            scripted(CELL, SUPER, &['A']),
            false,
            Variant::default(),
        ),
        (
            "super-fill",
            scripted(CELL, SUPER, &['A']),
            true,
            Variant::default(),
        ),
    ] {
        let kind = if fill { FRAME_FILL } else { FRAME_GLYPHS };
        let mut s = StandIn::new(name, ANSWERS, &frame_bytes(kind, &set));
        let now = t0();
        if fill {
            s.helper.fill(CELL, Band::NONE, &[], now);
        } else {
            let ask = Shown {
                cluster_width: Default::default(),
                doubled: None,
                variant,
                chars: vec!['A'],
                clusters: Vec::new(),
            };
            s.helper.ask(CELL, Band::NONE, &ask, now);
        }
        assert!(s.run(now).is_empty(), "{name}");
        let why = s.helper.failed().unwrap_or_default();
        assert!(why.contains("superscript or subscript"), "{name}: {why}");
    }
}

#[test]
fn a_frit_is_read_with_the_name_of_its_file() {
    let dir = Scratch::new("helper-read");
    let path = dir.join("ter-v16n.vtfrit");
    std::fs::write(&path, "source=kbd:ter-v16n\n").unwrap();
    let trust = crate::font::trust().below(&dir);
    let read = FritText::read(&path, &dir, &trust);
    let gone = FritText::read(&dir.join("gone.vtfrit"), &dir, &trust);
    assert_eq!(
        read,
        Ok(FritText {
            name: "ter-v16n".to_owned(),
            text: "source=kbd:ter-v16n\n".to_owned(),
        })
    );
    assert!(gone.unwrap_err().contains("gone.vtfrit"));
}

#[test]
fn an_asks_glyphs_are_taken_and_then_served_from_the_store() {
    let mut fixture = frame_bytes(FRAME_GLYPHS, &set(CELL, &['A']));
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    let mut s = StandIn::new("ask", ANSWERS, &fixture);
    let now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A', 'B']), now);
    let answers = s.run(now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(answers.len(), 1);
    assert_eq!(answers[0].at, AT);
    assert!(answers[0].face.has_glyph('A'));
    assert!(s.held('A'));
    // A is held and B is known lacking: nothing more is asked
    s.helper.ask(CELL, Band::NONE, &shown(&['A', 'B']), now);
    assert!(!s.helper.busy());
}

const ACUTE: [char; 2] = ['e', '\u{301}'];
const GRAVE: [char; 2] = ['e', '\u{300}'];

/// A set at `cell` mapping each of `clusters` to a solid glyph.
fn cluster_set(cell: Cell, clusters: &[&[char]]) -> Vec<u8> {
    let mut s = StrikeBuilder::new(PixelFormat::R8, cell, Style::default()).compress(false);
    let g = s.glyph(
        Width::Narrow,
        vec![255; (cell.width * cell.height) as usize],
    );
    for &k in clusters {
        s.cluster(k, g);
    }
    SetBuilder::new("t").strike(s).write().unwrap()
}

/// The screen showing `clusters` whole, and each of their characters.
fn clustered(clusters: &[&[char]]) -> Shown {
    let mut chars: Vec<char> = clusters.iter().flat_map(|k| k.iter().copied()).collect();
    chars.sort_unstable();
    chars.dedup();
    Shown {
        cluster_width: Default::default(),
        doubled: None,
        variant: Variant::default(),
        chars,
        clusters: clusters.iter().map(|k| k.to_vec()).collect(),
    }
}

// ADR-1200 D20, D21: a cluster is asked for by its whole text, its
// glyph held by that text, and one the helper did not shape is known
// lacking: neither is asked for again
#[test]
fn a_cluster_is_asked_for_whole_and_held_by_its_text() {
    let mut fixture = frame_bytes(FRAME_GLYPHS, &cluster_set(CELL, &[&ACUTE]));
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    let mut s = StandIn::new("cluster", ANSWERS, &fixture);
    let now = t0();
    let only_clusters = Shown {
        cluster_width: Default::default(),
        doubled: None,
        variant: Variant::default(),
        chars: Vec::new(),
        clusters: vec![ACUTE.to_vec(), GRAVE.to_vec()],
    };
    s.helper.ask(CELL, Band::NONE, &only_clusters, now);
    let answers = s.run(now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(answers.len(), 1);
    // the cells showing e or U+0301 redraw
    assert_eq!(answers[0].chars, ['e', '\u{301}']);
    let state = s.helper.cells.iter().find(|c| c.at == AT).unwrap();
    assert!(state.clusters.held.contains(ACUTE.as_slice()));
    assert!(state.clusters.absent.contains(GRAVE.as_slice()));
    assert!(!state.held.has(u32::from('e')));
    assert!(
        crate::deferred::lock(&s.helper.store)
            .fresh(
                (AT, Variant::default()),
                Arc::new(Set::from_bytes(cluster_set(CELL, &[&ACUTE])).unwrap()),
                &[],
                1
            )
            .unwrap()
            .texts()
            .next()
            .is_none(),
        "the store holds it by its text"
    );
    s.helper.ask(CELL, Band::NONE, &only_clusters, now);
    assert!(!s.helper.busy());
}

// ADR-1200 D22 step 2: an arrived é redraws the cells whose cluster
// holds e or U+0301, as e U+0301 now draws é's glyph
#[test]
fn an_arrived_codepoint_redraws_the_clusters_it_composes_from() {
    let mut fixture = frame_bytes(FRAME_GLYPHS, &set(CELL, &['\u{e9}', 'A']));
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    let mut s = StandIn::new("decomposed", ANSWERS, &fixture);
    let now = t0();
    s.helper
        .ask(CELL, Band::NONE, &shown(&['\u{e9}', 'A']), now);
    let answers = s.run(now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(answers.len(), 1);
    assert_eq!(answers[0].chars, ['A', 'e', '\u{e9}', '\u{301}']);
}

// ADR-1200 D22 step 3: where the font has no glyph for a mark of a
// cluster asked for, as for superscript alef on lam with shadda and
// fatha, the helper sends a console built with right-to-left text the
// part it draws, the cluster without that
// mark; the console holds the part and knows the cluster lacking. A
// part that leaves out a letter is what was not asked
// covers: text.overhang
#[cfg(feature = "bidi")]
#[test]
fn the_part_of_a_cluster_the_font_draws_is_taken() {
    let whole = ['\u{644}', '\u{651}', '\u{64e}', '\u{670}'];
    let part = ['\u{644}', '\u{651}', '\u{64e}'];
    let mut fixture = frame_bytes(FRAME_GLYPHS, &cluster_set(CELL, &[&part]));
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    let mut s = StandIn::new("cluster-part", ANSWERS, &fixture);
    let now = t0();
    s.helper.ask(CELL, Band::NONE, &clustered(&[&whole]), now);
    let answers = s.run(now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(answers.len(), 1);
    let state = s.helper.cells.iter().find(|c| c.at == AT).unwrap();
    assert!(state.clusters.held.contains(part.as_slice()));
    assert!(state.clusters.absent.contains(whole.as_slice()));
    let marks = ['\u{651}', '\u{64e}', '\u{670}'];
    let mut s = StandIn::new(
        "cluster-no-letter",
        ANSWERS,
        &frame_bytes(FRAME_GLYPHS, &cluster_set(CELL, &[&marks])),
    );
    s.helper.ask(CELL, Band::NONE, &clustered(&[&whole]), now);
    let _ = s.run(now);
    let why = s.helper.failed().unwrap();
    assert!(why.contains("cluster the ask did not name"), "{why}");
}

// ADR-1004 D6: an arrived presentation form redraws the cells showing
// the letters it presents, since a joined letter the font has no
// joined form for draws it; beh's initial form redraws beh, and
// lam-alef's ligature lam and alef
// covers: text.bidi
#[cfg(feature = "bidi")]
#[test]
fn an_arrived_presentation_form_redraws_its_letters() {
    let forms = ['\u{fe91}', '\u{fefb}'];
    let mut fixture = frame_bytes(FRAME_GLYPHS, &set(CELL, &forms));
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    let mut s = StandIn::new("presented", ANSWERS, &fixture);
    let now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&forms), now);
    let answers = s.run(now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(answers.len(), 1);
    assert_eq!(
        answers[0].chars,
        ['\u{627}', '\u{628}', '\u{644}', '\u{fe91}', '\u{fefb}']
    );
}

// ADR-1200 D15: a cluster the ask did not name, or one in a fill, is
// what was not asked, and gives the helper up
#[test]
fn a_cluster_not_asked_for_gives_the_helper_up() {
    let mut s = StandIn::new(
        "cluster-unasked",
        ANSWERS,
        &frame_bytes(FRAME_GLYPHS, &cluster_set(CELL, &[&GRAVE])),
    );
    let now = t0();
    s.helper.ask(CELL, Band::NONE, &clustered(&[&ACUTE]), now);
    let _ = s.run(now);
    let why = s.helper.failed().unwrap();
    assert!(why.contains("cluster the ask did not name"), "{why}");
    let (why, _) = given_up(
        "cluster-fill",
        ANSWERS,
        &frame_bytes(FRAME_FILL, &cluster_set(CELL, &[&ACUTE])),
        true,
    );
    assert!(why.contains("fill carries a cluster"), "{why}");
}

// A sources query's answer is the helper's lines as they came, after
// the frit's warnings, which are logged as any frit's are. A frit the
// helper draws nothing from is an error that says why.
//
// References:
// - ADR-1204 D7: check-config names each missing font.
#[test]
fn a_sources_query_answers_with_the_helpers_lines() {
    let lines = "missing\t1\tfont:Gone\tno font of family Gone\nolder\t2\tfont:Old\t2.242";
    let mut fixture = frame_bytes(FRAME_WARNING, b"t needs the font Gone");
    fixture.extend(frame_bytes(FRAME_SOURCES, lines.as_bytes()));
    let mut s = StandIn::new("sources", ANSWERS, &fixture);
    let clock = t0;
    assert_eq!(
        s.helper.sources(Duration::from_secs(5), &clock).as_deref(),
        Ok(lines)
    );
    assert!(s.helper.warned.contains("t needs the font Gone"));
    let fixture = frame_bytes(FRAME_DRAWS_NOTHING, b"no source is on this machine");
    let mut s = StandIn::new("sources-nothing", ANSWERS, &fixture);
    assert_eq!(
        s.helper.sources(Duration::from_secs(5), &clock),
        Err("no source is on this machine".to_owned())
    );
}

// `vtium check-config` names each missing font itself, from the sources
// query's answer, so a helper it starts logs none of the frit's
// warnings, which the console would log, and the answer still comes.
//
// References:
// - ADR-1204 D7: check-config names each missing font.
#[test]
fn a_quiet_helper_logs_no_warning_and_still_answers() {
    let lines = "missing\t1\tfont:Gone\tno font of family Gone";
    let mut fixture = frame_bytes(FRAME_WARNING, b"t needs the font Gone");
    fixture.extend(frame_bytes(FRAME_SOURCES, lines.as_bytes()));
    let mut s = StandIn::new("sources-quiet", ANSWERS, &fixture);
    s.helper.quiet();
    assert_eq!(
        s.helper.sources(Duration::from_secs(5), &t0).as_deref(),
        Ok(lines)
    );
    assert!(s.helper.warned.is_empty(), "{:?}", s.helper.warned);
}

// the frit's warnings are logged once each, at most WARNINGS_MAX a
// boot, and none gives the helper up
#[test]
fn a_frits_warnings_are_kept_to_their_bound() {
    let mut fixture = Vec::new();
    for i in 0..WARNINGS_MAX + 4 {
        fixture.extend(frame_bytes(
            FRAME_WARNING,
            format!("t:{i}: a key that does nothing").as_bytes(),
        ));
    }
    fixture.extend(frame_bytes(FRAME_WARNING, b"t:0: a key that does nothing"));
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    let mut s = StandIn::new("warnings", ANSWERS, &fixture);
    let now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    let _ = s.run(now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(s.helper.warned.len(), WARNINGS_MAX);
    // the log says once that it holds no more of them
    assert!(s.helper.warnings_dropped);
    assert!(s.helper.warned.contains("t:0: a key that does nothing"));
    assert!(
        !s.helper
            .warned
            .contains(&format!("t:{WARNINGS_MAX}: a key that does nothing"))
    );
}

// a helper that runs without its landlock domain is logged once a
// boot, with whether its fallback filter stands in for the domain, and
// its report answers nothing, so the answer it leads still lands; a
// helper whose sandbox is whole is not logged (ADR-1200 D15, the
// maintainer, 2026-11-13)
#[test]
fn a_helper_without_its_landlock_domain_is_logged_once() {
    let report = |parts: u32| frame_bytes(FRAME_SANDBOX, &parts.to_le_bytes());
    let run = |name: &str, reports: &[u32]| {
        let mut fixture = Vec::new();
        for &parts in reports {
            fixture.extend(report(parts));
        }
        fixture.extend(frame_bytes(FRAME_GLYPHS, &set(CELL, &['A'])));
        fixture.extend(frame_bytes(FRAME_DONE, &[]));
        let mut s = StandIn::new(name, ANSWERS, &fixture);
        let now = t0();
        s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
        let answers = s.run(now);
        assert_eq!(s.helper.failed(), None);
        assert!(!answers.is_empty(), "the answer after the report lands");
        s.helper.sandbox_noted.clone()
    };
    let whole = SANDBOX_DOMAIN | SANDBOX_SOCKETS;
    assert_eq!(run("sandbox-whole", &[whole]), None);
    let fallback = SANDBOX_FALLBACK | SANDBOX_SOCKETS;
    let noted = run("sandbox-fallback", &[fallback, whole]).unwrap();
    assert!(
        noted.starts_with("The font helper runs without its landlock domain"),
        "{noted}"
    );
    assert!(
        noted.contains("A seccomp filter refuses in its place"),
        "{noted}"
    );
    let noted = run("sandbox-none", &[0]).unwrap();
    assert!(
        noted.contains("No filter takes the domain's place"),
        "{noted}"
    );
    assert!(noted.contains("without its seccomp filters"), "{noted}");
    assert_eq!(sandbox_notice(whole), None);
}

/// A sandbox report with `parts` in force, the seccomp filters missing
/// for `seccomp` and the domain for `domain`, as the helper sends it.
fn report(parts: u32, seccomp: u32, domain: u32) -> u32 {
    parts | seccomp << SANDBOX_SECCOMP_SHIFT | domain << SANDBOX_DOMAIN_SHIFT
}

// The console's line names each part of the helper's sandbox that is
// not in force and the reason the helper gives for it, telling apart
// the setting, an architecture with no filter, the three kernels
// without seccomp filters and the three without landlock, and says
// whether the fallback filter stands in for the domain.
//
// References:
// - ADR-1200 D15, "Nothing on screen, and the log says why": the
//   reasons the line tells apart.
#[test]
fn the_sandbox_line_names_each_missing_part_and_its_reason() {
    let whole = SANDBOX_DOMAIN | SANDBOX_SOCKETS;
    assert_eq!(sandbox_notice(report(whole, 0, 0)), None);
    let off = sandbox_notice(report(0, 1, 1)).unwrap();
    assert_eq!(
        off,
        "The font helper runs without its seccomp filters and its landlock domain, because the setting helper-sandbox=off turns them off. A font that took over the font helper could open network connections and change any file that the font helper's account may change."
    );
    let seccomp = |why: u32| sandbox_notice(report(SANDBOX_DOMAIN, why, 0)).unwrap();
    assert_eq!(
        seccomp(2),
        "The font helper runs without its seccomp filters, because vtium has no seccomp filter for this machine's architecture."
    );
    assert_eq!(
        seccomp(3),
        "The font helper runs without its seccomp filters, because this kernel is new enough to offer seccomp but was built without it."
    );
    assert_eq!(
        seccomp(4),
        "The font helper runs without its seccomp filters, because this kernel was built with seccomp but without seccomp filters."
    );
    assert_eq!(
        seccomp(5),
        "The font helper runs without its seccomp filters, because this kernel is older than Linux 3.8, too old for vtium's seccomp filters."
    );
    let domain = |parts: u32, why: u32| sandbox_notice(report(parts, 0, why)).unwrap();
    let fallback = SANDBOX_FALLBACK | SANDBOX_SOCKETS;
    assert_eq!(
        domain(fallback, 2),
        "The font helper runs without its landlock domain, because this kernel is older than Linux 5.13, which added landlock. A seccomp filter refuses in its place the file, signal and tracing calls that the domain refuses."
    );
    assert_eq!(
        domain(fallback, 3),
        "The font helper runs without its landlock domain, because this kernel was built without landlock. A seccomp filter refuses in its place the file, signal and tracing calls that the domain refuses."
    );
    assert_eq!(
        domain(fallback, 4),
        "The font helper runs without its landlock domain, because this kernel did not turn landlock on at boot, as when the boot's lsm= list leaves it out. A seccomp filter refuses in its place the file, signal and tracing calls that the domain refuses."
    );
    // a number vtium does not know, 0 included, is given as a number
    // and never named as a reason
    assert_eq!(
        seccomp(9),
        "The font helper runs without its seccomp filters. The font helper's report gives the reason as number 9, which vtium does not know."
    );
    assert_eq!(
        seccomp(0),
        "The font helper runs without its seccomp filters. The font helper's report gives the reason as number 0, which vtium does not know."
    );
    assert_eq!(
        domain(fallback, 7),
        "The font helper runs without its landlock domain. The font helper's report gives the reason as number 7, which vtium does not know. A seccomp filter refuses in its place the file, signal and tracing calls that the domain refuses."
    );
    assert_eq!(
        domain(fallback, 0),
        "The font helper runs without its landlock domain. The font helper's report gives the reason as number 0, which vtium does not know. A seccomp filter refuses in its place the file, signal and tracing calls that the domain refuses."
    );
    // a kernel without seccomp filters and without landlock: neither
    // filter stands in for the domain
    let neither = sandbox_notice(report(0, 3, 3)).unwrap();
    assert_eq!(
        neither,
        "The font helper runs without its landlock domain, because this kernel was built without landlock. No filter takes the domain's place, so a font that took over the font helper could change any file that the font helper's account may change. The font helper runs without its seccomp filters, because this kernel is new enough to offer seccomp but was built without it."
    );
}

// The sandbox line is logged once a boot, however many helpers start:
// a second helper, for another font set or user, or a helper started
// again, logs nothing more.
//
// References:
// - ADR-1200 D15, "Nothing on screen, and the log says why": one line
//   once a boot.
#[test]
fn the_sandbox_line_is_logged_once_a_boot_for_every_helper() {
    let boot = AtomicBool::new(false);
    let line = sandbox_notice(report(0, 1, 1)).unwrap();
    assert!(log_sandbox_once(&boot, &line), "the first helper's line");
    assert!(!log_sandbox_once(&boot, &line), "a second helper's line");
    assert!(!log_sandbox_once(&boot, &line), "a helper started again");
}

// helper-sandbox=off reaches each helper the console starts as
// `--sandbox off`, and with the key on or unset the console passes
// no option, so the helper sandboxes itself in full; a value the key
// cannot take is refused as a bad value, and the sandbox stays on.
//
// References:
// - ADR-1200 D15, "A setting turns the sandbox off": vtium tells each
//   helper on its command line.
// - ADR-1510 D2: a bad value of a known key is refused.
#[test]
fn helper_sandbox_off_reaches_the_helper_on_its_command_line() {
    use std::collections::BTreeMap;
    let load = |text: &str| {
        let files: BTreeMap<PathBuf, Vec<u8>> = [(
            PathBuf::from("/etc/vtium/vtium.conf"),
            text.as_bytes().to_vec(),
        )]
        .into_iter()
        .collect();
        crate::config::load(&[], &mut |p: &Path| Ok(files.get(p).cloned()))
    };
    let off = load("helper-sandbox=off\n");
    assert!(off.reports.is_empty(), "{:?}", off.reports);
    assert!(!sandbox_on(&off.config));
    assert_eq!(sandbox_args(&off.config), ["--sandbox", "off"]);
    for text in ["helper-sandbox=on\n", ""] {
        let on = load(text);
        assert!(sandbox_on(&on.config), "{text:?}");
        assert!(sandbox_args(&on.config).is_empty(), "{text:?}");
    }
    for bad in ["yes", "no", "1", "OFF", "disabled"] {
        let l = load(&format!("helper-sandbox={bad}\n"));
        assert!(
            matches!(
                l.reports.iter().map(|r| &r.problem).collect::<Vec<_>>()[..],
                [crate::config::Problem::NotAChoice {
                    key: "helper-sandbox",
                    ..
                }]
            ),
            "{bad}: {:?}",
            l.reports
        );
        assert!(sandbox_on(&l.config), "{bad}");
    }
}

// a sandbox report of any length but four bytes is refused, and the
// helper is given up
#[test]
fn a_sandbox_report_of_another_length_gives_the_helper_up() {
    let mut fixture = frame_bytes(FRAME_SANDBOX, &[1, 0]);
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    let (why, _) = given_up("sandbox-short", ANSWERS, &fixture, false);
    assert!(why.contains("a sandbox report without its parts"), "{why}");
    let mut fixture = frame_bytes(FRAME_SANDBOX, &[1, 0, 0, 0, 0]);
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    let (why, _) = given_up("sandbox-long", ANSWERS, &fixture, false);
    assert!(why.contains("past its kind's 4"), "{why}");
}

// a warning's control characters are escaped before the log, and a
// warning answers nothing: an ask the helper only warns on is still
// past due at ANSWER_MS, and vtium starts the helper again to find the
// source it hung on
#[test]
fn a_warning_is_escaped_and_answers_nothing() {
    let mut fixture = Vec::new();
    for _ in 0..3 {
        fixture.extend(frame_bytes(FRAME_WARNING, b"t:1: bad\x1b[2J\nline"));
    }
    let mut s = StandIn::new("warn-only", ANSWERS, &fixture);
    let now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    for _ in 0..3 {
        s.woke.recv_timeout(Duration::from_secs(10)).unwrap();
    }
    let late = now + Duration::from_millis(ANSWER_MS + 1);
    let _ = s.helper.poll(late);
    assert_eq!(
        s.helper.warned.iter().collect::<Vec<_>>(),
        ["t:1: bad\\u{1b}[2J\\nline"]
    );
    assert!(s.helper.search.is_some(), "the ask was past due");
    assert_eq!(s.helper.failed(), None);
}

// a cluster past CLUSTER_SCALARS_MAX is never asked for, as the helper
// refuses it; nor is a lone codepoint taken for a cluster
#[test]
fn a_cluster_past_its_bound_is_not_asked_for() {
    let mut helper = Helper::new(
        // never started: nothing is asked
        Launcher::open(Command {
            program: stand_in_shell().to_path_buf(),
            args: Vec::new(),
            account: None,
        })
        .unwrap(),
        Arc::new(|| {}),
        t0(),
        frit("source=builtin:unscii-8\n"),
        None,
        Shared::default(),
        1 << 20,
    );
    let long = vec!['e'; vtium_font::CLUSTER_SCALARS_MAX as usize + 1];
    let shown = Shown {
        cluster_width: Default::default(),
        doubled: None,
        variant: Variant::default(),
        chars: Vec::new(),
        clusters: vec![long, vec!['e']],
    };
    helper.ask(CELL, Band::NONE, &shown, t0());
    assert!(helper.asks.is_empty());
    assert_eq!(helper.failed(), None);
}

// a program that writes new clusters faster than the helper answers
// grows nothing without bound: at most ASKS_MAX asks wait, and at most
// CLUSTERS_WAITING_MAX clusters a cell; what is left is asked for on a
// later change
#[test]
fn asks_and_the_clusters_they_wait_on_are_bounded() {
    let mut s = StandIn::new("flood-asks", "cat >/dev/null", b"");
    let now = t0();
    let cluster = |i: u32, j: u32| {
        vec![
            char::from_u32(0x4e00 + i).unwrap(),
            char::from_u32(0x300 + j).unwrap(),
        ]
    };
    for i in 0..1000 {
        let shown = Shown {
            cluster_width: Default::default(),
            doubled: None,
            variant: Variant::default(),
            chars: Vec::new(),
            clusters: (0..10).map(|j| cluster(i, j)).collect(),
        };
        s.helper.ask(CELL, Band::NONE, &shown, now);
        assert!(s.helper.asks.len() <= ASKS_MAX);
    }
    assert_eq!(s.helper.asks.len(), ASKS_MAX);
    let waiting = |h: &Helper, cell: Cell| {
        h.cells
            .iter()
            .find(|c| c.at.cell == cell)
            .map_or(0, |c| c.clusters.pending.len())
    };
    assert_eq!(waiting(&s.helper, CELL), ASKS_MAX * 10);
    // one ask of more clusters than may wait at a cell
    let other = Cell {
        width: 16,
        height: 32,
    };
    let mut t = StandIn::new("flood-one", "cat >/dev/null", b"");
    let shown = Shown {
        cluster_width: Default::default(),
        doubled: None,
        variant: Variant::default(),
        chars: Vec::new(),
        clusters: (0..5000).map(|i| cluster(i / 10, i % 10)).collect(),
    };
    t.helper.ask(other, Band::NONE, &shown, now);
    assert_eq!(waiting(&t.helper, other), CLUSTERS_WAITING_MAX);
    t.helper.ask(other, Band::NONE, &shown, now);
    assert_eq!(t.helper.asks.len(), 1, "nothing more fits");
}

// a codepoint past an ask's FIRST_MAX is not asked for, so its answer
// does not take it for one the frit lacks; a later change asks for it
#[test]
fn a_codepoint_past_an_asks_bound_is_not_taken_for_lacking() {
    let mut s = StandIn::new("first-max", ANSWERS, &frame_bytes(FRAME_DONE, &[]));
    let now = t0();
    let n = u32::try_from(FIRST_MAX).unwrap() + 5;
    let chars: Vec<char> = (0..n)
        .map(|i| char::from_u32(0x4e00 + i).unwrap())
        .collect();
    s.helper.ask(CELL, Band::NONE, &shown(&chars), now);
    let _ = s.run(now);
    assert_eq!(s.helper.failed(), None);
    let state = s.helper.cells.iter().find(|c| c.at == AT).unwrap();
    let last = u32::from(chars[FIRST_MAX - 1]);
    assert!(state.absent.has(last));
    for &c in &chars[FIRST_MAX..] {
        let c = u32::from(c);
        assert!(!state.absent.has(c) && !state.pending.has(c), "{c:x}");
    }
}

// the record of clusters the frit lacks is bounded: past it, it is
// emptied, and a cluster shown again is asked for again
#[test]
fn the_record_of_clusters_lacked_is_bounded() {
    let mut s = StandIn::new("lacked", ANSWERS, &frame_bytes(FRAME_DONE, &[]));
    let now = t0();
    {
        let state = s.helper.state(AT, Variant::default()).unwrap();
        for i in 0..u32::try_from(CLUSTERS_LACKED_MAX).unwrap() {
            let c = char::from_u32(0x300 + i % 0x70).unwrap();
            let d = char::from_u32(0x4e00 + i / 0x70).unwrap();
            state.clusters.absent.insert(vec![d, c]);
        }
        assert_eq!(state.clusters.absent.len(), CLUSTERS_LACKED_MAX);
    }
    s.helper.ask(CELL, Band::NONE, &clustered(&[&ACUTE]), now);
    let _ = s.run(now);
    assert_eq!(s.helper.failed(), None);
    let state = s.helper.cells.iter().find(|c| c.at == AT).unwrap();
    assert_eq!(
        state.clusters.absent.iter().collect::<Vec<_>>(),
        [&ACUTE.to_vec()]
    );
}

#[test]
fn a_fill_fills_until_its_end() {
    let mut fixture = frame_bytes(FRAME_FILL, &set(CELL, &['A']));
    fixture.extend(frame_bytes(FRAME_FILL, &set(CELL, &['B'])));
    fixture.extend(filled(CELL));
    let mut s = StandIn::new("fill", ANSWERS, &fixture);
    let now = t0();
    s.helper.fill(CELL, Band::NONE, &[], now);
    assert_eq!(s.run(now).len(), 2);
    assert_eq!(s.helper.failed(), None);
    assert!(s.held('A') && s.held('B'));
    assert!(!s.helper.busy());
}

// ADR-1302 D13: two fills at one cell size with two bands are two
// fills: each frame and each fill's end names its band, and the glyphs
// go to that band's record and store alone
#[test]
fn one_cell_size_with_two_bands_fills_twice() {
    // more rows above than below, so reading the two the wrong way
    // round names a band no fill was asked at
    let band = Band { above: 2, below: 1 };
    let spaced = SpacedCell { cell: CELL, band };
    let mut fixture = spaced_frame(FRAME_FILL, band, &set(CELL, &['A']));
    fixture.extend(frame_bytes(FRAME_FILL, &set(CELL, &['B'])));
    fixture.extend(spaced_end(FRAME_FILLED, spaced));
    fixture.extend(filled(CELL));
    let mut s = StandIn::new("bands", ANSWERS, &fixture);
    let now = t0();
    s.helper.fill(CELL, Band::NONE, &[], now);
    s.helper.fill(CELL, band, &[], now);
    assert_eq!(s.run(now).len(), 2);
    assert_eq!(s.helper.failed(), None);
    let held = |at: SpacedCell, c: char| {
        s.helper
            .cells
            .iter()
            .any(|r| r.at == at && r.held.has(u32::from(c)))
    };
    assert!(held(spaced, 'A') && !held(AT, 'A'), "A at the band alone");
    assert!(held(AT, 'B') && !held(spaced, 'B'), "B with no band alone");
    let store = crate::deferred::lock(&s.helper.store);
    assert!(store.holds(spaced) && store.holds(AT));
    drop(store);
    assert!(!s.helper.busy(), "both fills ended");
}

#[test]
fn a_helper_that_exits_at_its_idle_interval_is_started_again() {
    let mut fixture = frame_bytes(FRAME_GLYPHS, &set(CELL, &['A']));
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    let mut s = StandIn::new("idle", "head -c 1 >/dev/null; cat \"$1\"", &fixture);
    let now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    assert_eq!(s.run(now).len(), 1);
    s.until_gone(now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(s.helper.restarts(), 0);
    // the cell asked for again, as after its glyphs were dropped; with
    // no helper running, the drop starts none
    s.helper.retain(&[], now);
    assert!(s.helper.running.is_none());
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    assert_eq!(s.run(now).len(), 1);
    assert_eq!(s.helper.failed(), None);
}

#[test]
fn a_helper_that_closes_before_it_exits_is_no_failure() {
    // its output closes, then it lingers before exiting 0: reaped, not
    // ended
    let mut fixture = frame_bytes(FRAME_GLYPHS, &set(CELL, &['A']));
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    let mut s = StandIn::new(
        "linger",
        "head -c 1 >/dev/null; cat \"$1\"; exec 1>&-; sleep 0.3; exit 0",
        &fixture,
    );
    let now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    assert_eq!(s.run(now).len(), 1);
    s.until_gone(now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(s.helper.restarts(), 0);
}

fn given_up(name: &str, script: &str, fixture: &[u8], fill: bool) -> (String, u32) {
    let mut s = StandIn::new(name, script, fixture);
    let now = t0();
    if fill {
        s.helper.fill(CELL, Band::NONE, &[], now);
    } else {
        s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    }
    let _ = s.run(now);
    let why = s.helper.failed().unwrap().to_owned();
    // given up: nothing more is asked
    s.helper.ask(CELL, Band::NONE, &shown(&['Z']), now);
    assert!(!s.helper.busy());
    (why, s.helper.restarts())
}

#[test]
fn a_helper_is_held_to_the_account_it_was_started_as() {
    let status = |uid: &str, gid: &str, groups: &str| {
        format!("Name:\tvtium-config\nUid:\t{uid}\nGid:\t{gid}\nGroups:\t{groups}\n")
    };
    let nobody = "65534\t65534\t65534\t65534";
    let account = (65534, 65534);
    assert_eq!(holds_account(&status(nobody, nobody, ""), account), Ok(()));
    // a supplementary group the drop left, as when the console may not
    // change its groups
    for groups in ["0", "998", "1000 998 "] {
        let why = holds_account(&status(nobody, nobody, groups), account).unwrap_err();
        assert!(why.contains("supplementary groups"), "{why}");
    }
    // any of the four uids or gids another account's, root's saved id
    // included
    for (uid, gid) in [
        ("0\t0\t0\t0", nobody),
        ("65534\t65534\t0\t65534", nobody),
        (nobody, "65534\t0\t65534\t65534"),
        (nobody, "65534\t65534\t65534\t0"),
        ("65534\t65534", nobody),
    ] {
        let why = holds_account(&status(uid, gid, ""), account).unwrap_err();
        assert!(
            why.contains("not as 65534") || why.contains("not with 65534"),
            "{why}"
        );
    }
    for broken in [String::new(), format!("Uid:\t{nobody}\nGid:\t{nobody}\n")] {
        let why = holds_account(&broken, account).unwrap_err();
        assert!(why.contains("do not read"), "{why}");
    }
}

#[test]
fn a_helper_that_keeps_supplementary_groups_is_refused_at_its_start() {
    // a console that is not root asks std for its own account: std's
    // drop of the supplementary groups fails as it would for a root
    // without the capability, std goes on, and the helper starts holding
    // the console's groups, which the console must refuse
    let own = std::fs::read_to_string("/proc/self/status").unwrap();
    let groups = status_ids(&own, "Groups:").unwrap();
    let me = rustix::process::geteuid().as_raw();
    if me == 0 || groups.is_empty() {
        // as root the drop works, and with no group there is none to keep
        return;
    }
    let gid = rustix::process::getegid().as_raw();
    let command = Command {
        program: stand_in_shell().to_path_buf(),
        args: vec!["-c".into(), "cat >/dev/null".into()],
        account: Some((me, gid)),
    };
    let (tx, woke) = mpsc::channel::<()>();
    let tx = std::sync::Mutex::new(tx);
    let mut helper = Helper::new(
        Launcher::open(command).unwrap(),
        Arc::new(move || {
            let _ = tx.lock().map(|t| t.send(()));
        }),
        t0(),
        frit("source=builtin:unscii-8\n"),
        None,
        Shared::default(),
        64 << 20,
    );
    helper.ask(CELL, Band::NONE, &shown(&['A']), t0());
    let why = helper.failed().unwrap();
    assert!(why.contains("supplementary groups"), "{why}");
    assert!(helper.running.is_none());
    assert!(!helper.busy());
    drop(woke);
}

/// A pointer ask for the text shape at cells 16 pixels tall.
fn text_pointer_ask() -> crate::pointer_file::Ask {
    crate::pointer_file::Ask {
        svg: true,
        shape: vtium_term::PointerShape::Text,
        size: None,
        heights: vec![16],
        bytes: Arc::new(b"<svg/>".to_vec()),
    }
}

/// A picture frame of the text shape, 2 × 2 white pixels, for cells
/// `size` pixels tall.
fn text_picture(size: u32) -> Vec<u8> {
    let text = vtium_term::PointerShape::ALL
        .iter()
        .position(|&s| s == vtium_term::PointerShape::Text)
        .unwrap();
    let mut p = Vec::new();
    for v in [len32(text), size, 2, 2, 0, 0] {
        p.extend_from_slice(&v.to_le_bytes());
    }
    p.extend_from_slice(&[255; 16]);
    raw_frame(FRAME_POINTER, &p)
}

/// A stand-in helper for pointer asks alone, which answers with
/// `fixture`.
fn pointer_stand_in(name: &str, script: &str, fixture: &[u8]) -> StandIn {
    let mut s = StandIn::new(name, script, fixture);
    s.helper.pointers_only = true;
    s
}

// A pointer ask goes through the helper client that asks for glyphs:
// the helper starts for it, and the picture and the done frame that
// end the ask come back as its answer. The helper is then idle.
#[test]
fn a_pointer_ask_is_answered_through_the_helper_client() {
    let mut fixture = text_picture(16);
    fixture.extend(raw_frame(FRAME_DONE, &[]));
    let mut s = pointer_stand_in("pointer-ask", ANSWERS, &fixture);
    let now = t0();
    s.helper.pointer(&text_pointer_ask(), now);
    assert!(s.helper.busy());
    assert!(s.helper.pointer_answer().is_none());
    let _ = s.run(now);
    assert!(matches!(
        s.helper.pointer_answer(),
        Some(Ok(crate::pointer_file::Answer::Pictures(p))) if p.len() == 1
    ));
    assert!(!s.helper.busy());
    assert!(s.helper.failed().is_none());
}

// A helper that sends more pictures than the pointer ask can bring is
// ended and given up for the boot, so a hostile file costs one helper:
// the ask's answer says why, and a later pointer ask gets the same
// reason at once, with no helper started.
#[test]
fn a_helper_that_sends_more_pictures_than_asked_is_given_up() {
    let mut fixture = text_picture(16);
    fixture.extend(text_picture(16));
    fixture.extend(raw_frame(FRAME_DONE, &[]));
    let mut s = pointer_stand_in("pointer-flood", ANSWERS, &fixture);
    let now = t0();
    s.helper.pointer(&text_pointer_ask(), now);
    let _ = s.run(now);
    let why = s.helper.pointer_answer().unwrap().unwrap_err();
    assert!(why.contains("more than 1 pictures"), "{why}");
    assert!(s.helper.failed().is_some());
    s.helper.pointer(&text_pointer_ask(), now);
    assert_eq!(s.helper.pointer_answer(), Some(Err(why)));
    assert!(!s.helper.busy());
    assert!(s.helper.running.is_none());
}

// A helper that does not answer a pointer ask in time is ended, and the
// ask's answer says how long vtium waited. The stand-in reads every
// request and answers none, and it keeps running until the client ends
// it. It runs cat in its own place rather than in the background,
// because a shell gives a background command /dev/null for its
// standard input: that cat would end at once and the shell with it,
// and the client would then see a helper that exited, not one that
// gave no answer.
#[test]
fn a_pointer_ask_left_unanswered_gets_why() {
    let mut s = pointer_stand_in("pointer-silent", "exec cat >/dev/null", b"");
    let now = t0();
    s.helper.pointer(&text_pointer_ask(), now);
    let _ = s.helper.poll(now);
    assert_eq!(s.helper.pointer_answer(), None);
    let late = now + Duration::from_millis(ANSWER_MS + 1);
    assert_eq!(
        s.helper.deadline(),
        Some(now + Duration::from_millis(ANSWER_MS))
    );
    let _ = s.helper.poll(late);
    assert_eq!(
        s.helper.pointer_answer(),
        Some(Err("the font helper gave no answer for 10 s".to_owned()))
    );
    assert!(!s.helper.busy());
}

/// A frit of three sources, which the stand-ins of
/// [`one_source_fails`] tell apart by the bit the console's
/// leave-out request gives each.
const THREE: &str = "source=builtin:unscii-8\nsource=kbd:second\n# a comment\n  source = builtin:unifont # the third\nscript-source=kbd:small\n";

/// A stand-in helper that reads the sources the console leaves out as
/// the real helper does, from a leave-out request at the head of its
/// input, all 16 bits of it, none when the input starts with another
/// request, and refuses a request that names a source past the
/// sixteenth, as the real helper does. It logs the bits of each start
/// to `$1.masks`. With every source of [`THREE`] left out it reads
/// each request whole and answers each as the real helper answers a
/// frit that draws nothing: a failed frame for an ask or a fill, and
/// no cell for a cell query. Otherwise it does `fails` when the
/// shell's test `fails_when` holds, with the bits in `$b`, how many
/// starts left out no source in `$n` and how many starts there have
/// been in `$k`, and else it answers with the fixture.
fn source_stand_in(fails_when: &str, fails: &str) -> String {
    format!(
        "r=$(head -c 16 | od -An -tx1 | tr -d ' \\n')
case \"$r\" in
56545653524551410c000000*) m=${{r#????????????????????????}} ;;
*) m=00000000 ;;
esac
case \"$m\" in ????0000) ;; *) cat \"$1.refused\"; exit 1 ;; esac
lo=${{m%??????}}; hi=${{m#??}}; hi=${{hi%????}}
b=$((0x$hi$lo))
echo \"$b\" >>\"$1.masks\"
n=$(grep -c '^0$' \"$1.masks\")
k=$(grep -c '' \"$1.masks\")
u32() {{ head -c 4 | od -An -tu4 | tr -d ' \\n'; }}
skip() {{ head -c \"$1\" >/dev/null; }}
if [ $((b & 7)) = 7 ]; then
  while q=$(head -c 8) && [ -n \"$q\" ]; do
    kind=$(u32)
    skip \"$(u32)\"; skip \"$(u32)\"; skip 36
    skip $((4 * $(u32)))
    c=$(u32); while [ \"$c\" -gt 0 ]; do skip $((4 * $(u32))); c=$((c - 1)); done
    skip $((8 * $(u32))); skip 4
    if [ \"$kind\" = 4 ]; then cat \"$1.nocell\"; else cat \"$1.none\"; fi
  done
  exit 0
fi
if {fails_when}; then {fails}; fi
cat \"$1\"; exec cat >/dev/null"
    )
}

/// The shell's test that one of the sources whose bits `guilty` sets is
/// read.
fn reads(guilty: u32) -> String {
    format!("[ $(( (b & {guilty}) != {guilty} )) = 1 ]")
}

/// A stand-in of [`source_stand_in`] for the frit [`THREE`], whose
/// fixture answers an ask for `A`, with `extra` after it.
fn sourced(name: &str, fails_when: &str, fails: &str, extra: &[u8]) -> StandIn {
    let mut fixture = frame_bytes(FRAME_GLYPHS, &set(CELL, &['A']));
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    fixture.extend_from_slice(extra);
    sourced_with(name, fails_when, fails, &fixture)
}

/// [`sourced`] whose fixture is `fixture`.
fn sourced_with(name: &str, fails_when: &str, fails: &str, fixture: &[u8]) -> StandIn {
    let s = StandIn::of(
        name,
        &source_stand_in(fails_when, fails),
        fixture,
        (64 << 20, THREE),
    );
    let answer = s.dir.join("answer");
    let at = |ext: &str| PathBuf::from(format!("{}.{ext}", answer.display()));
    std::fs::write(
        at("none"),
        raw_frame(FRAME_FAILED, b"t: no source of the font set is left"),
    )
    .unwrap();
    std::fs::write(
        at("refused"),
        raw_frame(FRAME_FAILED, b"a malformed request"),
    )
    .unwrap();
    std::fs::write(
        at("nocell"),
        raw_frame(FRAME_NO_OWN_CELL, b"t: no source of the font set is left"),
    )
    .unwrap();
    for b in 0..8 {
        std::fs::write(
            at(&format!("hung.{b}")),
            raw_frame(FRAME_WARNING, format!("hung {b}").as_bytes()),
        )
        .unwrap();
    }
    s
}

/// Polls as the loop does, each time the reader wakes it, until the
/// helper has nothing outstanding or is given up, and moves the clock
/// `now` to the helper's deadline each time a hung helper says it
/// hangs. It returns the answers taken.
fn drive(s: &mut StandIn, now: &mut Instant) -> Vec<Answer> {
    let mut answers = Vec::new();
    let mut warned = s.helper.warned.len();
    for _ in 0..400 {
        answers.extend(s.helper.poll(*now));
        if s.helper.warned.len() > warned {
            // a hung helper said so: its answer is past due
            warned = s.helper.warned.len();
            *now = s.helper.deadline().unwrap();
            answers.extend(s.helper.poll(*now));
        }
        if !s.helper.busy() || s.helper.failed().is_some() {
            break;
        }
        // the wait only bounds a test that would otherwise hang
        s.woke.recv_timeout(Duration::from_secs(10)).unwrap();
    }
    answers
}

/// The bits each helper of `s` started with, in order.
fn masks(s: &StandIn) -> Vec<u32> {
    std::fs::read_to_string(s.dir.join("answer.masks"))
        .unwrap()
        .lines()
        .map(|l| l.parse().unwrap())
        .collect()
}

/// Asks for `A` of the frit [`THREE`] from a stand-in helper that does
/// `fails` whenever it reads one of the sources whose bits `guilty`
/// sets: crash, hang or reach its CPU limit. It returns the stand-in,
/// the answers taken and the bits each helper started with.
fn one_source_fails(name: &str, guilty: u32, fails: &str) -> (StandIn, Vec<Answer>, Vec<u32>) {
    let mut s = sourced(name, &reads(guilty), fails, &[]);
    let mut now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    let answers = drive(&mut s, &mut now);
    let masks = masks(&s);
    (s, answers, masks)
}

// ADR-1200 D14: a source that crashes the helper fails alone. vtium
// starts the helper again with no source, then with the first, then
// with the first two, which crash it again, so the second source is
// the one. The helper then draws from the first and the third, and
// every later helper leaves the second out.
#[test]
fn a_source_that_crashes_the_helper_is_left_out_and_the_others_draw() {
    crate::getty::tests::logged();
    let (mut s, answers, masks) = one_source_fails("source-crash", 0b010, "kill -SEGV $$");
    assert_eq!(s.helper.failed(), None);
    assert_eq!(answers.len(), 1);
    assert!(s.held('A'));
    assert_eq!(masks, [0, 0b111, 0b110, 0b100, 0b010]);
    // the line says what vtium saw, the failure as vtium added the font
    let blamed: Vec<String> = crate::getty::tests::logged()
        .into_iter()
        .filter(|l| l.contains("leaves out source="))
        .collect();
    assert_eq!(
        blamed,
        [
            "the font set t leaves out source=kbd:second until vtium starts again, because the font helper was ended by signal 11 when vtium added that font; the font set's other fonts go on drawing"
        ]
    );
    // a helper started afresh still leaves the second source out
    let launcher = s.helper.launcher.clone();
    s.helper.begin_again(&launcher);
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), t0());
    assert_eq!(s.run(t0()).len(), 1);
    let masks = std::fs::read_to_string(s.dir.join("answer.masks")).unwrap();
    assert_eq!(masks.lines().last(), Some("2"));
    assert_eq!(s.helper.failed(), None);
}

// a source the helper hangs on fails alone, as one it crashes on does
#[test]
fn a_source_the_helper_hangs_on_is_left_out_and_the_others_draw() {
    let (s, answers, masks) = one_source_fails(
        "source-hang",
        0b010,
        "cat \"$1.hung.$b\"; exec cat >/dev/null",
    );
    assert_eq!(s.helper.failed(), None);
    assert_eq!(answers.len(), 1);
    assert!(s.held('A'));
    assert_eq!(masks, [0, 0b111, 0b110, 0b100, 0b010]);
}

// a source that makes the helper reach its CPU limit fails alone
#[test]
fn a_source_that_spends_the_helpers_cpu_is_left_out_and_the_others_draw() {
    let (s, answers, masks) = one_source_fails("source-cpu", 0b010, "kill -XCPU $$");
    assert_eq!(s.helper.failed(), None);
    assert_eq!(answers.len(), 1);
    assert!(s.held('A'));
    assert_eq!(masks, [0, 0b111, 0b110, 0b100, 0b010]);
}

// a cell query made while vtium looks for the failed source waits for
// the helper started after the search, and its answer comes late
#[test]
fn a_cell_query_during_the_search_is_answered_late() {
    let own = cell_frame(FRAME_OWN_CELL, 8, 16);
    let mut s = sourced("source-cell", &reads(0b010), "kill -SEGV $$", &own);
    let mut now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    s.until(now, |h| h.search.is_some());
    let no = s
        .helper
        .own_cell(16, Duration::from_secs(10), &t0)
        .unwrap_err();
    assert!(no.late, "{no}");
    assert!(!no.from_frit, "{no}");
    let _ = drive(&mut s, &mut now);
    s.until(now, |h| !h.late.is_empty() || !h.kept.is_empty());
    s.until(now, |h| !h.kept.is_empty());
    assert_eq!(s.helper.failed(), None);
    assert_eq!(
        s.helper.late_cells(),
        [(16, Ok(OwnCell::Bitmap(CELL)))],
        "the query was asked again of the helper started after the search"
    );
}

// a helper that also fails with no source to read is the helper's own
// failure: vtium gives it up at once, starts it no more, and blames no
// source
#[test]
fn a_helper_that_fails_with_no_font_is_given_up_at_once() {
    for (name, script, said) in [
        ("source-none-dies", "kill -SEGV $$", "signal 11"),
        (
            "source-none-exits",
            "cat >/dev/null & exit 3",
            "exit status: 3",
        ),
    ] {
        let mut s = StandIn::of(name, script, b"", (64 << 20, THREE));
        let mut now = t0();
        s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
        let _ = drive(&mut s, &mut now);
        let why = s.helper.failed().unwrap();
        assert!(why.contains(said), "{why}");
        assert!(why.ends_with("before it read any font"), "{why}");
        assert_eq!(s.helper.restarts(), 1, "the search used one chance");
        assert_eq!(s.helper.left_out(), 0, "no source was blamed");
        // given up: nothing more is asked
        s.helper.ask(CELL, Band::NONE, &shown(&['Z']), now);
        assert!(!s.helper.busy());
    }
}

// vtium numbers the sources as the helper's frit reader does: each
// source= line in order, a comment and spaces left out, and no other
// key; and the helper reads its leave-out request as vtium writes it
#[test]
fn the_sources_are_numbered_as_the_helper_numbers_them() {
    let text = format!("name=t\n{THREE}layer=sym\nsource=kbd:lat9w-16 # in a layer\nlayer=\n");
    let parsed = vtium_config::frit::Frit::parse_spliced(
        text.as_bytes(),
        Path::new("t"),
        "t",
        Path::new("/nonexistent"),
    )
    .unwrap();
    let spelled: Vec<String> = parsed.sources.iter().map(|s| s.spelling.clone()).collect();
    assert_eq!(source_lines(&text), spelled);
    assert_eq!(
        spelled,
        [
            "builtin:unscii-8",
            "kbd:second",
            "builtin:unifont",
            "kbd:lat9w-16"
        ]
    );
    for mask in [0b010, 0b1111] {
        assert_eq!(
            vtium_config::serve::read_request(&mut &leave_out_request(mask)[..]),
            Ok(Some(vtium_config::serve::Request::LeaveOut(mask)))
        );
    }
}

// with every source crashing the helper, each fails in turn, never
// read again once it has, and the font set then falls back to the
// built-in font
#[test]
fn a_font_set_whose_every_source_crashes_the_helper_falls_back() {
    let (s, answers, masks) = one_source_fails("source-all", 0b111, "kill -SEGV $$");
    assert!(answers.is_empty());
    assert_eq!(masks, [0, 7, 6, 1, 7, 5, 3, 7, 3]);
    let why = s.helper.failed().unwrap();
    assert!(why.contains("every font"), "{why}");
    assert!(!s.helper.busy());
}

// a crash while an ask and a fill are under way fails one source: each
// start of the search is sent both, and the start with no source
// fails each of them, one failed frame for each, as the real helper
// does
#[test]
fn a_crash_with_an_ask_and_a_fill_under_way_fails_one_source() {
    let mut fixture = frame_bytes(FRAME_GLYPHS, &set(CELL, &['A']));
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    fixture.extend(frame_bytes(FRAME_FILL, &set(CELL, &['B'])));
    fixture.extend(filled(CELL));
    let mut s = sourced_with("source-fill", &reads(0b010), "kill -SEGV $$", &fixture);
    let mut now = t0();
    s.helper.fill(CELL, Band::NONE, &['B'], now);
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    let answers = drive(&mut s, &mut now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(masks(&s), [0, 0b111, 0b110, 0b100, 0b010]);
    assert_eq!(s.helper.left_out(), 0b010);
    assert_eq!(answers.len(), 2);
    assert!(s.held('A') && s.held('B'));
    assert!(!s.helper.busy(), "the fill is done");
}

/// Polls `s` as the loop does, moving the clock 9 s after each poll that
/// took one of the helper's frames, so no answer is ever past ANSWER_MS,
/// until the helper has nothing outstanding or is given up. The clock
/// follows the frames taken, not the reader's wake-ups: a poll can take a
/// frame whose wake-up comes after it, and that wake-up then brings no
/// frame.
fn drive_slowly(s: &mut StandIn, now: &mut Instant) {
    for _ in 0..200 {
        let taken = s.helper.taken;
        let _ = s.helper.poll(*now);
        if s.helper.failed().is_some() || !s.helper.busy() {
            break;
        }
        if s.helper.taken > taken {
            *now += Duration::from_secs(9);
        } else {
            let _ = s.woke.recv_timeout(Duration::from_secs(10));
        }
    }
}

// ADR-1200 D14 bounds each start of the search as it bounds a helper.
// A start that keeps sending glyphs for its ask, each within
// ANSWER_MS, and never ends the ask, fails at REQUEST_MS. A start that
// keeps sending fill glyphs it was never asked for gives no answer at
// all, so it fails at ANSWER_MS. Either way the source it added is
// left out, and the line says which bound it missed.
#[test]
fn a_search_start_that_never_ends_its_ask_fails_at_its_bound() {
    for (kind, said) in [
        (FRAME_GLYPHS, "left an ask unanswered for 60 s"),
        (FRAME_FILL, "gave no answer for 10 s"),
    ] {
        crate::getty::tests::logged();
        let fails = "if [ $n = 1 ] && [ $b = 0 ]; then kill -SEGV $$; fi; while :; do cat \"$1.a\"; sleep 0.1; done";
        let mut s = sourced(&format!("source-stream-{kind}"), &reads(0b010), fails, &[]);
        std::fs::write(
            s.dir.join("answer.a"),
            frame_bytes(kind, &set(CELL, &['A'])),
        )
        .unwrap();
        let mut now = t0();
        s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
        drive_slowly(&mut s, &mut now);
        assert_eq!(s.helper.failed(), None);
        assert!(s.helper.search.is_none(), "the search ended");
        assert_eq!(s.helper.left_out(), 0b010);
        assert!(s.held('A'));
        assert_eq!(masks(&s), [0, 0b111, 0b110, 0b100, 0b010]);
        let blamed: Vec<String> = crate::getty::tests::logged()
            .into_iter()
            .filter(|l| l.contains("leaves out source="))
            .collect();
        assert!(
            blamed.first().is_some_and(|l| l.contains(said)),
            "{blamed:?}"
        );
    }
}

// a fill has no bound on its whole length, in a search as in normal
// running: a start of the search whose ask is done and whose fill
// goes on past REQUEST_MS, each batch within ANSWER_MS, has not failed
#[test]
fn a_fill_that_outlasts_the_asks_bound_in_a_search_is_no_failure() {
    let fill: Vec<char> = "BCDEFGHI".chars().collect();
    let fails = "if [ $k = 1 ]; then kill -SEGV $$; fi; cat \"$1\"; for i in 0 1 2 3 4 5 6 7; do cat \"$1.f$i\"; sleep 0.1; done; cat \"$1.end\"; exec cat >/dev/null";
    let mut s = sourced("source-long-fill", "true", fails, &[]);
    for (i, c) in fill.iter().enumerate() {
        std::fs::write(
            s.dir.join(format!("answer.f{i}")),
            frame_bytes(FRAME_FILL, &set(CELL, &[*c])),
        )
        .unwrap();
    }
    std::fs::write(s.dir.join("answer.end"), filled(CELL)).unwrap();
    let mut now = t0();
    s.helper.fill(CELL, Band::NONE, &fill, now);
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    drive_slowly(&mut s, &mut now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(s.helper.left_out(), 0, "no source was blamed");
    assert_eq!(masks(&s), [0, 7, 6, 4, 0]);
    assert!(s.held('A') && fill.iter().all(|&c| s.held(c)));
}

// a start of the search that sends what is not a frame fails as a
// crash does, and the source it added is left out; so does the last
// start, which reads every source
#[test]
fn a_malformed_answer_during_a_search_fails_that_start() {
    for (guilty, want) in [
        (0b010, vec![0, 7, 6, 4, 2]),
        (0b100, vec![0, 7, 6, 4, 0, 4]),
    ] {
        let fails = "if [ $k = 1 ]; then kill -SEGV $$; fi; printf garbage!; exec cat >/dev/null";
        let mut s = sourced(
            &format!("source-garbage-{guilty}"),
            &reads(guilty),
            fails,
            &[],
        );
        let mut now = t0();
        s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
        let answers = drive(&mut s, &mut now);
        assert_eq!(s.helper.failed(), None, "{guilty:#b}");
        assert_eq!(s.helper.left_out(), guilty, "{guilty:#b}");
        assert_eq!(answers.len(), 1, "{guilty:#b}");
        assert_eq!(masks(&s), want, "{guilty:#b}");
    }
}

// a helper that the kernel ends in the middle of a frame, here for its
// CPU time after 12 bytes of one, has ended its output: its exit
// status decides, so the source it was reading is left out
#[test]
fn a_helper_ended_in_the_middle_of_a_frame_fails_one_source() {
    let (s, answers, masks) =
        one_source_fails("source-cut", 0b010, "head -c 12 \"$1\"; kill -XCPU $$");
    assert_eq!(s.helper.failed(), None);
    assert_eq!(s.helper.left_out(), 0b010);
    assert_eq!(answers.len(), 1);
    assert_eq!(masks, [0, 0b111, 0b110, 0b100, 0b010]);
}

// a source that crashes the helper while it answers the font set's
// cell query, the first request at start for a font set with no cell=
// line, starts a search as any crash does; the query comes back late,
// and the helper started after the search answers it
#[test]
fn a_crash_during_a_cell_query_starts_a_search_and_the_cell_comes_late() {
    let own = cell_frame(FRAME_OWN_CELL, 8, 16);
    let mut s = sourced("source-cell-crash", &reads(0b010), "kill -SEGV $$", &own);
    let mut now = t0();
    let first = s
        .helper
        .own_cell(16, Duration::from_secs(10), &t0)
        .unwrap_err();
    assert!(first.late, "{first}");
    s.until(now, |h| h.search.is_some() || h.failed().is_some());
    assert!(s.helper.search.is_some(), "the crash started a search");
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    let answers = drive(&mut s, &mut now);
    s.until(now, |h| !h.kept.is_empty());
    assert_eq!(s.helper.failed(), None);
    assert_eq!(answers.len(), 1);
    assert_eq!(s.helper.left_out(), 0b010);
    assert_eq!(s.helper.restarts(), 1, "the search used one chance");
    assert_eq!(s.helper.late_cells(), [(16, Ok(OwnCell::Bitmap(CELL)))]);
    assert_eq!(masks(&s), [0, 0b111, 0b110, 0b100, 0b010]);
}

// ADR-1200 D15 gives the helper three chances a boot, which the
// searches and the plain restarts share: an exit with no work uses
/// Why vtium gives a font set up when the helper crashes after the
/// last of its chances: the helper is the subject, and the count
/// belongs to the chances.
const GIVEN_UP_AFTER_A_CRASH: &str = "the font helper was ended by signal 11 after vtium had used all 3 chances that it gives the font helper each boot";

// one, each search uses one, and the failure after the third gives the
// font set up, though a source is still in use
#[test]
fn the_searches_and_the_plain_restarts_share_three_chances() {
    let fails = "if [ $k = 1 ]; then cat \"$1\"; exit 3; fi; kill -SEGV $$";
    let mut s = sourced("source-chances", "true", fails, &[]);
    let mut now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    assert_eq!(drive(&mut s, &mut now).len(), 1);
    s.until_gone(now);
    assert_eq!(
        s.helper.restarts(),
        1,
        "the exit with no work used a chance"
    );
    s.helper.ask(CELL, Band::NONE, &shown(&['B']), now);
    let _ = drive(&mut s, &mut now);
    assert_eq!(masks(&s), [0, 0, 7, 6, 1, 7, 5, 3]);
    assert_eq!(s.helper.left_out(), 0b011);
    assert_eq!(s.helper.restarts(), RESTARTS_MAX);
    let why = s.helper.failed().unwrap();
    assert_eq!(why, GIVEN_UP_AFTER_A_CRASH);
}

// when the last start of a search, which reads every source still in
// use, ends its work, no source fails on its own: vtium keeps that
// helper and takes its answers, and starts no other
#[test]
fn the_last_start_of_a_search_is_kept_when_it_succeeds() {
    let mut s = sourced("source-last", "[ $k = 1 ]", "kill -SEGV $$", &[]);
    let mut now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    let answers = drive(&mut s, &mut now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(answers.len(), 1);
    assert!(s.held('A'));
    assert_eq!(masks(&s), [0, 7, 6, 4, 0]);
    assert_eq!(s.helper.restarts(), 1, "the search alone used a chance");
    assert_eq!(s.helper.left_out(), 0);
    assert!(s.helper.search.is_none() && s.helper.suspect.is_none());
    assert!(s.helper.running.is_some(), "the last start runs on");
}

// a failure that no source causes again uses one chance for its
// search, and nothing more; the fourth such failure gives the font set
// up
#[test]
fn a_failure_no_source_causes_again_uses_one_chance() {
    // it crashes on each start that leaves out no source and is the
    // first since the last search: the first start of each round
    let fails = "if [ $((n % 2)) = 1 ]; then kill -SEGV $$; fi; cat \"$1\"; exit 0";
    let done = frame_bytes(FRAME_DONE, &[]);
    let mut s = sourced_with("source-flake", "[ \"$b\" = 0 ]", fails, &done);
    let mut now = t0();
    for (round, c) in (0u32..).zip(['A', 'B', 'C', 'D']) {
        s.helper.ask(CELL, Band::NONE, &shown(&[c]), now);
        let _ = drive(&mut s, &mut now);
        s.until_gone(now);
        if round < 3 {
            assert_eq!(s.helper.failed(), None, "round {round}");
            assert_eq!(s.helper.restarts(), round + 1, "round {round}");
        }
    }
    let mut one = [0, 7, 6, 4, 0].repeat(3);
    one.push(0);
    assert_eq!(masks(&s), one);
    assert_eq!(s.helper.restarts(), RESTARTS_MAX);
    assert_eq!(s.helper.left_out(), 0, "no source was blamed");
    let why = s.helper.failed().unwrap();
    assert_eq!(why, GIVEN_UP_AFTER_A_CRASH);
}

// ADR-1200 D15 bounds the chances of a frit for the whole boot, not
// for one helper's life. The frit's helper is dropped when no head
// draws the frit, and a program can bring the frit back at once with
// two OSC 50 switches. The new helper takes the record of the dropped
// one: its first start leaves out the font that made a helper fail,
// and its chances are those that are left.
#[test]
fn a_helper_made_again_for_the_frit_keeps_the_left_out_font_and_the_chances() {
    let (mut s, _, masks_before) = one_source_fails("source-again", 0b010, "kill -SEGV $$");
    assert_eq!(s.helper.left_out(), 0b010);
    assert_eq!(s.helper.restarts(), 1);
    s.again();
    let mut now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    let _ = drive(&mut s, &mut now);
    assert_eq!(s.helper.failed(), None);
    let mut expected = masks_before;
    expected.push(0b010);
    assert_eq!(
        masks(&s),
        expected,
        "the new helper's first start leaves the font out"
    );
    assert_eq!(s.helper.left_out(), 0b010);
    assert_eq!(s.helper.restarts(), 1, "no chance came back");
}

// vtium keeps each left-out font by its spelling, so a frit whose file
// is read again with its fonts in another order leaves out the same
// font, and not the font that now has its place
#[test]
fn a_left_out_font_is_kept_by_its_spelling() {
    let record = FailureRecord::default();
    record.borrow_mut().left_out.insert("kbd:y".to_owned());
    let mut s = StandIn::of(
        "spelling",
        ANSWERS,
        b"",
        (64 << 20, "source=kbd:x\nsource=kbd:y\n"),
    );
    s.helper.failures = Rc::clone(&record);
    assert_eq!(s.helper.left_out(), 0b10);
    let mut s = StandIn::of(
        "spelling-moved",
        ANSWERS,
        b"",
        (64 << 20, "source = kbd:y # moved\nsource=kbd:x\n"),
    );
    s.helper.failures = record;
    assert_eq!(s.helper.left_out(), 0b01);
}

// the chances a frit has used stay used when its helper is made again:
// two failures that no source causes again use two chances, the helper
// is made again, and the fourth such failure gives the font set up
#[test]
fn the_fourth_failure_across_a_helper_made_again_gives_the_font_set_up() {
    let fails = "if [ $((n % 2)) = 1 ]; then kill -SEGV $$; fi; cat \"$1\"; exit 0";
    let done = frame_bytes(FRAME_DONE, &[]);
    let mut s = sourced_with("source-again-flake", "[ \"$b\" = 0 ]", fails, &done);
    let mut now = t0();
    for (round, c) in (0u32..).zip(['A', 'B', 'C', 'D']) {
        if round == 2 {
            s.again();
        }
        s.helper.ask(CELL, Band::NONE, &shown(&[c]), now);
        let _ = drive(&mut s, &mut now);
        s.until_gone(now);
        if round < 3 {
            assert_eq!(s.helper.failed(), None, "round {round}");
            assert_eq!(s.helper.restarts(), round + 1, "round {round}");
        }
    }
    assert_eq!(s.helper.failed(), Some(GIVEN_UP_AFTER_A_CRASH));
}

// a new login while vtium looks for the failed source ends the search,
// and the cell query that waited for it is asked of the next helper,
// whose answer comes late
#[test]
fn a_cell_query_that_waited_for_a_search_is_asked_after_a_new_login() {
    let own = cell_frame(FRAME_OWN_CELL, 8, 16);
    let mut s = sourced("source-login", "[ $k = 1 ]", "kill -SEGV $$", &own);
    let now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    s.until(now, |h| h.search.is_some());
    assert!(s.helper.own_cell(16, Duration::from_secs(10), &t0).is_err());
    let launcher = s.helper.launcher.clone();
    s.helper.begin_again(&launcher);
    assert_eq!(s.helper.restarts(), 1, "a new login keeps the chances used");
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    let _ = s.run(now);
    s.until(now, |h| !h.kept.is_empty());
    assert_eq!(s.helper.failed(), None);
    assert_eq!(s.helper.late_cells(), [(16, Ok(OwnCell::Bitmap(CELL)))]);
}

// a blame is told to the console once, and the console's cell queries
// asked again go to the helper started without the source, their
// answers to come late
#[test]
fn a_blame_is_told_once_and_the_cells_are_asked_again() {
    let (mut s, _, _) = one_source_fails("source-told", 0b010, "kill -SEGV $$");
    assert!(s.helper.take_blamed());
    assert!(!s.helper.take_blamed(), "told once");
    s.helper.ask_cells_again(&[16, 20], t0());
    assert_eq!(s.helper.late, [16, 20]);
}

// The last start of a search answers a cell query that waited for the
// search, and the console takes that late answer while it waits for a
// new cell query of its own. The last start has then ended the work it
// was sent, so the search ends there: vtium leaves out no source, uses
// no other chance, and keeps the helper, which goes on answering.
#[test]
fn a_late_cell_taken_while_the_console_waits_ends_the_search() {
    crate::getty::tests::logged();
    let mut fixture = cell_frame(FRAME_OWN_CELL, 8, 16);
    fixture.extend(cell_frame(FRAME_OWN_CELL, 8, 16));
    // the first helper crashes on its cell query, and the last start
    // of the search answers half a second late
    let fails = "if [ $k = 1 ]; then kill -SEGV $$; fi; if [ $b = 0 ]; then sleep 0.5; fi";
    let mut s = sourced_with("source-late-own", "true", fails, &fixture);
    let now = t0();
    assert!(s.helper.own_cell(16, Duration::from_secs(10), &t0).is_err());
    s.until(now, |h| h.suspect.is_some() || h.failed().is_some());
    assert!(s.helper.suspect.is_some(), "the last start runs");
    let second = s.helper.own_cell(16, Duration::from_secs(10), &t0);
    assert_eq!(second, Ok(OwnCell::Bitmap(CELL)));
    assert_eq!(s.helper.kept.len(), 1, "the late answer was taken");
    assert!(s.helper.suspect.is_none(), "the search ended");
    let _ = s.helper.poll(now + Duration::from_secs(11));
    assert_eq!(s.helper.failed(), None);
    assert_eq!(s.helper.left_out(), 0, "no source is left out");
    assert_eq!(s.helper.restarts(), 1, "the search alone used a chance");
    assert!(s.helper.search.is_none());
    assert!(s.helper.running.is_some(), "the last start runs on");
    assert_eq!(masks(&s), [0, 7, 6, 4, 0]);
    let kept: Vec<String> = crate::getty::tests::logged()
        .into_iter()
        .filter(|l| l.contains("keeps every font"))
        .collect();
    assert_eq!(kept.len(), 1, "{kept:?}");
}

// The last start of a search is sent an ask after its start, while the
// ask it was sent at its start is still open, and it crashes on the
// later ask. The search never tried that ask, so the crash says
// nothing about the source the search added last. vtium leaves no
// source out for it and begins a new search, which uses a second
// chance, finds that no source fails on its own, and keeps its last
// start.
#[test]
fn a_last_start_that_fails_on_later_work_begins_a_new_search() {
    crate::getty::tests::logged();
    // the fifth start, the last of the first search, reads the ask it
    // is sent at its start, whose size the test writes to `$1.size`,
    // without answering it, and crashes when the next request comes;
    // every start from the sixth answers both asks
    let fails = "if [ $k = 1 ]; then kill -SEGV $$; fi
if [ $k = 5 ]; then
  head -c $(( $(cat \"$1.size\") - 16 )) >/dev/null
  head -c 1 >/dev/null
  kill -SEGV $$
fi
if [ $k -ge 6 ]; then cat \"$1\"; cat \"$1.done\"; exec cat >/dev/null; fi";
    let mut s = sourced("source-later", "true", fails, &[]);
    std::fs::write(s.dir.join("answer.done"), frame_bytes(FRAME_DONE, &[])).unwrap();
    let mut now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    // the size of the ask as vtium sends it to a start; a start that
    // leaves out no source has read 16 bytes of it already, while it
    // looked for a leave-out request
    let a = s.helper.asks.front().unwrap();
    let (want, clusters): (Vec<u32>, Vec<Text>) = (
        a.want.iter().copied().collect(),
        a.clusters.iter().cloned().collect(),
    );
    let what = (ASK, a.at, (a.variant, a.doubled), a.cluster_width);
    let size = s.helper.request(what, (&want, &clusters), &[]).len();
    std::fs::write(s.dir.join("answer.size"), size.to_string()).unwrap();
    s.until(now, |h| h.suspect.is_some() || h.failed().is_some());
    let open = s
        .helper
        .suspect
        .as_ref()
        .map(|x| (x.open.asks, x.open.fills.len()));
    assert_eq!(open, Some((1, 0)), "the last start was sent one ask");
    s.helper.ask(CELL, Band::NONE, &shown(&['B']), now);
    let _ = drive(&mut s, &mut now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(s.helper.left_out(), 0, "no source is left out");
    assert_eq!(s.helper.restarts(), 2, "a second search used a chance");
    assert_eq!(masks(&s), [0, 7, 6, 4, 0, 7, 6, 4, 0]);
    assert!(s.helper.search.is_none() && s.helper.suspect.is_none());
    assert!(s.helper.running.is_some(), "the last start runs on");
    let logged = crate::getty::tests::logged();
    let blamed: Vec<&String> = logged
        .iter()
        .filter(|l| l.contains("leaves out source="))
        .collect();
    assert!(blamed.is_empty(), "{blamed:?}");
    // each search's line counts the chances as the docs do
    let searches: Vec<&str> = logged
        .iter()
        .filter(|l| l.contains("added one at a time"))
        .filter_map(|l| l.rsplit_once(" (").map(|(_, n)| n))
        .collect();
    assert_eq!(
        searches,
        [
            "chance 1 of the 3 that vtium gives the font helper each boot)",
            "chance 2 of the 3 that vtium gives the font helper each boot)"
        ]
    );
}

// The last start of a search is sent the waiting ask and the fill at
// its start. It answers the ask, then crashes while the fill is still
// open. The early bold ask that the answer makes due waits until the
// search has ended, so the last start is sent nothing after its
// start, and its crash blames the source the search added last. vtium
// leaves that source out at once, uses no second chance, and sends
// the bold ask to the helper it starts without the source.
//
// References:
// - ADR-1200 D15, "Restarted after a failure": three chances a boot,
//   shared by the searches and the plain restarts.
// - ADR-1200 D18, note of 2026-11-25: the one ask for glyphs the
//   screen does not show yet.
#[test]
fn a_bold_ask_waits_until_the_search_ends_so_the_last_starts_crash_blames_its_source() {
    crate::getty::tests::logged();
    // the first helper crashes at once; the fifth start, the last of
    // the search, answers the ask and crashes with the fill open; the
    // helper started after the blame answers the fill and the bold ask
    let fails = "if [ $k = 1 ]; then kill -SEGV $$; fi
if [ $k = 5 ]; then cat \"$1.ask\"; kill -SEGV $$; fi
if [ $k -ge 6 ]; then cat \"$1.after\"; exec cat >/dev/null; fi";
    let mut s = sourced("source-bold-early", "true", fails, &filled(CELL));
    s.helper.asks_bold_early = true;
    let mut ask = frame_bytes(FRAME_GLYPHS, &set(CELL, &['A']));
    ask.extend(frame_bytes(FRAME_DONE, &[]));
    std::fs::write(s.dir.join("answer.ask"), ask).unwrap();
    let mut after = filled(CELL);
    after.extend(answered(BOLD, &bold_ascii()));
    std::fs::write(s.dir.join("answer.after"), after).unwrap();
    let mut now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    s.helper.fill(CELL, Band::NONE, &['A'], now);
    let _ = drive(&mut s, &mut now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(s.helper.restarts(), 1, "the search alone used a chance");
    assert_eq!(s.helper.left_out(), 0b100, "the third source is left out");
    assert_eq!(masks(&s), [0, 7, 6, 4, 0, 4]);
    assert!(s.held('A'));
    assert!(held_in(&s.helper, BOLD, '~'), "the bold ask went out after");
    assert!(s.helper.asks.is_empty());
    let blamed: Vec<String> = crate::getty::tests::logged()
        .into_iter()
        .filter(|l| l.contains("leaves out source="))
        .collect();
    assert_eq!(blamed.len(), 1, "{blamed:?}");
    assert!(blamed[0].contains("source=builtin:unifont"), "{blamed:?}");
}

// ADR-1302 D13 changed the request's and the answers' layout: a helper
// of an older build refuses the request as not one, and its fill's end
// has no band. Either gives the helper up, saying the two come from
// different builds and that reinstalling vtium mends it.
#[test]
fn a_helper_of_another_build_is_given_up_saying_to_reinstall_vtium() {
    let refused = raw_frame(FRAME_FAILED, b"not a serve request");
    let mut old_end = Vec::new();
    for w in [CELL.width, CELL.height] {
        old_end.extend(w.to_le_bytes());
    }
    let old_end = raw_frame(FRAME_FILLED, &old_end);
    for (name, fixture) in [("oldhelper", refused), ("oldend", old_end)] {
        let (why, _) = given_up(name, ANSWERS, &fixture, true);
        assert!(why.contains("different builds of vtium"), "{name}: {why}");
        assert!(why.contains("reinstall vtium"), "{name}: {why}");
    }
}

#[test]
fn garbage_gives_the_helper_up_at_once() {
    for (name, fixture, fill, says) in [
        (
            "garbage",
            b"VTVOUTL1 not a frame".to_vec(),
            false,
            "refused",
        ),
        (
            "badset",
            frame_bytes(FRAME_GLYPHS, b"VTGLASS\0junk"),
            false,
            "cannot read",
        ),
        (
            "cell",
            frame_bytes(
                FRAME_GLYPHS,
                &set(
                    Cell {
                        width: 8,
                        height: 8,
                    },
                    &['A'],
                ),
            ),
            false,
            "not asked",
        ),
        (
            "unasked",
            frame_bytes(FRAME_GLYPHS, &set(CELL, &['A', 'B'])),
            false,
            "did not name",
        ),
        (
            "nofill",
            frame_bytes(FRAME_FILL, &set(CELL, &['A'])),
            false,
            "not asked",
        ),
        (
            "fillcell",
            frame_bytes(
                FRAME_FILL,
                &set(
                    Cell {
                        width: 8,
                        height: 8,
                    },
                    &['A'],
                ),
            ),
            true,
            "not asked",
        ),
        (
            "failed",
            frame_bytes(FRAME_FAILED, b"no source of the frit is here"),
            false,
            "no source of the frit is here",
        ),
    ] {
        let (why, restarts) = given_up(name, ANSWERS, &fixture, fill);
        assert!(why.contains(says), "{name}: {why}");
        assert_eq!(restarts, 0, "{name}");
    }
}

#[test]
fn a_fill_that_repeats_a_glyph_is_refused() {
    // the same set over and over, as a stand-in that floods: given up
    // at the second, however long the flood
    let a = frame_bytes(FRAME_FILL, &set(CELL, &['A']));
    let flood: Vec<u8> = a.iter().copied().cycle().take(a.len() * 20_000).collect();
    let (why, _) = given_up("flood", ANSWERS, &flood, true);
    assert!(why.contains("repeats"), "{why}");
    // an ask's glyph sent twice
    let mut twice = frame_bytes(FRAME_GLYPHS, &set(CELL, &['A']));
    twice.extend(frame_bytes(FRAME_GLYPHS, &set(CELL, &['A'])));
    let (why, _) = given_up("twice", ANSWERS, &twice, false);
    assert!(why.contains("did not name"), "{why}");
}

// D18: an ask during a fill and the fill are two requests, and each
// sends the glyph the ask named; the console takes the ask's and drops
// the fill's, which the store already holds
#[test]
fn a_fill_frame_of_glyphs_an_ask_brought_is_dropped() {
    let (a, b) = (set(CELL, &['A']), set(CELL, &['B']));
    let mut fixture = frame_bytes(FRAME_GLYPHS, &a);
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    fixture.extend(frame_bytes(FRAME_FILL, &a));
    fixture.extend(frame_bytes(FRAME_FILL, &b));
    fixture.extend(filled(CELL));
    let mut s = StandIn::new("fillafterask", ANSWERS, &fixture);
    let now = t0();
    s.helper.fill(CELL, Band::NONE, &[], now);
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    let answers = s.run(now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(answers.len(), 2);
    assert!(s.held('A') && s.held('B'));
    assert_eq!(
        crate::deferred::lock(&s.helper.store).bytes(),
        (a.len() + b.len()) as u64
    );
    assert!(!s.helper.busy());
}

// D18, D19: the fill's later batch sends again the glyph an ask during
// the fill brought; the store takes the batch without it, holds it
// once and counts the batch as written again
#[test]
fn an_asked_glyph_a_later_fill_batch_sends_again_is_held_once() {
    let mut fixture = frame_bytes(FRAME_GLYPHS, &set(CELL, &['A']));
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    fixture.extend(frame_bytes(FRAME_FILL, &set(CELL, &['A', 'B'])));
    fixture.extend(filled(CELL));
    let mut s = StandIn::new("askinfill", ANSWERS, &fixture);
    let now = t0();
    s.helper.fill(CELL, Band::NONE, &[], now);
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    let answers = s.run(now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(answers.len(), 2);
    assert!(answers[0].face.has_glyph('A'));
    assert!(answers[1].face.has_glyph('B') && !answers[1].face.has_glyph('A'));
    assert!(s.held('A') && s.held('B'));
    let (a, b) = (set(CELL, &['A']).len(), set(CELL, &['B']).len());
    assert_eq!(
        crate::deferred::lock(&s.helper.store).bytes(),
        (a + b) as u64
    );
    assert!(!s.helper.busy());
}

// D18: a fill batch on its way when an ask is sent brings the asked
// glyph first; the ask's answer adds nothing, and the ask ends
#[test]
fn an_asks_glyph_a_fill_batch_brought_first_adds_nothing() {
    let ab = set(CELL, &['A', 'B']);
    let mut fixture = frame_bytes(FRAME_FILL, &ab);
    fixture.extend(frame_bytes(FRAME_GLYPHS, &set(CELL, &['A'])));
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    fixture.extend(filled(CELL));
    let mut s = StandIn::new("fillbeforeask", ANSWERS, &fixture);
    let now = t0();
    s.helper.fill(CELL, Band::NONE, &[], now);
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    let answers = s.run(now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(answers.len(), 1);
    assert!(s.held('A') && s.held('B'));
    assert_eq!(
        crate::deferred::lock(&s.helper.store).bytes(),
        ab.len() as u64
    );
    assert!(!s.helper.busy());
    // answered, so A is neither pending nor lacking
    let state = s.helper.cells.iter().find(|c| c.at == AT).unwrap();
    assert!(!state.pending.has(0x41) && !state.absent.has(0x41));
}

// D18, D19: the fill's limit weighs a batch as the store writes it
// again, so a batch whose new glyphs fit is taken
#[test]
fn a_fill_batch_fits_the_store_by_its_new_glyphs() {
    let (a, c) = (set(CELL, &['A']), set(CELL, &['C']));
    let limit = (a.len() + c.len()) as u64;
    let mut fixture = frame_bytes(FRAME_GLYPHS, &a);
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    fixture.extend(frame_bytes(FRAME_FILL, &set(CELL, &['A', 'C'])));
    fixture.extend(filled(CELL));
    let mut s = StandIn::limited("fillfits", ANSWERS, &fixture, limit);
    let now = t0();
    s.helper.fill(CELL, Band::NONE, &[], now);
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    let answers = s.run(now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(answers.len(), 2);
    assert!(s.held('A') && s.held('C'));
    assert_eq!(crate::deferred::lock(&s.helper.store).bytes(), limit);
    assert!(!s.helper.full);
}

// D18, D19: an ask's frame carries B, which the store holds and the
// ask's compaction drops; the store keeps B from the frame
#[test]
fn a_held_glyph_whose_set_an_ask_evicts_is_kept_from_the_asks_frame() {
    let bc = set(CELL, &['B', 'C']);
    let limit = bc.len() as u64;
    let mut fixture = frame_bytes(FRAME_FILL, &set(CELL, &['A', 'B']));
    fixture.extend(frame_bytes(FRAME_GLYPHS, &bc));
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    fixture.extend(filled(CELL));
    let mut s = StandIn::limited("evictheld", ANSWERS, &fixture, limit);
    let now = t0();
    s.helper.fill(CELL, Band::NONE, &[], now);
    s.helper.ask(CELL, Band::NONE, &shown(&['B', 'C']), now);
    let answers = s.run(now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(answers.len(), 2);
    assert!(answers[1].face.has_glyph('B') && answers[1].face.has_glyph('C'));
    assert!(!s.held('A') && s.held('B') && s.held('C'));
    assert_eq!(crate::deferred::lock(&s.helper.store).bytes(), limit);
}

/// A set of `chars` at `cell`, each glyph its own shade, its chunks
/// raw as the helper writes them.
fn shades(cell: Cell, chars: &[char]) -> Vec<u8> {
    let mut s = StrikeBuilder::new(PixelFormat::R8, cell, Style::default()).compress(false);
    for (seed, &c) in (1u32..).zip(chars) {
        let g = s.glyph(
            Width::Narrow,
            (0..cell.width * cell.height)
                .map(|i| u8::try_from((i * 7 + seed * 13) % 254 + 1).unwrap())
                .collect(),
        );
        s.map(c, g);
    }
    SetBuilder::new("t").strike(s).write().unwrap()
}

// #145, ADR-1200 D19: an ask at a full store compacts the set that
// holds both glyphs the screen shows and the least recently shown:
// the shown stay, only enough of the others leave to cover the
// overrun, and the store counts the set as written again
#[test]
fn an_ask_at_a_full_store_compacts_the_set_it_takes_room_from() {
    let abcd = shades(CELL, &['A', 'B', 'C', 'D']);
    let e = shades(CELL, &['E']);
    let limit = (abcd.len() + e.len() - 1) as u64;
    let script = format!(
        "head -c {} >/dev/null; cat \"$1\"; head -c {} >/dev/null; cat \"$1.ask\"; cat >/dev/null",
        request_bytes(FILL),
        request_bytes(ASK) + 4
    );
    let mut fill = frame_bytes(FRAME_FILL, &abcd);
    fill.extend(filled(CELL));
    let mut s = StandIn::limited("askcompacts", &script, &fill, limit);
    let mut ask = frame_bytes(FRAME_GLYPHS, &e);
    ask.extend(frame_bytes(FRAME_DONE, &[]));
    std::fs::write(s.dir.join("answer.ask"), ask).unwrap();
    let now = t0();
    s.helper.fill(CELL, Band::NONE, &[], now);
    assert_eq!(s.run(now).len(), 1);
    assert_eq!(
        crate::deferred::lock(&s.helper.store).bytes(),
        abcd.len() as u64
    );
    // the screen shows A and B; C and D are the least recently shown
    crate::deferred::lock(&s.helper.store).touch(AT, &shown(&['A', 'B']));
    s.helper.ask(CELL, Band::NONE, &shown(&['E']), now);
    let answers = s.run(now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(answers.len(), 1);
    assert!(answers[0].face.has_glyph('E'));
    // one byte over: C alone covers it
    assert!(s.held('A') && s.held('B') && s.held('D') && s.held('E'));
    assert!(!s.held('C'));
    let rebuilt = shades(CELL, &['A', 'B', 'D']).len();
    assert_eq!(
        crate::deferred::lock(&s.helper.store).bytes(),
        (rebuilt + e.len()) as u64
    );
    assert!(crate::deferred::lock(&s.helper.store).bytes() <= limit);
    // C, evicted, is asked for again when the screen shows it
    s.helper.ask(CELL, Band::NONE, &shown(&['C']), now);
    assert!(s.helper.busy());
}

// a zoom or a hotplug during a fill: the frames already on their way
// for the cell no display draws are dropped, and the helper stays
#[test]
fn a_fill_frame_for_a_cell_no_display_draws_is_dropped() {
    let mut fixture = frame_bytes(FRAME_FILL, &set(CELL, &['A']));
    fixture.extend(filled(CELL));
    fixture.extend(stopped(CELL));
    let mut s = StandIn::new("late", ANSWERS, &fixture);
    let now = t0();
    s.helper.fill(CELL, Band::NONE, &[], now);
    s.helper.retain(&[], now);
    // the three frames read, then taken
    for _ in 0..3 {
        s.woke.recv_timeout(Duration::from_secs(10)).unwrap();
    }
    assert!(s.helper.poll(now).is_empty());
    assert_eq!(s.helper.failed(), None);
    assert!(!s.held('A'));
    assert!(s.helper.stopped.is_empty());
    assert!(!s.helper.busy());
}

// D15, D17: a zoom away and back before the old fill's frames are
// taken; the frames before the helper took the stop are the old
// fill's and dropped, the new fill's are taken, and the helper stays
#[test]
fn a_cell_stopped_and_filled_again_takes_only_the_new_fill() {
    let mut fixture = frame_bytes(FRAME_FILL, &set(CELL, &['A']));
    fixture.extend(stopped(CELL));
    fixture.extend(frame_bytes(FRAME_FILL, &set(CELL, &['A'])));
    fixture.extend(filled(CELL));
    let mut s = StandIn::new("zoomback", ANSWERS, &fixture);
    let now = t0();
    s.helper.fill(CELL, Band::NONE, &[], now);
    s.helper.retain(&[], now);
    s.helper.fill(CELL, Band::NONE, &[], now);
    let answers = s.run(now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(answers.len(), 1);
    assert!(s.held('A'));
    assert!(s.helper.stopped.is_empty());
    assert!(!s.helper.busy());
}

// the old fill's end, sent before the helper took the stop, leaves the
// new fill running, and the new fill's glyphs are taken
#[test]
fn a_stopped_fills_end_does_not_end_the_next_fill() {
    let mut fixture = frame_bytes(FRAME_FILL, &set(CELL, &['A']));
    fixture.extend(filled(CELL));
    fixture.extend(stopped(CELL));
    fixture.extend(frame_bytes(FRAME_FILL, &set(CELL, &['A'])));
    fixture.extend(frame_bytes(FRAME_FILL, &set(CELL, &['B'])));
    fixture.extend(filled(CELL));
    let mut s = StandIn::new("zoombackend", ANSWERS, &fixture);
    let now = t0();
    s.helper.fill(CELL, Band::NONE, &[], now);
    s.helper.retain(&[], now);
    s.helper.fill(CELL, Band::NONE, &[], now);
    let answers = s.run(now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(answers.len(), 2);
    assert!(s.held('A') && s.held('B'));
    assert!(!s.helper.busy());
}

// D15: the word that a stop was taken, with no stop sent, was not
// asked
#[test]
fn a_stop_taken_that_was_not_sent_gives_the_helper_up() {
    let mut fixture = frame_bytes(FRAME_FILL, &set(CELL, &['A']));
    fixture.extend(stopped(CELL));
    let (why, restarts) = given_up("stopunsent", ANSWERS, &fixture, true);
    assert!(why.contains("stop not sent"), "{why}");
    assert_eq!(restarts, 0);
}

/// The bytes of the stand-ins' request of `kind` at the cell, naming
/// nothing.
fn request_bytes(kind: u32) -> usize {
    encode(
        &frit("source=builtin:unscii-8\n"),
        None,
        (
            kind,
            CELL,
            Band::NONE,
            (Variant::default(), None),
            ClusterWidth::Double,
        ),
        (&[], &[]),
        &[],
    )
    .len()
}

// a zoom away from a cell whose fill stopped at the store's limit: the
// frames already on their way for it are dropped, and the helper stays
#[test]
fn a_late_frame_for_a_stopped_cell_no_display_draws_is_dropped() {
    let a = set(CELL, &['A']);
    let script = format!(
        "head -c {} >/dev/null; cat \"$1\"; head -c {stop} >/dev/null; cat \"$1.late\"; head -c {stop} >/dev/null; cat \"$1.stop\"; cat >/dev/null",
        request_bytes(FILL),
        stop = request_bytes(STOP)
    );
    let mut s = StandIn::limited("stoppedlate", &script, &frame_bytes(FRAME_FILL, &a), 1);
    // B, sent before the helper took the store's stop, then that stop
    // taken; then the second stop taken
    let mut late = frame_bytes(FRAME_FILL, &set(CELL, &['B']));
    late.extend(stopped(CELL));
    std::fs::write(s.dir.join("answer.late"), late).unwrap();
    std::fs::write(s.dir.join("answer.stop"), stopped(CELL)).unwrap();
    let now = t0();
    s.helper.fill(CELL, Band::NONE, &[], now);
    // A past the store's limit: dropped, and the fill stopped
    s.woke.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(s.helper.poll(now).is_empty());
    assert_eq!(s.helper.failed(), None);
    s.helper.retain(&[], now);
    // B, then the two stops taken
    for _ in 0..3 {
        s.woke.recv_timeout(Duration::from_secs(10)).unwrap();
    }
    assert!(s.helper.poll(now).is_empty());
    assert_eq!(s.helper.failed(), None);
    assert!(s.helper.stopped.is_empty());
}

// a cell dropped whose fill is done is stopped too
// ADR-1302 D13: a stop at a cell with a band names the band, and the
// helper's word that it took the stop names it too, so the console
// takes it against the stop it sent at that band
#[test]
fn a_stop_at_a_band_is_taken_at_that_band() {
    let band = Band { above: 2, below: 1 };
    let at = SpacedCell { cell: CELL, band };
    let mut fixture = spaced_frame(FRAME_FILL, band, &set(CELL, &['A']));
    fixture.extend(spaced_end(FRAME_FILLED, at));
    let script = format!(
        "head -c {} >/dev/null; cat \"$1\"; head -c {} >\"$1.stop\"; cat \"$1.taken\"",
        request_bytes(FILL),
        request_bytes(STOP)
    );
    let mut s = StandIn::new("bandstop", &script, &fixture);
    std::fs::write(s.dir.join("answer.taken"), spaced_end(FRAME_STOPPED, at)).unwrap();
    let now = t0();
    s.helper.fill(CELL, band, &[], now);
    assert_eq!(s.run(now).len(), 1);
    s.helper.retain(&[], now);
    s.until_gone(now);
    assert_eq!(s.helper.failed(), None, "the stop at the band is taken");
    assert!(s.helper.stopped.is_empty(), "no stop is left waiting");
    let stop = encode(
        &s.helper.frit,
        None,
        (
            STOP,
            CELL,
            band,
            (Variant::default(), None),
            ClusterWidth::Double,
        ),
        (&[], &[]),
        &[],
    );
    assert_eq!(std::fs::read(s.dir.join("answer.stop")).unwrap(), stop);
}

#[test]
fn a_cell_dropped_after_its_fill_is_stopped() {
    let mut fixture = frame_bytes(FRAME_FILL, &set(CELL, &['A']));
    fixture.extend(filled(CELL));
    let script = format!(
        "head -c {} >/dev/null; cat \"$1\"; head -c {} >\"$1.stop\"; cat \"$1.taken\"",
        request_bytes(FILL),
        request_bytes(STOP)
    );
    let mut s = StandIn::new("donestop", &script, &fixture);
    std::fs::write(s.dir.join("answer.taken"), stopped(CELL)).unwrap();
    let now = t0();
    s.helper.fill(CELL, Band::NONE, &[], now);
    assert_eq!(s.run(now).len(), 1);
    assert!(!s.helper.busy());
    s.helper.retain(&[], now);
    s.until_gone(now);
    assert_eq!(s.helper.failed(), None);
    let stop = encode(
        &s.helper.frit,
        None,
        (
            STOP,
            CELL,
            Band::NONE,
            (Variant::default(), None),
            ClusterWidth::Double,
        ),
        (&[], &[]),
        &[],
    );
    assert_eq!(std::fs::read(s.dir.join("answer.stop")).unwrap(), stop);
}

#[test]
fn a_fill_with_no_glyph_is_a_failure() {
    let (why, _) = given_up("none", ANSWERS, &filled(CELL), true);
    assert!(why.contains("no glyph"), "{why}");
}

#[test]
fn a_frame_past_its_bound_is_refused_before_allocation() {
    // a length of 4 GiB: refused on the head, with no allocation of it
    let mut huge = FRAME_GLYPHS.to_le_bytes().to_vec();
    huge.extend_from_slice(&u32::MAX.to_le_bytes());
    let e = frame(&mut &huge[..]);
    assert!(
        matches!(&e, Some(Event::Malformed(why)) if why.contains("past its kind's")),
        "{e:?}"
    );
    let mut done = FRAME_DONE.to_le_bytes().to_vec();
    done.extend_from_slice(&1u32.to_le_bytes());
    assert!(matches!(frame(&mut &done[..]), Some(Event::Malformed(_))));
    let (why, _) = given_up("huge", ANSWERS, &huge, false);
    assert!(why.contains("past its kind's"), "{why}");
}

// ADR-1401 D7: the console takes a coverage answer only whole. It
// refuses an answer whose count runs past the codepoints the payload
// holds, an answer with a codepoint that is not a Unicode scalar
// value, and an answer whose font name is not UTF-8.
#[test]
fn a_malformed_coverage_answer_is_refused() {
    let answer = |count: u32, codepoints: &[u32], font: &[u8]| {
        let mut payload = count.to_le_bytes().to_vec();
        for c in codepoints {
            payload.extend_from_slice(&c.to_le_bytes());
        }
        payload.extend_from_slice(font);
        frame(&mut &raw_frame(FRAME_COVERAGE, &payload)[..])
    };
    let whole = answer(2, &[0x41, 0x416], b"Noto Sans");
    assert!(
        matches!(&whole, Some(Event::Coverage(c, f)) if c == &[0x41, 0x416] && f == "Noto Sans"),
        "{whole:?}"
    );
    let none = answer(0, &[], b"");
    assert!(
        matches!(&none, Some(Event::Coverage(c, f)) if c.is_empty() && f.is_empty()),
        "{none:?}"
    );
    let malformed: [(&str, u32, &[u32], &[u8]); 6] = [
        ("a count past the payload", 3, &[0x41, 0x42], b""),
        ("a count of u32::MAX", u32::MAX, &[0x41], b""),
        ("a surrogate", 1, &[0xd800], b""),
        ("a codepoint past U+10FFFF", 1, &[0x11_0000], b""),
        ("a font name that is not UTF-8", 1, &[0x41], b"Noto\xff"),
        ("a font name cut inside a character", 0, &[], b"Noto \xd0"),
    ];
    for (what, count, codepoints, font) in malformed {
        let e = answer(count, codepoints, font);
        assert!(matches!(&e, Some(Event::Malformed(_))), "{what}: {e:?}");
    }
    let short = frame(&mut &raw_frame(FRAME_COVERAGE, &[1, 0])[..]);
    assert!(
        matches!(&short, Some(Event::Malformed(why)) if why.contains("without its count")),
        "{short:?}"
    );
}

#[test]
fn a_fill_stops_at_the_stores_limit_and_an_ask_past_it_evicts() {
    let a = set(CELL, &['A']);
    let limit = (a.len() as u64) * 3 / 2;
    let mut fixture = frame_bytes(FRAME_FILL, &a);
    fixture.extend(frame_bytes(FRAME_FILL, &set(CELL, &['B'])));
    fixture.extend(frame_bytes(FRAME_FILL, &set(CELL, &['C'])));
    fixture.extend(stopped(CELL));
    fixture.extend(frame_bytes(FRAME_GLYPHS, &set(CELL, &['D'])));
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    let mut s = StandIn::limited("limit", ANSWERS, &fixture, limit);
    let now = t0();
    s.helper.fill(CELL, Band::NONE, &[], now);
    s.helper.ask(CELL, Band::NONE, &shown(&['D']), now);
    let answers = s.run(now);
    assert_eq!(s.helper.failed(), None);
    // the fill took A and stopped; the ask's D evicted A, and the
    // eviction is no answer, so it damages nothing
    assert_eq!(answers.len(), 2);
    let holds = |a: &Answer, c| a.face.has_glyph(c);
    assert!(holds(&answers[0], 'A') && !holds(&answers[0], 'D'));
    assert!(holds(&answers[1], 'D') && !holds(&answers[1], 'A'));
    assert!(!s.held('A') && !s.held('B') && !s.held('C') && s.held('D'));
    assert!(crate::deferred::lock(&s.helper.store).bytes() <= limit);
    assert!(s.helper.stopped.is_empty());
    assert!(!s.helper.busy());
    // A, evicted, is asked for again when the screen shows it
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    assert!(s.helper.busy());
}

#[test]
fn an_asks_glyphs_the_store_cannot_take_are_asked_again() {
    // one frame past the whole limit: dropped, and its glyph not left
    // pending
    let a = set(CELL, &['A']);
    let limit = (a.len() as u64) / 2;
    let mut fixture = frame_bytes(FRAME_GLYPHS, &a);
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    let mut s = StandIn::limited("toolarge", ANSWERS, &fixture, limit);
    let now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    assert!(s.run(now).is_empty());
    assert_eq!(s.helper.failed(), None);
    assert!(!s.held('A'));
    assert!(!s.helper.busy());
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    assert!(s.helper.busy());
}

#[test]
fn a_helper_that_hangs_is_started_again_then_given_up() {
    let mut s = StandIn::new("hangs", "exec sleep 30", b"");
    let mut now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    assert!(s.helper.poll(now).is_empty());
    assert_eq!(
        s.helper.deadline(),
        Some(after(now, Duration::from_millis(ANSWER_MS)))
    );
    for _ in 0..=RESTARTS_MAX {
        now = after(now, Duration::from_millis(ANSWER_MS));
        assert!(s.helper.poll(now).is_empty());
    }
    let why = s.helper.failed().unwrap();
    assert!(why.contains("gave no answer for 10 s"), "{why}");
    assert!(s.helper.running.is_none());
}

#[test]
fn an_ask_has_one_deadline_however_often_frames_come() {
    // a fill's frame keeps the answer due, but not the ask's end
    let mut s = StandIn::new(
        "deadline",
        ANSWERS,
        &frame_bytes(FRAME_FILL, &set(CELL, &['B'])),
    );
    let now = t0();
    s.helper.fill(CELL, Band::NONE, &[], now);
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    let _ = s.woke.recv_timeout(Duration::from_secs(10));
    let nearly = after(now, Duration::from_millis(REQUEST_MS - 1));
    let late = after(now, Duration::from_millis(REQUEST_MS));
    assert_eq!(s.helper.poll(nearly).len(), 1);
    assert_eq!(s.helper.deadline(), Some(late));
    assert!(s.helper.poll(late).is_empty());
    assert!(s.helper.search.is_some(), "the ask was past its end");
}

#[test]
fn a_helper_ended_by_its_cpu_limit_is_replaced_a_bounded_number_of_times() {
    // each helper answers its ask, then the kernel ends it for its CPU
    // time: the next need starts another, up to RESTARTS_MAX
    let mut fixture = frame_bytes(FRAME_GLYPHS, &set(CELL, &['A']));
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    let mut s = StandIn::new(
        "cpu",
        "head -c 1 >/dev/null; cat \"$1\"; kill -XCPU $$",
        &fixture,
    );
    let now = t0();
    for round in 0..=RESTARTS_MAX {
        s.helper.retain(&[], now);
        s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
        assert_eq!(s.run(now).len(), 1, "round {round}");
        s.until_gone(now);
        if round < RESTARTS_MAX {
            assert_eq!(s.helper.failed(), None, "round {round}");
        }
    }
    let why = s.helper.failed().unwrap();
    assert_eq!(
        why,
        "the font helper reached its CPU limit after vtium had used all 3 chances that it gives the font helper each boot"
    );
}

fn own_cell_frame(width: u32, height: u32) -> Vec<u8> {
    cell_frame(FRAME_OWN_CELL, width, height)
}

fn cell_frame(kind: u32, width: u32, height: u32) -> Vec<u8> {
    let mut c = width.to_le_bytes().to_vec();
    c.extend_from_slice(&height.to_le_bytes());
    frame_bytes(kind, &c)
}

// overhang part 1 (the maintainer, 2026-11-01): an outline first source
// answers frame 10 with its cell at the height asked, which the query
// names; a cell of another height is refused as malformed, a warning
// before the answer is logged and the wait goes on, and a bitmap
// source's frame 6 answers a query at any height
#[test]
fn an_outline_frits_cell_is_asked_at_a_height_and_taken_only_at_it() {
    let wait = Duration::from_secs(10);
    let nine = Cell {
        width: 9,
        height: 16,
    };
    let mut s = StandIn::new(
        "outline-cell",
        ANSWERS,
        &cell_frame(FRAME_OUTLINE_CELL, 9, 16),
    );
    assert_eq!(s.helper.own_cell(16, wait, &t0), Ok(OwnCell::Outline(nine)));
    let mut s = StandIn::new("outline-cell-bitmap", ANSWERS, &own_cell_frame(8, 16));
    assert_eq!(s.helper.own_cell(22, wait, &t0), Ok(OwnCell::Bitmap(CELL)));
    let mut warned = frame_bytes(FRAME_WARNING, b"t: line 2: an unknown key");
    warned.extend(cell_frame(FRAME_OUTLINE_CELL, 12, 22));
    let mut s = StandIn::new("outline-cell-warned", ANSWERS, &warned);
    assert_eq!(
        s.helper.own_cell(22, wait, &t0),
        Ok(OwnCell::Outline(Cell {
            width: 12,
            height: 22
        }))
    );
    let mut s = StandIn::new(
        "outline-cell-taller",
        ANSWERS,
        &cell_frame(FRAME_OUTLINE_CELL, 9, 17),
    );
    let why = s.helper.own_cell(16, wait, &t0).unwrap_err().to_string();
    assert_eq!(why, "an outline cell 17 pixels high, where 16 was asked");
    s.run(t0());
    assert!(
        s.helper.failed().unwrap().contains("17 pixels high"),
        "{:?}",
        s.helper.failed()
    );
}

// overhang part 1: a cell query made while the helper is busy waits
// past the frames sent before its answer, which the next poll takes,
// and past the late answer to an earlier query, which is kept with the
// height that query asked at (ADR-1200 D17, the maintainer, 2026-11-03)
#[test]
fn a_cell_answer_is_found_behind_other_frames_and_a_late_answer() {
    let wait = Duration::from_secs(10);
    let mut fixture = frame_bytes(FRAME_GLYPHS, &set(CELL, &['A']));
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    fixture.extend(cell_frame(FRAME_OUTLINE_CELL, 9, 16));
    let mut s = StandIn::new("cell-behind", ANSWERS, &fixture);
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), t0());
    let nine = Cell {
        width: 9,
        height: 16,
    };
    assert_eq!(s.helper.own_cell(16, wait, &t0), Ok(OwnCell::Outline(nine)));
    assert_eq!(s.run(t0()).len(), 1, "the ask's answer is kept");
    assert_eq!(s.helper.failed(), None);
    let mut two = cell_frame(FRAME_OUTLINE_CELL, 9, 16);
    two.extend(cell_frame(FRAME_OUTLINE_CELL, 12, 22));
    let mut s = StandIn::new("cell-late-then-now", HELD, &two);
    assert!(
        s.helper
            .own_cell(16, Duration::from_millis(20), &t0)
            .is_err()
    );
    std::fs::write(s.dir.join("answer.go"), b"").unwrap();
    assert_eq!(
        s.helper.own_cell(22, wait, &t0),
        Ok(OwnCell::Outline(Cell {
            width: 12,
            height: 22
        }))
    );
    assert_eq!(s.helper.late_cells(), [(16, Ok(OwnCell::Outline(nine)))]);
    assert_eq!(s.helper.late_cells(), [], "each is handed on once");
    let _ = s.helper.poll(t0());
    assert_eq!(s.helper.failed(), None);
}

// ADR-1200 D17: a query asked again at the same height, as an OSC 50
// switch back asks after the start's query had no answer in time,
// takes its own answer, the second; the first query's answer, which
// the helper sends before it, is handed on late behind it. The app
// keeps the answer in time (`a_late_cell_answer_fills_only_a_missing_answer`).
#[test]
fn a_query_asked_again_keeps_its_own_answer_and_the_earlier_one_late() {
    let mut two = frame_bytes(FRAME_NO_OWN_CELL, b"the first query's answer");
    two.extend(own_cell_frame(8, 16));
    let mut s = StandIn::new("cell-asked-again", HELD, &two);
    assert!(
        s.helper
            .own_cell(16, Duration::from_millis(20), &t0)
            .unwrap_err()
            .late
    );
    std::fs::write(s.dir.join("answer.go"), b"").unwrap();
    assert_eq!(
        s.helper.own_cell(16, Duration::from_secs(10), &t0),
        Ok(OwnCell::Bitmap(CELL))
    );
    let first = NoCell {
        why: "the first query's answer".to_owned(),
        late: false,
        from_frit: true,
    };
    assert_eq!(s.helper.late_cells(), [(16, Err(first))]);
    let _ = s.helper.poll(t0());
    assert_eq!(s.helper.failed(), None);
}

// ADR-1200 D17 (the maintainer, 2026-11-03): each query left unanswered
// past its wait keeps its own late answer, with the height it asked
// at, in the order they came, and the helper is not given up; a late
// outline cell of another height than its query asked is refused as
// malformed, as one in time is
#[test]
fn each_late_cell_answer_is_kept_with_its_height() {
    let mut two = cell_frame(FRAME_OUTLINE_CELL, 9, 16);
    two.extend(cell_frame(FRAME_OUTLINE_CELL, 12, 22));
    let mut s = StandIn::new("cell-late-twice", HELD, &two);
    let short = Duration::from_millis(20);
    assert!(s.helper.own_cell(16, short, &t0).unwrap_err().late);
    assert!(s.helper.own_cell(22, short, &t0).unwrap_err().late);
    assert_eq!(s.helper.late_cells(), [], "nothing came yet");
    std::fs::write(s.dir.join("answer.go"), b"").unwrap();
    s.until(t0(), |h| h.kept.len() == 2);
    assert_eq!(s.helper.failed(), None);
    let twelve = Cell {
        width: 12,
        height: 22,
    };
    let nine = Cell {
        width: 9,
        height: 16,
    };
    assert_eq!(
        s.helper.late_cells(),
        [
            (16, Ok(OwnCell::Outline(nine))),
            (22, Ok(OwnCell::Outline(twelve)))
        ]
    );
    let taller = cell_frame(FRAME_OUTLINE_CELL, 12, 22);
    let mut s = StandIn::new("cell-late-taller", HELD, &taller);
    assert!(s.helper.own_cell(16, short, &t0).unwrap_err().late);
    std::fs::write(s.dir.join("answer.go"), b"").unwrap();
    s.until(t0(), |h| h.failed().is_some());
    assert!(
        s.helper
            .failed()
            .is_some_and(|why| why.contains("22 pixels high, where 16 was asked")),
        "{:?}",
        s.helper.failed()
    );
    assert_eq!(s.helper.late_cells(), []);
}

// A cell query whose answer is late is work the helper owes. A helper
// that then sends nothing for the answer's bound is ended as a hung
// helper is, so a console that waits for the cell before it answers a
// program's queries cannot wait on a hung helper forever.
//
// References:
// - ADR-1200 D14: a hung helper is ended and started again.
// - ADR-1200 D17: the answer that comes after the wait.
#[test]
fn a_hung_helper_that_owes_a_late_cell_is_started_again() {
    let mut s = StandIn::new("cell-late-hung", HELD, &cell_frame(FRAME_OWN_CELL, 8, 16));
    let short = Duration::from_millis(20);
    let asked = t0();
    assert!(s.helper.own_cell(16, short, &t0).unwrap_err().late);
    assert!(!s.helper.busy(), "no ask, pointer ask or fill is out");
    let due = s.helper.deadline().unwrap();
    assert!(due >= after(asked, Duration::from_millis(ANSWER_MS)));
    crate::getty::tests::logged();
    let _ = s.helper.poll(due);
    let hung: Vec<String> = crate::getty::tests::logged()
        .into_iter()
        .filter(|l| l.contains("gave no answer for 10 s"))
        .collect();
    assert_eq!(hung.len(), 1, "the hung helper is ended once: {hung:?}");
}

// A query at 16 pixels is answered late, as when a program switches
// with OSC 50 to a font set whose cell comes after the wait. The
// program switches back and to the same font set again within the
// answer's bound, which asks at 16 pixels again. The helper answers both
// while vtium waits for the second answer, so the helper owes nothing
// more. The watchdog is then off, and a poll at the time the first
// query's answer would have been due ends nothing and uses no chance.
//
// References:
// - ADR-1200 D14: a hung helper is ended and started again, and a boot
//   gives it three chances.
// - ADR-1200 D17: the answer that comes after the wait.
#[test]
fn a_late_answer_taken_while_vtium_waits_for_another_leaves_no_watchdog() {
    let mut two = cell_frame(FRAME_OUTLINE_CELL, 9, 16);
    two.extend(cell_frame(FRAME_OUTLINE_CELL, 9, 16));
    let mut s = StandIn::new("cell-late-then-in-time", HELD, &two);
    let short = Duration::from_millis(20);
    assert!(s.helper.own_cell(16, short, &t0).unwrap_err().late);
    let due = s.helper.deadline().unwrap();
    std::fs::write(s.dir.join("answer.go"), b"").unwrap();
    let nine = Cell {
        width: 9,
        height: 16,
    };
    assert_eq!(
        s.helper.own_cell(16, Duration::from_secs(5), &t0),
        Ok(OwnCell::Outline(nine)),
        "the second query is answered in time"
    );
    assert!(!s.helper.owes(), "nothing is owed");
    assert_eq!(s.helper.deadline(), None, "no watchdog");
    crate::getty::tests::logged();
    let _ = s.helper.poll(due);
    let hung: Vec<String> = crate::getty::tests::logged()
        .into_iter()
        .filter(|l| l.contains("gave no answer"))
        .collect();
    assert_eq!(hung, Vec::<String>::new(), "the helper is not ended");
    assert_eq!(s.helper.restarts(), 0, "no chance is used");
    assert_eq!(s.helper.failed(), None);
    assert_eq!(s.helper.late_cells(), [(16, Ok(OwnCell::Outline(nine)))]);
}

// the operator's decision on #145, 2026-10-28: the console asks the
// helper for a bitmap frit's own cell and waits a bounded time, and a
// refusal is no failure; an answer after the wait is kept (ADR-1200
// D17, the maintainer, 2026-11-03), a refusal as a refusal
#[test]
fn a_frits_own_cell_is_asked_and_waited_for_a_bounded_time() {
    let wait = Duration::from_secs(10);
    let mut s = StandIn::new("own-cell", ANSWERS, &own_cell_frame(8, 16));
    assert_eq!(s.helper.own_cell(16, wait, &t0), Ok(OwnCell::Bitmap(CELL)));
    let refusal = frame_bytes(
        FRAME_NO_OWN_CELL,
        b"source=outline:/f.ttf is an outline font",
    );
    let mut s = StandIn::new("own-cell-refused", ANSWERS, &refusal);
    let why = s.helper.own_cell(16, wait, &t0).unwrap_err().to_string();
    assert!(why.contains("is an outline font"), "{why}");
    let _ = s.helper.poll(t0());
    assert_eq!(s.helper.failed(), None);
    // the answer comes after the wait, and is kept
    let mut s = StandIn::new("own-cell-late", HELD, &own_cell_frame(8, 16));
    let why = s
        .helper
        .own_cell(16, Duration::from_millis(50), &t0)
        .unwrap_err()
        .to_string();
    assert_eq!(why, "the helper gave no answer in 50 ms");
    std::fs::write(s.dir.join("answer.go"), b"").unwrap();
    s.until(t0(), |h| !h.kept.is_empty());
    assert_eq!(s.helper.failed(), None);
    assert_eq!(s.helper.late_cells(), [(16, Ok(OwnCell::Bitmap(CELL)))]);
    let mut s = StandIn::new("own-cell-late-refused", HELD, &refusal);
    assert!(
        s.helper
            .own_cell(16, Duration::from_millis(50), &t0)
            .unwrap_err()
            .late
    );
    std::fs::write(s.dir.join("answer.go"), b"").unwrap();
    s.until(t0(), |h| !h.kept.is_empty());
    assert_eq!(s.helper.failed(), None);
    let why = "source=outline:/f.ttf is an outline font".to_owned();
    assert_eq!(
        s.helper.late_cells(),
        [(
            16,
            Err(NoCell {
                why,
                late: false,
                from_frit: true
            })
        )]
    );
    // past the cell's bound, or never asked: given up
    let mut s = StandIn::new("own-cell-huge", ANSWERS, &own_cell_frame(8, 4096));
    assert!(s.helper.own_cell(16, wait, &t0).is_err());
    s.run(t0());
    assert!(s.helper.failed().unwrap().contains("past CELL_SIDE_MAX"));
    let mut s = StandIn::new("own-cell-unasked", ANSWERS, &own_cell_frame(8, 16));
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), t0());
    s.run(t0());
    assert!(s.helper.failed().unwrap().contains("a cell query not made"));
}

/// A directory of its own for a test of the helper's program, empty.
/// The test removes it when it ends.
fn program_dir(name: &str) -> Scratch {
    Scratch::new(&format!("program-{name}"))
}

/// A copy of `from` at `to`, with `mode`. `cp` writes it, so this
/// process never holds the file open to write: a child another test
/// forks meanwhile would hold that descriptor until its exec, and
/// starting the copy then fails with ETXTBSY.
fn program_copy(from: &str, to: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let copied = Process::new("cp").arg(from).arg(to).status().unwrap();
    assert!(copied.success(), "cp {from} {}", to.display());
    std::fs::set_permissions(to, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// What `ls -l /proc/self/fd` lists in a process `launcher` starts: its
/// descriptors and what each names.
fn descriptors_of(launcher: &Launcher) -> String {
    let out = launcher.process().output().unwrap();
    assert!(out.status.success(), "{out:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn sh(program: &Path, script: &str) -> Command {
    Command {
        program: program.to_owned(),
        args: vec!["-c".into(), script.into()],
        account: None,
    }
}

// the checks, on what fstat gives: root's file, or the account vtium
// runs as, and written by its owner alone
#[test]
fn a_program_root_or_vtium_owns_and_alone_writes_is_taken() {
    let me = 1000;
    assert_eq!(refusal(FileType::RegularFile, 0, 0o100_755, me), None);
    assert_eq!(refusal(FileType::RegularFile, me, 0o100_700, me), None);
    for mode in [0o104_755, 0o102_755, 0o106_755] {
        assert_eq!(
            refusal(FileType::RegularFile, 0, mode, 0),
            Some(format!("is setuid or setgid (mode {:o})", mode & 0o7777)),
            "{mode:o}"
        );
    }
    for kind in [
        FileType::Directory,
        FileType::Fifo,
        FileType::Symlink,
        FileType::CharacterDevice,
    ] {
        assert_eq!(
            refusal(kind, 0, 0o755, me).as_deref(),
            Some("is not a regular file"),
            "{kind:?}"
        );
    }
    assert_eq!(
        refusal(FileType::RegularFile, 1001, 0o100_755, me).as_deref(),
        Some("belongs to uid 1001, which is neither root nor vtium's own account")
    );
    // as root, only root's file
    assert!(refusal(FileType::RegularFile, me, 0o100_755, 0).is_some());
    for mode in [0o100_775, 0o100_757, 0o100_777, 0o100_765] {
        assert!(
            refusal(FileType::RegularFile, 0, mode, me).is_some_and(
                |why| why.starts_with("can be changed by its group or by other users")
            ),
            "{mode:o}"
        );
    }
}

// each refusal on a real file, checked on the file opened
#[test]
fn the_helpers_program_is_refused_unless_absolute_regular_and_kept_from_others() {
    let me = rustix::process::geteuid().as_raw();
    let dir = program_dir("refused");
    let refused = |path: &Path, me: u32| open_program(path, me).unwrap_err();
    let why = refused(Path::new("usr/bin/vtium-config"), me);
    assert_eq!(
        why,
        "vtium does not start the font helper usr/bin/vtium-config: the path is not absolute"
    );
    let why = refused(&dir, me);
    assert!(why.ends_with(": it is not a regular file"), "{why}");
    let fifo = dir.join("fifo");
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &fifo,
        FileType::Fifo,
        Mode::from_raw_mode(0o600),
        0,
    )
    .unwrap();
    let why = refused(&fifo, me);
    assert!(why.ends_with(": it is not a regular file"), "{why}");
    let program = dir.join("vtium-config");
    program_copy("/bin/true", &program, 0o755);
    assert!(open_program(&program, me).is_ok());
    let why = refused(&program, me.wrapping_add(1));
    assert!(
        why.contains(&format!("belongs to uid {me}, which is neither root")),
        "{why}"
    );
    program_copy("/bin/true", &program, 0o4755);
    let why = refused(&program, me);
    assert!(why.contains("is setuid or setgid (mode 4755)"), "{why}");
    for (mode, shown) in [(0o775, "775"), (0o757, "757")] {
        program_copy("/bin/true", &program, mode);
        let why = refused(&program, me);
        assert_eq!(
            why,
            format!(
                "vtium does not start the font helper {}: it can be changed by its group or by other users (mode {shown})",
                program.display()
            )
        );
    }
}

// a package upgrade renames a new program into place; a console that
// opened the old one goes on starting the old one
#[test]
fn a_helper_started_after_an_upgrade_is_the_program_opened_at_start() {
    let dir = program_dir("upgrade");
    let program = dir.join("vtium-config");
    program_copy("/bin/sh", &program, 0o755);
    let launcher = Launcher::open(sh(&program, "echo the old one")).unwrap();
    let new = dir.join("vtium-config.new");
    program_copy("/bin/false", &new, 0o755);
    std::fs::rename(&new, &program).unwrap();
    let out = launcher.process().output().unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "the old one\n");
}

// the opened program reaches no program vtium starts: not the helper
// started from it, nor the session's shell on its pty
#[test]
fn no_child_holds_the_helpers_program() {
    let dir = program_dir("descriptor");
    let program = dir.join("vtium-config");
    program_copy("/bin/sh", &program, 0o755);
    let name = program.display().to_string();
    let launcher = Launcher::open(sh(&program, "ls -l /proc/self/fd/")).unwrap();
    let listed = descriptors_of(&launcher);
    assert!(
        listed.contains("pipe:"),
        "the listing is the helper's: {listed}"
    );
    assert!(!listed.contains(&name), "{listed}");

    // the session writes its listing into a FIFO the test reads
    let out = dir.join("session");
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &out,
        FileType::Fifo,
        Mode::from_raw_mode(0o600),
        0,
    )
    .unwrap();
    let (tx, listing) = mpsc::channel();
    let fifo = out.clone();
    std::thread::spawn(move || {
        let _ = tx.send(std::fs::read_to_string(fifo));
    });
    let mut shell = Process::new("/bin/sh");
    shell.args(["-c", "ls -l /proc/self/fd/ >\"$0\""]).arg(&out);
    let size = vtium_platform::pty::Size {
        cols: 80,
        rows: 24,
        cell_width: 8,
        cell_height: 16,
    };
    let _pty =
        vtium_platform::pty::Pty::spawn(shell, size, vtium_platform::pty::Erase::Delete).unwrap();
    // bounded so the test fails instead of hanging
    let listed = listing
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    assert!(
        listed.contains("/dev/pts/"),
        "the listing is the session's: {listed}"
    );
    assert!(!listed.contains(&name), "{listed}");
    drop(launcher);
}

// font-helper names the program, its default the one vtium is built
// with
#[test]
fn font_helper_names_the_program_vtium_opens() {
    let config = |args: &[&str]| {
        let args: Vec<std::ffi::OsString> = args.iter().map(Into::into).collect();
        crate::config::load(&args, &mut |_: &Path| Ok(None)).config
    };
    assert_eq!(program(&config(&[])), Path::new(PROGRAM));
    let shell = stand_in_shell();
    let sh = config(&[&format!("--font-helper={}", shell.display())]);
    assert_eq!(program(&sh), shell);
    let opened = Launcher::configured(&sh).unwrap();
    assert_eq!(opened.command.program, shell);
    assert_eq!(opened.command.args, ["serve"]);
    // the console's locale goes with it, for the built-in font the
    // helper leaves characters to
    let localized = opened.with_locale("ja_JP.UTF-8");
    assert_eq!(localized.command.args, ["serve", "--locale", "ja_JP.UTF-8"]);
    // helper-sandbox=off goes with it too, and on adds nothing
    let helper = format!("--font-helper={}", shell.display());
    let off = Launcher::configured(&config(&[&helper, "--helper-sandbox=off"])).unwrap();
    assert_eq!(off.command.args, ["serve", "--sandbox", "off"]);
    let on = Launcher::configured(&config(&[&helper, "--helper-sandbox=on"])).unwrap();
    assert_eq!(on.command.args, ["serve"]);
    let why = Launcher::configured(&config(&["--font-helper=sh"])).unwrap_err();
    assert!(why.contains("is not absolute"), "{why}");
}

// The configured helper is told the active fonts, under
// the account vtium runs as, whatever the system's batch holds
//
// References:
// - ADR-1205 D4: the console tells each font helper the active fonts.
#[test]
fn a_configured_helper_is_told_the_active_fonts() {
    let args = [std::ffi::OsString::from(format!(
        "--font-helper={}",
        stand_in_shell().display()
    ))];
    let config = crate::config::load(&args, &mut |_: &Path| Ok(None)).config;
    let launcher = Launcher::configured(&config).unwrap();
    let active = launcher.active.unwrap();
    let me = rustix::process::geteuid().as_raw();
    assert_eq!(&active[..8], b"VTVSREQA");
    assert_eq!(active[8..12], ACTIVE.to_le_bytes());
    assert_eq!(active[12..16], me.to_le_bytes());
    // a stand-in started from a test's own command is told nothing
    assert!(
        Launcher::open(sh(stand_in_shell(), ""))
            .unwrap()
            .active
            .is_none()
    );
}

// a helper of an older build refuses the active fonts' request as a
// kind it does not know, and the console says what to do
#[test]
fn a_helper_that_does_not_know_the_active_fonts_is_of_another_build() {
    assert_eq!(
        failure("a malformed request: an unknown request kind\n"),
        OTHER_BUILD
    );
    assert_eq!(failure("no glyph"), "the helper failed: no glyph");
}

// a start that fails names /proc/self/fd, which a start without /proc
// mounted fails at: the console asks for a glyph, the helper's start
// fails, and the reason the console gives the helper up for is that
// message
#[test]
fn a_failed_start_names_proc_self_fd() {
    let dir = program_dir("unstartable");
    let program = dir.join("vtium-config");
    // no one may execute it
    program_copy("/bin/true", &program, 0o644);
    let launcher = Launcher::open(sh(&program, "")).unwrap();
    // the error the kernel gives this start, for the message's text.
    // qemu-user runs glibc's posix_spawn child as a fork that shares no
    // memory with its parent, so the child's failed exec never reaches
    // the parent, and the start succeeds. There the test cannot judge
    // what it claims, and it skips and says so.
    let e = match launcher.process().spawn() {
        Err(e) => e,
        Ok(mut child) => {
            let _ = child.wait();
            let qemu = std::env::var_os("VTIUM_QEMU_USER");
            assert!(qemu.is_some(), "a program no one may execute started");
            eprintln!(
                "{} reports no failed exec to the process that starts a program: skipped",
                qemu.unwrap_or_default().display()
            );
            return;
        }
    };
    let mut helper = Helper::new(
        launcher,
        Arc::new(|| {}),
        t0(),
        frit("source=builtin:unscii-8\n"),
        None,
        Shared::default(),
        1 << 20,
    );
    helper.ask(CELL, Band::NONE, &shown(&['A']), t0());
    assert_eq!(
        helper.failed(),
        Some(
            format!(
                "vtium cannot start the font helper {}: {e}. vtium starts it through /proc/self/fd, so /proc must be mounted.",
                program.display()
            )
            .as_str()
        )
    );
    assert!(helper.running.is_none(), "nothing started");
}

#[test]
fn a_missing_helper_is_not_opened() {
    let why = Launcher::open(Command {
        program: Path::new("/nonexistent/vtium-config").to_owned(),
        args: Vec::new(),
        account: None,
    })
    .unwrap_err();
    assert!(
        why.starts_with("cannot open the font helper /nonexistent/vtium-config"),
        "{why}"
    );
}

#[test]
fn held_codepoints_read_back_as_ranges() {
    let mut b = Bits::new();
    for c in [0x41, 0x42, 0x43, 0x45, 0x10_ffff] {
        b.put(c, true);
    }
    b.put(0x42, false);
    assert_eq!(
        b.ranges(),
        vec![
            (0x41, 0x41),
            (0x43, 0x43),
            (0x45, 0x45),
            (0x10_ffff, 0x10_ffff)
        ]
    );
}

#[test]
fn nobody_is_read_from_passwd() {
    let passwd = "root:x:0:0::/root:/bin/sh\nnobody:x:65534:65534:Nobody:/:/usr/bin/nologin\n";
    assert_eq!(nobody(passwd), Some((65534, 65534)));
    assert_eq!(nobody("nobody:x:0:0::/:/bin/sh\n"), None);
}

/// A stand-in for vtium's root process: it starts each font helper from
/// `script` on one end of a stream socket, records each start and stop,
/// and reports the exit status the test gives in place of the helper's
/// own.
#[cfg(feature = "split")]
struct StandInRoot {
    script: String,
    fixture: PathBuf,
    reports: std::process::ExitStatus,
    asked: RefCell<Vec<String>>,
    children: RefCell<Vec<std::process::Child>>,
}

#[cfg(feature = "split")]
impl Root for StandInRoot {
    fn start_font(&self) -> std::io::Result<Launched> {
        use rustix::net::{AddressFamily, SocketFlags, SocketType, socketpair};
        let (ours, theirs) = socketpair(
            AddressFamily::UNIX,
            SocketType::STREAM,
            SocketFlags::CLOEXEC,
            None,
        )?;
        let child = Process::new(stand_in_shell())
            .args(["-c", &undumpable(&self.script), "stand-in"])
            .arg(&self.fixture)
            .stdin(std::process::Stdio::from(theirs.try_clone()?))
            .stdout(std::process::Stdio::from(theirs))
            .stderr(std::process::Stdio::null())
            .spawn()?;
        let id = u32::try_from(self.children.borrow().len()).unwrap() + 1;
        self.children.borrow_mut().push(child);
        self.asked.borrow_mut().push(format!("start {id}"));
        let (tell, exit) = mpsc::channel();
        // the stand-in reports its end at once; the reader still waits
        // for the socket to close first
        tell.send(self.reports).unwrap();
        Ok(Launched {
            id,
            socket: ours,
            exit,
        })
    }

    fn stop(&self, id: u32) {
        self.asked.borrow_mut().push(format!("stop {id}"));
    }
}

#[cfg(feature = "split")]
impl Drop for StandInRoot {
    fn drop(&mut self) {
        for child in self.children.get_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

// Under the privilege split, vtium's root process starts the font helper
// and reaps it. The helper's end then counts by the status the root
// process reports: here the helper itself exits 3 at its idle time, and
// the root process reports 0, so no chance is used.
#[cfg(feature = "split")]
#[test]
fn the_root_process_starts_the_font_helper_and_reports_its_end() {
    use std::os::unix::process::ExitStatusExt;
    let mut fixture = frame_bytes(FRAME_GLYPHS, &set(CELL, &['A']));
    fixture.extend(frame_bytes(FRAME_DONE, &[]));
    let mut s = StandIn::new("root", "head -c 1 >/dev/null; cat \"$1\"; exit 3", &fixture);
    let root = Rc::new(StandInRoot {
        script: "head -c 1 >/dev/null; cat \"$1\"; exit 3".to_owned(),
        fixture: s.dir.join("answer"),
        reports: std::process::ExitStatus::from_raw(0),
        asked: RefCell::default(),
        children: RefCell::default(),
    });
    let placeholder = Helper::new(
        s.helper.launcher.clone(),
        Arc::new(|| {}),
        t0(),
        frit("source=builtin:unscii-8\n"),
        None,
        Shared::default(),
        64 << 20,
    );
    let helper = std::mem::replace(&mut s.helper, placeholder);
    s.helper = helper.through_root(Rc::clone(&root) as Rc<dyn Root>);
    let now = t0();
    s.helper.ask(CELL, Band::NONE, &shown(&['A']), now);
    assert_eq!(s.run(now).len(), 1);
    s.until_gone(now);
    assert_eq!(s.helper.failed(), None);
    assert_eq!(s.helper.restarts(), 0, "the root process's status counts");
    assert_eq!(*root.asked.borrow(), ["start 1"]);
    // a helper that runs is stopped through the root process
    s.helper.ask(CELL, Band::NONE, &shown(&['B']), now);
    s.helper.stop();
    assert_eq!(*root.asked.borrow(), ["start 1", "start 2", "stop 2"]);
}
