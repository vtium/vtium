//! The `fit=` lines of a frit, as the console reads them for the glyphs
//! it draws itself: the built-in font's letters and the box drawing,
//! block and other sprites.
//!
//! A frit's `fit=` line says how the set of characters on the `source=`
//! or `cover=` line above it uses a row that line spacing makes taller.
//! The font helper reads the whole frit and places every glyph it draws by
//! those lines. The console draws a character itself when the frit's
//! fonts lack it, or when the frit's sprite policy has vtium draw it, and
//! such a glyph takes the `fit=` of the frit's set that holds the
//! character. Without one, the character takes its default
//! ([`Fit::default_for`]).
//!
//! The console knows a set only by the ranges its `cover=` lines name:
//! `U+XXXX`, `U+XXXX-U+YYYY`, `ascii` and `latin-1`. A source with no
//! `cover=` line holds whatever its font holds, which only the helper
//! knows, so a `fit=` for such a whole source reaches only the glyphs that
//! source's font draws. The CJK sets that `cover=` names, such as
//! `gb2312`, are tables of thousands of characters that only the helper
//! carries, so their `fit=` lines also reach only their fonts' glyphs.
//!
//! The console reads the frit's text after the helper's checks, so a line
//! the console cannot read is a line the helper refuses or warns about,
//! and the console leaves it out here.
//!
//! References:
//! - ADR-1302 D13: `fit=` and the default fit of each kind of character.
//! - ADR-1200 D1: the frit's `source=`, `cover=`, `style=` and `fit=` keys.

use vtium_font::Style;
use vtium_font::spacing::Fit;

/// One range of a source's `cover=` lines and the fit its glyphs take
/// there: the `fit=` of the `cover=` line that names the range, else the
/// source's own `fit=`, else `None` for the character's default.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Range {
    first: u32,
    last: u32,
    style: Style,
    fit: Option<Fit>,
}

/// A frit's `fit=` lines, as ranges in the frit's order of sources
/// ([`FritFits::of`]).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct FritFits {
    ranges: Vec<Range>,
}

/// One `cover=` line's ranges and that line's own `fit=`.
type CoverLine = (Vec<(u32, u32)>, Option<Fit>);

/// The `source=` line being read and the keys read under it so far.
#[derive(Default)]
struct Reading {
    style: Style,
    covers: Vec<CoverLine>,
    /// The `fit=` that follows the `source=` line itself.
    whole: Option<Fit>,
}

impl Reading {
    /// The source's ranges, each with the fit its glyphs take there.
    fn ranges(self) -> impl Iterator<Item = Range> {
        let (style, whole) = (self.style, self.whole);
        self.covers.into_iter().flat_map(move |(ranges, fit)| {
            ranges.into_iter().map(move |(first, last)| Range {
                first,
                last,
                style,
                fit: fit.or(whole),
            })
        })
    }
}

impl FritFits {
    /// The `fit=` lines of the frit `text`, each with the ranges of the
    /// set it is for. A source's `cover=` lines keep the frit's order, so
    /// the first source that covers a character gives its fit, as the
    /// first source that covers it gives its glyph.
    pub fn of(text: &str) -> FritFits {
        let mut ranges = Vec::new();
        let mut reading: Option<Reading> = None;
        // the set a `fit=` line is for: true after a `cover=` line, false
        // after a `source=` line
        let mut after_cover = false;
        for line in text.split('\n') {
            let body = line.split('#').next().unwrap_or_default();
            let Some((key, value)) = body.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            if key == "source" {
                ranges.extend(reading.take().into_iter().flat_map(Reading::ranges));
                reading = Some(Reading::default());
                after_cover = false;
                continue;
            }
            let Some(source) = reading.as_mut() else {
                continue;
            };
            match key {
                "cover" => {
                    source.covers.push((cover(value), None));
                    after_cover = true;
                }
                "style" => {
                    if let Some(style) = style(value) {
                        source.style = style;
                    }
                }
                "fit" => {
                    let Some(fit) = Fit::parse(value) else {
                        continue;
                    };
                    match source.covers.last_mut() {
                        Some((_, set)) if after_cover => *set = Some(fit),
                        _ => source.whole = Some(fit),
                    }
                }
                _ => {}
            }
        }
        ranges.extend(reading.into_iter().flat_map(Reading::ranges));
        FritFits { ranges }
    }

    /// How the console's own glyph for `c` in `style` uses a taller row:
    /// the fit of the first source that covers `c` and draws `style`, else
    /// of the first regular source that covers it, as the helper looks for
    /// a glyph in a style before the regular glyph it synthesises the
    /// style from; else `c`'s default.
    pub fn fit(&self, c: u32, style: Style) -> Fit {
        let regular = Style::default();
        let mut styles = vec![style];
        if style != regular {
            styles.push(regular);
        }
        styles
            .into_iter()
            .find_map(|style| {
                self.ranges
                    .iter()
                    .find(|r| r.style == style && (r.first..=r.last).contains(&c))
            })
            .and_then(|r| r.fit)
            .unwrap_or_else(|| Fit::default_for(c))
    }
}

/// A `cover=` value's ranges where the console can read them: `ascii`,
/// `latin-1`, `U+XXXX` or `U+XXXX-U+YYYY`. A named CJK set, or a value the
/// helper refuses, gives no range.
fn cover(value: &str) -> Vec<(u32, u32)> {
    const ASCII: (u32, u32) = (0x20, 0x7e);
    match value {
        "ascii" => return vec![ASCII],
        "latin-1" => return vec![ASCII, (0xa0, 0xff)],
        _ => {}
    }
    let point = |s: &str| {
        let hex = s.strip_prefix("U+")?;
        if !(4..=6).contains(&hex.len()) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        u32::from_str_radix(hex, 16)
            .ok()
            .filter(|&c| c <= 0x10_ffff)
    };
    let (a, b) = match value.split_once('-') {
        Some((a, b)) => (point(a), point(b)),
        None => (point(value), point(value)),
    };
    match (a, b) {
        (Some(a), Some(b)) if a <= b => vec![(a, b)],
        _ => Vec::new(),
    }
}

/// A `style=` value: regular, bold, italic or bold-italic.
fn style(value: &str) -> Option<Style> {
    let (bold, italic) = match value {
        "regular" => (false, false),
        "bold" => (true, false),
        "italic" => (false, true),
        "bold-italic" => (true, true),
        _ => return None,
    };
    Some(Style { bold, italic })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOLD: Style = Style {
        bold: true,
        italic: false,
    };

    #[test]
    fn a_cover_lines_fit_reaches_its_ranges_only() {
        let fits = FritFits::of(
            "source=font:Main\n\
             source=font:Lines\n\
             cover=U+2500-U+257F\n\
             fit=natural\n\
             cover=U+2580-U+259F\n",
        );
        assert_eq!(fits.fit(0x2500, Style::default()), Fit::Natural);
        assert_eq!(fits.fit(0x257f, Style::default()), Fit::Natural);
        // the second cover= line has no fit= of its own: the default
        assert_eq!(fits.fit(0x2588, Style::default()), Fit::Fill);
        // a character no cover= line names: the default
        assert_eq!(fits.fit(0x2800, Style::default()), Fit::Fill);
        assert_eq!(fits.fit(u32::from('A'), Style::default()), Fit::Natural);
    }

    #[test]
    fn a_sources_own_fit_reaches_its_cover_lines_without_one() {
        let fits = FritFits::of(
            "source=font:Lines\n\
             fit=natural\n\
             cover=U+2500-U+257F\n\
             cover=U+2580-U+259F # blocks\n\
             fit=room\n",
        );
        assert_eq!(fits.fit(0x2500, Style::default()), Fit::Natural);
        assert_eq!(fits.fit(0x2588, Style::default()), Fit::Room);
        assert_eq!(fits.fit(0x28ff, Style::default()), Fit::Fill);
    }

    #[test]
    fn the_first_source_that_covers_a_character_gives_its_fit() {
        let fits = FritFits::of(
            "source=font:A\n\
             cover=U+2500-U+257F\n\
             source=font:B\n\
             cover=U+2500-U+259F\n\
             fit=natural\n",
        );
        // A covers U+2500 and states no fit=, so U+2500 takes its default
        assert_eq!(fits.fit(0x2500, Style::default()), Fit::Fill);
        assert_eq!(fits.fit(0x2588, Style::default()), Fit::Natural);
    }

    #[test]
    fn a_whole_sources_fit_and_a_named_cjk_set_reach_no_range() {
        let fits = FritFits::of(
            "source=font:Main\n\
             fit=natural\n\
             source=font:CJK\n\
             cover=gb2312\n\
             fit=fill\n\
             source=font:Latin\n\
             cover=latin-1\n\
             fit=fill\n",
        );
        assert_eq!(fits.fit(0x2500, Style::default()), Fit::Fill);
        assert_eq!(fits.fit(0x4e00, Style::default()), Fit::Natural);
        assert_eq!(fits.fit(u32::from('A'), Style::default()), Fit::Fill);
        assert_eq!(fits.fit(0xe9, Style::default()), Fit::Fill);
        assert_eq!(fits.fit(0x100, Style::default()), Fit::Natural);
    }

    #[test]
    fn a_style_takes_its_own_sources_fit_before_the_regular_ones() {
        let fits = FritFits::of(
            "source=font:Regular\n\
             cover=U+2500-U+257F\n\
             fit=natural\n\
             source=font:Bold\n\
             style=bold\n\
             cover=U+2500-U+250F\n\
             fit=room\n",
        );
        assert_eq!(fits.fit(0x2500, Style::default()), Fit::Natural);
        assert_eq!(fits.fit(0x2500, BOLD), Fit::Room);
        // the bold source does not cover U+2510, the regular one does
        assert_eq!(fits.fit(0x2510, BOLD), Fit::Natural);
    }

    #[test]
    fn lines_the_helper_refuses_are_left_out() {
        let fits = FritFits::of(
            "fit=natural\n\
             cover=U+2500-U+257F\n\
             source=font:A\n\
             cover=U+25-U+2\n\
             cover=U+2500-U+257F\n\
             fit=snug\n",
        );
        assert_eq!(fits, FritFits::of("source=font:A\ncover=U+2500-U+257F\n"));
        assert_eq!(fits.fit(0x2500, Style::default()), Fit::Fill);
    }
}
