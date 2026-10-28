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

use std::sync::Arc;

use vtium_font::set::{Set, SetFace};
use vtium_font::{Cell, Face, Font, Glyph, Psf, Sha256, Style as FontStyle, Unifont};
use vtium_render::{
    Budgets, Canvas, Config, DecoColor, DecorationColors, Depth, DrawState, Orientation, Profile,
    Renderer, Storage,
};
use vtium_term::{Age, Glyph as Text, Rgb, Terminal, WidthMethod};

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
    /// A PSF font `tools/testfont` writes, read through vtium-font's PSF
    /// loader at its own size.
    Psf(TestFont),
    /// A PSF font `tools/testfont` writes, vitrified as `vtium-vitrify
    /// create` builds it (ADR-1200), with `fill`, a built-in set, filling
    /// the characters the font lacks, and drawn from the set at the font's
    /// own size, as the console draws a set from its store.
    Set {
        font: TestFont,
        fill: Option<&'static str>,
    },
    /// The vitrified set [`crate::colorset`] builds, unifont's ASCII in a
    /// 1-bit strike and color glyphs in an RGBA strike at 8x16, drawn for
    /// cells `height` pixels high (vtium/vtium#23).
    ColorSet { height: u32 },
}

/// The PSF fonts the small displays draw with (vtium/vtium#75).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestFont {
    /// `vtium-test-8x8.psf`, PSF2.
    Small8x8,
    /// `vtium-test-8x16.psf`, PSF1.
    Small8x16,
}

impl TestFont {
    /// The font's file, as `tools/testfont` writes it.
    pub fn bytes(self) -> Result<Vec<u8>, String> {
        match self {
            TestFont::Small8x8 => vtium_testfont::small8x8()?.psf2(),
            TestFont::Small8x16 => vtium_testfont::small8x16()?.psf1(),
        }
    }

    /// The name `tools/testfont` gives the font's file.
    fn file(self) -> &'static str {
        match self {
            TestFont::Small8x8 => "vtium-test-8x8.psf",
            TestFont::Small8x16 => "vtium-test-8x16.psf",
        }
    }

    /// The set `vtium-vitrify create` builds from a frit naming the font,
    /// then `fill`: its bytes. The font is parsed in this process, where
    /// `create` parses it in its child.
    pub fn vitrify(self, fill: Option<&str>) -> Result<Vec<u8>, String> {
        use vtium_vitrify::build::{Font as Bitmap, Parsed, Part, assemble, build_key};
        use vtium_vitrify::child::Request;
        use vtium_vitrify::frit::Frit;
        use vtium_vitrify::sources::{Input, Resolved};
        let bytes = self.bytes()?;
        let mut frit = format!("source=file:/usr/share/vtium-tests/{}\n", self.file());
        if let Some(fill) = fill {
            frit.push_str(&format!("source=builtin:{fill}\n"));
        }
        let name = self.file().trim_end_matches(".psf");
        let frit = Frit::parse(frit.as_bytes(), std::path::Path::new("/tier2.vtfrit"), name)
            .map_err(|e| e.to_string())?;
        let mut fonts = vec![(
            Bitmap::from_glyphs(vtium_vitrify::decode::font(&bytes, None, None)?),
            Resolved {
                input: Input::Font(Request {
                    font: bytes.clone(),
                    unimap: None,
                    def_uni: None,
                }),
                sha256: Sha256::digest(&bytes),
                unimap_sha256: None,
                def_uni_sha256: None,
            },
        )];
        if let Some(fill) = fill {
            let font = Bitmap::from_builtin(fill)?;
            let resolved = Resolved {
                input: Input::Builtin(font.clone()),
                sha256: font.digest(),
                unimap_sha256: None,
                def_uni_sha256: None,
            };
            fonts.push((font, resolved));
        }
        let read: Vec<Option<Resolved>> = fonts.iter().map(|(_, r)| Some(r.clone())).collect();
        let parts: Vec<Part<'_>> = frit
            .sources
            .iter()
            .zip(&fonts)
            .map(|(source, (font, resolved))| Part {
                source,
                resolved,
                parsed: Parsed::Bitmap(font.clone()),
            })
            .collect();
        assemble(&frit, &parts, &[], &build_key(&frit, &read))
    }
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

/// A configured decoration color: `underline-color` or `strike-color`
/// as an index into the palette or as a color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecoSpec {
    Index(u8),
    Rgb(u8, u8, u8),
}

/// The decoration colors a screen configures; an absent one follows the
/// cell's foreground.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Decorations {
    pub underline: Option<DecoSpec>,
    pub strike: Option<DecoSpec>,
    /// `force-decoration-colors`: the configured underline color wins over
    /// SGR 58's.
    pub force: bool,
}

impl Decorations {
    /// The colors as the renderer takes them: an index resolved through
    /// each frame's palette, as the C reference resolved it on every frame
    /// when it drew vtium's goldens.
    fn colors(&self) -> DecorationColors {
        let color = |spec: Option<DecoSpec>| match spec {
            None => DecoColor::Foreground,
            Some(DecoSpec::Index(i)) => DecoColor::Indexed(i),
            Some(DecoSpec::Rgb(r, g, b)) => DecoColor::Rgb(Rgb::new(r, g, b)),
        };
        DecorationColors {
            underline: color(self.underline),
            strike: color(self.strike),
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
    /// The display's depth: a 1-bit screen is drawn in black and white.
    pub depth: Depth,
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
            depth: Depth::Color,
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
        FontKind::Psf(font) => {
            let psf = Psf::parse(&font.bytes()?).map_err(|e| format!("{font:?}: {e}"))?;
            draw_with(screen, || Font::new(psf.clone()))
        }
        FontKind::Set { font, fill } => {
            let set = Set::from_bytes(font.vitrify(fill)?).map_err(|e| format!("{font:?}: {e}"))?;
            let set = Arc::new(set);
            let height = set.cells().first().map_or(16, |c| c.height);
            let face = SetFace::new(set, height).ok_or_else(|| format!("{font:?}: no strike"))?;
            draw_with(screen, || Font::new(face.clone()))
        }
        FontKind::ColorSet { height } => {
            let face = SetFace::new(crate::colorset::set()?, height)
                .ok_or("the color set has no strike")?;
            draw_with(screen, || Font::new(face.clone()))
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
        decorations: screen.decorations.colors(),
        depth: screen.depth,
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

#[cfg(test)]
mod tests {
    use super::*;

    // A decoration color that names an index follows the palette, as the
    // C reference resolved it on every frame: a second part that changes the
    // index with OSC 4 draws, where the renderer that took the color once
    // had the screen refused.
    #[test]
    fn decoration_colors_follow_a_palette_change() {
        let screen = Screen {
            decorations: Decorations {
                underline: Some(DecoSpec::Index(196)),
                strike: Some(DecoSpec::Rgb(0, 0xc0, 0xff)),
                force: true,
            },
            next: Some(b"\x1b]4;196;rgb:00/ff/00\x1b\\"),
            ..Screen::new("d", "decorations", b"\x1b[4mx", 4, 1)
        };
        assert_eq!(
            screen.decorations.colors(),
            DecorationColors {
                underline: DecoColor::Indexed(196),
                strike: DecoColor::Rgb(Rgb::new(0, 0xc0, 0xff)),
                force: true,
            }
        );
        let image = draw(&screen).unwrap();
        let green = image.rgb().chunks(3).any(|p| p == [0, 0xff, 0]);
        assert!(green, "the underline did not take index 196's new color");
    }
    // ADR-1200 D10: a PSF test font vitrified as `create` builds it draws
    // every small screen exactly as the PSF loader draws it, so the small
    // displays' goldens judge the set too.
    #[test]
    fn a_vitrified_test_font_draws_as_the_psf_loader_does() {
        let mut compared = Vec::new();
        for case in crate::screens::CASES {
            let FontKind::Psf(font) = case.screen.font else {
                continue;
            };
            compared.push(case.screen.name);
            let psf = draw(&case.screen).unwrap();
            let set = draw(&Screen {
                font: FontKind::Set { font, fill: None },
                ..case.screen
            })
            .unwrap();
            assert!(psf == set, "{}", case.screen.name);
        }
        assert_eq!(compared, ["small-320x240", "small-640x480"]);
    }

    // The color glyph screen at twice its cell, where the cache stores a
    // glyph at the set's size or at its drawn size by configuration: every
    // configuration draws the same bytes, and in the first three rows,
    // which hold no decoration, each pixel is the 16-pixel drawing's
    // doubled. A decoration is drawn for the scaled cell, not doubled.
    #[test]
    fn the_color_set_draws_alike_at_twice_its_cell() {
        let case = crate::screens::CASES
            .iter()
            .find(|c| c.screen.name == "emoji-rgba")
            .expect("the emoji-rgba screen");
        let one = draw(&case.screen).unwrap();
        let two = draw(&Screen {
            font: FontKind::ColorSet { height: 32 },
            ..case.screen
        })
        .unwrap();
        assert_eq!(
            (two.width(), two.height()),
            (2 * one.width(), 2 * one.height())
        );
        let w = one.width() as usize;
        let undecorated = 2 * w * 2 * 48;
        for (i, p) in two.rgb().chunks(3).take(undecorated).enumerate() {
            let (x, y) = (i % (2 * w), i / (2 * w));
            let at = ((y / 2) * w + x / 2) * 3;
            assert_eq!(p, &one.rgb()[at..at + 3], "{x},{y}");
        }
    }
    // A PSF test font draws through vtium-font's PSF loader at its own
    // cell: an 8x8 cell for the 8x8 font, where unifont's would be 8x16,
    // and each glyph's rows as the font holds them.
    #[test]
    fn psf_test_fonts_draw_at_their_own_cell() {
        for (font, height, glyphs) in [
            (TestFont::Small8x8, 8, vtium_testfont::small8x8().unwrap()),
            (
                TestFont::Small8x16,
                16,
                vtium_testfont::small8x16().unwrap(),
            ),
        ] {
            let screen = Screen {
                font: FontKind::Psf(font),
                ..Screen::new("psf", "a capital A", b"A", 1, 1)
            };
            let image = draw(&screen).unwrap();
            assert_eq!((image.width(), image.height()), (8, height), "{font:?}");
            let a = usize::from(b'A' - b' ');
            for (y, want) in glyphs.glyphs[a].iter().enumerate() {
                let row = &image.rgb()[y * 24..(y + 1) * 24];
                let got = row
                    .chunks(3)
                    .fold(0u8, |b, p| (b << 1) | u8::from(p[0] > 0x80));
                assert_eq!(got, *want, "{font:?} row {y}");
            }
        }
    }
}
