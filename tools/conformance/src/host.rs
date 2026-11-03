//! The fixture host (INTERFACE.md, "The fixture host"): it answers each
//! request a handler makes from the case's fixture files at once, so no
//! case depends on time or on a helper (ADR-1100, D7).
//!
//! It answers as the image helper does, with every picture premultiplied
//! (ADR-1101 D7): a `.rgba` fixture's straight pixels, and the raw pixels
//! a request describes, go through vtium-image's one conversion, so tier 1
//! reads each reply through the console's path for the helper's replies.

use core::fmt::Write;
use std::path::{Path, PathBuf};

use vtium_term::images::decode::convert::{self, Alpha, Layout};
use vtium_term::images::decode::{FrameData, Medium, Payload, Reply, Request};

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
            (r.any_image, "any"),
            (r.frames.is_some(), "frames"),
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
            return Reply::Ok(Payload::Discarded);
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
        if r.any_image {
            return self.any_image(r, name);
        }
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
                // a helper reads the picture's header and refuses it before
                // decoding, as ADR-1100 D7 asks: past the largest picture,
                // then past the most bytes
                if width > r.largest.0 || height > r.largest.1 {
                    return Reply::TooLarge;
                }
                let bytes = u64::from(width)
                    .saturating_mul(u64::from(height))
                    .saturating_mul(4);
                if bytes > r.largest_bytes {
                    return Reply::OverBudget;
                }
                let mut pixels = Vec::new();
                let straight = data.get(8..).unwrap_or_default();
                // a fixture of another length than its picture is a
                // helper's lie, which the console's check fails
                convert::to_reply(Layout::Rgba, Alpha::Straight, straight, &mut pixels);
                return Reply::Ok(Payload::Rgba {
                    width,
                    height,
                    data: pixels,
                });
            }
        } else if r.inflate {
            if let Some(bytes) = self.read(&format!("{name}.bytes")) {
                return if bytes.len() < min {
                    Reply::TooShort
                } else if r.exact && bytes.len() > min {
                    Reply::Corrupt
                } else {
                    described(r, bytes.get(..min).unwrap_or_default())
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
                described(r, read.get(..min).unwrap_or_default())
            };
        }
        match r.medium {
            Medium::Direct(_) => Reply::Corrupt,
            _ => Reply::Unreadable,
        }
    }
}

impl Host {
    /// An any-image request, as the helper answers it from its decoders: a
    /// still picture from `NAME.rgba`, as for PNG, or an animated GIF's
    /// frames from `NAME.frames`, its first frame alone when the request
    /// asked for none or the frames pass its bounds; `CORRUPT` with neither.
    fn any_image(&self, r: &Request, name: &str) -> Reply {
        let (data, animated) = match self.read(&format!("{name}.frames")) {
            Some(data) => (data, true),
            None => match self.read(&format!("{name}.rgba")) {
                Some(data) => (data, false),
                None => return Reply::Corrupt,
            },
        };
        let word = |i: usize| {
            data.get(i..i.saturating_add(4))
                .and_then(|b| <[u8; 4]>::try_from(b).ok())
                .map(u32::from_le_bytes)
        };
        let (Some(width), Some(height)) = (word(0), word(4)) else {
            return Reply::Corrupt;
        };
        if width > r.largest.0 || height > r.largest.1 {
            return Reply::TooLarge;
        }
        let frame = u64::from(width)
            .saturating_mul(u64::from(height))
            .saturating_mul(4);
        if frame > r.largest_bytes {
            return Reply::OverBudget;
        }
        let frame_len = usize::try_from(frame).unwrap_or(usize::MAX);
        let still = |straight: &[u8]| {
            let mut data = Vec::new();
            convert::to_reply(Layout::Rgba, Alpha::Straight, straight, &mut data);
            Reply::Ok(Payload::Rgba {
                width,
                height,
                data,
            })
        };
        if !animated {
            return still(data.get(8..).unwrap_or_default());
        }
        // NAME.frames: width, height, the frame count and the loop count as
        // u32, the loop count u32::MAX for none, a u16 delay for each
        // frame, then the frames' straight RGBA one after another
        let (Some(count), Some(loops)) = (word(8), word(12)) else {
            return Reply::Corrupt;
        };
        let count = usize::try_from(count).unwrap_or(usize::MAX);
        let at = 16usize.saturating_add(count.saturating_mul(2));
        let delays: Vec<u16> = data
            .get(16..at)
            .unwrap_or_default()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&b| u16::from_le_bytes(b))
            .collect();
        let pixels = data.get(at..).unwrap_or_default();
        if delays.len() != count || count == 0 {
            return Reply::Corrupt;
        }
        let past = frame.saturating_mul(u64::try_from(count - 1).unwrap_or(u64::MAX));
        let fits = r.frames.is_some_and(|l| {
            u64::try_from(count).is_ok_and(|c| c <= u64::from(l.max_frames))
                && past <= l.max_frame_bytes
        });
        if count == 1 || !fits {
            return still(pixels.get(..frame_len).unwrap_or(pixels));
        }
        let mut out = Vec::new();
        convert::to_reply(Layout::Rgba, Alpha::Straight, pixels, &mut out);
        Reply::Ok(Payload::Frames {
            width,
            height,
            delays,
            loops: u16::try_from(loops).ok(),
            data: FrameData::Memory(out),
        })
    }
}

/// The reply to `r` for its raw pixels `bytes`, converted as its
/// description says, as the helper converts them.
fn described(r: &Request, bytes: &[u8]) -> Reply {
    let Some(d) = r.pixels else {
        return Reply::Failed;
    };
    let mut data = Vec::new();
    convert::to_reply(d.layout, d.alpha, bytes, &mut data);
    Reply::Ok(Payload::Rgba {
        width: d.width,
        height: d.height,
        data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use vtium_term::images::decode::{Description, FrameLimits};

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
            any_image: false,
            frames: None,
            pixels: None,
            largest: (10_000, 10_000),
            largest_bytes: 1 << 30,
        }
    }

    /// A directory of fixtures of a test's own, removed when the test
    /// ends, whether it passes or fails.
    struct Fixtures(PathBuf);

    impl std::ops::Deref for Fixtures {
        type Target = std::path::Path;

        fn deref(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for Fixtures {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn fixtures(name: &str, files: &[(&str, &[u8])]) -> Fixtures {
        let dir = std::env::temp_dir().join(format!(
            "vtium-conformance-host-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap_or_default();
        for (file, data) in files {
            std::fs::write(dir.join(file), data).unwrap_or_default();
        }
        Fixtures(dir)
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
        let rgb = Description {
            layout: Layout::Rgb,
            alpha: Alpha::Straight,
            width: 1,
            height: 1,
            compressed: false,
        };
        let file = Request {
            offset: 5,
            max: 9,
            min: 3,
            pixels: Some(rgb),
            ..request(Medium::File(b"/tmp/r1".to_vec()))
        };
        let (line, reply) = host.answer(&file);
        assert_eq!(
            line,
            "host: 1 t=f flags=- O=5 max=9 min=3 name=/tmp/r1 len=0"
        );
        let picture = |data: &[u8]| {
            Reply::Ok(Payload::Rgba {
                width: 1,
                height: 1,
                data: data.to_vec(),
            })
        };
        assert_eq!(reply, picture(b"567\xff"));
        let (_, reply) = host.answer(&request(Medium::SharedMemory(b"r2".to_vec())));
        assert_eq!(reply, Reply::TooLarge);
        let inflate = Request {
            inflate: true,
            min: 4,
            pixels: Some(Description {
                layout: Layout::Rgba,
                compressed: true,
                ..rgb
            }),
            ..request(Medium::Direct(vec![1, 2]))
        };
        let (line, reply) = host.answer(&inflate);
        assert_eq!(
            line,
            "host: 3 t=d flags=inflate O=0 max=0 min=4 name= len=2"
        );
        // a, b, c at alpha d, 100: (97 × 100 + 127) / 255 is 38
        assert_eq!(reply, picture(&[38, 38, 39, 100]));
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
    }

    #[test]
    fn an_any_image_request_takes_a_still_picture_or_an_animations_frames() {
        // two frames of 1 by 1, loops forever, delays 3 and 4
        let mut frames = Vec::new();
        for v in [1u32, 1, 2, 0] {
            frames.extend_from_slice(&v.to_le_bytes());
        }
        frames.extend_from_slice(&[3, 0, 4, 0]);
        frames.extend_from_slice(&[1, 2, 3, 0xff, 4, 5, 6, 0xff]);
        let dir = fixtures(
            "any",
            &[
                ("direct-1.rgba", &picture(1, 1)),
                ("direct-2.frames", &frames),
            ],
        );
        let mut host = Host::new(&dir);
        let any = Request {
            any_image: true,
            ..request(Medium::Direct(vec![0; 3]))
        };
        let (line, reply) = host.answer(&any);
        assert_eq!(line, "host: 1 t=d flags=any O=0 max=0 min=0 name= len=3");
        assert_eq!(
            reply.checked(&any),
            Reply::Ok(Payload::Rgba {
                width: 1,
                height: 1,
                data: vec![0; 4],
            })
        );
        let gif = Request {
            frames: Some(FrameLimits {
                max_frames: 2,
                max_frame_bytes: 4,
            }),
            ..any.clone()
        };
        let (line, reply) = host.answer(&gif);
        assert!(line.contains("flags=any,frames"), "{line}");
        assert_eq!(
            reply.clone().checked(&gif),
            Reply::Ok(Payload::Frames {
                width: 1,
                height: 1,
                delays: vec![3, 4],
                loops: Some(0),
                data: FrameData::Memory(vec![1, 2, 3, 0xff, 4, 5, 6, 0xff]),
            })
        );
        // the third request reads direct-3, which has no fixture
        assert_eq!(host.answer(&gif).1, Reply::Corrupt);
        // frames a request did not ask for, or past its bounds: the first
        let mut host = Host::new(&dir);
        let _ = host.answer(&any);
        let tight = Request {
            frames: Some(FrameLimits {
                max_frames: 2,
                max_frame_bytes: 3,
            }),
            ..any
        };
        assert_eq!(
            host.answer(&tight).1,
            Reply::Ok(Payload::Rgba {
                width: 1,
                height: 1,
                data: vec![1, 2, 3, 0xff],
            })
        );
    }

    /// A fixture picture of `width` by `height`, its pixels all 0.
    fn picture(width: u32, height: u32) -> Vec<u8> {
        let mut rgba = Vec::new();
        rgba.extend_from_slice(&width.to_le_bytes());
        rgba.extend_from_slice(&height.to_le_bytes());
        rgba.resize(8 + (width * height * 4) as usize, 0);
        rgba
    }

    #[test]
    fn a_picture_past_the_requests_largest_is_refused_as_a_helper_refuses_it() {
        let dir = fixtures(
            "largest",
            &[
                ("wide.rgba", &picture(5, 1)),
                ("tall.rgba", &picture(1, 5)),
                ("heavy.rgba", &picture(3, 3)),
            ],
        );
        let mut host = Host::new(&dir);
        let png = |name: &[u8]| Request {
            png: true,
            largest: (4, 4),
            largest_bytes: 32,
            ..request(Medium::File(name.to_vec()))
        };
        assert_eq!(host.answer(&png(b"/wide")).1, Reply::TooLarge);
        assert_eq!(host.answer(&png(b"/tall")).1, Reply::TooLarge);
        // 3 by 3 fits each side but its 36 bytes pass the 32 allowed
        assert_eq!(host.answer(&png(b"/heavy")).1, Reply::OverBudget);
    }

    #[test]
    fn a_picture_exactly_at_each_bound_is_answered() {
        let dir = fixtures(
            "bound",
            &[
                ("side.rgba", &picture(4, 1)),
                ("bytes.rgba", &picture(2, 4)),
            ],
        );
        let mut host = Host::new(&dir);
        let png = |name: &[u8]| Request {
            png: true,
            largest: (4, 4),
            largest_bytes: 32,
            ..request(Medium::File(name.to_vec()))
        };
        // 4 pixels wide at a largest side of 4
        assert!(matches!(host.answer(&png(b"/side")).1, Reply::Ok(_)));
        // 2 by 4, 32 bytes, at a most of 32
        assert!(matches!(host.answer(&png(b"/bytes")).1, Reply::Ok(_)));
    }

    #[test]
    fn a_png_request_reads_the_decoded_picture_and_a_discard_reads_nothing() {
        let mut rgba = Vec::new();
        rgba.extend_from_slice(&1u32.to_le_bytes());
        rgba.extend_from_slice(&2u32.to_le_bytes());
        rgba.extend_from_slice(&[0x80, 0x40, 0x20, 0xff, 0xff, 0xff, 0xff, 0x80]);
        let dir = fixtures("png", &[("p.rgba", &rgba)]);
        let mut host = Host::new(&dir);
        let png = Request {
            png: true,
            ..request(Medium::TempFile(b"/tmp/p".to_vec()))
        };
        let want = Reply::Ok(Payload::Rgba {
            width: 1,
            height: 2,
            data: vec![0x80, 0x40, 0x20, 0xff, 0x80, 0x80, 0x80, 0x80],
        });
        // the straight fixture comes back premultiplied, as the helper's
        // pictures do
        assert_eq!(host.answer(&png).1, want);
        let discard = Request {
            discard: true,
            ..request(Medium::TempFile(b"/tmp/p".to_vec()))
        };
        let (line, reply) = host.answer(&discard);
        assert!(line.contains("flags=discard"), "{line}");
        assert_eq!(reply, Reply::Ok(Payload::Discarded));
    }
}
