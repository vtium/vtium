//! The router to the image protocols' handlers, the context they act
//! through, and the barrier a decode request holds (ADR-1100, D6, D7).
//!
//! The router sends DCS `q` with no intermediate to the sixel handler,
//! an APC string whose first byte is `G` to the kitty graphics handler,
//! and a complete OSC `1337;File=`, `1337;MultipartFile=`,
//! `1337;FilePart=` or `1337;FileEnd` to the iTerm2 compatibility handler,
//! each only while its protocol is switched on and a handler is
//! installed; every other string stays as it was. An OSC 1337 string that
//! vtium drops, past the OSC bound or cut short, cancels the iTerm2
//! handler, so a transfer in parts it belonged to ends (ADR-1103, D1). While a handler's
//! request to the image helper is out, the input call takes nothing more,
//! so a later string cannot land before an earlier image.
//!
//! A handler that drops what a program sent, where its protocol answers
//! nothing, says why through [`Context::note`]. The terminal counts each
//! protocol's reasons and hands the host a note for the host's log the
//! first time a reason comes and again each time its count reaches a power
//! of ten ([`Terminal::take_image_notes`]).

use core::fmt;

use vtium_image::decode::{Reply, Request};
use vtium_image::{
    Cause, Context, Edit, Handler, Intro, Limits, Margins, ReplyKind, SixelState, Store, Switches,
};
use vtium_parse::{Chunk, Intermediates, Params, StringEnd};

use super::reply::Query;
use super::{State, Terminal, modes};
use crate::bounds::{IMAGE_NOTE_BYTES_MAX, IMAGE_NOTE_REASONS_MAX, INPUT_WORK_MAX};

/// The control strings the router sends to a handler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageString {
    /// DCS `q`: sixel.
    Sixel,
    /// APC `G`: the kitty graphics protocol.
    Kitty,
    /// OSC `1337;File=` and the transfer in parts: iTerm2's inline images.
    Iterm2,
}

impl ImageString {
    /// The protocol's name, for a log line.
    pub fn name(self) -> &'static str {
        match self {
            Self::Sixel => "sixel",
            Self::Kitty => "kitty graphics",
            Self::Iterm2 => "iTerm2",
        }
    }
}

/// Why a handler dropped what a program sent, for the host's log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageNote {
    /// The protocol whose handler wrote the note.
    pub protocol: ImageString,
    /// Who can act on the reason: the program's author, or the operator.
    pub cause: Cause,
    /// The handler's reason, at most `IMAGE_NOTE_BYTES_MAX` bytes of it.
    pub why: String,
    /// How many notes of this protocol and reason the terminal has had,
    /// this one included: 1, or a power of ten.
    pub count: u64,
}

/// The handlers' notes: how often each protocol's reason has come, and
/// the notes the host has not taken.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Notes {
    /// Each protocol, cause and reason the terminal has had, with its
    /// count, at most `IMAGE_NOTE_REASONS_MAX` of them. RIS keeps them, so
    /// a program cannot start a count again.
    seen: Vec<(ImageString, Cause, String, u64)>,
    /// The notes the host has not taken. They need no bound of their own:
    /// each counted reason puts one out at counts 1, 10, 100 and so on, the
    /// 20 powers of ten a `u64` holds, so at most `IMAGE_NOTE_REASONS_MAX` ×
    /// 20, 1,280, wait between two takes.
    out: Vec<ImageNote>,
}

impl Notes {
    /// Counts `why` from `protocol`'s handler, and keeps a note for the host
    /// when the count is 1 or a power of ten. A new reason once the table
    /// is full is counted nowhere and reaches no log.
    fn add(&mut self, protocol: ImageString, cause: Cause, why: &str) {
        let why = cut(why, IMAGE_NOTE_BYTES_MAX);
        let full = self.seen.len() >= IMAGE_NOTE_REASONS_MAX;
        let found = self
            .seen
            .iter_mut()
            .find(|(p, c, w, _)| *p == protocol && *c == cause && w == why);
        let count = match found {
            Some((_, _, _, n)) => {
                *n = n.saturating_add(1);
                *n
            }
            None if full => return,
            None => {
                self.seen.push((protocol, cause, why.to_owned(), 1));
                1
            }
        };
        if power_of_ten(count) {
            self.out.push(ImageNote {
                protocol,
                cause,
                why: why.to_owned(),
                count,
            });
        }
    }

    fn take(&mut self) -> Vec<ImageNote> {
        core::mem::take(&mut self.out)
    }
}

/// `s` cut to at most `max` bytes, at a character's boundary.
fn cut(s: &str, max: usize) -> &str {
    let mut end = s.len().min(max);
    while !s.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    s.get(..end).unwrap_or_default()
}

/// Whether `n` is 1, 10, 100 and so on.
fn power_of_ten(mut n: u64) -> bool {
    while n >= 10 && n.is_multiple_of(10) {
        n /= 10;
    }
    n == 1
}

/// The handlers, the string being fed, the protocol switches, the limits,
/// the request out, and the notes for the host's log.
#[derive(Clone)]
pub(super) struct Router {
    sixel: Option<Box<dyn Handler>>,
    kitty: Option<Box<dyn Handler>>,
    iterm2: Option<Box<dyn Handler>>,
    /// The string a handler is being fed.
    active: Option<ImageString>,
    /// The handler being called, which is out of its slot meanwhile and
    /// still counts as installed.
    running: Option<ImageString>,
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
    notes: Notes,
}

impl Default for Router {
    fn default() -> Self {
        Self {
            sixel: None,
            kitty: None,
            iterm2: None,
            active: None,
            running: None,
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
            notes: Notes::default(),
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
            .field("running", &self.running)
            .field("switches", &self.switches)
            .field("limits", &self.limits)
            .field("registers", &self.registers)
            .field("helper", &self.helper)
            .field("awaiting", &self.awaiting)
            .field("notes", &self.notes)
            .finish()
    }
}

/// Two routers compare by their handlers, each in its state, by what they
/// route, by what is out and by their notes.
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
            && self.running == other.running
            && self.apc_open == other.apc_open
            && self.switches == other.switches
            && self.limits == other.limits
            && self.registers == other.registers
            && self.helper == other.helper
            && self.awaiting == other.awaiting
            && self.notes == other.notes
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
        let installed = self.running == Some(which)
            || match which {
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
        let (left, right) = self.state.screen.lr_margins();
        Margins {
            top,
            bottom,
            left,
            right,
        }
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

    fn colors(&self) -> Option<vtium_image::Colors> {
        Some(self.state.image_colors())
    }

    fn note(&mut self, cause: Cause, why: &str) {
        self.state.images.notes.add(self.which, cause, why);
    }
}

impl State {
    /// The terminal's colors as a palette-index picture takes them: the
    /// palette its frames draw through, the palette vtium.conf configured
    /// with the colors a program has since set through OSC 4, 10, 11, 12, 17
    /// and 19.
    fn image_colors(&self) -> vtium_image::Colors {
        use crate::palette::Special;
        let p = &self.colors;
        let rgb = |c: crate::style::Rgb| [c.r, c.g, c.b];
        vtium_image::Colors {
            table: core::array::from_fn(|i| rgb(p.color(u8::try_from(i).unwrap_or(u8::MAX)))),
            fg: rgb(p.fg()),
            bg: rgb(p.bg()),
            selection_bg: p.special(Special::SelectionBg).map(rgb),
            selection_fg: p.special(Special::SelectionFg).map(rgb),
            cursor: p.cursor().map(rgb),
        }
    }

    /// After the terminal's colors change, makes the pixels of every kitty
    /// palette-index picture (`f=8` and `f=16`) in both buffers again in
    /// the new colors. The kitty graphics specification says such a picture
    /// "is drawn using the current colors of the terminal and is
    /// automatically redrawn when they change". Every place that changes the
    /// colors marks every cell changed, so the cells under the pictures are
    /// redrawn too. The store counts each pixel it looks at as input work,
    /// so a flood of color changes stops the input call early.
    ///
    /// References:
    /// - ADR-1100 D15: when the colors change, and what the store does.
    pub(super) fn recolor_images(&mut self) {
        let table = self.image_colors().resolved();
        self.screen.with_images(|store, _, _| store.recolor(&table));
    }

    /// Calls the handler for `which` with a context over this state. The
    /// handler is out of its slot meanwhile, and is marked as running, so
    /// the protocol still reads as offered from inside the call, as the
    /// mode rows ask.
    fn with_handler(&mut self, which: ImageString, f: impl FnOnce(&mut dyn Handler, &mut Ctx<'_>)) {
        let Some(mut handler) = self.images.slot(which).take() else {
            return;
        };
        let outer = self.images.running.replace(which);
        {
            let mut ctx = Ctx { state: self, which };
            f(handler.as_mut(), &mut ctx);
        }
        self.images.running = outer;
        *self.images.slot(which) = Some(handler);
    }

    /// Each handler drops what it holds open between its strings, such
    /// as a kitty upload in chunks, with no answer: the session ended.
    pub(super) fn images_withdraw(&mut self) {
        for which in [ImageString::Sixel, ImageString::Kitty, ImageString::Iterm2] {
            self.with_handler(which, |h, ctx| h.withdraw(ctx));
        }
    }

    /// Whether the router sends `which` to its handler: the protocol is
    /// switched on, a handler is installed, and the budget is above 0.
    pub(super) fn image_offered(&self, which: ImageString) -> bool {
        self.images.on(which) && self.screen.images().limit() > 0
    }

    /// The sixel modes as the mode table holds them, and the register count
    /// in force. A mode the table has no row for reads as its state at
    /// reset: 80 and 8452 reset, 1070 as `private-color-registers` says.
    pub(super) fn sixel_state(&self) -> SixelState {
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

    /// The limits D11's settings give the handlers: XTSMGRAPHICS reads the
    /// register count and the largest picture from them.
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "XTSMGRAPHICS, which the sixel handler's build adds, calls it"
        )
    )]
    pub(super) fn image_limits(&self) -> Limits {
        self.images.limits
    }

    /// Sets the color register count XTSMGRAPHICS asks for, or with `None`
    /// returns it to `num-color-registers`; the next sixel takes it.
    ///
    /// A change of the count in force resets the sixel handler, so the
    /// registers the pictures share start again from the defaults: a
    /// register past the new count would otherwise keep a color no picture
    /// can name.
    pub(super) fn set_sixel_registers(&mut self, count: Option<u32>) {
        let before = self.sixel_state().registers;
        self.images.registers = count;
        if self.sixel_state().registers != before {
            self.with_handler(ImageString::Sixel, |h, ctx| h.reset(ctx));
        }
    }

    /// Ends a sixel string being fed, giving back what it holds, and
    /// resets the sixel handler, dropping the registers the pictures
    /// share.
    fn drop_sixel(&mut self) {
        if self.images.active == Some(ImageString::Sixel) {
            self.images.active = None;
        }
        self.with_handler(ImageString::Sixel, |h, ctx| {
            h.cancel(ctx);
            h.reset(ctx);
        });
    }

    /// RIS: each handler returns to its state at start, before the store
    /// empties, and the register count returns to `num-color-registers`.
    pub(super) fn images_reset(&mut self) {
        for which in [ImageString::Sixel, ImageString::Kitty, ImageString::Iterm2] {
            self.with_handler(which, |h, ctx| h.reset(ctx));
        }
        self.images.registers = None;
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

    /// A run of a DCS or APC string's data to the handler being fed, with the
    /// work the input call may still do: returns how many bytes it took,
    /// fewer than all once it reached the bound (D5).
    pub(super) fn image_put_run(&mut self, run: &[u8]) -> usize {
        let Some(which) = self.images.active else {
            return run.len();
        };
        let left = INPUT_WORK_MAX.saturating_sub(self.screen.images().work());
        let mut taken = run.len();
        self.with_handler(which, |h, ctx| taken = h.feed_some(ctx, run, left));
        taken
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

    /// An APC string starts. While kitty graphics is not offered, the kitty
    /// handler is cancelled and withdrawn, so an upload in chunks left open
    /// when kitty was switched off gives back what it holds (ADR-1100, D6).
    pub(super) fn image_apc_start(&mut self) {
        self.images.active = None;
        self.images.apc_open = true;
        if !self.image_offered(ImageString::Kitty) {
            self.with_handler(ImageString::Kitty, |h, ctx| {
                h.cancel(ctx);
                h.withdraw(ctx);
            });
        }
    }

    /// An APC string's data: its first byte chooses the kitty handler, and
    /// what follows the `G` is fed to it.
    pub(super) fn image_apc_put(&mut self, data: Chunk<'_>) {
        if core::mem::take(&mut self.images.apc_open) {
            let kitty = matches!(data, Chunk::Bytes([b'G', ..]));
            if !kitty {
                return;
            }
            if !self.image_offered(ImageString::Kitty) {
                return;
            }
            self.images.active = Some(ImageString::Kitty);
            self.with_handler(ImageString::Kitty, |h, ctx| h.start(ctx, Intro::String));
            if let Chunk::Bytes([_, rest @ ..]) = data
                && !rest.is_empty()
            {
                self.image_put(Chunk::Bytes(rest));
            }
            return;
        }
        self.image_put(data);
    }

    /// A run of an APC string's data: as [`image_apc_put`](Self::image_apc_put)
    /// for its first byte, and the rest to the kitty handler with the work
    /// the input call may still do. Returns how many bytes it took, fewer
    /// than all once the handler reached the bound (D5).
    pub(super) fn image_apc_put_run(&mut self, run: &[u8]) -> usize {
        if !self.images.apc_open {
            return self.image_put_run(run);
        }
        let Some((&first, rest)) = run.split_first() else {
            return 0;
        };
        self.images.apc_open = false;
        if first != b'G' {
            return run.len();
        }
        if !self.image_offered(ImageString::Kitty) {
            return run.len();
        }
        self.images.active = Some(ImageString::Kitty);
        self.with_handler(ImageString::Kitty, |h, ctx| h.start(ctx, Intro::String));
        if rest.is_empty() {
            return 1;
        }
        self.image_put_run(rest).saturating_add(1)
    }

    pub(super) fn image_apc_end(&mut self, end: StringEnd) {
        // an APC string with no data at all is no kitty string
        self.images.apc_open = false;
        self.image_end(end);
    }

    /// A complete OSC `1337;File=`, `1337;MultipartFile=`,
    /// `1337;FilePart=` or `1337;FileEnd` goes to the iTerm2 handler while
    /// it is on, whole: returns whether it did.
    pub(super) fn image_osc(&mut self, data: &[u8]) -> bool {
        let Some(args) = data.strip_prefix(b"1337;") else {
            return false;
        };
        let picture = [&b"File="[..], b"MultipartFile=", b"FilePart="]
            .iter()
            .any(|p| args.starts_with(p))
            || args == b"FileEnd";
        if !picture || !self.image_offered(ImageString::Iterm2) {
            return false;
        }
        self.with_handler(ImageString::Iterm2, |h, ctx| {
            h.start(ctx, Intro::String);
            h.feed(ctx, Chunk::Bytes(args));
            h.finish(ctx);
        });
        true
    }

    /// An OSC string vtium drops, past the OSC bound or cut short, of which
    /// `data` is the first bytes: one of number 1337 cancels the iTerm2
    /// handler, which ends a transfer in parts it is gathering.
    pub(super) fn image_osc_dropped(&mut self, data: &[u8]) {
        if matches!(data.strip_prefix(b"1337"), Some([] | [b';', ..])) {
            self.with_handler(ImageString::Iterm2, |h, ctx| h.cancel(ctx));
        }
    }

    /// The input call stops while a request is out, and once its image
    /// work reaches [`INPUT_WORK_MAX`] (D5).
    pub(super) fn images_paused(&self) -> bool {
        self.images.awaiting.is_some() || self.screen.images().work() >= INPUT_WORK_MAX
    }
}

impl Terminal {
    /// Installs the handler the router sends `which` to, or removes it.
    /// The outgoing handler is cancelled and withdrawn first: a string it
    /// was being fed ends, an upload in chunks it holds open is dropped,
    /// and it gives back everything it reserved.
    pub fn set_image_handler(&mut self, which: ImageString, handler: Option<Box<dyn Handler>>) {
        self.state.with_handler(which, |h, ctx| {
            h.cancel(ctx);
            h.withdraw(ctx);
        });
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
    /// ignored as every string vtium does not take is. A protocol turned
    /// off has its handler cancelled at once, so a string it was being fed
    /// ends. Sixel's handler is reset too, dropping the registers the
    /// pictures share. A kitty upload in chunks left open gives back what
    /// it holds at the next APC string, which withdraws the kitty handler
    /// while kitty graphics is not offered (ADR-1100, D6).
    pub fn set_image_switches(&mut self, switches: Switches) {
        let old = self.state.images.switches;
        self.state.images.switches = switches;
        let turned = [
            (ImageString::Sixel, old.sixel, switches.sixel),
            (ImageString::Kitty, old.kitty, switches.kitty),
            (ImageString::Iterm2, old.iterm2, switches.iterm2),
        ];
        for (which, was, now) in turned {
            if was && !now && which == ImageString::Sixel {
                self.state.drop_sixel();
            } else if was && !now {
                if self.state.images.active == Some(which) {
                    self.state.images.active = None;
                }
                self.state.with_handler(which, |h, ctx| h.cancel(ctx));
            }
        }
    }

    pub fn image_switches(&self) -> Switches {
        self.state.images.switches
    }

    /// Sets the limits D11's settings give the handlers.
    ///
    /// A change of `max-graphic-size` cancels a sixel string being fed and
    /// resets the sixel handler. A change of `num-color-registers` or
    /// `private-color-registers` does that and resets what the program set
    /// from them: the register count returns to the new limit, and modes
    /// 80, 1070 and 8452 return to their state at reset, 1070's from the
    /// new `private-color-registers`. A call that sets the values already
    /// in force changes nothing, so a host that rereads its configuration
    /// keeps a program's registers and modes.
    pub fn set_image_limits(&mut self, limits: Limits) {
        let old = self.state.images.limits;
        self.state.images.limits = limits;
        let registers =
            (old.registers, old.private_registers) != (limits.registers, limits.private_registers);
        if registers || old.graphic != limits.graphic {
            self.state.drop_sixel();
        }
        if !registers {
            return;
        }
        self.state.images.registers = None;
        // The three rows only RIS resets that the host's register settings
        // govern; 2028, the table's other such row, is not the host's.
        modes::set(&mut self.state, true, 80, false);
        modes::set(&mut self.state, true, 1070, limits.private_registers);
        modes::set(&mut self.state, true, 8452, false);
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

    /// The notes the handlers wrote since the last call, in order, for the
    /// host's log: each the first time its protocol and reason came, and
    /// each time their count reached a power of ten, so a program that sends
    /// the same bad picture every frame costs the log a line for each tenfold.
    pub fn take_image_notes(&mut self) -> Vec<ImageNote> {
        self.state.images.notes.take()
    }

    /// Drops the request out with no reply, as when the program that made
    /// it has gone: the handler that asked is cancelled, gives back what it
    /// held and writes nothing, and input goes on. A handed-back `FAILED`
    /// would instead say the helper failed, in a reply to the program and
    /// a note for the log.
    pub fn abandon_image_request(&mut self) {
        let Some((which, _, _)) = self.state.images.awaiting.take() else {
            return;
        };
        self.state.with_handler(which, |h, ctx| h.cancel(ctx));
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
mod tests;
