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
    Context, Handler, ImageKey, Intro, Limits, NewImage, Place, PlacementKey, Refusal, ReplyKind,
    Store, Switches,
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

/// Two routers compare by what they route and what is out; a handler's
/// own state is its own.
impl PartialEq for Router {
    fn eq(&self, other: &Self) -> bool {
        let slots = |r: &Self| (r.sixel.is_some(), r.kitty.is_some(), r.iterm2.is_some());
        slots(self) == slots(other)
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

    fn add_image(&mut self, image: NewImage) -> Result<ImageKey, Refusal> {
        self.state.screen.add_image(image)
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
        if !sixel || !self.images.on(ImageString::Sixel) {
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
        if end.is_complete() {
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
            if !kitty || !self.images.on(ImageString::Kitty) {
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
        if !args.starts_with(b"File=") || !self.images.on(ImageString::Iterm2) {
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
    pub fn set_image_handler(&mut self, which: ImageString, handler: Option<Box<dyn Handler>>) {
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
