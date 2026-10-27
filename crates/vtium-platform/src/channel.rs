//! The two directions of a pty, as state with no event loop: the child's
//! output read and fed to a [`Consumer`] in parts it chooses, and the bytes
//! written to the child, queued while it does not read.
//!
//! [`Channel`] does its I/O through an [`Endpoint`], the pty master in
//! [`crate::pty`] and a scripted stand-in in the tests, and tells the loop
//! what it needs next as a [`Next`]. What a consumer is fed, and every
//! reply it writes, depend only on the bytes the child sent: a stall
//! delays them, and nothing is dropped while the consumer keeps the
//! contract on [`Consumer::output`].

use std::io;

use crate::bounds::{READ_MAX, READS_PER_TURN, REPLY_HIGH, REPLY_LOW, REPLY_MAX};
use crate::queue::{Kind, WriteQueue};

/// What reads the child's output: the terminal.
pub trait Consumer {
    /// Takes bytes from the front of `output` and returns how many it took.
    ///
    /// Bytes not taken are offered again, from where it stopped, on a later
    /// turn of the loop, so the loop serves its other sources in between.
    /// Replies to the child go to `replies`. Once [`Replies::full`] is
    /// true the consumer returns, at the latest after the sequence whose
    /// reply filled it: the pty then reads nothing more until the child
    /// has read its replies.
    fn output(&mut self, output: &[u8], replies: &mut Replies<'_>) -> usize;

    /// The child exited with `status`, and the output it left has been
    /// read. The host closes the pty by dropping it, outside the loop's
    /// dispatch.
    fn exited(&mut self, status: std::process::ExitStatus);
}

/// The replies a [`Consumer`] writes while it reads.
pub struct Replies<'a>(&'a mut dyn ReplySink);

impl Replies<'_> {
    /// Writes a reply to the child, behind everything queued for it.
    pub fn write(&mut self, reply: &[u8]) {
        self.0.reply(reply);
    }

    /// The child has not read [`crate::REPLY_HIGH`] bytes of
    /// replies: the consumer stops taking output.
    pub fn full(&self) -> bool {
        self.0.full()
    }
}

/// Where [`Replies`] go.
trait ReplySink {
    fn reply(&mut self, reply: &[u8]);
    fn full(&self) -> bool;
}

/// The pty master, or a stand-in for it.
pub(crate) trait Endpoint {
    /// Reads the child's output; `WouldBlock` when there is none.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
    /// Writes to the child; `WouldBlock` when it is not reading.
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize>;
}

/// What the loop does for a channel after a call.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[must_use]
pub(crate) struct Next {
    /// Renew the pty's edge-triggered registration, so output that came
    /// while it was not read is reported again.
    pub(crate) rearm: bool,
    /// Call [`Channel::feed_held`] on a later turn.
    pub(crate) feed: bool,
}

impl Next {
    pub(crate) fn or(self, other: Next) -> Next {
        Next {
            rearm: self.rearm || other.rearm,
            feed: self.feed || other.feed,
        }
    }
}

/// The writing half: the queue and its stall.
struct Writer<E> {
    end: E,
    queue: WriteQueue,
    /// Replies reached [`REPLY_HIGH`]: nothing is read or fed.
    backlog: bool,
    /// A reply passed [`REPLY_MAX`]: replies are dropped until the drain.
    dropping: bool,
    /// The child exited: replies are dropped and never stall the reading.
    orphaned: bool,
}

impl<E: Endpoint> Writer<E> {
    /// Writes `bytes` as `kind`, straight to the child when nothing is
    /// queued, and queues what it does not take.
    fn send(&mut self, kind: Kind, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        if kind == Kind::Reply {
            if self.orphaned {
                return;
            }
            if self.dropping || self.queue.replies().saturating_add(bytes.len()) > REPLY_MAX {
                self.dropping = true;
                return;
            }
        }
        let mut rest = bytes;
        if self.queue.is_empty() {
            let n = write_some(&mut self.end, rest);
            rest = rest.get(n..).unwrap_or_default();
            if rest.is_empty() {
                return;
            }
            if n > 0 && kind == Kind::Motion {
                // a report the child has begun to read is never replaced
                self.queue.push(Kind::User, rest);
                return;
            }
        }
        self.queue.push(kind, rest);
        if self.queue.replies() >= REPLY_HIGH {
            self.backlog = true;
        }
    }

    /// Writes what is queued until the child stops reading. Returns true
    /// when the replies drained below [`REPLY_LOW`] and so ended a backlog.
    fn flush(&mut self) -> bool {
        loop {
            let front = self.queue.front();
            if front.is_empty() {
                break;
            }
            let n = write_some(&mut self.end, front);
            if n == 0 {
                break;
            }
            self.queue.consume(n);
        }
        if (self.backlog || self.dropping) && self.queue.replies() < REPLY_LOW {
            self.backlog = false;
            self.dropping = false;
            return true;
        }
        false
    }
}

impl<E: Endpoint> ReplySink for Writer<E> {
    fn reply(&mut self, reply: &[u8]) {
        self.send(Kind::Reply, reply);
    }

    fn full(&self) -> bool {
        self.backlog || self.dropping
    }
}

/// Writes what the child takes now: 0 when it is not reading or the write
/// fails. A failed write leaves the bytes queued; the child's exit or the
/// host's close ends the pty.
fn write_some<E: Endpoint>(end: &mut E, bytes: &[u8]) -> usize {
    loop {
        match end.write(bytes) {
            Ok(n) => return n.min(bytes.len()),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return 0,
        }
    }
}

/// A pty's two directions.
pub(crate) struct Channel<E> {
    writer: Writer<E>,
    buf: Box<[u8]>,
    /// `buf[start..end]` was read and not yet taken by the consumer.
    start: usize,
    end: usize,
    /// The host paused reading, as an image decode does.
    paused: bool,
    /// A [`Next::feed`] was handed out and its call has not come yet.
    feed_owed: bool,
    /// The last read found no output, and none is held.
    drained: bool,
}

impl<E: Endpoint> Channel<E> {
    pub(crate) fn new(end: E) -> Self {
        Channel {
            writer: Writer {
                end,
                queue: WriteQueue::default(),
                backlog: false,
                dropping: false,
                orphaned: false,
            },
            buf: vec![0; READ_MAX].into_boxed_slice(),
            start: 0,
            end: 0,
            paused: false,
            feed_owed: false,
            drained: false,
        }
    }

    /// Bytes queued for the child.
    pub(crate) fn queued(&self) -> usize {
        self.writer.queue.len()
    }

    /// Reply bytes queued for the child.
    pub(crate) fn queued_replies(&self) -> usize {
        self.writer.queue.replies()
    }

    /// Writes `bytes` of `kind` to the child.
    pub(crate) fn send(&mut self, kind: Kind, bytes: &[u8]) {
        self.writer.send(kind, bytes);
    }

    /// Nothing is read or fed: a pause or a backlog of replies.
    fn stalled(&self) -> bool {
        self.paused || self.writer.backlog || self.writer.dropping
    }

    fn held(&self) -> bool {
        self.start < self.end
    }

    /// Output is held or stalled: a read now would read nothing.
    pub(crate) fn waiting(&self) -> bool {
        self.held() || self.stalled()
    }

    /// The last read found no output, and none is held.
    pub(crate) fn drained(&self) -> bool {
        self.drained && !self.held()
    }

    /// The child exited. What it wrote is still read and fed; replies to it
    /// are dropped, so a backlog of them no longer holds the reading.
    pub(crate) fn orphan(&mut self) -> Next {
        let stalled = self.writer.backlog || self.writer.dropping;
        self.writer.orphaned = true;
        self.writer.backlog = false;
        self.writer.dropping = false;
        if stalled {
            return self.restart();
        }
        Next::default()
    }

    /// A feed on a later turn, unless one is owed already.
    pub(crate) fn feed_later(&mut self) -> Next {
        if self.feed_owed {
            return Next::default();
        }
        self.feed_owed = true;
        Next {
            rearm: false,
            feed: true,
        }
    }

    /// What resumes reading once nothing stalls it: a rearm for output
    /// that came meanwhile, and a feed for output held.
    fn restart(&mut self) -> Next {
        if self.stalled() {
            return Next::default();
        }
        let rearm = Next {
            rearm: true,
            feed: false,
        };
        if self.held() {
            return rearm.or(self.feed_later());
        }
        rearm
    }

    /// One call of the consumer on the held bytes. Returns true when it
    /// took them all.
    fn feed_once<C: Consumer + ?Sized>(&mut self, consumer: &mut C) -> bool {
        let Channel {
            writer,
            buf,
            start,
            end,
            ..
        } = self;
        let held = buf.get(*start..*end).unwrap_or_default();
        let mut replies = Replies(writer);
        let n = consumer.output(held, &mut replies).min(held.len());
        *start = start.saturating_add(n);
        !self.held()
    }

    /// The pty reported readiness.
    pub(crate) fn ready<C: Consumer + ?Sized>(
        &mut self,
        readable: bool,
        writable: bool,
        consumer: &mut C,
    ) -> Next {
        let mut next = Next::default();
        if writable && self.writer.flush() {
            next = next.or(self.restart());
        }
        if readable {
            next = next.or(self.read(consumer));
        }
        next
    }

    /// Reads and feeds until the pty has no more, the consumer holds some
    /// back, or [`READS_PER_TURN`] reads are done.
    fn read<C: Consumer + ?Sized>(&mut self, consumer: &mut C) -> Next {
        if self.held() || self.stalled() {
            return Next::default();
        }
        for _ in 0..READS_PER_TURN {
            match self.writer.end.read(&mut self.buf) {
                Ok(0) => {
                    self.drained = true;
                    return Next::default();
                }
                Ok(n) => {
                    self.drained = false;
                    self.start = 0;
                    self.end = n.min(self.buf.len());
                    let all = self.feed_once(consumer);
                    if self.stalled() {
                        return Next::default();
                    }
                    if !all {
                        return self.feed_later();
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                // WouldBlock: all read. EIO: no slave is open, as during a
                // vhangup; the next edge reports when one is.
                Err(_) => {
                    self.drained = true;
                    return Next::default();
                }
            }
        }
        Next {
            rearm: true,
            feed: false,
        }
    }

    /// The feed a [`Next::feed`] asked for: one consumer call on the held
    /// bytes, so other sources are served between calls.
    pub(crate) fn feed_held<C: Consumer + ?Sized>(&mut self, consumer: &mut C) -> Next {
        self.feed_owed = false;
        if !self.held() || self.stalled() {
            return Next::default();
        }
        let all = self.feed_once(consumer);
        if self.stalled() {
            return Next::default();
        }
        if all {
            // reading resumes: output may have come while this was held
            return Next {
                rearm: true,
                feed: false,
            };
        }
        self.feed_later()
    }

    /// Stops reading and feeding until [`Channel::resume`].
    pub(crate) fn pause(&mut self) {
        self.paused = true;
    }

    pub(crate) fn resume(&mut self) -> Next {
        if !self.paused {
            return Next::default();
        }
        self.paused = false;
        self.restart()
    }
}

#[cfg(test)]
mod tests;
