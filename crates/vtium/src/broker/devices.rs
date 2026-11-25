//! The broker's VT and devices: the drawer's requests for the cards, the
//! framebuffers, the input devices, DRM master, the switches and the VT's
//! modes, served by vtium-platform's [`Holder`].
//!
//! The broker opens the VT before it starts the drawer, as vtium in one
//! process opens it: the device the `vt` key names, or a free VT. It then
//! holds the VT for the drawer's whole run. Once a turn of the broker's
//! loop, the holder acts on the switches the kernel asked for and on the
//! release's bound. After any end of the drawer, the broker gives the VT
//! back through [`Devices::give_back`], and its panic hook puts the VT back
//! if the broker itself panics.
//!
//! References:
//! - ADR-1400 D4, "What the broker does": the VT, the devices, master, the
//!   release's bound and the acquire.
//! - ADR-1400 D4, "When the drawer dies": the restore after any end of the
//!   drawer, and the broker's own panic hook.

use std::cell::RefCell;
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use vtium_platform::event_loop::Loop;
use vtium_platform::vt::Tty;
use vtium_platform::vt::broker::Holder;

use super::outbox::Outbox;
use super::protocol::Request;

/// The seat the broker serves. vtium's VTs are seat0's.
const SEAT: &str = "seat0";

/// The VT the broker holds: the device the `vt` key names, or a free VT
/// that the kernel finds, as vtium in one process chooses it.
pub fn vt_path(args: &[OsString]) -> io::Result<PathBuf> {
    let loaded = vtium::config::load(args, &mut vtium::config::read_file);
    let start = vtium::start::Start::new(&loaded.config, String::new());
    match start.vt {
        Some(vt) => Ok(vt),
        None => Tty::find()?.ok_or_else(|| io::Error::other("no VT found")),
    }
}

/// The VT and the devices the broker holds for the drawer.
pub struct Devices {
    holder: Rc<RefCell<Holder>>,
}

impl Devices {
    /// Opens the VT at `path` in the broker's loop and takes it, with the
    /// broker's panic hook set to put it back. Call it before the drawer
    /// starts, while the broker has one thread.
    pub fn open<D: 'static>(lp: &mut Loop<D>, path: &std::path::Path) -> io::Result<Devices> {
        let (tty, num) = Tty::open(path)?;
        let holder = Holder::start(lp, tty, num, SEAT)?;
        let restore = holder.borrow().restorer()?;
        vtium::hook::restore_first(move || restore.run());
        log::info!(target: "broker", "vtium's root process holds VT {num} for its drawing process");
        Ok(Devices { holder })
    }

    /// Whether the devices serve `request`.
    pub fn serves(request: &Request) -> bool {
        Holder::<Tty>::serves(request)
    }

    /// Serves one of the drawer's requests for the devices or the VT, and
    /// sends the answers.
    pub fn serve(
        &mut self,
        request: Request,
        now: Instant,
        out: &mut Outbox<'_>,
    ) -> io::Result<()> {
        let orders = self.holder.borrow_mut().serve(request, now);
        send(orders, out)
    }

    /// Acts on the VT's switches and the release's bound, and sends what
    /// follows. Returns whether the VT hung up, so the drawer must stop.
    pub fn turn(&mut self, now: Instant, out: &mut Outbox<'_>) -> bool {
        let orders = self.holder.borrow_mut().turn(now);
        if let Err(e) = send(orders, out) {
            log::warn!(target: "broker", "vtium's root process cannot tell its drawing process about the VT: {e}");
        }
        self.holder.borrow().hung_up()
    }

    /// When the broker's loop must run [`Devices::turn`] again.
    pub fn due(&self) -> Option<Instant> {
        self.holder.borrow().due()
    }

    /// A handle that gives the VT back after the drawer's end: text mode,
    /// `VT_AUTO` and the saved keyboard mode, then master dropped and the
    /// input devices revoked.
    pub fn give_back(&self) -> impl FnMut() + use<> {
        let holder = Rc::clone(&self.holder);
        move || holder.borrow_mut().give_back()
    }
}

/// Sends each order, each with its descriptor, and returns the first
/// error.
fn send(orders: vtium_platform::vt::broker::Orders, out: &mut Outbox<'_>) -> io::Result<()> {
    let mut first = Ok(());
    for (order, fd) in orders {
        if let Err(e) = out.order(order, fd) {
            first = first.and(Err(e));
        }
    }
    first
}
