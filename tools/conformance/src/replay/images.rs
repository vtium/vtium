//! The replay's image side: the reference host's image settings, the image
//! commands, and the fixture host answering requests (INTERFACE.md, "The
//! commands", "The fixture host" and "Reference host").

use std::path::Path;

use vtium_term::images::{
    ImageKey, Iterm2, Kitty, Limits, NewImage, Place, Protocol, Sixel, Switches,
};
use vtium_term::{ImageString, Terminal};

use crate::command::{self, Command};
use crate::host::Host;

/// The reference host's budget: 320,000,000 bytes for each screen, which
/// vtium reads as each console's store (ADR-1100, D11).
const BUDGET: u64 = 320_000_000;
/// The reference host's cell size.
const CELL: (u32, u32) = (8, 16);
/// `graphic-max` and `registers` take 0 as the default.
const GRAPHIC_DEFAULT: u32 = 10_000;
const REGISTERS_DEFAULT: u32 = 1024;

/// The images the test commands made, by the number the stream gives
/// them, and the fixture host while it is installed.
pub(super) struct Images {
    made: [Option<ImageKey>; 64],
    host: Option<Host>,
    fixtures: std::path::PathBuf,
}

impl Images {
    /// The reference host's image settings on `term`: the budget, the cell
    /// size, the sixel, kitty and iTerm2 handlers installed, the three
    /// protocols on, and the fixture host installed.
    pub(super) fn new(term: &mut Terminal, fixtures: &Path) -> Self {
        term.set_image_handler(ImageString::Sixel, Some(Box::new(Sixel::default())));
        term.set_image_handler(ImageString::Kitty, Some(Box::new(Kitty::default())));
        term.set_image_handler(ImageString::Iterm2, Some(Box::new(Iterm2::default())));
        term.screen_mut().set_image_limit(BUDGET);
        let _ = term.screen_mut().set_image_cell(CELL.0, CELL.1);
        term.set_image_switches(Switches {
            sixel: true,
            kitty: true,
            iterm2: true,
        });
        term.set_image_limits(Limits::default());
        term.set_image_helper(true);
        Self {
            made: [None; 64],
            host: Some(Host::new(fixtures)),
            fixtures: fixtures.to_path_buf(),
        }
    }

    /// Runs an image command; returns the `test` line it prints, if any.
    pub(super) fn command(&mut self, term: &mut Terminal, cmd: Command) -> Option<String> {
        let error = |name: &str, errno: i32| Some(format!("test {name} error {errno}\n"));
        match cmd {
            Command::Cell { width, height } => {
                match term.screen_mut().set_image_cell(width, height) {
                    Ok(()) => {
                        term.set_cell_size(width, height);
                        None
                    }
                    Err(e) => error("cell", e.errno()),
                }
            }
            Command::Limit(bytes) => {
                term.screen_mut().set_image_limit(bytes);
                None
            }
            Command::Image(i) => self.image(term, i).err().and_then(|e| error("image", e)),
            Command::Place(p) => self.place(term, p).err().and_then(|e| error("place", e)),
            Command::Sixel(on) => {
                let s = term.image_switches();
                term.set_image_switches(Switches { sixel: on, ..s });
                None
            }
            Command::KittyGraphics(on) => {
                let s = term.image_switches();
                term.set_image_switches(Switches { kitty: on, ..s });
                None
            }
            Command::GraphicMax { width, height } => {
                let or = |v: u32| if v == 0 { GRAPHIC_DEFAULT } else { v };
                let l = term.image_limits();
                term.set_image_limits(Limits {
                    graphic: (or(width), or(height)),
                    ..l
                });
                None
            }
            Command::Registers { count, private } => {
                let l = term.image_limits();
                term.set_image_limits(Limits {
                    registers: if count == 0 { REGISTERS_DEFAULT } else { count },
                    private_registers: private,
                    ..l
                });
                None
            }
            Command::Host(on) => {
                self.host = on.then(|| Host::new(&self.fixtures));
                term.set_image_helper(on);
                None
            }
            _ => None,
        }
    }

    /// `image`: `w` by `h` pixels of `fill`, as a sixel would make it or a
    /// kitty image of id `id`, transient with `tr`.
    fn image(&mut self, term: &mut Terminal, i: command::Image) -> Result<(), i32> {
        // a kitty image is named by its id
        if i.kitty && i.id == 0 {
            return Err(-22);
        }
        let n = usize::from(i.n);
        let pixels = usize::try_from(u64::from(i.width).saturating_mul(u64::from(i.height)))
            .map_err(|_| -22)?;
        let image = NewImage {
            protocol: if i.kitty {
                Protocol::KITTY
            } else {
                Protocol::SIXEL
            },
            width: i.width,
            height: i.height,
            pixels: vec![i.fill; pixels],
            id: if i.kitty { i.id } else { 0 },
            number: 0,
            transient: i.transient,
            opaque: None,
        };
        let key = term.screen_mut().add_image(image).map_err(|e| e.errno())?;
        if let Some(slot) = self.made.get_mut(n) {
            *slot = Some(key);
        }
        Ok(())
    }

    /// `place`: image `img` with its box's top left at `col`, `row`, the
    /// cursor's when absent.
    fn place(&mut self, term: &mut Terminal, p: command::Place) -> Result<(), i32> {
        let key = self
            .made
            .get(usize::from(p.img))
            .copied()
            .flatten()
            .ok_or(-2)?;
        let (cx, cy) = term.screen().cursor();
        let to_u32 = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
        let place = Place {
            col: p.col.unwrap_or(to_u32(cx)),
            row: p.row.unwrap_or(to_u32(cy)),
            x: p.x,
            y: p.y,
            cols: p.cols,
            rows: p.rows,
            src_x: p.src_x,
            src_y: p.src_y,
            src_width: p.src_width,
            src_height: p.src_height,
            z: p.z,
            id: p.id,
        };
        term.screen_mut()
            .place_image(key, place)
            .map(|_| ())
            .map_err(|e| e.errno())
    }

    /// Answers the request out with the fixture host: its `host:` line and
    /// the bytes the answer made the terminal write. Without the host, or
    /// with no request out, nothing.
    pub(super) fn answer(&mut self, term: &mut Terminal) -> Option<(String, Vec<u8>)> {
        let host = self.host.as_mut()?;
        let request = term.take_image_request()?;
        let (line, reply) = host.answer(&request);
        term.image_reply(reply);
        Some((line, term.take_replies()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A terminal with the reference host's image settings.
    fn host() -> Terminal {
        let mut t = Terminal::new(10, 4, 16);
        let _ = Images::new(&mut t, Path::new("fixtures"));
        t
    }

    #[test]
    fn the_reference_host_installs_the_sixel_kitty_and_iterm2_handlers() {
        let t = host();
        let installed = |which, is: fn(&dyn core::any::Any) -> bool| {
            t.image_handler(which).is_some_and(|h| is(h.as_any()))
        };
        assert!(installed(ImageString::Sixel, |h| h.is::<Sixel>()));
        assert!(installed(ImageString::Kitty, |h| h.is::<Kitty>()));
        assert!(installed(ImageString::Iterm2, |h| h.is::<Iterm2>()));
        assert!(t.image_switches().iterm2);
    }

    #[test]
    fn the_handlers_read_a_dcs_q_and_an_apc_g_string() {
        // the sixel handler draws its picture at the cursor
        let mut t = host();
        t.input(b"ab");
        t.input(b"\x1bPq#0;2;100;0;0#0~~~~-~~\x1b\\");
        assert!(t.screen().images().work() > 0, "the sixel handler read it");
        let placed = t.screen().placements();
        assert_eq!(placed.len(), 1);
        assert_eq!((placed[0].col, placed[0].image_width), (2, 4));
        assert!(t.replies().is_empty());
        // the kitty handler places the picture an APC G string sends, and
        // answers nothing for an image without an id
        let mut t = host();
        t.input(b"ab");
        t.input(b"\x1b_Ga=T,f=24,s=1,v=1;AAAA\x1b\\");
        assert!(t.screen().images().work() > 0, "the kitty handler read it");
        assert_eq!(t.screen().images().placement_count(), 1);
        assert!(t.replies().is_empty());
    }
}
