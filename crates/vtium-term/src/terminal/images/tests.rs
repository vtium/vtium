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
    let margins = |top, bottom, left, right| {
        Some(Margins {
            top,
            bottom,
            left,
            right,
        })
    };
    let mut t = probed(1 << 20);
    let _ = t.input(b"\x1bPq\x1b\\");
    assert_eq!(probe(&t).margins, margins(0, 3, 0, 7));
    let _ = t.input(b"\x1b[2;3r\x1bPq\x1b\\");
    assert_eq!(probe(&t).margins, margins(1, 2, 0, 7));
    // DECSLRM's margins, while DECLRMM is set
    let _ = t.input(b"\x1b[?69h\x1b[3;6s\x1bPq\x1b\\");
    assert_eq!(probe(&t).margins, margins(1, 2, 2, 5));
    let _ = t.input(b"\x1b[?69l\x1b[r\x1bPq\x1b\\");
    assert_eq!(probe(&t).margins, margins(0, 3, 0, 7));
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
            let _ = t.input(format!("\x1b[?80{verb}\x1b[?1070{verb}\x1b[?8452{verb}").as_bytes());
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

/// A terminal of `cols` by 2 with kitty graphics on.
fn kitty_term(cols: usize) -> Terminal {
    let mut t = Terminal::new(cols, 2, 0);
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
    t
}

/// Each placement's pixels, in draw order.
fn placed_pixels(t: &Terminal) -> Vec<Vec<u32>> {
    t.screen()
        .placements()
        .iter()
        .map(|p| p.pixels.as_slice().to_vec())
        .collect()
}

// The kitty graphics specification says a palette-index picture "is
// drawn using the current colors of the terminal and is automatically
// redrawn when they change". So a kitty f=8 picture sent before OSC 4
// changes color 1 shows the new color, as one sent after it does.
//
// References:
// - ADR-1100 D15: a palette-index picture follows the colors.
// covers: image.kitty
#[test]
fn a_palette_index_picture_takes_the_colors_of_now_whenever_it_arrived() {
    let mut t = kitty_term(4);
    let old = t.palette().color(1);
    // index 1, then the default background (248) and foreground (249)
    let picture = |id: u8| format!("\x1b_Ga=T,f=8,s=3,v=1,i={id},C=1;Afj5\x1b\\");
    t.input(picture(1).as_bytes());
    let (fg, bg) = (t.palette().fg(), t.palette().bg());
    assert_eq!(
        placed_pixels(&t),
        vec![vec![opaque(old), opaque(bg), opaque(fg)]]
    );
    t.input(b"\x1b]4;1;rgb:12/34/56\x1b\\");
    let new = t.palette().color(1);
    assert_ne!(old, new);
    t.input(picture(2).as_bytes());
    assert_eq!(t.replies(), b"\x1b_Gi=1;OK\x1b\\\x1b_Gi=2;OK\x1b\\");
    let now = vec![opaque(new), opaque(bg), opaque(fg)];
    assert_eq!(placed_pixels(&t), vec![now.clone(), now]);
}

// Each way a program changes the colors, OSC 4, 10, 11, 12, 17 and 19,
// and each reset, OSC 104, 110, 111, 112, 117 and 119, converts a
// palette-index picture's pixels again and marks its cells changed; so
// does the host's palette. The picture shows index 1 and indices 248
// to 253, so each of these strings changes one of its pixels.
//
// References:
// - ADR-1100 D15: when the colors change.
// covers: image.kitty
#[test]
fn every_color_change_and_reset_redraws_a_palette_index_picture() {
    let mut t = kitty_term(8);
    // 1, 248, 249, 250, 251, 252, 253 as base64
    t.input(b"\x1b_Ga=T,f=8,s=7,v=1,i=1,C=1,q=1;Afj5+vv8/Q==\x1b\\");
    let changes: [&[u8]; 12] = [
        b"\x1b]4;1;rgb:12/34/56\x1b\\",
        b"\x1b]104;1\x1b\\",
        b"\x1b]10;rgb:01/02/03\x1b\\",
        b"\x1b]110\x1b\\",
        b"\x1b]11;rgb:04/05/06\x1b\\",
        b"\x1b]111\x1b\\",
        b"\x1b]12;rgb:07/08/09\x1b\\",
        b"\x1b]112\x1b\\",
        b"\x1b]17;rgb:0a/0b/0c\x1b\\",
        b"\x1b]117\x1b\\",
        b"\x1b]19;rgb:0d/0e/0f\x1b\\",
        b"\x1b]119\x1b\\",
    ];
    for change in changes {
        let before = placed_pixels(&t);
        let age = t.screen_mut().checkpoint();
        t.input(change);
        let after = placed_pixels(&t);
        let what = String::from_utf8_lossy(change);
        assert_ne!(after, before, "{what:?} redraws the picture");
        // the picture's cell is marked changed for the next frame
        assert!(t.screen().damage().changed(0, 0, age), "{what:?}");
        // and the pixels are those of a picture sent now
        t.input(b"\x1b_Ga=T,f=8,s=7,v=1,i=2,C=1,q=1;Afj5+vv8/Q==\x1b\\");
        let fresh = placed_pixels(&t);
        assert_eq!(fresh.first(), fresh.last(), "{what:?}");
        t.input(b"\x1b_Ga=d,d=I,i=2\x1b\\");
    }
    // the host's palette, a change of the configured palette
    let before = placed_pixels(&t);
    let mut p = t.palette().clone();
    p.set_color(1, crate::style::Rgb::new(1, 1, 1));
    t.set_palette(p);
    assert_ne!(placed_pixels(&t), before, "the host's palette");
}

/// The pixels of the picture placed last, after feeding `t` an f=8
/// picture of indices 248 to 253, each one pixel wide.
fn special_indices(t: &mut Terminal, id: u32) -> Vec<u32> {
    // 248, 249, 250, 251, 252, 253 as base64
    t.input(format!("\x1b_Ga=T,f=8,s=6,v=1,i={id},C=1;+Pn6+/z9\x1b\\").as_bytes());
    let placed = t.screen().placements();
    placed
        .last()
        .map(|p| p.pixels.as_slice().to_vec())
        .unwrap_or_default()
}

// Indices 248 to 253 take the default background and foreground, the
// selection's background and foreground and the cursor's color that
// OSC 10, 11, 12, 17 and 19 set, and with none set, the default
// foreground for 250 and 252 and the default background for 251 and
// 253, as kitty's specification asks (ADR-1100, D15).
// covers: image.kitty
#[test]
fn osc_colors_reach_a_palette_index_picture() {
    let mut t = kitty_term(8);
    let (fg, bg) = (t.palette().fg(), t.palette().bg());
    let unset = special_indices(&mut t, 1);
    assert_eq!(
        unset,
        [bg, fg, fg, bg, fg, bg].map(opaque).to_vec(),
        "with no OSC color set"
    );
    for (n, rgb) in [
        (10, "01/02/03"),
        (11, "04/05/06"),
        (12, "07/08/09"),
        (17, "0a/0b/0c"),
        (19, "0d/0e/0f"),
    ] {
        t.input(format!("\x1b]{n};rgb:{rgb}\x1b\\").as_bytes());
    }
    let c = |r: u8, g: u8, b: u8| opaque(crate::style::Rgb::new(r, g, b));
    assert_eq!(
        special_indices(&mut t, 2),
        vec![
            c(4, 5, 6),
            c(1, 2, 3),
            c(10, 11, 12),
            c(13, 14, 15),
            c(7, 8, 9),
            c(4, 5, 6)
        ],
        "with OSC 10, 11, 12, 17 and 19 set"
    );
}
