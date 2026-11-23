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
//! A source that one callback takes out of the loop while the loop
//! dispatches a batch of events leaves the loop through [`Removals`], so
//! no event of that batch finds it gone.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::io;
use std::rc::Rc;
use std::time::Duration;

pub use calloop::signals::Signal;
use calloop::signals::Signals;
use calloop::{Dispatcher, EventSource, InsertError, LoopHandle, LoopSignal, RegistrationToken};

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
        Ok(self.inner.dispatch(timeout, data)?)
    }

    /// Handles events until [`Loop::stopper`] stops it, calling `each_turn`
    /// after each turn.
    pub fn run(&mut self, data: &mut D, each_turn: impl FnMut(&mut D)) -> io::Result<()> {
        Ok(self.inner.run(None, data, each_turn)?)
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
/// [`Removals::remove`] is marked removed at once, so its callback skips
/// its work and answers as the caller said. The loop takes the source out
/// in an idle callback, which calloop runs after the whole batch, and the
/// source holds its slot until then. A source that removes itself from its
/// own callback needs none of this, since calloop handles that case.
pub(crate) struct Removals<D: 'static> {
    handle: LoopHandle<'static, D>,
    /// The sources removed whose idle callback has not run yet.
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

    /// Removes the source of `token` from the loop once the batch being
    /// dispatched is done, and marks it removed now. Outside a dispatch,
    /// the next dispatch removes it.
    pub(crate) fn remove(&self, token: RegistrationToken) {
        self.removed.borrow_mut().push(token);
        let (handle, removed) = (self.handle.clone(), Rc::clone(&self.removed));
        self.handle.insert_idle(move |_| {
            handle.remove(token);
            removed.borrow_mut().retain(|t| *t != token);
        });
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use std::time::Duration;

    use calloop::ping::make_ping;
    use calloop::timer::{TimeoutAction, Timer};
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
            "the idle callback ran"
        );
    }
}
