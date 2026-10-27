//! The pty and the event loop through a real pty, with children that are
//! this binary run again in a role named by `VTIUM_PTY_CHILD`.
//!
//! The binary has its own `main` (`harness = false`): a signal the loop
//! handles is blocked in the thread that adds it, and this way that thread
//! is the only one, so a signal sent to the process waits for the loop.
//! `cargo test -p vtium-platform --test pty <name>` runs the cases whose
//! name contains `<name>`.

use std::cell::Cell;
use std::fs;
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::path::PathBuf;
use std::process::{Command, ExitStatus};
use std::rc::Rc;
use std::time::Duration;

use rustix::termios::{OptionalActions, SpecialCodeIndex, tcgetattr, tcsetattr};
use vtium_platform::calloop::timer::{TimeoutAction, Timer};
use vtium_platform::event_loop::{Loop, Signal};
use vtium_platform::pty::{Consumer, Erase, Pty, Replies, Size};
use vtium_platform::{MOTION_COALESCE_AT, REPLY_HIGH};

const SIZE: Size = Size {
    cols: 80,
    rows: 25,
    cell_width: 8,
    cell_height: 16,
};

fn main() {
    if let Ok(role) = std::env::var("VTIUM_PTY_CHILD") {
        child(&role);
        return;
    }
    let filter: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .collect();
    let cases: [(&str, fn()); 6] = [
        // first: no thread has started yet
        (
            "sigterm_is_served_while_the_loop_waits",
            sigterm_is_served_while_the_loop_waits,
        ),
        (
            "replies_to_a_child_that_does_not_read_stay_bounded",
            replies_to_a_child_that_does_not_read_stay_bounded,
        ),
        (
            "a_child_under_1003_sees_every_button_and_a_bounded_queue",
            a_child_under_1003_sees_every_button_and_a_bounded_queue,
        ),
        (
            "output_and_replies_do_not_depend_on_how_reads_split",
            output_and_replies_do_not_depend_on_how_reads_split,
        ),
        (
            "the_window_size_reaches_the_child",
            the_window_size_reaches_the_child,
        ),
        (
            "the_child_starts_with_no_signal_blocked",
            the_child_starts_with_no_signal_blocked,
        ),
    ];
    let mut ran = 0;
    for (name, case) in cases {
        if !filter.is_empty() && !filter.iter().any(|f| name.contains(f.as_str())) {
            continue;
        }
        print!("test {name} ... ");
        std::io::stdout().flush().ok();
        case();
        println!("ok");
        ran += 1;
    }
    println!("\ntest result: ok. {ran} passed");
}

/// splitmix64: a seeded generator with no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next() % n as u64) as usize
    }
}

/// A seeded stream of text and runs of queries.
fn stream(seed: u64, len: usize) -> Vec<u8> {
    let mut rng = Rng(seed);
    let mut s = Vec::with_capacity(len);
    while s.len() < len {
        if rng.below(5) == 0 {
            s.extend(std::iter::repeat_n(b'?', 1 + rng.below(500)));
        } else {
            for _ in 0..1 + rng.below(2000) {
                let b = rng.below(256) as u8;
                s.push(if b == b'?' { b'.' } else { b });
            }
        }
    }
    s.truncate(len);
    s
}

/// The replies a whole feed gives: `R<n>;` for the n-th `?`.
fn replies_to(stream: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut n = 0;
    for &b in stream {
        if b == b'?' {
            n += 1;
            out.extend_from_slice(format!("R{n};").as_bytes());
        }
    }
    out
}

// ---- the children ----

fn child(role: &str) {
    let stdin = std::io::stdin();
    let mut t = tcgetattr(stdin.as_fd()).expect("tcgetattr");
    t.make_raw();
    tcsetattr(stdin.as_fd(), OptionalActions::Now, &t).expect("tcsetattr");
    let mut out = std::io::stdout();
    match role {
        // Floods queries and never reads.
        "flood" => {
            let _ = out.write_all(&vec![b'?'; 1 << 20]);
            loop {
                std::thread::park();
            }
        }
        // Says it is ready if it leads the pty's session and sees the
        // erase character the host set, reads up to a `Q`, records it and
        // exits.
        "echo" => {
            let leader = rustix::termios::tcgetsid(stdin.as_fd()).expect("tcgetsid")
                == rustix::process::getpid();
            let erase = t.special_codes[SpecialCodeIndex::VERASE] == 0x7f;
            out.write_all(if leader && erase { b"!" } else { b"N" })
                .expect("ready");
            out.flush().expect("flush");
            let got = read_until(|got| got.ends_with(b"Q"));
            fs::write(env_path("VTIUM_PTY_OUT"), got).expect("record");
        }
        // Says it is ready, reads nothing until told to, then reads up to
        // the last button release and records it.
        "late" => {
            out.write_all(b"!").expect("ready");
            out.flush().expect("flush");
            let go = env_path("VTIUM_PTY_GO");
            while !go.exists() {
                nap(10);
            }
            let got = read_until(|got| got.ends_with(LAST_RELEASE));
            fs::write(env_path("VTIUM_PTY_OUT"), got).expect("record");
        }
        // Writes a seeded stream in random pieces while a thread reads the
        // replies, then records them.
        "stream" => {
            let seed: u64 = std::env::var("VTIUM_PTY_SEED")
                .expect("seed")
                .parse()
                .expect("seed");
            let s = stream(seed, STREAM_LEN);
            let want = replies_to(&s).len();
            let reader = std::thread::spawn(move || read_until(|got| got.len() >= want));
            let mut rng = Rng(seed ^ 0x5eed);
            let mut at = 0;
            while at < s.len() {
                let n = (1 + rng.below(4096)).min(s.len() - at);
                out.write_all(&s[at..at + n]).expect("write");
                out.flush().expect("flush");
                at += n;
                if rng.below(8) == 0 {
                    nap(1);
                }
            }
            let got = reader.join().expect("reader");
            fs::write(env_path("VTIUM_PTY_OUT"), got).expect("record");
        }
        // Reports its window size, then waits for a SIGWINCH's new size.
        // Reports the signals blocked in it, from /proc.
        "sigmask" => {
            let status = fs::read_to_string("/proc/self/status").expect("status");
            let line = status
                .lines()
                .find(|l| l.starts_with("SigBlk:"))
                .expect("SigBlk");
            out.write_all(format!("{line}!").as_bytes()).expect("write");
            out.flush().expect("flush");
            read_until(|got| got.ends_with(b"Q"));
        }
        "winsize" => {
            let first = rustix::termios::tcgetwinsize(stdin.as_fd()).expect("size");
            let line = format!(
                "{}x{} {}x{}!",
                first.ws_col, first.ws_row, first.ws_xpixel, first.ws_ypixel
            );
            out.write_all(line.as_bytes()).expect("write");
            out.flush().expect("flush");
            loop {
                let now = rustix::termios::tcgetwinsize(stdin.as_fd()).expect("size");
                if now.ws_col != first.ws_col {
                    let line = format!(
                        "{}x{} {}x{}!",
                        now.ws_col, now.ws_row, now.ws_xpixel, now.ws_ypixel
                    );
                    out.write_all(line.as_bytes()).expect("write");
                    out.flush().expect("flush");
                    break;
                }
                nap(5);
            }
            read_until(|got| got.ends_with(b"Q"));
        }
        _ => panic!("no child role {role}"),
    }
}

fn read_until(done: impl Fn(&[u8]) -> bool) -> Vec<u8> {
    let mut got = Vec::new();
    let mut buf = [0u8; 4096];
    let mut stdin = std::io::stdin();
    while !done(&got) {
        let n = stdin.read(&mut buf).expect("read");
        assert!(n > 0, "the pty closed");
        got.extend_from_slice(&buf[..n]);
    }
    got
}

#[expect(
    clippy::disallowed_methods,
    reason = "a test child paces itself; time reaches no state under test"
)]
fn nap(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).expect(name))
}

/// A scratch directory for one case.
fn scratch(case: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("vtium-platform-pty-{}-{case}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn role(name: &str) -> Command {
    let mut c = Command::new(std::env::current_exe().expect("current exe"));
    c.env("VTIUM_PTY_CHILD", name);
    c
}

// ---- the host ----

/// The terminal stand-in: records what it takes, replies `R<n>;` to the
/// n-th `?`, and takes a random part of what it is offered when it has a
/// generator.
#[derive(Default)]
struct Term {
    taken: Vec<u8>,
    queries: usize,
    reply_len_max: usize,
    takes: Option<Rng>,
    exited: Option<ExitStatus>,
}

impl Consumer for Term {
    fn output(&mut self, output: &[u8], replies: &mut Replies<'_>) -> usize {
        let limit = match &mut self.takes {
            Some(rng) => rng.below(output.len() + 1),
            None => output.len(),
        };
        for (i, &b) in output[..limit].iter().enumerate() {
            self.taken.push(b);
            if b == b'?' {
                self.queries += 1;
                let reply = format!("R{};", self.queries);
                self.reply_len_max = self.reply_len_max.max(reply.len());
                replies.write(reply.as_bytes());
                if replies.full() {
                    return i + 1;
                }
            }
        }
        limit
    }

    fn exited(&mut self, status: ExitStatus) {
        self.exited = Some(status);
    }
}

#[derive(Default)]
struct Host {
    pty: Option<Pty>,
    term: Term,
    ticks: u64,
    usr1: u64,
}

fn parts(h: &mut Host) -> Option<(&mut Pty, &mut Term)> {
    let term = &mut h.term;
    h.pty.as_mut().map(|p| (p, term))
}

/// A loop with a timer that ticks every 5 ms and a SIGUSR1 counter.
fn host_loop() -> Loop<Host> {
    let mut lp: Loop<Host> = Loop::new().expect("loop");
    lp.handle()
        .insert_source(
            Timer::from_duration(Duration::from_millis(5)),
            |_, _, h: &mut Host| {
                h.ticks += 1;
                TimeoutAction::ToDuration(Duration::from_millis(5))
            },
        )
        .expect("timer");
    lp.on_signal(Signal::SIGUSR1, |h: &mut Host| h.usr1 += 1)
        .expect("SIGUSR1");
    lp
}

fn open(lp: &Loop<Host>, h: &mut Host, command: Command) {
    let mut pty = Pty::spawn(command, SIZE, Erase::Delete).expect("spawn");
    pty.insert(&lp.handle(), parts).expect("insert");
    h.pty = Some(pty);
}

/// Turns the loop until `done`, at most `turns` turns of up to 10 ms.
fn turn_until(lp: &mut Loop<Host>, h: &mut Host, turns: usize, done: impl Fn(&Host) -> bool) {
    for _ in 0..turns {
        if done(h) {
            return;
        }
        lp.dispatch(Some(Duration::from_millis(10)), h)
            .expect("dispatch");
    }
    assert!(done(h), "the loop did not get there in {turns} turns");
}

fn raise(signal: rustix::process::Signal) {
    rustix::process::kill_process(rustix::process::getpid(), signal).expect("kill");
}

// ---- the cases ----

// vtium/vtium#20: SIGTERM served while the loop waits.
fn sigterm_is_served_while_the_loop_waits() {
    struct Data {
        term: Rc<Cell<bool>>,
    }
    let mut lp: Loop<Data> = Loop::new().expect("loop");
    let stop = lp.stopper();
    lp.on_signal(Signal::SIGTERM, move |d: &mut Data| {
        d.term.set(true);
        stop.stop();
    })
    .expect("SIGTERM");
    // The sender starts after the mask is set, so it inherits it, and the
    // signal can only wait for the loop's signalfd.
    let sender = std::thread::spawn(|| {
        nap(100);
        raise(rustix::process::Signal::TERM);
    });
    let mut data = Data {
        term: Rc::new(Cell::new(false)),
    };
    // No timeout and no other source: the loop wakes for the signal alone.
    lp.run(&mut data, |_| {}).expect("run");
    sender.join().expect("sender");
    assert!(data.term.get());
}

// vtium/vtium#20, aaronsb/kmscon#245: the reply bound, timers and signals
// served throughout, and a pty reopened with an empty queue.
fn replies_to_a_child_that_does_not_read_stay_bounded() {
    let mut lp = host_loop();
    let mut h = Host::default();
    open(&lp, &mut h, role("flood"));

    // Turn until the reading has stalled: 50 turns with nothing taken.
    let mut most = 0;
    let mut still = 0;
    let mut last = 0;
    for _ in 0..5000 {
        lp.dispatch(Some(Duration::from_millis(10)), &mut h)
            .expect("dispatch");
        let queued = h.pty.as_ref().expect("pty").queued_replies();
        most = most.max(queued);
        if h.term.taken.len() == last && queued >= REPLY_HIGH {
            still += 1;
            if still == 50 {
                break;
            }
        } else {
            still = 0;
            last = h.term.taken.len();
        }
    }
    assert_eq!(still, 50, "the reading never stalled");
    assert!(most >= REPLY_HIGH);
    assert!(
        most < REPLY_HIGH + h.term.reply_len_max,
        "{most} bytes of replies queued"
    );
    assert!(h.term.taken.len() < 1 << 20, "the flood was read whole");

    // The stall stalls the pty alone: timers and signals are served.
    let (ticks, taken) = (h.ticks, h.term.taken.len());
    raise(rustix::process::Signal::USR1);
    turn_until(&mut lp, &mut h, 500, |h| {
        h.usr1 == 1 && h.ticks >= ticks + 10
    });
    assert_eq!(h.term.taken.len(), taken);

    // The session ends; the next one starts with nothing queued and its
    // child reads only what is written to it.
    h.pty = None;
    let dir = scratch("reopen");
    let mut echo = role("echo");
    echo.env("VTIUM_PTY_OUT", dir.join("out"));
    h.term = Term::default();
    open(&lp, &mut h, echo);
    assert_eq!(h.pty.as_ref().expect("pty").queued(), 0);
    turn_until(&mut lp, &mut h, 500, |h| !h.term.taken.is_empty());
    assert!(
        h.term.taken == b"!",
        "the child does not lead the pty's session, or VERASE is not DEL: {}",
        String::from_utf8_lossy(&h.term.taken)
    );
    h.pty.as_mut().expect("pty").write(b"hello Q");
    turn_until(&mut lp, &mut h, 500, |h| h.term.exited.is_some());
    assert!(h.term.exited.expect("exit").success());
    assert_eq!(fs::read(dir.join("out")).expect("out"), b"hello Q");
    let _ = fs::remove_dir_all(dir);
}

const LAST_RELEASE: &[u8] = b"\x1b[<0;999;999m";

// vtium/vtium#20, aaronsb/kmscon#247: under any-motion tracking (mode
// 1003) a child that does not read sees a bounded queue, every button
// report in order, and the latest position.
fn a_child_under_1003_sees_every_button_and_a_bounded_queue() {
    let dir = scratch("1003");
    let mut lp = host_loop();
    let mut h = Host::default();
    let mut late = role("late");
    late.env("VTIUM_PTY_OUT", dir.join("out"))
        .env("VTIUM_PTY_GO", dir.join("go"));
    open(&lp, &mut h, late);
    turn_until(&mut lp, &mut h, 500, |h| h.term.taken.ends_with(b"!"));

    let mut buttons: Vec<Vec<u8>> = Vec::new();
    let mut last_motion = Vec::new();
    let mut motion_bytes = 0;
    let mut most = 0;
    for i in 0..40_000u32 {
        let (x, y) = (1 + i % 200, 1 + i % 60);
        let pty = h.pty.as_mut().expect("pty");
        let motion = format!("\x1b[<35;{x};{y}M").into_bytes();
        motion_bytes += motion.len();
        pty.write_motion(&motion);
        last_motion = motion;
        if i % 400 == 399 {
            let press = format!("\x1b[<0;{x};{y}M").into_bytes();
            let release = format!("\x1b[<0;{x};{y}m").into_bytes();
            pty.write(&press);
            pty.write(&release);
            buttons.push(press);
            buttons.push(release);
        }
        most = most.max(pty.queued());
        if i % 100 == 0 {
            lp.dispatch(Some(Duration::ZERO), &mut h).expect("dispatch");
        }
    }
    h.pty.as_mut().expect("pty").write(LAST_RELEASE);
    buttons.push(LAST_RELEASE.to_vec());

    // Past the bound only button reports and one motion report after each
    // of them can be queued.
    let button_bytes: usize = buttons.iter().map(Vec::len).sum();
    let bound = MOTION_COALESCE_AT + button_bytes + (buttons.len() + 1) * 16;
    assert!(most <= bound, "{most} bytes queued, bound {bound}");
    assert!(
        motion_bytes > 10 * bound,
        "the motion would not have passed it"
    );

    fs::write(dir.join("go"), b"").expect("go");
    turn_until(&mut lp, &mut h, 3000, |h| h.term.exited.is_some());
    assert!(h.term.exited.expect("exit").success());
    let got = fs::read(dir.join("out")).expect("out");
    let reports = sgr_reports(&got);
    let seen: Vec<&[u8]> = reports
        .iter()
        .copied()
        .filter(|r| !r.starts_with(b"\x1b[<35;"))
        .collect();
    let sent: Vec<&[u8]> = buttons.iter().map(Vec::as_slice).collect();
    assert_eq!(seen, sent, "the child missed or reordered a button report");
    let last_seen_motion = reports
        .iter()
        .rev()
        .find(|r| r.starts_with(b"\x1b[<35;"))
        .expect("a motion report");
    assert_eq!(*last_seen_motion, last_motion.as_slice());
    let _ = fs::remove_dir_all(dir);
}

/// The SGR mouse reports in `bytes`, each `ESC [ <` up to `M` or `m`.
fn sgr_reports(bytes: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut rest = bytes;
    while let Some(at) = rest.windows(3).position(|w| w == b"\x1b[<") {
        let tail = &rest[at..];
        let end = tail
            .iter()
            .position(|&b| b == b'M' || b == b'm')
            .expect("a whole report");
        out.push(&tail[..=end]);
        rest = &tail[end + 1..];
    }
    out
}

const STREAM_LEN: usize = 400_000;

// docs/architecture.md, "Determinism": chunking invariance through a real
// pty, the child writing in random pieces and the consumer taking random
// parts.
fn output_and_replies_do_not_depend_on_how_reads_split() {
    for seed in [1u64, 2, 3] {
        let dir = scratch(&format!("stream-{seed}"));
        let mut lp = host_loop();
        let mut h = Host::default();
        h.term.takes = Some(Rng(seed.wrapping_mul(77)));
        let mut child = role("stream");
        child
            .env("VTIUM_PTY_OUT", dir.join("out"))
            .env("VTIUM_PTY_SEED", seed.to_string());
        open(&lp, &mut h, child);
        turn_until(&mut lp, &mut h, 30_000, |h| h.term.exited.is_some());
        assert!(h.term.exited.expect("exit").success(), "seed {seed}");
        let s = stream(seed, STREAM_LEN);
        let differ = h.term.taken.iter().zip(&s).position(|(a, b)| a != b);
        assert!(
            h.term.taken == s,
            "seed {seed}: the consumer saw other bytes: {} of {}, first difference at {differ:?}",
            h.term.taken.len(),
            s.len()
        );
        let got = fs::read(dir.join("out")).expect("out");
        assert!(
            got == replies_to(&s),
            "seed {seed}: the child read other replies"
        );
        let _ = fs::remove_dir_all(dir);
    }
}

// pty.c:770-783, 941-964
fn the_window_size_reaches_the_child() {
    let mut lp = host_loop();
    let mut h = Host::default();
    open(&lp, &mut h, role("winsize"));
    turn_until(&mut lp, &mut h, 500, |h| h.term.taken.ends_with(b"!"));
    assert_eq!(h.term.taken, b"80x25 640x400!");
    let big = Size {
        cols: 300,
        rows: 100,
        cell_width: 300,
        cell_height: 16,
    };
    h.pty.as_mut().expect("pty").resize(big).expect("resize");
    turn_until(&mut lp, &mut h, 500, |h| {
        h.term.taken.len() > 14 && h.term.taken.ends_with(b"!")
    });
    // 300 cells of 300 pixels pass 16 bits: the pixel width is 65535.
    assert_eq!(&h.term.taken[14..], b"300x100 65535x1600!");
    h.pty.as_mut().expect("pty").write(b"Q");
    turn_until(&mut lp, &mut h, 500, |h| h.term.exited.is_some());
}

// The loop blocks the signals it handles; login, the shell and everything
// they start must not inherit that.
fn the_child_starts_with_no_signal_blocked() {
    let mut lp = host_loop();
    let mut h = Host::default();
    open(&lp, &mut h, role("sigmask"));
    turn_until(&mut lp, &mut h, 500, |h| h.term.taken.ends_with(b"!"));
    assert_eq!(
        String::from_utf8_lossy(&h.term.taken),
        "SigBlk:\t0000000000000000!"
    );
    h.pty.as_mut().expect("pty").write(b"Q");
    turn_until(&mut lp, &mut h, 500, |h| h.term.exited.is_some());
}
