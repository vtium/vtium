//! The checks the build makes on each face it writes, which `cullet-mono
//! check` also makes on faces built earlier.

use std::path::Path;

use crate::pins::{Pin, Pins};
use crate::recipe::{self, Face};
use crate::sfnt::Font;
use crate::sha256::{Sha256, hex};

/// What one face measures, for the build's record.
#[derive(Clone, Debug)]
pub struct Measure {
    pub bytes: u64,
    pub sha256: String,
    pub characters: usize,
}

/// Checks the bytes of `face` against the recipe and against `pin`, the
/// face's row of `pins.tsv`. Returns what the face measures and every check
/// it fails; an empty list means the face is the one pinned.
pub fn face(face: &Face, bytes: &[u8], pin: &Pin) -> (Measure, Vec<String>) {
    let sha256 = hex(&Sha256::digest(bytes));
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let mut measure = Measure {
        bytes: size,
        sha256: sha256.clone(),
        characters: 0,
    };
    let mut problems = Vec::new();
    let file = face.output;
    match Font::parse(bytes) {
        Err(why) => problems.push(format!("{file}: {why}")),
        Ok(font) => {
            match font.names() {
                Err(why) => problems.push(format!("{file}: {why}")),
                Ok(names) => problems.extend(name_problems(face, &names)),
            }
            problems.extend(reserved_word_elsewhere(file, &font));
            match font.characters() {
                Err(why) => problems.push(format!("{file}: {why}")),
                Ok(chars) => {
                    measure.characters = chars.len();
                    if chars.len() != recipe::CHARACTERS {
                        problems.push(format!(
                            "{file} maps {} characters, not {}",
                            chars.len(),
                            recipe::CHARACTERS
                        ));
                    }
                    let logos: Vec<u32> = chars.range(recipe::FONT_LOGOS).copied().collect();
                    if let (Some(first), Some(last)) = (logos.first(), logos.last()) {
                        problems.push(format!(
                            "{file} maps {} Font Logos characters, which carry no license, from U+{first:04X} to U+{last:04X}",
                            logos.len()
                        ));
                    }
                }
            }
        }
    }
    if size != pin.bytes {
        problems.push(format!("{file} is {size} bytes, not {}", pin.bytes));
    }
    if sha256 != pin.sha256 {
        problems.push(format!(
            "{file} has SHA-256 {sha256}, not {} as pins.tsv pins it (built with {})",
            pin.sha256, pin.from
        ));
    }
    (measure, problems)
}

/// The face's names: the family is "Cullet Mono", the style and full name
/// are the face's own, the copyright notice and the description credit
/// Terminus, and no other name uses the reserved word.
fn name_problems(face: &Face, names: &[crate::sfnt::Name]) -> Vec<String> {
    let file = face.output;
    let mut problems = Vec::new();
    for (id, what) in [
        (0, "copyright notice"),
        (1, "family name"),
        (10, "description"),
    ] {
        if !names.iter().any(|n| n.name_id == id) {
            problems.push(format!("{file} has no {what} (name {id})"));
        }
    }
    for name in names {
        let expected = match name.name_id {
            0 => Some(recipe::COPYRIGHT),
            1 | 16 => Some(recipe::FAMILY),
            2 | 17 => Some(face.style),
            4 => Some(face.name),
            10 => Some(recipe::DESCRIPTION),
            _ => None,
        };
        if let Some(expected) = expected
            && name.text != expected
        {
            problems.push(format!(
                "{file}: name {} on platform {} is {:?}, not {expected:?}",
                name.name_id, name.platform, name.text
            ));
        }
        if !recipe::NAME_IDS_THAT_MAY_MENTION.contains(&name.name_id)
            && name
                .text
                .to_lowercase()
                .contains(&recipe::RESERVED_WORD.to_lowercase())
        {
            problems.push(format!(
                "{file}: name {} on platform {} is {:?}, which uses the reserved name {:?}",
                name.name_id,
                name.platform,
                name.text,
                recipe::RESERVED_WORD
            ));
        }
    }
    problems
}

/// Each table outside the `name` table that holds the reserved word, in
/// ASCII or in UTF-16BE, in any letter case. FontForge's `BDF ` table, for
/// example, names the bitmap strikes' family, once as FAMILY_NAME and once
/// in each strike's XLFD name.
fn reserved_word_elsewhere(file: &str, font: &Font) -> Vec<String> {
    let word = recipe::RESERVED_WORD.to_ascii_lowercase();
    let word = word.as_bytes();
    let mut problems = Vec::new();
    for (tag, table) in font.tables_except(b"name") {
        let ascii = table
            .windows(word.len())
            .any(|w| w.to_ascii_lowercase() == word);
        let utf16 = table.windows(word.len().saturating_mul(2)).any(|w| {
            w.as_chunks::<2>()
                .0
                .iter()
                .zip(word)
                .all(|(&[high, low], &c)| high == 0 && low.to_ascii_lowercase() == c)
        });
        if ascii || utf16 {
            problems.push(format!(
                "{file}: the {} table uses the reserved name {:?}",
                String::from_utf8_lossy(&tag).trim_end(),
                recipe::RESERVED_WORD
            ));
        }
    }
    problems
}

/// `cullet-mono check DIR`: checks each face in `dir` against the recipe
/// and `pins.tsv`.
pub fn dir(dir: &Path) -> Result<(), Vec<String>> {
    let pins = Pins::tree().map_err(|e| vec![e])?;
    let mut problems = Vec::new();
    for f in recipe::FACES {
        let Some(pin) = pins.face(f.output) else {
            problems.push(format!("pins.tsv pins no face {}", f.output));
            continue;
        };
        let path = dir.join(f.output);
        match std::fs::read(&path) {
            Err(e) => problems.push(format!("{}: {e}", path.display())),
            Ok(bytes) => {
                let (m, p) = face(&f, &bytes, pin);
                println!(
                    "{}\t{} bytes\t{} characters\tSHA-256 {}",
                    f.output, m.bytes, m.characters, m.sha256
                );
                problems.extend(p);
            }
        }
    }
    if problems.is_empty() {
        println!("Both faces are the ones pins.tsv pins.");
        Ok(())
    } else {
        Err(problems)
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use crate::sfnt::tests::{font, font_with};

    const REGULAR: Face = recipe::FACES[0];

    fn names(family: &str) -> Vec<(u16, u16, &str)> {
        vec![
            (3, 0, recipe::COPYRIGHT),
            (3, 1, family),
            (3, 2, "Regular"),
            (3, 4, "Cullet Mono"),
            (3, 10, recipe::DESCRIPTION),
            (1, 1, family),
        ]
    }

    fn chars(extra: &[u32]) -> Vec<u32> {
        let n = u32::try_from(recipe::CHARACTERS - extra.len()).unwrap();
        (0x4E00..0x4E00 + n).chain(extra.iter().copied()).collect()
    }

    fn pin_for(bytes: &[u8]) -> Pin {
        Pin {
            file: REGULAR.output.into(),
            bytes: bytes.len() as u64,
            sha256: hex(&Sha256::digest(bytes)),
            from: "fontforge 20251009".into(),
        }
    }

    fn problems(bytes: &[u8], pin: &Pin) -> Vec<String> {
        face(&REGULAR, bytes, pin).1
    }

    #[test]
    fn a_face_that_matches_passes() {
        let bytes = font(&names("Cullet Mono"), &chars(&[]));
        let (m, p) = face(&REGULAR, &bytes, &pin_for(&bytes));
        assert_eq!(p, Vec::<String>::new());
        assert_eq!(m.characters, recipe::CHARACTERS);
    }

    #[test]
    fn a_font_logos_character_fails() {
        let bytes = font(&names("Cullet Mono"), &chars(&[0xF303]));
        let p = problems(&bytes, &pin_for(&bytes));
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(
            p[0].contains("Font Logos") && p[0].contains("U+F303"),
            "{p:?}"
        );
    }

    #[test]
    fn a_reserved_name_as_the_family_fails() {
        let bytes = font(&names("Terminus (TTF)"), &chars(&[]));
        let p = problems(&bytes, &pin_for(&bytes));
        assert!(
            p.iter().any(|p| p.contains("name 1 on platform 3")
                && p.contains("uses the reserved name \"Terminus\"")),
            "{p:?}"
        );
        assert!(p.iter().any(|p| p.contains("not \"Cullet Mono\"")), "{p:?}");
    }

    #[test]
    fn the_bare_word_in_any_name_but_the_attribution_fails() {
        // name 3 is the unique ID, 6 the PostScript name, 16 and 17 the
        // typographic family and style, and 25 the variations prefix
        for id in [1, 2, 3, 4, 6, 16, 17, 25] {
            for (platform, word) in [(3, "Terminus"), (1, "terminus"), (3, "TERMINUS")] {
                let mut n = names("Cullet Mono");
                let text = format!("Cullet {word} Mono");
                n.push((platform, id, &text));
                let bytes = font(&n, &chars(&[]));
                let p = problems(&bytes, &pin_for(&bytes));
                assert!(
                    p.iter()
                        .any(|p| p.contains(&format!("name {id} on platform {platform}"))
                            && p.contains("uses the reserved name")),
                    "name {id}, {word}: {p:?}"
                );
            }
        }
    }

    #[test]
    fn the_attribution_may_name_terminus() {
        for id in [13, 14] {
            let mut n = names("Cullet Mono");
            n.push((3, id, "Terminus Font is licensed under the OFL"));
            let bytes = font(&n, &chars(&[]));
            assert_eq!(problems(&bytes, &pin_for(&bytes)), Vec::<String>::new());
        }
    }

    #[test]
    fn a_face_without_the_attribution_fails() {
        let without: Vec<_> = names("Cullet Mono")
            .into_iter()
            .filter(|n| n.1 != 0 && n.1 != 10)
            .collect();
        let bytes = font(&without, &chars(&[]));
        let p = problems(&bytes, &pin_for(&bytes));
        assert_eq!(p.len(), 2, "{p:?}");
        assert!(
            p[0].contains("no copyright notice") && p[1].contains("no description"),
            "{p:?}"
        );
        let mut other = names("Cullet Mono");
        other[4].2 = "Cullet Mono";
        let bytes = font(&other, &chars(&[]));
        let p = problems(&bytes, &pin_for(&bytes));
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(p[0].contains("name 10 on platform 3"), "{p:?}");
    }

    #[test]
    fn the_bare_word_in_another_table_fails() {
        let utf16: Vec<u8> = "TerMinus"
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect();
        let cases: [(&[u8; 4], &[u8]); 4] = [
            (b"BDF ", b"FAMILY_NAME\0Terminus\0"),
            (
                b"BDF ",
                b"-xos4-TERMINUS-Medium-R-Normal--12-120-72-72-C-60-ISO10646-1\0",
            ),
            (b"PfEd", &utf16),
            (b"post", b"\x08terminus"),
        ];
        for (tag, data) in cases {
            let bytes = font_with(&names("Cullet Mono"), &chars(&[]), &[(*tag, data)]);
            let p = problems(&bytes, &pin_for(&bytes));
            let table = String::from_utf8_lossy(tag);
            assert_eq!(
                p,
                [format!(
                    "{}: the {} table uses the reserved name \"Terminus\"",
                    REGULAR.output,
                    table.trim_end()
                )],
                "{table}"
            );
        }
        let clean = font_with(
            &names("Cullet Mono"),
            &chars(&[]),
            &[(*b"BDF ", b"FAMILY_NAME\0Cullet Mono\0Terminess\0")],
        );
        assert_eq!(problems(&clean, &pin_for(&clean)), Vec::<String>::new());
    }

    #[test]
    fn a_wrong_character_count_fails() {
        let mut c = chars(&[]);
        c.pop();
        let bytes = font(&names("Cullet Mono"), &c);
        let p = problems(&bytes, &pin_for(&bytes));
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(p[0].contains("maps 11637 characters"), "{p:?}");
    }

    #[test]
    fn other_bytes_than_the_pinned_ones_fail() {
        let bytes = font(&names("Cullet Mono"), &chars(&[]));
        let mut pin = pin_for(&bytes);
        pin.sha256 = "0".repeat(64);
        pin.bytes += 1;
        let p = problems(&bytes, &pin);
        assert_eq!(p.len(), 2, "{p:?}");
        assert!(
            p[0].contains("bytes, not") && p[1].contains("as pins.tsv pins it"),
            "{p:?}"
        );
    }
}
