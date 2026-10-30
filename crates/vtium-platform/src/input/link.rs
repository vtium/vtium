//! What ties the input to the loop it was inserted in: each open device's
//! descriptor, the repeat timers and the pointer's hide timer.

use std::os::fd::OwnedFd;
use std::time::{Duration, Instant};

use calloop::generic::Generic;
use calloop::timer::{TimeoutAction, Timer};
use calloop::{Interest, LoopHandle, Mode, PostAction, RegistrationToken};

use super::{DeviceId, Input, Open};

/// What ties the input to the loop it was inserted in.
pub(super) trait Link {
    fn watch(&self, id: DeviceId, fd: OwnedFd) -> Option<RegistrationToken>;
    fn unwatch(&self, token: RegistrationToken);
    fn repeat(&self, id: DeviceId, serial: u64, delay: Duration, rate: Duration);
    /// Arms the pointer's hide timer, `after` from now, if it is not.
    fn hide(&self, after: Duration);
}

pub(super) struct LoopLink<D: 'static, A> {
    handle: LoopHandle<'static, D>,
    access: A,
    /// The hide timer is in the loop.
    hide_armed: std::rc::Rc<std::cell::Cell<bool>>,
}

impl<D: 'static, A> LoopLink<D, A> {
    /// The link to the loop behind `handle`; `access` finds the input in
    /// the loop's data.
    pub(super) fn new(handle: &LoopHandle<'static, D>, access: A) -> LoopLink<D, A> {
        LoopLink {
            handle: handle.clone(),
            access,
            hide_armed: std::rc::Rc::new(std::cell::Cell::new(false)),
        }
    }
}

impl<D, A, O> Link for LoopLink<D, A>
where
    D: 'static,
    O: Open + 'static,
    A: Fn(&mut D) -> Option<&mut Input<O>> + Clone + 'static,
{
    fn watch(&self, id: DeviceId, fd: OwnedFd) -> Option<RegistrationToken> {
        let access = self.access.clone();
        self.handle
            .insert_source(
                Generic::new(fd, Interest::READ, Mode::Level),
                move |readiness, _, data: &mut D| {
                    let Some(input) = access(data) else {
                        return Ok(PostAction::Remove);
                    };
                    if readiness.error {
                        input.hangup(id);
                        return Ok(PostAction::Continue);
                    }
                    #[expect(
                        clippy::disallowed_methods,
                        reason = "the double click reads the clock, held in vtium-platform"
                    )]
                    let now = Instant::now();
                    input.dispatch(id, now);
                    Ok(PostAction::Continue)
                },
            )
            .inspect_err(|e| log::warn!(target: "input", "cannot watch a device: {}", e.error))
            .ok()
    }

    fn unwatch(&self, token: RegistrationToken) {
        self.handle.remove(token);
    }

    fn repeat(&self, id: DeviceId, serial: u64, delay: Duration, rate: Duration) {
        let access = self.access.clone();
        let inserted = self
            .handle
            .insert_source(Timer::from_duration(delay), move |_, _, data| {
                if access(data).is_some_and(|input| input.repeat(id, serial)) {
                    TimeoutAction::ToDuration(rate)
                } else {
                    TimeoutAction::Drop
                }
            });
        if let Err(e) = inserted {
            log::warn!(target: "input", "cannot start the key repeat: {}", e.error);
        }
    }

    fn hide(&self, after: Duration) {
        if self.hide_armed.replace(true) {
            return;
        }
        let access = self.access.clone();
        let armed = std::rc::Rc::clone(&self.hide_armed);
        let inserted = self
            .handle
            .insert_source(Timer::from_duration(after), move |_, _, data| {
                #[expect(
                    clippy::disallowed_methods,
                    reason = "the pointer's hide timer is one of the timers vtium-platform holds"
                )]
                let now = Instant::now();
                match access(data).and_then(|input| input.hide(now)) {
                    Some(left) => TimeoutAction::ToDuration(left),
                    None => {
                        armed.set(false);
                        TimeoutAction::Drop
                    }
                }
            });
        if let Err(e) = inserted {
            self.hide_armed.set(false);
            log::warn!(target: "input", "cannot arm the pointer's hide timer: {}", e.error);
        }
    }
}
