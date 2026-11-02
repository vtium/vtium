//! The fixture host (INTERFACE.md, "The fixture host"): it answers each
//! request a handler makes from the case's fixture files at once, so no
//! case depends on time or on a helper (ADR-1100, D7).

use core::fmt::Write;
use std::path::{Path, PathBuf};

use vtium_term::images::decode::{Medium, Payload, Reply, Request};

/// The fixture host of one replay.
#[derive(Clone, Debug)]
pub struct Host {
    fixtures: PathBuf,
    /// Requests answered so far.
    count: u64,
    /// Direct requests answered so far.
    direct: u64,
}

impl Host {
    pub fn new(fixtures: &Path) -> Self {
        Self {
            fixtures: fixtures.to_path_buf(),
            count: 0,
            direct: 0,
        }
    }

    /// Answers `request`: the `host:` line it prints, and the outcome.
    pub fn answer(&mut self, request: &Request) -> (String, Reply) {
        self.count = self.count.saturating_add(1);
        let direct = matches!(request.medium, Medium::Direct(_));
        if direct {
            self.direct = self.direct.saturating_add(1);
        }
        let line = self.line(request);
        let name = if direct {
            format!("direct-{}", self.direct)
        } else {
            let name = request.medium.name();
            let last = name.rsplit(|&b| b == b'/').next().unwrap_or(name);
            String::from_utf8_lossy(last).into_owned()
        };
        (line, self.outcome(request, &name))
    }

    fn line(&self, r: &Request) -> String {
        let named = !matches!(r.medium, Medium::Direct(_));
        let flags: Vec<&str> = [
            (r.inflate, "inflate"),
            (r.png, "png"),
            (r.exact, "exact"),
            (r.discard, "discard"),
        ]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect();
        let flags = if flags.is_empty() {
            "-".to_owned()
        } else {
            flags.join(",")
        };
        let mut name = String::new();
        for &b in r.medium.name() {
            if (0x21..=0x7e).contains(&b) && b != b'\\' {
                name.push(char::from(b));
            } else {
                let _ = write!(name, "\\x{b:02x}");
            }
        }
        let len = match &r.medium {
            Medium::Direct(bytes) => bytes.len(),
            _ => 0,
        };
        let (offset, max) = if named { (r.offset, r.max) } else { (0, 0) };
        format!(
            "host: {} t={} flags={flags} O={offset} max={max} min={} name={name} len={len}",
            self.count,
            r.medium.letter(),
            r.min
        )
    }

    fn read(&self, name: &str) -> Option<Vec<u8>> {
        // a fixture name the request gave is never a path out of the directory
        if name.is_empty() || name.starts_with('.') {
            return None;
        }
        std::fs::read(self.fixtures.join(name)).ok()
    }

    fn outcome(&self, r: &Request, name: &str) -> Reply {
        if r.discard {
            return Reply::Ok(Payload::Bytes(Vec::new()));
        }
        if let Some(text) = self.read(&format!("{name}.error")) {
            let word = text
                .split(|b| b.is_ascii_whitespace())
                .find(|w| !w.is_empty())
                .unwrap_or_default();
            return match word {
                b"UNREADABLE" => Reply::Unreadable,
                b"TOO_SHORT" => Reply::TooShort,
                b"CORRUPT" => Reply::Corrupt,
                b"TOO_LARGE" => Reply::TooLarge,
                b"OVER_BUDGET" => Reply::OverBudget,
                _ => Reply::Failed,
            };
        }
        let min = usize::try_from(r.min).unwrap_or(usize::MAX);
        if r.png {
            if let Some(data) = self.read(&format!("{name}.rgba")) {
                let word = |i: usize| {
                    data.get(i..i.saturating_add(4))
                        .and_then(|b| <[u8; 4]>::try_from(b).ok())
                        .map(u32::from_le_bytes)
                };
                let (Some(width), Some(height)) = (word(0), word(4)) else {
                    return Reply::Corrupt;
                };
                let data = data.get(8..).unwrap_or_default().to_vec();
                return Reply::Ok(Payload::Rgba {
                    width,
                    height,
                    data,
                });
            }
        } else if r.inflate {
            if let Some(bytes) = self.read(&format!("{name}.bytes")) {
                return if bytes.len() < min {
                    Reply::TooShort
                } else if r.exact && bytes.len() > min {
                    Reply::Corrupt
                } else {
                    Reply::Ok(Payload::Bytes(
                        bytes.get(..min).unwrap_or_default().to_vec(),
                    ))
                };
            }
        } else if let Some(data) = self.read(name) {
            let offset = usize::try_from(r.offset).unwrap_or(usize::MAX);
            if offset >= data.len() {
                return Reply::Unreadable;
            }
            let rest = data.get(offset..).unwrap_or_default();
            let max = usize::try_from(r.max).unwrap_or(usize::MAX);
            let read = if max == 0 {
                rest
            } else {
                rest.get(..max.min(rest.len())).unwrap_or(rest)
            };
            return if read.len() < min {
                Reply::TooShort
            } else {
                Reply::Ok(Payload::Bytes(read.get(..min).unwrap_or_default().to_vec()))
            };
        }
        match r.medium {
            Medium::Direct(_) => Reply::Corrupt,
            _ => Reply::Unreadable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(medium: Medium) -> Request {
        Request {
            medium,
            offset: 0,
            max: 0,
            min: 0,
            inflate: false,
            png: false,
            exact: false,
            discard: false,
            largest: (10_000, 10_000),
            largest_bytes: 1 << 30,
        }
    }

    fn fixtures(name: &str, files: &[(&str, &[u8])]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "vtium-conformance-host-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap_or_default();
        for (file, data) in files {
            std::fs::write(dir.join(file), data).unwrap_or_default();
        }
        dir
    }

    #[test]
    fn each_request_prints_its_line_and_takes_its_fixture() {
        let dir = fixtures(
            "line",
            &[
                ("r1", b"0123456789"),
                ("r2.error", b"TOO_LARGE\n"),
                ("direct-1.bytes", b"abcdefgh"),
            ],
        );
        let mut host = Host::new(&dir);
        let file = Request {
            offset: 5,
            max: 9,
            min: 3,
            ..request(Medium::File(b"/tmp/r1".to_vec()))
        };
        let (line, reply) = host.answer(&file);
        assert_eq!(
            line,
            "host: 1 t=f flags=- O=5 max=9 min=3 name=/tmp/r1 len=0"
        );
        assert_eq!(reply, Reply::Ok(Payload::Bytes(b"567".to_vec())));
        let (_, reply) = host.answer(&request(Medium::SharedMemory(b"r2".to_vec())));
        assert_eq!(reply, Reply::TooLarge);
        let inflate = Request {
            inflate: true,
            min: 4,
            ..request(Medium::Direct(vec![1, 2]))
        };
        let (line, reply) = host.answer(&inflate);
        assert_eq!(
            line,
            "host: 3 t=d flags=inflate O=0 max=0 min=4 name= len=2"
        );
        assert_eq!(reply, Reply::Ok(Payload::Bytes(b"abcd".to_vec())));
        let exact = Request {
            exact: true,
            ..inflate
        };
        assert_eq!(
            host.answer(&exact).1,
            Reply::Corrupt,
            "direct-2 has no fixture"
        );
        let odd = request(Medium::File(b"/a b\\c".to_vec()));
        let (line, reply) = host.answer(&odd);
        assert!(line.ends_with("name=/a\\x20b\\x5cc len=0"), "{line}");
        assert_eq!(reply, Reply::Unreadable);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_png_request_reads_the_decoded_picture_and_a_discard_reads_nothing() {
        let mut rgba = Vec::new();
        rgba.extend_from_slice(&1u32.to_le_bytes());
        rgba.extend_from_slice(&2u32.to_le_bytes());
        rgba.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let dir = fixtures("png", &[("p.rgba", &rgba)]);
        let mut host = Host::new(&dir);
        let png = Request {
            png: true,
            ..request(Medium::TempFile(b"/tmp/p".to_vec()))
        };
        let want = Reply::Ok(Payload::Rgba {
            width: 1,
            height: 2,
            data: vec![1, 2, 3, 4, 5, 6, 7, 8],
        });
        assert_eq!(host.answer(&png).1, want);
        let discard = Request {
            discard: true,
            ..request(Medium::TempFile(b"/tmp/p".to_vec()))
        };
        let (line, reply) = host.answer(&discard);
        assert!(line.contains("flags=discard"), "{line}");
        assert_eq!(reply, Reply::Ok(Payload::Bytes(Vec::new())));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
