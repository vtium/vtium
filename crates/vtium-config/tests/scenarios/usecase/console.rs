//! The stand-in console of the use cases: a vtium console that reads what
//! vtium-config writes to it and refuses what vtium refuses.
//!
//! It answers XTVERSION, DA1 and the OSC 50 query as a vtium console with
//! `allow-font-ops` on does. An OSC 50 switch names a font set, and the
//! console loads that set the way vtium's font helper loads it: through
//! [`vtium_config::serve::Engine::load_through`], behind the gate that
//! batch.conf's active fonts open. So a set whose font files are not
//! installed, or are installed but not among the active fonts, draws
//! nothing, and the console then draws the built-in font, at the cell
//! vtium would use. The stand-in calls vtium's own refusal code for this
//! rather than copying it, so it refuses exactly what that code refuses.
//!
//! OSC 4 and OSC 104 change the palette, and the grid request `CSI 8 ;
//! rows ; cols t` changes the grid, which is how vtium meets a new font
//! size. After every key the runner can read the font set the console
//! was asked for, the font it really draws, its cell, its palette by name,
//! and its grid.
//!
//! When the person restarts vtium, the console reads the installed
//! vtium.conf again for its head, through vtium-settings' own reading of
//! a head's keys, [`vtium_settings::value_for`].
//!
//! References:
//! - ADR-1205 D4: a font draws only while the batch lists it and the file passes the trust rule.
//! - ADR-1201 D4, "Sizes, live on a vtium console": vtium meets the grid request by changing the font size.

use std::path::{Path, PathBuf};

use vtium_config::serve::{ActiveAsk, Engine, FritText, OwnCell};
use vtium_config::tui::input::Reply;
use vtium_font::Cell;
use vtium_term::{Palette, Rgb};

/// Where the console finds what it draws from.
#[derive(Clone, Debug)]
pub struct Places {
    /// The frits' directory, which holds `NAME.vtfrit`.
    pub frits: PathBuf,
    /// The installed vtium.conf.
    pub conf: PathBuf,
    /// The installed batch.conf, the active fonts.
    pub batch: PathBuf,
    /// The folders the font helper searches for a `font:` source.
    pub font_dirs: Vec<PathBuf>,
}

/// What the console draws its text with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Drawn {
    /// The font set of this name, or a built-in font that the set names.
    Set(String),
    /// The built-in font, because the font set asked for draws nothing:
    /// why, as the font helper says it.
    BuiltIn(String),
}

/// The stand-in console.
#[derive(Clone, Debug)]
pub struct Console {
    pub places: Places,
    /// The head this console runs, whose section of vtium.conf it reads.
    pub head: String,
    /// The display's density, which turns a size in points into pixels.
    pub dpi: u32,
    /// The font set asked for last, which the OSC 50 query reports.
    pub font: String,
    pub drawn: Drawn,
    /// The cell the console draws in, in pixels.
    pub cell: Cell,
    /// The pixel height the console draws its font at.
    pub height: u32,
    /// The sixteen colors of the palette in use.
    pub colors: [Rgb; 16],
    /// The grid, columns by rows.
    pub grid: (usize, usize),
    /// Every byte vtium-config wrote, in order.
    pub received: Vec<u8>,
    /// The fonts the helper found missing or refused while the console
    /// loaded the set in use, as the console logs them.
    pub log: Vec<String>,
}

/// The height a console draws at when vtium.conf sets no size: the
/// built-in font's.
const DEFAULT_HEIGHT: u32 = 16;

impl Console {
    /// The console as vtium starts it on `head` from the installed files,
    /// at `grid`.
    pub fn start(places: Places, head: &str, dpi: u32, grid: (usize, usize)) -> Console {
        let mut console = Console {
            places,
            head: head.to_owned(),
            dpi,
            font: String::new(),
            drawn: Drawn::BuiltIn(String::new()),
            cell: Cell {
                width: 8,
                height: 16,
            },
            height: DEFAULT_HEIGHT,
            colors: named("legacy"),
            grid,
            received: Vec::new(),
            log: Vec::new(),
        };
        console.restart();
        console
    }

    /// vtium starts again: it reads the installed vtium.conf for its head,
    /// its font set, size and palette, and loads the font set.
    pub fn restart(&mut self) {
        let conf = std::fs::read_to_string(&self.places.conf).unwrap_or_default();
        let value = |key: &str| vtium_settings::value_for(&conf, &self.head, key).map(|(v, _)| v);
        self.height = value("font-size")
            .and_then(|s| s.trim().parse::<u32>().ok())
            .filter(|p| *p > 0)
            .map_or(DEFAULT_HEIGHT, |points| {
                vtium_config::specimen::pixel_height(points, self.dpi)
            });
        self.colors = named(value("palette").as_deref().unwrap_or("legacy"));
        let font = value("font-name").unwrap_or_else(|| "unifont".to_owned());
        self.switch(&font);
    }

    /// The palette in use by its name, when its sixteen colors are one of
    /// vtium's named palettes, else `custom`.
    pub fn palette(&self) -> String {
        vtium_term::PALETTE_NAMES
            .iter()
            .find(|n| named(n) == self.colors)
            .map_or_else(|| "custom".to_owned(), |n| (*n).to_owned())
    }

    /// OSC 50 with a name: the console loads the set behind the active
    /// fonts' gate, and draws the built-in font when the set draws
    /// nothing.
    fn switch(&mut self, name: &str) {
        self.font = name.to_owned();
        self.log.clear();
        let loaded = self.load(name);
        let (drawn, own) = match loaded {
            Ok((own, warnings)) => {
                self.log = warnings;
                (Drawn::Set(name.to_owned()), own)
            }
            Err(why) => {
                self.log.push(why.clone());
                let builtin = vtium_font::set::builtin_name(self.height, "C");
                (Drawn::BuiltIn(why), builtin_cell(builtin))
            }
        };
        self.drawn = drawn;
        self.cell = match own {
            // a bitmap font draws at the largest whole multiple of its own
            // cell that is not over the height asked for
            OwnCell::Bitmap(c) => {
                let k = self.height.checked_div(c.height).unwrap_or(1).max(1);
                Cell {
                    width: c.width.saturating_mul(k),
                    height: c.height.saturating_mul(k),
                }
            }
            OwnCell::Outline(c) => c,
        };
    }

    /// Loads the set `name` as vtium's font helper does behind the active
    /// fonts: its own cell at the console's height, and the warnings the
    /// console logs, or why the set draws nothing.
    fn load(&self, name: &str) -> Result<(OwnCell, Vec<String>), String> {
        if vtium_config::frit::RESERVED.contains(&name) {
            return Ok((builtin_cell(name), Vec::new()));
        }
        let path = self.places.frits.join(format!("{name}.vtfrit"));
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("there is no font set {name}: {e}"))?;
        let batch = std::fs::read(&self.places.batch).unwrap_or_default();
        let read = vtium_settings::batch::Batch::parse(&batch, &self.places.batch);
        let paths: Vec<PathBuf> = read
            .batch
            .entries()
            .iter()
            .map(|vtium_settings::batch::Entry::Path(p)| p.clone())
            .collect();
        let euid = rustix::process::geteuid().as_raw();
        let gate = ActiveAsk { owner: euid, paths }.gate(euid)?;
        let helper_paths = vtium_config::serve::Paths {
            font_dirs: self.places.font_dirs.clone(),
            kbd_dirs: Vec::new(),
            vconsole_conf: Path::new("/nonexistent/vconsole.conf").to_path_buf(),
            kernel_cmdline: Path::new("/nonexistent/cmdline").to_path_buf(),
            locale: "C".to_owned(),
        };
        let frit = FritText {
            name: name.to_owned(),
            text,
        };
        let engine = Engine::load_through(&frit, &helper_paths, None, &gate)?;
        let own = engine.own_cell(self.height)?;
        Ok((own, engine.warnings().to_vec()))
    }

    /// Reads `bytes` as the console does, and gives the replies it sends
    /// back, in the order their queries came.
    pub fn read(&mut self, bytes: &[u8]) -> Vec<Reply> {
        self.received.extend_from_slice(bytes);
        let text = String::from_utf8_lossy(bytes).into_owned();
        let mut replies = Vec::new();
        let mut rest = text.as_str();
        while let Some(at) = rest.find('\x1b') {
            rest = &rest[at..];
            if let Some(after) = rest.strip_prefix("\x1b[>q") {
                replies.push(Reply::Version("vtium(0.1.0)".into()));
                rest = after;
            } else if let Some(after) = rest.strip_prefix("\x1b[c") {
                replies.push(Reply::Attributes);
                rest = after;
            } else if let Some(osc) = rest.strip_prefix("\x1b]") {
                let Some((body, after)) =
                    osc.split_once("\x1b\\").or_else(|| osc.split_once('\x07'))
                else {
                    break;
                };
                rest = after;
                self.osc(body, &mut replies);
            } else if let Some(csi) = rest.strip_prefix("\x1b[8;") {
                let end = csi.find('t').unwrap_or(csi.len());
                let (params, after) = csi.split_at(end);
                let mut n = params.split(';').filter_map(|p| p.parse::<usize>().ok());
                if let (Some(rows), Some(cols)) = (n.next(), n.next()) {
                    self.grid = (cols, rows);
                }
                rest = after.get(1..).unwrap_or_default();
            } else {
                rest = rest.get(1..).unwrap_or_default();
            }
        }
        replies
    }

    /// One OSC string's body, its number and its text.
    fn osc(&mut self, body: &str, replies: &mut Vec<Reply>) {
        let (number, text) = body.split_once(';').unwrap_or((body, ""));
        match number {
            "50" if text == "?" => replies.push(Reply::Font(self.font.clone())),
            "50" => self.switch(text),
            "4" => {
                let mut parts = text.split(';');
                while let (Some(index), Some(spec)) = (parts.next(), parts.next()) {
                    let (Ok(index), Some(rgb)) = (index.parse::<usize>(), parse_rgb(spec)) else {
                        continue;
                    };
                    if let Some(c) = self.colors.get_mut(index) {
                        *c = rgb;
                    }
                }
            }
            "104" => {
                let conf = std::fs::read_to_string(&self.places.conf).unwrap_or_default();
                let name = vtium_settings::value_for(&conf, &self.head, "palette")
                    .map_or_else(|| "legacy".to_owned(), |(v, _)| v);
                self.colors = named(&name);
            }
            _ => {}
        }
    }
}

/// The own cell of vtium's built-in font `name`: Unscii-8's 8×8, and
/// Unifont's 8×16.
fn builtin_cell(name: &str) -> OwnCell {
    let height = if name == "unscii-8" { 8 } else { 16 };
    OwnCell::Bitmap(Cell { width: 8, height })
}

/// The sixteen colors of vtium's palette `name`, `legacy`'s for a name
/// vtium does not know.
fn named(name: &str) -> [Rgb; 16] {
    let palette = Palette::named(name)
        .or_else(|| Palette::named("legacy"))
        .expect("vtium's palette legacy");
    std::array::from_fn(|i| palette.color(u8::try_from(i).unwrap_or(0)))
}

/// A color as X11 writes it in the two forms a program sends:
/// `rgb:RR/GG/BB`, one to four hex digits for each channel, and `#RRGGBB`.
fn parse_rgb(spec: &str) -> Option<Rgb> {
    let channel = |hex: &str| -> Option<u8> {
        let v = u32::from_str_radix(hex, 16).ok()?;
        let bits = u32::try_from(hex.len()).ok()?.checked_mul(4)?;
        let max = 1u32.checked_shl(bits)?.checked_sub(1)?;
        u8::try_from(v.checked_mul(255)?.checked_div(max)?).ok()
    };
    if let Some(rgb) = spec.strip_prefix("rgb:") {
        let mut p = rgb.split('/');
        return Some(Rgb {
            r: channel(p.next()?)?,
            g: channel(p.next()?)?,
            b: channel(p.next()?)?,
        });
    }
    let hex = spec.strip_prefix('#').filter(|h| h.len() == 6)?;
    Some(Rgb {
        r: channel(hex.get(0..2)?)?,
        g: channel(hex.get(2..4)?)?,
        b: channel(hex.get(4..6)?)?,
    })
}
