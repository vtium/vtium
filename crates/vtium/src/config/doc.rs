//! `docs/config.md` and `install/vtium.conf.example`, generated from the
//! key table.

use std::fmt::Write as _;

use super::keys::{
    Default, FRIT_CELL_MAX, FRIT_CELL_MIN, KEYS, Key, Kind, OSD_BORDER, OVERLAY_COLORS, SB_MEMORY,
    SCHEMES, SECTIONS, Scope, Section, TEXT_MAX,
};
use super::{FILE_MAX, LINE_MAX};

const SOURCE: &str = "crates/vtium-settings/src/keys.rs";
const BLESS: &str = "VTIUM_BLESS=1 cargo test -p vtium generated_files";

fn kind(k: &Key) -> String {
    match k.kind {
        Kind::Switch => "switch".to_owned(),
        Kind::Number { unit, word, .. } => format!("number ({unit}){}", or(word)),
        Kind::Text => "text".to_owned(),
        Kind::Path => "path".to_owned(),
        Kind::Paths => "paths, separated by `:`".to_owned(),
        Kind::Choice(c) => format!("one of `{}`", c.join("`, `")),
        Kind::Command => "command".to_owned(),
        Kind::Rgb => "color, `r,g,b`".to_owned(),
        Kind::Mode => "resolution, one per line".to_owned(),
        Kind::KeyBinding => "key combinations, `<Mod>...key`, separated by commas".to_owned(),
        Kind::DecoColor => "color: `fg`, a palette number or `#rrggbb`".to_owned(),
        Kind::Char => "character".to_owned(),
        Kind::Size { .. } => "size, `WxH`".to_owned(),
        Kind::Cell => "character cell, `WIDTHxHEIGHT` in pixels".to_owned(),
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
        Kind::Path => format!("absolute, {TEXT_MAX} bytes, no control character"),
        Kind::Paths => format!("each absolute, {TEXT_MAX} bytes in all, no control character"),
        Kind::Choice(_) => "the choices".to_owned(),
        Kind::Command => format!("{TEXT_MAX} bytes in a file"),
        Kind::Rgb => "each 0 to 255".to_owned(),
        Kind::Mode => "names of 63 bytes, each side 1 to 65535".to_owned(),
        Kind::KeyBinding => format!(
            "{TEXT_MAX} bytes; modifiers Shift, Lock, Control, Alt, Logo; keys as xkb names them"
        ),
        Kind::DecoColor => "a palette number from 0 to 255, or six hex digits".to_owned(),
        Kind::Char => "one character, no control character or space".to_owned(),
        Kind::Size { max } => format!("each side 1 to {max}"),
        Kind::Cell => format!("each side {FRIT_CELL_MIN} to {FRIT_CELL_MAX}, no leading zero"),
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
        "{} without image support, {} with it",
        vtium_term::SB_MEMORY_DEFAULT_MINSPEC,
        vtium_term::SB_MEMORY_DEFAULT_IMAGES
    )
}

fn option(k: &Key) -> String {
    let long = match (k.kind, k.scope) {
        (_, Scope::Section) => return "none".to_owned(),
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
    let renamed = {
        let names: Vec<String> = vtium_settings::keys::RENAMED
            .iter()
            .map(|(old, key)| format!("`{old}` is `{}`", key.name))
            .collect();
        match names.split_last() {
            Some((last, rest)) => format!("{}, and {last}", rest.join(", ")),
            None => String::new(),
        }
    };
    let mut out = format!(
        "# Configuration\n\
         \n\
         <!-- Generated from {SOURCE}; do not edit. `{BLESS}` rewrites it. -->\n\
         \n\
         vtium reads one file, `vtium.conf` in the configuration directory (`--configdir`, `/etc/vtium` by default), then its command line, which wins over the file. vtium never reads `/etc/kmscon` or a seat file such as `seat0.seat.conf`: a `kmscon.conf` from kmscon is copied to `/etc/vtium/vtium.conf` by hand, and vtium reads the keys `kmscon` takes, in the ways the vtium crate's README, \"Reading the configuration\", sets out. With no file, vtium runs its defaults and its compiled-in heads and rules. The file is read once, at start: a change applies when vtium starts again, as at a restart of its service.\n\
         \n\
         The file has two parts. Every line above the first section heading is the top level: vtium's own keys, the defaults every head inherits, and the flow's `version` and `policy`. From the first section heading on come the sections: a `[look NAME]` for each look, which names a frit and its size, a `[head NAME]` for each head, and one `[rules]` list, which sends each display and each input device to a head. A file with a section needs `version = 1`. A head's section wins over its look's, and its look's over the top level, for the keys the table below says each may set. The page [The sections of vtium.conf](head-file.md) describes each section and its keys.\n\
         \n\
         vtium reads vtium.conf only when root owns it, or, for a vtium that runs as another user, that user, and neither its group nor others can write it or a directory above it. It reads none of a file that fails, runs its compiled-in configuration, and says which path fails and the command that mends it, `chown root:root PATH` or `chmod go-w PATH`.\n\
         \n\
         `vtium check-config [--no-devices] [FILE]` reads vtium.conf, or FILE in its place to check it before it is installed, both its parts in one run, as the start would, through the start's parser and evaluator. It applies nothing, exits 1 on any warning of the keys and on each error and warning of the sections, each named with its line, and on a file whose flow wires no complete route: a display present that its first matching rule sends to a head that a keyboard present reaches. With no display or no keyboard present, or with `--no-devices`, it judges the rules alone and says so. It notes on standard output what it does not count: the density points convert at, and of the sections each rule sending input to a head by `id` or `name` alone, where no udev daemon runs, each rule using a key that reads udev's database, each rule using `connector-path:`, which it cannot check against the displays present since it opens no card to read a connector's `PATH`, a head no rule names, every head isolated, and what the devices present find: each rule matching none of them, each device no rule places with what the policy gives it, no head that can be the active head having a display, under a closed policy no keyboard reaching any head, a head claiming pointers while every keyboard is claimed elsewhere, every keyboard ignored, and a head whose only keyboard is one device. It reads the devices from sysfs and udev's database, `/run/udev/data`, and opens nothing under `/dev`. A FILE that root does not own, or that others can write, is checked all the same, with a note, since installing it makes it root's.\n\
         \n\
         A top-level line holds one key: `key=value`, or a switch's `key` to turn it on and `no-key` to turn it off. A section's line is `key = value`. `#` starts a comment at the start of a line or after a blank; a `#` inside a word, or one that opens a `#rrggbb` value, is part of the value, and `\\#` after a blank is a literal `#`. Spaces and tabs around the key and the value are dropped. On the command line a key is `--key=VALUE` or `--key VALUE`, and a switch `--key` or `--no-key`.\n\
         \n\
         vtium reports, once at start, a top-level key it does not know, however often it appears, and a value it cannot use: a number out of bounds, which it clamps, and a value it cannot read, which it ignores, keeping the value the sources before it gave. An error in a section never drops a top-level key. It drops the section or the rule that holds it, or every section when it leaves their structure untrustworthy, as [Invalid files](head-file.md#invalid-files) describes. It reads at most {FILE_MAX} bytes of the file and {LINE_MAX} of a line.\n\
         \n\
         Four key bindings had other names before the maintainer renamed them to more accepted terms: {renamed}. vtium still reads each old name as its new one and reports each line that uses it, so that you can rename it. Where a source sets both names, the new one wins.\n\
         \n\
         `install/vtium.conf.example` in `crates/vtium` lists the same keys at their defaults, under the same sections, then the compiled-in looks, heads and rules. Source names the GitHub issue, the specification or the released program each key comes from.\n"
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
         `install/vtium.conf.example` carries these schemes for the six color keys, commented out, for color blindness and personal preference. vtium draws whatever colors are set and checks no contrast between a text and its box.\n\n\
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
    use vtium_settings::keys::Level;
    let set_in: Vec<&str> = [
        (Level::Top, "top level"),
        (Level::Look, "`[look NAME]`"),
        (Level::Head, "`[head NAME]`"),
    ]
    .into_iter()
    .filter(|(level, _)| k.levels.contains(*level))
    .map(|(_, place)| place)
    .chain((k.scope != Scope::Section).then_some("command line"))
    .collect();
    let set_in = set_in.join(", ");
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

/// A group's heading: `# -- TITLE ---`.
fn heading(title: &str) -> String {
    rule('-', title)
}

/// A section's heading, `# == TITLE ===`, then what the section covers.
fn section_heading(title: &str, about: &str) -> String {
    format!("{}\n#\n{}", rule('=', title), paragraphs("#", [about]))
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
                .example_paragraphs()
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
    paragraphs("##", only.example_paragraphs())
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
                "Alternatives for the six colors above. To use one, remove the # at the start of its six lines. vtium uses whatever colors you set; it does not check that text stands out against its box."
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
            format!("An example of vtium's configuration file. Copy it to /etc/vtium/vtium.conf and edit it. It is generated from {SOURCE}.").as_str(),
            "The settings come first, one per line: key=value. For an on/off setting, write its key to turn it on and no-key to turn it off. Each setting is shown commented out, at its default. An option on vtium's command line overrides these lines, and a [look] or [head] section below overrides both.",
            "A comment may leave out details that docs/config.md gives for each setting.",
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
    // the sections, the template's text, from which the compiled-in flow
    // is parsed too (ADR-1403 D3)
    let _ = write!(
        out,
        "\n{}\n{}",
        section_heading(SECTIONS_TITLE, SECTIONS_ABOUT),
        vtium_settings::TEMPLATE
    );
    out
}

/// The heading of the example's sections, below the keys.
const SECTIONS_TITLE: &str = "Looks, heads and rules";

/// What the example's sections are.
const SECTIONS_ABOUT: &str = "The sections after the settings. A look is a font and its size; a head is a screen with its own consoles; the rules send each display and each input device to a head. These are the sections vtium uses, as shipped, when vtium.conf has none.";

/// The most bytes the generated example may hold, so that at least 16 KiB of
/// vtium.conf's [`FILE_MAX`] (64 KiB) stay for a user's own heads, looks and
/// rules (ADR-1403 D3). A user starts from a copy of the example, and
/// vtium reads no file larger than [`FILE_MAX`], so every byte the example
/// holds is a byte its copy cannot give to the user's own sections. This
/// number is `FILE_MAX` minus that reserve. Raising it takes the reserve from
/// the user, unless `FILE_MAX` rises too, and `FILE_MAX` is a bound that
/// vtium-settings, its fuzz targets and ADR-1403 D3 fix. When the example
/// needs more room, shorten the comments in the key table first.
pub(super) const EXAMPLE_MAX: usize = 49_152;

/// The room the example keeps below [`EXAMPLE_MAX`] for the settings that
/// are planned: about 1.5 KiB for libseat's seat settings (three or four keys
/// at 350 bytes, as a key in the Displays section takes), and about 2 KiB
/// for further key bindings and kitty keyboard settings (five keys at 400
/// bytes). The estimate is 4 KiB with a margin of about half a key.
const EXAMPLE_ROOM: usize = 4_096;

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

    // ADR-1403 D3: the example leaves room for a user's own sections
    #[test]
    fn the_example_leaves_16_kib_of_the_file_bound() {
        let example = example();
        assert!(
            example.len() <= EXAMPLE_MAX,
            "the example holds {} bytes, more than {EXAMPLE_MAX}",
            example.len()
        );
        const { assert!(EXAMPLE_MAX + 16 * 1024 <= FILE_MAX) };
    }

    // a paragraph marked for the docs is in docs/config.md and not in the
    // example, and every other paragraph is in both
    #[test]
    fn a_docs_only_paragraph_is_in_the_markdown_and_not_in_the_example() {
        let flat = |text: &str| -> String {
            text.lines()
                .map(|l| l.trim_start_matches('#').trim())
                .collect::<Vec<_>>()
                .join(" ")
        };
        let example = flat(&example());
        let markdown = markdown();
        let mut left_out = 0;
        for k in vtium_settings::keys::KEYS.iter() {
            let shown: Vec<&str> = k.example_paragraphs().collect();
            let first = k.paragraphs().next().unwrap_or_default();
            let first = first.split_whitespace().collect::<Vec<_>>().join(" ");
            if !example.contains(&first) {
                // a key set on the command line alone is not in the example
                continue;
            }
            for p in k.paragraphs() {
                // the markdown holds each paragraph whole, on one line
                assert!(
                    markdown.contains(p),
                    "{}: {p:?} is not in docs/config.md",
                    k.name
                );
                let words = p.split_whitespace().collect::<Vec<_>>().join(" ");
                if shown.contains(&p) {
                    assert!(
                        example.contains(&words),
                        "{}: {p:?} is not in the example",
                        k.name
                    );
                } else {
                    left_out += 1;
                    assert!(
                        !example.contains(&words),
                        "{}: {p:?} is in the example",
                        k.name
                    );
                }
            }
        }
        assert!(left_out > 0);
    }

    // the example keeps room for the settings planned next
    #[test]
    fn the_example_keeps_room_for_the_planned_settings() {
        let example = example();
        assert!(
            example.len() + EXAMPLE_ROOM <= EXAMPLE_MAX,
            "the example holds {} bytes, which leaves {} of the {EXAMPLE_ROOM} bytes of room \
             below {EXAMPLE_MAX}; shorten the comments in the key table",
            example.len(),
            EXAMPLE_MAX.saturating_sub(example.len())
        );
    }

    // ADR-1403 D3: the example is a vtium.conf whose sections are the
    // compiled-in flow, read as vtium reads it, and whose keys, all
    // commented out, set nothing
    #[test]
    fn the_example_runs_the_compiled_in_flow() {
        let example = example();
        let flow = vtium_settings::parse_sections(&example)
            .expect("the example's sections read whole")
            .expect("the example holds a flow");
        // the same flow, each line counted in its own file
        let unlined = |mut c: vtium_settings::Config| {
            c.heads.iter_mut().for_each(|h| h.line = 0);
            c.rules.iter_mut().for_each(|r| r.line = 0);
            c
        };
        assert_eq!(
            unlined(flow.config),
            unlined(vtium_settings::Config::builtin())
        );
        let loaded = crate::config::load(&[], &mut |_: &std::path::Path| {
            Ok(Some(example.clone().into_bytes()))
        });
        assert_eq!(loaded.reports, []);
        assert_eq!(loaded.config, crate::config::Config::default());
    }

    // a vtium.conf copied from an earlier example has box-drawing heading
    // rules. They are comments, so vtium reads the file as it reads the
    // example, and the line editor keeps them as they are.
    #[test]
    fn a_file_with_box_drawing_heading_rules_reads_and_edits_as_before() {
        let example = example();
        let old: String = example
            .lines()
            .map(|l| {
                let mut line = l.to_owned();
                for (plain, flat, boxed) in [("# == ", '=', '═'), ("# -- ", '-', '─')] {
                    if let Some(rest) = l.strip_prefix(plain) {
                        line = rule(boxed, rest.trim_end_matches(flat).trim_end());
                    }
                }
                line + "\n"
            })
            .collect();
        assert_ne!(old, example);
        assert!(old.contains("# ── Font ─") && old.contains("# ══ Fonts and text ═"));
        let flow = vtium_settings::parse_sections(&old)
            .expect("the sections read whole")
            .expect("the file holds a flow");
        let want = vtium_settings::parse_sections(&example)
            .expect("the example's sections read whole")
            .expect("the example holds a flow");
        assert_eq!(flow.config, want.config);
        let loaded = crate::config::load(&[], &mut |_: &std::path::Path| {
            Ok(Some(old.clone().into_bytes()))
        });
        assert_eq!(loaded.reports, []);
        assert_eq!(loaded.config, crate::config::Config::default());
        let edit = |text: &str| {
            vtium_settings::edit_conf(text, &[vtium_settings::Edit::Set("font-size", "14")])
                .expect("the edit applies")
        };
        let edited = edit(&old);
        assert!(edited.contains("font-size=14") || edited.contains("font-size = 14"));
        assert_eq!(
            edited.matches("# ── ").count(),
            old.matches("# ── ").count()
        );
        assert_eq!(
            edited.matches("# ══ ").count(),
            old.matches("# ══ ").count()
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
            let loaded = crate::config::load(&[], &mut |_: &std::path::Path| {
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
        crate::config::load(&[], &mut |_: &std::path::Path| {
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
            "# An example of vtium's configuration file. Copy it to /etc/vtium/vtium.conf\n# and edit it."
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
            .filter(|(_, l)| l.starts_with("# == "))
            .map(|(i, _)| i)
            .collect();
        let sections = shown();
        // the keys' sections, then the heading of the looks, heads and rules
        assert_eq!(headings.len(), sections.len() + 1);
        let last = headings.last().map_or("", |&at| lines[at]);
        assert!(
            last.starts_with(&format!("# == {SECTIONS_TITLE} =")),
            "{last}"
        );
        for (n, (title, groups)) in sections.iter().enumerate() {
            let at = headings[n];
            assert!(
                lines[at].starts_with(&format!("# == {title} =")),
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
                lines[at + 3 + about].starts_with("# -- "),
                "{title}: a group follows"
            );
            let end = headings.get(n + 1).copied().unwrap_or(lines.len());
            let inside: Vec<&str> = lines[at..end]
                .iter()
                .filter(|l| l.starts_with("# -- "))
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
                .position(|l| l.starts_with(&format!("# -- {title} -")))
                .unwrap_or_else(|| panic!("no heading for {title}"));
            assert_eq!(lines[at].chars().count(), 78, "{title}");
            assert!(lines[at].ends_with("--"), "{title}");
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
            .skip_while(|l| !l.starts_with("# -- On-screen display -"))
            .skip(2)
            .take_while(|l| !l.is_empty())
            .collect();
        assert_eq!(
            osd.first().copied(),
            Some("## osd: What the on-screen display shows at the top of each screen after a")
        );
        assert!(osd.contains(&"##"), "paragraphs are separated");
        let next = osd
            .iter()
            .position(|l| l.starts_with("## osd-timeout: "))
            .expect("osd-timeout's paragraph");
        assert_eq!(osd[next - 1], "#", "a key break before osd-timeout");
        assert_eq!(
            osd[next - 2],
            "## A notice about a problem appears whatever `osd` says.",
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
            .split("# -- Keyboard -")
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
