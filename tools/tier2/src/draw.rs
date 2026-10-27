//! Drawing a screen: the bytes through `vtium-term`, the frames through
//! `vtium-render` into memory, as a double-buffered display takes them.
//!
//! The flow is the C reference's tier 2 renderer's (`tests/test_pixels.c`,
//! `render_screen`): the cursor hidden, the bytes fed, a frame drawn; for a
//! screen with a second part, a frame repeating it into the other buffer,
//! the second part fed, and two more frames, after which both buffers must
//! hold the same picture, and the picture a fresh renderer draws. The image
//! is the buffer drawn last.
//!
//! Each screen is drawn once with the default configuration and again with
//! each way ADR-1300 says changes speed and never pixels: a cache of no
//! glyphs, the performance profile's budgets, and glyphs stored at 8 bits
//! and at their drawn size. Every one must draw the same bytes.

use vtium_font::{Cell, Face, Font, Glyph, Style as FontStyle, Unifont};
use vtium_render::{
    Budgets, Canvas, Config, DecoColor, DecorationColors, DrawState, Orientation, Profile,
    Renderer, Storage,
};
use vtium_term::{Age, Glyph as Text, Palette, Rgb, Terminal, WidthMethod};

use crate::ppm::Image;

/// The font a screen is drawn with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontKind {
    /// The built-in unifont at 16 pixels, 8x16 cells.
    Unifont,
    /// Sprites only, in a cell of the size the C reference's outline font
    /// gave: the screen holds nothing but sprites and spaces, which draw
    /// the same in any font of that cell, so the C reference's golden
    /// judges it while vtium has no outline font (vtium/vtium#23).
    Sprites { width: u32, height: u32 },
}

/// A face with a cell and no glyphs but the space, under the sprites.
#[derive(Clone, Copy, Debug)]
pub struct SpritesOnly(Cell);

impl Face for SpritesOnly {
    fn cell(&self) -> Cell {
        self.0
    }

    fn size_step(&self) -> u32 {
        self.0.height
    }

    fn has_glyph(&self, c: char) -> bool {
        c == ' '
    }

    fn draw(&self, cluster: &[char], _style: FontStyle) -> Option<Glyph> {
        (cluster == [' ']).then(|| Glyph::mask(self.0.width, self.0.height))?
    }
}

/// A configured decoration colour: `underline-color` or `strike-color`
/// as an index into the palette or as a colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecoSpec {
    Index(u8),
    Rgb(u8, u8, u8),
}

/// The decoration colours a screen configures; an absent one follows the
/// cell's foreground.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Decorations {
    pub underline: Option<DecoSpec>,
    pub strike: Option<DecoSpec>,
    /// `force-decoration-colors`: the configured underline colour wins over
    /// SGR 58's.
    pub force: bool,
}

impl Decorations {
    /// The colours resolved through `palette`, as the renderer takes them.
    fn resolve(&self, palette: &Palette) -> DecorationColors {
        let colour = |spec: Option<DecoSpec>| match spec {
            None => DecoColor::Foreground,
            Some(DecoSpec::Index(i)) => DecoColor::Rgb(palette.color(i)),
            Some(DecoSpec::Rgb(r, g, b)) => DecoColor::Rgb(Rgb::new(r, g, b)),
        };
        DecorationColors {
            underline: colour(self.underline),
            strike: colour(self.strike),
            force: self.force,
        }
    }
}

/// One screen of tier 2.
#[derive(Clone, Copy, Debug)]
pub struct Screen {
    pub name: &'static str,
    /// What the screen shows and what correct looks like: the report's
    /// caption.
    pub desc: &'static str,
    pub bytes: &'static [u8],
    /// The second part, fed after two frames; two more frames follow.
    pub next: Option<&'static [u8]>,
    pub cols: usize,
    pub rows: usize,
    /// The display's size in pixels before it is turned; `None` for the
    /// grid's own size.
    pub display: Option<(u32, u32)>,
    pub orientation: Orientation,
    pub font: FontKind,
    pub width_method: WidthMethod,
    pub decorations: Decorations,
}

impl Screen {
    /// A screen of `cols` by `rows` in unifont, unturned, sized to its
    /// grid, with no second part.
    pub const fn new(
        name: &'static str,
        desc: &'static str,
        bytes: &'static [u8],
        cols: usize,
        rows: usize,
    ) -> Screen {
        Screen {
            name,
            desc,
            bytes,
            next: None,
            cols,
            rows,
            display: None,
            orientation: Orientation::Normal,
            font: FontKind::Unifont,
            width_method: WidthMethod::Unicode,
            decorations: Decorations {
                underline: None,
                strike: None,
                force: false,
            },
        }
    }
}

/// The ways a screen is drawn, each of which must give the same bytes.
fn configs(orientation: Orientation) -> [(&'static str, Config); 4] {
    let base = Config {
        orientation,
        ..Config::default()
    };
    [
        ("default", base),
        (
            "a cache of no glyphs",
            Config {
                budgets: Some(Budgets::glyphs(0)),
                ..base
            },
        ),
        (
            "the performance profile",
            Config {
                profile: Profile::Performance,
                ..base
            },
        ),
        (
            "8-bit glyphs at their drawn size",
            Config {
                storage: Storage {
                    bits: false,
                    unscaled: false,
                },
                ..base
            },
        ),
    ]
}

/// Draws `screen`, and checks it draws the same in every configuration.
pub fn draw(screen: &Screen) -> Result<Image, String> {
    match screen.font {
        FontKind::Unifont => draw_with(screen, || Font::new(Unifont::new(16))),
        FontKind::Sprites { width, height } => {
            draw_with(screen, || Font::new(SpritesOnly(Cell { width, height })))
        }
    }
}

fn draw_with<F: Face>(screen: &Screen, font: impl Fn() -> Font<F>) -> Result<Image, String> {
    let mut first: Option<(&str, Image)> = None;
    for (name, config) in configs(screen.orientation) {
        let img = draw_config(screen, &font, config)?;
        match &first {
            None => first = Some((name, img)),
            Some((_, want)) if *want == img => {}
            Some((base, _)) => {
                return Err(format!(
                    "drawn with {name}, it differs from the {base} drawing"
                ));
            }
        }
    }
    first
        .map(|(_, img)| img)
        .ok_or_else(|| "no configuration".to_owned())
}

/// A display of two buffers, each knowing the frame it holds.
struct Display {
    buffers: [Canvas; 2],
    /// The swap after which each buffer was shown, if it has been.
    shown: [Option<u64>; 2],
    back: usize,
    swaps: u64,
}

impl Display {
    fn new(width: u32, height: u32) -> Display {
        // new memory holds anything: what is drawn must not depend on it
        Display {
            buffers: [
                Canvas::new(width, height, 0x00a5_5a3c),
                Canvas::new(width, height, 0x003c_5aa5),
            ],
            shown: [None; 2],
            back: 0,
            swaps: 0,
        }
    }

    /// The back buffer's age: 0 when it was never shown, else how many
    /// frames back the frame it holds is.
    fn age(&self) -> u32 {
        self.shown[self.back].map_or(0, |n| u32::try_from(self.swaps - n + 1).unwrap_or(u32::MAX))
    }

    /// Draws the terminal's frame into the back buffer and swaps when the
    /// frame drew anything, as a display does.
    fn frame<F: Face>(&mut self, r: &mut Renderer<F>, term: &mut Terminal, since: &mut Age) {
        let state = DrawState {
            foreground: term.palette().fg(),
            background: term.palette().bg(),
            ..DrawState::default()
        };
        let age = self.age();
        let drew = {
            let frame = term.screen().frame(term.palette(), *since);
            !r.draw(&frame, &mut self.buffers[self.back], age, &state)
                .damage
                .is_empty()
        };
        *since = term.screen_mut().checkpoint();
        if drew {
            self.swaps += 1;
            self.shown[self.back] = Some(self.swaps);
            self.back ^= 1;
        }
    }

    /// The buffer shown last.
    fn front(&self) -> &Canvas {
        &self.buffers[self.back ^ 1]
    }
}

fn feed(term: &mut Terminal, mut bytes: &[u8]) -> Result<(), String> {
    while !bytes.is_empty() {
        let taken = term.input(bytes);
        let replies = term.take_replies();
        if taken == 0 && replies.is_empty() {
            return Err("the terminal stopped taking input".to_owned());
        }
        bytes = &bytes[taken..];
    }
    Ok(())
}

/// Refuses a sprites-only screen holding a character that is not a sprite
/// or a space, a combining mark on either included: the C reference drew
/// it from an outline font, which vtium does not have.
fn check_sprites<F: Face>(font: &Font<F>, term: &Terminal) -> Result<(), String> {
    let frame = term.screen().frame(term.palette(), Age::NEVER);
    for row in 0..frame.rows() {
        for col in 0..frame.cols() {
            let mut buf = [0u8; 4];
            let text = match frame.cell(row, col).map(|c| c.glyph) {
                Some(Text::Char(c)) => &*c.encode_utf8(&mut buf),
                Some(Text::Cluster(s)) => s,
                _ => continue,
            };
            if let Some(c) = text.chars().find(|&c| !font.has_glyph(c)) {
                return Err(format!(
                    "cell {col},{row} holds U+{:04X}, which only an outline font draws",
                    u32::from(c)
                ));
            }
        }
    }
    Ok(())
}

fn draw_config<F: Face>(
    screen: &Screen,
    make_font: &impl Fn() -> Font<F>,
    config: Config,
) -> Result<Image, String> {
    let font = make_font();
    let mut term = Terminal::new(screen.cols, screen.rows, 0);
    let config = Config {
        decorations: screen.decorations.resolve(term.palette()),
        ..config
    };
    // as a host sets it at start: the screen's method, and what RIS
    // returns to
    term.set_default_width_method(screen.width_method);
    term.screen_mut().set_width_method(screen.width_method);
    // the cursor's blink state is not part of what is compared
    feed(&mut term, b"\x1b[?25l")?;
    feed(&mut term, screen.bytes)?;
    if matches!(screen.font, FontKind::Sprites { .. }) {
        check_sprites(&font, &term)?;
    }
    let cell = font.cell();
    let (w, h) = screen.display.unwrap_or((
        cell.width * screen.cols as u32,
        cell.height * screen.rows as u32,
    ));
    let (w, h) = match screen.orientation {
        Orientation::Right | Orientation::Left => (h, w),
        _ => (w, h),
    };
    let mut display = Display::new(w, h);
    let mut r = Renderer::new(font, w, h, config);
    let mut since = Age::NEVER;
    display.frame(&mut r, &mut term, &mut since);
    if let Some(next) = screen.next {
        display.frame(&mut r, &mut term, &mut since);
        feed(&mut term, next)?;
        display.frame(&mut r, &mut term, &mut since);
        display.frame(&mut r, &mut term, &mut since);
        if display.buffers[0].pixels() != display.buffers[1].pixels() {
            return Err("the two buffers differ after the last frame".to_owned());
        }
        let mut fresh = Display::new(w, h);
        let mut r2 = Renderer::new(make_font(), w, h, config);
        let mut never = Age::NEVER;
        fresh.frame(&mut r2, &mut term, &mut never);
        if fresh.front().pixels() != display.front().pixels() {
            return Err("the frames drawn in turn differ from a fresh draw".to_owned());
        }
    }
    let front = display.front();
    Image::from_xrgb(front.width(), front.height(), front.pixels())
        .ok_or_else(|| format!("a display of {w}x{h} pixels"))
}
