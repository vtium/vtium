//! The picture decoders (ADR-1101 D6, ADR-1103): PNG through `png`, JPEG
//! through `zune-jpeg` and GIF through `gif`, its first frame or every
//! frame composed whole, each pure Rust and linked into this program
//! alone.
//!
//! Each decoder reads the picture's header first, and refuses a width or
//! height past the largest the request allows as `TOO_LARGE`, and a
//! picture whose width × height × 4 bytes pass `max_bytes` as
//! `OVER_BUDGET`, before it allocates a pixel. It decodes to 8-bit RGBA
//! with no gamma or color-profile correction, as the C reference did, and
//! premultiplies the result through vtium-image's one conversion, so the
//! picture comes back in the store's format. A file that does not decode
//! is `CORRUPT`. Each takes the worker's proof that it has locked itself
//! in ([`Locked`]).

use std::io::{Cursor, Seek};

use vtium_image::decode::convert;
use vtium_image::decode::wire::Status;

use crate::bounds::PNG_WORK_MAX;
use crate::job::Failure;
use crate::sandbox::Locked;

/// A decoded picture: premultiplied R, G, B, A bytes, exactly width ×
/// height × 4 of them, and how many bytes of the file went unused.
#[derive(Debug, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub rest: u64,
}

/// The bounds a picture's header is checked against.
#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub largest: (u32, u32),
    pub max_bytes: u64,
}

impl Bounds {
    /// The bytes of a `width` by `height` picture, or why it is refused.
    pub fn check(&self, width: u32, height: u32) -> Result<usize, Failure> {
        if width == 0 || height == 0 {
            return Err(Failure::new(Status::Corrupt, "a picture with no pixels"));
        }
        if width > self.largest.0 || height > self.largest.1 {
            return Err(Failure::new(
                Status::TooLarge,
                format!("{width}x{height} past max-graphic-size"),
            ));
        }
        u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|n| n.checked_mul(4))
            .filter(|&n| n <= self.max_bytes)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| {
                Failure::new(
                    Status::OverBudget,
                    format!("{width}x{height} past the budget"),
                )
            })
    }
}

/// A buffer of `len` zero bytes, or `OVER_BUDGET` when memory refuses.
fn zeroed(len: usize) -> Result<Vec<u8>, Failure> {
    let mut v = Vec::new();
    v.try_reserve_exact(len)
        .map_err(|_| Failure::new(Status::OverBudget, format!("cannot allocate {len} bytes")))?;
    v.resize(len, 0);
    Ok(v)
}

/// The PNG signature.
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Which decoder a file's first bytes call for, for a request with the
/// any-image flag: PNG, JPEG's start-of-image marker, or a GIF.
pub fn any(data: &[u8], bounds: Bounds, locked: &Locked) -> Result<Picture, Failure> {
    if data.starts_with(PNG_SIGNATURE) {
        png(data, bounds, locked)
    } else if data.starts_with(b"\xff\xd8\xff") {
        jpeg(data, bounds, locked)
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        gif(data, bounds, locked)
    } else {
        Err(Failure::new(Status::Corrupt, "not a PNG, JPEG or GIF file"))
    }
}

fn png_failure(e: &png::DecodingError) -> Failure {
    match e {
        png::DecodingError::LimitsExceeded => Failure::new(
            Status::OverBudget,
            "the PNG decoder's work passes the budget",
        ),
        _ => Failure::new(Status::Corrupt, format!("corrupt PNG: {e}")),
    }
}

/// A PNG decoded: palette, grey, 16-bit and interlaced pictures expanded
/// to 8-bit RGBA. Bytes after the end chunk are counted as unused; a
/// stream whose end chunk is missing after a whole picture decodes and
/// counts none, as the C reference accepted.
pub fn png(data: &[u8], bounds: Bounds, _: &Locked) -> Result<Picture, Failure> {
    let mut cursor = Cursor::new(data);
    let work = usize::try_from(bounds.max_bytes.min(PNG_WORK_MAX)).unwrap_or(usize::MAX);
    let mut decoder = png::Decoder::new_with_limits(&mut cursor, png::Limits { bytes: work });
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let header = decoder.read_header_info().map_err(|e| png_failure(&e))?;
    let (width, height) = (header.width, header.height);
    let len = bounds.check(width, height)?;
    let mut reader = decoder.read_info().map_err(|e| png_failure(&e))?;
    palette_whole(reader.info())?;
    let mut rgba = zeroed(len)?;
    let info = reader.next_frame(&mut rgba).map_err(|e| png_failure(&e))?;
    if (info.width, info.height) != (width, height) || info.bit_depth != png::BitDepth::Eight {
        return Err(Failure::new(Status::Corrupt, "a PNG frame of another size"));
    }
    let channels = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => {
            return Err(Failure::new(Status::Corrupt, "a palette PNG not expanded"));
        }
    };
    expand(&mut rgba, channels);
    convert::premultiply_in_place(&mut rgba);
    // the picture is whole; what follows the end chunk is counted, and an
    // end chunk that is missing costs the picture nothing
    let rest = match reader.finish() {
        Ok(()) => {
            drop(reader);
            let at = cursor.stream_position().unwrap_or(0);
            u64::try_from(data.len())
                .unwrap_or(u64::MAX)
                .saturating_sub(at)
        }
        Err(_) => 0,
    };
    Ok(Picture {
        width,
        height,
        rgba,
        rest,
    })
}

/// Pixels of `channels` bytes each at the front of `rgba`, spread in place
/// to R, G, B, A, from the last pixel back so nothing is overwritten
/// before it is read: grey to all three colors, a missing alpha as opaque.
fn expand(rgba: &mut [u8], channels: usize) {
    if channels >= 4 || channels == 0 {
        return;
    }
    let pixels = rgba.len() / 4;
    for i in (0..pixels).rev() {
        let from = i.saturating_mul(channels);
        let mut px = [0u8, 0, 0, 0xff];
        match rgba.get(from..from.saturating_add(channels)) {
            Some(&[g]) => px = [g, g, g, 0xff],
            Some(&[g, a]) => px = [g, g, g, a],
            Some(&[r, g, b]) => px = [r, g, b, 0xff],
            _ => {}
        }
        if let Some(slot) = rgba.get_mut(i.saturating_mul(4)..i.saturating_mul(4).saturating_add(4))
        {
            slot.copy_from_slice(&px);
        }
    }
}

/// A palette picture's palette checked before the decoder expands it:
/// whole entries of three bytes, at most 256 of them. `png` 0.18.1 builds
/// its table of colors without these checks and slices past a palette
/// whose length is not a multiple of three, a panic a hostile file can
/// cause. A transparency chunk with more entries than the palette is no
/// such danger: `png` ignores the chunk and decodes the picture opaque,
/// and the C reference showed such files too.
fn palette_whole(info: &png::Info<'_>) -> Result<(), Failure> {
    if info.color_type != png::ColorType::Indexed {
        return Ok(());
    }
    let entries = info.palette.as_ref().map_or(0, |p| p.len());
    if entries == 0 || entries.checked_rem(3) != Some(0) || entries > 256 * 3 {
        return Err(Failure::new(Status::Corrupt, "a PNG palette cut short"));
    }
    Ok(())
}

/// A JPEG decoded to 8-bit RGB, opaque, its orientation tag not read.
pub fn jpeg(data: &[u8], bounds: Bounds, _: &Locked) -> Result<Picture, Failure> {
    use zune_jpeg::zune_core::bytestream::ZCursor;
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;
    let options = DecoderOptions::new_safe()
        .set_max_width(usize::try_from(bounds.largest.0).unwrap_or(usize::MAX))
        .set_max_height(usize::try_from(bounds.largest.1).unwrap_or(usize::MAX))
        .jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(data), options);
    let corrupt = |e: zune_jpeg::errors::DecodeErrors| {
        Failure::new(Status::Corrupt, format!("corrupt JPEG: {e:?}"))
    };
    decoder.decode_headers().map_err(corrupt)?;
    let (w, h) = decoder
        .dimensions()
        .ok_or_else(|| Failure::new(Status::Corrupt, "a JPEG with no size"))?;
    let width = u32::try_from(w).unwrap_or(u32::MAX);
    let height = u32::try_from(h).unwrap_or(u32::MAX);
    let len = bounds.check(width, height)?;
    let mut rgba = zeroed(len)?;
    let rgb_len = len.saturating_div(4).saturating_mul(3);
    let out = rgba
        .get_mut(..rgb_len)
        .ok_or_else(|| Failure::new(Status::Corrupt, "a JPEG of no pixels"))?;
    decoder.decode_into(out).map_err(corrupt)?;
    expand(&mut rgba, 3);
    Ok(Picture {
        width,
        height,
        rgba,
        rest: 0,
    })
}

/// Where a GIF's composed frames go: each frame's number from 0 and its
/// premultiplied R, G, B, A bytes.
pub type WriteFrame<'a> = dyn FnMut(u32, &[u8]) -> Result<(), Failure> + 'a;

/// The bounds on an animated GIF's frames, from the request's frames flag
/// (ADR-1101 D6, D7).
#[derive(Clone, Copy, Debug)]
pub struct FrameBounds {
    /// The most frames the reply may count, the first included.
    pub max_frames: u32,
    /// The most bytes the frames past the first may take together.
    pub max_frame_bytes: u64,
}

/// A GIF decoded frame by frame: its size, each frame's delay, its loop
/// count, and why it came back as its first frame alone, if it did.
#[derive(Debug, PartialEq, Eq)]
pub struct Frames {
    pub width: u32,
    pub height: u32,
    /// Each frame's delay in hundredths of a second, one for each frame
    /// written.
    pub delays: Vec<u16>,
    /// The loop count of the GIF's NETSCAPE2.0 application extension, 0
    /// meaning forever, or `None` when it has none.
    pub loops: Option<u16>,
    /// Why a GIF of more than one frame came back as its first frame
    /// alone, for vtium's log.
    pub still: Option<String>,
}

/// A GIF's first frame, laid on a transparent canvas of the GIF's own
/// size, its transparent pixels kept.
pub fn gif(data: &[u8], bounds: Bounds, locked: &Locked) -> Result<Picture, Failure> {
    let mut rgba = Vec::new();
    let frames = gif_frames(data, bounds, None, locked, &mut |_, frame| {
        rgba = frame.to_vec();
        Ok(())
    })?;
    Ok(Picture {
        width: frames.width,
        height: frames.height,
        rgba,
        rest: 0,
    })
}

/// A GIF's frames, each composed whole onto a canvas of the GIF's own size
/// and handed to `write` with its number from 0 as premultiplied R, G, B, A
/// bytes, the frame before it no longer needed once `write` returns. Without
/// `frames` only the first frame is decoded.
///
/// The first frame is laid on a transparent canvas, and each later frame
/// on the canvas the disposal method of the frame before it leaves: 0 or
/// 1 the frame before as it is, 2 the frame before with its own rectangle
/// cleared to transparent, 3 the canvas the frame before was laid on
/// (ADR-1102 D1). A frame's transparent pixels leave the canvas beneath
/// them, and a frame's rectangle is clipped to the canvas. The canvas's
/// size is checked against the bounds before a pixel is allocated. Before
/// it writes each frame past the first, it checks that the count stays
/// within `max_frames` and that the frames past the first stay within
/// `max_frame_bytes`; when either check fails, or a frame past the first
/// does not decode, the GIF comes back as its first frame alone, with the
/// reason in [`Frames::still`], and `write` has to keep only the first.
pub fn gif_frames(
    data: &[u8],
    bounds: Bounds,
    frames: Option<FrameBounds>,
    _: &Locked,
    write: &mut WriteFrame<'_>,
) -> Result<Frames, Failure> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let limit = bounds.max_bytes.saturating_add(1 << 20);
    options.set_memory_limit(gif::MemoryLimit::Bytes(
        std::num::NonZeroU64::new(limit).unwrap_or(std::num::NonZeroU64::MIN),
    ));
    let corrupt =
        |e: gif::DecodingError| Failure::new(Status::Corrupt, format!("corrupt GIF: {e}"));
    let mut decoder = options.read_info(data).map_err(corrupt)?;
    let (width, height) = (u32::from(decoder.width()), u32::from(decoder.height()));
    let len = bounds.check(width, height)?;
    let loops = match decoder.repeat() {
        // the decoder reads a count of 0 as forever, so a finite 0 is the
        // default it keeps when the GIF has no extension
        gif::Repeat::Finite(0) => None,
        gif::Repeat::Finite(n) => Some(n),
        gif::Repeat::Infinite => Some(0),
    };
    let mut out = Frames {
        width,
        height,
        delays: Vec::new(),
        loops,
        still: None,
    };
    let canvas_len = u64::try_from(len).unwrap_or(u64::MAX);
    let mut canvas = zeroed(len)?;
    let mut saved: Vec<u8> = Vec::new();
    let mut count = 0u32;
    loop {
        let frame = match decoder.read_next_frame() {
            Ok(Some(frame)) => frame,
            Ok(None) if count > 0 => break,
            Ok(None) => return Err(Failure::new(Status::Corrupt, "a GIF with no frame")),
            Err(e) if count == 0 => return Err(corrupt(e)),
            Err(e) => {
                out.still = Some(format!("a frame past the first does not decode: {e}"));
                break;
            }
        };
        if count > 0 {
            let Some(bounds) = frames else {
                break;
            };
            let past = u64::from(count).saturating_mul(canvas_len);
            if count >= bounds.max_frames {
                out.still = Some(format!("more than {} frames", bounds.max_frames));
                break;
            }
            if past > bounds.max_frame_bytes {
                out.still = Some("the frames pass the frame allowance".to_owned());
                break;
            }
        }
        if frame.dispose == gif::DisposalMethod::Previous {
            saved.clear();
            saved
                .try_reserve_exact(len)
                .map_err(|_| Failure::new(Status::OverBudget, "cannot allocate a saved canvas"))?;
            saved.extend_from_slice(&canvas);
        }
        let rect = Rect::of(frame, width, height);
        lay(&mut canvas, width, frame, rect);
        write(count, &canvas)?;
        out.delays.push(frame.delay);
        count = count.saturating_add(1);
        match frame.dispose {
            gif::DisposalMethod::Background => clear(&mut canvas, width, rect),
            gif::DisposalMethod::Previous => core::mem::swap(&mut canvas, &mut saved),
            gif::DisposalMethod::Any | gif::DisposalMethod::Keep => {}
        }
    }
    if out.still.is_some() {
        out.delays.truncate(1);
    }
    Ok(out)
}

/// A frame's rectangle clipped to the canvas: its left, top, the width and
/// height that lie inside, and the frame's own width.
#[derive(Clone, Copy, Debug)]
struct Rect {
    left: usize,
    top: usize,
    width: usize,
    height: usize,
    stride: usize,
}

impl Rect {
    fn of(frame: &gif::Frame<'_>, width: u32, height: u32) -> Self {
        let (left, top) = (usize::from(frame.left), usize::from(frame.top));
        let canvas_w = usize::try_from(width).unwrap_or(0);
        let canvas_h = usize::try_from(height).unwrap_or(0);
        let stride = usize::from(frame.width);
        Self {
            left,
            top,
            width: stride.min(canvas_w.saturating_sub(left)),
            height: usize::from(frame.height).min(canvas_h.saturating_sub(top)),
            stride,
        }
    }

    /// The byte range of row `y` of the rectangle in a canvas `canvas_w`
    /// pixels wide.
    fn row(&self, y: usize, canvas_w: usize) -> core::ops::Range<usize> {
        let at = self
            .top
            .saturating_add(y)
            .saturating_mul(canvas_w)
            .saturating_add(self.left)
            .saturating_mul(4);
        at..at.saturating_add(self.width.saturating_mul(4))
    }
}

/// Lays `frame`'s pixels inside `rect` on `canvas`, premultiplied: a pixel
/// with any alpha replaces the one beneath, a transparent one leaves it.
fn lay(canvas: &mut [u8], width: u32, frame: &gif::Frame<'_>, rect: Rect) {
    let canvas_w = usize::try_from(width).unwrap_or(0);
    for y in 0..rect.height {
        let from = y.saturating_mul(rect.stride).saturating_mul(4);
        let src = frame.buffer.get(from..).unwrap_or_default();
        let Some(dst) = canvas.get_mut(rect.row(y, canvas_w)) else {
            continue;
        };
        for (d, &[r, g, b, a]) in dst
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(src.as_chunks::<4>().0)
        {
            if a != 0 {
                d.copy_from_slice(&convert::reply_bytes(convert::premultiply(r, g, b, a)));
            }
        }
    }
}

/// Clears `rect` of `canvas` to transparent, disposal method 2.
fn clear(canvas: &mut [u8], width: u32, rect: Rect) {
    let canvas_w = usize::try_from(width).unwrap_or(0);
    for y in 0..rect.height {
        if let Some(row) = canvas.get_mut(rect.row(y, canvas_w)) {
            row.fill(0);
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    reason = "a test fails by panicking"
)]
pub(crate) mod tests {
    use super::*;

    /// A PNG of `width` by `height` in `color`, its rows given, written
    /// with the png crate's encoder.
    pub(crate) fn png_file(width: u32, height: u32, color: png::ColorType, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(color);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(data).unwrap();
        w.finish().unwrap();
        out
    }

    const WIDE: Bounds = Bounds {
        largest: (100, 100),
        max_bytes: 1 << 20,
    };

    #[test]
    fn a_png_decodes_premultiplied_and_counts_what_follows_its_end() {
        let file = png_file(
            2,
            1,
            png::ColorType::Rgba,
            &[0xff, 0x80, 0, 0xff, 0xff, 0xff, 0xff, 0x80],
        );
        let pic = png(&file, WIDE, &Locked::unsandboxed()).unwrap();
        assert_eq!((pic.width, pic.height, pic.rest), (2, 1, 0));
        assert_eq!(pic.rgba, [0xff, 0x80, 0, 0xff, 0x80, 0x80, 0x80, 0x80]);
        let mut junk = file.clone();
        junk.extend_from_slice(b"12345");
        assert_eq!(png(&junk, WIDE, &Locked::unsandboxed()).unwrap().rest, 5);
        // a grey picture spreads to all three colors, opaque
        let grey = png_file(2, 1, png::ColorType::Grayscale, &[0x10, 0x20]);
        assert_eq!(
            png(&grey, WIDE, &Locked::unsandboxed()).unwrap().rgba,
            [0x10, 0x10, 0x10, 0xff, 0x20, 0x20, 0x20, 0xff]
        );
        let rgb = png_file(1, 1, png::ColorType::Rgb, &[1, 2, 3]);
        assert_eq!(
            png(&rgb, WIDE, &Locked::unsandboxed()).unwrap().rgba,
            [1, 2, 3, 0xff]
        );
    }

    #[test]
    fn a_png_end_chunk_removed_and_junk_appended_decodes_with_none_ignored() {
        let file = png_file(1, 1, png::ColorType::Rgb, &[1, 2, 3]);
        // IEND is the last 12 bytes
        let mut cut = file[..file.len() - 12].to_vec();
        cut.extend_from_slice(b"junkjunk");
        let pic = png(&cut, WIDE, &Locked::unsandboxed()).unwrap();
        assert_eq!((pic.rgba.as_slice(), pic.rest), (&[1u8, 2, 3, 0xff][..], 0));
    }

    #[test]
    fn a_header_past_the_bounds_is_refused_before_any_pixel() {
        let file = png_file(3, 2, png::ColorType::Rgb, &[0; 18]);
        let narrow = Bounds {
            largest: (2, 100),
            max_bytes: 1 << 20,
        };
        assert_eq!(
            png(&file, narrow, &Locked::unsandboxed())
                .unwrap_err()
                .status,
            Status::TooLarge
        );
        let poor = Bounds {
            largest: (100, 100),
            max_bytes: 23,
        };
        assert_eq!(
            png(&file, poor, &Locked::unsandboxed()).unwrap_err().status,
            Status::OverBudget
        );
        let enough = Bounds {
            largest: (3, 2),
            max_bytes: 24,
        };
        assert!(png(&file, enough, &Locked::unsandboxed()).is_ok());
        assert_eq!(
            png(&file[..30], WIDE, &Locked::unsandboxed())
                .unwrap_err()
                .status,
            Status::Corrupt
        );
    }

    #[test]
    fn any_image_takes_png_by_its_signature_and_refuses_what_it_does_not_know() {
        let file = png_file(1, 1, png::ColorType::Rgb, &[1, 2, 3]);
        assert_eq!(
            any(&file, WIDE, &Locked::unsandboxed()).unwrap().rgba,
            [1, 2, 3, 0xff]
        );
        assert_eq!(
            any(b"BM...", WIDE, &Locked::unsandboxed())
                .unwrap_err()
                .status,
            Status::Corrupt
        );
        assert_eq!(
            any(b"\xff\xd8\xffjunk", WIDE, &Locked::unsandboxed())
                .unwrap_err()
                .status,
            Status::Corrupt
        );
        assert_eq!(
            any(b"GIF89ajunk", WIDE, &Locked::unsandboxed())
                .unwrap_err()
                .status,
            Status::Corrupt
        );
    }

    #[test]
    fn a_gif_first_frame_lies_on_a_transparent_canvas() {
        let mut out = Vec::new();
        {
            let palette = [0u8, 0, 0, 0xff, 0, 0];
            let mut enc = gif::Encoder::new(&mut out, 3, 2, &palette).unwrap();
            let mut frame = gif::Frame {
                left: 1,
                top: 1,
                width: 2,
                height: 1,
                buffer: std::borrow::Cow::Borrowed(&[1, 0]),
                transparent: Some(0),
                ..gif::Frame::default()
            };
            frame.palette = None;
            enc.write_frame(&frame).unwrap();
        }
        let pic = gif(&out, WIDE, &Locked::unsandboxed()).unwrap();
        assert_eq!((pic.width, pic.height), (3, 2));
        let mut want = vec![0u8; 24];
        want[16..20].copy_from_slice(&[0xff, 0, 0, 0xff]);
        assert_eq!(pic.rgba, want);
    }

    /// A GIF 3 by 1 pixels whose global palette holds transparent black,
    /// red, green and blue, with `repeat` if given, and `frames` each as
    /// (left, width, palette indices, disposal method, delay).
    fn gif_file(
        repeat: Option<gif::Repeat>,
        frames: &[(u16, u16, &[u8], gif::DisposalMethod, u16)],
    ) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let palette = [0u8, 0, 0, 0xff, 0, 0, 0, 0xff, 0, 0, 0, 0xff];
            let mut enc = gif::Encoder::new(&mut out, 3, 1, &palette).unwrap();
            if let Some(r) = repeat {
                enc.set_repeat(r).unwrap();
            }
            for &(left, width, indices, dispose, delay) in frames {
                let frame = gif::Frame {
                    left,
                    top: 0,
                    width,
                    height: 1,
                    buffer: std::borrow::Cow::Borrowed(indices),
                    transparent: Some(0),
                    dispose,
                    delay,
                    ..gif::Frame::default()
                };
                enc.write_frame(&frame).unwrap();
            }
        }
        out
    }

    const R: [u8; 4] = [0xff, 0, 0, 0xff];
    const G: [u8; 4] = [0, 0xff, 0, 0xff];
    const B: [u8; 4] = [0, 0, 0xff, 0xff];
    const CLEAR: [u8; 4] = [0, 0, 0, 0];

    const ALL: FrameBounds = FrameBounds {
        max_frames: 1024,
        max_frame_bytes: 1 << 20,
    };

    /// Every frame `gif_frames` writes, the frames it reports, and only the
    /// first frame when it reports the GIF still, as the job keeps them.
    fn decode_frames(file: &[u8], frames: FrameBounds) -> (Vec<Vec<[u8; 4]>>, Frames) {
        let mut written: Vec<Vec<[u8; 4]>> = Vec::new();
        let got = gif_frames(
            file,
            WIDE,
            Some(frames),
            &Locked::unsandboxed(),
            &mut |n, rgba| {
                assert_eq!(n as usize, written.len(), "frames in order");
                written.push(rgba.chunks(4).map(|p| p.try_into().unwrap()).collect());
                Ok(())
            },
        )
        .unwrap();
        if got.still.is_some() {
            written.truncate(1);
        }
        assert_eq!(written.len(), got.delays.len());
        (written, got)
    }

    use gif::DisposalMethod::{Any, Background, Keep, Previous};

    // covers: image.iterm2
    #[test]
    fn disposal_methods_0_and_1_lay_the_next_frame_on_the_frame_before() {
        for dispose in [Any, Keep] {
            let file = gif_file(
                None,
                &[(0, 3, &[1, 1, 1], dispose, 5), (1, 1, &[2], Keep, 7)],
            );
            let (frames, got) = decode_frames(&file, ALL);
            assert_eq!(frames, [vec![R, R, R], vec![R, G, R]], "{dispose:?}");
            assert_eq!(got.delays, [5, 7]);
            assert_eq!(got.still, None);
        }
    }

    // covers: image.iterm2
    #[test]
    fn disposal_method_2_clears_the_frames_own_rectangle_to_transparent() {
        let file = gif_file(
            None,
            &[
                (0, 3, &[1, 1, 1], Keep, 0),
                (1, 1, &[2], Background, 0),
                (2, 1, &[3], Keep, 0),
            ],
        );
        let (frames, _) = decode_frames(&file, ALL);
        assert_eq!(
            frames,
            [vec![R, R, R], vec![R, G, R], vec![R, CLEAR, B]],
            "only the second frame's own pixel is cleared"
        );
    }

    // covers: image.iterm2
    #[test]
    fn disposal_method_3_restores_the_canvas_the_frame_was_laid_on() {
        let file = gif_file(
            None,
            &[
                (0, 3, &[1, 1, 1], Keep, 0),
                (1, 1, &[2], Previous, 0),
                (2, 1, &[3], Keep, 0),
            ],
        );
        let (frames, _) = decode_frames(&file, ALL);
        assert_eq!(frames, [vec![R, R, R], vec![R, G, R], vec![R, R, B]]);
        // the first frame's previous canvas is the transparent one
        let file = gif_file(None, &[(0, 2, &[1, 1], Previous, 0), (2, 1, &[3], Keep, 0)]);
        let (frames, _) = decode_frames(&file, ALL);
        assert_eq!(frames, [vec![R, R, CLEAR], vec![CLEAR, CLEAR, B]]);
    }

    #[test]
    fn a_frames_transparent_pixels_leave_the_canvas_beneath() {
        let file = gif_file(
            None,
            &[(0, 3, &[1, 2, 3], Keep, 0), (0, 3, &[0, 3, 0], Keep, 0)],
        );
        let (frames, _) = decode_frames(&file, ALL);
        assert_eq!(frames, [vec![R, G, B], vec![R, B, B]]);
    }

    // covers: image.iterm2
    #[test]
    fn the_loop_count_comes_from_the_netscape_extension_or_its_absence() {
        let two = [
            (0, 3, &[1u8, 1, 1][..], Keep, 0),
            (0, 3, &[2, 2, 2][..], Keep, 0),
        ];
        let loops = |repeat| decode_frames(&gif_file(repeat, &two), ALL).1.loops;
        assert_eq!(loops(None), None);
        assert_eq!(loops(Some(gif::Repeat::Infinite)), Some(0));
        assert_eq!(loops(Some(gif::Repeat::Finite(2))), Some(2));
    }

    // covers: image.iterm2
    #[test]
    fn a_gif_past_its_frame_bounds_comes_back_as_its_first_frame() {
        let three = [
            (0, 3, &[1u8, 1, 1][..], Keep, 3),
            (0, 3, &[2, 2, 2][..], Keep, 4),
            (0, 3, &[3, 3, 3][..], Keep, 5),
        ];
        let file = gif_file(None, &three);
        let bounds = |max_frames, max_frame_bytes| FrameBounds {
            max_frames,
            max_frame_bytes,
        };
        // three frames fit three, and two frames past the first of 12 bytes
        let (frames, got) = decode_frames(&file, bounds(3, 24));
        assert_eq!(frames.len(), 3);
        assert_eq!(got.still, None);
        for (max_frames, bytes) in [(2, 24), (3, 23), (1, 1 << 20)] {
            let (frames, got) = decode_frames(&file, bounds(max_frames, bytes));
            assert_eq!(
                frames,
                [vec![R, R, R]],
                "{max_frames} frames, {bytes} bytes"
            );
            assert_eq!(got.delays, [3]);
            assert!(got.still.is_some());
        }
        // without the frames flag only the first frame is decoded, with no
        // reason to log
        let mut seen = 0;
        let got = gif_frames(&file, WIDE, None, &Locked::unsandboxed(), &mut |_, _| {
            seen += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!((seen, got.delays.len(), got.still), (1, 1, None));
    }

    #[test]
    fn a_frame_past_the_first_that_does_not_decode_leaves_the_first_frame() {
        let three = [
            (0, 3, &[1u8, 1, 1][..], Keep, 0),
            (0, 3, &[2, 2, 2][..], Keep, 0),
        ];
        let file = gif_file(None, &three);
        // cut inside the second frame's data, before the trailer
        let cut = &file[..file.len() - 4];
        let (frames, got) = decode_frames(cut, ALL);
        assert_eq!(frames, [vec![R, R, R]]);
        assert!(got.still.is_some(), "{got:?}");
        // a first frame that does not decode is a corrupt file
        let header_only = &file[..file.len() - 30];
        assert_eq!(
            gif_frames(
                header_only,
                WIDE,
                Some(ALL),
                &Locked::unsandboxed(),
                &mut |_, _| Ok(())
            )
            .unwrap_err()
            .status,
            Status::Corrupt
        );
    }

    /// Bounds as wide as the jpeg fuzz target decodes under.
    const FUZZ: Bounds = Bounds {
        largest: (512, 512),
        max_bytes: 1 << 20,
    };

    /// A hostile JPEG gives a picture or a refusal, never a panic: the
    /// helper's release build aborts on a panic, which kills the worker
    /// and counts a crash against the console (ADR-1101 D8).
    fn decodes_or_refuses(data: &[u8]) {
        if let Ok(p) = jpeg(data, FUZZ, &Locked::unsandboxed()) {
            assert_eq!(
                p.rgba.len() as u64,
                u64::from(p.width) * u64::from(p.height) * 4
            );
        }
    }

    /// `zune-jpeg` 0.4.21's scalar IDCT added two coefficients without
    /// wrapping, so a progressive JPEG with large coefficients overflowed
    /// (`src/idct/scalar.rs` line 128), a panic wherever overflow is
    /// checked, as in tests. The fixture is the fuzz target's crash input,
    /// minimized; `tests/fixtures/jpeg/README.md` gives its origin.
    #[test]
    fn a_progressive_jpeg_whose_coefficients_overflow_does_not_panic() {
        decodes_or_refuses(include_bytes!("../tests/fixtures/jpeg/idct-overflow.jpg"));
    }

    /// 0.4.21 asserted that a horizontally subsampled component's buffers
    /// had sizes the file can contradict (`src/worker.rs` line 412), a
    /// panic in every build, the helper's release build included.
    #[test]
    fn a_jpeg_whose_subsampling_contradicts_its_buffers_does_not_panic() {
        decodes_or_refuses(include_bytes!("../tests/fixtures/jpeg/upsample-assert.jpg"));
    }

    /// `png` 0.18.1 builds its palette table by copying the palette three
    /// bytes at a time and slices past its end when the palette's length
    /// is not a multiple of three (`src/decoder/transform/palette.rs` line
    /// 74), a panic in every build, which a palette PNG can cause. The
    /// fixture is the png fuzz target's crash input, minimized;
    /// `tests/fixtures/png/README.md` gives its origin.
    /// A palette picture of two colors whose transparency chunk has four
    /// entries, more than the palette, decodes: `png` ignores the chunk,
    /// so the colors come out opaque.
    #[test]
    fn a_palette_png_whose_transparency_outruns_its_palette_decodes() {
        let mut file = Vec::new();
        let mut enc = png::Encoder::new(&mut file, 2, 1);
        enc.set_color(png::ColorType::Indexed);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_palette(vec![10, 20, 30, 40, 50, 60]);
        enc.set_trns(vec![0, 0x80, 0x40, 0x20]);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&[1, 0]).unwrap();
        w.finish().unwrap();
        for decoded in [
            png(&file, WIDE, &Locked::unsandboxed()),
            any(&file, WIDE, &Locked::unsandboxed()),
        ] {
            let pic = decoded.unwrap();
            assert_eq!((pic.width, pic.height), (2, 1));
            assert_eq!(pic.rgba, [40, 50, 60, 0xff, 10, 20, 30, 0xff]);
        }
    }

    #[test]
    fn a_palette_png_whose_palette_is_cut_short_is_corrupt_and_does_not_panic() {
        let file = include_bytes!("../tests/fixtures/png/palette-remainder.png");
        for decoded in [
            png(file, FUZZ, &Locked::unsandboxed()),
            any(file, FUZZ, &Locked::unsandboxed()),
        ] {
            assert_eq!(decoded.unwrap_err().status, Status::Corrupt);
        }
    }
}
