//! The font the kernel holds for a VT, and its Unicode map (ADR-1200 D8):
//! `KDGETMODE`, `KDFONTOP`'s get and `GIO_UNIMAP`, for `vtium-vitrify`'s
//! `console:` source. The console makes none of these calls.
//!
//! [`read`] holds the logic over [`FontIoctls`], so a test stands in for
//! the kernel; [`ConsoleTty`] is the kernel's VT. The kernel answers a font
//! get only for a VT in text mode (`drivers/tty/vt/vt.c`, `con_font_get`),
//! so [`read`] checks the mode first and names it.

use std::ffi::c_int;
use std::fmt;
use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::path::Path;

use rustix::fs::{Mode, OFlags};
use rustix::ioctl::{Getter, Opcode, Updater, ioctl};

/// `KDGETMODE`, `linux/kd.h`.
const KDGETMODE: Opcode = 0x4B3B;
/// `KDFONTOP`, `linux/kd.h`.
const KDFONTOP: Opcode = 0x4B72;
/// `GIO_UNIMAP`, `linux/kd.h`.
const GIO_UNIMAP: Opcode = 0x4B66;
/// `KD_TEXT`, `linux/kd.h`.
pub const KD_TEXT: u32 = 0;
/// `KD_FONT_OP_GET`, `linux/kd.h`: glyph rows at a pitch of 32.
pub const KD_FONT_OP_GET: u32 = 1;
/// `KD_FONT_OP_GET_TALL`, `linux/kd.h`: glyph rows at the pitch the call
/// gives as its height.
pub const KD_FONT_OP_GET_TALL: u32 = 5;

/// The widest glyph asked for: a PSF row is read into 32 bits.
pub const FONT_WIDTH_MAX: u32 = 32;
/// The row pitch `KD_FONT_OP_GET_TALL` is asked for: the kernel's tallest
/// font, `max_font_height` in `drivers/tty/vt/vt.c`.
pub const FONT_PITCH_TALL: u32 = 64;
/// The row pitch of `KD_FONT_OP_GET`.
pub const FONT_PITCH: u32 = 32;
/// The most glyphs a console font holds, `max_font_glyphs`.
pub const FONT_GLYPHS_MAX: u32 = 512;

/// The bytes a get at `pitch` writes for the largest font asked for.
fn font_bytes(pitch: u32) -> usize {
    let bytes = FONT_GLYPHS_MAX
        .saturating_mul(FONT_WIDTH_MAX.div_ceil(8))
        .saturating_mul(pitch);
    usize::try_from(bytes).unwrap_or(usize::MAX)
}

/// `struct console_font_op`, `linux/kd.h`.
#[repr(C)]
#[derive(Debug)]
struct ConsoleFontOp {
    op: u32,
    flags: u32,
    width: u32,
    height: u32,
    charcount: u32,
    data: *mut u8,
}

/// `struct unipair`, `linux/kd.h`: a codepoint and the glyph it draws.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UniPair {
    pub unicode: u16,
    pub fontpos: u16,
}

/// `struct unimapdesc`, `linux/kd.h`.
#[repr(C)]
#[derive(Debug)]
struct UnimapDesc {
    entry_ct: u16,
    entries: *mut UniPair,
}

/// A font get's answer: the font's width, height and glyph count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontShape {
    pub width: u32,
    pub height: u32,
    pub charcount: u32,
}

/// The three calls, as the kernel answers them: what [`read`] needs of a
/// VT.
pub trait FontIoctls {
    /// `KDGETMODE`: `KD_TEXT` or `KD_GRAPHICS`.
    fn display_mode(&self) -> io::Result<u32>;

    /// `KDFONTOP` with `op`, a get, asking for at most `width` by
    /// `height` and `data.len()` bytes: the font's glyphs written into
    /// `data`, each `pitch` rows of `width.div_ceil(8)` bytes, where
    /// `pitch` is 32 for `KD_FONT_OP_GET` and `height` for
    /// `KD_FONT_OP_GET_TALL`.
    fn font_get(&self, op: u32, width: u32, height: u32, data: &mut [u8]) -> io::Result<FontShape>;

    /// `GIO_UNIMAP` into `entries`: the count of the kernel's map, which
    /// is written whole when it fits. `ENOMEM` with the count when it does
    /// not.
    fn unimap(&self, entries: &mut [UniPair]) -> (io::Result<()>, u16);
}

/// A VT's font and Unicode map, its glyphs packed `height` rows each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsoleFont {
    pub width: u32,
    pub height: u32,
    pub charcount: u32,
    /// `charcount` glyphs of `height` rows of `width.div_ceil(8)` bytes,
    /// the first pixel the high bit.
    pub data: Vec<u8>,
    /// The map's pairs as the kernel gave them.
    pub unimap: Vec<UniPair>,
}

/// Why a VT's font could not be read.
#[derive(Debug)]
#[non_exhaustive]
pub enum FontError {
    /// The VT is in graphics mode, so the kernel answers no font get.
    Graphics,
    /// The font is wider than 32 pixels, or past the kernel's shape.
    Shape(FontShape),
    /// A call failed.
    Io(&'static str, io::Error),
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FontError::Graphics => f.write_str(
                "the VT is in graphics mode, and the kernel gives a font only for a VT in text mode",
            ),
            FontError::Shape(s) => write!(
                f,
                "the kernel's font is {}x{} with {} glyphs, past {FONT_WIDTH_MAX} pixels wide, \
                 {FONT_PITCH_TALL} high or {FONT_GLYPHS_MAX} glyphs",
                s.width, s.height, s.charcount
            ),
            FontError::Io(call, e) => write!(f, "{call}: {e}"),
        }
    }
}

impl std::error::Error for FontError {}

/// Reads the font and Unicode map of the VT `dev` stands for: the mode
/// checked, the font got with `KD_FONT_OP_GET_TALL`, or with
/// `KD_FONT_OP_GET` on a kernel without it, and the map got whole.
pub fn read(dev: &impl FontIoctls) -> Result<ConsoleFont, FontError> {
    let mode = dev
        .display_mode()
        .map_err(|e| FontError::Io("KDGETMODE", e))?;
    if mode != KD_TEXT {
        return Err(FontError::Graphics);
    }
    let mut tall = vec![0u8; font_bytes(FONT_PITCH_TALL)];
    let (shape, pitch, data) = match dev.font_get(
        KD_FONT_OP_GET_TALL,
        FONT_WIDTH_MAX,
        FONT_PITCH_TALL,
        &mut tall,
    ) {
        Ok(shape) => (shape, FONT_PITCH_TALL, tall),
        // a kernel before KD_FONT_OP_GET_TALL refuses the op
        Err(e) if matches!(e.raw_os_error(), Some(libc::EINVAL | libc::ENOSYS)) => {
            let mut short = vec![0u8; font_bytes(FONT_PITCH)];
            let shape = dev
                .font_get(KD_FONT_OP_GET, FONT_WIDTH_MAX, FONT_PITCH, &mut short)
                .map_err(|e| FontError::Io("KDFONTOP", e))?;
            (shape, FONT_PITCH, short)
        }
        Err(e) => return Err(FontError::Io("KDFONTOP", e)),
    };
    if !(1..=FONT_WIDTH_MAX).contains(&shape.width)
        || !(1..=pitch).contains(&shape.height)
        || !(1..=FONT_GLYPHS_MAX).contains(&shape.charcount)
    {
        return Err(FontError::Shape(shape));
    }
    let data = pack(&data, shape, pitch).ok_or(FontError::Shape(shape))?;
    Ok(ConsoleFont {
        width: shape.width,
        height: shape.height,
        charcount: shape.charcount,
        data,
        unimap: unimap(dev)?,
    })
}

/// The glyphs of `data`, `pitch` rows apart, packed `shape.height` rows
/// each.
fn pack(data: &[u8], shape: FontShape, pitch: u32) -> Option<Vec<u8>> {
    let row = usize::try_from(shape.width.div_ceil(8)).ok()?;
    let height = usize::try_from(shape.height).ok()?;
    let stride = row.checked_mul(usize::try_from(pitch).ok()?)?;
    let keep = row.checked_mul(height)?;
    let count = usize::try_from(shape.charcount).ok()?;
    let mut out = Vec::with_capacity(keep.checked_mul(count)?);
    for glyph in 0..count {
        let start = glyph.checked_mul(stride)?;
        out.extend_from_slice(data.get(start..start.checked_add(keep)?)?);
    }
    Some(out)
}

/// The kernel's Unicode map, asked for again at the size it gives while it
/// grows.
fn unimap(dev: &impl FontIoctls) -> Result<Vec<UniPair>, FontError> {
    let mut entries = vec![UniPair::default(); 256];
    for _ in 0..4 {
        let (result, count) = dev.unimap(&mut entries);
        match result {
            Ok(()) => {
                entries.truncate(usize::from(count));
                return Ok(entries);
            }
            Err(e) if e.raw_os_error() == Some(libc::ENOMEM) => {
                entries = vec![UniPair::default(); usize::from(count)];
            }
            Err(e) => return Err(FontError::Io("GIO_UNIMAP", e)),
        }
    }
    Err(FontError::Io(
        "GIO_UNIMAP",
        io::Error::from_raw_os_error(libc::ENOMEM),
    ))
}

/// A VT's tty opened to read its font: read-only, never as the
/// controlling terminal.
#[derive(Debug)]
pub struct ConsoleTty {
    fd: OwnedFd,
}

impl ConsoleTty {
    /// Opens the VT at `path`, such as `/dev/tty3`.
    pub fn open(path: &Path) -> io::Result<ConsoleTty> {
        let fd = rustix::fs::open(
            path,
            OFlags::RDONLY | OFlags::NOCTTY | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        Ok(ConsoleTty { fd })
    }
}

impl FontIoctls for ConsoleTty {
    fn display_mode(&self) -> io::Result<u32> {
        #[expect(unsafe_code, reason = "an ioctl rustix does not wrap")]
        // SAFETY: KDGETMODE writes one `int`.
        let mode = unsafe { ioctl(self.fd.as_fd(), Getter::<KDGETMODE, c_int>::new())? };
        u32::try_from(mode).map_err(|_| io::ErrorKind::InvalidData.into())
    }

    fn font_get(&self, op: u32, width: u32, height: u32, data: &mut [u8]) -> io::Result<FontShape> {
        let pitch = if op == KD_FONT_OP_GET_TALL {
            height
        } else {
            FONT_PITCH
        };
        // the kernel writes width.div_ceil(8) * pitch * charcount bytes for
        // the font it holds, at most this many glyphs
        let per_glyph = usize::try_from(width.div_ceil(8).saturating_mul(pitch))
            .map_err(|_| io::ErrorKind::InvalidInput)?;
        let charcount = u32::try_from(data.len().checked_div(per_glyph).unwrap_or(0))
            .unwrap_or(u32::MAX)
            .min(FONT_GLYPHS_MAX);
        let mut cfo = ConsoleFontOp {
            op,
            flags: 0,
            width,
            height,
            charcount,
            data: data.as_mut_ptr(),
        };
        #[expect(unsafe_code, reason = "an ioctl rustix does not wrap")]
        // SAFETY: KDFONTOP reads and writes one `struct console_font_op`,
        // which `ConsoleFontOp` lays out field for field. The kernel
        // refuses a font of more than `charcount` glyphs or wider or taller
        // than asked with ENOSPC before it writes, so it writes at most
        // `charcount` glyphs of `width.div_ceil(8) * pitch` bytes through
        // `data`, which holds that many and outlives the call.
        unsafe {
            ioctl(
                self.fd.as_fd(),
                Updater::<KDFONTOP, ConsoleFontOp>::new(&mut cfo),
            )?;
        }
        Ok(FontShape {
            width: cfo.width,
            height: cfo.height,
            charcount: cfo.charcount,
        })
    }

    fn unimap(&self, entries: &mut [UniPair]) -> (io::Result<()>, u16) {
        let mut desc = UnimapDesc {
            entry_ct: u16::try_from(entries.len()).unwrap_or(u16::MAX),
            entries: entries.as_mut_ptr(),
        };
        #[expect(unsafe_code, reason = "an ioctl rustix does not wrap")]
        // SAFETY: GIO_UNIMAP reads one `struct unimapdesc`, which
        // `UnimapDesc` lays out field for field, writes at most `entry_ct`
        // pairs through `entries`, which holds that many and outlives the
        // call, and writes the map's count back into `entry_ct`.
        let result = unsafe {
            ioctl(
                self.fd.as_fd(),
                Updater::<GIO_UNIMAP, UnimapDesc>::new(&mut desc),
            )
        };
        (result.map_err(io::Error::from), desc.entry_ct)
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    reason = "a test fails by panicking"
)]
mod tests {
    use std::cell::RefCell;
    use std::path::PathBuf;

    use super::*;

    /// `linux/kd.h` from `VTIUM_KERNEL_HEADERS`, or `/usr/include`.
    fn kd_h() -> String {
        let root = std::env::var_os("VTIUM_KERNEL_HEADERS")
            .map_or_else(|| PathBuf::from("/usr/include"), PathBuf::from);
        let path = root.join("linux/kd.h");
        std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "cannot read {}: {e}; install the kernel's UAPI headers or set VTIUM_KERNEL_HEADERS",
                path.display()
            )
        })
    }

    /// The value of `#define NAME VALUE` in `text`, decimal or hex.
    fn define(text: &str, name: &str) -> u32 {
        text.lines()
            .find_map(|line| {
                let mut words = line.split_whitespace();
                (words.next() == Some("#define") && words.next() == Some(name))
                    .then(|| words.next())
                    .flatten()
            })
            .map(|value| match value.strip_prefix("0x") {
                Some(hex) => u32::from_str_radix(hex, 16),
                None => value.parse(),
            })
            .unwrap_or_else(|| panic!("{name} is not defined"))
            .unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    #[test]
    fn request_numbers_match_the_header() {
        let kd = kd_h();
        for (name, ours) in [
            ("KDGETMODE", KDGETMODE),
            ("KDFONTOP", KDFONTOP),
            ("GIO_UNIMAP", GIO_UNIMAP),
        ] {
            assert_eq!(define(&kd, name), ours, "{name}");
        }
        for (name, ours) in [
            ("KD_TEXT", KD_TEXT),
            ("KD_FONT_OP_GET", KD_FONT_OP_GET),
            ("KD_FONT_OP_GET_TALL", KD_FONT_OP_GET_TALL),
        ] {
            assert_eq!(define(&kd, name), ours, "{name}");
        }
    }

    #[test]
    fn structures_have_the_headers_layout() {
        let ptr = size_of::<*mut u8>();
        // five unsigned ints, then the pointer at its alignment
        assert_eq!(
            size_of::<ConsoleFontOp>(),
            (20usize).next_multiple_of(ptr) + ptr
        );
        assert_eq!(size_of::<UniPair>(), 4);
        assert_eq!(size_of::<UnimapDesc>(), 2usize.next_multiple_of(ptr) + ptr);
    }

    /// A stand-in kernel: a VT's mode, font and map, answered as
    /// `con_font_get` and `con_get_unimap` answer.
    struct Kernel {
        mode: u32,
        tall: bool,
        shape: FontShape,
        /// Each glyph's rows, `shape.height` of a byte each.
        glyphs: Vec<Vec<u8>>,
        map: Vec<UniPair>,
        calls: RefCell<Vec<u32>>,
    }

    impl Kernel {
        fn new(width: u32, height: u32, count: u32) -> Kernel {
            let glyphs = (0..count)
                .map(|g| (0..height).map(|r| (g as u8) ^ (r as u8)).collect())
                .collect();
            Kernel {
                mode: KD_TEXT,
                tall: true,
                shape: FontShape {
                    width,
                    height,
                    charcount: count,
                },
                glyphs,
                map: (0..300)
                    .map(|i| UniPair {
                        unicode: 0x100 + i,
                        fontpos: i % count as u16,
                    })
                    .collect(),
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl FontIoctls for Kernel {
        fn display_mode(&self) -> io::Result<u32> {
            Ok(self.mode)
        }

        fn font_get(
            &self,
            op: u32,
            width: u32,
            height: u32,
            data: &mut [u8],
        ) -> io::Result<FontShape> {
            self.calls.borrow_mut().push(op);
            let pitch = match op {
                KD_FONT_OP_GET_TALL if self.tall => height,
                KD_FONT_OP_GET => 32,
                _ => return Err(io::Error::from_raw_os_error(libc::EINVAL)),
            };
            let s = self.shape;
            if s.width > width || s.height > height {
                return Err(io::Error::from_raw_os_error(libc::ENOSPC));
            }
            let row = s.width.div_ceil(8) as usize;
            for (g, rows) in self.glyphs.iter().enumerate() {
                for (r, &b) in rows.iter().enumerate() {
                    data[g * row * pitch as usize + r * row] = b;
                }
            }
            Ok(s)
        }

        fn unimap(&self, entries: &mut [UniPair]) -> (io::Result<()>, u16) {
            let count = self.map.len() as u16;
            if self.map.len() > entries.len() {
                return (Err(io::Error::from_raw_os_error(libc::ENOMEM)), count);
            }
            entries[..self.map.len()].copy_from_slice(&self.map);
            (Ok(()), count)
        }
    }

    fn packed(k: &Kernel) -> Vec<u8> {
        k.glyphs.iter().flatten().copied().collect()
    }

    #[test]
    fn a_tall_get_packs_the_glyphs_and_reads_the_whole_map() {
        let k = Kernel::new(8, 16, 256);
        let font = read(&k).expect("read");
        assert_eq!((font.width, font.height, font.charcount), (8, 16, 256));
        assert_eq!(font.data, packed(&k));
        // 300 pairs pass the first 256 asked for, so the map is asked again
        assert_eq!(font.unimap, k.map);
        assert_eq!(*k.calls.borrow(), [KD_FONT_OP_GET_TALL]);
    }

    #[test]
    fn a_kernel_without_the_tall_get_is_read_at_a_pitch_of_32() {
        let mut k = Kernel::new(8, 14, 512);
        k.tall = false;
        let font = read(&k).expect("read");
        assert_eq!(font.data, packed(&k));
        assert_eq!(*k.calls.borrow(), [KD_FONT_OP_GET_TALL, KD_FONT_OP_GET]);
    }

    #[test]
    fn a_vt_in_graphics_mode_is_refused_before_any_font_call() {
        let mut k = Kernel::new(8, 16, 256);
        k.mode = 1;
        assert!(matches!(read(&k), Err(FontError::Graphics)));
        assert!(k.calls.borrow().is_empty());
    }

    #[test]
    fn a_font_wider_than_32_is_refused() {
        let k = Kernel::new(40, 16, 256);
        let e = read(&k).expect_err("refused");
        assert!(e.to_string().contains("KDFONTOP"), "{e}");
    }
}
