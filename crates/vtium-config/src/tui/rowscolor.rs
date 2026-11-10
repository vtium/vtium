//! The colors of the Flow view as paths ([`super::rows`]), which a person
//! sets in a file of their own, `$XDG_CONFIG_HOME/vtium/vtium-config.conf`,
//! else `~/.config/vtium/vtium-config.conf`. vtium-config reads it once at
//! start, and vtium itself never reads it.
//!
//! The file sets colors by role, not a whole theme: the kind of box a
//! title names (`display`, `terminal`, `font-set`, `input`), the state a
//! border shows (`placed`, `waiting`, `set-aside`, `picked`), and the
//! lines between boxes (`line`). Each value is `#rrggbb / BASIC`, drawn as
//! the 24-bit color where the terminal takes 24-bit color and as the basic
//! color elsewhere; or a basic color alone; or `none`. A role the file
//! leaves out, or sets on a line vtium-config cannot read, takes the
//! `console` set's value. The same file holds vtium-config's own settings
//! above its sections, `draft` and `history` ([`crate::store`]). When the
//! file is missing, or has no `[colors]` section, vtium-config writes the
//! section once with three sets, `console` live and `muted` and `mono`
//! commented out, keeping every line the file had. `NO_COLOR` turns every
//! color off, whatever the file says.

use std::path::{Path, PathBuf};

use super::color::{Color, Style};

/// What a color in the file colors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Display,
    Terminal,
    FontSet,
    Input,
    Placed,
    Waiting,
    SetAside,
    Picked,
    Line,
}

impl Role {
    pub const ALL: [Role; 9] = [
        Role::Display,
        Role::Terminal,
        Role::FontSet,
        Role::Input,
        Role::Placed,
        Role::Waiting,
        Role::SetAside,
        Role::Picked,
        Role::Line,
    ];

    /// The role's key in the file.
    pub const fn key(self) -> &'static str {
        match self {
            Role::Display => "display",
            Role::Terminal => "terminal",
            Role::FontSet => "font-set",
            Role::Input => "input",
            Role::Placed => "placed",
            Role::Waiting => "waiting",
            Role::SetAside => "set-aside",
            Role::Picked => "picked",
            Role::Line => "line",
        }
    }

    const fn at(self) -> usize {
        match self {
            Role::Display => 0,
            Role::Terminal => 1,
            Role::FontSet => 2,
            Role::Input => 3,
            Role::Placed => 4,
            Role::Waiting => 5,
            Role::SetAside => 6,
            Role::Picked => 7,
            Role::Line => 8,
        }
    }
}

/// One role's color: a 24-bit color and the basic color drawn where the
/// terminal takes no 24-bit color; both `None` for no color.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Paint {
    pub rgb: Option<[u8; 3]>,
    pub basic: Option<Color>,
}

impl Paint {
    const fn basic(c: Color) -> Paint {
        Paint {
            rgb: None,
            basic: Some(c),
        }
    }

    const fn both(rgb: [u8; 3], c: Color) -> Paint {
        Paint {
            rgb: Some(rgb),
            basic: Some(c),
        }
    }
}

/// The colors of every role, and whether the terminal draws 24-bit color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    paints: [Paint; 9],
    pub truecolor: bool,
}

impl Default for Palette {
    fn default() -> Palette {
        Palette::console()
    }
}

/// The `console` set: the console's own basic colors.
const CONSOLE: [Paint; 9] = [
    Paint::basic(Color::Cyan),
    Paint::basic(Color::Yellow),
    Paint::basic(Color::Magenta),
    Paint::basic(Color::Green),
    Paint::basic(Color::White),
    Paint::basic(Color::Yellow),
    Paint::basic(Color::Grey),
    Paint::basic(Color::BrightMagenta),
    Paint::basic(Color::White),
];

/// The `muted` set: softer tones that read on black.
const MUTED: [Paint; 9] = [
    Paint::both([0x7f, 0xb3, 0xc8], Color::Cyan),
    Paint::both([0xd8, 0xc2, 0x7a], Color::Yellow),
    Paint::both([0xc4, 0x9a, 0xc9], Color::Magenta),
    Paint::both([0x8f, 0xbf, 0x8a], Color::Green),
    Paint::both([0xb8, 0xb8, 0xb0], Color::White),
    Paint::both([0xd9, 0xa3, 0x5b], Color::Yellow),
    Paint::both([0x6c, 0x6c, 0x68], Color::Grey),
    Paint::both([0xe0, 0x8f, 0xb0], Color::BrightMagenta),
    Paint::both([0x9a, 0x9a, 0x94], Color::White),
];

impl Palette {
    /// The `console` set, today's basic colors.
    pub const fn console() -> Palette {
        Palette {
            paints: CONSOLE,
            truecolor: false,
        }
    }

    /// The color of `role`.
    pub fn paint(&self, role: Role) -> Paint {
        of(&self.paints, role)
    }

    /// Text in `role`'s color: the 24-bit color where the terminal takes
    /// it, else the basic color, else the terminal's own.
    pub fn style(&self, role: Role) -> Style {
        let p = self.paint(role);
        Style {
            fg: p.basic,
            rgb: if self.truecolor { p.rgb } else { None },
            ..Style::PLAIN
        }
    }
}

/// `role`'s color in the set `set`.
fn of(set: &[Paint; 9], role: Role) -> Paint {
    set.get(role.at()).copied().unwrap_or_default()
}

/// The basic colors by name. Blue and bright blue are hard to read on
/// black, so the TUI draws neither ([`Color`]).
const BASIC: [(&str, Option<Color>); 16] = [
    ("black", Some(Color::Black)),
    ("red", Some(Color::Red)),
    ("green", Some(Color::Green)),
    ("yellow", Some(Color::Yellow)),
    ("blue", None),
    ("magenta", Some(Color::Magenta)),
    ("cyan", Some(Color::Cyan)),
    ("white", Some(Color::White)),
    ("bright-black", Some(Color::Grey)),
    ("bright-red", Some(Color::BrightRed)),
    ("bright-green", Some(Color::BrightGreen)),
    ("bright-yellow", Some(Color::BrightYellow)),
    ("bright-blue", None),
    ("bright-magenta", Some(Color::BrightMagenta)),
    ("bright-cyan", Some(Color::BrightCyan)),
    ("bright-white", Some(Color::BrightWhite)),
];

/// The basic color `name`, or why it is none.
fn basic(name: &str) -> Result<Color, String> {
    match BASIC.iter().find(|(n, _)| *n == name) {
        Some((_, Some(c))) => Ok(*c),
        Some((_, None)) => Err(format!(
            "{name} is hard to read on black, so vtium-config does not draw it"
        )),
        None => Err(format!("{name} is not one of the basic colors")),
    }
}

/// `#rrggbb` as its three bytes.
fn hex(text: &str) -> Option<[u8; 3]> {
    let digits = text.strip_prefix('#')?;
    if digits.len() != 6 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |at: usize| u8::from_str_radix(digits.get(at..at.saturating_add(2))?, 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// A role's value: `#rrggbb / BASIC`, a basic color alone, or `none`.
fn value(text: &str) -> Result<Paint, String> {
    if text == "none" {
        return Ok(Paint::default());
    }
    match text.split_once('/') {
        Some((rgb, name)) => {
            let rgb = hex(rgb.trim())
                .ok_or_else(|| format!("{} is not a color written as #rrggbb", rgb.trim()))?;
            Ok(Paint::both(rgb, basic(name.trim())?))
        }
        None => Ok(Paint::basic(basic(text)?)),
    }
}

/// The palette `text` sets, each role it leaves out at the `console`
/// set's value, and the lines it could not read, each with its number and
/// why.
pub fn parse(text: &str) -> (Palette, Vec<(usize, String)>) {
    let mut palette = Palette::console();
    let mut skipped = Vec::new();
    let mut in_colors = false;
    for (i, line) in text.lines().enumerate() {
        let n = i.saturating_add(1);
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_colors = section.trim() == "colors";
            if !in_colors {
                skipped.push((
                    n,
                    format!("[{}] is not a section of this file", section.trim()),
                ));
            }
            continue;
        }
        if !in_colors {
            // vtium-config's own settings, which the store reads
            let own = line
                .split_once('=')
                .is_some_and(|(k, _)| matches!(k.trim(), "draft" | "history"));
            if !own {
                skipped.push((n, "it is outside the [colors] section".to_owned()));
            }
            continue;
        }
        let Some((key, val)) = line.split_once('=') else {
            skipped.push((n, "it is not written as ROLE = COLOR".to_owned()));
            continue;
        };
        let Some(role) = Role::ALL.iter().find(|r| r.key() == key.trim()) else {
            skipped.push((n, format!("{} is not a role", key.trim())));
            continue;
        };
        match value(val.trim()) {
            Ok(p) => {
                if let Some(slot) = palette.paints.get_mut(role.at()) {
                    *slot = p;
                }
            }
            Err(why) => skipped.push((n, why)),
        }
    }
    (palette, skipped)
}

/// Where the file is: under `XDG_CONFIG_HOME`, else under `HOME`'s
/// `.config`; `None` when neither is set.
pub fn path(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let set = |name: &str| env(name).filter(|v| !v.is_empty());
    let base = match set("XDG_CONFIG_HOME") {
        Some(x) => PathBuf::from(x),
        None => PathBuf::from(set("HOME")?).join(".config"),
    };
    Some(base.join("vtium").join("vtium-config.conf"))
}

/// Whether the terminal takes 24-bit color: COLORTERM says so.
pub fn truecolor(env: &dyn Fn(&str) -> Option<String>) -> bool {
    env("COLORTERM").is_some_and(|v| v == "truecolor" || v == "24bit")
}

/// The line `role = value` of `paint`, as the file writes it.
fn line(role: Role, paint: Paint) -> String {
    let name = |c: Color| {
        BASIC
            .iter()
            .find(|(_, x)| *x == Some(c))
            .map_or("none", |(n, _)| n)
    };
    let value = match (paint.rgb, paint.basic) {
        (Some([r, g, b]), Some(c)) => format!("#{r:02x}{g:02x}{b:02x} / {}", name(c)),
        (_, Some(c)) => name(c).to_owned(),
        _ => "none".to_owned(),
    };
    format!("{} = {value}", role.key())
}

/// The file vtium-config writes when there is none: what it is for, the
/// roles, and three sets, `console` live and `muted` and `mono` commented
/// out.
pub fn template() -> String {
    let mut out = String::from(TEMPLATE_HEAD);
    out.push_str("[colors]\n# console: the console's own basic colors.\n");
    for r in Role::ALL {
        out.push_str(&line(r, of(&CONSOLE, r)));
        out.push('\n');
    }
    out.push_str("\n# muted: softer tones, with a basic color for each where the\n# terminal takes no 24-bit color.\n");
    for r in Role::ALL {
        out.push_str(&format!("# {}\n", line(r, of(&MUTED, r))));
    }
    out.push_str("\n# mono: no color at all; bold and underline carry the highlight.\n");
    for r in Role::ALL {
        out.push_str(&format!("# {}\n", line(r, Paint::default())));
    }
    out
}

const TEMPLATE_HEAD: &str = "\
# vtium-config.conf: the colors of vtium-config's Flow view. vtium-config
# reads this file when it starts. vtium itself never reads it.
#
# Each line sets the color of one role. A title takes the color of the
# kind of box it names: display, terminal, font-set or input. A border
# takes the color of the state of what its box holds: placed (in a path,
# or free to pick), waiting (connected but in no path), set-aside (cannot
# be picked) or picked (picked into the new path). line colors the lines
# between the boxes.
#
# A color is written #rrggbb / BASIC. vtium-config draws the 24-bit color
# #rrggbb where the terminal takes 24-bit color, that is where COLORTERM is
# truecolor or 24bit, and the basic color BASIC elsewhere. A color may also
# be a basic color alone, or none for no color at all. The basic colors are
# black, red, green, yellow, magenta, cyan, white, bright-black, bright-red,
# bright-green, bright-yellow, bright-magenta, bright-cyan and
# bright-white. Blue and bright-blue are hard to read on black, so
# vtium-config does not draw them.
#
# A role this file leaves out takes the console set's color. A # starts a
# comment only at the start of a line. With NO_COLOR set, vtium-config
# draws no color, whatever this file says. The lines above this comment,
# draft and history, are vtium-config's own settings, which it keeps up to
# date itself.
#
# The file holds three sets of colors. The console set is the one in use.
# To use another, put a # before each line of the console set and take the
# # away from each line of the other set.

";

/// Whether this process may write `file`: not as root into a folder that
/// belongs to another user, judged by the nearest folder above it that
/// exists.
fn may_write(file: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    if !rustix::process::geteuid().is_root() {
        return true;
    }
    file.ancestors()
        .skip(1)
        .find_map(|d| std::fs::metadata(d).ok())
        .is_some_and(|m| m.uid() == 0)
}

/// Writes the three sets at the end of `file`, after the lines `had` it
/// has; a file that is not there yet is made, with its folders, readable
/// by everyone and writable by its owner.
fn write(file: &Path, had: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o644)
        .open(file)?;
    let gap = match had {
        "" => "",
        t if t.ends_with('\n') => "\n",
        _ => "\n\n",
    };
    f.write_all(format!("{gap}{}", template()).as_bytes())
}

/// Whether `text` has a `[colors]` section.
fn has_colors(text: &str) -> bool {
    text.lines().any(|l| l.trim() == "[colors]")
}

/// The palette from the file `env` locates, and what the status line says
/// once about it: where vtium-config wrote it the first time, or the
/// lines it skipped.
pub fn load(env: &dyn Fn(&str) -> Option<String>) -> (Palette, Option<String>) {
    let tc = truecolor(env);
    let Some(file) = path(env) else {
        return (Palette::console(), None);
    };
    let shown = file.display();
    let console = Palette {
        truecolor: tc,
        ..Palette::console()
    };
    let text = match std::fs::read_to_string(&file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            return (
                console,
                Some(format!("Could not read the color file {shown}: {e}.")),
            );
        }
    };
    if !has_colors(&text) {
        // the first time: the three sets, the console set in use
        let note = if !may_write(&file) {
            None
        } else {
            match write(&file, &text) {
                Ok(()) => Some(format!("Wrote the color sets into {shown}.")),
                Err(e) => Some(format!("Could not write the color file {shown}: {e}.")),
            }
        };
        return (console, note);
    }
    let (mut palette, skipped) = parse(&text);
    palette.truecolor = tc;
    let note = skipped.first().map(|(n, why)| {
        let also = match skipped.len().saturating_sub(1) {
            0 => String::new(),
            1 => " and 1 more line".to_owned(),
            m => format!(" and {m} more lines"),
        };
        format!("The color file {shown} skips line {n}{also}: on line {n}, {why}.")
    });
    (palette, note)
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::expect_used,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    #[test]
    fn the_file_sets_each_role_and_the_rest_keep_the_console_set() {
        let (p, skipped) = parse(
            "# a comment\n[colors]\ndisplay = #7fb3c8 / cyan\npicked = none\ninput = bright-green\n",
        );
        assert!(skipped.is_empty(), "{skipped:?}");
        assert_eq!(
            p.paint(Role::Display),
            Paint::both([0x7f, 0xb3, 0xc8], Color::Cyan)
        );
        assert_eq!(p.paint(Role::Picked), Paint::default());
        assert_eq!(p.paint(Role::Input), Paint::basic(Color::BrightGreen));
        // a role the file leaves out takes the console set's color
        assert_eq!(p.paint(Role::Terminal), Paint::basic(Color::Yellow));
        assert_eq!(p.style(Role::Picked), Style::PLAIN);
    }

    #[test]
    fn a_line_it_cannot_read_is_skipped_and_named() {
        let (p, skipped) = parse(
            "display = red\n[colors]\ndisplay = blue\nwaiting = #12345 / red\nlines = white\nterminal\nline = bright-red\n",
        );
        let lines: Vec<usize> = skipped.iter().map(|(n, _)| *n).collect();
        assert_eq!(lines, [1, 3, 4, 5, 6]);
        assert!(skipped[1].1.contains("hard to read"), "{skipped:?}");
        assert_eq!(p.paint(Role::Display), Paint::basic(Color::Cyan));
        assert_eq!(p.paint(Role::Waiting), Paint::basic(Color::Yellow));
        assert_eq!(p.paint(Role::Line), Paint::basic(Color::BrightRed));
    }

    #[test]
    fn the_24_bit_color_is_drawn_only_where_the_terminal_takes_it() {
        let env = |v: Option<&'static str>| {
            move |name: &str| {
                (name == "COLORTERM")
                    .then(|| v.map(str::to_owned))
                    .flatten()
            }
        };
        assert!(truecolor(&env(Some("truecolor"))));
        assert!(truecolor(&env(Some("24bit"))));
        assert!(!truecolor(&env(Some("yes"))));
        assert!(!truecolor(&env(None)));
        let (mut p, _) = parse("[colors]\ndisplay = #7fb3c8 / cyan\n");
        assert_eq!(p.style(Role::Display).rgb, None);
        assert_eq!(p.style(Role::Display).fg, Some(Color::Cyan));
        p.truecolor = true;
        assert_eq!(p.style(Role::Display).rgb, Some([0x7f, 0xb3, 0xc8]));
        assert_eq!(p.style(Role::Display).sgr(true), "\x1b[38;2;127;179;200m");
        // NO_COLOR draws no color, 24-bit or not
        assert_eq!(p.style(Role::Display).sgr(false), "");
    }

    #[test]
    fn the_written_file_holds_three_sets_with_console_live() {
        let text = template();
        let (p, skipped) = parse(&text);
        assert!(skipped.is_empty(), "{skipped:?}");
        assert_eq!(p, Palette::console());
        for set in ["# console:", "# muted:", "# mono:"] {
            assert!(text.contains(set), "{set}");
        }
        // uncommenting muted, and commenting console, reads the muted set
        let muted: String = text
            .lines()
            .map(|l| {
                if Role::ALL
                    .iter()
                    .any(|r| l.starts_with(&format!("{} = ", r.key())))
                {
                    format!("# {l}")
                } else if l.starts_with("# ")
                    && Role::ALL
                        .iter()
                        .any(|r| l.starts_with(&format!("# {} = #", r.key())))
                {
                    l.trim_start_matches("# ").to_owned()
                } else {
                    l.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let (p, skipped) = parse(&muted);
        assert!(skipped.is_empty(), "{skipped:?}");
        assert_eq!(p.paints, MUTED);
        assert!(text.lines().all(|l| l.chars().count() <= 78), "{text}");
    }

    #[test]
    fn the_first_run_writes_the_file_once_where_xdg_says() {
        let root = std::env::temp_dir().join(format!("vtium-config-colors-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let xdg = root.join("config");
        let env = |name: &str| match name {
            "XDG_CONFIG_HOME" => Some(xdg.display().to_string()),
            "HOME" => Some("/nonexistent".to_owned()),
            _ => None,
        };
        let file = xdg.join("vtium").join("vtium-config.conf");
        let (p, note) = load(&env);
        assert_eq!(p, Palette::console());
        assert_eq!(
            note,
            Some(format!("Wrote the color sets into {}.", file.display()))
        );
        assert_eq!(
            std::fs::read_to_string(&file).expect("the file"),
            template()
        );
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file)
            .expect("the file")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o644);
        // the next start reads it, and says nothing; vtium-config's own
        // settings above the section are no colors and no trouble
        std::fs::write(
            &file,
            "draft = north\nhistory = 5\n[colors]\nline = bright-red\n",
        )
        .expect("an edit");
        let (p, note) = load(&env);
        assert_eq!(
            (p.paint(Role::Line), note),
            (Paint::basic(Color::BrightRed), None)
        );
        // a line it cannot read is named in one message
        std::fs::write(&file, "[colors]\nline = brown\n").expect("an edit");
        let (_, note) = load(&env);
        let note = note.expect("a message");
        assert!(note.contains("skips line 2"), "{note}");
        // a file the store made, with no colors yet, keeps its lines and
        // gains the sets after them
        std::fs::write(&file, "draft = north\n").expect("the store's file");
        let (p, note) = load(&env);
        assert_eq!(p, Palette::console());
        assert!(note.is_some_and(|n| n.starts_with("Wrote the color sets")));
        let text = std::fs::read_to_string(&file).expect("the file");
        assert_eq!(text, format!("draft = north\n\n{}", template()));
        let _ = std::fs::remove_dir_all(&root);
        // with no HOME and no XDG_CONFIG_HOME there is no file to read
        assert_eq!(load(&|_: &str| None), (Palette::console(), None));
    }
}
