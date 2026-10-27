//! The channel against a scripted child: the reply bound, the stall and
//! its end, pauses, the per-turn read bound, and the chunking-invariance
//! property (docs/architecture.md, "Determinism"): with random read sizes,
//! random takes by the consumer, a child that reads its input at random
//! and pauses at random, the consumer sees exactly the child's bytes and
//! the child reads exactly the replies a whole feed gives. A failure names
//! its seed.
//!
//! The script follows epoll's edge-triggered rule: output that arrives is
//! reported once, input the child reads reports the pty writable once, and
//! a rearm reports what is there now. Anything else the channel would need
//! to see again is lost, so a missing rearm shows up as a stream that
//! never finishes.

#![expect(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    reason = "test arithmetic on bounded values"
)]

use std::cell::RefCell;
use std::io;
use std::process::ExitStatus;
use std::rc::Rc;

use super::*;

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

    /// In `0..n`; 0 when `n` is 0.
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next() % n as u64) as usize
    }

    fn chance(&mut self, one_in: usize) -> bool {
        self.below(one_in) == 0
    }
}

/// The child as the pty sees it.
struct Child {
    /// Everything the child will write.
    stream: Vec<u8>,
    /// How much of `stream` it has written.
    written: usize,
    /// How much of that the pty has read.
    read: usize,
    /// Written by the pty, not yet read by the child.
    input: Vec<u8>,
    /// The kernel's room for `input`.
    room: usize,
    /// What the child read, in order.
    got: Vec<u8>,
    /// Read sizes are at most this.
    max_read: usize,
    /// Reads take all they can, rather than a random part.
    whole_reads: bool,
    rng: Rng,
}

impl Child {
    fn new(stream: Vec<u8>, max_read: usize, room: usize, seed: u64) -> Self {
        Child {
            stream,
            written: 0,
            read: 0,
            input: Vec::new(),
            room,
            got: Vec::new(),
            max_read,
            whole_reads: false,
            rng: Rng(seed),
        }
    }

    /// Writes up to `n` more bytes of its stream; true when it wrote any.
    fn write(&mut self, n: usize) -> bool {
        let before = self.written;
        self.written = (self.written + n).min(self.stream.len());
        self.written > before
    }

    /// Reads up to `n` bytes of its input; true when it read any.
    fn read_input(&mut self, n: usize) -> bool {
        let n = n.min(self.input.len());
        self.got.extend(self.input.drain(..n));
        n > 0
    }
}

/// The scripted master: the child's side is shared with the test.
struct Script(Rc<RefCell<Child>>);

impl Endpoint for Script {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut c = self.0.borrow_mut();
        let ready = c.written - c.read;
        if ready == 0 {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let cap = ready.min(buf.len()).min(c.max_read);
        let n = if c.whole_reads {
            cap
        } else {
            1 + c.rng.below(cap)
        };
        let from = c.read;
        buf[..n].copy_from_slice(&c.stream[from..from + n]);
        c.read += n;
        Ok(n)
    }

    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut c = self.0.borrow_mut();
        let room = c.room - c.input.len();
        if room == 0 {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let n = 1 + c.rng.below(room.min(bytes.len()));
        c.input.extend_from_slice(&bytes[..n]);
        Ok(n)
    }
}

/// A consumer that replies `R<n>;` to the n-th `?` and takes a random
/// part of what it is offered, stopping when the replies are full.
struct Term {
    taken: Vec<u8>,
    queries: usize,
    takes: Rng,
    /// Ignores [`Replies::full`], as a consumer must not.
    reckless: bool,
}

impl Term {
    fn new(seed: u64) -> Self {
        Term {
            taken: Vec::new(),
            queries: 0,
            takes: Rng(seed),
            reckless: false,
        }
    }
}

impl Consumer for Term {
    fn output(&mut self, output: &[u8], replies: &mut Replies<'_>) -> usize {
        let limit = self.takes.below(output.len() + 1);
        for (i, &b) in output[..limit].iter().enumerate() {
            self.taken.push(b);
            if b == b'?' {
                self.queries += 1;
                replies.write(format!("R{};", self.queries).as_bytes());
                if replies.full() && !self.reckless {
                    return i + 1;
                }
            }
        }
        limit
    }

    fn exited(&mut self, _: ExitStatus) {}
}

/// The replies a whole feed gives to `stream`.
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

/// The longest reply a test stream can draw: `R` and `;` around a count.
const REPLY_LEN_MAX: usize = 10;

/// The loop around a channel: pending edges and the owed feed.
struct Driver {
    ch: Channel<Script>,
    child: Rc<RefCell<Child>>,
    readable: bool,
    writable: bool,
    feed: bool,
    /// The queued replies reached [`REPLY_HIGH`] at some point.
    stalled: bool,
}

impl Driver {
    fn new(child: Child) -> Self {
        let child = Rc::new(RefCell::new(child));
        Driver {
            ch: Channel::new(Script(Rc::clone(&child))),
            child,
            readable: false,
            writable: false,
            feed: false,
            stalled: false,
        }
    }

    fn apply(&mut self, next: Next, seed: u64) {
        if next.feed {
            assert!(!self.feed, "seed {seed}: a second feed was owed");
            self.feed = true;
        }
        if next.rearm {
            let c = self.child.borrow();
            self.readable |= c.written > c.read;
            self.writable |= c.input.len() < c.room;
        }
    }

    /// One turn of the loop: the pending edges, then the owed feed.
    fn turn(&mut self, term: &mut Term, seed: u64) {
        if self.readable || self.writable {
            let (r, w) = (self.readable, self.writable);
            self.readable = false;
            self.writable = false;
            let next = self.ch.ready(r, w, term);
            self.apply(next, seed);
        }
        if self.feed {
            self.feed = false;
            let next = self.ch.feed_held(term);
            self.apply(next, seed);
        }
        let queued = self.ch.queued_replies();
        self.stalled |= queued >= REPLY_HIGH;
        assert!(
            queued < REPLY_HIGH + REPLY_LEN_MAX,
            "seed {seed}: {queued} bytes of replies queued"
        );
    }

    fn child_writes(&mut self, n: usize) {
        if self.child.borrow_mut().write(n) {
            self.readable = true;
        }
    }

    fn child_reads(&mut self, n: usize) {
        if self.child.borrow_mut().read_input(n) {
            self.writable = true;
        }
    }

    fn done(&self, term: &Term) -> bool {
        let c = self.child.borrow();
        term.taken.len() == c.stream.len() && self.ch.queued() == 0 && c.input.is_empty()
    }
}

/// A stream of text and runs of queries, some long enough to fill the
/// replies past [`REPLY_HIGH`] while the child does not read.
fn stream(rng: &mut Rng, len: usize) -> Vec<u8> {
    let mut s = Vec::with_capacity(len);
    while s.len() < len {
        if rng.chance(4) {
            let run = 1 + rng.below(12_000);
            s.extend(std::iter::repeat_n(b'?', run));
        } else {
            let run = 1 + rng.below(3000);
            for _ in 0..run {
                let b = rng.below(256) as u8;
                s.push(if b == b'?' { b'.' } else { b });
            }
        }
    }
    s.truncate(len);
    s
}

/// Runs one seeded case. Returns whether the replies stalled the reading.
fn chunking_case(seed: u64) -> bool {
    let mut rng = Rng(seed);
    let len = 20_000 + rng.below(160_000);
    let s = stream(&mut rng, len);
    let max_read = match rng.below(4) {
        0 => 1,
        1 => 1 + rng.below(64),
        _ => READ_MAX,
    };
    let room = 1 + rng.below(8192);
    let mut d = Driver::new(Child::new(s.clone(), max_read, room, rng.next()));
    let mut term = Term::new(rng.next());
    let mut asleep = false;
    let mut keys = 0usize;

    // The child writes, reads, sleeps and wakes at random; the host types
    // and pauses at random.
    for _ in 0..200_000 {
        match rng.below(16) {
            0..=5 => {
                let n = 1 + rng.below(8192);
                d.child_writes(n);
            }
            6..=8 if !asleep => {
                let n = 1 + rng.below(4096);
                d.child_reads(n);
            }
            9 if rng.chance(20) => asleep = !asleep,
            10 if rng.chance(10) => {
                d.ch.send(Kind::User, b"k");
                keys += 1;
            }
            11 if rng.chance(30) => {
                if d.ch.paused {
                    let next = d.ch.resume();
                    d.apply(next, seed);
                } else {
                    d.ch.pause();
                }
            }
            _ => d.turn(&mut term, seed),
        }
    }

    // Then it writes the rest and reads everything, and nothing pauses:
    // every byte reaches the consumer, and every reply the child.
    if d.ch.paused {
        let next = d.ch.resume();
        d.apply(next, seed);
    }
    d.child_writes(usize::MAX / 2);
    let mut turns = 0;
    while !d.done(&term) {
        d.turn(&mut term, seed);
        d.child_reads(usize::MAX);
        turns += 1;
        assert!(
            turns < 10_000_000,
            "seed {seed}: stuck with {} of {} bytes taken, {} queued",
            term.taken.len(),
            s.len(),
            d.ch.queued()
        );
    }

    assert!(term.taken == s, "seed {seed}: the consumer saw other bytes");
    let got = d.child.borrow().got.clone();
    let typed = got.iter().filter(|&&b| b == b'k').count();
    let replies: Vec<u8> = got.into_iter().filter(|&b| b != b'k').collect();
    assert_eq!(typed, keys, "seed {seed}: keys lost");
    assert!(
        replies == replies_to(&s),
        "seed {seed}: the child read other replies"
    );
    d.stalled
}

// covers the Determinism rule "Chunking invariance" for the pty
#[test]
fn what_the_consumer_sees_and_replies_does_not_depend_on_reads() {
    let mut stalls = 0;
    for seed in 0..48 {
        if chunking_case(seed) {
            stalls += 1;
        }
    }
    // The cases reach the stall, so it is part of what they compare.
    assert!(stalls >= 4, "only {stalls} cases stalled on replies");
}

/// A child that never reads, flooding queries.
fn flood(n: usize) -> Driver {
    let mut d = Driver::new(Child::new(vec![b'?'; n], READ_MAX, 4096, 7));
    d.child_writes(n);
    d
}

/// A consumer that takes all it is offered unless the replies fill.
fn eager() -> Term {
    let mut t = Term::new(0);
    t.takes = Rng(u64::MAX);
    t
}

impl Consumer for Vec<u8> {
    fn output(&mut self, output: &[u8], _: &mut Replies<'_>) -> usize {
        self.extend_from_slice(output);
        output.len()
    }

    fn exited(&mut self, _: ExitStatus) {}
}

/// Takes everything, always, replying to each query.
struct Whole(Term);

impl Consumer for Whole {
    fn output(&mut self, output: &[u8], replies: &mut Replies<'_>) -> usize {
        for &b in output {
            self.0.taken.push(b);
            if b == b'?' {
                self.0.queries += 1;
                replies.write(format!("R{};", self.0.queries).as_bytes());
                if replies.full() && !self.0.reckless {
                    return self.0.taken.len();
                }
            }
        }
        output.len()
    }

    fn exited(&mut self, _: ExitStatus) {}
}

// aaronsb/kmscon#245: a child that queries without reading stalls itself.
#[test]
fn replies_to_a_child_that_does_not_read_stop_the_reading() {
    let mut d = flood(1_000_000);
    let mut term = Whole(eager());
    for _ in 0..1000 {
        d.turn(&mut term.0, 1);
        d.readable = true;
    }
    let queued = d.ch.queued_replies();
    assert!(queued >= REPLY_HIGH, "{queued}");
    assert!(queued < REPLY_HIGH + REPLY_LEN_MAX, "{queued}");
    let taken = term.0.taken.len();
    assert!(taken < 1_000_000, "the flood was read whole");
    // Further edges read nothing.
    let next = d.ch.ready(true, false, &mut term);
    assert_eq!(next, Next::default());
    assert_eq!(term.0.taken.len(), taken);
}

#[test]
fn reading_resumes_once_the_child_reads_its_replies() {
    let mut d = flood(1_000_000);
    let mut term = eager();
    for _ in 0..1000 {
        d.turn(&mut term, 2);
        d.readable = true;
    }
    let taken = term.taken.len();
    assert!(d.ch.queued_replies() >= REPLY_HIGH);
    // Each read frees at most the kernel's room, 4096 bytes: the stall
    // holds until the replies drain below the low mark.
    let next = loop {
        d.child_reads(usize::MAX);
        let next = d.ch.ready(false, true, &mut term);
        if next.rearm {
            break next;
        }
        assert_eq!(next, Next::default());
        assert!(d.ch.queued_replies() >= REPLY_LOW);
    };
    assert!(d.ch.queued_replies() < REPLY_LOW);
    d.apply(next, 2);
    d.turn(&mut term, 2);
    assert!(term.taken.len() > taken);
}

/// Ignores [`Replies::full`] and answers each byte with 64 bytes, as an
/// XTGETTCAP flood can.
struct Amplifier;

impl Consumer for Amplifier {
    fn output(&mut self, output: &[u8], replies: &mut Replies<'_>) -> usize {
        for _ in output {
            replies.write(&[b'A'; 64]);
        }
        output.len()
    }

    fn exited(&mut self, _: ExitStatus) {}
}

// pty.c:479-484: past REPLY_MAX replies are dropped until the drain.
#[test]
fn a_consumer_that_ignores_the_stall_loses_replies_past_the_ceiling() {
    let mut d = flood(READ_MAX);
    d.child.borrow_mut().whole_reads = true;
    let next = d.ch.ready(true, false, &mut Amplifier);
    assert_eq!(next, Next::default());
    let queued = d.ch.queued_replies();
    assert!(queued <= REPLY_MAX, "{queued}");
    assert!(queued > REPLY_MAX - 64, "{queued}");
    // A later reply is dropped too, though it would fit.
    d.child_reads(64);
    let _ = d.ch.ready(false, true, &mut Amplifier);
    let queued = d.ch.queued_replies();
    d.ch.send(Kind::Reply, b"R");
    assert_eq!(d.ch.queued_replies(), queued);
    // Once the child reads them below the low mark, replies are kept.
    while d.ch.queued_replies() >= REPLY_LOW {
        d.child_reads(usize::MAX);
        let _ = d.ch.ready(false, true, &mut Amplifier);
    }
    let queued = d.ch.queued_replies();
    d.child.borrow_mut().room = 0;
    d.ch.send(Kind::Reply, b"R");
    assert_eq!(d.ch.queued_replies(), queued + 1);
}

#[test]
fn a_pause_holds_output_until_the_resume() {
    let mut d = Driver::new(Child::new(b"abcdef".to_vec(), 2, 4096, 4));
    let mut out: Vec<u8> = Vec::new();
    d.ch.pause();
    d.child_writes(6);
    let next = d.ch.ready(true, false, &mut out);
    assert_eq!(next, Next::default());
    assert!(out.is_empty());
    let next = d.ch.resume();
    assert!(next.rearm);
    d.apply(next, 4);
    while d.readable || d.feed {
        let (r, f) = (d.readable, d.feed);
        d.readable = false;
        d.feed = false;
        let mut next = Next::default();
        if r {
            next = d.ch.ready(true, false, &mut out);
        }
        if f {
            next = next.or(d.ch.feed_held(&mut out));
        }
        d.apply(next, 4);
    }
    assert_eq!(out, b"abcdef");
}

#[test]
fn a_held_part_is_fed_on_a_later_turn() {
    struct Halves(Vec<u8>);
    impl Consumer for Halves {
        fn output(&mut self, output: &[u8], _: &mut Replies<'_>) -> usize {
            let n = output.len().div_ceil(2);
            self.0.extend_from_slice(&output[..n]);
            n
        }
        fn exited(&mut self, _: ExitStatus) {}
    }
    let mut d = Driver::new(Child::new(b"0123456789".to_vec(), READ_MAX, 4096, 5));
    d.child.borrow_mut().max_read = 10;
    d.child.borrow_mut().rng = Rng(u64::MAX - 9);
    d.child_writes(10);
    let mut h = Halves(Vec::new());
    let next = d.ch.ready(true, false, &mut h);
    assert!(next.feed && !next.rearm);
    let taken = h.0.len();
    assert!(taken < 10);
    // While a part is held, an edge reads nothing more.
    assert_eq!(d.ch.ready(true, false, &mut h), Next::default());
    let mut next = d.ch.feed_held(&mut h);
    while next.feed {
        next = d.ch.feed_held(&mut h);
    }
    assert!(next.rearm, "the last part rearms for what came meanwhile");
    assert_eq!(h.0.len(), d.child.borrow().read);
}

// pty.c:627-651
#[test]
fn a_turn_reads_at_most_reads_per_turn_times() {
    let n = READS_PER_TURN + 5;
    let mut d = Driver::new(Child::new(vec![b'x'; n], 1, 4096, 6));
    d.child_writes(n);
    let mut out: Vec<u8> = Vec::new();
    let next = d.ch.ready(true, false, &mut out);
    assert_eq!(out.len(), READS_PER_TURN);
    assert!(next.rearm);
    let next = d.ch.ready(true, false, &mut out);
    assert_eq!(out.len(), n);
    assert!(!next.rearm);
}

#[test]
fn user_bytes_reach_a_child_that_reads_in_order() {
    let mut d = flood(0);
    let mut out: Vec<u8> = Vec::new();
    d.ch.send(Kind::User, b"hello");
    d.ch.send(Kind::Motion, b"<35;1;1M");
    d.ch.send(Kind::User, b" world");
    while d.ch.queued() > 0 {
        let _ = d.ch.ready(false, true, &mut out);
    }
    assert_eq!(d.child.borrow().input, b"hello<35;1;1M world");
}

#[test]
fn a_paste_into_a_child_that_does_not_read_is_queued_whole() {
    let mut d = flood(0);
    let paste = vec![b'p'; 3 * REPLY_MAX];
    d.ch.send(Kind::User, &paste);
    let c = d.child.borrow();
    assert_eq!(c.input.len() + d.ch.queued(), paste.len());
    // A paste never stalls reading.
    assert!(!d.ch.stalled());
}
