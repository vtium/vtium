//! A connected monitor's EDID as the report uses it: the name and size it
//! shows, and the serials it only ever replaces. vtium-platform's reader
//! checks the base block and gives the monitor's id, its serial in the
//! form vtium logs it; this module reads only what that reader does not
//! give, the monitor's name and the raw serial number.

use vtium_platform::display::{self, Serial};

use super::Machine;

/// A monitor's EDID: its base block, checked as the console checks it.
pub struct Edid {
    base: Vec<u8>,
    parsed: display::Edid,
}

/// The offsets of the base block's four descriptors.
const DESCRIPTORS: [usize; 4] = [54, 72, 90, 108];

/// The tag of the display descriptor holding the monitor's name.
const NAME: u8 = 0xfc;

impl Edid {
    /// The EDID in the connector directory `dir`, where it holds one whose
    /// header and checksum are good.
    pub fn read(m: &Machine, dir: &str) -> Option<Edid> {
        let base = m.bytes(&format!("{dir}/edid"), 128).ok()?;
        let parsed = display::Edid::parse(&base)?;
        Some(Edid { base, parsed })
    }

    /// The monitor's name, up to its line feed, without the spaces that pad
    /// it, each character that is not printable ASCII read as `_`.
    pub fn name(&self) -> Option<String> {
        let d = DESCRIPTORS
            .iter()
            .filter_map(|&at| self.base.get(at..at + 18))
            .find(|d| d.get(..4) == Some(&[0, 0, 0, NAME]))?;
        let text = d.get(5..)?;
        let end = text.iter().position(|&b| b == b'\n').unwrap_or(text.len());
        let name: String = String::from_utf8_lossy(text.get(..end)?)
            .trim()
            .chars()
            .map(|c| {
                if c.is_ascii_graphic() || c == ' ' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        (!name.is_empty()).then_some(name)
    }

    /// The serial string as vtium logs it, where the EDID has one; the
    /// report only ever replaces it.
    pub fn serial_text(&self) -> Option<String> {
        match self.parsed.id().serial() {
            Serial::Text(text) => Some(text.clone()),
            Serial::Number(_) | Serial::None => None,
        }
    }

    /// The serial number, bytes 12 to 15, where it is not 0, whether or
    /// not a serial string wins over it in the monitor's id; the report
    /// only ever replaces it.
    pub fn serial_number(&self) -> Option<u32> {
        let b = self.base.get(12..16)?;
        let n = u32::from_le_bytes([*b.first()?, *b.get(1)?, *b.get(2)?, *b.get(3)?]);
        (n != 0).then_some(n)
    }

    /// The size it states, in millimetres, as the console reads it.
    pub fn mm(&self) -> Option<(u32, u32)> {
        self.parsed.mm()
    }
}
