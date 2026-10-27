//! drm2d in the event loop: the card's page flip events, the bound on
//! waiting for a flip, and the wake-up retries.

use std::io;
use std::marker::PhantomData;
use std::time::Duration;

use calloop::generic::Generic;
use calloop::timer::{TimeoutAction, Timer};
use calloop::{Interest, LoopHandle, Mode as Trigger, PostAction, RegistrationToken};

use super::super::card::Card;
use super::super::kms::Kms;
use super::super::mode::ModePolicy;
use super::Drm2d;

/// The loop's timers, while armed.
#[derive(Default)]
pub(super) struct Timers {
    pub(super) flip_wait: Option<RegistrationToken>,
    pub(super) wake_retry: Option<RegistrationToken>,
}

#[derive(Clone, Copy)]
pub(super) enum Clock {
    /// The bound on waiting for a flip, [`crate::FLIP_WAIT_MS`].
    FlipWait,
    /// The next wake-up try, after [`crate::WAKE_RETRY_MS`].
    WakeRetry,
}

impl Clock {
    fn after(self) -> Duration {
        let ms = match self {
            Clock::FlipWait => crate::FLIP_WAIT_MS,
            Clock::WakeRetry => crate::WAKE_RETRY_MS,
        };
        Duration::from_millis(u64::try_from(ms).unwrap_or(u64::MAX))
    }
}

/// What ties a device to the loop it was inserted in.
pub(super) trait Link {
    fn arm(&self, clock: Clock) -> Option<RegistrationToken>;
    fn disarm(&self, token: RegistrationToken);
}

impl Drm2d<Card> {
    /// Takes a card, asleep: [`Drm2d::wake_up`] takes DRM master and sets
    /// the displays up.
    pub fn new(card: Card, policy: ModePolicy) -> io::Result<Drm2d<Card>> {
        Drm2d::with(card, policy)
    }
}

#[expect(private_bounds, reason = "the card is a Card outside the crate")]
impl<K: Kms + 'static> Drm2d<K> {
    /// Puts the card in the loop behind `handle`: its page flip events,
    /// the bound on waiting for a flip and the wake-up retries. `access`
    /// finds the device in the loop's data, or `None` once it is gone.
    pub fn insert<D, A>(&mut self, handle: &LoopHandle<'static, D>, access: A) -> io::Result<()>
    where
        D: 'static,
        A: Fn(&mut D) -> Option<&mut Drm2d<K>> + Clone + 'static,
    {
        let source = handle
            .insert_source(
                Generic::new(self.card.fd(), Interest::READ, Trigger::Level),
                {
                    let access = access.clone();
                    move |readiness, _, data: &mut D| {
                        let Some(video) = access(data) else {
                            return Ok(PostAction::Remove);
                        };
                        if readiness.error {
                            log::error!("error or hangup on the DRM card");
                            return Ok(PostAction::Remove);
                        }
                        if let Err(e) = video.dispatch() {
                            log::warn!("cannot read the DRM card's events: {e}");
                        }
                        Ok(PostAction::Continue)
                    }
                },
            )
            .map_err(|e| io::Error::from(e.error))?;
        self.link = Some(Box::new(LoopLink {
            handle: handle.clone(),
            source,
            access,
            card: PhantomData::<fn(&K)>,
        }));
        self.settle();
        Ok(())
    }
}

struct LoopLink<D: 'static, K, A> {
    handle: LoopHandle<'static, D>,
    source: RegistrationToken,
    access: A,
    card: PhantomData<fn(&K)>,
}

impl<D, K, A> Link for LoopLink<D, K, A>
where
    D: 'static,
    K: Kms + 'static,
    A: Fn(&mut D) -> Option<&mut Drm2d<K>> + Clone + 'static,
{
    fn arm(&self, clock: Clock) -> Option<RegistrationToken> {
        let access = self.access.clone();
        self.handle
            .insert_source(Timer::from_duration(clock.after()), move |_, _, data| {
                if let Some(video) = access(data) {
                    match clock {
                        Clock::FlipWait => {
                            video.timers.flip_wait = None;
                            video.flip_wait_expired();
                        }
                        Clock::WakeRetry => {
                            video.timers.wake_retry = None;
                            video.retry_wake();
                        }
                    }
                }
                TimeoutAction::Drop
            })
            .inspect_err(|e| log::error!("cannot arm a display timer: {e}"))
            .ok()
    }

    fn disarm(&self, token: RegistrationToken) {
        self.handle.remove(token);
    }
}

impl<D: 'static, K, A> Drop for LoopLink<D, K, A> {
    fn drop(&mut self) {
        self.handle.remove(self.source);
    }
}
