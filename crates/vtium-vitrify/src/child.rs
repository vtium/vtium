//! The parser's child process (ADR-1200 D14): the binary run as
//! `vtium-vitrify --parse`, with an empty environment, `/` as its
//! directory and no descriptor but its three pipes. The child sets
//! no-new-privileges and its limits, refuses to run as root, parses a
//! bitmap font or rasterises an outline one, and writes the glyph form
//! ([`crate::glyphs::Glyphs`]) or the outline form
//! ([`crate::outline::Outline`]), which its parent checks field by field.
//! `serve` sets the same sandbox on itself, with 300 s of CPU
//! (`HELPER_SECONDS`) in place of the child's 30, and parses in its own
//! process, so only the tests start the child; their parent's side is
//! `tests/common/parser.rs`.

use std::io::{Read, Write};

use rustix::process::{Resource, Rlimit, geteuid, setrlimit};

use crate::decode;
use crate::outline::{OUTLINE_BYTES_MAX, OutlineRequest};

/// The argument that makes the binary the parser.
pub const PARSE_ARG: &str = "--parse";

/// The parser's address space: the largest font, decompressed, its glyph
/// form and the map of every codepoint, with room to spare; or the
/// largest outline font, read twice, and its largest answer.
pub const PARSER_MEMORY: u64 = 512 << 20;
/// The parser's CPU time, in seconds: the kernel ends it past this.
pub const PARSER_SECONDS: u64 = 30;
/// How long the parser may take by the wall clock: past it, the parser is
/// killed and its source fails, so `create` never hangs holding the
/// store's lock. Twice its CPU time, for a loaded machine.
pub const PARSER_WALL_SECONDS: u64 = 2 * PARSER_SECONDS;

const REQUEST_MAGIC: &[u8; 8] = b"VTVREQ1\0";
/// The most bytes a request takes: a font and two unimaps, each at most a
/// PSF's bound, or an outline font and what it is drawn at.
const REQUEST_BYTES_MAX: u64 = {
    let bitmap = 8 + 3 * (4 + vtium_font::PSF_BYTES_MAX as u64);
    let outline = OUTLINE_BYTES_MAX + (1 << 20);
    if bitmap > outline { bitmap } else { outline }
};

/// What the parser is given: a font file's bytes and the unimaps it may
/// take.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Request {
    pub font: Vec<u8>,
    /// The frit's `unimap=`.
    pub unimap: Option<Vec<u8>>,
    /// kbd's `def.uni`, for a font with no table and no `unimap=`.
    pub def_uni: Option<Vec<u8>>,
}

impl Request {
    /// The request as the parser reads it.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = REQUEST_MAGIC.to_vec();
        for part in [
            Some(&self.font),
            self.unimap.as_ref(),
            self.def_uni.as_ref(),
        ] {
            match part {
                Some(bytes) => {
                    out.extend_from_slice(
                        &u32::try_from(bytes.len()).unwrap_or(u32::MAX).to_le_bytes(),
                    );
                    out.extend_from_slice(bytes);
                }
                None => out.extend_from_slice(&u32::MAX.to_le_bytes()),
            }
        }
        out
    }

    fn decode(mut bytes: &[u8]) -> Result<Request, &'static str> {
        bytes = bytes
            .strip_prefix(&REQUEST_MAGIC[..])
            .ok_or("not a parse request")?;
        let mut part = || -> Result<Option<Vec<u8>>, &'static str> {
            let (len, rest) = bytes.split_first_chunk::<4>().ok_or("cut short")?;
            let len = u32::from_le_bytes(*len);
            if len == u32::MAX {
                bytes = rest;
                return Ok(None);
            }
            let len = usize::try_from(len).map_err(|_| "cut short")?;
            if len > rest.len() {
                return Err("cut short");
            }
            let (head, tail) = rest.split_at(len);
            bytes = tail;
            Ok(Some(head.to_vec()))
        };
        let font = part()?.ok_or("no font")?;
        let (unimap, def_uni) = (part()?, part()?);
        if !bytes.is_empty() {
            return Err("bytes after the request");
        }
        Ok(Request {
            font,
            unimap,
            def_uni,
        })
    }
}

/// The parser's side: limits set, privilege checked, the request read
/// from stdin and parsed, the glyph form written to stdout. Returns the
/// exit status.
pub fn run_parser() -> u8 {
    match parser_main() {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "{e}");
            1
        }
    }
}

fn parser_main() -> Result<(), String> {
    rustix::thread::set_no_new_privs(true)
        .map_err(|e| format!("the parser cannot set no-new-privileges: {e}"))?;
    for (resource, limit) in [
        (Resource::As, PARSER_MEMORY),
        (Resource::Cpu, PARSER_SECONDS),
        (Resource::Fsize, 0),
        (Resource::Nproc, 0),
        (Resource::Core, 0),
    ] {
        let limit = Rlimit {
            current: Some(limit),
            maximum: Some(limit),
        };
        setrlimit(resource, limit).map_err(|e| format!("the parser cannot set its limits: {e}"))?;
    }
    if geteuid().is_root() {
        return Err("the parser refuses to run as root".to_owned());
    }
    let mut input = Vec::new();
    std::io::stdin()
        .take(REQUEST_BYTES_MAX)
        .read_to_end(&mut input)
        .map_err(|e| format!("the parser cannot read its request: {e}"))?;
    let answer = if input.starts_with(crate::outline::REQUEST_MAGIC) {
        let request = OutlineRequest::decode(&input)
            .map_err(|why| format!("a malformed outline request: {why}"))?;
        drop(input);
        crate::outline::rasterise(&request)?.encode()
    } else {
        let request =
            Request::decode(&input).map_err(|why| format!("a malformed parse request: {why}"))?;
        decode::font(
            &request.font,
            request.unimap.as_deref(),
            request.def_uni.as_deref(),
        )?
        .encode()
    };
    std::io::stdout()
        .write_all(&answer)
        .map_err(|e| format!("the parser cannot answer: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_reads_back() {
        for r in [
            Request {
                font: vec![1, 2, 3],
                unimap: None,
                def_uni: Some(b"0x00 U+0041\n".to_vec()),
            },
            Request {
                font: vec![],
                unimap: Some(vec![]),
                def_uni: None,
            },
        ] {
            assert_eq!(Request::decode(&r.encode()), Ok(r));
        }
        let mut long = Request::default().encode();
        long.push(0);
        assert!(Request::decode(&long).is_err());
        assert!(Request::decode(b"VTVREQ1\0\x05\0\0\0ab").is_err());
    }
}
