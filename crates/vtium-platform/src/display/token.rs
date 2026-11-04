//! A display's buffer, lent for one frame (ADR-1401 D2).
//!
//! When the loop takes a frame for a display, the display lends it a
//! [`Token`]: an owned value naming one of the display's buffers by its
//! index and carrying that buffer's age and what the head's render thread
//! needs to draw into it. The token goes to the render thread with the
//! frame and comes back with the frame's damage, and the loop gives it
//! back to the display at the swap. The display keeps its device, its
//! buffers, its modesets, its flips and its cursor plane; only the right
//! to draw into one buffer travels.
//!
//! On the CPU path the token carries the buffer's memory: a dumb buffer's
//! mapping, the framebuffer's mapping, or the XRGB8888 copy fbdev converts
//! from. The display and the token share that memory, and the display
//! keeps it mapped while the token is out, even when it stops using the
//! buffer, so the render thread never writes into memory that is gone.
//! On the GPU path, with the `gpu` feature, the token carries the buffer as
//! a [`DmaBuf`], which the head's render thread imports into its GL context
//! as an EGL image and draws into (ADR-1303 D7). No display lends one yet:
//! the displays' GBM buffers come with the work that chooses the path
//! (ADR-1303 D11, part 6).
//!
//! A display records each token it lends as a [`Loan`]. The token holds a
//! ticket the loan watches, so the display knows when its token is still
//! out, when it comes back, and when the owner dropped it without giving
//! it back. A display that must stop a token writing before it comes
//! back, as fbdev does before it lets its framebuffer go to the next VT,
//! revokes it while it holds the memory's lock: the token draws nothing
//! from then on.

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError, Weak};

#[cfg(feature = "gpu")]
use super::DmaBuf;
use super::{Canvas, DisplayId};

/// The memory of a buffer a frame is drawn into, shared by the display
/// and the token it lends.
pub type Memory = Mutex<dyn AsMut<[u8]> + Send>;

/// How many buffers a DRM display keeps (ADR-1401 D2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Buffers {
    /// Single buffering: vtium draws into the buffer being shown, so a
    /// frame can be seen half drawn, and no frame waits for a flip.
    One,
    /// Double buffering: a frame is drawn once the flip of the frame
    /// before it has completed.
    #[default]
    Two,
    /// Triple buffering: a frame is drawn while the flip of the frame
    /// before it is still pending.
    Three,
}

impl Buffers {
    /// The buffers for `count`, which is 1, 2 or 3.
    pub fn new(count: u8) -> Option<Buffers> {
        match count {
            1 => Some(Buffers::One),
            2 => Some(Buffers::Two),
            3 => Some(Buffers::Three),
            _ => None,
        }
    }

    pub fn count(self) -> usize {
        match self {
            Buffers::One => 1,
            Buffers::Two => 2,
            Buffers::Three => 3,
        }
    }
}

/// A display's buffer, lent for one frame. It names the buffer by its
/// index among the display's buffers and carries the buffer's age: how
/// many of the display's own frames ago the buffer was last drawn, 1 for
/// the frame before this one, 2 for the frame before that, and 0 when its
/// contents are unknown, so the frame is drawn whole.
///
/// It is [`Send`], so the render thread draws into it. Give it back to the
/// display that lent it, with the frame's damage, or unused; a token the
/// owner drops instead counts as one that came back too late.
pub struct Token {
    id: DisplayId,
    buffer: usize,
    age: u32,
    lease: u64,
    width: u32,
    height: u32,
    lent: Lent,
    ticket: Arc<()>,
    /// Set by the display, under the memory's lock, once the token may
    /// write no more.
    revoked: Arc<AtomicBool>,
}

/// What a token lends: the buffer's memory on the CPU path, or the buffer
/// as a dma-buf on the GPU path.
enum Lent {
    Memory {
        stride: u32,
        memory: Arc<Memory>,
    },
    #[cfg(feature = "gpu")]
    DmaBuf(Arc<DmaBuf>),
}

impl Token {
    /// A token for `memory`, a `width` by `height` buffer of `stride`
    /// bytes a line, for a stand-in display outside this crate. Its
    /// buffer is index 0.
    pub fn new(
        id: DisplayId,
        (width, height): (u32, u32),
        stride: u32,
        age: u32,
        memory: Arc<Memory>,
    ) -> Token {
        Token {
            id,
            buffer: 0,
            age,
            lease: 0,
            width,
            height,
            lent: Lent::Memory { stride, memory },
            ticket: Arc::new(()),
            revoked: Arc::default(),
        }
    }

    /// A token for `buffer`, a dma-buf, for a stand-in display outside this
    /// crate. Its buffer is index 0 and its size the dma-buf's.
    #[cfg(feature = "gpu")]
    pub fn new_dma_buf(id: DisplayId, age: u32, buffer: Arc<DmaBuf>) -> Token {
        let (width, height) = buffer.size();
        Token {
            id,
            buffer: 0,
            age,
            lease: 0,
            width,
            height,
            lent: Lent::DmaBuf(buffer),
            ticket: Arc::new(()),
            revoked: Arc::default(),
        }
    }

    /// The token with `lease`, the stand-in display's lease when it lent
    /// it, so the stand-in can tell a token that comes back too late as a
    /// real display does.
    pub fn leased(mut self, lease: u64) -> Token {
        self.lease = lease;
        self
    }

    /// The lease of the display when it lent the token.
    pub fn lease(&self) -> u64 {
        self.lease
    }

    /// The display that lent it.
    pub fn id(&self) -> DisplayId {
        self.id
    }

    /// The index of its buffer among the display's buffers.
    pub fn buffer(&self) -> usize {
        self.buffer
    }

    pub fn age(&self) -> u32 {
        self.age
    }

    /// The buffer's size in pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Calls `draw` with the buffer as a canvas in XRGB8888, and returns
    /// what it returns. `None` where the buffer's memory is shorter than
    /// its lines, where the display revoked the token, or where the token
    /// lends a dma-buf, and `draw` is then not called.
    pub fn draw<R>(&mut self, draw: impl FnOnce(Canvas<'_>) -> R) -> Option<R> {
        let (stride, memory) = match &self.lent {
            Lent::Memory { stride, memory } => (*stride, memory),
            #[cfg(feature = "gpu")]
            Lent::DmaBuf(_) => return None,
        };
        let len = usize::try_from(u64::from(stride).checked_mul(u64::from(self.height))?).ok()?;
        let mut memory = lock(memory);
        if self.revoked.load(Ordering::Acquire) {
            return None;
        }
        let pixels = (*memory).as_mut().get_mut(..len)?;
        Some(draw(Canvas {
            pixels,
            width: self.width,
            height: self.height,
            stride,
            age: self.age,
        }))
    }

    /// The buffer as a dma-buf, for the GPU path to import; `None` for a
    /// token that lends memory.
    #[cfg(feature = "gpu")]
    pub fn dma_buf(&self) -> Option<&DmaBuf> {
        match &self.lent {
            Lent::DmaBuf(buffer) => Some(buffer),
            Lent::Memory { .. } => None,
        }
    }

    /// The token's memory, as it comes back to a display that clears it;
    /// `None` for a token that lends a dma-buf.
    pub(crate) fn memory(&self) -> Option<&Memory> {
        match &self.lent {
            Lent::Memory { memory, .. } => Some(memory),
            #[cfg(feature = "gpu")]
            Lent::DmaBuf(_) => None,
        }
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Token")
            .field("id", &self.id)
            .field("buffer", &self.buffer)
            .field("age", &self.age)
            .field("lease", &self.lease)
            .field("size", &(self.width, self.height))
            .finish_non_exhaustive()
    }
}

/// A token a display lent, as the display records it.
#[derive(Debug)]
pub(crate) struct Loan {
    /// The buffer it names.
    pub(crate) buffer: usize,
    /// The display's lease when it was lent: a display that changes what
    /// its buffers mean, as a modeset that gives it new buffers, a VT
    /// release or a blank, takes a new lease, and a token of an older one
    /// comes back too late.
    pub(crate) lease: u64,
    ticket: Weak<()>,
    revoked: Arc<AtomicBool>,
    /// The memory it lent, which a revoke locks. Weak, so the loan never
    /// keeps a buffer alive or counts as a holder of it.
    memory: Weak<Memory>,
}

impl Loan {
    /// Lends `memory`, buffer `buffer` of display `id`, at `age` under
    /// `lease`.
    pub(crate) fn lend(
        id: DisplayId,
        buffer: usize,
        age: u32,
        lease: u64,
        (width, height): (u32, u32),
        stride: u32,
        memory: Arc<Memory>,
    ) -> (Token, Loan) {
        let ticket = Arc::new(());
        let revoked = Arc::new(AtomicBool::new(false));
        let loan = Loan {
            buffer,
            lease,
            ticket: Arc::downgrade(&ticket),
            revoked: Arc::clone(&revoked),
            memory: Arc::downgrade(&memory),
        };
        let token = Token {
            id,
            buffer,
            age,
            lease,
            width,
            height,
            lent: Lent::Memory { stride, memory },
            ticket,
            revoked,
        };
        (token, loan)
    }

    /// Whether the token is still out: neither given back nor dropped.
    pub(crate) fn out(&self) -> bool {
        self.ticket.strong_count() > 0
    }

    /// Whether `token` is the one this loan lent.
    pub(crate) fn lent(&self, token: &Token) -> bool {
        Weak::ptr_eq(&self.ticket, &Arc::downgrade(&token.ticket))
    }

    /// Stops the token writing into the memory it lent: takes that
    /// memory's lock, waiting for a draw in progress when `wait`, else only
    /// where no render thread draws now, and revokes the token under it.
    /// Returns whether the token is revoked; `false` where the lock is busy
    /// and `wait` is false. A token whose memory is gone writes nothing,
    /// and counts as revoked.
    pub(crate) fn revoke(&self, wait: bool) -> bool {
        let Some(memory) = self.memory.upgrade() else {
            return true;
        };
        let guard = if wait {
            Some(lock(&memory))
        } else {
            try_lock(&memory)
        };
        let Some(guard) = guard else {
            return false;
        };
        self.revoked.store(true, Ordering::Release);
        drop(guard);
        true
    }

    /// Whether `token`, this loan's, comes back under the same lease.
    pub(crate) fn current(&self, token: &Token, lease: u64) -> bool {
        self.lent(token) && token.lease == lease && self.lease == lease
    }
}

/// Locks `memory`, waiting for a render thread drawing into it. A thread
/// that panicked while drawing ends the process, so a poisoned lock only
/// meets a test, which takes the memory as it is.
pub(crate) fn lock<M: ?Sized>(memory: &Mutex<M>) -> MutexGuard<'_, M> {
    memory.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Locks `memory` where no render thread is drawing into it now.
pub(crate) fn try_lock<M: ?Sized>(memory: &Mutex<M>) -> Option<MutexGuard<'_, M>> {
    match memory.try_lock() {
        Ok(guard) => Some(guard),
        Err(TryLockError::Poisoned(p)) => Some(p.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    fn memory(len: usize) -> Arc<Memory> {
        Arc::new(Mutex::new(vec![0u8; len]))
    }

    #[test]
    fn a_loan_knows_its_token_is_out_until_it_is_dropped() {
        let (token, loan) = Loan::lend(DisplayId(1), 0, 0, 7, (2, 2), 8, memory(16));
        assert!(loan.out());
        assert!(loan.current(&token, 7));
        assert!(!loan.current(&token, 8), "a newer lease");
        drop(token);
        assert!(!loan.out());
    }

    #[test]
    fn a_token_draws_into_its_memory_and_no_other() {
        let shared = memory(16);
        let (mut token, loan) = Loan::lend(DisplayId(1), 1, 2, 0, (2, 2), 8, Arc::clone(&shared));
        let (other, _) = Loan::lend(DisplayId(1), 1, 2, 0, (2, 2), 8, memory(16));
        assert!(!loan.lent(&other));
        let age = token.draw(|c| {
            c.pixels[0] = 9;
            c.age
        });
        assert_eq!(age, Some(2));
        assert_eq!(lock(&shared).as_mut()[0], 9);
    }

    #[test]
    fn a_revoked_token_draws_nothing() {
        let shared = memory(16);
        let (mut token, loan) = Loan::lend(DisplayId(1), 0, 0, 0, (2, 2), 8, Arc::clone(&shared));
        assert!(loan.revoke(false));
        assert_eq!(token.draw(|c| c.pixels.fill(9)), None);
        assert!(lock(&shared).as_mut().iter().all(|&b| b == 0));
    }

    // When the token is dropped and nothing else holds its memory, the
    // memory is gone and the loan has nothing to lock. Nothing can write
    // into memory that is gone, so revoke counts the token as revoked,
    // whether it waits or not.
    #[test]
    fn a_loan_whose_memory_is_gone_counts_as_revoked() {
        let (token, loan) = Loan::lend(DisplayId(1), 0, 0, 0, (2, 2), 8, memory(16));
        drop(token);
        assert!(loan.memory.upgrade().is_none(), "the memory is gone");
        assert!(loan.revoke(true));
        assert!(loan.revoke(false));
    }

    #[test]
    fn a_token_whose_memory_is_short_draws_nothing() {
        let (mut token, _) = Loan::lend(DisplayId(1), 0, 0, 0, (2, 2), 8, memory(15));
        assert_eq!(token.draw(|_| ()), None);
    }

    #[cfg(feature = "gpu")]
    #[test]
    fn a_token_that_lends_a_dma_buf_gives_it_and_no_canvas() {
        use crate::display::{DmaBuf, Plane};
        let plane = Plane {
            fd: std::fs::File::open("/dev/null").unwrap().into(),
            offset: 0,
            stride: 64,
        };
        let buffer = Arc::new(DmaBuf::new((16, 8), 0x3432_5258, 0, vec![plane]).unwrap());
        let mut token = Token::new_dma_buf(DisplayId(2), 1, Arc::clone(&buffer));
        assert_eq!((token.size(), token.age()), ((16, 8), 1));
        assert!(token.dma_buf().is_some_and(|b| std::ptr::eq(b, &*buffer)));
        assert!(token.memory().is_none());
        assert_eq!(token.draw(|_| ()), None);
        let memory_token = Token::new(DisplayId(2), (2, 2), 8, 0, memory(16));
        assert!(memory_token.dma_buf().is_none());
    }

    #[test]
    fn the_buffers_are_one_two_or_three() {
        assert_eq!(Buffers::new(0), None);
        assert_eq!(Buffers::new(4), None);
        let counts: Vec<usize> = (1..=3).map(|n| Buffers::new(n).unwrap().count()).collect();
        assert_eq!(counts, [1, 2, 3]);
    }
}
