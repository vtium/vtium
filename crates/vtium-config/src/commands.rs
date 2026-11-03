//! The commands that list, check and change the console's fonts and
//! settings (ADR-1201 D2): `fonts`, `check FILE`, `config KEY`,
//! `remove-frit NAME` and `new`'s non-interactive forms. Each is
//! non-interactive, prints one line a result, tab-separated, and exits 0
//! on success, 1 on a failure and 2 on a usage error.
//!
//! A command that changes something computes a [`Plan`] and hands it to
//! [`crate::apply::make`]: run as a user it writes a draft and prints the
//! command that installs it, and run as root it installs at once through
//! `apply`'s steps, so a script never needs the TUI.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use vtium_settings::keys::{self, Default as KeyDefault, Kind, Scope};
use vtium_settings::{Edit, edit_conf, setting, switch};

use crate::draft::{FileEdit, Plan, Removal};
use crate::frit::{FRIT_BYTES_MAX, Frit, RESERVED, valid_name};
use crate::specimen::Sources;
use crate::tui::{Paths, model::FontState};

/// The usage error `why`, as every command reports one.
fn usage(err: &mut dyn Write, why: &str) -> u8 {
    let _ = writeln!(err, "vtium-config: {why}\nTry vtium-config --help.");
    2
}

/// The failure `why`.
fn failed(err: &mut dyn Write, why: &str) -> u8 {
    let _ = writeln!(err, "vtium-config: {why}");
    1
}

/// vtium.conf in `paths`' configuration directory: its path and its text,
/// `None` when there is none.
fn conf(paths: &Paths) -> Result<(PathBuf, Option<String>), String> {
    let path = paths.configdir.join("vtium.conf");
    let text = crate::tui::read_conf(&path)?;
    Ok((path, text))
}

/// `fonts`: each frit and built-in font, one line each, tab-separated: its
/// name, its state, and the sources this machine lacks or refuses, which
/// the glyph helper finds under its own sandbox, so this process parses
/// no font (ADR-1201 D3).
pub fn fonts(words: &[String], paths: &Paths, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    if !words.is_empty() {
        return usage(err, "fonts takes no argument");
    }
    let fonts = crate::tui::fonts(&paths.frits);
    let reading: Vec<String> = fonts
        .iter()
        .filter(|f| f.state == FontState::Reads)
        .map(|f| f.name.clone())
        .collect();
    // one helper answers for every frit, under one wait
    let mut found = crate::tui::find_sources(&paths.frits, &paths.helper, &reading).into_iter();
    for font in fonts {
        let (state, left_out) = match &font.state {
            FontState::BuiltIn => ("built in".to_owned(), String::new()),
            FontState::Refused(why) => (format!("does not read: {why}"), String::new()),
            FontState::Reads => match found.next().map(|(_, f)| f) {
                Some(Sources::LeftOut(left)) => ("reads".to_owned(), left_out_text(&left)),
                Some(Sources::DrawsNothing(why)) => {
                    (format!("draws nothing: {why}"), String::new())
                }
                // no answer came: nothing is known of its sources, and it
                // is not said to draw nothing
                Some(Sources::Unknown(why)) => ("reads".to_owned(), format!("unknown: {why}")),
                None => ("reads".to_owned(), String::new()),
            },
        };
        let _ = writeln!(out, "{}\t{state}\t{left_out}", font.name);
    }
    0
}

/// The sources left out, as `fonts` lists them: each source's spelling, a
/// refused one marked so.
pub fn left_out_text(left: &[crate::serve::LeftOut]) -> String {
    left.iter()
        .map(|l| {
            if l.missing {
                l.spelling.clone()
            } else {
                format!("{} (refused)", l.spelling)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// `check FILE`: a frit, a file named `*.vtfrit`, read strictly by the
/// helper's frit reader; any other file as vtium.conf, checked by the
/// console's own `vtium check-config` on a copy of it, each problem named
/// with FILE and its line. Writes nothing.
pub fn check(words: &[String], paths: &Paths, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let [file] = words else {
        return usage(err, "check takes one file");
    };
    let path = Path::new(file);
    if path.extension().is_some_and(|e| e == "vtfrit") {
        return match Frit::load(path) {
            Ok(frit) => {
                for warning in &frit.warnings {
                    let _ = writeln!(out, "warning\t{warning}");
                }
                let _ = writeln!(out, "ok\t{file}");
                0
            }
            Err(e) => failed(err, &e.to_string()),
        };
    }
    let text = match crate::tui::read_conf(path) {
        Ok(Some(text)) => text,
        Ok(None) => return failed(err, &format!("{file}: no such file")),
        Err(why) => return failed(err, &why),
    };
    let copy = match crate::apply::Copy::new("vtium.conf", text.as_bytes()) {
        Ok(copy) => copy,
        Err(why) => return failed(err, &why),
    };
    let ran = std::process::Command::new(&paths.vtium)
        .arg("check-config")
        .arg("--configdir")
        .arg(copy.dir())
        .stdin(std::process::Stdio::null())
        .output();
    let ran = match ran {
        Ok(ran) => ran,
        Err(e) => {
            return failed(err, &format!("{}: {e}", paths.vtium.to_string_lossy()));
        }
    };
    let copied = copy.path.to_string_lossy().into_owned();
    for line in String::from_utf8_lossy(&ran.stderr).lines() {
        let _ = writeln!(out, "{}", line.replace(&copied, file));
    }
    if ran.status.success() {
        let _ = writeln!(out, "ok\t{file}");
        0
    } else {
        1
    }
}

/// A key's default, as `config KEY` prints it.
fn default_text(key: &keys::Key) -> String {
    match key.default {
        KeyDefault::Switch(true) => "on".to_owned(),
        KeyDefault::Switch(false) => "off".to_owned(),
        KeyDefault::Number(n) => n.to_string(),
        KeyDefault::Text(t) => t.to_owned(),
        KeyDefault::Command(words) => words.join(" "),
        KeyDefault::Rgb([r, g, b]) => format!("{r},{g},{b}"),
        KeyDefault::Unset => "unset".to_owned(),
    }
}

/// The value `text`, vtium.conf, gives `key`, as `config KEY` prints it,
/// and whether the file sets it.
pub fn value_text(key: &keys::Key, text: Option<&str>) -> (String, bool) {
    let set = match key.kind {
        Kind::Switch => text
            .and_then(|t| switch(t, key.name))
            .map(|on| if on { "on" } else { "off" }.to_owned()),
        _ => text.and_then(|t| setting(t, key.name)),
    };
    match set {
        Some(value) => (value, true),
        None => (default_text(key), false),
    }
}

/// The key vtium.conf takes as `name`, or why there is none.
pub fn conf_key(name: &str) -> Result<&'static keys::Key, String> {
    match keys::find(name) {
        Some(key) if key.scope == Scope::CommandLine => Err(format!(
            "{name} is set on vtium's command line only, never in vtium.conf"
        )),
        Some(key) => Ok(key),
        None => Err(format!(
            "vtium.conf has no key named {name:?}; docs/config.md lists them"
        )),
    }
}

/// The edit that sets `key` to `value`, or why vtium would not take it.
/// A switch takes `on` or `off`, and a key of a fixed set of words one of
/// them; any other value goes to the console's own check when the change
/// is installed.
pub fn set_edit<'a>(key: &'static keys::Key, value: &'a str) -> Result<Edit<'a>, String> {
    match key.kind {
        Kind::Switch => match value {
            "on" => Ok(Edit::Switch(key.name, true)),
            "off" => Ok(Edit::Switch(key.name, false)),
            _ => Err(format!("{} is a switch, on or off", key.name)),
        },
        Kind::Choice(words) if !words.contains(&value) => {
            Err(format!("{} takes one of {}", key.name, words.join(", ")))
        }
        _ => Ok(Edit::Set(key.name, value)),
    }
}

/// The plan that makes `edit` to vtium.conf in `paths`.
fn conf_plan(paths: &Paths, edit: Edit<'_>) -> Result<Plan, String> {
    let (path, old) = conf(paths)?;
    let new = edit_conf(old.as_deref().unwrap_or_default(), &[edit])
        .map_err(|e| crate::tui::words::conf_edit(&e))?;
    let files = if old.as_deref() == Some(&new) || (old.is_none() && new.is_empty()) {
        Vec::new()
    } else {
        vec![FileEdit { path, old, new }]
    };
    Ok(Plan {
        files,
        removals: Vec::new(),
    })
}

/// `config KEY` prints the value vtium.conf gives the key, tab-separated
/// with where it comes from, `vtium.conf` or `default`; `config KEY VALUE`
/// sets it and `config --unset KEY` leaves it at its default, each one
/// change to vtium.conf, line by line (ADR-1201 D5).
pub fn config(words: &[String], paths: &Paths, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let (name, value, unset) = match words {
        [flag, name] if flag == "--unset" => (name, None, true),
        [name] => (name, None, false),
        [name, value] => (name, Some(value.as_str()), false),
        _ => return usage(err, "config takes KEY, KEY VALUE or --unset KEY"),
    };
    let key = match conf_key(name) {
        Ok(key) => key,
        Err(why) => return failed(err, &why),
    };
    let edit = match (value, unset) {
        (Some(value), _) => match set_edit(key, value) {
            Ok(edit) => edit,
            Err(why) => return failed(err, &why),
        },
        (None, true) => Edit::Unset(key.name),
        (None, false) => {
            return match conf(paths) {
                Ok((_, text)) => {
                    let (value, set) = value_text(key, text.as_deref());
                    let from = if set { "vtium.conf" } else { "default" };
                    let _ = writeln!(out, "{}\t{value}\t{from}", key.name);
                    0
                }
                Err(why) => failed(err, &why),
            };
        }
    };
    match conf_plan(paths, edit) {
        Ok(plan) => crate::apply::make(&plan, paths, out, err),
        Err(why) => failed(err, &why),
    }
}

/// `remove-frit NAME`: removes the frit `NAME.vtfrit`, refusing a built-in
/// font, a name with no frit, and a frit vtium.conf's `font-name` or a
/// head's profile names.
pub fn remove_frit(
    words: &[String],
    paths: &Paths,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let [name] = words else {
        return usage(err, "remove-frit takes one frit's name");
    };
    if RESERVED.contains(&name.as_str()) {
        return failed(err, &format!("{name} is a built-in font, not a frit"));
    }
    if !valid_name(name) {
        return failed(err, &format!("{name:?} is no frit's name"));
    }
    let path = paths.frits.join(format!("{name}.vtfrit"));
    let old = match read_bounded(&path) {
        Ok(bytes) => bytes,
        Err(why) => return failed(err, &why),
    };
    let heads = crate::tui::read_conf(&paths.configdir.join("heads.conf"));
    match (conf(paths), heads) {
        (Ok((_, text)), Ok(heads)) => {
            if let Some(by) = crate::apply::naming(name, text.as_deref(), heads.as_deref()) {
                return failed(
                    err,
                    &format!(
                        "{name} is the frit {by} names, so it is not removed; name another font there first"
                    ),
                );
            }
        }
        (Err(why), _) | (_, Err(why)) => return failed(err, &why),
    }
    let plan = Plan {
        files: Vec::new(),
        removals: vec![Removal { path, old }],
    };
    crate::apply::make(&plan, paths, out, err)
}

/// A frit's file read whole, at most [`FRIT_BYTES_MAX`] bytes, only a
/// regular file.
fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let shown = path.display();
    let meta = std::fs::metadata(path).map_err(|e| format!("{shown}: {e}"))?;
    if !meta.is_file() {
        return Err(format!("{shown}: not a regular file"));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| {
            f.take(u64::try_from(FRIT_BYTES_MAX).unwrap_or(0).saturating_add(1))
                .read_to_end(&mut bytes)
        })
        .map_err(|e| format!("{shown}: {e}"))?;
    if bytes.len() > FRIT_BYTES_MAX {
        return Err(format!("{shown}: larger than {FRIT_BYTES_MAX} bytes"));
    }
    Ok(bytes)
}

/// What `new --template` prints: a frit with each key it takes explained,
/// to edit and install with `new --from FILE`.
pub const TEMPLATE: &str = "\
# A frit: a font set file, NAME.vtfrit in /etc/vtium/fonts, which the
# console's glyph helper draws from. One key=value a line; # starts a
# comment. vtium-config new --from FILE checks this file and installs it.
#
# The frit's name, as font-name and OSC 50 name it: letters, digits, '.',
# '_', '+' and '-'. Left out, the file's name less .vtfrit.
#name=myfont
#
# The fonts, in order: each later source fills only the characters the
# ones before it lack. A source is font:FAMILY, a family matched on the
# machine that draws it; kbd:NAME, a kbd console font; file:PATH, a PSF
# file; bdf:PATH, a BDF font; outline:PATH, a TrueType or OpenType file;
# vconsole, the console font vconsole.conf names; console:VT, the font the
# kernel's console held at start; or builtin:unifont or builtin:unscii-8.
source=font:DejaVu Sans Mono
#source=builtin:unifont
#
# The cell in pixels, WIDTHxHEIGHT, each side 6 to 96. Left out, the first
# source sets it.
#cell=8x16
#
# An outline frit's starting size in points, 6 to 96.
#size=12
#
# How outline glyphs are hinted: light, the default, or full.
#hinting=light
";

/// A source as `new` takes it: a source's spelling when it starts with a
/// form a frit's `source=` takes, and otherwise a font family, `font:`.
fn source_spelling(word: &str) -> String {
    const FORMS: [&str; 7] = [
        "font:", "kbd:", "file:", "bdf:", "outline:", "console:", "builtin:",
    ];
    if word == "vconsole" || FORMS.iter().any(|f| word.starts_with(f)) {
        word.to_owned()
    } else {
        format!("font:{word}")
    }
}

/// A frit's name made from a font's: lowercase letters and digits, each
/// other run of characters one `-`, at most 64 characters.
pub fn name_from(font: &str) -> String {
    let family = font.split_once(':').map_or(font, |(_, rest)| rest);
    let family = family.rsplit('/').next().unwrap_or(family);
    let mut name = String::new();
    for c in family.chars() {
        if c.is_ascii_alphanumeric() {
            name.push(c.to_ascii_lowercase());
        } else if !name.is_empty() && !name.ends_with('-') {
            name.push('-');
        }
    }
    let name: String = name.trim_end_matches('-').chars().take(64).collect();
    name.trim_end_matches('-').to_owned()
}

/// What `new`'s words ask for.
struct NewArgs {
    fonts: Vec<String>,
    name: Option<String>,
    cell: Option<String>,
    size: Option<String>,
    from: Option<String>,
    template: bool,
}

fn new_args(words: &[String]) -> Result<NewArgs, String> {
    let mut a = NewArgs {
        fonts: Vec::new(),
        name: None,
        cell: None,
        size: None,
        from: None,
        template: false,
    };
    let mut it = words.iter();
    while let Some(word) = it.next() {
        let mut value = |flag: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| format!("new's {flag} takes a value"))
        };
        match word.as_str() {
            "--template" => a.template = true,
            "--from" => a.from = Some(value("--from")?),
            "--name" => a.name = Some(value("--name")?),
            "--cell" => a.cell = Some(value("--cell")?),
            "--size" => a.size = Some(value("--size")?),
            w if w.starts_with("--") => return Err(format!("new takes no option {w}")),
            w => a.fonts.push(w.to_owned()),
        }
    }
    Ok(a)
}

/// A new frit's text from `new`'s fonts and options.
pub fn frit_from(name: &str, fonts: &[String], cell: Option<&str>, size: Option<&str>) -> String {
    let mut text = format!("# {name}: made by vtium-config new\nname={name}\n");
    for font in fonts {
        text.push_str(&format!("source={}\n", source_spelling(font)));
    }
    if let Some(cell) = cell {
        text.push_str(&format!("cell={cell}\n"));
    }
    if let Some(size) = size {
        text.push_str(&format!("size={size}\n"));
    }
    text
}

/// The plan that installs the frit `text` as `name` in `paths`' frits,
/// once the frit reader takes it strictly, as the helper reads it.
pub fn frit_plan(paths: &Paths, name: &str, text: &str) -> Result<Plan, String> {
    if !valid_name(name) || RESERVED.contains(&name) {
        return Err(format!(
            "{name:?} cannot name a frit: 1 to 64 letters, digits, '.', '_', '+' and '-', and not a built-in font's name"
        ));
    }
    let path = paths.frits.join(format!("{name}.vtfrit"));
    let frit = Frit::parse(text.as_bytes(), &path, name).map_err(|e| e.to_string())?;
    if frit.name != name {
        return Err(format!(
            "the frit says name={}, but it is installed as {name}; make them one",
            frit.name
        ));
    }
    let old = match std::fs::read(&path) {
        Ok(bytes) => Some(
            String::from_utf8(bytes)
                .map_err(|_| format!("{}: the installed frit is not UTF-8 text", path.display()))?,
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    if old.as_deref() == Some(text) {
        return Ok(Plan::default());
    }
    Ok(Plan {
        files: vec![FileEdit {
            path,
            old,
            new: text.to_owned(),
        }],
        removals: Vec::new(),
    })
}

/// `new`'s non-interactive forms (ADR-1200 D4, ADR-1201 D9): `new
/// --template` prints a frit to edit; `new --from FILE [--name NAME]`
/// installs the frit in FILE; `new FONT... [--name NAME] [--cell WxH]
/// [--size POINTS]` makes a frit of the fonts, in order, each a family or
/// a source as a frit spells it.
pub fn new(words: &[String], paths: &Paths, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let a = match new_args(words) {
        Ok(a) => a,
        Err(why) => return usage(err, &why),
    };
    if a.template {
        if !a.fonts.is_empty() || a.from.is_some() || a.name.is_some() {
            return usage(err, "new --template takes nothing else");
        }
        let _ = write!(out, "{TEMPLATE}");
        return 0;
    }
    let (name, text) = match (&a.from, a.fonts.is_empty()) {
        (Some(file), true) if a.cell.is_none() && a.size.is_none() => {
            let bytes = match read_bounded(Path::new(file)) {
                Ok(bytes) => bytes,
                Err(why) => return failed(err, &why),
            };
            let Ok(text) = String::from_utf8(bytes) else {
                return failed(err, &format!("{file}: not UTF-8 text"));
            };
            let stem = Path::new(file)
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.strip_suffix(".vtfrit").unwrap_or(n).to_owned())
                .unwrap_or_default();
            let named = Frit::parse(text.as_bytes(), Path::new(file), &stem)
                .map(|f| f.name)
                .unwrap_or(stem);
            (a.name.clone().unwrap_or(named), text)
        }
        (None, false) => {
            let name = a
                .name
                .clone()
                .unwrap_or_else(|| a.fonts.first().map(|f| name_from(f)).unwrap_or_default());
            let text = frit_from(&name, &a.fonts, a.cell.as_deref(), a.size.as_deref());
            (name, text)
        }
        _ => {
            return usage(
                err,
                "new takes FONT... with --name, --cell and --size, or --from FILE with --name, or --template",
            );
        }
    };
    match frit_plan(paths, &name, &text) {
        Ok(plan) => crate::apply::make(&plan, paths, out, err),
        Err(why) => failed(err, &why),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_font_names_a_frit_in_lowercase_words() {
        assert_eq!(name_from("JetBrains Mono"), "jetbrains-mono");
        assert_eq!(name_from("font:Terminus 16"), "terminus-16");
        assert_eq!(
            name_from("bdf:/usr/share/fonts/misc/Tiny5.bdf"),
            "tiny5-bdf"
        );
        assert_eq!(name_from("--"), "");
    }

    #[test]
    fn a_family_becomes_a_font_source_and_a_spelling_stays() {
        assert_eq!(source_spelling("JetBrains Mono"), "font:JetBrains Mono");
        assert_eq!(source_spelling("kbd:ter-v16n"), "kbd:ter-v16n");
        assert_eq!(source_spelling("vconsole"), "vconsole");
    }

    #[test]
    fn the_template_reads_as_a_frit() {
        let frit = Frit::parse(TEMPLATE.as_bytes(), Path::new("t.vtfrit"), "t");
        assert!(frit.is_ok(), "{frit:?}");
    }
}
