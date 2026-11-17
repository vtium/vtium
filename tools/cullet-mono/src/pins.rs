//! `pins.tsv`: each source the build fetches, and each face it must
//! produce, with its size and SHA-256.

/// One row of `pins.tsv`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pin {
    /// The file's name: in the source cache for a source, in the output
    /// folder for a face.
    pub file: String,
    /// The file's size in bytes.
    pub bytes: u64,
    /// The SHA-256 of the file's bytes, as 64 lower-case hex digits.
    pub sha256: String,
    /// Where the file comes from: the URL of a source, or the FontForge
    /// version that built a face.
    pub from: String,
}

/// The rows of `pins.tsv`, split by kind.
#[derive(Clone, Debug, Default)]
pub struct Pins {
    pub sources: Vec<Pin>,
    pub faces: Vec<Pin>,
}

/// The pins as the tree holds them.
pub const PINS: &str = include_str!("../pins.tsv");

impl Pins {
    /// Reads `text` in the form of `pins.tsv`: comment lines start with
    /// `#`, and every other line holds five tab-separated fields.
    pub fn parse(text: &str) -> Result<Pins, String> {
        let mut pins = Pins::default();
        for (n, line) in text.lines().enumerate() {
            let n = n.saturating_add(1);
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = line.split('\t').collect();
            let [kind, file, bytes, sha256, from] = fields[..] else {
                return Err(format!(
                    "pins.tsv line {n} has {} fields, not 5",
                    fields.len()
                ));
            };
            let bytes = bytes
                .parse()
                .map_err(|_| format!("pins.tsv line {n}: the size {bytes:?} is not a number"))?;
            if sha256.len() != 64
                || !sha256
                    .bytes()
                    .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
            {
                return Err(format!(
                    "pins.tsv line {n}: {sha256:?} is not 64 lower-case hex digits"
                ));
            }
            let pin = Pin {
                file: file.to_owned(),
                bytes,
                sha256: sha256.to_owned(),
                from: from.to_owned(),
            };
            match kind {
                "source" => pins.sources.push(pin),
                "face" => pins.faces.push(pin),
                _ => return Err(format!("pins.tsv line {n}: unknown kind {kind:?}")),
            }
        }
        Ok(pins)
    }

    /// The pins the tree holds.
    pub fn tree() -> Result<Pins, String> {
        Pins::parse(PINS)
    }

    /// The face pinned under `file`.
    pub fn face(&self, file: &str) -> Option<&Pin> {
        self.faces.iter().find(|p| p.file == file)
    }

    /// The source pinned under `file`.
    pub fn source(&self, file: &str) -> Option<&Pin> {
        self.sources.iter().find(|p| p.file == file)
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use crate::recipe;

    #[test]
    fn the_tree_pins_every_source_and_face_the_recipe_names() {
        let pins = Pins::tree().unwrap();
        for face in recipe::FACES {
            assert!(
                pins.face(face.output).is_some(),
                "{} is not pinned",
                face.output
            );
        }
        assert!(pins.source(recipe::TERMINUS_ARCHIVE).is_some());
        assert!(pins.source(recipe::PATCHER_ARCHIVE).is_some());
        assert_eq!(pins.faces.len(), recipe::FACES.len());
        assert_eq!(pins.sources.len(), 2);
        for source in &pins.sources {
            assert!(source.from.starts_with("https://"), "{}", source.from);
        }
    }

    #[test]
    fn a_malformed_row_is_refused() {
        let good = "a".repeat(64);
        assert!(Pins::parse(&format!("source\tf\t1\t{good}\thttps://x\n")).is_ok());
        assert!(Pins::parse(&format!("source\tf\t1\t{good}\n")).is_err());
        assert!(Pins::parse(&format!("source\tf\tone\t{good}\thttps://x\n")).is_err());
        assert!(Pins::parse("source\tf\t1\tABC\thttps://x\n").is_err());
        assert!(Pins::parse(&format!("font\tf\t1\t{good}\thttps://x\n")).is_err());
    }
}
