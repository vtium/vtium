//! A seat's clock that only the test moves. It starts at the time it is
//! made and stands still until the test moves it on, so a timer the seat
//! keeps, such as the cursor's blink or the blank's countdown, fires at
//! the turn the test chose, however fast or slowly the machine runs the
//! test.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// The clock. Its clones share one time, so a test's device can read the
/// time the seat reads.
#[derive(Clone)]
pub struct SeatClock(Rc<Cell<Instant>>);

impl SeatClock {
    /// A clock standing at the real time.
    #[expect(
        clippy::disallowed_methods,
        reason = "the seat's clock starts at the real time, and only the test moves it on"
    )]
    pub fn new() -> SeatClock {
        SeatClock(Rc::new(Cell::new(Instant::now())))
    }

    /// The time the clock stands at.
    pub fn now(&self) -> Instant {
        self.0.get()
    }

    /// Moves the clock on by `by`.
    pub fn advance(&self, by: Duration) {
        self.0.set(self.0.get() + by);
    }

    /// The clock as the seat reads it.
    pub fn clock(&self) -> vtium::term::Clock {
        let seat = self.clone();
        Rc::new(move || seat.now())
    }
}
