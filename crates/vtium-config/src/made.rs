//! How the glyph helper made a superscript or subscript glyph (ADR-1202
//! D14): which of ADR-1202's ways the engine took for the source that
//! draws the character. The engine records it as it draws
//! ([`crate::serve::Engine::draw_made`]), the helper sends it in a frame of
//! its own ([`crate::serve::FRAME_MADE`]), and the font preview names it in
//! its caption ([`crate::tui::words::script_caption`]).

/// How the engine made one superscript or subscript glyph.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Made {
    /// An outline source's glyph, rasterized at two thirds of the size its
    /// normal glyph is drawn at (D2).
    Outline,
    /// A color glyph, scaled smoothly from its source to two thirds of its
    /// normal size (D5).
    Color,
    /// The source's own font at a smaller whole multiple (D3): its normal
    /// glyphs drawn at `normal` times the font's size, and its script
    /// glyphs at `small` times.
    Multiple { normal: u32, small: u32 },
    /// Another height of a CPI source than `cp-size=` takes (D3): its
    /// glyphs `height` pixels high, drawn at `multiple` times that.
    Height { height: u32, multiple: u32 },
    /// A font the frit names with `script-source=` (D3, D12): the line's
    /// spelling, the font's height in pixels, and the whole multiple it is
    /// drawn at.
    Named {
        spelling: String,
        height: u32,
        multiple: u32,
    },
    /// The source's normal glyph scaled to two thirds, each pixel fully on
    /// or fully off (D4).
    Scaled,
}

/// The most bytes of a `script-source=` spelling a frame carries, which
/// is the most a frit's line holds.
pub const SPELLING_BYTES_MAX: usize = 4096;

/// The most codepoints a frame names.
pub const MADE_MAX: usize = 65_536;

impl Made {
    /// The payload of a [`crate::serve::FRAME_MADE`]: for each codepoint
    /// in `made`, in order, the codepoint u32, the tag u32 (1 outline, 2
    /// color, 3 a smaller multiple, 4 a CPI height, 5 a `script-source=`
    /// font, 6 scaled), two numbers u32 and u32 (the normal and the small
    /// multiple for 3, the height and the multiple for 4 and 5, else 0),
    /// and the spelling's length u32 and its UTF-8 bytes (none but for 5).
    /// Little-endian throughout.
    pub fn encode_all<'a>(made: impl IntoIterator<Item = (&'a u32, &'a Made)>) -> Vec<u8> {
        let mut out = Vec::new();
        for (&c, how) in made.into_iter().take(MADE_MAX) {
            let (tag, a, b, spelling) = match how {
                Made::Outline => (1, 0, 0, ""),
                Made::Color => (2, 0, 0, ""),
                Made::Multiple { normal, small } => (3, *normal, *small, ""),
                Made::Height { height, multiple } => (4, *height, *multiple, ""),
                Made::Named {
                    spelling,
                    height,
                    multiple,
                } => (5, *height, *multiple, spelling.as_str()),
                Made::Scaled => (6, 0, 0, ""),
            };
            let mut end = spelling.len().min(SPELLING_BYTES_MAX);
            while !spelling.is_char_boundary(end) {
                end -= 1;
            }
            let bytes = spelling.as_bytes().get(..end).unwrap_or_default();
            for v in [c, tag, a, b, u32::try_from(bytes.len()).unwrap_or(0)] {
                out.extend_from_slice(&v.to_le_bytes());
            }
            out.extend_from_slice(bytes);
        }
        out
    }

    /// The codepoints and ways a [`crate::serve::FRAME_MADE`]'s payload
    /// names, as [`Made::encode_all`] writes them; `None` for a payload cut
    /// short, past its bounds, or naming a tag, a codepoint or a spelling
    /// that is not one.
    pub fn decode_all(mut payload: &[u8]) -> Option<Vec<(u32, Made)>> {
        let mut out = Vec::new();
        let u32_at = |p: &mut &[u8]| -> Option<u32> {
            let (head, rest) = p.split_at_checked(4)?;
            *p = rest;
            Some(u32::from_le_bytes(head.try_into().ok()?))
        };
        while !payload.is_empty() {
            if out.len() >= MADE_MAX {
                return None;
            }
            let c = u32_at(&mut payload).filter(|&c| char::from_u32(c).is_some())?;
            let (tag, a, b) = (
                u32_at(&mut payload)?,
                u32_at(&mut payload)?,
                u32_at(&mut payload)?,
            );
            let n = usize::try_from(u32_at(&mut payload)?).ok()?;
            if n > SPELLING_BYTES_MAX || (tag != 5 && n != 0) {
                return None;
            }
            let (spelling, rest) = payload.split_at_checked(n)?;
            payload = rest;
            let how = match tag {
                1 => Made::Outline,
                2 => Made::Color,
                3 => Made::Multiple {
                    normal: a,
                    small: b,
                },
                4 => Made::Height {
                    height: a,
                    multiple: b,
                },
                5 => Made::Named {
                    spelling: String::from_utf8(spelling.to_vec()).ok()?,
                    height: a,
                    multiple: b,
                },
                6 => Made::Scaled,
                _ => return None,
            };
            out.push((c, how));
        }
        Some(out)
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::expect_used,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    #[test]
    fn every_way_reads_back_and_a_bad_payload_is_refused() {
        let made: std::collections::BTreeMap<u32, Made> = [
            (0x31, Made::Outline),
            (0x32, Made::Color),
            (
                0x33,
                Made::Multiple {
                    normal: 2,
                    small: 1,
                },
            ),
            (
                0x34,
                Made::Height {
                    height: 8,
                    multiple: 1,
                },
            ),
            (
                0x35,
                Made::Named {
                    spelling: "kbd:ter-112n".to_owned(),
                    height: 12,
                    multiple: 1,
                },
            ),
            (0x36, Made::Scaled),
        ]
        .into_iter()
        .collect();
        let bytes = Made::encode_all(&made);
        let back = Made::decode_all(&bytes).expect("reads back");
        assert_eq!(back, made.into_iter().collect::<Vec<_>>());
        assert_eq!(Made::decode_all(&[]), Some(Vec::new()));
        // cut short, an unknown tag, a spelling on a way that has none
        assert_eq!(Made::decode_all(&bytes[..bytes.len() - 1]), None);
        let mut tag = bytes.clone();
        tag[4] = 9;
        assert_eq!(Made::decode_all(&tag), None);
        let mut spelled = Made::encode_all(
            &[(0x31, Made::Outline)]
                .into_iter()
                .collect::<std::collections::BTreeMap<_, _>>(),
        );
        spelled[16] = 1;
        spelled.push(b'x');
        assert_eq!(Made::decode_all(&spelled), None);
        // a codepoint that is no scalar value
        let mut surrogate = Made::encode_all(
            &[(0x31, Made::Scaled)]
                .into_iter()
                .collect::<std::collections::BTreeMap<_, _>>(),
        );
        surrogate[..4].copy_from_slice(&0xd800u32.to_le_bytes());
        assert_eq!(Made::decode_all(&surrogate), None);
    }
}
