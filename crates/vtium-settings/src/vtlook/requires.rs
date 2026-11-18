//! A vtlook's display requirements, its `[requires]` section.
//!
//! A display that does not meet them draws the built-in vtlook on its own,
//! while the head's other displays keep the vtlook. That is not an error.
//! Every key takes `any`, which sets no requirement, and a key the section
//! leaves out is `any` too. A measured requirement is a pair of keys, a
//! minimum and a maximum, and a minimum above its maximum is an error.
//!
//! References:
//! - ADR-1304 D7: the eight keys, their values and the errors of a malformed section.

use super::report::quoted;

/// The order of the keys, as the template and vtium-config write them.
pub const KEYS: [&str; 8] = [
    "min-depth",
    "max-depth",
    "min-size",
    "max-size",
    "min-dpi",
    "max-dpi",
    "min-fps",
    "gpu",
];

/// The largest density and frame rate a requirement may name.
const MEASURE_MAX: u32 = 1000;

/// A display's depth, in the order a requirement compares them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Depth {
    /// A display drawn at 1 bit a pixel.
    One,
    /// A gray panel, 8 bits a pixel.
    Gray,
    /// 16-bit color.
    Sixteen,
    /// 24-bit color; a 32-bit buffer counts as 24.
    TwentyFour,
}

impl Depth {
    fn of(value: &str) -> Option<Depth> {
        match value {
            "1" => Some(Depth::One),
            "8" => Some(Depth::Gray),
            "16" => Some(Depth::Sixteen),
            "24" | "32" => Some(Depth::TwentyFour),
            _ => None,
        }
    }
}

/// A display mode's size, in the mode's own pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    fn of(value: &str) -> Option<Size> {
        let (w, h) = value.split_once('x')?;
        let side = |s: &str| {
            s.bytes()
                .all(|b| b.is_ascii_digit())
                .then(|| s.parse::<u32>().ok())
                .flatten()
                .filter(|n| (1..=u32::from(u16::MAX)).contains(n))
        };
        Some(Size {
            width: side(w)?,
            height: side(h)?,
        })
    }
}

/// The frame rate a vtlook's filters must keep.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fps {
    #[default]
    Any,
    /// The display's refresh rate.
    Refresh,
    /// This many frames a second.
    Rate(u32),
}

/// Which path a head must draw on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Gpu {
    #[default]
    Any,
    /// The GPU path only.
    Yes,
    /// The CPU path only.
    No,
}

/// A vtlook's display requirements. `None` is `any`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Requires {
    pub min_depth: Option<Depth>,
    pub max_depth: Option<Depth>,
    pub min_size: Option<Size>,
    pub max_size: Option<Size>,
    pub min_dpi: Option<u32>,
    pub max_dpi: Option<u32>,
    pub min_fps: Fps,
    pub gpu: Gpu,
}

/// A whole number from 1 to [`MEASURE_MAX`], written without a sign.
fn measure(value: &str) -> Option<u32> {
    value
        .bytes()
        .all(|b| b.is_ascii_digit())
        .then(|| value.parse::<u32>().ok())
        .flatten()
        .filter(|n| (1..=MEASURE_MAX).contains(n))
}

/// What a key takes, as its error says it.
fn needs(key: &str) -> &'static str {
    match key {
        "min-depth" | "max-depth" => "any, 1, 8, 16 or 24",
        "min-size" | "max-size" => "any or WxH, each side a whole number from 1 to 65535",
        "min-dpi" | "max-dpi" => "any or a whole number from 1 to 1000",
        "min-fps" => "any, refresh or a whole number from 1 to 1000",
        _ => "any, yes or no",
    }
}

impl Requires {
    /// Sets the key `key` to `value`. `Err` holds the error's message when
    /// the key is unknown or the value is not one it takes.
    pub(super) fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        if !KEYS.contains(&key) {
            return Err(format!(
                "found {}, needs one of {}",
                quoted(key),
                list(&KEYS)
            ));
        }
        let bad = || format!("{key} = {}, needs {}", quoted(value), needs(key));
        let any = value == "any";
        match key {
            "min-depth" | "max-depth" => {
                let depth = if any {
                    None
                } else {
                    Some(Depth::of(value).ok_or_else(bad)?)
                };
                if key == "min-depth" {
                    self.min_depth = depth;
                } else {
                    self.max_depth = depth;
                }
            }
            "min-size" | "max-size" => {
                let size = if any {
                    None
                } else {
                    Some(Size::of(value).ok_or_else(bad)?)
                };
                if key == "min-size" {
                    self.min_size = size;
                } else {
                    self.max_size = size;
                }
            }
            "min-dpi" | "max-dpi" => {
                let dpi = if any {
                    None
                } else {
                    Some(measure(value).ok_or_else(bad)?)
                };
                if key == "min-dpi" {
                    self.min_dpi = dpi;
                } else {
                    self.max_dpi = dpi;
                }
            }
            "min-fps" => {
                self.min_fps = match value {
                    "any" => Fps::Any,
                    "refresh" => Fps::Refresh,
                    _ => Fps::Rate(measure(value).ok_or_else(bad)?),
                };
            }
            _ => {
                self.gpu = match value {
                    "any" => Gpu::Any,
                    "yes" => Gpu::Yes,
                    "no" => Gpu::No,
                    _ => return Err(bad()),
                };
            }
        }
        Ok(())
    }

    /// Each pair whose minimum exceeds its maximum: the minimum's key, then
    /// the error's message.
    pub(super) fn out_of_order(&self, values: &[(&str, &str)]) -> Vec<(&'static str, String)> {
        let written = |key: &str| {
            values
                .iter()
                .rev()
                .find(|(k, _)| *k == key)
                .map_or("", |(_, v)| *v)
        };
        let mut out = Vec::new();
        let mut pair = |min: &'static str, max: &'static str, wrong: bool| {
            if wrong {
                out.push((
                    min,
                    format!("{min} = {} exceeds {max} = {}", written(min), written(max)),
                ));
            }
        };
        pair(
            "min-depth",
            "max-depth",
            matches!((self.min_depth, self.max_depth), (Some(a), Some(b)) if a > b),
        );
        pair(
            "min-size",
            "max-size",
            matches!((self.min_size, self.max_size), (Some(a), Some(b)) if a.width > b.width || a.height > b.height),
        );
        pair(
            "min-dpi",
            "max-dpi",
            matches!((self.min_dpi, self.max_dpi), (Some(a), Some(b)) if a > b),
        );
        out
    }
}

/// Words joined as a sentence lists them: "a, b or c".
pub(super) fn list(words: &[&str]) -> String {
    match words {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_key_takes_any_and_its_own_values() {
        let mut r = Requires::default();
        for (key, value) in [
            ("min-depth", "16"),
            ("max-depth", "32"),
            ("min-size", "1280x720"),
            ("max-size", "3840x2160"),
            ("min-dpi", "110"),
            ("max-dpi", "any"),
            ("min-fps", "refresh"),
            ("gpu", "yes"),
        ] {
            assert_eq!(r.set(key, value), Ok(()), "{key} = {value}");
        }
        assert_eq!(r.min_depth, Some(Depth::Sixteen));
        assert_eq!(r.max_depth, Some(Depth::TwentyFour));
        assert_eq!(
            r.min_size,
            Some(Size {
                width: 1280,
                height: 720
            })
        );
        assert_eq!(r.max_dpi, None);
        assert_eq!(r.min_fps, Fps::Refresh);
        assert_eq!(r.gpu, Gpu::Yes);
        assert!(r.out_of_order(&[]).is_empty());
    }

    // Each wrong value's message names the key and the value, and what the
    // key takes, beginning with any.
    #[test]
    fn a_wrong_value_says_what_the_key_takes() {
        let mut r = Requires::default();
        let cases = [
            (
                "min-depth",
                "12",
                "min-depth = 12, needs any, 1, 8, 16 or 24",
            ),
            ("gpu", "maybe", "gpu = maybe, needs any, yes or no"),
            (
                "min-dpi",
                "0",
                "min-dpi = 0, needs any or a whole number from 1 to 1000",
            ),
            (
                "min-fps",
                "+60",
                "min-fps = +60, needs any, refresh or a whole number from 1 to 1000",
            ),
            (
                "max-size",
                "1280x0",
                "max-size = 1280x0, needs any or WxH, each side a whole number from 1 to 65535",
            ),
            (
                "depth",
                "8",
                "found depth, needs one of min-depth, max-depth, min-size, max-size, min-dpi, max-dpi, min-fps or gpu",
            ),
        ];
        for (key, value, message) in cases {
            assert_eq!(r.set(key, value), Err(message.to_owned()));
        }
    }

    #[test]
    fn a_minimum_above_its_maximum_names_both_keys() {
        let mut r = Requires::default();
        let values = [("min-size", "1920x1080"), ("max-size", "1280x800")];
        for (k, v) in values {
            assert_eq!(r.set(k, v), Ok(()));
        }
        assert_eq!(
            r.out_of_order(&values),
            [(
                "min-size",
                "min-size = 1920x1080 exceeds max-size = 1280x800".to_owned()
            )]
        );
        // either side out of order is enough
        let values = [("min-size", "800x1200"), ("max-size", "1280x800")];
        for (k, v) in values {
            assert_eq!(r.set(k, v), Ok(()));
        }
        assert_eq!(r.out_of_order(&values).len(), 1);
        let values = [("min-depth", "24"), ("max-depth", "8")];
        let mut r = Requires::default();
        for (k, v) in values {
            assert_eq!(r.set(k, v), Ok(()));
        }
        assert_eq!(r.out_of_order(&values).len(), 1);
    }
}
