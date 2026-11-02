//! Kitty's raw pixel formats, `f=24` (R, G, B) and `f=32` (R, G, B, A
//! with straight alpha), converted into the store's premultiplied ARGB as
//! they arrive (ADR-1100, D7).
//!
//! A [`Raw`] takes the decoded bytes of a payload in any pieces and turns
//! each whole pixel into one `u32` as soon as its bytes are in, holding at
//! most one partial pixel between pieces. It wants exactly the pixels its
//! width and height call for: bytes past them are ignored, as
//! kitty ignores them (the C reference's black-box survey of kitty 0.49.2, kmscon's ADR 107).
//!
//! Premultiplying uses integers only: each channel becomes
//! `(channel × alpha + 127) / 255`, so every machine stores the same
//! pixels.

/// The premultiplied ARGB pixel of straight-alpha `r`, `g`, `b`, `a`.
pub(crate) fn premultiply(r: u8, g: u8, b: u8, a: u8) -> u32 {
    let a32 = u32::from(a);
    let scale = |c: u8| -> u32 {
        if a == 0xff {
            u32::from(c)
        } else {
            u32::from(c)
                .saturating_mul(a32)
                .saturating_add(127)
                .checked_div(255)
                .unwrap_or(0)
        }
    };
    a32.wrapping_shl(24) | scale(r).wrapping_shl(16) | scale(g).wrapping_shl(8) | scale(b)
}

/// The pixel of one group of `bpp` bytes: R, G, B, and A when `bpp` is 4.
fn pixel(bytes: &[u8]) -> u32 {
    match *bytes {
        [r, g, b, a] => premultiply(r, g, b, a),
        [r, g, b] => premultiply(r, g, b, 0xff),
        _ => 0,
    }
}

/// Whole pixels of `bpp` bytes each from `data` onto `out`, in one pass
/// that also finds whether every one of them is opaque.
fn convert_into(out: &mut Vec<u32>, data: &[u8], bpp: usize) -> bool {
    let mut opaque = true;
    out.extend(data.chunks_exact(bpp.max(1)).map(|bytes| {
        let px = pixel(bytes);
        opaque &= px.wrapping_shr(24) == 0xff;
        px
    }));
    opaque
}

/// The pixels of `data`, `bpp` bytes each, as one conversion into `out`,
/// emptied first, and whether every pixel is opaque: for bytes the image
/// helper read or inflated, and for a picture it decoded as R, G, B, A.
pub(crate) fn convert(data: &[u8], bpp: usize, mut out: Vec<u32>) -> (Vec<u32>, bool) {
    out.clear();
    out.reserve_exact(data.len().checked_div(bpp.max(1)).unwrap_or(0));
    let opaque = convert_into(&mut out, data, bpp);
    (out, opaque)
}

/// The pixels of one raw transmission, as they arrive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Raw {
    /// Bytes a pixel: 3 for `f=24`, 4 for `f=32`.
    bpp: u8,
    /// The pixels the width and height call for.
    want: usize,
    pixels: Vec<u32>,
    /// The bytes of a pixel not yet whole.
    part: [u8; 4],
    held: u8,
    /// Every pixel so far is opaque.
    opaque: bool,
}

impl Raw {
    /// A transmission of `count` pixels of `bpp` bytes each, into
    /// `buffer`, emptied first and grown to hold them now, once the budget
    /// has reserved them: a new buffer, or the spare one a freed image of
    /// the same size left, whose memory is already mapped.
    pub(crate) fn new(bpp: u8, count: usize, mut buffer: Vec<u32>) -> Self {
        buffer.clear();
        buffer.reserve_exact(count);
        Self {
            bpp,
            want: count,
            pixels: buffer,
            part: [0; 4],
            held: 0,
            opaque: true,
        }
    }

    /// Takes the next decoded bytes.
    pub(crate) fn push(&mut self, mut data: &[u8]) {
        let bpp = usize::from(self.bpp);
        // the rest of a pixel the previous piece began
        if self.held > 0 && !self.complete() {
            let need = bpp.saturating_sub(usize::from(self.held));
            let (head, rest) = data.split_at(need.min(data.len()));
            let at = usize::from(self.held);
            if let Some(slot) = self.part.get_mut(at..at.saturating_add(head.len())) {
                slot.copy_from_slice(head);
            }
            self.held = self
                .held
                .saturating_add(u8::try_from(head.len()).unwrap_or(u8::MAX));
            data = rest;
            if usize::from(self.held) == bpp {
                let px = pixel(self.part.get(..bpp).unwrap_or_default());
                self.opaque &= px.wrapping_shr(24) == 0xff;
                self.pixels.push(px);
                self.held = 0;
            }
        }
        let room = self
            .want
            .saturating_sub(self.pixels.len())
            .saturating_mul(bpp)
            .saturating_sub(usize::from(self.held));
        // bytes past the pixels wanted are ignored
        let taken = data.get(..room.min(data.len())).unwrap_or_default();
        let whole = taken
            .len()
            .checked_div(bpp)
            .unwrap_or(0)
            .saturating_mul(bpp);
        let (pixels, tail) = taken.split_at(whole);
        self.opaque &= convert_into(&mut self.pixels, pixels, bpp);
        if !tail.is_empty() {
            if let Some(slot) = self.part.get_mut(..tail.len()) {
                slot.copy_from_slice(tail);
            }
            self.held = u8::try_from(tail.len()).unwrap_or(0);
        }
    }

    /// Whether every pixel wanted has arrived.
    pub(crate) fn complete(&self) -> bool {
        self.pixels.len() >= self.want
    }

    /// The pixels, once complete, and whether every one is opaque.
    pub(crate) fn into_pixels(self) -> (Vec<u32>, bool) {
        (self.pixels, self.opaque)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_alpha_becomes_premultiplied_with_integer_rounding() {
        assert_eq!(premultiply(0xff, 0x80, 0x00, 0xff), 0xffff_8000);
        assert_eq!(premultiply(0xff, 0xff, 0xff, 0x80), 0x8080_8080);
        // (0x80 × 0x80 + 127) / 255 = 64.75, down to 0x40
        assert_eq!(premultiply(0x80, 0x80, 0x80, 0x80), 0x8040_4040);
        // 3 x 0x80 / 255 is 1.5: 2 rounded, where truncation gives 1
        assert_eq!(premultiply(3, 3, 3, 0x80), 0x8002_0202);
        assert_eq!(premultiply(0xff, 0xff, 0xff, 0), 0);
    }

    #[test]
    fn rgb_and_rgba_arrive_in_any_pieces() {
        let rgb: Vec<u8> = (0..30u8).collect();
        let (want, opaque) = convert(&rgb, 3, Vec::new());
        assert!(opaque);
        for cut in 0..rgb.len() {
            let mut raw = Raw::new(3, 10, Vec::new());
            let (a, b) = rgb.split_at(cut);
            raw.push(a);
            raw.push(b);
            assert!(raw.complete());
            assert_eq!(raw.into_pixels(), (want.clone(), true), "cut at {cut}");
        }
        let rgba: Vec<u8> = (0..40u8).map(|b| b.wrapping_mul(7)).collect();
        let mut raw = Raw::new(4, 10, Vec::new());
        for byte in &rgba {
            raw.push(core::slice::from_ref(byte));
        }
        assert_eq!(raw.into_pixels(), convert(&rgba, 4, Vec::new()));
    }

    #[test]
    fn bytes_past_the_pixels_wanted_are_ignored() {
        let mut raw = Raw::new(3, 2, Vec::new());
        raw.push(&[1, 2, 3, 4]);
        assert!(!raw.complete());
        raw.push(&[5, 6, 7, 8, 9]);
        assert!(raw.complete());
        assert_eq!(
            raw.into_pixels(),
            (
                vec![premultiply(1, 2, 3, 0xff), premultiply(4, 5, 6, 0xff)],
                true
            )
        );
    }

    #[test]
    #[expect(clippy::indexing_slicing, reason = "a test fails by panicking")]
    fn the_conversion_finds_a_pixel_below_full_alpha_wherever_it_falls() {
        let mut rgba = [0xffu8; 16];
        for at in 0..4 {
            let mut data = rgba;
            data[at * 4 + 3] = 0xfe;
            assert!(!convert(&data, 4, Vec::new()).1, "pixel {at}");
            // and cut inside that pixel, through the partial pixel's path
            let mut raw = Raw::new(4, 4, Vec::new());
            let (a, b) = data.split_at(at * 4 + 2);
            raw.push(a);
            raw.push(b);
            assert!(!raw.into_pixels().1, "pixel {at}, cut inside it");
        }
        rgba[3] = 0xff;
        assert!(convert(&rgba, 4, Vec::new()).1);
        // a buffer handed in is emptied and filled, its memory kept
        let spare = vec![7u32; 4];
        let ptr = spare.as_ptr();
        let (out, _) = convert(&rgba, 4, spare);
        assert_eq!((out.as_ptr(), out.len()), (ptr, 4));
        assert!(out.iter().all(|&px| px == 0xffff_ffff));
    }
}
