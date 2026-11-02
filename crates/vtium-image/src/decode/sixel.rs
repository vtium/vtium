//! The sixel decoder: a DCS `q` string's data, taken a byte at a time as
//! it arrives, drawn straight into premultiplied pixels (ADR-1100, D3 and
//! D7). The decoder never holds the string itself, only the picture it
//! has drawn so far, so a picture of tens of megabytes of text costs its
//! pixels and nothing more.
//!
//! The rules, from DEC's VT330/VT340 Programmer Reference Manual and
//! xterm's ctlseqs:
//!
//! - A sixel is a byte from `?` to `~`: its value less 63 holds six dots,
//!   bit 0 on top. Each sixel moves the graphics cursor one column right.
//!   `$` returns the cursor to the left edge of the band (graphics carriage
//!   return), and `-` moves it to the left edge of the next band, six dots
//!   down (graphics new line).
//! - `! Pn` repeats the sixel right after it Pn times; a repeat of 0 is 1.
//! - `# Pc` selects color register Pc, and `# Pc ; Pu ; Px ; Py ; Pz`
//!   defines it first: Pu 1 is HLS, hue in degrees with blue at 0, red at
//!   120 and green at 240, lightness and saturation in percent; Pu 2 is
//!   RGB in percent. A register past the count in force is the last one.
//! - `" Pan ; Pad ; Ph ; Pv`, raster attributes, before any sixel, give
//!   the picture's size Ph by Pv. Pan and Pad, the pixel aspect, are read
//!   and not applied: one sixel dot is one square pixel (D3), as the pixel
//!   aspect of the string's first parameter is not applied either.
//! - Every other byte is ignored, and one that ends a parameter list ends
//!   it before it is ignored.
//!
//! A dot takes its register's color when it is drawn, so a register
//! defined again later changes what is drawn after and nothing before:
//! encoders such as libsixel's high-color mode define their registers
//! again band after band. The picture is as wide as the furthest column a
//! sixel reached and as tall as one past the lowest dot set, both at least
//! the raster attributes' size, so a picture whose height is not a
//! multiple of six keeps its height. At the end, every pixel no dot set
//! takes register 0 as it stands then, unless the string's second
//! parameter is 1, which leaves those pixels transparent.
//!
//! Memory and work. The decoder asks its [`Budget`] before its pixel
//! memory changes size, and raster attributes reserve the picture's size
//! before any sixel, so a picture that cannot fit is refused before it is
//! decoded. A picture that outgrows its buffer grows each side by at least
//! a quarter, holding both buffers' bytes while the rows move across. A
//! few bytes can make the decoder write many pixels (`!10000~` writes
//! 60,000), so it counts every byte read and every pixel it writes,
//! allocates or copies as work, which the handler adds to the input call's
//! count (D5).

use std::fmt;

/// The fewest and most color registers a picture may have, and the count
/// when none is given (the C reference's `TSM_SIXEL_REGISTERS_*`).
pub(crate) const REGISTERS_MIN: u32 = 2;
pub(crate) const REGISTERS_MAX: u32 = 65_535;
pub(crate) const REGISTERS_DEFAULT: u32 = 1024;

/// A parameter stops growing here; every use clamps far below it.
const PARAM_CAP: u32 = 0x00ff_ffff / 10;
/// The parameters a color introducer or raster attributes read; more are
/// read and dropped.
const PARAMS: usize = 5;

/// The VT340's default color map, registers 0 to 15, in RGB percent
/// (VT330/VT340 Programmer Reference Manual, the sixel color map). The rest
/// start black.
const VT340: [[u32; 3]; 16] = [
    [0, 0, 0],
    [20, 20, 80],
    [80, 13, 13],
    [20, 80, 20],
    [80, 20, 80],
    [20, 80, 80],
    [80, 80, 20],
    [53, 53, 53],
    [26, 26, 26],
    [33, 33, 60],
    [60, 26, 26],
    [33, 60, 33],
    [60, 33, 60],
    [33, 60, 60],
    [60, 60, 33],
    [80, 80, 80],
];

/// What the decoder asks before its pixel memory changes size: the image
/// budget. `resize(old, new)` moves the bytes it holds from `old` to `new`;
/// a growth may be refused, and the picture is then dropped. A shrink is
/// always granted.
pub(crate) trait Budget {
    fn resize(&mut self, old: u64, new: u64) -> bool;
}

/// A set of color registers, each an opaque ARGB color.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Palette(Vec<u32>);

impl fmt::Debug for Palette {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Palette({} registers)", self.0.len())
    }
}

impl Palette {
    /// `count` registers, clamped to [`REGISTERS_MIN`] and
    /// [`REGISTERS_MAX`], 0 meaning [`REGISTERS_DEFAULT`]; registers 0 to
    /// 15 start from the VT340's color map and the rest black.
    pub(crate) fn new(count: u32) -> Self {
        let count = Self::clamped(count);
        let mut palette = Self(vec![0; usize::try_from(count).unwrap_or(0)]);
        palette.reset();
        palette
    }

    /// The count of registers a palette of `count` has.
    pub(crate) fn clamped(count: u32) -> u32 {
        if count == 0 {
            REGISTERS_DEFAULT
        } else {
            count.clamp(REGISTERS_MIN, REGISTERS_MAX)
        }
    }

    /// How many registers it has.
    pub(crate) fn count(&self) -> u32 {
        u32::try_from(self.0.len()).unwrap_or(REGISTERS_MAX)
    }

    /// Every register back to its default.
    pub(crate) fn reset(&mut self) {
        let mut defaults = VT340.iter();
        for slot in &mut self.0 {
            *slot = match defaults.next() {
                Some(&[r, g, b]) => rgb_percent(r, g, b),
                None => argb(0, 0, 0),
            };
        }
    }

    /// Register `n`, or the last register for a number past them.
    fn get(&self, n: u32) -> u32 {
        let last = self.0.len().saturating_sub(1);
        let i = usize::try_from(n).map_or(last, |i| i.min(last));
        self.0.get(i).copied().unwrap_or(0xff00_0000)
    }

    fn set(&mut self, n: u32, color: u32) {
        let last = self.0.len().saturating_sub(1);
        let i = usize::try_from(n).map_or(last, |i| i.min(last));
        if let Some(slot) = self.0.get_mut(i) {
            *slot = color;
        }
    }
}

fn argb(r: u32, g: u32, b: u32) -> u32 {
    0xff00_0000 | (r.min(255) << 16) | (g.min(255) << 8) | b.min(255)
}

/// A percentage, 0 to 100 and past it clamped, as an 8-bit channel,
/// rounded to the nearest.
fn percent_to_8bit(p: u32) -> u32 {
    p.min(100).saturating_mul(255).saturating_add(50) / 100
}

fn rgb_percent(r: u32, g: u32, b: u32) -> u32 {
    argb(percent_to_8bit(r), percent_to_8bit(g), percent_to_8bit(b))
}

/// HLS as DEC defines it: the hue in degrees with blue at 0, red at 120
/// and green at 240, so turning it by 240 degrees gives the usual HLS with
/// red at 0; lightness and saturation in percent. Each value past its
/// range is clamped to it. The arithmetic is exact, in integers, over one
/// denominator: a channel's value is `n / D` with `D` = 1,200,000, and its
/// 8-bit channel is that value times 255, rounded half up.
fn hls_percent(h: u32, l: u32, s: u32) -> u32 {
    const D: i64 = 1_200_000;
    let hue = i64::from(h.min(360).saturating_add(240) % 360);
    let l = i64::from(l.min(100));
    let s = i64::from(s.min(100));
    let (sector, within) = (hue / 60, hue % 60);
    // chroma, over 10,000: (1 - |2L - 1|) * S
    let chroma =
        (100_i64.saturating_sub((l.saturating_mul(2).saturating_sub(100)).abs())).saturating_mul(s);
    let c = chroma.saturating_mul(120);
    let x = chroma.saturating_mul(2).saturating_mul(if sector % 2 == 1 {
        60_i64.saturating_sub(within)
    } else {
        within
    });
    // the lightness less half the chroma, never below 0
    let m = l
        .saturating_mul(200)
        .saturating_sub(chroma)
        .saturating_mul(60);
    let (r, g, b) = match sector {
        0 => (c, x, 0),
        1 => (x, c, 0),
        2 => (0, c, x),
        3 => (0, x, c),
        4 => (x, 0, c),
        _ => (c, 0, x),
    };
    let channel = |v: i64| -> u32 {
        let n = v.saturating_add(m);
        if n <= 0 {
            0
        } else if n >= D {
            255
        } else {
            let q = n.saturating_mul(510).saturating_add(D) / (2 * D);
            u32::try_from(q).unwrap_or(255)
        }
    };
    argb(channel(r), channel(g), channel(b))
}

/// Where the decoder is in the string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// No picture in progress.
    Idle,
    /// Sixels and the controls of one byte.
    Data,
    /// `! Pn`.
    Repeat,
    /// `# Pc ; Pu ; Px ; Py ; Pz`.
    Color,
    /// `" Pan ; Pad ; Ph ; Pv`.
    Raster,
    /// The budget refused the picture's memory: the rest is ignored.
    Failed,
}

/// Why a picture ended without pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Empty {
    /// No picture was in progress.
    Idle,
    /// The picture had no size.
    NoData,
    /// The budget refused its memory.
    Refused,
}

/// A finished picture: premultiplied ARGB, rows top to bottom. The
/// decoder's budget holds at least its bytes.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Picture {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u32>,
    /// The string's second parameter was 1: pixels no dot set are
    /// transparent.
    pub(crate) transparent: bool,
}

/// The decoder of one picture at a time.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Decoder {
    /// `max-graphic-size`, each side.
    max_w: u32,
    max_h: u32,
    /// The registers of the picture in progress.
    palette: Palette,

    mode: Mode,
    params: [u32; PARAMS],
    /// How many parameters have begun; past [`PARAMS`] the rest are
    /// dropped.
    nparams: usize,

    transparent: bool,
    /// Raster attributes count only before the first sixel.
    data_seen: bool,
    ra_w: u32,
    ra_h: u32,

    /// The color of the register selected.
    color: u32,
    /// The graphics cursor: its column, at most `max_w`, and the top row
    /// of its band, at most `max_h`.
    x: u32,
    y: u32,
    /// The furthest column a sixel reached, and one past the lowest row a
    /// dot set.
    width: u32,
    height: u32,

    /// The picture so far, `cap_w` pixels a row, 0 meaning no dot set
    /// there; empty until the first dot.
    buf: Vec<u32>,
    cap_w: u32,
    cap_h: u32,
    /// The bytes of the budget the decoder holds.
    reserved: u64,

    /// Bytes read and pixels written, allocated or copied, ever.
    work: u64,
}

impl fmt::Debug for Decoder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Decoder")
            .field("mode", &self.mode)
            .field("size", &(self.width, self.height))
            .field("buffer", &(self.cap_w, self.cap_h))
            .field("reserved", &self.reserved)
            .field("work", &self.work)
            .finish_non_exhaustive()
    }
}

/// Whether `c` is a sixel, which holds six dots.
fn is_sixel(c: u8) -> bool {
    (b'?'..=b'~').contains(&c)
}

/// One past the highest row `bits` sets, as a count of rows from the top.
fn rows_of(bits: u32) -> u32 {
    32_u32.saturating_sub(bits.leading_zeros())
}

fn index(row: u32, stride: u32, col: u32) -> usize {
    let i = u64::from(row)
        .saturating_mul(u64::from(stride))
        .saturating_add(u64::from(col));
    usize::try_from(i).unwrap_or(usize::MAX)
}

impl Decoder {
    /// A decoder for one picture, with the largest picture `max` pixels
    /// each side, and its registers `palette`: the registers the pictures
    /// share (mode 1070 reset), or registers of its own at their defaults.
    /// `transparent` is the string's second parameter being 1.
    pub(crate) fn new(max: (u32, u32), palette: Palette, transparent: bool) -> Self {
        let color = palette.get(0);
        Self {
            max_w: max.0,
            max_h: max.1,
            palette,
            mode: Mode::Data,
            params: [0; PARAMS],
            nparams: 0,
            transparent,
            data_seen: false,
            ra_w: 0,
            ra_h: 0,
            color,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            buf: Vec::new(),
            cap_w: 0,
            cap_h: 0,
            reserved: 0,
            work: 0,
        }
    }

    /// Takes the registers back, once the picture is done or dropped.
    pub(crate) fn take_palette(&mut self) -> Palette {
        core::mem::replace(&mut self.palette, Palette(Vec::new()))
    }

    /// The work done so far: bytes read, and pixels written, allocated or
    /// copied. It only grows.
    pub(crate) fn work(&self) -> u64 {
        self.work
    }

    /// The bytes of the budget the decoder holds.
    #[cfg(test)]
    pub(crate) fn reserved(&self) -> u64 {
        self.reserved
    }

    /// Asks the budget to move what the decoder holds to `size` bytes.
    fn reserve(&mut self, size: u64, budget: &mut dyn Budget) -> bool {
        if !budget.resize(self.reserved, size) && size > self.reserved {
            return false;
        }
        self.reserved = size;
        true
    }

    /// Frees the picture and gives its bytes back.
    fn release(&mut self, budget: &mut dyn Budget) {
        self.buf = Vec::new();
        self.cap_w = 0;
        self.cap_h = 0;
        if self.reserved > 0 {
            budget.resize(self.reserved, 0);
        }
        self.reserved = 0;
    }

    fn fail(&mut self, budget: &mut dyn Budget) {
        self.release(budget);
        self.mode = Mode::Failed;
    }

    /// Gives back what the decoder holds, as a cancelled string does.
    pub(crate) fn abort(&mut self, budget: &mut dyn Budget) {
        self.release(budget);
        self.mode = Mode::Idle;
    }

    /// Moves the picture into a buffer of `w` by `h` pixels, each at least
    /// the current one, keeping what is drawn. Both buffers are held of the
    /// budget while the rows move across, then only the new one.
    fn resize(&mut self, w: u32, h: u32, budget: &mut dyn Budget) -> bool {
        let pixels = u64::from(w).saturating_mul(u64::from(h));
        let size = pixels.saturating_mul(4);
        let old = u64::from(self.cap_w)
            .saturating_mul(u64::from(self.cap_h))
            .saturating_mul(4);
        let both = old.saturating_add(size);
        if both > self.reserved && !self.reserve(both, budget) {
            return false;
        }
        let Ok(n) = usize::try_from(pixels) else {
            return false;
        };
        let mut buf = vec![0_u32; n];
        // rows at and below the height hold nothing yet
        let (old_w, new_w) = (
            usize::try_from(self.cap_w).unwrap_or(0),
            usize::try_from(w).unwrap_or(0),
        );
        if old_w > 0 && new_w > 0 {
            let rows = usize::try_from(self.height).unwrap_or(0);
            for (dst, src) in buf
                .chunks_exact_mut(new_w)
                .zip(self.buf.chunks_exact(old_w))
                .take(rows)
            {
                if let (Some(dst), Some(src)) = (dst.get_mut(..old_w), src.get(..old_w)) {
                    dst.copy_from_slice(src);
                }
            }
        }
        self.work = self
            .work
            .saturating_add(pixels)
            .saturating_add(u64::from(self.height).saturating_mul(u64::from(self.cap_w)));
        self.buf = buf;
        self.cap_w = w;
        self.cap_h = h;
        self.reserve(size, budget);
        true
    }

    /// Grows the picture to hold the columns below `need_w` and the rows
    /// below `need_h`. A side that grows grows by at least a quarter, so a
    /// picture that outgrows its raster attributes, or has none, is copied
    /// a number of times that grows with the logarithm of its size.
    fn grow(&mut self, need_w: u32, need_h: u32, budget: &mut dyn Budget) -> bool {
        // the first buffer is at least the raster area
        let (mut w, mut h) = if self.buf.is_empty() {
            (self.ra_w, self.ra_h)
        } else {
            (self.cap_w, self.cap_h)
        };
        if need_w > w {
            w = need_w.max(w.saturating_add(w / 4).min(self.max_w));
        }
        if need_h > h {
            h = need_h.max(h.saturating_add(h / 4).min(self.max_h));
        }
        self.resize(w.max(need_w), h.max(need_h), budget)
    }

    /// Draws the dots of `bits` `count` times at the graphics cursor in the
    /// color selected, and moves the cursor past them.
    fn put(&mut self, bits: u32, count: u32, budget: &mut dyn Budget) {
        self.data_seen = true;
        if self.y >= self.max_h {
            return;
        }
        let x0 = self.x;
        let x1 = x0.saturating_add(count.min(self.max_w)).min(self.max_w);
        self.x = x1;
        self.width = self.width.max(x1);
        if bits == 0 || x0 >= x1 {
            return;
        }
        let end = self.y.saturating_add(rows_of(bits)).min(self.max_h);
        if (x1 > self.cap_w || end > self.cap_h) && !self.grow(x1, end, budget) {
            self.fail(budget);
            return;
        }
        self.height = self.height.max(end);
        let (color, stride) = (self.color, self.cap_w);
        let run = usize::try_from(x1.saturating_sub(x0)).unwrap_or(0);
        for bit in 0..6_u32 {
            if bits & (1 << bit) == 0 {
                continue;
            }
            let row = self.y.saturating_add(bit);
            if row >= end {
                break;
            }
            let start = index(row, stride, x0);
            if let Some(line) = self.buf.get_mut(start..start.saturating_add(run)) {
                line.fill(color);
            }
            self.work = self.work.saturating_add(u64::from(x1.saturating_sub(x0)));
        }
    }

    /// [`put`](Self::put) of one sixel that lies inside the buffer, the
    /// bulk of every encoder's output: returns false, having done nothing,
    /// for `put` to draw it.
    fn put_one(&mut self, bits: u32) -> bool {
        let (x, y, stride) = (self.x, self.y, self.cap_w);
        if x >= stride || y.saturating_add(6) > self.cap_h {
            return false;
        }
        self.data_seen = true;
        self.x = x.saturating_add(1);
        self.width = self.width.max(self.x);
        if bits == 0 {
            return true;
        }
        self.height = self.height.max(y.saturating_add(rows_of(bits)));
        self.work = self.work.saturating_add(6);
        let color = self.color;
        let step = usize::try_from(stride).unwrap_or(0);
        let mut at = index(y, stride, x);
        // all six rows, masked: cheaper than a branch for each dot
        for bit in 0..6_u32 {
            let mask = 0_u32.wrapping_sub(bits.wrapping_shr(bit) & 1);
            if let Some(p) = self.buf.get_mut(at) {
                *p = (*p & !mask) | (color & mask);
            }
            at = at.saturating_add(step);
        }
        true
    }

    /// Acts on the parameters `mode` collected, which a byte just ended.
    fn end_params(&mut self, mode: Mode, budget: &mut dyn Budget) {
        let [p0, p1, p2, p3, p4] = self.params;
        let n = self.nparams;
        match mode {
            Mode::Color => {
                if n >= 5 && p1 == 1 {
                    self.palette.set(p0, hls_percent(p2, p3, p4));
                } else if n >= 5 && p1 == 2 {
                    self.palette.set(p0, rgb_percent(p2, p3, p4));
                }
                self.color = self.palette.get(p0);
            }
            Mode::Raster if !self.data_seen => {
                // Pan and Pad are read and not applied (D3)
                self.ra_w = if n >= 3 { p2.min(self.max_w) } else { 0 };
                self.ra_h = if n >= 4 { p3.min(self.max_h) } else { 0 };
                // reserved now, so a picture that cannot fit is refused
                // before it is decoded; the first dot allocates it
                self.release(budget);
                let area = u64::from(self.ra_w)
                    .saturating_mul(u64::from(self.ra_h))
                    .saturating_mul(4);
                if area > 0 && !self.reserve(area, budget) {
                    self.fail(budget);
                }
            }
            _ => {}
        }
    }

    fn start_params(&mut self, mode: Mode) {
        self.params = [0; PARAMS];
        self.nparams = 1;
        self.mode = mode;
    }

    /// A byte in [`Mode::Data`].
    fn data(&mut self, c: u8, budget: &mut dyn Budget) {
        if (b'?'..=b'~').contains(&c) {
            let bits = u32::from(c.saturating_sub(b'?'));
            if !self.put_one(bits) {
                self.put(bits, 1, budget);
            }
            return;
        }
        match c {
            b'!' => self.start_params(Mode::Repeat),
            b'#' => self.start_params(Mode::Color),
            b'"' => self.start_params(Mode::Raster),
            b'$' => self.x = 0,
            b'-' => {
                self.x = 0;
                self.y = self.y.saturating_add(6).min(self.max_h);
            }
            // every other byte is ignored
            _ => {}
        }
    }

    fn step(&mut self, c: u8, budget: &mut dyn Budget) {
        let mode = self.mode;
        match mode {
            Mode::Idle | Mode::Failed => return,
            Mode::Data => return self.data(c, budget),
            Mode::Repeat | Mode::Color | Mode::Raster => {}
        }
        if c.is_ascii_digit() {
            // past the parameters it reads, digits are dropped
            let i = self.nparams.saturating_sub(1);
            if let (true, Some(p)) = (self.nparams <= PARAMS, self.params.get_mut(i)) {
                if *p < PARAM_CAP {
                    *p = p
                        .saturating_mul(10)
                        .saturating_add(u32::from(c.saturating_sub(b'0')));
                }
            }
            return;
        }
        if c == b';' && mode != Mode::Repeat {
            if self.nparams <= PARAMS {
                self.nparams = self.nparams.saturating_add(1);
            }
            return;
        }
        self.mode = Mode::Data;
        if mode == Mode::Repeat {
            // a repeat applies to the sixel right after it, and anything
            // else ends it
            if (b'?'..=b'~').contains(&c) {
                let count = self.params[0].max(1);
                self.put(u32::from(c.saturating_sub(b'?')), count, budget);
            } else {
                self.data(c, budget);
            }
            return;
        }
        self.end_params(mode, budget);
        // raster attributes the budget refused end the picture
        if self.mode != Mode::Failed {
            self.data(c, budget);
        }
    }

    /// Feeds the string's data.
    pub(crate) fn feed(&mut self, bytes: &[u8], budget: &mut dyn Budget) {
        let mut rest = bytes;
        while let Some((&c, after)) = rest.split_first() {
            if self.mode == Mode::Data && is_sixel(c) {
                let n = rest.iter().take_while(|&&b| is_sixel(b)).count();
                let (run, tail) = rest.split_at(n);
                self.work = self
                    .work
                    .saturating_add(u64::try_from(n).unwrap_or(u64::MAX));
                let done = self.put_run(run);
                for &c in run.get(done..).unwrap_or_default() {
                    self.data(c, budget);
                }
                rest = tail;
                continue;
            }
            self.work = self.work.saturating_add(1);
            self.step(c, budget);
            rest = after;
        }
    }

    /// Draws a run of sixels, each one column, at the graphics cursor, as
    /// far as the buffer holds the band, and returns how many it drew: the
    /// bulk of every encoder's output. Each sixel writes only the dots it
    /// sets, so the cost follows the dots drawn: an encoder paints a column
    /// once for each color in it, and each pass sets few of its six dots.
    /// The rest of the run is for [`put`](Self::put) to grow the picture.
    fn put_run(&mut self, run: &[u8]) -> usize {
        let (x, y, stride) = (self.x, self.y, self.cap_w);
        if x >= stride || y.saturating_add(6) > self.cap_h {
            return 0;
        }
        let room = usize::try_from(stride.saturating_sub(x)).unwrap_or(0);
        let n = run.len().min(room);
        let step = usize::try_from(stride).unwrap_or(0);
        let start = index(y, stride, x);
        // the band from the cursor's column on its top row to the end of
        // its sixth row
        let end = index(y.saturating_add(6), stride, 0);
        let Some(band) = self.buf.get_mut(start..end) else {
            return 0;
        };
        let color = self.color;
        let (mut any, mut set) = (0_u32, 0_u64);
        for (col, &c) in run.iter().take(n).enumerate() {
            let mut bits = u32::from(c.wrapping_sub(b'?'));
            any |= bits;
            set = set.saturating_add(u64::from(bits.count_ones()));
            while bits != 0 {
                let row = usize::try_from(bits.trailing_zeros()).unwrap_or(0);
                if let Some(p) = band.get_mut(row.saturating_mul(step).saturating_add(col)) {
                    *p = color;
                }
                bits &= bits.wrapping_sub(1);
            }
        }
        self.data_seen = true;
        self.x = x.saturating_add(u32::try_from(n).unwrap_or(0));
        self.width = self.width.max(self.x);
        if any != 0 {
            self.height = self.height.max(y.saturating_add(rows_of(any)));
        }
        self.work = self.work.saturating_add(set);
        n
    }

    /// Ends the picture, as ST does, and returns it, holding exactly its
    /// bytes of the budget, or why there is none, having given back what it
    /// held. The decoder is idle afterwards.
    pub(crate) fn finish(&mut self, budget: &mut dyn Budget) -> Result<Picture, Empty> {
        if matches!(self.mode, Mode::Repeat | Mode::Color | Mode::Raster) {
            // parameters the end of the string ends still act
            self.step(0, budget);
        }
        let mode = core::mem::replace(&mut self.mode, Mode::Idle);
        match mode {
            Mode::Idle => return Err(Empty::Idle),
            Mode::Failed => return Err(Empty::Refused),
            _ => {}
        }
        let w = self.width.max(self.ra_w);
        let h = self.height.max(self.ra_h);
        if w == 0 || h == 0 {
            self.release(budget);
            return Err(Empty::NoData);
        }
        // columns of `?` past the last dot, and raster attributes larger
        // than what was drawn, widen the picture at the end
        if (w > self.cap_w || h > self.cap_h)
            && !self.resize(w.max(self.cap_w), h.max(self.cap_h), budget)
        {
            self.release(budget);
            return Err(Empty::Refused);
        }
        let n = u64::from(w).saturating_mul(u64::from(h));
        self.work = self.work.saturating_add(n);
        let mut px = core::mem::take(&mut self.buf);
        let (cap_w, wide) = (
            usize::try_from(self.cap_w).unwrap_or(0),
            usize::try_from(w).unwrap_or(0),
        );
        let rows = usize::try_from(h).unwrap_or(0);
        if cap_w != wide {
            for row in 1..rows {
                let from = row.saturating_mul(cap_w);
                px.copy_within(from..from.saturating_add(wide), row.saturating_mul(wide));
            }
        }
        px.truncate(wide.saturating_mul(rows));
        px.shrink_to_fit();
        if !self.transparent {
            let bg = self.palette.get(0);
            for p in px.iter_mut().filter(|p| **p == 0) {
                *p = bg;
            }
        }
        // the pixels now hold exactly their own bytes
        let size = n.saturating_mul(4);
        if size < self.reserved {
            self.reserve(size, budget);
        }
        self.cap_w = 0;
        self.cap_h = 0;
        Ok(Picture {
            width: w,
            height: h,
            pixels: px,
            transparent: self.transparent,
        })
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::expect_used,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    /// A budget that grants up to `limit` bytes and records what it
    /// holds.
    struct Bank {
        limit: u64,
        held: u64,
        peak: u64,
    }

    impl Bank {
        fn new(limit: u64) -> Self {
            Self {
                limit,
                held: 0,
                peak: 0,
            }
        }
    }

    impl Budget for Bank {
        fn resize(&mut self, old: u64, new: u64) -> bool {
            assert_eq!(old, self.held, "the decoder knows what it holds");
            if new > old && new > self.limit {
                return false;
            }
            self.held = new;
            self.peak = self.peak.max(new);
            true
        }
    }

    fn decode_with(bytes: &[u8], transparent: bool, bank: &mut Bank) -> Result<Picture, Empty> {
        let mut d = Decoder::new((10_000, 10_000), Palette::new(256), transparent);
        d.feed(bytes, bank);
        d.finish(bank)
    }

    fn decode(bytes: &[u8]) -> Picture {
        let mut bank = Bank::new(u64::MAX);
        let p = decode_with(bytes, false, &mut bank).expect("a picture");
        assert_eq!(bank.held, u64::from(p.width * p.height * 4));
        p
    }

    const BLUE: u32 = 0xff33_33cc; // VT340 register 1: 20% 20% 80%
    const RED: u32 = 0xffcc_2121; // register 2: 80% 13% 13%

    // covers: image.sixel
    #[test]
    fn a_sixel_draws_six_dots_bit_0_on_top_in_its_registers_color() {
        // `#1` selects blue; `A` is 63 + 2, the second dot only; `~` all six
        let p = decode(b"#1A~");
        assert_eq!((p.width, p.height), (2, 6));
        let black = 0xff00_0000; // register 0 fills the dots no sixel set
        let col = |x: usize| (0..6).map(|y| p.pixels[y * 2 + x]).collect::<Vec<_>>();
        assert_eq!(col(0), [black, BLUE, black, black, black, black]);
        assert_eq!(col(1), [BLUE; 6]);
    }

    // covers: image.sixel
    #[test]
    fn a_picture_ends_at_its_lowest_dot_and_its_furthest_column() {
        // `@` sets the top dot only; the `?` after it widens the picture
        // with an empty column; `-` starts the next band at row 6
        let p = decode(b"#2@?-#2@");
        assert_eq!((p.width, p.height), (2, 7));
        assert_eq!(p.pixels[0], RED);
        assert_eq!(p.pixels[6 * 2], RED);
        assert_eq!(p.pixels[1], 0xff00_0000);
    }

    // covers: image.sixel
    #[test]
    fn a_repeat_draws_its_sixel_that_many_times_and_0_once() {
        let p = decode(b"#1!5~-!0~");
        assert_eq!((p.width, p.height), (5, 12));
        assert!(p.pixels[..30].iter().all(|&c| c == BLUE));
        assert_eq!(p.pixels[30], BLUE);
        assert_eq!(p.pixels[31], 0xff00_0000);
        // a repeat ended by something other than a sixel draws nothing
        let p = decode(b"#1!5$~");
        assert_eq!(p.width, 1);
    }

    // covers: image.sixel
    #[test]
    fn carriage_return_overprints_the_band_in_another_color() {
        // blue over the top three dots, then red over the bottom three
        let p = decode(b"#1F$#2w");
        let col: Vec<u32> = (0..6).map(|y| p.pixels[y]).collect();
        assert_eq!(col, [BLUE, BLUE, BLUE, RED, RED, RED]);
    }

    // covers: image.sixel
    #[test]
    fn a_register_defined_again_changes_only_what_is_drawn_after() {
        let p = decode(b"#5;2;100;0;0~#5;2;0;100;0~");
        assert_eq!(p.pixels[0], 0xffff_0000);
        assert_eq!(p.pixels[1], 0xff00_ff00);
    }

    // covers: image.sixel
    #[test]
    fn rgb_and_hls_define_registers_and_a_number_past_them_is_the_last() {
        let one = |s: &[u8]| decode(s).pixels[0];
        assert_eq!(one(b"#3;2;100;50;0~"), 0xffff_8000);
        // DEC's hues: blue at 0, red at 120, green at 240
        assert_eq!(one(b"#3;1;0;50;100~"), 0xff00_00ff);
        assert_eq!(one(b"#3;1;120;50;100~"), 0xffff_0000);
        assert_eq!(one(b"#3;1;240;50;100~"), 0xff00_ff00);
        // lightness 100 is white whatever the hue
        assert_eq!(one(b"#3;1;77;100;100~"), 0xffff_ffff);
        // values past their range are clamped
        assert_eq!(one(b"#3;2;900;0;0~"), 0xffff_0000);
        // register 9999 of 256 is register 255
        let p = decode(b"#9999;2;0;0;100#255~");
        assert_eq!(p.pixels[0], 0xff00_00ff);
        // a color introducer with too few parameters only selects
        assert_eq!(one(b"#2;2;100~"), RED);
    }

    /// The C reference's HLS, in floating point, as its sixel decoder
    /// computed it.
    fn hls_float(h: u32, l: u32, s: u32) -> u32 {
        let unit = |v: f64| {
            if v <= 0.0 {
                0
            } else if v >= 1.0 {
                255
            } else {
                (v * 255.0 + 0.5) as u32
            }
        };
        let (h, l, s) = (h.min(360), l.min(100), s.min(100));
        let hue = f64::from((h + 240) % 360) / 60.0;
        let light = f64::from(l) / 100.0;
        let sat = f64::from(s) / 100.0;
        let c =
            (1.0 - if light > 0.5 {
                2.0 * light - 1.0
            } else {
                1.0 - 2.0 * light
            }) * sat;
        let sector = hue as u32;
        let f = hue - f64::from(sector);
        let x = c * if sector & 1 == 1 { 1.0 - f } else { f };
        let m = light - c / 2.0;
        let (r, g, b) = match sector {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        argb(unit(r + m), unit(g + m), unit(b + m))
    }

    // Every HLS color, by integers, equals the C reference's floating
    // point but where the exact value of a channel lies halfway between
    // two levels: there the integers round up, where the floating point
    // went either way by how the decimal fractions were stored. The exact
    // halves are counted from the integers' own remainder.
    // covers: image.sixel
    #[test]
    fn hls_in_integers_is_the_c_references_but_at_exact_halves() {
        let channels = |c: u32| [(c >> 16) & 0xff, (c >> 8) & 0xff, c & 0xff];
        let (mut differ, mut halves) = (0, 0);
        for h in 0..=360 {
            for l in 0..=100 {
                for s in 0..=100 {
                    let (ours, theirs) = (hls_percent(h, l, s), hls_float(h, l, s));
                    if ours == theirs {
                        continue;
                    }
                    differ += 1;
                    for (a, b) in channels(ours).into_iter().zip(channels(theirs)) {
                        assert!(
                            a == b || a == b + 1,
                            "HLS {h};{l};{s}: {ours:08x} against {theirs:08x}"
                        );
                        // a channel that differs is an exact half: its value
                        // times 255, plus a half, is a whole number
                        if a != b {
                            assert!(exact_half(h, l, s, a), "HLS {h};{l};{s}");
                            halves += 1;
                        }
                    }
                }
            }
        }
        // the count pins the rule: it moves if the rounding does
        assert_eq!((differ, halves), (2747, 2805));
    }

    /// Whether a channel of HLS `h`, `l`, `s` that comes out as `level`
    /// lies exactly halfway below it, by exact fractions.
    fn exact_half(h: u32, l: u32, s: u32, level: u32) -> bool {
        let hue = i64::from((h + 240) % 360);
        let (l, s) = (i64::from(l), i64::from(s));
        let chroma = (100 - (2 * l - 100).abs()) * s;
        let (sector, within) = (hue / 60, hue % 60);
        let f = if sector % 2 == 1 { 60 - within } else { within };
        let m = (200 * l - chroma) * 60;
        let values = [chroma * 120 + m, chroma * 2 * f + m, m];
        values.iter().any(|&n| {
            let twice = n * 510 + 1_200_000;
            twice % 2_400_000 == 0 && twice / 2_400_000 == i64::from(level)
        })
    }

    // covers: image.sixel
    #[test]
    fn the_second_parameter_1_leaves_undrawn_pixels_clear() {
        let mut bank = Bank::new(u64::MAX);
        let p = decode_with(b"#1A", true, &mut bank).expect("a picture");
        assert!(p.transparent);
        assert_eq!(p.pixels, [0, BLUE]);
        // otherwise register 0 as it stands at the end fills them
        let p = decode(b"#1A#0;2;100;100;100");
        assert_eq!(p.pixels, [0xffff_ffff, BLUE]);
    }

    // covers: image.sixel
    #[test]
    fn raster_attributes_size_the_picture_and_reserve_it_first() {
        let p = decode(b"\"1;1;5;9#1@");
        assert_eq!((p.width, p.height), (5, 9));
        // Pan and Pad do not stretch a dot: one dot is one pixel
        let p = decode(b"\"2;1;1;1#1@");
        assert_eq!((p.width, p.height), (1, 1));
        // after the first sixel they count for nothing
        let p = decode(b"#1@\"1;1;5;9");
        assert_eq!((p.width, p.height), (1, 1));
        // a picture the budget cannot hold is refused before any sixel
        let mut bank = Bank::new(5 * 9 * 4 - 1);
        assert_eq!(
            decode_with(b"\"1;1;5;9#1@", false, &mut bank),
            Err(Empty::Refused)
        );
        assert_eq!(bank.held, 0);
        assert_eq!(bank.peak, 0, "nothing was allocated");
    }

    // A picture its raster attributes sized draws its sixels into the
    // buffer they made, each dot in its own row, through the run of sixels
    // the decoder draws without growing.
    // covers: image.sixel
    #[test]
    fn a_picture_sized_first_draws_each_dot_in_its_row() {
        let p = decode(b"\"1;1;4;12#1~@w_-#2Ao");
        assert_eq!((p.width, p.height), (4, 12));
        let black = 0xff00_0000;
        let col = |x: usize| (0..12).map(|y| p.pixels[y * 4 + x]).collect::<Vec<_>>();
        let (b, r, k) = (BLUE, RED, black);
        // `~` all six, `@` the top, `w` the bottom three, `_` the sixth;
        // in the next band `A` the second and `o` the fifth and sixth
        assert_eq!(col(0), [b, b, b, b, b, b, k, r, k, k, k, k]);
        assert_eq!(col(1), [b, k, k, k, k, k, k, k, k, k, r, r]);
        assert_eq!(col(2), [k, k, k, b, b, b, k, k, k, k, k, k]);
        assert_eq!(col(3), [k, k, k, k, k, b, k, k, k, k, k, k]);
    }

    // covers: image.sixel
    #[test]
    fn raster_attributes_reserve_the_picture_before_any_sixel() {
        let mut bank = Bank::new(u64::MAX);
        let mut d = Decoder::new((10_000, 10_000), Palette::new(16), false);
        d.feed(b"\"1;1;5;9#", &mut bank);
        assert_eq!(bank.held, 5 * 9 * 4);
        d.abort(&mut bank);
        assert_eq!(bank.held, 0);
    }

    // covers: image.sixel
    #[test]
    fn a_picture_past_the_budget_is_refused_and_gives_its_bytes_back() {
        let mut bank = Bank::new(4096);
        let r = decode_with(&[b'~'; 2000], false, &mut bank);
        assert_eq!(r, Err(Empty::Refused));
        assert_eq!(bank.held, 0);
    }

    // covers: image.sixel
    #[test]
    fn the_largest_picture_cuts_what_lies_past_it() {
        let mut bank = Bank::new(u64::MAX);
        let mut d = Decoder::new((4, 7), Palette::new(16), false);
        d.feed(b"#1!100~-~-~-~", &mut bank);
        let p = d.finish(&mut bank).expect("a picture");
        assert_eq!((p.width, p.height), (4, 7));
    }

    // covers: image.sixel
    #[test]
    fn a_string_with_no_dot_and_no_size_has_no_picture() {
        let mut bank = Bank::new(u64::MAX);
        assert_eq!(
            decode_with(b"#1;2;1;2;3$-", false, &mut bank),
            Err(Empty::NoData)
        );
        // a size alone is a picture of register 0
        let p = decode(b"\"1;1;2;1");
        assert_eq!(p.pixels, [0xff00_0000; 2]);
    }

    // covers: image.sixel
    #[test]
    fn the_work_counts_the_pixels_a_short_string_writes() {
        let mut bank = Bank::new(u64::MAX);
        let mut d = Decoder::new((10_000, 10_000), Palette::new(16), false);
        d.feed(b"!10000~", &mut bank);
        assert!(d.work() >= 60_000, "{}", d.work());
    }

    // covers: image.sixel
    #[test]
    fn a_picture_fed_in_pieces_is_the_picture_fed_whole() {
        let s: &[u8] = b"\"1;1;9;13#1;2;10;20;30#1~~@@!7N$#2??~~-#3;1;300;40;60!3}--#4~";
        let whole = decode(s);
        for cut in 0..s.len() {
            let mut bank = Bank::new(u64::MAX);
            let mut d = Decoder::new((10_000, 10_000), Palette::new(256), false);
            d.feed(&s[..cut], &mut bank);
            d.feed(&s[cut..], &mut bank);
            assert_eq!(
                d.finish(&mut bank).expect("a picture"),
                whole,
                "cut at {cut}"
            );
        }
    }

    // covers: image.sixel
    #[test]
    fn a_picture_that_grows_holds_both_buffers_then_its_own() {
        let mut bank = Bank::new(u64::MAX);
        let mut d = Decoder::new((10_000, 10_000), Palette::new(16), false);
        d.feed(&[b'~'; 300], &mut bank);
        assert_eq!(bank.held, d.reserved());
        let p = d.finish(&mut bank).expect("a picture");
        assert_eq!(bank.held, 300 * 6 * 4);
        assert!(bank.peak > bank.held);
        assert_eq!(p.pixels.len(), 300 * 6);
    }
}
