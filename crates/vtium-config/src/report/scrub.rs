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

/// `text` with the serial in each device id the head file can hold
/// replaced by `[serial]`: first in each `edid:` or `serial:` token,
/// whatever its shape, then in each bare monitor id shaped as vtium
/// writes one, `MAKER:0xPRODUCT:SERIAL`.
fn replace_id_serials(text: &str) -> String {
    replace_bare_ids(&replace_tagged_ids(text))
}

/// The byte offsets of the run that starts at `from` and ends at the next
/// space, tab or line end.
fn run_end(text: &str, from: usize) -> usize {
    text.get(from..)
        .and_then(|r| r.find(char::is_whitespace))
        .map_or(text.len(), |n| from + n)
}

/// `text` with the serial field of each `edid:MAKER:PRODUCT:SERIAL` and
/// `serial:VENDOR:PRODUCT:SERIAL` token replaced, in any case and whatever
/// its fields hold: everything after the token's second colon past its tag,
/// to the next space. A tag that a letter or digit touches before it is
/// part of another word, and a token with fewer fields is left alone.
fn replace_tagged_ids(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut starts: Vec<usize> = Vec::new();
    for tag in ["edid:", "serial:"] {
        let mut from = 0;
        while let Some(found) = lower.get(from..).and_then(|r| r.find(tag)) {
            let at = from + found;
            from = at + tag.len();
            if !word(at.checked_sub(1).and_then(|i| bytes.get(i))) {
                starts.push(from);
            }
        }
    }
    starts.sort_unstable();
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    for fields in starts {
        if fields < copied {
            continue;
        }
        let end = run_end(text, fields);
        let run = text.get(fields..end).unwrap_or_default();
        let Some(serial) = run.match_indices(':').nth(1).map(|(i, _)| fields + i + 1) else {
            continue;
        };
        if serial < end {
            out.push_str(text.get(copied..serial).unwrap_or_default());
            out.push_str("[serial]");
            copied = end;
        }
    }
    out.push_str(text.get(copied..).unwrap_or_default());
    out
}

/// `text` with the serial of each bare monitor id replaced: three maker
/// letters in either case, each `@` to `_` as the kernel reads a PNP id or
/// their lower case, then `:`, the product as one to four hex digits with
/// or without `0x` or `0X`, `:`, and the serial up to the next space. An
/// id that a letter or digit touches before it is part of another word,
/// and one with no serial is left as it is.
fn replace_bare_ids(text: &str) -> String {
    let bytes = text.as_bytes();
    let maker =
        |b: Option<&u8>| b.is_some_and(|b| (b'@'..=b'_').contains(b) || b.is_ascii_lowercase());
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    let mut from = 0;
    while let Some(found) = text.get(from..).and_then(|rest| rest.find(':')) {
        let colon = from + found;
        from = colon + 1;
        let Some(start) = colon.checked_sub(3) else {
            continue;
        };
        if start < copied
            || !(start..colon).all(|i| maker(bytes.get(i)))
            || word(start.checked_sub(1).and_then(|i| bytes.get(i)))
        {
            continue;
        }
        let mut product = colon + 1;
        if bytes.get(product) == Some(&b'0') && matches!(bytes.get(product + 1), Some(b'x' | b'X'))
        {
            product += 2;
        }
        let digits = (product..)
            .take_while(|&i| bytes.get(i).is_some_and(u8::is_ascii_hexdigit))
            .count();
        if !(1..=4).contains(&digits) || bytes.get(product + digits) != Some(&b':') {
            continue;
        }
        let serial = product + digits + 1;
        let end = run_end(text, serial);
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
        // the forms the head file accepts, and a malformed head file's
        // token as check-config echoes it
        for (given, want) in [
            ("DEL:4321:S1", "DEL:4321:[serial]"),
            ("DEL:0X4321:S1", "DEL:0X4321:[serial]"),
            ("DEL:0x21:S1", "DEL:0x21:[serial]"),
            ("del:0x4321:S1", "del:0x4321:[serial]"),
            (
                "rule 3: edid:del:0x4321:SN_1 is malformed",
                "rule 3: edid:del:0x4321:[serial] is malformed",
            ),
            (
                "display EDID:x:y:z:w -> desk",
                "display EDID:x:y:[serial] -> desk",
            ),
            (
                "keyboard serial:046d:c52b:A1B2C3 -> desk",
                "keyboard serial:046d:c52b:[serial] -> desk",
            ),
            ("serial:1:2:3,edid:a:b:c", "serial:1:2:[serial]"),
        ] {
            assert_eq!(replace_id_serials(given), want, "{given}");
        }
        // no serial, no id shape, or a word before it: left as it is
        for kept in [
            "AAA:0x0001: next",
            "AAA:0x0001:",
            "ab:0x1234:x",
            "xDEL:0x4321:SER",
            "DEL:0x43210:SER",
            "DEL::SER",
            "time 12:0x",
            "é:0x",
            "/etc/vtium/vtium.conf:3: unknown key frob",
            "2026-11-03T20:27:03+00:00 vtium[100]: log: vtium Revision v1.2.3",
            "edid:only:two",
            "myedid:a:b:c",
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
