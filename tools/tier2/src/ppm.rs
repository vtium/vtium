//! Binary PPM (P6) images, read strictly and written plainly.
//!
//! The reader follows the Netpbm format's description (netpbm.sourceforge.net,
//! "ppm"): the magic `P6`, then the width, height and maxval as decimal
//! numbers separated by whitespace (blanks, tabs, CRs and LFs), with a
//! comment from `#` to the next CR or LF anywhere before the single
//! whitespace character that ends the header, then the raster. It refuses
//! what it cannot judge: a maxval other than 255, a side of 0 or past
//! [`SIDE_MAX`], and a raster shorter or longer than the header says, so a
//! file still being written, or two images in one file, is never read as
//! one image.

use std::fmt;

/// The longest side read: past every display tier 3 captures (7680x2160).
pub const SIDE_MAX: u32 = 16384;

/// An RGB image, 8 bits a channel, rows top to bottom.
#[derive(Clone, PartialEq, Eq)]
pub struct Image {
    width: u32,
    height: u32,
    rgb: Vec<u8>,
}

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Image({}x{})", self.width, self.height)
    }
}

impl Image {
    /// An image of `rgb`, 3 bytes a pixel; `None` when the sizes disagree
    /// or a side is 0 or past [`SIDE_MAX`].
    pub fn new(width: u32, height: u32, rgb: Vec<u8>) -> Option<Image> {
        let len = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(3)?;
        let sides = (1..=SIDE_MAX).contains(&width) && (1..=SIDE_MAX).contains(&height);
        (sides && rgb.len() == len).then_some(Image { width, height, rgb })
    }

    /// The image of `pixels`, each `0x00RRGGBB`, as a `Canvas` holds them.
    pub fn from_xrgb(width: u32, height: u32, pixels: &[u32]) -> Option<Image> {
        let rgb = pixels
            .iter()
            .flat_map(|p| {
                let [_, r, g, b] = p.to_be_bytes();
                [r, g, b]
            })
            .collect();
        Image::new(width, height, rgb)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// The pixels, 3 bytes each, row by row.
    pub fn rgb(&self) -> &[u8] {
        &self.rgb
    }

    /// Row `y`'s bytes.
    pub fn row(&self, y: u32) -> &[u8] {
        let stride = self.width as usize * 3;
        let start = y as usize * stride;
        &self.rgb[start..start + stride]
    }

    /// The image as a P6 file.
    pub fn to_ppm(&self) -> Vec<u8> {
        let mut out = format!("P6\n{} {}\n255\n", self.width, self.height).into_bytes();
        out.extend_from_slice(&self.rgb);
        out
    }
}

/// Why a file is not a PPM this reader takes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PpmError {
    /// It does not start with `P6`.
    Magic,
    /// The header ends before its three numbers.
    Truncated,
    /// A header field is not a decimal number.
    Field(&'static str),
    /// A side is 0 or past [`SIDE_MAX`].
    Size(u64, u64),
    /// The maxval is not 255.
    Maxval(u64),
    /// No single whitespace character ends the header.
    HeaderEnd,
    /// The raster's length differs from width x height x 3.
    Raster { got: usize, want: usize },
}

impl fmt::Display for PpmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PpmError::Magic => write!(f, "not a binary PPM: no P6 magic"),
            PpmError::Truncated => write!(f, "the header ends before its width, height and maxval"),
            PpmError::Field(name) => write!(f, "the {name} is not a decimal number"),
            PpmError::Size(w, h) => {
                write!(f, "a size of {w}x{h}: each side must be 1 to {SIDE_MAX}")
            }
            PpmError::Maxval(m) => write!(f, "maxval {m}: only 255, 8 bits a channel, is read"),
            PpmError::HeaderEnd => write!(f, "no whitespace character ends the header"),
            PpmError::Raster { got, want } => {
                write!(f, "the raster is {got} bytes, the header says {want}")
            }
        }
    }
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

/// Skips whitespace and comments from `pos`.
fn skip(bytes: &[u8], mut pos: usize) -> usize {
    while let Some(&b) = bytes.get(pos) {
        if is_space(b) {
            pos += 1;
        } else if b == b'#' {
            while bytes.get(pos).is_some_and(|&c| c != b'\n' && c != b'\r') {
                pos += 1;
            }
        } else {
            break;
        }
    }
    pos
}

/// The decimal field at `pos`, and where it ends.
fn field(bytes: &[u8], pos: usize, name: &'static str) -> Result<(u64, usize), PpmError> {
    let digits = bytes
        .get(pos..)
        .unwrap_or_default()
        .iter()
        .take_while(|b| !is_space(**b) && **b != b'#')
        .count();
    if digits == 0 {
        return Err(PpmError::Truncated);
    }
    let text = &bytes[pos..pos + digits];
    if !text.iter().all(u8::is_ascii_digit) || digits > 9 {
        return Err(PpmError::Field(name));
    }
    let value = text.iter().fold(0u64, |v, d| v * 10 + u64::from(d - b'0'));
    Ok((value, pos + digits))
}

/// Reads a P6 image.
pub fn parse(bytes: &[u8]) -> Result<Image, PpmError> {
    if !bytes.starts_with(b"P6") {
        return Err(PpmError::Magic);
    }
    let (width, pos) = field(bytes, skip(bytes, 2), "width")?;
    let (height, pos) = field(bytes, skip(bytes, pos), "height")?;
    let (maxval, mut pos) = field(bytes, skip(bytes, pos), "maxval")?;
    // a comment may come before the whitespace that ends the header
    if bytes.get(pos) == Some(&b'#') {
        while bytes.get(pos).is_some_and(|&c| c != b'\n' && c != b'\r') {
            pos += 1;
        }
    }
    if !bytes.get(pos).copied().is_some_and(is_space) {
        return Err(PpmError::HeaderEnd);
    }
    let raster = &bytes[pos + 1..];
    let sides = 1..=u64::from(SIDE_MAX);
    if !sides.contains(&width) || !sides.contains(&height) {
        return Err(PpmError::Size(width, height));
    }
    if maxval != 255 {
        return Err(PpmError::Maxval(maxval));
    }
    let want = (width * height * 3) as usize;
    if raster.len() != want {
        return Err(PpmError::Raster {
            got: raster.len(),
            want,
        });
    }
    Image::new(width as u32, height as u32, raster.to_vec()).ok_or(PpmError::Size(width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_image_reads_back() {
        let img = Image::new(2, 1, vec![1, 2, 3, 4, 5, 6]).unwrap();
        assert_eq!(parse(&img.to_ppm()).unwrap(), img);
    }

    #[test]
    fn comments_and_any_whitespace_may_separate_the_fields() {
        let img = parse(b"P6#c\r\n1\t# w\n1\r255\n\x01\x02\x03").unwrap();
        assert_eq!(
            (img.width(), img.height(), img.rgb()),
            (1, 1, &[1u8, 2, 3][..])
        );
        let img = parse(b"P6 1 1 255# end\n\x01\x02\x03").unwrap();
        assert_eq!(img.rgb(), &[1, 2, 3]);
    }

    #[test]
    fn a_raster_byte_that_looks_like_whitespace_is_kept() {
        let img = parse(b"P6 1 1 255\n\n\n\n").unwrap();
        assert_eq!(img.rgb(), b"\n\n\n");
    }
}
