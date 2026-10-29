//! A display's EDID, its base block read once for what vtium takes from
//! it: the physical size the density uses (ADR-1301 D3) and the monitor
//! it names, which the head file's `edid:` matches (ADR-1401 D4).

use std::fmt;
use std::io::Read;
use std::path::Path;

/// The bytes of an EDID's base block, the most read of one.
const BASE: usize = 128;

/// The eight bytes every EDID starts with.
const HEADER: [u8; 8] = [0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0];

/// The base block's four 18-byte descriptors, by offset.
const DESCRIPTORS: [usize; 4] = [54, 72, 90, 108];

/// The tag of the display descriptor holding the serial string.
const SERIAL_STRING: u8 = 0xff;

/// What an EDID's base block states.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Edid {
    /// The physical size in millimetres, across and down, from the image
    /// size in centimetres, bytes 21 and 22; `None` for a side of 0, as
    /// EDID 1.4 states an aspect ratio with no size.
    pub(crate) mm: Option<(u32, u32)>,
    pub(crate) id: EdidId,
}

impl Edid {
    /// The base block at the start of `bytes`; `None` for bytes that are
    /// no EDID or whose base block's checksum fails.
    pub(crate) fn parse(bytes: &[u8]) -> Option<Edid> {
        let base = bytes.get(..BASE)?;
        if base.get(..HEADER.len())? != HEADER
            || base.iter().fold(0u8, |sum, &b| sum.wrapping_add(b)) != 0
        {
            return None;
        }
        let byte = |at: usize| base.get(at).copied();
        let (w, h) = (byte(21)?, byte(22)?);
        let mm = (w != 0 && h != 0).then(|| {
            (
                u32::from(w).saturating_mul(10),
                u32::from(h).saturating_mul(10),
            )
        });
        let maker = u16::from_be_bytes([byte(8)?, byte(9)?]);
        let product = u16::from_le_bytes([byte(10)?, byte(11)?]);
        let number = u32::from_le_bytes([byte(12)?, byte(13)?, byte(14)?, byte(15)?]);
        let text = DESCRIPTORS
            .iter()
            .filter_map(|&at| base.get(at..at.checked_add(18)?))
            .find(|d| d.get(..4) == Some(&[0, 0, 0, SERIAL_STRING]))
            .and_then(|d| text(d.get(5..)?));
        let serial = match text {
            Some(text) => Serial::Text(text),
            None if number != 0 => Serial::Number(number),
            None => Serial::None,
        };
        Some(Edid {
            mm,
            id: EdidId {
                maker: [10, 5, 0].map(|shift| letter(maker >> shift)),
                product,
                serial,
            },
        })
    }

    /// The EDID of connector `name` (`HDMI-A-1`), read from `card`, the
    /// card's directory in sysfs, where the kernel keeps it as
    /// `card<N>-<name>/edid`; `None` where there is none to read.
    pub(crate) fn sysfs(card: &Path, name: &str) -> Option<Edid> {
        let suffix = format!("-{name}");
        let mut dirs: Vec<String> = std::fs::read_dir(card)
            .ok()?
            .filter_map(Result::ok)
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|d| {
                d.strip_prefix("card")
                    .and_then(|d| d.strip_suffix(&suffix))
                    .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
            })
            .collect();
        dirs.sort();
        let file = std::fs::File::open(card.join(dirs.first()?).join("edid")).ok()?;
        let mut edid = Vec::with_capacity(BASE);
        file.take(BASE as u64).read_to_end(&mut edid).ok()?;
        Edid::parse(&edid)
    }
}

/// A display descriptor's text: its 13 bytes up to the line feed that
/// ends it, without the spaces that pad it; `None` where that leaves
/// nothing. A byte that is no printable ASCII, a space within the text
/// included, reads as `_`, so the text is one token of the head file.
fn text(bytes: &[u8]) -> Option<String> {
    let end = bytes
        .iter()
        .position(|&b| b == b'\n')
        .unwrap_or(bytes.len());
    let text = bytes.get(..end)?.trim_ascii();
    (!text.is_empty()).then(|| {
        text.iter()
            .map(|&b| {
                if b.is_ascii_graphic() {
                    char::from(b)
                } else {
                    '_'
                }
            })
            .collect()
    })
}

/// A letter of the PNP maker id, five bits of it, as the kernel reads it:
/// 1 is `A`, and a value past `Z` reads as the character it lands on.
fn letter(bits: u16) -> char {
    char::from(b'@'.saturating_add(u8::try_from(bits & 0x1f).unwrap_or(0)))
}

/// The monitor an EDID names, as the head file's `edid:` matches it:
/// `MAKER:PRODUCT:SERIAL` (ADR-1401 D4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdidId {
    maker: [char; 3],
    product: u16,
    serial: Serial,
}

/// Which serial an EDID states: its serial string, else its serial
/// number, else none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Serial {
    /// The display descriptor `0xff`'s text.
    Text(String),
    /// The base block's serial number, bytes 12 to 15, where it is not 0.
    Number(u32),
    None,
}

impl EdidId {
    /// The PNP maker id, three letters.
    pub fn maker(&self) -> String {
        self.maker.iter().collect()
    }

    /// The product code.
    pub fn product(&self) -> u16 {
        self.product
    }

    pub fn serial(&self) -> &Serial {
        &self.serial
    }
}

/// `AAA:0x0001:LARGE00001`: the maker, the product code in four hex
/// digits, and the serial string, else the serial number in eight hex
/// digits, else nothing.
impl fmt::Display for EdidId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:0x{:04X}:", self.maker(), self.product)?;
        match &self.serial {
            Serial::Text(text) => f.write_str(text),
            Serial::Number(n) => write!(f, "0x{n:08X}"),
            Serial::None => Ok(()),
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "a test's offsets are small and fail by panicking"
)]
mod tests {
    use super::*;

    /// A base block from maker `AAA`, product 1 and serial number
    /// `number`, stating `cm`, with `descriptors` at the descriptors'
    /// offsets, its checksum made good.
    fn block(cm: (u8, u8), number: u32, descriptors: &[[u8; 18]]) -> Vec<u8> {
        let mut b = vec![0u8; BASE];
        b[..8].copy_from_slice(&HEADER);
        // A is 1, so AAA is 00001 00001 00001.
        b[8..10].copy_from_slice(&0b0_00001_00001_00001u16.to_be_bytes());
        b[10..12].copy_from_slice(&1u16.to_le_bytes());
        b[12..16].copy_from_slice(&number.to_le_bytes());
        b[18] = 1;
        b[19] = 4;
        (b[21], b[22]) = cm;
        for (d, &at) in descriptors.iter().zip(&DESCRIPTORS) {
            b[at..at + 18].copy_from_slice(d);
        }
        let sum = b.iter().fold(0u8, |s, &x| s.wrapping_add(x));
        b[127] = 0u8.wrapping_sub(sum);
        b
    }

    /// A display descriptor tagged `tag` holding `text`, ended by a line
    /// feed and padded with spaces as EDID pads it.
    fn descriptor(tag: u8, text: &[u8]) -> [u8; 18] {
        let mut d = [0u8; 18];
        d[3] = tag;
        d[5..].fill(b' ');
        d[5..5 + text.len()].copy_from_slice(text);
        if let Some(end) = d.get_mut(5 + text.len()) {
            *end = b'\n';
        }
        d
    }

    fn id(b: &[u8]) -> String {
        Edid::parse(b).unwrap().id.to_string()
    }

    #[test]
    fn an_edids_serial_string_wins_over_its_serial_number() {
        let b = block((60, 34), 1, &[descriptor(SERIAL_STRING, b"LARGE00001")]);
        assert_eq!(id(&b), "AAA:0x0001:LARGE00001");
    }

    #[test]
    fn an_edid_with_no_serial_string_names_its_serial_number_else_none() {
        let name = descriptor(0xfc, b"Monitor");
        assert_eq!(id(&block((60, 34), 1, &[name])), "AAA:0x0001:0x00000001");
        let b = block((60, 34), 0xdead_beef, &[]);
        assert_eq!(
            Edid::parse(&b).unwrap().id.serial(),
            &Serial::Number(0xdead_beef)
        );
        assert_eq!(id(&b), "AAA:0x0001:0xDEADBEEF");
        // a serial number of 0 is none, and so is a serial string of
        // spaces alone
        assert_eq!(id(&block((60, 34), 0, &[name])), "AAA:0x0001:");
        let blank = descriptor(SERIAL_STRING, b"");
        assert_eq!(id(&block((60, 34), 7, &[blank])), "AAA:0x0001:0x00000007");
    }

    #[test]
    fn the_serial_string_is_read_from_any_descriptor_and_only_from_one() {
        // a detailed timing whose byte 3 happens to be 0xff is no
        // descriptor; the serial string sits in the fourth slot
        let mut timing = descriptor(SERIAL_STRING, b"TIMING");
        timing[0] = 0x02;
        let b = block(
            (60, 34),
            0,
            &[
                timing,
                descriptor(0xfc, b"Monitor"),
                [0; 18],
                descriptor(SERIAL_STRING, b"S1"),
            ],
        );
        assert_eq!(id(&b), "AAA:0x0001:S1");
    }

    #[test]
    fn a_serial_string_is_its_13_bytes_to_the_line_feed() {
        // all 13 bytes with no line feed, a string padded both sides, and
        // a space and a byte past ASCII within one, each `_`
        let full = descriptor(SERIAL_STRING, b"ABCDEFGHIJKLM");
        assert_eq!(id(&block((60, 34), 0, &[full])), "AAA:0x0001:ABCDEFGHIJKLM");
        let padded = descriptor(SERIAL_STRING, b"  X1");
        assert_eq!(id(&block((60, 34), 0, &[padded])), "AAA:0x0001:X1");
        let odd = descriptor(SERIAL_STRING, b"A B\xe9C");
        assert_eq!(id(&block((60, 34), 0, &[odd])), "AAA:0x0001:A_B_C");
        // what follows the line feed is padding, whatever it holds
        let mut tail = descriptor(SERIAL_STRING, b"X2");
        tail[8..].fill(b'Z');
        assert_eq!(id(&block((60, 34), 0, &[tail])), "AAA:0x0001:X2");
    }

    #[test]
    fn the_maker_is_three_letters_of_five_bits_and_the_product_little_endian() {
        let mut b = block((60, 34), 0, &[]);
        // DEL is 4, 5 and 12; product 0xa0c4 is stored c4 a0
        b[8..10].copy_from_slice(&(4u16 << 10 | 5 << 5 | 12).to_be_bytes());
        b[10..12].copy_from_slice(&[0xc4, 0xa0]);
        b[127] = 0;
        b[127] = 0u8.wrapping_sub(b.iter().fold(0u8, |s, &x| s.wrapping_add(x)));
        let edid = Edid::parse(&b).unwrap();
        assert_eq!(edid.id.maker(), "DEL");
        assert_eq!(edid.id.product(), 0xa0c4);
        assert_eq!(edid.id.to_string(), "DEL:0xA0C4:");
    }

    #[test]
    fn an_edid_states_its_image_size_in_centimetres() {
        // a 27-inch monitor's, 60 by 34 cm; a base block with an extension
        // after it reads the same
        let mm = |b: &[u8]| Edid::parse(b).unwrap().mm;
        assert_eq!(mm(&block((60, 34), 0, &[])), Some((600, 340)));
        let mut extended = block((60, 34), 0, &[]);
        extended.extend([0x02; 128]);
        assert_eq!(mm(&extended), Some((600, 340)));
        // no size, and an aspect ratio alone
        assert_eq!(mm(&block((0, 0), 0, &[])), None);
        assert_eq!(mm(&block((0, 79), 0, &[])), None);
    }

    #[test]
    fn bytes_that_are_no_edid_state_nothing() {
        // a block cut short, its own bytes summing to 0 so only its length
        // refuses it, a bad header and a bad checksum
        let short = |n: usize| {
            let mut b = block((60, 34), 1, &[])[..n].to_vec();
            b[n - 1] = 0;
            b[n - 1] = 0u8.wrapping_sub(b.iter().fold(0u8, |s, &x| s.wrapping_add(x)));
            b
        };
        let mut header = block((60, 34), 1, &[]);
        header[0] = 1;
        header[127] = header[127].wrapping_sub(1);
        let mut sum = block((60, 34), 1, &[]);
        sum[127] ^= 1;
        for (why, b) in [
            ("127 bytes", short(127)),
            ("64 bytes", short(64)),
            ("header", header),
            ("checksum", sum),
        ] {
            assert_eq!(Edid::parse(&b), None, "{why}");
        }
    }

    /// A stand-in card directory in sysfs for `test`, holding `edids` by
    /// connector directory.
    fn sysfs(test: &str, edids: &[(&str, Vec<u8>)]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("vtium-edid-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (conn, bytes) in edids {
            std::fs::create_dir_all(dir.join(conn)).unwrap();
            std::fs::write(dir.join(conn).join("edid"), bytes).unwrap();
        }
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_connectors_edid_is_read_from_its_cards_directory_in_sysfs() {
        let dir = sysfs(
            "read",
            &[
                ("card0-HDMI-A-1", block((60, 34), 1, &[])),
                ("card0-HDMI-A-11", block((30, 17), 2, &[])),
                ("card0-DP-1", Vec::new()),
            ],
        );
        let read = |name| Edid::sysfs(&dir, name).map(|e| (e.mm, e.id.to_string()));
        let (hdmi, hdmi11, dp, gone) = (
            read("HDMI-A-1"),
            read("HDMI-A-11"),
            read("DP-1"),
            read("eDP-1"),
        );
        let _ = std::fs::remove_dir_all(&dir);
        let at = |mm, n: &str| Some((Some(mm), format!("AAA:0x0001:{n}")));
        assert_eq!(hdmi, at((600, 340), "0x00000001"));
        assert_eq!(hdmi11, at((300, 170), "0x00000002"));
        // an empty edid file, as a connector with no EDID has, and none
        assert_eq!((dp, gone), (None, None));
        assert_eq!(
            Edid::sysfs(Path::new("/nonexistent/vtium"), "HDMI-A-1"),
            None
        );
    }
}
