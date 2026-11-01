//! The router to the image protocols' handlers, the context they act
//! through, and the barrier a decode request holds (ADR-1100, D6, D7).
//!
//! The router sends DCS `q` with no intermediate to the sixel handler,
//! an APC string whose first byte is `G` to the kitty graphics handler,
//! and a complete OSC `1337;File=` to the iTerm2 compatibility handler,
//! each only while its protocol is switched on and a handler is
//! installed; every other string stays as it was. While a handler's
//! request to the image helper is out, the input call takes nothing more,
//! so a later string cannot land before an earlier image.

use core::fmt;

use vtium_image::decode::{Reply, Request};
use vtium_image::{
    Context, Handler, ImageKey, Intro, Limits, NewImage, Place, PlacementKey, Protocol, Refusal,
    ReplyKind, Reservation, Store, Switches,
};
use vtium_parse::{Chunk, Intermediates, Params, StringEnd};

use super::reply::Query;
use super::{State, Terminal};
use crate::bounds::INPUT_WORK_MAX;

/// The control strings the router sends to a handler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageString {
    /// DCS `q`: sixel.
    Sixel,
    /// APC `G`: the kitty graphics protocol.
    Kitty,
    /// OSC `1337;File=`: iTerm2's inline images.
    Iterm2,
}

/// The handlers, the string being fed, the protocol switches, the limits,
/// and the request out.
#[derive(Clone)]
pub(super) struct Router {
    sixel: Option<Box<dyn Handler>>,
    kitty: Option<Box<dyn Handler>>,
    iterm2: Option<Box<dyn Handler>>,
    /// The string a handler is being fed.
    active: Option<ImageString>,
    /// An APC string whose first byte has not come.
    apc_open: bool,
    switches: Switches,
    limits: Limits,
    /// The host has an image helper.
    helper: bool,
    /// The request out, the handler it answers, and whether the host has
    /// taken it.
    awaiting: Option<(ImageString, Request, bool)>,
}

impl Default for Router {
    fn default() -> Self {
        Self {
            sixel: None,
            kitty: None,
            iterm2: None,
            active: None,
            apc_open: false,
            switches: Switches {
                sixel: false,
                kitty: false,
                iterm2: false,
            },
            limits: Limits::default(),
            helper: false,
            awaiting: None,
        }
    }
}

impl fmt::Debug for Router {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Router")
            .field("sixel", &self.sixel.is_some())
            .field("kitty", &self.kitty.is_some())
            .field("iterm2", &self.iterm2.is_some())
            .field("active", &self.active)
            .field("switches", &self.switches)
            .field("limits", &self.limits)
            .field("helper", &self.helper)
            .field("awaiting", &self.awaiting)
            .finish()
    }
}

/// Two routers compare by their handlers, each in its state, by what they
/// route and by what is out.
impl PartialEq for Router {
    fn eq(&self, other: &Self) -> bool {
        let same = |a: &Option<Box<dyn Handler>>, b: &Option<Box<dyn Handler>>| match (a, b) {
            (Some(a), Some(b)) => a.same(b.as_ref()),
            (None, None) => true,
            _ => false,
        };
        same(&self.sixel, &other.sixel)
            && same(&self.kitty, &other.kitty)
            && same(&self.iterm2, &other.iterm2)
            && self.active == other.active
            && self.apc_open == other.apc_open
            && self.switches == other.switches
            && self.limits == other.limits
            && self.helper == other.helper
            && self.awaiting == other.awaiting
    }
}

impl Router {
    fn slot(&mut self, which: ImageString) -> &mut Option<Box<dyn Handler>> {
        match which {
            ImageString::Sixel => &mut self.sixel,
            ImageString::Kitty => &mut self.kitty,
            ImageString::Iterm2 => &mut self.iterm2,
        }
    }

    fn on(&self, which: ImageString) -> bool {
        let on = match which {
            ImageString::Sixel => self.switches.sixel,
            ImageString::Kitty => self.switches.kitty,
            ImageString::Iterm2 => self.switches.iterm2,
        };
        let installed = match which {
            ImageString::Sixel => self.sixel.is_some(),
            ImageString::Kitty => self.kitty.is_some(),
            ImageString::Iterm2 => self.iterm2.is_some(),
        };
        on && installed
    }
}

/// What a handler reaches: the terminal's state, for the string `which`.
struct Ctx<'a> {
    state: &'a mut State,
    which: ImageString,
}

impl Context for Ctx<'_> {
    fn store(&self) -> &Store {
        self.state.screen.images()
    }

    fn reserve(&mut self, protocol: Protocol, id: u32, bytes: u64) -> Result<Reservation, Refusal> {
        self.state
            .screen
            .with_images(|store, screens, _| store.reserve(protocol, id, bytes, screens))
    }

    fn grow(&mut self, reservation: &mut Reservation, more: u64) -> Result<(), Refusal> {
        self.state
            .screen
            .with_images(|store, screens, _| store.grow(reservation, more, screens))
    }

    fn unreserve(&mut self, reservation: Reservation) {
        self.state
            .screen
            .with_images(|store, _, _| store.unreserve(reservation));
    }

    fn add_image(
        &mut self,
        image: NewImage,
        reservation: Option<Reservation>,
    ) -> Result<ImageKey, Refusal> {
        self.state
            .screen
            .with_images(|store, screens, _| store.add_image(image, reservation, screens))
    }

    fn drop_image(&mut self, image: ImageKey) {
        self.state
            .screen
            .with_images(|store, _, _| store.drop_image(image));
    }

    fn find_image(&mut self, id: u32, number: u32) -> Option<ImageKey> {
        self.state
            .screen
            .with_images(|store, _, _| store.find_image(id, number))
    }

    fn touch(&mut self, image: ImageKey) {
        self.state
            .screen
            .with_images(|store, _, _| store.touch(image));
    }

    fn free_id(&mut self) -> u32 {
        self.state.screen.with_images(|store, _, _| store.free_id())
    }

    fn place(&mut self, image: ImageKey, place: Place) -> Result<PlacementKey, Refusal> {
        self.state.screen.place_image(image, place)
    }

    fn helper(&self) -> bool {
        self.state.images.helper
    }

    fn request(&mut self, request: Request) {
        self.state.images.awaiting = Some((self.which, request, false));
    }

    fn cell_size(&self) -> (u32, u32) {
        self.state.screen.images().cell()
    }

    fn grid(&self) -> (usize, usize) {
        (self.state.screen.cols(), self.state.screen.rows())
    }

    fn cursor(&self) -> (usize, usize) {
        self.state.screen.cursor()
    }

    fn move_cursor(&mut self, col: usize, row: usize) {
        self.state.screen.restore_cursor(col, row);
    }

    fn index(&mut self) {
        self.state.screen.move_down(1, true);
    }

    fn reply(&mut self, kind: ReplyKind, bytes: &[u8]) {
        let query = match kind {
            ReplyKind::KittyGraphics => Query::ImageGraphics,
        };
        self.state.reply(query, bytes);
    }

    fn switches(&self) -> Switches {
        self.state.images.switches
    }

    fn limits(&self) -> Limits {
        self.state.images.limits
    }

    fn add_work(&mut self, work: u64) {
        self.state.screen.add_image_work(work);
    }
}

impl State {
    /// Calls the handler for `which` with a context over this state; the
    /// handler is out of its slot meanwhile.
    fn with_handler(
        &mut self,
        which: ImageString,
        f: impl FnOnce(&mut dyn Handler, &mut dyn Context),
    ) {
        let Some(mut handler) = self.images.slot(which).take() else {
            return;
        };
        {
            let mut ctx = Ctx { state: self, which };
            f(handler.as_mut(), &mut ctx);
        }
        *self.images.slot(which) = Some(handler);
    }

    /// Whether the router sends `which` to its handler: the protocol is
    /// switched on, a handler is installed, and the budget is above 0.
    fn image_offered(&self, which: ImageString) -> bool {
        self.images.on(which) && self.screen.images().limit() > 0
    }

    /// DCS `q` with no intermediate goes to the sixel handler while sixel
    /// is on: returns whether it did.
    pub(super) fn image_dcs_hook(
        &mut self,
        params: &Params,
        intermediates: Intermediates,
        final_byte: u8,
    ) -> bool {
        let sixel =
            params.marker().is_none() && intermediates.as_bytes().is_empty() && final_byte == b'q';
        if !sixel || !self.image_offered(ImageString::Sixel) {
            return false;
        }
        self.images.active = Some(ImageString::Sixel);
        self.with_handler(ImageString::Sixel, |h, ctx| {
            h.start(ctx, Intro::Dcs(params))
        });
        true
    }

    pub(super) fn image_put(&mut self, data: Chunk<'_>) {
        if let Some(which) = self.images.active {
            self.with_handler(which, |h, ctx| h.feed(ctx, data));
        }
    }

    /// The string ends: the handler finishes it when it ended with its
    /// terminator, and cancels it otherwise.
    pub(super) fn image_end(&mut self, end: StringEnd) {
        let Some(which) = self.images.active.take() else {
            return;
        };
        // a protocol switched off, or a budget set to 0, while the string
        // streamed in stores nothing (D10)
        if end.is_complete() && self.image_offered(which) {
            self.with_handler(which, |h, ctx| h.finish(ctx));
        } else {
            self.with_handler(which, |h, ctx| h.cancel(ctx));
        }
    }

    pub(super) fn image_apc_start(&mut self) {
        self.images.active = None;
        self.images.apc_open = true;
    }

    /// An APC string's data: its first byte chooses the kitty handler, and
    /// what follows the `G` is fed to it.
    pub(super) fn image_apc_put(&mut self, data: Chunk<'_>) {
        if core::mem::take(&mut self.images.apc_open) {
            let kitty = matches!(data, Chunk::Bytes([b'G', ..]));
            if !kitty || !self.image_offered(ImageString::Kitty) {
                return;
            }
            self.images.active = Some(ImageString::Kitty);
            self.with_handler(ImageString::Kitty, |h, ctx| h.start(ctx, Intro::String));
            if let Chunk::Bytes([_, rest @ ..]) = data {
                if !rest.is_empty() {
                    self.image_put(Chunk::Bytes(rest));
                }
            }
            return;
        }
        self.image_put(data);
    }

    pub(super) fn image_apc_end(&mut self, end: StringEnd) {
        self.images.apc_open = false;
        self.image_end(end);
    }

    /// A complete OSC `1337;File=` goes to the iTerm2 handler while it is
    /// on, whole: returns whether it did.
    pub(super) fn image_osc(&mut self, data: &[u8]) -> bool {
        let Some(args) = data.strip_prefix(b"1337;") else {
            return false;
        };
        if !args.starts_with(b"File=") || !self.image_offered(ImageString::Iterm2) {
            return false;
        }
        self.with_handler(ImageString::Iterm2, |h, ctx| {
            h.start(ctx, Intro::String);
            h.feed(ctx, Chunk::Bytes(args));
            h.finish(ctx);
        });
        true
    }

    /// The input call stops while a request is out, and once its image
    /// work reaches [`INPUT_WORK_MAX`] (D5).
    pub(super) fn images_paused(&self) -> bool {
        self.images.awaiting.is_some() || self.screen.images().work() >= INPUT_WORK_MAX
    }
}

impl Terminal {
    /// Installs the handler the router sends `which` to, or removes it.
    /// The outgoing handler is cancelled first, so it gives back what it
    /// reserved, and a string it was being fed ends.
    pub fn set_image_handler(&mut self, which: ImageString, handler: Option<Box<dyn Handler>>) {
        self.state.with_handler(which, |h, ctx| h.cancel(ctx));
        if self.state.images.active == Some(which) {
            self.state.images.active = None;
        }
        *self.state.images.slot(which) = handler;
    }

    /// Turns the image protocols on or off: an off protocol's strings are
    /// ignored as every string vtium does not take is.
    pub fn set_image_switches(&mut self, switches: Switches) {
        self.state.images.switches = switches;
    }

    pub fn image_switches(&self) -> Switches {
        self.state.images.switches
    }

    /// Sets the limits D11's settings give the handlers.
    pub fn set_image_limits(&mut self, limits: Limits) {
        self.state.images.limits = limits;
    }

    pub fn image_limits(&self) -> Limits {
        self.state.images.limits
    }

    /// Says whether the host has an image helper to answer requests.
    pub fn set_image_helper(&mut self, on: bool) {
        self.state.images.helper = on;
    }

    /// The request a handler made, once, for the host to send to the image
    /// helper. Until [`image_reply`](Self::image_reply) hands its answer
    /// back, [`input`](Self::input) takes nothing.
    pub fn take_image_request(&mut self) -> Option<Request> {
        match self.state.images.awaiting.as_mut() {
            Some((_, request, taken)) if !*taken => {
                *taken = true;
                Some(request.clone())
            }
            _ => None,
        }
    }

    /// Whether a request is out.
    pub fn image_request_pending(&self) -> bool {
        self.state.images.awaiting.is_some()
    }

    /// Hands the helper's answer to the handler that asked, its sizes
    /// checked against the request, and lets input go on.
    pub fn image_reply(&mut self, reply: Reply) {
        let Some((which, request, _)) = self.state.images.awaiting.take() else {
            return;
        };
        let reply = reply.checked(&request);
        self.state
            .with_handler(which, |h, ctx| h.decoded(ctx, reply));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A handler that keeps the bytes it was fed.
    #[derive(Clone, Debug, Default, PartialEq)]
    struct Keep(Vec<u8>);

    impl Handler for Keep {
        fn start(&mut self, _: &mut dyn Context, _: Intro<'_>) {}
        fn feed(&mut self, _: &mut dyn Context, data: Chunk<'_>) {
            if let Chunk::Bytes(b) = data {
                self.0.extend_from_slice(b);
            }
        }
        fn finish(&mut self, _: &mut dyn Context) {}
        fn cancel(&mut self, _: &mut dyn Context) {}
        fn decoded(&mut self, _: &mut dyn Context, _: Reply) {}
        fn clone_box(&self) -> Box<dyn Handler> {
            Box::new(self.clone())
        }
        fn as_any(&self) -> &dyn core::any::Any {
            self
        }
        fn same(&self, other: &dyn Handler) -> bool {
            other.as_any().downcast_ref::<Self>() == Some(self)
        }
    }

    fn fed(bytes: &[u8]) -> Terminal {
        let mut t = Terminal::new(4, 2, 0);
        t.screen_mut().set_image_limit(1 << 20);
        t.set_image_switches(Switches {
            sixel: false,
            kitty: true,
            iterm2: false,
        });
        t.set_image_handler(ImageString::Kitty, Some(Box::new(Keep::default())));
        let _ = t.input(bytes);
        t
    }

    #[test]
    fn two_terminals_compare_by_their_handlers_state() {
        assert!(fed(b"\x1b_Gab").state == fed(b"\x1b_Gab").state);
        assert!(
            fed(b"\x1b_Gab").state != fed(b"\x1b_Gax").state,
            "a half-fed string differs"
        );
        let t = fed(b"\x1b_Gab");
        assert!(
            t.state.clone() == t.state,
            "a clone keeps the handler's state"
        );
    }
}
