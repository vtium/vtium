//! The event loop: calloop, holding descriptors, timers and signals.
//!
//! [`Loop`] owns one calloop event loop and one signalfd, which every
//! signal the console handles goes through: SIGTERM and SIGINT for the
//! binary, and the VT's release and acquire signals. Descriptors and
//! timers go in through [`Loop::handle`], as calloop sources: the pty
//! ([`crate::pty::Pty::insert`]), and the display, udev and input devices.
//!
//! A signal handled here is blocked in the thread that adds it and stays
//! pending until the loop reads it, so signals are added before any other
//! thread starts, and threads started later inherit the mask. A child
//! process inherits it too, so the pty clears it in its child before the
//! child runs ([`crate::pty::Pty::spawn`]).
//!
//! When a callback takes another of the console's sources out of the loop
//! while the loop dispatches a batch of events, the source leaves the loop
//! only when the dispatch returns, so no event of that batch finds it
//! gone. A source removed between dispatches leaves at once.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::io;
use std::rc::Rc;
use std::time::Duration;

pub use calloop::signals::Signal;
use calloop::signals::Signals;
use calloop::{Dispatcher, EventSource, InsertError, LoopHandle, LoopSignal, RegistrationToken};

thread_local! {
    /// The dispatches of a [`Loop`] running on this thread, and the
    /// removals waiting for them to end.
    static DISPATCH: RefCell<Dispatching> = RefCell::default();
}

/// A removal that [`Removals::remove`] put off until the dispatch returns.
type Deferred = Box<dyn FnOnce()>;

#[derive(Default)]
struct Dispatching {
    /// How many dispatches run, one inside another. vtium's loop never
    /// nests them, but a count keeps the state right if a test does.
    depth: usize,
    pending: Vec<Deferred>,
}

/// Marks a dispatch of a [`Loop`] as running on this thread while it
/// lives. When the outermost dispatch ends, the removals that callbacks
/// asked for during it take their sources out of the loop.
struct InDispatch;

impl InDispatch {
    fn enter() -> InDispatch {
        DISPATCH.with_borrow_mut(|d| d.depth = d.depth.saturating_add(1));
        InDispatch
    }
}

impl Drop for InDispatch {
    fn drop(&mut self) {
        let pending = DISPATCH.with_borrow_mut(|d| {
            d.depth = d.depth.saturating_sub(1);
            if d.depth == 0 {
                std::mem::take(&mut d.pending)
            } else {
                Vec::new()
            }
        });
        // A panic in a callback unwinds through here, with the loop in an
        // unknown state. The removals are then dropped undone, and the
        // sources go when the loop drops.
        if std::thread::panicking() {
            return;
        }
        // The borrow is over, so a source whose drop removes another
        // source removes it at once.
        for removal in pending {
            removal();
        }
    }
}

/// A signal's handler.
type Handler<D> = Box<dyn FnMut(&mut D)>;

/// The console's event loop, over data `D` its sources handle.
pub struct Loop<D: 'static> {
    inner: calloop::EventLoop<'static, D>,
    signals: Dispatcher<'static, Signals, D>,
    handlers: Rc<RefCell<BTreeMap<Signal, Handler<D>>>>,
}

impl<D: 'static> Loop<D> {
    pub fn new() -> io::Result<Self> {
        let inner = calloop::EventLoop::try_new()?;
        let handlers: Rc<RefCell<BTreeMap<Signal, Handler<D>>>> = Rc::default();
        let signals = Dispatcher::new(Signals::new(&[])?, {
            let handlers = Rc::clone(&handlers);
            move |event, _, data: &mut D| {
                if let Some(handler) = handlers.borrow_mut().get_mut(&event.signal()) {
                    handler(data);
                }
            }
        });
        inner.handle().register_dispatcher(signals.clone())?;
        Ok(Loop {
            inner,
            signals,
            handlers,
        })
    }

    /// Where sources go in: descriptors, timers, idle callbacks.
    pub fn handle(&self) -> LoopHandle<'static, D> {
        self.inner.handle()
    }

    /// Stops [`Loop::run`] from any thread or callback.
    pub fn stopper(&self) -> LoopSignal {
        self.inner.get_signal()
    }

    /// Handles `signal` with `handler`, which replaces any handler it had.
    /// From here the signal is blocked in this thread and read from the
    /// loop's signalfd.
    pub fn on_signal(
        &mut self,
        signal: Signal,
        handler: impl FnMut(&mut D) + 'static,
    ) -> io::Result<()> {
        self.signals.as_source_mut().add_signals(&[signal])?;
        self.handlers.borrow_mut().insert(signal, Box::new(handler));
        Ok(())
    }

    /// Waits for events, at most `timeout` (`None`: until one comes), and
    /// handles them.
    pub fn dispatch(&mut self, timeout: Option<Duration>, data: &mut D) -> io::Result<()> {
        let _dispatch = InDispatch::enter();
        Ok(self.inner.dispatch(timeout, data)?)
    }

    /// Handles events until [`Loop::stopper`] stops it, calling `each_turn`
    /// after each turn.
    pub fn run(&mut self, data: &mut D, mut each_turn: impl FnMut(&mut D)) -> io::Result<()> {
        // Each turn's dispatch is marked as running, and `each_turn` runs
        // outside the mark, as code between two calls of `dispatch` does.
        let mut dispatch = Some(InDispatch::enter());
        Ok(self.inner.run(None, data, |data| {
            drop(dispatch.take());
            each_turn(data);
            dispatch = Some(InDispatch::enter());
        })?)
    }
}

/// Takes sources out of a loop once the batch of events that the loop is
/// dispatching is done.
///
/// One poll of the loop returns a batch of events, and calloop dispatches
/// them in order: the descriptors' events first, then the timers that
/// expired. When a callback removes another source at once, an event of
/// that source may still wait later in the batch. calloop then finds no
/// source for it and logs "Received an event for non-existent source". A
/// source that the same callback inserts may also take the removed
/// source's slot in the loop, so the slot's next event in the batch was
/// meant for a source that left.
///
/// A source inserted through [`Removals::insert`] and removed through
/// [`Removals::remove`] while [`Loop::dispatch`] or [`Loop::run`]
/// dispatches is marked removed at once, so its callback skips its work
/// and answers as the caller said. The source leaves the loop when the
/// dispatch returns, and holds its slot until then. A source removed
/// between dispatches leaves the loop at once, since no batch is waiting:
/// its descriptor closes then, and does not wait for a later event to end a
/// dispatch. A source that removes itself from its own callback needs none
/// of this, since calloop handles that case.
///
/// The dispatch is tracked per thread, so a loop driven by calloop's own
/// `dispatch`, outside [`Loop`], removes its sources at once.
///
/// In vtium the batch case comes from the card's descriptor, which disarms
/// a flip's bound or a stale bound in the batch in which that bound
/// expired, and from an input device, which removes itself after a read
/// error. vtium removes whole cards and input devices between dispatches,
/// as it handles udev's events and the VT's release.
pub(crate) struct Removals<D: 'static> {
    handle: LoopHandle<'static, D>,
    /// The sources removed during the dispatch that runs, which leave the
    /// loop when it returns.
    removed: Rc<RefCell<Vec<RegistrationToken>>>,
}

impl<D: 'static> Clone for Removals<D> {
    fn clone(&self) -> Self {
        Removals {
            handle: self.handle.clone(),
            removed: Rc::clone(&self.removed),
        }
    }
}

impl<D: 'static> Removals<D> {
    pub(crate) fn new(handle: &LoopHandle<'static, D>) -> Self {
        Removals {
            handle: handle.clone(),
            removed: Rc::default(),
        }
    }

    /// Inserts `source` with `callback`. Once [`Removals::remove`] has
    /// removed the source, an event for it that was already in the batch
    /// gets `removed`'s answer and never reaches `callback`.
    pub(crate) fn insert<S, F>(
        &self,
        source: S,
        mut callback: F,
        removed: fn() -> S::Ret,
    ) -> Result<RegistrationToken, InsertError<S>>
    where
        S: EventSource + 'static,
        F: FnMut(S::Event, &mut S::Metadata, &mut D) -> S::Ret + 'static,
    {
        let own: Rc<Cell<Option<RegistrationToken>>> = Rc::default();
        let gone = Rc::clone(&self.removed);
        let token = self.handle.insert_source(source, {
            let own = Rc::clone(&own);
            move |event, metadata, data| {
                if own.get().is_some_and(|t| gone.borrow().contains(&t)) {
                    return removed();
                }
                callback(event, metadata, data)
            }
        })?;
        own.set(Some(token));
        Ok(token)
    }

    /// Removes the source of `token` from the loop. During a dispatch,
    /// the source is marked removed now and leaves the loop when the
    /// dispatch returns. Between dispatches, it leaves at once.
    pub(crate) fn remove(&self, token: RegistrationToken) {
        let deferred = DISPATCH.with_borrow_mut(|d| {
            if d.depth == 0 {
                return false;
            }
            // The removal holds the loop weakly, so a loop dropped before
            // it runs is not kept alive by it.
            let (handle, removed) = (self.handle.downgrade(), Rc::clone(&self.removed));
            d.pending.push(Box::new(move || {
                if let Some(handle) = handle.upgrade() {
                    handle.remove(token);
                }
                removed.borrow_mut().retain(|t| *t != token);
            }));
            true
        });
        if deferred {
            self.removed.borrow_mut().push(token);
        } else {
            self.handle.remove(token);
        }
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use std::cell::Cell;
    use std::os::fd::{AsFd, BorrowedFd};
    use std::os::unix::net::UnixStream;
    use std::rc::Rc;
    use std::time::Duration;

    use calloop::generic::Generic;
    use calloop::ping::make_ping;
    use calloop::timer::{TimeoutAction, Timer};
    use calloop::{Interest, Mode, PostAction, RegistrationToken};
    use log::Level;

    use super::{Loop, Removals};
    use crate::display::fake::logs;

    #[derive(Default)]
    struct Seen {
        fired: Vec<&'static str>,
    }

    // A descriptor's callback removes a timer that expired in the same
    // batch and arms another. The removed timer's event, later in the
    // batch, reaches neither its callback nor the new timer, and calloop
    // finds its source still there.
    #[test]
    fn a_timer_removed_in_the_batch_it_expired_in_does_nothing() {
        let mut lp = Loop::<Seen>::new().unwrap();
        let removals = Removals::new(&lp.handle());
        let old = removals
            .insert(
                Timer::immediate(),
                |_, _, seen: &mut Seen| {
                    seen.fired.push("old");
                    TimeoutAction::Drop
                },
                || TimeoutAction::Drop,
            )
            .unwrap();
        let (ping, source) = make_ping().unwrap();
        lp.handle()
            .insert_source(source, {
                let removals = removals.clone();
                move |(), (), _: &mut Seen| {
                    removals.remove(old);
                    removals
                        .insert(
                            Timer::from_duration(Duration::from_secs(60)),
                            |_, _, seen: &mut Seen| {
                                seen.fired.push("new");
                                TimeoutAction::Drop
                            },
                            || TimeoutAction::Drop,
                        )
                        .unwrap();
                }
            })
            .unwrap();
        logs::capture();
        ping.ping();
        let mut seen = Seen::default();
        lp.dispatch(Some(Duration::ZERO), &mut seen).unwrap();
        assert_eq!(seen.fired, Vec::<&str>::new());
        assert_eq!(logs::at_least(Level::Warn), Vec::<String>::new());
        assert!(
            removals.removed.borrow().is_empty(),
            "the removal ran when the dispatch ended"
        );
    }

    /// The descriptor that a test's source watches: one end of a socket
    /// pair, which marks `closed` when the source drops it and so closes
    /// its descriptor.
    ///
    /// The tests watch this end rather than wait for end of file on the
    /// other one. The other end reads end of file only once every copy of
    /// this end is closed, and a child process that another test in the
    /// same binary forks holds a copy of every descriptor until it runs
    /// its program. A pty test that starts `/bin/sh`, for example, then
    /// keeps the other end from reading end of file for a moment after the
    /// loop has closed this end.
    struct Watched {
        end: UnixStream,
        closed: Rc<Cell<bool>>,
    }

    impl AsFd for Watched {
        fn as_fd(&self) -> BorrowedFd<'_> {
            self.end.as_fd()
        }
    }

    impl Drop for Watched {
        fn drop(&mut self) {
            // `end` drops, and its descriptor closes, as soon as this
            // returns.
            self.closed.set(true);
        }
    }

    /// A descriptor source in `removals`' loop whose callback holds
    /// `alive`. Returns its token, whether its descriptor is closed, and
    /// the other end of its socket pair. While the test holds the other
    /// end, the source's descriptor has nothing to read and stays quiet.
    fn watched(
        removals: &Removals<Seen>,
        alive: &Rc<()>,
    ) -> (RegistrationToken, Rc<Cell<bool>>, UnixStream) {
        let (end, other) = UnixStream::pair().unwrap();
        let closed = Rc::new(Cell::new(false));
        let watched = Watched {
            end,
            closed: Rc::clone(&closed),
        };
        let alive = Rc::clone(alive);
        let token = removals
            .insert(
                Generic::new(watched, Interest::READ, Mode::Level),
                move |_, _, _: &mut Seen| {
                    let _ = &alive;
                    Ok(PostAction::Continue)
                },
                || Ok(PostAction::Continue),
            )
            .unwrap();
        (token, closed, other)
    }

    // vtium removes a card or an input device between dispatches, as it
    // handles udev's events and the VT's release. The source leaves the
    // loop at once: its descriptor closes and its callback is dropped,
    // whether or not the loop ever dispatches again.
    #[test]
    fn a_source_removed_between_dispatches_leaves_the_loop_at_once() {
        let lp = Loop::<Seen>::new().unwrap();
        let removals = Removals::new(&lp.handle());
        let alive = Rc::new(());
        let (token, closed, _other) = watched(&removals, &alive);
        assert!(!closed.get());
        removals.remove(token);
        assert!(closed.get(), "the descriptor is closed");
        assert_eq!(Rc::strong_count(&alive), 1, "the callback is dropped");
        assert!(removals.removed.borrow().is_empty());
    }

    // A source removed inside a dispatch leaves the loop when that dispatch
    // returns, not at a later one.
    #[test]
    fn a_source_removed_inside_a_dispatch_leaves_when_the_dispatch_returns() {
        let mut lp = Loop::<Seen>::new().unwrap();
        let removals = Removals::new(&lp.handle());
        let alive = Rc::new(());
        let (token, closed, _other) = watched(&removals, &alive);
        let (ping, source) = make_ping().unwrap();
        lp.handle()
            .insert_source(source, {
                let removals = removals.clone();
                move |(), (), _: &mut Seen| removals.remove(token)
            })
            .unwrap();
        ping.ping();
        lp.dispatch(Some(Duration::ZERO), &mut Seen::default())
            .unwrap();
        assert!(closed.get(), "the descriptor is closed");
        assert_eq!(Rc::strong_count(&alive), 1, "the callback is dropped");
    }

    // `Loop::run` calls `each_turn` between two dispatches, so a source
    // removed there leaves the loop at once, as between two calls of
    // `Loop::dispatch`.
    #[test]
    fn a_source_removed_between_the_turns_of_run_leaves_the_loop_at_once() {
        let mut lp = Loop::<Seen>::new().unwrap();
        let removals = Removals::new(&lp.handle());
        let alive = Rc::new(());
        let (token, closed, _other) = watched(&removals, &alive);
        let (ping, source) = make_ping().unwrap();
        lp.handle()
            .insert_source(source, |(), (), _: &mut Seen| {})
            .unwrap();
        ping.ping();
        let stop = lp.stopper();
        let mut closed_at_once = None;
        lp.run(&mut Seen::default(), |_| {
            removals.remove(token);
            closed_at_once = Some(closed.get() && Rc::strong_count(&alive) == 1);
            stop.stop();
        })
        .unwrap();
        assert_eq!(closed_at_once, Some(true));
    }

    // A loop dropped after a removal takes every source with it: nothing
    // that the removal left behind keeps the loop alive.
    #[test]
    fn a_loop_dropped_after_a_removal_leaks_no_source() {
        let mut lp = Loop::<Seen>::new().unwrap();
        let removals = Removals::new(&lp.handle());
        let alive = Rc::new(());
        let (gone, _, _) = watched(&removals, &alive);
        let (_kept, kept_closed, _other) = watched(&removals, &alive);
        let (ping, source) = make_ping().unwrap();
        let pinged = lp
            .handle()
            .insert_source(source, {
                let removals = removals.clone();
                move |(), (), _: &mut Seen| removals.remove(gone)
            })
            .unwrap();
        ping.ping();
        lp.dispatch(Some(Duration::ZERO), &mut Seen::default())
            .unwrap();
        removals.remove(gone);
        // The ping's callback holds the loop through its own `Removals`, as
        // any callback that holds a loop handle does, so it leaves first.
        lp.handle().remove(pinged);
        drop(removals);
        drop(lp);
        assert_eq!(Rc::strong_count(&alive), 1, "every source is dropped");
        assert!(kept_closed.get(), "the kept source's descriptor is closed");
    }

    // A removal that waits for a dispatch to end holds its loop weakly, so
    // a loop dropped while the removal waits takes every source with it.
    // The dispatch is tracked per thread, so a callback of one loop that
    // removes a source of a second loop puts that removal off until the
    // first loop's dispatch returns. The callback then drops the second
    // loop, and both of its sources go at once.
    #[test]
    fn a_loop_dropped_while_a_removal_waits_takes_every_source_with_it() {
        let mut outer = Loop::<Seen>::new().unwrap();
        let inner = Loop::<Seen>::new().unwrap();
        let removals = Removals::new(&inner.handle());
        let alive = Rc::new(());
        let (gone, gone_closed, _gone_other) = watched(&removals, &alive);
        let (_kept, kept_closed, _kept_other) = watched(&removals, &alive);
        let sources = Rc::downgrade(&alive);
        drop(alive);
        let seen_at_drop = Rc::new(Cell::new(None));
        let (ping, source) = make_ping().unwrap();
        outer
            .handle()
            .insert_source(source, {
                let seen_at_drop = Rc::clone(&seen_at_drop);
                let mut held = Some((inner, removals));
                move |(), (), _: &mut Seen| {
                    let Some((inner, removals)) = held.take() else {
                        return;
                    };
                    removals.remove(gone);
                    let waiting = super::DISPATCH.with_borrow(|d| d.pending.len());
                    drop(removals);
                    drop(inner);
                    seen_at_drop.set(Some((
                        waiting,
                        gone_closed.get(),
                        kept_closed.get(),
                        sources.strong_count(),
                    )));
                }
            })
            .unwrap();
        ping.ping();
        outer
            .dispatch(Some(Duration::ZERO), &mut Seen::default())
            .unwrap();
        assert_eq!(
            seen_at_drop.get(),
            Some((1, true, true, 0)),
            "with the removal waiting, the dropped loop closed both descriptors and dropped both callbacks"
        );
        assert!(
            super::DISPATCH.with_borrow(|d| d.pending.is_empty()),
            "the waiting removal ran when the dispatch returned"
        );
    }
}
