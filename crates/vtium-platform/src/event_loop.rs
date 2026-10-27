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
//! thread starts, and threads started later inherit the mask. std's
//! `Command` clears the mask in each child before it runs.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io;
use std::rc::Rc;
use std::time::Duration;

pub use calloop::signals::Signal;
use calloop::signals::Signals;
use calloop::{Dispatcher, LoopHandle, LoopSignal};

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
