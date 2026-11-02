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
    Context, Edit, Handler, Intro, Limits, Margins, ReplyKind, SixelState, Store, Switches,
};
use vtium_parse::{Chunk, Intermediates, Params, StringEnd};

use super::reply::Query;
use super::{State, Terminal, modes};
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
    /// The color register count XTSMGRAPHICS set, for the next sixel.
    registers: Option<u32>,
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
            registers: None,
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
            .field("registers", &self.registers)
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
            && self.registers == other.registers
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

    fn edit_store(&mut self, f: &mut dyn FnMut(&mut Edit<'_>)) {
        self.state.screen.with_images(|store, screens, cols| {
            f(&mut Edit::new(store, screens, cols));
        });
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

    fn margins(&self) -> Margins {
        let (top, bottom) = self.state.screen.margins();
        Margins { top, bottom }
    }

    fn index(&mut self, n: usize) {
        self.state.screen.move_down(n, true);
    }

    fn scroll_up(&mut self, n: usize) {
        self.state.screen.scroll_up(n);
    }

    fn sixel(&self) -> SixelState {
        self.state.sixel_state()
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

    /// The sixel modes as the mode table holds them, and the register count
    /// in force. A mode the table has no row for reads as its state at
    /// reset: 80 and 8452 reset, 1070 as `private-color-registers` says.
    fn sixel_state(&self) -> SixelState {
        // DECRQM's report: 1 or 3 set, 2 or 4 reset, 0 no row
        let mode = |number: u32, otherwise: bool| match modes::report(self, true, number) {
            1 | 3 => true,
            2 | 4 => false,
            _ => otherwise,
        };
        let limits = self.images.limits;
        SixelState {
            display: mode(80, false),
            private_registers: mode(1070, limits.private_registers),
            cursor_right: mode(8452, false),
            registers: self
                .images
                .registers
                .map_or(limits.registers, |n| n.min(limits.registers)),
        }
    }

    /// Sets the color register count XTSMGRAPHICS asks for, or with `None`
    /// returns it to `num-color-registers`; the next sixel takes it.
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "XTSMGRAPHICS, which the sixel handler's build adds, calls it"
        )
    )]
    pub(super) fn set_sixel_registers(&mut self, count: Option<u32>) {
        self.images.registers = count;
    }

    /// RIS: each handler returns to its state at start, before the store
    /// empties.
    pub(super) fn images_reset(&mut self) {
        for which in [ImageString::Sixel, ImageString::Kitty, ImageString::Iterm2] {
            self.with_handler(which, |h, ctx| h.reset(ctx));
        }
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

    /// The handler the router sends `which` to, if one is installed.
    pub fn image_handler(&self, which: ImageString) -> Option<&dyn Handler> {
        let slot = match which {
            ImageString::Sixel => &self.state.images.sixel,
            ImageString::Kitty => &self.state.images.kitty,
            ImageString::Iterm2 => &self.state.images.iterm2,
        };
        slot.as_deref()
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
    use vtium_image::{NewImage, Place, Protocol, Refusal};

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

    /// A handler that, at each string's start, notes what the context
    /// reports, and at its finish stores and places an 8 by 16 sixel at
    /// the cursor through one store edit, reserving part of its cost first
    /// and growing the reservation once.
    #[derive(Clone, Debug, Default, PartialEq)]
    struct Probe {
        margins: Option<Margins>,
        sixel: Option<SixelState>,
        placed: Option<Result<(u32, u32), Refusal>>,
    }

    impl Handler for Probe {
        fn start(&mut self, ctx: &mut dyn Context, _: Intro<'_>) {
            self.margins = Some(ctx.margins());
            self.sixel = Some(ctx.sixel());
        }
        fn feed(&mut self, _: &mut dyn Context, _: Chunk<'_>) {}
        fn finish(&mut self, ctx: &mut dyn Context) {
            let (col, row) = ctx.cursor();
            let at = Place {
                col: u32::try_from(col).unwrap_or(0),
                row: u32::try_from(row).unwrap_or(0),
                ..Place::default()
            };
            self.placed = ctx.edit(|e| {
                let mut r = e.reserve(Protocol::SIXEL, 0, 100)?;
                e.grow(&mut r, 400)?;
                let image = NewImage {
                    protocol: Protocol::SIXEL,
                    width: 8,
                    height: 16,
                    pixels: vec![0xffff_ffff; 128],
                    id: 0,
                    number: 0,
                    transient: false,
                };
                let key = e.add_image(image, Some(r))?;
                let p = e.place(key, at)?;
                e.extent(p).ok_or(Refusal::NotFound)
            });
        }
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

    /// An 8 by 4 terminal, its cell 8 by 16 pixels, with `limit` bytes for
    /// images and the probe as its sixel handler.
    fn probed(limit: u64) -> Terminal {
        let mut t = Terminal::new(8, 4, 16);
        t.screen_mut().set_image_limit(limit);
        t.set_image_switches(Switches {
            sixel: true,
            kitty: false,
            iterm2: false,
        });
        t.set_image_handler(ImageString::Sixel, Some(Box::new(Probe::default())));
        t
    }

    fn probe(t: &Terminal) -> Probe {
        t.image_handler(ImageString::Sixel)
            .and_then(|h| h.as_any().downcast_ref::<Probe>())
            .cloned()
            .unwrap_or_default()
    }

    /// DECRQM's answer for DEC mode `n`.
    fn decrqm(t: &mut Terminal, n: u32) -> u8 {
        let _ = t.take_replies();
        let _ = t.input(format!("\x1b[?{n}$p").as_bytes());
        let reply = String::from_utf8(t.take_replies()).unwrap_or_default();
        let prefix = format!("\x1b[?{n};");
        reply
            .strip_prefix(&prefix)
            .and_then(|r| r.strip_suffix("$y"))
            .and_then(|v| v.parse().ok())
            .unwrap_or(u8::MAX)
    }

    #[test]
    fn the_context_reports_the_scroll_regions_margins() {
        let mut t = probed(1 << 20);
        let _ = t.input(b"\x1bPq\x1b\\");
        assert_eq!(probe(&t).margins, Some(Margins { top: 0, bottom: 3 }));
        let _ = t.input(b"\x1b[2;3r\x1bPq\x1b\\");
        assert_eq!(probe(&t).margins, Some(Margins { top: 1, bottom: 2 }));
        let _ = t.input(b"\x1b[r\x1bPq\x1b\\");
        assert_eq!(probe(&t).margins, Some(Margins { top: 0, bottom: 3 }));
    }

    #[test]
    fn the_context_reports_the_sixel_modes_as_decrqm_does() {
        for private in [true, false] {
            let mut t = probed(1 << 20);
            t.set_image_limits(Limits {
                private_registers: private,
                ..Limits::default()
            });
            for set in [true, false] {
                let verb = if set { 'h' } else { 'l' };
                let _ =
                    t.input(format!("\x1b[?80{verb}\x1b[?1070{verb}\x1b[?8452{verb}").as_bytes());
                let _ = t.input(b"\x1bPq\x1b\\");
                let got = probe(&t).sixel;
                // a mode the table has no row for reads as its state at
                // reset; one it has reads as DECRQM reports it
                let expect = |t: &mut Terminal, n: u32, reset: bool| match decrqm(t, n) {
                    1 | 3 => Some(true),
                    2 | 4 => Some(false),
                    0 => Some(reset),
                    _ => None,
                };
                let display = expect(&mut t, 80, false);
                let private_registers = expect(&mut t, 1070, private);
                let cursor_right = expect(&mut t, 8452, false);
                assert!(display.is_some() && private_registers.is_some() && cursor_right.is_some());
                assert_eq!(got.map(|s| s.display), display, "mode 80");
                assert_eq!(
                    got.map(|s| s.private_registers),
                    private_registers,
                    "mode 1070"
                );
                assert_eq!(got.map(|s| s.cursor_right), cursor_right, "mode 8452");
            }
        }
    }

    #[test]
    fn the_context_reports_the_register_count_in_force() {
        let mut t = probed(1 << 20);
        let count = |t: &mut Terminal| {
            let _ = t.input(b"\x1bPq\x1b\\");
            probe(t).sixel.map(|s| s.registers)
        };
        assert_eq!(count(&mut t), Some(1024), "num-color-registers");
        t.state.set_sixel_registers(Some(16));
        assert_eq!(count(&mut t), Some(16));
        t.state.set_sixel_registers(Some(5000));
        assert_eq!(count(&mut t), Some(1024), "never past num-color-registers");
        t.set_image_limits(Limits {
            registers: 256,
            ..Limits::default()
        });
        t.state.set_sixel_registers(None);
        assert_eq!(count(&mut t), Some(256));
    }

    #[test]
    fn a_store_edit_charges_the_budget_once_and_anchors_to_the_line() {
        // 8 by 16 pixels at 4 bytes, and the image's own charge
        let cost = 8 * 16 * 4 + vtium_image::IMAGE_OVERHEAD;
        let mut t = probed(1 << 20);
        let _ = t.input(b"\x1b[3;2H\x1bPq\x1b\\");
        assert_eq!(probe(&t).placed, Some(Ok((1, 1))));
        assert_eq!(
            t.screen().images().bytes(),
            cost,
            "the reservation counts toward the image's cost, once"
        );
        let at = |t: &Terminal| -> Vec<(i64, u32)> {
            t.screen()
                .placements()
                .iter()
                .map(|p| (p.row, p.col))
                .collect()
        };
        assert_eq!(at(&t), [(2, 1)]);
        // a line feed on the bottom row scrolls the screen up one row: the
        // placement moves with the line that holds it
        let _ = t.input(b"\x1b[4;1H\n");
        assert_eq!(at(&t), [(1, 1)]);
        // RIS empties the store and gives every byte back
        let _ = t.input(b"\x1bc");
        assert_eq!(t.screen().images().bytes(), 0);

        // a budget below the cost refuses it, and the reservation goes back
        let mut t = probed(cost - 1);
        let _ = t.input(b"\x1bPq\x1b\\");
        assert_eq!(probe(&t).placed, Some(Err(Refusal::NoSpace)));
        assert_eq!(t.screen().images().bytes(), 0);
        assert_eq!(t.screen().images().image_count(), 0);
    }

    /// A handler that keeps a reservation from a string's finish until RIS,
    /// as a kitty upload in chunks keeps one between its strings.
    #[derive(Clone, Debug, Default, PartialEq)]
    struct Holding(Option<vtium_image::Reservation>);

    impl Handler for Holding {
        fn start(&mut self, _: &mut dyn Context, _: Intro<'_>) {}
        fn feed(&mut self, _: &mut dyn Context, _: Chunk<'_>) {}
        fn finish(&mut self, ctx: &mut dyn Context) {
            self.0 = ctx.reserve(Protocol::KITTY, 0, 4096).ok();
        }
        fn cancel(&mut self, _: &mut dyn Context) {}
        fn decoded(&mut self, _: &mut dyn Context, _: Reply) {}
        fn reset(&mut self, ctx: &mut dyn Context) {
            if let Some(r) = self.0.take() {
                ctx.unreserve(r);
            }
        }
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

    #[test]
    fn ris_resets_each_handler_so_it_gives_back_what_it_holds() {
        let mut t = Terminal::new(8, 4, 16);
        t.screen_mut().set_image_limit(1 << 20);
        t.set_image_switches(Switches {
            sixel: false,
            kitty: true,
            iterm2: false,
        });
        t.set_image_handler(ImageString::Kitty, Some(Box::new(Holding::default())));
        let _ = t.input(b"\x1b_Gx\x1b\\");
        assert_eq!(t.screen().images().bytes(), 4096);
        let _ = t.input(b"\x1bc");
        assert_eq!(t.screen().images().bytes(), 0);
        let held = t
            .image_handler(ImageString::Kitty)
            .and_then(|h| h.as_any().downcast_ref::<Holding>())
            .map(|h| h.0.is_some());
        assert_eq!(held, Some(false));
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
