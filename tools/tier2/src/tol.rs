//! Tolerance sidecars: `<screen>.tol` beside a golden, the format tier 3's
//! goldens use (aaronsb/kmscon#18).
//!
//! `key=value` lines, `#` starting a comment:
//!
//! ```text
//! max_delta=1          the largest per-channel difference allowed
//! max_pixels=200       how many pixels may differ
//! mask=0,780,1280,800  a rectangle x0,y0,x1,y1, end-exclusive, left out
//! ```
//!
//! No sidecar means exact, and an absent key is 0. A sidecar this reader
//! cannot hold to its meaning is refused, never read as exact: an unknown
//! or repeated key, a value that is not a decimal number, a max_delta past
//! 255, a mask that is empty or inverted, and a tolerance with one limit at
//! 0 and the other not, which allows nothing and reads as if it allowed
//! something (aaronsb/kmscon#10). A mask that reaches past the image is
//! refused when the image is compared.

use std::fmt;

/// A rectangle of pixels, end-exclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

impl Rect {
    pub fn contains(&self, x: u32, y: u32) -> bool {
        (self.x0..self.x1).contains(&x) && (self.y0..self.y1).contains(&y)
    }

    /// Reads `x0,y0,x1,y1`, refusing an empty or inverted rectangle.
    pub fn parse(text: &str) -> Result<Rect, String> {
        let v: Vec<u32> = text
            .split(',')
            .map(|s| number(s.trim()).and_then(|n| u32::try_from(n).ok()))
            .collect::<Option<_>>()
            .ok_or_else(|| format!("mask {text:?}: want four decimal numbers x0,y0,x1,y1"))?;
        match v[..] {
            [x0, y0, x1, y1] if x0 < x1 && y0 < y1 => Ok(Rect { x0, y0, x1, y1 }),
            [_, _, _, _] => Err(format!(
                "mask {text:?}: empty or inverted, want x0 < x1 and y0 < y1"
            )),
            _ => Err(format!(
                "mask {text:?}: want four decimal numbers x0,y0,x1,y1"
            )),
        }
    }
}

impl fmt::Display for Rect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{},{},{},{}", self.x0, self.y0, self.x1, self.y1)
    }
}

/// How far a case may differ from its golden.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tolerance {
    pub max_delta: u8,
    pub max_pixels: u64,
    pub masks: Vec<Rect>,
}

/// A decimal number: digits only, no sign, at most 18 of them.
fn number(s: &str) -> Option<u64> {
    (!s.is_empty() && s.len() <= 18 && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.parse().ok())
        .flatten()
}

impl Tolerance {
    /// Reads a sidecar's text; `name` labels the errors.
    pub fn parse(text: &str, name: &str) -> Result<Tolerance, String> {
        let mut tol = Tolerance::default();
        let (mut delta, mut pixels) = (false, false);
        for (n, line) in text.lines().enumerate() {
            let at = |e: String| format!("{name}:{}: {e}", n + 1);
            let line = line.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(at(format!("{line:?}: want key=value")));
            };
            let (key, value) = (key.trim(), value.trim());
            let num = || {
                number(value).ok_or_else(|| at(format!("{key}: {value:?} is not a decimal number")))
            };
            match key {
                "max_delta" if !delta => {
                    tol.max_delta = u8::try_from(num()?)
                        .map_err(|_| at(format!("max_delta {value} is past 255")))?;
                    delta = true;
                }
                "max_pixels" if !pixels => {
                    tol.max_pixels = num()?;
                    pixels = true;
                }
                "max_delta" | "max_pixels" => return Err(at(format!("{key} given twice"))),
                "mask" => tol.masks.push(Rect::parse(value).map_err(at)?),
                _ => return Err(at(format!("unknown key {key:?}"))),
            }
        }
        if (tol.max_delta == 0) != (tol.max_pixels == 0) {
            return Err(format!(
                "{name}: max_delta={} with max_pixels={} allows no difference; give both or neither",
                tol.max_delta, tol.max_pixels
            ));
        }
        Ok(tol)
    }

    /// Whether this tolerance is exact: no limit and no mask.
    pub fn is_exact(&self) -> bool {
        *self == Tolerance::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sidecar_reads_its_keys_and_comments() {
        let t = Tolerance::parse(
            "# why\nmax_delta = 2\nmax_pixels=10 # at most\nmask=0,1,5,6\nmask=7,7,8,8\n",
            "t.tol",
        )
        .unwrap();
        assert_eq!((t.max_delta, t.max_pixels), (2, 10));
        assert_eq!(
            t.masks,
            [
                Rect {
                    x0: 0,
                    y0: 1,
                    x1: 5,
                    y1: 6
                },
                Rect {
                    x0: 7,
                    y0: 7,
                    x1: 8,
                    y1: 8
                }
            ]
        );
        assert!(Tolerance::parse("", "t.tol").unwrap().is_exact());
        assert!(
            !Tolerance::parse("mask=0,0,1,1", "t.tol")
                .unwrap()
                .is_exact()
        );
    }

    #[test]
    fn a_mask_is_end_exclusive() {
        let r = Rect::parse("1,1,3,2").unwrap();
        assert!(r.contains(1, 1) && r.contains(2, 1));
        assert!(!r.contains(3, 1) && !r.contains(1, 2));
    }
}
