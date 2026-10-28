//! The corpus's synthetic cases, written from their recipes: each case a
//! byte stream of hand-written escape sequences that exercises one area
//! of terminal behaviour, with the cuts of a case that has them and the
//! fixtures the replay serves it.
//!
//! The streams are committed under `corpus/cases/`, copied from the C
//! reference, whose `test/conformance/gen-synthetic.py` wrote them; this
//! generator writes every one of them byte for byte, which
//! `tests/generate.rs` checks. A new case is added here, written with
//! `conformance generate DIR`, and its goldens reviewed as their own
//! change. The streams a client recorded, and the sixel strings libsixel's
//! `img2sixel` wrote, are inputs, copied under `generator/` with their
//! provenance.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

/// `ESC _ tsm-test;CMD;K=V... ESC \`: a test command of the replay
/// interface, its keys in the order given.
macro_rules! tt {
    ($cmd:expr $(, $k:tt = $v:expr)* $(,)?) => {
        $crate::generate::test_command($cmd, &[$((stringify!($k), ($v).to_string())),*])
    };
}

mod host;
mod images;
mod kitty;
mod sixel;
mod text;

const ESC: &str = "\x1b";
const CSI: &str = "\x1b[";
const OSC: &str = "\x1b]";
const DCS: &str = "\x1bP";
const APC: &str = "\x1b_";
const ST: &str = "\x1b\\";
/// A cut in a case's text: where the stream is cut, written to its
/// `.cuts` file and dropped from the stream.
const CUT: &str = "\0tsm-cut\0";
/// DSR 6, which asks for the cursor, so a case's replies record where it
/// went.
const DSR: &str = "\x1b[6n";

/// The files the generator reads, under its input directory.
const INPUTS: [&str; 6] = [
    "recorded/icat-detect.bin",
    "recorded/icat-gif-frames.bin",
    "recorded/mpv-shm-frames.bin",
    "sixel/gradient.six",
    "sixel/highcolor.six",
    "sixel/palette16.six",
];

/// The generator's inputs: the streams a client wrote in the C
/// reference's probes, and `img2sixel`'s sixel strings.
pub struct Inputs {
    files: BTreeMap<&'static str, Vec<u8>>,
}

impl Inputs {
    /// Reads every input under `dir`, refusing a missing one.
    pub fn read(dir: &Path) -> Result<Self, String> {
        let mut files = BTreeMap::new();
        for name in INPUTS {
            let path = dir.join(name);
            let data = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            files.insert(name, data);
        }
        Ok(Self { files })
    }

    /// An input's bytes, each read as the character of the same number,
    /// as the C reference's generator read them (Latin-1): a byte past
    /// 0x7f is written back as two bytes of UTF-8.
    fn latin1(&self, name: &str) -> String {
        self.files
            .get(name)
            .map(|data| data.iter().map(|&b| char::from(b)).collect())
            .unwrap_or_default()
    }
}

/// What a case's recipe makes.
enum Stream {
    /// Text, written as UTF-8, with [`CUT`] where it is cut.
    Text(String),
    /// Bytes as they are, ill-formed UTF-8 among them.
    Bytes(Vec<u8>),
}

/// A recipe's stream and the fixtures the replay's host serves the case.
struct Made {
    stream: Stream,
    files: BTreeMap<String, Vec<u8>>,
}

impl From<String> for Made {
    fn from(text: String) -> Self {
        Self {
            stream: Stream::Text(text),
            files: BTreeMap::new(),
        }
    }
}

impl Made {
    fn with_files(text: String, files: &[(&str, Vec<u8>)]) -> Self {
        Self {
            stream: Stream::Text(text),
            files: files
                .iter()
                .map(|(name, data)| ((*name).to_owned(), data.clone()))
                .collect(),
        }
    }
}

type Recipe = fn(&Inputs) -> Made;

/// A case: its name, the screen it replays on, and its recipe.
#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    cols: u16,
    rows: u16,
    recipe: Recipe,
}

/// A case at 80x24.
const fn case(name: &'static str, recipe: Recipe) -> Case {
    sized(name, 80, 24, recipe)
}

const fn sized(name: &'static str, cols: u16, rows: u16, recipe: Recipe) -> Case {
    Case {
        name,
        cols,
        rows,
        recipe,
    }
}

/// One case as written: its stream, its cuts, and its fixtures by name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Generated {
    pub name: String,
    pub cols: u16,
    pub rows: u16,
    pub stream: Vec<u8>,
    pub cuts: Vec<usize>,
    pub files: BTreeMap<String, Vec<u8>>,
}

impl Generated {
    /// The `.cuts` file, one offset a line, for a case with cuts.
    pub fn cuts_file(&self) -> Option<String> {
        (!self.cuts.is_empty()).then(|| self.cuts.iter().map(|c| format!("{c}\n")).collect())
    }

    fn new(name: String, cols: u16, rows: u16, made: Made) -> Self {
        let (stream, cuts) = match made.stream {
            Stream::Text(text) => {
                let mut stream = Vec::new();
                let mut cuts = Vec::new();
                for (n, part) in text.split(CUT).enumerate() {
                    if n > 0 {
                        cuts.push(stream.len());
                    }
                    stream.extend_from_slice(part.as_bytes());
                }
                (stream, cuts)
            }
            Stream::Bytes(bytes) => (bytes, Vec::new()),
        };
        Self {
            name,
            cols,
            rows,
            stream,
            cuts,
            files: made.files,
        }
    }
}

/// Every case, in the order `cases.list` gives them: each `kit-host-` case
/// again as `-nohook`, with the host removed and no fixtures, so its file
/// media answer EBADF and its inline PNG and zlib EINVAL, with no request.
pub fn cases(inputs: &Inputs) -> Vec<Generated> {
    let all: Vec<Case> = [
        text::CASES,
        images::CASES,
        sixel::CASES,
        kitty::CASES,
        host::CASES,
    ]
    .concat();
    let mut out: Vec<Generated> = all
        .iter()
        .map(|c| Generated::new(c.name.to_owned(), c.cols, c.rows, (c.recipe)(inputs)))
        .collect();
    for c in all.iter().filter(|c| c.name.starts_with("kit-host-")) {
        let made = match (c.recipe)(inputs).stream {
            Stream::Text(text) => tt!("host", on = 0) + &text,
            Stream::Bytes(_) => continue,
        };
        out.push(Generated::new(
            format!("{}-nohook", c.name),
            c.cols,
            c.rows,
            made.into(),
        ));
    }
    out
}

/// Writes each case under `dir`: `<name>.bin`, `<name>.cuts` for a case
/// with cuts, and its fixtures in `<name>.files/`. Returns the lines
/// `cases.list` takes, `<name> <cols> <rows>`.
pub fn write(dir: &Path, cases: &[Generated]) -> Result<String, String> {
    let put = |path: &Path, data: &[u8]| {
        std::fs::write(path, data).map_err(|e| format!("{}: {e}", path.display()))
    };
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut list = String::new();
    for case in cases {
        put(&dir.join(format!("{}.bin", case.name)), &case.stream)?;
        if let Some(cuts) = case.cuts_file() {
            put(&dir.join(format!("{}.cuts", case.name)), cuts.as_bytes())?;
        }
        if !case.files.is_empty() {
            let files = dir.join(format!("{}.files", case.name));
            std::fs::create_dir_all(&files).map_err(|e| format!("{}: {e}", files.display()))?;
            for (name, data) in &case.files {
                put(&files.join(name), data)?;
            }
        }
        let _ = writeln!(list, "{} {} {}", case.name, case.cols, case.rows);
    }
    Ok(list)
}

/// The text of a test command, for [`tt!`].
pub(crate) fn test_command(cmd: &str, keys: &[(&str, String)]) -> String {
    let mut out = format!("{APC}tsm-test;{cmd}");
    for (k, v) in keys {
        let _ = write!(out, ";{k}={v}");
    }
    out + ST
}

/// SGR with `params`, `;` between them.
fn sgr(params: &[u32]) -> String {
    let params: Vec<String> = params.iter().map(u32::to_string).collect();
    format!("{CSI}{}m", params.join(";"))
}

/// CUP to `row`, `col`, from 1.
fn cup(row: u32, col: u32) -> String {
    format!("{CSI}{row};{col}H")
}

/// The test command that sets the cell size.
fn cell(w: u32, h: u32) -> String {
    tt!("cell", w = w, h = h)
}

/// `data` in base64, with padding.
fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let digit = |n: u32| char::from(ALPHABET[(n & 63) as usize]);
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0_u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        out.push(digit(n >> 18));
        out.push(digit(n >> 12));
        out.push(if chunk.len() > 1 { digit(n >> 6) } else { '=' });
        out.push(if chunk.len() > 2 { digit(n) } else { '=' });
    }
    out
}

/// The hex of `text`'s UTF-8, lowercase.
fn hex(text: &str) -> String {
    text.bytes().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_pads_each_length() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(&[0xff, 0, 0xff]), "/wD/");
    }

    #[test]
    fn a_test_command_keeps_its_keys_in_order() {
        assert_eq!(
            tt!("focus", in = 1, n = "x"),
            "\x1b_tsm-test;focus;in=1;n=x\x1b\\"
        );
        assert_eq!(tt!("sync-frames"), "\x1b_tsm-test;sync-frames\x1b\\");
    }

    #[test]
    fn cuts_split_the_text_and_leave_it() {
        let g = Generated::new("c".into(), 1, 1, format!("ab{CUT}\u{e9}{CUT}c").into());
        assert_eq!(g.stream, "ab\u{e9}c".as_bytes());
        assert_eq!(g.cuts, [2, 4]);
        assert_eq!(g.cuts_file().as_deref(), Some("2\n4\n"));
    }
}
