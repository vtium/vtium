//! Which source a key of a font set (a frit) describes, and which sources
//! take a key.
//!
//! A frit's text, and a layer's, is a list of sources. Each source is a
//! `source=` line followed by the keys that describe it, such as `cover=`,
//! `fit=` and `strike=`. A key of that kind describes the last `source=`
//! line above it. A key above the first `source=` line describes nothing,
//! and so does a key after a `layer=` line and before the next `source=`
//! line, because the layer's sources are the layer's own. Some sources take
//! no `strike=` or `script-source=` line at all.
//!
//! Several readers read a frit's lines: vtium-config's frit reader, which
//! the glyph helper runs, the console's reader of `fit=` lines, its test of
//! whether a zoom asks for bitmap strikes, and `vtium check-config`'s check
//! of font files. Each one asks this module which source a key describes,
//! and whether that source takes the key, so they all read the same set.
//! A line that describes nothing is one the frit reader reports and
//! ignores, and the other readers leave it out too.
//!
//! References:
//! - ADR-1510 D2: a key where it does not apply is reported and ignored
//!   alone, and the rest of the set loads.
//! - ADR-1200 D1: the frit's keys, and a source's keys after its line.
//! - ADR-1204 D2: `strike=`, and the sources that take it.
//! - ADR-1204 D5: a `layer=` line and the layer's lines in place of it.

/// What a source's key on a line of a frit describes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Describes<'a> {
    /// The source of the last `source=` line above, by that line's value.
    Source(&'a str),
    /// Nothing, because no `source=` line comes above the key.
    BeforeFirstSource,
    /// Nothing, because a `layer=` line comes after the last `source=`
    /// line. A frit's own `layer=NAME` line and the `layer=` line that
    /// closes a layer's lines in a spliced frit both count.
    AfterLayer,
}

impl Describes<'_> {
    /// Why the key `key` describes nothing here, worded as the frit reader
    /// reports it; `None` when it describes a source.
    pub fn why_nothing(&self, key: &str) -> Option<String> {
        match self {
            Describes::Source(_) => None,
            Describes::BeforeFirstSource => Some(format!(
                "{key}= comes before any source= line, so it has no source to describe."
            )),
            Describes::AfterLayer => Some(format!(
                "{key}= follows a layer= line, which has no source for it to describe. Put {key}= in the layer's file, or after a source= line of the font set."
            )),
        }
    }
}

/// Follows a frit's lines in order, to say what a source's key on each
/// line describes. A reader hands it the key and value of every line that
/// holds a key, in order, and skips blank and comment lines.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CurrentSource<'a> {
    now: Describes<'a>,
}

impl Default for CurrentSource<'_> {
    fn default() -> Self {
        CurrentSource {
            now: Describes::BeforeFirstSource,
        }
    }
}

impl<'a> CurrentSource<'a> {
    /// Reads the line `key=value`, and returns what a source's key on that
    /// line describes. A `source=` line opens its own source, so the
    /// answer for it is that source. A `layer=` line, with a name or
    /// without, ends the source above it. Every other key leaves the
    /// current source as it is.
    pub fn read(&mut self, key: &str, value: &'a str) -> Describes<'a> {
        match key {
            "source" => self.now = Describes::Source(value),
            "layer" => self.now = Describes::AfterLayer,
            _ => {}
        }
        self.now
    }
}

/// Whether the source `source`, a `source=` line's value, takes `strike=`
/// lines: the other heights of its design. A built-in font
/// (`builtin:NAME`) and the system's console font (`vconsole` or
/// `console:VT`) have no design that a frit can name other heights of, so
/// they take none.
pub fn takes_strikes(source: &str) -> bool {
    !(source == "vconsole" || source.starts_with("builtin:") || source.starts_with("console:"))
}

/// Whether the source `source`, a `source=` line's value, takes
/// `script-source=` lines: smaller bitmap fonts for superscript and
/// subscript. An outline font (`outline:PATH`) draws at any size, so it
/// takes none.
pub fn takes_script_sources(source: &str) -> bool {
    !source.starts_with("outline:")
}

/// Whether the source `source`, a `source=` line's value, may be an
/// outline font: an outline file (`outline:PATH`), or a family
/// (`font:NAME`), which the glyph helper finds as an outline font or a
/// console font. Every other source is a bitmap font.
pub fn may_be_outline(source: &str) -> bool {
    source.starts_with("outline:") || source.starts_with("font:")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a source's key describes on each key line of `text`.
    fn walk(text: &str) -> Vec<Describes<'_>> {
        let mut current = CurrentSource::default();
        text.lines()
            .filter_map(|line| line.split_once('='))
            .map(|(k, v)| current.read(k, v))
            .collect()
    }

    #[test]
    fn a_key_describes_the_last_source_above_it() {
        use Describes::{AfterLayer, BeforeFirstSource, Source};
        // above the first source, nothing; each source= line opens its
        // own source; other keys of the set change nothing
        assert_eq!(
            walk(
                "cover=ascii\nsource=font:A\ncover=ascii\ncell=8x16\nfit=room\nsource=kbd:b\nstrike=kbd:c"
            ),
            [
                BeforeFirstSource,
                Source("font:A"),
                Source("font:A"),
                Source("font:A"),
                Source("font:A"),
                Source("kbd:b"),
                Source("kbd:b"),
            ]
        );
        // a layer= line ends the source above it, with a name or without,
        // until the next source= line
        assert_eq!(
            walk(
                "source=font:A\nlayer=sym\nfit=room\nsource=font:S\ncover=U+E000\nlayer=\nstyle=bold\nsource=font:B\nfit=natural"
            ),
            [
                Source("font:A"),
                AfterLayer,
                AfterLayer,
                Source("font:S"),
                Source("font:S"),
                AfterLayer,
                AfterLayer,
                Source("font:B"),
                Source("font:B"),
            ]
        );
        // a layer= line above the first source counts as a layer line
        assert_eq!(walk("layer=sym\nfit=room"), [AfterLayer, AfterLayer]);
    }

    #[test]
    fn a_key_that_describes_nothing_says_why() {
        assert_eq!(Describes::Source("font:A").why_nothing("fit"), None);
        assert_eq!(
            Describes::BeforeFirstSource.why_nothing("cover").as_deref(),
            Some("cover= comes before any source= line, so it has no source to describe.")
        );
        assert_eq!(
            Describes::AfterLayer.why_nothing("fit").as_deref(),
            Some(
                "fit= follows a layer= line, which has no source for it to describe. Put fit= in the layer's file, or after a source= line of the font set."
            )
        );
    }

    #[test]
    fn each_kind_of_source_takes_its_own_keys() {
        // the value of a source= line of each kind, and whether it takes
        // strike=, takes script-source= and may be an outline font
        let kinds = [
            ("kbd:ter-v16n", true, true, false),
            ("file:/f.psf", true, true, false),
            ("bdf:/f.bdf", true, true, false),
            ("hex:/f.hex", true, true, false),
            ("font:Terminus", true, true, true),
            ("outline:/f.ttf", true, false, true),
            ("builtin:unifont", false, true, false),
            ("builtin:unscii-8", false, true, false),
            ("vconsole", false, true, false),
            ("console:3", false, true, false),
        ];
        for (source, strikes, scripts, outline) in kinds {
            assert_eq!(takes_strikes(source), strikes, "{source}");
            assert_eq!(takes_script_sources(source), scripts, "{source}");
            assert_eq!(may_be_outline(source), outline, "{source}");
        }
    }
}
