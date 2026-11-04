//! The privacy rules (ADR-1200 D24): the document holds neither the
//! machine's hostname nor a monitor's serial. The report never prints
//! them itself, but a log line or a program's output can, so every part
//! it gathers has each whole-word occurrence of the hostname and of each
//! connected monitor's serial replaced. A monitor unplugged earlier is no
//! longer in sysfs, so vtium's own form of a monitor's id,
//! `MAKER:0xPRODUCT:SERIAL`, has its serial replaced wherever it shows,
//! whatever monitor it names.

use super::Machine;
use super::edid::Edid;
use super::parts::connectors;

/// The shortest name replaced. A shorter one identifies no machine, and
/// replacing it as a word would garble the text around it.
const SHORTEST: usize = 3;

/// The hostnames that name no machine in particular, left alone.
const ANONYMOUS: [&str; 2] = ["localhost", "localhost.localdomain"];

/// The names the document must not hold, each with what replaces it.
pub struct Secrets {
    /// Each name in ASCII lower case, longest first.
    names: Vec<(String, &'static str)>,
}

impl Secrets {
    /// The secrets of `m`: its hostname, as the kernel and `/etc/hostname`
    /// give it, and the serial string and serial number of each monitor
    /// whose EDID sysfs holds.
    pub fn of(m: &Machine) -> Secrets {
        let mut s = Secrets { names: Vec::new() };
        for file in ["/proc/sys/kernel/hostname", "/etc/hostname"] {
            if let Ok(text) = m.text(file, 256) {
                let name = text.trim();
                s.add(name, "[hostname]");
                if let Some((first, _)) = name.split_once('.') {
                    s.add(first, "[hostname]");
                }
            }
        }
        for (_, dir) in connectors(m, None) {
            let Some(edid) = Edid::read(m, &dir) else {
                continue;
            };
            if let Some(text) = edid.serial_text() {
                s.add(&text, "[serial]");
            }
            if let Some(n) = edid.serial_number() {
                s.add(&format!("0x{n:08x}"), "[serial]");
                s.add(&format!("{n:08x}"), "[serial]");
                s.add(&n.to_string(), "[serial]");
            }
        }
        s.names
            .sort_by(|a, b| b.0.len().cmp(&a.0.len()).then(a.0.cmp(&b.0)));
        s.names.dedup();
        s
    }

    fn add(&mut self, name: &str, with: &'static str) {
        let name = name.trim().to_ascii_lowercase();
        if name.chars().count() >= SHORTEST && !ANONYMOUS.contains(&name.as_str()) {
            self.names.push((name, with));
        }
    }

    /// `text` with the serial in each monitor id shaped as vtium writes one
    /// replaced, then each whole-word occurrence of a secret, in any case.
    pub fn scrub(&self, text: &str) -> String {
        self.names
            .iter()
            .fold(replace_id_serials(text), |t, (name, with)| {
                replace_word(&t, name, with)
            })
    }
}

/// Whether `b` belongs to a word: an ASCII letter or digit.
fn word(b: Option<&u8>) -> bool {
    b.is_some_and(u8::is_ascii_alphanumeric)
}

/// `text` with each occurrence of `name` (in ASCII lower case) that no
/// letter or digit touches on either side, in any case, replaced by
/// `with`.
fn replace_word(text: &str, name: &str, with: &str) -> String {
    // ASCII lower case keeps every byte where it is, so an offset in
    // `lower` is the same offset in `text`
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    let mut from = 0;
    while let Some(found) = lower.get(from..).and_then(|rest| rest.find(name)) {
        let at = from + found;
        let end = at + name.len();
        let alone = !word(at.checked_sub(1).and_then(|i| bytes.get(i))) && !word(bytes.get(end));
        if alone {
            out.push_str(text.get(copied..at).unwrap_or_default());
            out.push_str(with);
            copied = end;
            from = end;
        } else {
            // past this match's first character, to a boundary of `lower`
            from = at
                + lower
                    .get(at..)
                    .and_then(|r| r.chars().next())
                    .map_or(1, char::len_utf8);
        }
    }
    out.push_str(text.get(copied..).unwrap_or_default());
    out
}

/// `text` with the serial of each monitor id shaped as vtium writes one,
/// `MAKER:0xPRODUCT:SERIAL` (vtium-platform's `EdidId`), replaced by
/// `[serial]`: three maker letters, each a byte from `@` to `_` as the
/// kernel reads a PNP id, then `:0x`, four hex digits, `:`, and the serial
/// up to the next space or line's end. An id that no letter or digit
/// touches before it is taken, and one with no serial is left as it is.
fn replace_id_serials(text: &str) -> String {
    let bytes = text.as_bytes();
    let maker = |b: Option<&u8>| b.is_some_and(|b| (b'@'..=b'_').contains(b));
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    let mut from = 0;
    while let Some(found) = text.get(from..).and_then(|rest| rest.find(":0x")) {
        let at = from + found;
        from = at + 1;
        let Some(start) = at.checked_sub(3) else {
            continue;
        };
        let id = (start..at).all(|i| maker(bytes.get(i)))
            && !start
                .checked_sub(1)
                .and_then(|i| bytes.get(i))
                .is_some_and(u8::is_ascii_alphanumeric)
            && (at + 3..at + 7).all(|i| bytes.get(i).is_some_and(u8::is_ascii_hexdigit))
            && bytes.get(at + 7) == Some(&b':');
        if !id {
            continue;
        }
        let serial = at + 8;
        let end = text
            .get(serial..)
            .and_then(|r| r.find(char::is_whitespace))
            .map_or(text.len(), |n| serial + n);
        if end > serial {
            out.push_str(text.get(copied..serial).unwrap_or_default());
            out.push_str("[serial]");
            copied = end;
            from = end;
        }
    }
    out.push_str(text.get(copied..).unwrap_or_default());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_replaced_only_as_a_whole_word_in_any_case() {
        let with = |t: &str| replace_word(t, "planthost", "[hostname]");
        assert_eq!(
            with("PlantHost said hi to planthost.example.org"),
            "[hostname] said hi to [hostname].example.org"
        );
        assert_eq!(
            with("planthosts and myplanthost"),
            "planthosts and myplanthost"
        );
        assert_eq!(with("planthost"), "[hostname]");
        assert_eq!(with("é planthost é"), "é [hostname] é");
        assert_eq!(with(""), "");
    }

    /// vtium logs a monitor as `MAKER:0xPRODUCT:SERIAL`; its serial goes
    /// whatever the monitor, one unplugged before the report included.
    #[test]
    fn a_serial_in_vtium_s_form_of_a_monitor_id_is_replaced() {
        assert_eq!(
            replace_id_serials("display DP-2 is GSM:0x5B7F:OLDSER99 now gone"),
            "display DP-2 is GSM:0x5B7F:[serial] now gone"
        );
        assert_eq!(
            replace_id_serials("edid: DEL:0x4321:SN_77310\nAAA:0x0001:0x00000001"),
            "edid: DEL:0x4321:[serial]\nAAA:0x0001:[serial]"
        );
        // no serial, no id shape, or a word before it: left as it is
        for kept in [
            "AAA:0x0001: next",
            "AAA:0x0001:",
            "ab:0x1234:x",
            "xDEL:0x4321:SER",
            "DEL:0x43:SER",
            "time 12:0x",
            "é:0x",
        ] {
            assert_eq!(replace_id_serials(kept), kept);
        }
    }

    #[test]
    fn short_and_anonymous_names_are_left_alone() {
        let mut s = Secrets { names: Vec::new() };
        s.add("pc", "[hostname]");
        s.add("localhost", "[hostname]");
        s.add(" box ", "[hostname]");
        assert_eq!(s.names, [("box".to_owned(), "[hostname]")]);
        assert_eq!(
            s.scrub("a pc in a box on localhost"),
            "a pc in a [hostname] on localhost"
        );
    }
}
