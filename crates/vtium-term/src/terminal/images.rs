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
    /// A handler holds an upload in chunks open between its strings, which
    /// output of any other kind ends.
    upload_open: bool,
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
            upload_open: false,
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
            .field("upload_open", &self.upload_open)
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
            && self.upload_open == other.upload_open
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

    /// Reads again whether a handler holds an upload in chunks open.
    fn note_uploads(&mut self) {
        self.upload_open = [&self.sixel, &self.kitty, &self.iterm2]
            .into_iter()
            .any(|h| h.as_ref().is_some_and(|h| h.upload_open()));
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

    fn colors(&self) -> Option<vtium_image::Colors> {
        use crate::palette::Special;
        let p = &self.state.colors;
        let rgb = |c: crate::style::Rgb| [c.r, c.g, c.b];
        Some(vtium_image::Colors {
            table: core::array::from_fn(|i| rgb(p.color(u8::try_from(i).unwrap_or(u8::MAX)))),
            fg: rgb(p.fg()),
            bg: rgb(p.bg()),
            selection_bg: p.special(Special::SelectionBg).map(rgb),
            selection_fg: p.special(Special::SelectionFg).map(rgb),
            cursor: p.cursor().map(rgb),
        })
    }

    fn note(&mut self, cause: Cause, why: &str) {
        self.state.images.notes.add(self.which, cause, why);
    }
}

impl State {
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
        self.images.note_uploads();
    }

    /// Output other than an image string arrived: text, a control, or
    /// another escape sequence. While a handler holds an upload in chunks
    /// open, it ends the upload. The spec asks a program to send nothing
    /// else between the chunks of one upload, and vtium holds it to that,
    /// so a program that dies mid-upload leaves no picture open to swallow
    /// the next program's first picture command (kitty keeps the upload
    /// open).
    #[inline]
    pub(super) fn image_other_output(&mut self) {
        if self.images.upload_open {
            self.images_interrupt();
        }
    }

    /// Each handler that holds an upload in chunks open drops it.
    pub(super) fn images_interrupt(&mut self) {
        for which in [ImageString::Sixel, ImageString::Kitty, ImageString::Iterm2] {
            self.with_handler(which, |h, ctx| {
                if h.upload_open() {
                    h.interrupt(ctx);
                }
            });
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
    /// handler is cancelled, so an upload in chunks that kitty was switched
    /// off during gives back what it holds.
    pub(super) fn image_apc_start(&mut self) {
        self.images.active = None;
        self.images.apc_open = true;
        if !self.image_offered(ImageString::Kitty) {
            self.with_handler(ImageString::Kitty, |h, ctx| h.cancel(ctx));
        }
    }

    /// An APC string's data: its first byte chooses the kitty handler, and
    /// what follows the `G` is fed to it.
    pub(super) fn image_apc_put(&mut self, data: Chunk<'_>) {
        if core::mem::take(&mut self.images.apc_open) {
            let kitty = matches!(data, Chunk::Bytes([b'G', ..]));
            if !kitty {
                self.image_other_output();
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
            self.image_other_output();
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
        if core::mem::take(&mut self.images.apc_open) {
            self.image_other_output();
        }
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
    /// The outgoing handler is cancelled first, so it gives back what it
    /// reserved, and a string it was being fed ends.
    pub fn set_image_handler(&mut self, which: ImageString, handler: Option<Box<dyn Handler>>) {
        self.state.with_handler(which, |h, ctx| h.cancel(ctx));
        if self.state.images.active == Some(which) {
            self.state.images.active = None;
        }
        *self.state.images.slot(which) = handler;
        self.state.images.note_uploads();
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
    /// ends and an upload in chunks gives back what it holds; sixel's is
    /// reset too, dropping the registers the pictures share.
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
    /// reports, and at its finish stores and places a sixel of `size`
    /// pixels at the cursor through one store edit, reserving 100 bytes of
    /// its cost first and growing the reservation by 400, and noting the
    /// bytes the store holds then.
    #[derive(Clone, Debug, PartialEq)]
    struct Probe {
        size: (u32, u32),
        margins: Option<Margins>,
        sixel: Option<SixelState>,
        reserved: Option<u64>,
        placed: Option<Result<(u32, u32), Refusal>>,
    }

    impl Default for Probe {
        fn default() -> Self {
            Self {
                size: (8, 16),
                margins: None,
                sixel: None,
                reserved: None,
                placed: None,
            }
        }
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
            let (width, height) = self.size;
            let pixels = usize::try_from(width.saturating_mul(height)).unwrap_or(0);
            let mut reserved = None;
            self.placed = ctx.edit(|e| {
                let mut r = e.reserve(Protocol::SIXEL, 0, 100)?;
                e.grow(&mut r, 400)?;
                reserved = Some(e.store().bytes());
                let image = NewImage {
                    protocol: Protocol::SIXEL,
                    width,
                    height,
                    pixels: vec![0xffff_ffff; pixels],
                    id: 0,
                    number: 0,
                    transient: false,
                    opaque: None,
                };
                let key = e.add_image(image, Some(r))?;
                let p = e.place(key, at)?;
                e.extent(p).ok_or(Refusal::NotFound)
            });
            self.reserved = reserved;
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
        probed_with(limit, Probe::default())
    }

    fn probed_with(limit: u64, probe: Probe) -> Terminal {
        let mut t = Terminal::new(8, 4, 16);
        t.screen_mut().set_image_limit(limit);
        t.set_image_switches(Switches {
            sixel: true,
            kitty: false,
            iterm2: false,
        });
        t.set_image_handler(ImageString::Sixel, Some(Box::new(probe)));
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

    /// A handler that keeps a reservation from a string's finish, as a
    /// kitty upload in chunks keeps one between its strings, and gives it
    /// back when it is cancelled, as an aborted upload does, or reset.
    #[derive(Clone, Debug, Default, PartialEq)]
    struct Holding(Option<vtium_image::Reservation>);

    impl Handler for Holding {
        fn start(&mut self, _: &mut dyn Context, _: Intro<'_>) {}
        fn feed(&mut self, _: &mut dyn Context, _: Chunk<'_>) {}
        fn finish(&mut self, ctx: &mut dyn Context) {
            if let Some(r) = self.0.take() {
                ctx.unreserve(r);
            }
            self.0 = ctx.reserve(Protocol::KITTY, 0, 4096).ok();
        }
        fn cancel(&mut self, ctx: &mut dyn Context) {
            if let Some(r) = self.0.take() {
                ctx.unreserve(r);
            }
        }
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
        // the second string's reservation takes the place of the first's,
        // which the handler gives back
        let _ = t.input(b"\x1b_Gx\x1b\\\x1b_Gx\x1b\\");
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
    fn sixel_is_offered_while_its_handler_runs() {
        let mut t = probed(1 << 20);
        let mut inside = None;
        t.state.with_handler(ImageString::Sixel, |_, ctx| {
            inside = Some(ctx.state.image_offered(ImageString::Sixel));
        });
        assert_eq!(inside, Some(true), "the mode rows read sixel as offered");
        assert!(t.state.image_offered(ImageString::Sixel));
        assert_eq!(t.state.images.running, None);
    }

    #[test]
    fn ris_returns_the_register_count_to_num_color_registers() {
        let mut t = probed(1 << 20);
        t.state.set_sixel_registers(Some(16));
        let _ = t.input(b"\x1bc\x1bPq\x1b\\");
        assert_eq!(probe(&t).sixel.map(|s| s.registers), Some(1024));
    }

    // A change of max-graphic-size, of the register settings, or of the
    // count XTSMGRAPHICS sets resets the sixel handler and the shared
    // registers; only the register settings reset the count and the modes
    // too, and setting the count in force again changes nothing.
    #[test]
    fn a_change_of_the_sixel_settings_resets_the_handler() {
        let mut t = Terminal::new(8, 4, 16);
        t.screen_mut().set_image_limit(1 << 20);
        t.set_image_switches(Switches {
            sixel: true,
            kitty: false,
            iterm2: false,
        });
        t.set_image_handler(ImageString::Sixel, Some(Box::new(Holding::default())));
        let hold = |t: &mut Terminal| {
            let _ = t.input(b"\x1bPq\x1b\\");
            assert_eq!(t.screen().images().bytes(), 4096);
        };
        hold(&mut t);
        t.state.set_sixel_registers(Some(16));
        assert_eq!(t.screen().images().bytes(), 0, "a new count resets");
        hold(&mut t);
        t.state.set_sixel_registers(Some(16));
        assert_eq!(t.screen().images().bytes(), 4096, "the same count does not");
        // the largest picture resets the handler and keeps the count
        t.set_image_limits(Limits {
            graphic: (500, 500),
            ..Limits::default()
        });
        assert_eq!(t.state.images.registers, Some(16));
        assert_eq!(t.screen().images().bytes(), 0);
        // the register count resets the handler, and the count follows
        hold(&mut t);
        t.set_image_limits(Limits {
            graphic: (500, 500),
            registers: 256,
            ..Limits::default()
        });
        assert_eq!(t.state.images.registers, None);
        assert_eq!(t.state.image_limits().registers, 256);
        assert_eq!(t.state.sixel_state().registers, 256);
        assert_eq!(t.screen().images().bytes(), 0, "the sixel handler reset");
    }

    #[test]
    fn kitty_switched_off_mid_upload_gives_its_reservation_back_at_the_next_apc() {
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
        t.set_image_switches(Switches {
            sixel: false,
            kitty: false,
            iterm2: false,
        });
        let _ = t.input(b"\x1b_Gm=0;AAAA\x1b\\");
        assert_eq!(t.screen().images().bytes(), 0);
    }

    #[test]
    fn an_edit_reports_a_placements_box_and_the_bytes_reserved() {
        // 16 by 48 pixels in 8 by 16 cells: 2 columns, 3 rows
        let mut t = probed_with(
            1 << 20,
            Probe {
                size: (16, 48),
                ..Probe::default()
            },
        );
        let _ = t.input(b"\x1bPq\x1b\\");
        let p = probe(&t);
        assert_eq!(p.placed, Some(Ok((2, 3))));
        assert_eq!(p.reserved, Some(500), "100 reserved, then 400 more");
    }

    /// A handler that, at each string's finish, scrolls the region up
    /// `scroll` rows and then indexes `index` rows through the context.
    #[derive(Clone, Debug, Default, PartialEq)]
    struct Mover {
        scroll: usize,
        index: usize,
    }

    impl Handler for Mover {
        fn start(&mut self, _: &mut dyn Context, _: Intro<'_>) {}
        fn feed(&mut self, _: &mut dyn Context, _: Chunk<'_>) {}
        fn finish(&mut self, ctx: &mut dyn Context) {
            ctx.scroll_up(self.scroll);
            ctx.index(self.index);
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

    /// The screen's rows, cell by cell, and the cursor.
    fn shown(t: &Terminal) -> (Vec<Vec<crate::Cell>>, (usize, usize)) {
        let s = t.screen();
        let rows = (0..s.rows())
            .map(|r| s.line(r).map(|l| l.cells().to_vec()).unwrap_or_default())
            .collect();
        (rows, s.cursor())
    }

    /// An 8 by 6 terminal with a letter on each row, the region rows 2 to
    /// 4 (DECSTBM 3;5), the cursor at row `row`, and the mover as its
    /// sixel handler.
    fn moving(row: usize, mover: Mover) -> Terminal {
        let mut t = Terminal::new(8, 6, 16);
        t.screen_mut().set_image_limit(1 << 20);
        t.set_image_switches(Switches {
            sixel: true,
            kitty: false,
            iterm2: false,
        });
        t.set_image_handler(ImageString::Sixel, Some(Box::new(mover)));
        let _ = t.input(b"a\r\nb\r\nc\r\nd\r\ne\r\nf\x1b[3;5r");
        let _ = t.input(format!("\x1b[{};3H", row.saturating_add(1)).as_bytes());
        t
    }

    #[test]
    fn the_context_indexes_as_that_many_line_feeds_do() {
        // at the bottom margin, above the region and below it
        for row in [4, 0, 5] {
            for n in [1, 2, 5] {
                let mut by_handler = moving(
                    row,
                    Mover {
                        scroll: 0,
                        index: n,
                    },
                );
                let _ = by_handler.input(b"\x1bPq\x1b\\");
                let mut by_ind = moving(row, Mover::default());
                let _ = by_ind.input("\x1bD".repeat(n).as_bytes());
                assert!(
                    shown(&by_handler) == shown(&by_ind),
                    "index {n} from row {row}"
                );
            }
        }
    }

    #[test]
    fn the_context_scrolls_the_region_and_leaves_the_cursor() {
        let mut by_handler = moving(
            1,
            Mover {
                scroll: 2,
                index: 0,
            },
        );
        let _ = by_handler.input(b"\x1bPq\x1b\\");
        let mut by_su = moving(1, Mover::default());
        let _ = by_su.input(b"\x1b[2S");
        assert!(shown(&by_handler) == shown(&by_su));
        assert_eq!(by_handler.screen().cursor(), (2, 1), "the cursor stays");
        let above = moving(1, Mover::default());
        assert!(shown(&by_handler) != shown(&above), "the region scrolled");
    }

    #[test]
    fn a_change_of_the_register_settings_cancels_a_sixel_being_fed() {
        let mut t = probed(1 << 20);
        let _ = t.input(b"\x1bPq");
        t.set_image_limits(Limits {
            registers: 256,
            ..Limits::default()
        });
        let _ = t.input(b"\x1b\\");
        assert_eq!(probe(&t).placed, None, "the string ended cancelled");
        assert_eq!(t.screen().images().image_count(), 0);
    }

    /// A terminal with kitty graphics on and a holding handler for it, its
    /// upload's 4096 bytes reserved.
    fn uploading() -> Terminal {
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
        t
    }

    #[test]
    fn an_apc_that_is_not_kitty_leaves_an_upload_alone() {
        let mut t = uploading();
        let _ = t.input(b"\x1b_x\x1b\\");
        assert_eq!(t.screen().images().bytes(), 4096);
    }

    #[test]
    fn switching_kitty_off_cancels_an_upload_at_once() {
        let mut t = uploading();
        t.set_image_switches(Switches {
            sixel: false,
            kitty: false,
            iterm2: false,
        });
        assert_eq!(t.screen().images().bytes(), 0);
        // switching another protocol off cancels nothing of kitty's
        let mut t = uploading();
        t.set_image_switches(Switches {
            sixel: true,
            kitty: true,
            iterm2: false,
        });
        t.set_image_switches(Switches {
            sixel: false,
            kitty: true,
            iterm2: false,
        });
        assert_eq!(t.screen().images().bytes(), 4096);
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

    /// An 8 by 4 terminal with the iTerm2 handler on, a helper, and a
    /// transfer in parts open with one part received.
    fn transferring() -> Terminal {
        let mut t = Terminal::new(8, 4, 16);
        t.screen_mut().set_image_limit(1 << 20);
        t.set_image_switches(Switches {
            sixel: true,
            kitty: false,
            iterm2: true,
        });
        t.set_image_helper(true);
        t.set_image_handler(
            ImageString::Iterm2,
            Some(Box::new(vtium_image::Iterm2::default())),
        );
        let _ = t.input(b"\x1b]1337;MultipartFile=inline=1\x07\x1b]1337;FilePart=QUJD\x07");
        assert!(t.screen().images().bytes() > 0, "the part is held");
        t
    }

    // covers: image.iterm2
    #[test]
    fn the_parts_of_a_transfer_reach_the_iterm2_handler() {
        let mut t = transferring();
        // text and other strings between the parts leave the transfer
        let _ = t.input(b"text\x1b]1337;SetUserVar=a=Yg==\x07\x1b]1337;FilePart=REVG\x1b\\");
        let _ = t.input(b"\x1b]1337;FileEnd\x07");
        let request = t.take_image_request().map(|r| (r.medium, r.any_image));
        assert_eq!(
            request,
            Some((
                vtium_image::decode::Medium::Direct(b"ABCDEF".to_vec()),
                true
            ))
        );
    }

    #[test]
    fn an_osc_1337_string_dropped_while_a_transfer_is_open_abandons_it() {
        // cut by CAN, by SUB, and by another string's start
        for cut in [&b"\x18"[..], b"\x1a", b"\x1b[m"] {
            let mut t = transferring();
            let mut bytes = b"\x1b]1337;FilePart=QUJD".to_vec();
            bytes.extend_from_slice(cut);
            let _ = t.input(&bytes);
            assert_eq!(t.screen().images().bytes(), 0, "{cut:?}");
            let _ = t.input(b"\x1b]1337;FileEnd\x07");
            assert_eq!(t.take_image_request(), None, "{cut:?}");
        }
        // one past the OSC bound, which vtium-parse marks truncated
        let mut t = transferring();
        let mut long = b"\x1b]1337;FilePart=".to_vec();
        long.resize(vtium_parse::OSC_MAX + 64, b'A');
        long.extend_from_slice(b"\x07");
        let _ = t.input(&long);
        assert_eq!(t.screen().images().bytes(), 0);
        // a dropped string of another number leaves the transfer
        let mut t = transferring();
        let _ = t.input(b"\x1b]52;c;QUJD\x18");
        assert!(t.screen().images().bytes() > 0);
    }

    #[test]
    fn switching_iterm2_off_abandons_a_transfer() {
        let mut t = transferring();
        t.set_image_switches(Switches {
            sixel: true,
            kitty: false,
            iterm2: false,
        });
        assert_eq!(t.screen().images().bytes(), 0);
    }

    /// A handler that notes `why` at each string's finish.
    #[derive(Clone, Debug, PartialEq)]
    struct Refuse(&'static str);

    impl Handler for Refuse {
        fn start(&mut self, _: &mut dyn Context, _: Intro<'_>) {}
        fn feed(&mut self, _: &mut dyn Context, _: Chunk<'_>) {}
        fn finish(&mut self, ctx: &mut dyn Context) {
            ctx.note(Cause::Program, self.0);
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

    /// A terminal whose sixel handler refuses every picture with `why`.
    fn refusing(why: &'static str) -> Terminal {
        let mut t = Terminal::new(8, 4, 0);
        t.screen_mut().set_image_limit(1 << 20);
        t.set_image_switches(Switches {
            sixel: true,
            kitty: false,
            iterm2: false,
        });
        t.set_image_handler(ImageString::Sixel, Some(Box::new(Refuse(why))));
        t
    }

    const SIXEL: &[u8] = b"\x1bPq#0~\x1b\\";

    #[test]
    fn a_handlers_note_reaches_the_host_once_with_its_protocol() {
        let mut t = refusing("a refused picture");
        assert_eq!(t.take_image_notes(), []);
        let _ = t.input(SIXEL);
        let note = ImageNote {
            protocol: ImageString::Sixel,
            cause: Cause::Program,
            why: "a refused picture".to_owned(),
            count: 1,
        };
        assert_eq!(t.take_image_notes(), [note]);
        assert_eq!(t.take_image_notes(), [], "taken once");
    }

    #[test]
    fn a_reason_that_repeats_reaches_the_host_at_each_power_of_ten() {
        let mut t = refusing("a refused picture");
        let mut counts = Vec::new();
        for _ in 0..12_345 {
            let _ = t.input(SIXEL);
            counts.extend(t.take_image_notes().into_iter().map(|n| n.count));
        }
        assert_eq!(counts, [1, 10, 100, 1000, 10_000]);
        // RIS keeps the count, so a program cannot start it again
        let _ = t.input(b"\x1bc");
        for _ in 0..(100_000 - 12_345) {
            let _ = t.input(SIXEL);
        }
        let notes = t.take_image_notes();
        assert_eq!(notes.iter().map(|n| n.count).collect::<Vec<_>>(), [100_000]);
    }

    #[test]
    fn each_protocol_and_reason_is_counted_on_its_own() {
        let mut t = refusing("one reason");
        let _ = t.input(SIXEL);
        t.set_image_handler(ImageString::Sixel, Some(Box::new(Refuse("another"))));
        let _ = t.input(SIXEL);
        let _ = t.input(SIXEL);
        let whys: Vec<_> = t
            .take_image_notes()
            .into_iter()
            .map(|n| (n.why, n.count))
            .collect();
        assert_eq!(
            whys,
            [("one reason".to_owned(), 1), ("another".to_owned(), 1)]
        );
        let mut notes = Notes::default();
        notes.add(ImageString::Kitty, Cause::Program, "one reason");
        notes.add(ImageString::Iterm2, Cause::Program, "one reason");
        assert_eq!(notes.take().len(), 2, "the same reason from two protocols");
        notes.add(ImageString::Kitty, Cause::Console, "one reason");
        let causes: Vec<_> = notes
            .take()
            .into_iter()
            .map(|n| (n.cause, n.count))
            .collect();
        assert_eq!(
            causes,
            [(Cause::Console, 1)],
            "the same reason under another cause"
        );
    }

    #[test]
    fn the_notes_hold_a_bounded_count_of_reasons_and_bytes() {
        let mut notes = Notes::default();
        let reasons: Vec<String> = (0..IMAGE_NOTE_REASONS_MAX + 10)
            .map(|i| format!("reason {i}"))
            .collect();
        for _ in 0..3 {
            for why in &reasons {
                notes.add(ImageString::Sixel, Cause::Console, why);
            }
        }
        let taken = notes.take();
        assert_eq!(
            taken.len(),
            IMAGE_NOTE_REASONS_MAX,
            "one note for each reason it counts"
        );
        assert!(taken.iter().all(|n| n.count == 1));
        assert_eq!(notes.seen.len(), IMAGE_NOTE_REASONS_MAX);
        // a long reason is cut at a character's boundary
        let mut notes = Notes::default();
        let long = "é".repeat(IMAGE_NOTE_BYTES_MAX);
        notes.add(ImageString::Iterm2, Cause::Console, &long);
        let why = notes.take().pop().map(|n| n.why).unwrap_or_default();
        assert!(why.len() <= IMAGE_NOTE_BYTES_MAX && why.len() + 2 > IMAGE_NOTE_BYTES_MAX);
        assert!(long.starts_with(&why));
    }

    /// The store's opaque pixel of `rgb`.
    fn opaque(rgb: crate::style::Rgb) -> u32 {
        0xff00_0000 | u32::from(rgb.r) << 16 | u32::from(rgb.g) << 8 | u32::from(rgb.b)
    }

    // A kitty f=8 picture takes the palette the frames draw through as it
    // arrives (ADR-1100, D15): one sent before OSC 4 changes color 1 keeps
    // the old color, and one sent after it takes the new one.
    // covers: image.kitty
    #[test]
    fn a_palette_index_picture_takes_the_colors_in_force_when_it_arrives() {
        let mut t = Terminal::new(4, 2, 0);
        t.screen_mut().set_image_limit(1 << 20);
        assert!(t.screen_mut().set_image_cell(8, 16).is_ok());
        t.set_image_switches(Switches {
            sixel: false,
            kitty: true,
            iterm2: false,
        });
        t.set_image_handler(
            ImageString::Kitty,
            Some(Box::new(vtium_image::Kitty::default())),
        );
        let old = t.palette().color(1);
        // index 1, then the default background (248) and foreground (249)
        let picture = |id: u8| format!("\x1b_Ga=T,f=8,s=3,v=1,i={id},C=1;Afj5\x1b\\");
        t.input(picture(1).as_bytes());
        t.input(b"\x1b]4;1;rgb:12/34/56\x1b\\");
        let new = t.palette().color(1);
        assert_ne!(old, new);
        t.input(picture(2).as_bytes());
        assert_eq!(t.replies(), b"\x1b_Gi=1;OK\x1b\\\x1b_Gi=2;OK\x1b\\");
        let (fg, bg) = (t.palette().fg(), t.palette().bg());
        let placed = t.screen().placements();
        let pixels: Vec<Vec<u32>> = placed
            .iter()
            .map(|p| p.pixels.as_slice().to_vec())
            .collect();
        assert_eq!(
            pixels,
            vec![
                vec![opaque(old), opaque(bg), opaque(fg)],
                vec![opaque(new), opaque(bg), opaque(fg)],
            ]
        );
    }
}
