//! `docs/config.md` and `install/vtium.conf.example`, generated from the
//! key table.

use std::fmt::Write as _;

use super::keys::{
    Default, KEYS, Key, Kind, OSD_BORDER, OVERLAY_COLORS, SB_MEMORY, SCHEMES, SECTIONS, Scope,
    Section, TEXT_MAX,
};
use super::{FILE_MAX, LINE_MAX};

const SOURCE: &str = "crates/vtium-settings/src/keys.rs";
const BLESS: &str = "VTIUM_BLESS=1 cargo test -p vtium generated_files";

fn kind(k: &Key) -> String {
    match k.kind {
        Kind::Switch => "switch".to_owned(),
        Kind::Number { unit, word, .. } => format!("number ({unit}){}", or(word)),
        Kind::Text => "text".to_owned(),
        Kind::Choice(c) => format!("one of `{}`", c.join("`, `")),
        Kind::Command => "command".to_owned(),
        Kind::Rgb => "color, `r,g,b`".to_owned(),
        Kind::Mode => "mode, a line each".to_owned(),
        Kind::Grab => "chords, `<Mod>...key`, separated by commas".to_owned(),
        Kind::DecoColor => "color, `fg`, a palette index or `#rrggbb`".to_owned(),
        Kind::Char => "character".to_owned(),
        Kind::Size { .. } => "size, `WxH`".to_owned(),
    }
}

/// A number key's word, as the tables give it after the number.
fn or(word: Option<&str>) -> String {
    word.map(|w| format!(", or `{w}`")).unwrap_or_default()
}

fn bound(k: &Key) -> String {
    match k.kind {
        Kind::Switch => "on or off".to_owned(),
        Kind::Number { min, max, word, .. } => format!("{min} to {max}, clamped{}", or(word)),
        Kind::Text => format!("{TEXT_MAX} bytes, no control character"),
        Kind::Choice(_) => "the choices".to_owned(),
        Kind::Command => format!("{TEXT_MAX} bytes in a file"),
        Kind::Rgb => "each 0 to 255".to_owned(),
        Kind::Mode => "names of 63 bytes, each side 1 to 65535".to_owned(),
        Kind::Grab => format!(
            "{TEXT_MAX} bytes; modifiers Shift, Lock, Control, Alt, Logo; keys as xkb names them"
        ),
        Kind::DecoColor => "an index 0 to 255, or six hex digits".to_owned(),
        Kind::Char => "one character, no control character or space".to_owned(),
        Kind::Size { max } => format!("each side 1 to {max}"),
    }
}

fn default(k: &Key) -> String {
    if k.name == SB_MEMORY.name {
        return sb_memory_defaults();
    }
    match k.tree_default() {
        Default::Switch(true) => "on".to_owned(),
        Default::Switch(false) => "off".to_owned(),
        Default::Number(n) => n.to_string(),
        Default::Text(t) => format!("`{t}`"),
        Default::Command(c) => format!("`{}`", c.join(" ")),
        Default::Rgb([r, g, b]) => format!("`{r},{g},{b}`"),
        Default::Unset => "unset".to_owned(),
    }
}

/// `sb-memory`'s default in each build, which differ because a build with
/// images prices kitty placeholder cells into a screen row (ADR-1001,
/// "Placeholder cells"). Both are written, so the documents are the same
/// whichever build generates them.
fn sb_memory_defaults() -> String {
    format!(
        "{} without images, {} with them",
        vtium_term::SB_MEMORY_DEFAULT_MINSPEC,
        vtium_term::SB_MEMORY_DEFAULT_IMAGES
    )
}

fn option(k: &Key) -> String {
    let long = match (k.kind, k.scope) {
        (Kind::Switch, Scope::CommandLine) => format!("`--{}`", k.name),
        (Kind::Switch, _) => format!("`--{0}`, `--no-{0}`", k.name),
        (Kind::Command, _) => format!("`--{} -- COMMAND...`", k.name),
        _ => format!("`--{}=VALUE`", k.name),
    };
    match k.short {
        Some(c) => format!("{long}, `-{c}`"),
        None => long,
    }
}

/// `docs/config.md`.
pub(super) fn markdown() -> String {
    let mut out = format!(
        "# Configuration\n\
         \n\
         <!-- Generated from {SOURCE}; do not edit. `{BLESS}` rewrites it. -->\n\
         \n\
         vtium reads `vtium.conf` in the configuration directory (`--configdir`, `/etc/vtium` by default), then `<seat>.seat.conf` there, then its command line. Each wins over the ones before it. vtium never reads the C reference's `/etc/kmscon`: a `kmscon.conf` from kmscon is copied to `/etc/vtium/vtium.conf` by hand, and vtium reads its keys as the C reference did, with the departures the vtium crate's README lists. A file that does not exist is skipped. The files are read once, at start: a change applies when vtium starts again, as at a restart of its service. `vtium check-config [FILE]` reads them as the start would, with the head file, `/etc/vtium/heads.conf`, or FILE in its place to check it before it is installed, through the start's parser and evaluator (ADR-1401 D7). It applies nothing, exits 1 on any warning and on each of the head file's errors and warnings, each named with its line, and notes on standard output what it does not count: the density points convert at, and of the head file each rule sending input to a head by `id` or `name` alone, where no udev daemon runs, each rule using a key that reads udev's database, each rule using `connector-path:`, which it cannot check against the displays present since it opens no card to read a connector's `PATH`, a head no rule names, every head isolated, and what the devices present find: each rule matching none of them, each device no rule places with what the policy gives it, no head that can be the active head having a display, under a closed policy no keyboard reaching any head, a head claiming pointers while every keyboard is claimed elsewhere, every keyboard ignored, and a head whose only keyboard is one device (ADR-1401 D9). It reads the devices from sysfs and udev's database, `/run/udev/data`, and opens nothing under `/dev`. A FILE that root does not own, or that others can write, is checked all the same, with a note, since installing it makes it root's.\n\
         \n\
         A file holds one key a line: `key=value`, or a switch's `key` to turn it on and `no-key` to turn it off. `#` starts a comment, except where it opens a `#rrggbb` color value. Spaces and tabs around the key and the value are dropped. On the command line a key is `--key=VALUE` or `--key VALUE`, and a switch `--key` or `--no-key`.\n\
         \n\
         vtium reports, once at start, a key it does not know, however often it appears, and a value it cannot use: a number out of bounds, which it clamps, and a value it cannot read, which it ignores, keeping the value the sources before it gave. It reads at most {FILE_MAX} bytes of a file and {LINE_MAX} of a line.\n\
         \n\
         `install/vtium.conf.example` in `crates/vtium` lists the same keys at their defaults, under the same sections. Source cites the C reference, kmscon at `b1f7a7c5`, or the decision a key comes from.\n"
    );
    for section in &SECTIONS {
        let _ = write!(
            out,
            "\n## {}\n\n{}\n\n\
             | Key | Option | Type | Default | Bound | Read by | Set in | What | Source |\n\
             |---|---|---|---|---|---|---|---|---|\n",
            section.title, section.about
        );
        for k in section.groups.iter().flat_map(|g| g.keys) {
            row(&mut out, k);
        }
    }
    out.push_str(
        "\n## Other color schemes for the notices and the on-screen display\n\n\
         `install/vtium.conf.example` carries these schemes for the six color keys, commented out, for color blindness and personal preference (ADR-1401 D5). vtium draws whatever colors are set and checks no contrast between a text and its box.\n\n\
         | Scheme | What it is |",
    );
    for key in OVERLAY_COLORS.iter().flatten() {
        let _ = write!(out, " `{}` |", key.name);
    }
    out.push_str("\n|---|---|");
    out.push_str(&"---|".repeat(OVERLAY_COLORS.iter().flatten().count()));
    out.push('\n');
    for scheme in &SCHEMES {
        let _ = write!(out, "| `{}` | {} |", scheme.name, scheme.what);
        for [r, g, b] in scheme.notice.iter().chain(&scheme.osd) {
            let _ = write!(out, " `{r},{g},{b}` |");
        }
        out.push('\n');
    }
    out
}

/// `k`'s row of a section's table in `docs/config.md`.
fn row(out: &mut String, k: &Key) {
    let set_in = match k.scope {
        Scope::Anywhere => "file, command line",
        Scope::CommandLine => "command line",
    };
    let _ = writeln!(
        out,
        "| `{}` | {} | {} | {} | {} | {} | {set_in} | {} | {} |",
        k.name,
        option(k),
        kind(k),
        default(k),
        bound(k),
        k.reader,
        k.what_line(),
        k.cite
    );
}

/// The widest line of the example, in columns.
const WIDTH: usize = 78;

/// `text` wrapped into lines of at most [`WIDTH`] columns, each starting
/// with `mark` and a space. A code span, such as `` `osd = full` ``, is
/// one word, and a word wider than a line has a line of its own.
fn wrap(mark: &str, text: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let mut open = false;
    for piece in text.split(' ').filter(|w| !w.is_empty()) {
        match words.last_mut() {
            Some(last) if open => {
                last.push(' ');
                last.push_str(piece);
            }
            _ => words.push(piece.to_owned()),
        }
        if piece.matches('`').count() % 2 == 1 {
            open = !open;
        }
    }
    let mut lines = Vec::new();
    let mut line = mark.to_owned();
    for word in &words {
        let wide = line
            .chars()
            .count()
            .saturating_add(1)
            .saturating_add(word.chars().count());
        if wide > WIDTH && line != mark {
            lines.push(std::mem::replace(&mut line, mark.to_owned()));
        }
        line.push(' ');
        line.push_str(word);
    }
    lines.push(line);
    lines
}

/// Paragraphs as comment lines: each wrapped by [`wrap`], with a line of
/// `mark` alone between two paragraphs.
fn paragraphs<'a>(mark: &str, texts: impl IntoIterator<Item = &'a str>) -> String {
    let mut out = String::new();
    for (i, text) in texts.into_iter().enumerate() {
        if i > 0 {
            out.push_str(mark);
            out.push('\n');
        }
        for line in wrap(mark, text) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

/// A heading line: `# RR TITLE `, then the rule character `R` to
/// [`WIDTH`].
fn rule(rule: char, title: &str) -> String {
    let mut line = format!("# {rule}{rule} {title} ");
    while line.chars().count() < WIDTH {
        line.push(rule);
    }
    line
}

/// A group's heading: `# ── TITLE ───`.
fn heading(title: &str) -> String {
    rule('─', title)
}

/// A section's heading, `# ══ TITLE ═══`, then what the section covers.
fn section_heading(title: &str, about: &str) -> String {
    format!("{}\n#\n{}", rule('═', title), paragraphs("#", [about]))
}

/// A group's description. A group of one key has the key's own
/// paragraphs. In a group of several, each key's first paragraph starts
/// with its name, a line of `##` alone separates one key's paragraphs, and
/// a line of `#` alone separates two keys, so a paragraph reads as part of
/// the key above it. Two keys whose descriptions each fit on one line
/// stand on adjacent lines, as a list does.
fn description(keys: &[&Key]) -> String {
    let [only] = keys else {
        let mut out = String::new();
        let mut short_before = false;
        for (i, k) in keys.iter().enumerate() {
            let texts: Vec<String> = k
                .paragraphs()
                .enumerate()
                .map(|(n, p)| {
                    if n == 0 {
                        format!("{}: {p}", k.name)
                    } else {
                        p.to_owned()
                    }
                })
                .collect();
            let text = paragraphs("##", texts.iter().map(String::as_str));
            let short = text.lines().count() == 1;
            if i > 0 && !(short && short_before) {
                out.push_str("#\n");
            }
            out.push_str(&text);
            short_before = short;
        }
        return out;
    };
    paragraphs("##", only.paragraphs())
}

/// The line that shows `k` at its default, commented out: `#key=value`, or
/// a switch's `#key` or `#no-key`.
fn key_line(k: &Key) -> String {
    let line = match (k.kind, k.tree_default()) {
        _ if k.name == SB_MEMORY.name => format!(
            "{}={}\n# (in a build with images: {}={})",
            k.name,
            vtium_term::SB_MEMORY_DEFAULT_MINSPEC,
            k.name,
            vtium_term::SB_MEMORY_DEFAULT_IMAGES
        ),
        (Kind::Switch, Default::Switch(true)) => k.name.to_owned(),
        (Kind::Switch, _) => format!("no-{}", k.name),
        (_, Default::Number(n)) => format!("{}={n}", k.name),
        (_, Default::Text(t)) => format!("{}={t}", k.name),
        (_, Default::Command(c)) => format!("{}={}", k.name, c.join(" ")),
        (_, Default::Rgb([r, g, b])) => format!("{}={r},{g},{b}", k.name),
        _ => format!("{}=", k.name),
    };
    format!("#{line}\n")
}

/// The example's schemes for the six color keys, each line commented out.
fn schemes() -> String {
    let mut out = format!(
        "\n{}\n\n{}",
        heading("Other color schemes"),
        paragraphs(
            "##",
            [
                "Other schemes for the six colors above. To use one, remove the # at the start of its six lines. vtium draws whatever colors are set and checks no contrast between a text and its box."
            ]
        )
    );
    for scheme in &SCHEMES {
        let what = format!("{}: {}", scheme.name, scheme.what);
        out.push('\n');
        out.push_str(&paragraphs("##", [what.as_str()]));
        out.push('\n');
        let values = scheme.notice.iter().chain(&scheme.osd);
        for (key, [r, g, b]) in OVERLAY_COLORS.iter().flatten().zip(values) {
            let _ = writeln!(out, "#{}={r},{g},{b}", key.name);
        }
    }
    out
}

/// A group's title and the keys of it a file may set.
type FileGroup = (&'static str, Vec<&'static Key>);

/// `section`'s groups as a file sees them: each with the keys of it a file
/// may set, and no group left with none.
fn in_files(section: &Section) -> Vec<FileGroup> {
    section
        .groups
        .iter()
        .map(|g| {
            let keys = g
                .keys
                .iter()
                .copied()
                .filter(|k| k.scope == Scope::Anywhere);
            (g.title, keys.collect())
        })
        .filter(|(_, keys): &FileGroup| !keys.is_empty())
        .collect()
}

/// `install/vtium.conf.example`: a header, then each section under its
/// heading, and in it each group under its heading, with its description
/// and then its keys at their defaults. Keys only the command line takes
/// are left out.
pub(super) fn example() -> String {
    let mut out = paragraphs(
        "#",
        [
            format!("vtium's configuration, installed as /etc/vtium/vtium.conf.example. It is generated from {SOURCE}, and docs/config.md describes each key.").as_str(),
            "One key a line: key=value, or a switch's key to turn it on and no-key to turn it off. Each key is shown at its default, commented out. The command line wins over this file.",
        ],
    );
    for section in &SECTIONS {
        let groups = in_files(section);
        if groups.is_empty() {
            continue;
        }
        let _ = write!(out, "\n{}", section_heading(section.title, section.about));
        for (title, keys) in groups {
            let _ = write!(out, "\n{}\n\n{}\n", heading(title), description(&keys));
            for k in &keys {
                out.push_str(&key_line(k));
            }
            if keys.iter().any(|k| k.name == OSD_BORDER.name) {
                out.push_str(&schemes());
            }
        }
    }
    out
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    #[test]
    fn generated_files_match_the_table() {
        let root = env!("CARGO_MANIFEST_DIR");
        let files = [
            (format!("{root}/../../docs/config.md"), markdown()),
            (format!("{root}/install/vtium.conf.example"), example()),
        ];
        let mut stale = Vec::new();
        for (path, want) in files {
            let have = std::fs::read_to_string(&path).unwrap_or_default();
            if have == want {
                continue;
            }
            if std::env::var_os("VTIUM_BLESS").is_some() {
                std::fs::write(&path, want).expect("rewrite a generated file");
            } else {
                stale.push(path);
            }
        }
        assert!(
            stale.is_empty(),
            "{stale:?} differ from the key table; rerun with VTIUM_BLESS=1 to rewrite them"
        );
    }

    // ADR-1401 D5: each scheme the example carries, its six lines with the
    // `#` taken off, is a vtium.conf that draws the scheme's colors.
    #[test]
    fn each_scheme_in_the_example_uncommented_sets_its_colors() {
        let example = example();
        for scheme in &SCHEMES {
            let head = format!("## {}: ", scheme.name);
            let lines: Vec<&str> = example
                .lines()
                .skip_while(|l| !l.starts_with(&head))
                .skip_while(|l| l.starts_with("##") || l.is_empty())
                .take(6)
                .map(|l| l.strip_prefix('#').unwrap_or(l))
                .collect();
            assert_eq!(lines.len(), 6, "{}", scheme.name);
            let file = format!("{}\n", lines.join("\n"));
            let loaded = crate::config::load(&[], None, &mut |_: &std::path::Path| {
                Ok(Some(file.clone().into_bytes()))
            });
            assert_eq!(loaded.reports, [], "{}", scheme.name);
            assert_eq!(
                crate::term::OverlayColors::configured(&loaded.config),
                crate::term::OverlayColors::scheme(scheme),
                "{}",
                scheme.name
            );
        }
    }

    /// The example with the `#` taken off each line that shows a key at its
    /// default value: the schemes' lines stay commented, and so does a line
    /// such as `#vt=`, which shows a key with no default.
    fn uncommented(example: &str) -> String {
        let shown: Vec<String> = KEYS
            .iter()
            .filter(|k| k.scope == Scope::Anywhere)
            .filter_map(|k| key_line(k).lines().next().map(str::to_owned))
            .filter(|l| !l.ends_with('='))
            .collect();
        example
            .lines()
            .map(|l| match l.strip_prefix('#') {
                Some(rest) if shown.iter().any(|s| s == l) => format!("{rest}\n"),
                _ => format!("{l}\n"),
            })
            .collect()
    }

    fn load_file(text: &str) -> crate::config::Loaded {
        let text = text.to_owned();
        crate::config::load(&[], None, &mut |_: &std::path::Path| {
            Ok(Some(text.clone().into_bytes()))
        })
    }

    #[test]
    fn every_line_of_the_example_fits_78_columns() {
        let example = example();
        for line in example.lines() {
            let text = line.trim_start_matches('#').trim_start();
            let one_word = !text.contains(' ') || (text.starts_with('`') && text.ends_with('`'));
            assert!(
                line.chars().count() <= 78 || one_word,
                "{} columns: {line}",
                line.chars().count()
            );
        }
        // the header's first paragraph wraps
        assert!(example.starts_with(
            "# vtium's configuration, installed as /etc/vtium/vtium.conf.example. It is\n# generated from"
        ));
    }

    /// Each section's title and its groups as the example shows them.
    fn shown() -> Vec<(&'static str, Vec<FileGroup>)> {
        SECTIONS.iter().map(|s| (s.title, in_files(s))).collect()
    }

    // Each section opens with its heading, a line of `#`, what it covers in
    // `#` lines and a blank line; its groups follow, before the next
    // section's heading, in the table's order.
    #[test]
    fn each_section_is_a_heading_what_it_covers_and_its_groups() {
        let example = example();
        let lines: Vec<&str> = example.lines().collect();
        let headings: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.starts_with("# ══ "))
            .map(|(i, _)| i)
            .collect();
        let sections = shown();
        assert_eq!(headings.len(), sections.len());
        for (n, (title, groups)) in sections.iter().enumerate() {
            let at = headings[n];
            assert!(
                lines[at].starts_with(&format!("# ══ {title} ═")),
                "{title}: {}",
                lines[at]
            );
            assert_eq!(lines[at].chars().count(), 78, "{title}");
            assert_eq!(lines[at - 1], "", "a blank line above {title}");
            assert_eq!(lines[at + 1], "#", "{title}");
            let about = lines[at + 2..]
                .iter()
                .take_while(|l| l.starts_with("# "))
                .count();
            assert!(about > 0, "{title} says nothing of what it covers");
            assert_eq!(lines[at + 2 + about], "", "{title}");
            assert!(
                lines[at + 3 + about].starts_with("# ── "),
                "{title}: a group follows"
            );
            let end = headings.get(n + 1).copied().unwrap_or(lines.len());
            let inside: Vec<&str> = lines[at..end]
                .iter()
                .filter(|l| l.starts_with("# ── "))
                .copied()
                .collect();
            let mut want: Vec<String> = groups.iter().map(|(g, _)| heading(g)).collect();
            if groups
                .iter()
                .any(|(_, keys)| keys.iter().any(|k| k.name == OSD_BORDER.name))
            {
                want.push(heading("Other color schemes"));
            }
            assert_eq!(inside, want, "{title}");
        }
    }

    // Each group is its heading, a blank line, its description in `##`
    // lines, a blank line, then its keys' lines in the table's order.
    #[test]
    fn each_group_is_a_heading_a_description_and_its_keys() {
        let example = example();
        let lines: Vec<&str> = example.lines().collect();
        for (title, keys) in shown().into_iter().flat_map(|(_, groups)| groups) {
            let at = lines
                .iter()
                .position(|l| l.starts_with(&format!("# ── {title} ─")))
                .unwrap_or_else(|| panic!("no heading for {title}"));
            assert_eq!(lines[at].chars().count(), 78, "{title}");
            assert!(lines[at].ends_with("──"), "{title}");
            assert_eq!(lines[at - 1], "", "a blank line above {title}");
            assert_eq!(lines[at + 1], "", "a blank line below {title}");
            let rest = &lines[at + 2..];
            let said = rest.iter().take_while(|l| !l.is_empty()).count();
            let separator = |l: &str| l == "##" || l == "#";
            assert!(
                rest[..said]
                    .iter()
                    .all(|l| l.starts_with("## ") || separator(l)),
                "{title}: a description line that is neither text nor a separator"
            );
            assert!(
                rest[..said].iter().any(|l| l.starts_with("## ")),
                "{title} has no description"
            );
            assert!(!separator(rest[0]), "{title}");
            assert!(!separator(rest[said - 1]), "{title}");
            assert!(
                !rest[..said]
                    .windows(2)
                    .any(|w| separator(w[0]) && separator(w[1])),
                "{title}: two separators in a row"
            );
            // a key's paragraphs are separated by `##`, and two keys by `#`
            // before the line that names the next key
            for (n, l) in rest[..said].iter().enumerate().skip(1) {
                let names = keys
                    .iter()
                    .any(|k| l.starts_with(&format!("## {}: ", k.name)));
                if keys.len() > 1 && names {
                    let before = rest[n - 1];
                    assert!(before == "#" || before.starts_with("## "), "{title}: {l}");
                } else {
                    assert_ne!(rest[n - 1], "#", "{title}: {l} follows a key break");
                }
            }
            assert_eq!(rest[said], "", "{title}");
            let set: Vec<&str> = rest[said + 1..]
                .iter()
                .take_while(|l| !l.is_empty())
                .filter(|l| !l.starts_with("# "))
                .copied()
                .collect();
            let want: Vec<String> = keys
                .iter()
                .map(|k| key_line(k).lines().next().unwrap_or_default().to_owned())
                .collect();
            assert_eq!(set, want, "{title}");
        }
    }

    // docs/config.md has a heading and a table for each section, and lists
    // every key once.
    #[test]
    fn the_markdown_has_a_table_for_each_section() {
        let md = markdown();
        for s in &SECTIONS {
            assert!(
                md.contains(&format!("\n## {}\n\n{}\n\n| Key |", s.title, s.about)),
                "{}",
                s.title
            );
        }
        for k in &KEYS {
            let row = format!("| `{}` |", k.name);
            assert_eq!(
                md.lines().filter(|l| l.starts_with(&row)).count(),
                1,
                "{}",
                k.name
            );
        }
    }

    #[test]
    fn a_group_of_several_keys_names_each_key_in_its_description() {
        let example = example();
        let osd: Vec<&str> = example
            .lines()
            .skip_while(|l| !l.starts_with("# ── On-screen display ─"))
            .skip(2)
            .take_while(|l| !l.is_empty())
            .collect();
        assert_eq!(
            osd.first().copied(),
            Some("## osd: What vtium's on-screen display draws at the top of each head after a")
        );
        assert!(osd.contains(&"##"), "paragraphs are separated");
        let next = osd
            .iter()
            .position(|l| l.starts_with("## osd-timeout: "))
            .expect("osd-timeout's paragraph");
        assert_eq!(osd[next - 1], "#", "a key break before osd-timeout");
        assert_eq!(
            osd[next - 2],
            "## A notice of a problem shows whatever `osd` says.",
            "osd's last paragraph ends above the break"
        );
        for name in ["osd-timeout: ", "focus-mark: "] {
            assert!(
                osd.iter().any(|l| l.starts_with(&format!("## {name}"))),
                "{name}"
            );
        }
        // descriptions that fit a line each stand on adjacent lines
        let keyboard = example
            .split("# ── Keyboard ─")
            .nth(1)
            .expect("a keyboard section");
        assert!(keyboard.contains(
            "## xkb-model: The keyboard model, as xkeyboard-config names it.\n## xkb-layout: "
        ));
    }

    // The parser ignores the headings, the descriptions and the blank
    // lines: the example as shipped sets nothing, and with each key's line
    // uncommented it sets each key to its default.
    #[test]
    fn the_example_parses_to_the_defaults() {
        let example = example();
        let shipped = load_file(&example);
        assert_eq!(shipped.reports, []);
        for k in KEYS.iter() {
            assert_ne!(
                shipped
                    .config
                    .origin(k)
                    .map(|o| matches!(o, crate::config::Origin::File { .. })),
                Some(true),
                "{} is set",
                k.name
            );
        }

        let loaded = load_file(&uncommented(&example));
        assert_eq!(loaded.reports, []);
        let mut set = Vec::new();
        for k in KEYS.iter().filter(|k| k.scope == Scope::Anywhere) {
            let from_file = matches!(
                loaded.config.origin(k),
                Some(crate::config::Origin::File { .. })
            );
            if k.tree_default() == Default::Unset {
                continue;
            }
            assert!(from_file, "{} is not set by its line", k.name);
            set.push(k.name);
            let want = if k.name == SB_MEMORY.name {
                Some(super::super::Value::Number(
                    vtium_term::SB_MEMORY_DEFAULT_MINSPEC as u64,
                ))
            } else {
                // the example shows the source tree's default
                super::super::value::default(&Key {
                    default: k.tree_default(),
                    ..*k
                })
            };
            assert_eq!(
                loaded
                    .config
                    .values
                    .get(k.name)
                    .and_then(|(v, _)| v.clone()),
                want,
                "{}",
                k.name
            );
        }
        assert!(set.len() > 60, "{} keys set", set.len());
    }

    // vtium-config edits vtium.conf line by line and writes no comment:
    // in the example, a key's new line goes directly below its commented
    // line, and every other line stays.
    #[test]
    fn the_editor_sets_each_key_below_its_line_in_the_example() {
        let example = example();
        let lines: Vec<&str> = example.lines().collect();
        for k in KEYS.iter().filter(|k| k.scope == Scope::Anywhere) {
            let (edit, line) = match (k.kind, k.tree_default()) {
                (Kind::Switch, Default::Switch(on)) => (
                    vtium_settings::Edit::Switch(k.name, !on),
                    if on {
                        format!("no-{}", k.name)
                    } else {
                        k.name.to_owned()
                    },
                ),
                _ => (
                    vtium_settings::Edit::Set(k.name, "1"),
                    format!("{}=1", k.name),
                ),
            };
            let shown = key_line(k);
            let shown = shown.lines().next().unwrap_or_default();
            let at = lines
                .iter()
                .position(|l| *l == shown)
                .unwrap_or_else(|| panic!("no line for {}", k.name));
            let out = vtium_settings::edit_conf(&example, &[edit]).expect("the edit");
            let mut want = lines.clone();
            want.insert(at + 1, &line);
            assert_eq!(out.lines().collect::<Vec<_>>(), want, "{}", k.name);
        }
    }
}
