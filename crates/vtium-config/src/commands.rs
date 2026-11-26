//! The commands that list, check and change the console's fonts and
//! settings (ADR-1201 D2): `fonts`, `check FILE`, `remove-frit NAME` and
//! `new-frit`'s non-interactive forms, and the key checks `get` and `set`
//! share. Each is non-interactive, prints one line a result,
//! tab-separated, and exits 0 on success, 1 on a failure and 2 on a usage
//! error. The flow's commands, `get` and `set` among them, are
//! [`crate::flowcli`]'s.
//!
//! `new-frit` and `remove-frit` keep their change in the current draft's
//! font set folder, as the TUI does, and install nothing
//! ([`crate::flowcli::font_set`]). `install` installs the draft, so a
//! script never needs the TUI.
//!
//! References:
//! - ADR-1403 D8, "Every font set change is kept at once": the two commands write the current draft's folder and install nothing.
//! - ADR-1201 D2: a command that writes installs nothing, and `install` installs.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use vtium_settings::fonts_report::Named;
use vtium_settings::keys::{self, Default as KeyDefault, Kind, Scope};
use vtium_settings::{Edit, fonts_report, setting, switch};

use crate::frit::{FRIT_BYTES_MAX, Frit, FritError, Origin, RESERVED, valid_name};
use crate::needs::{Differ, Differs};
use crate::specimen::{Found, Sources};
use crate::tui::{Paths, model::FontState};
use vtium_font::set::layer;

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
/// name, its state, and the sources this machine lacks or refuses. The
/// glyph helper finds those sources under its own sandbox, so this process
/// parses no font. After the list, a report in sentences names each font a
/// set needs and this machine lacks, also for a set with no font left,
/// each installed font of a lower version than the set asks for, and each
/// font file whose SHA-256 differs, with the font's upstream page where
/// the set gives one. A last line points to the help page on installing
/// fonts. A line of the report holds no tab, so it never reads as a font's
/// line.
///
/// References:
/// - ADR-1201 D3: vtium-config reads a font only through the sandboxed helper.
/// - ADR-1204 D7: what the report names, and the page it points to.
pub fn fonts(words: &[String], paths: &Paths, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    match words {
        [] => {}
        [files] if files == "--files" => return font_files(paths, out, err),
        [install, rest @ ..] if install == "--install-system" => {
            return install_system(rest, paths, out, err);
        }
        _ => {
            return usage(
                err,
                "fonts takes no argument, --files, or --install-system FILE [--replace]",
            );
        }
    }
    let frits = match &paths.frits {
        Ok(frits) => frits,
        Err(why) => return failed(err, why),
    };
    let fonts = crate::tui::fonts(frits);
    // each frit that reads, with its layers in place, as the helper is
    // handed it; the parsed frit names what each source needs
    let read: Vec<(String, Result<Frit, String>)> = fonts
        .iter()
        .filter(|f| f.state == FontState::Reads)
        .map(|f| {
            let path = crate::switch::frit_file(frits, &f.name)
                .unwrap_or_else(|| frits.join(format!("{}.vtfrit", f.name)));
            let layers = frits.join(layer::DIR);
            (
                f.name.clone(),
                Frit::load_in(&path, &layers).map_err(|e| e.to_string()),
            )
        })
        .collect();
    let texts: Vec<crate::serve::FritText> = read
        .iter()
        .filter_map(|(name, frit)| {
            frit.as_ref().ok().map(|f| crate::serve::FritText {
                name: name.clone(),
                text: f.text.clone(),
            })
        })
        .collect();
    // one helper answers for every frit, under one wait
    let mut answers = crate::specimen::found_of(&paths.helper, texts).into_iter();
    let mut found = read.into_iter().map(|(name, frit)| match frit {
        Ok(frit) => {
            let found = answers.next().unwrap_or_else(|| Found {
                sources: Sources::Unknown("the font helper did not answer".to_owned()),
                left_out: Vec::new(),
                differs: Vec::new(),
            });
            (name, Some(frit), found)
        }
        Err(why) => (
            name,
            None,
            Found {
                sources: Sources::DrawsNothing(why, Vec::new()),
                left_out: Vec::new(),
                differs: Vec::new(),
            },
        ),
    });
    let mut report = Vec::new();
    for font in fonts {
        let (state, left_out) = match &font.state {
            FontState::BuiltIn => ("built in".to_owned(), String::new()),
            FontState::Refused(why) => (format!("cannot be read: {why}"), String::new()),
            FontState::Reads => match found.next() {
                Some((name, frit, found)) => {
                    // a set with no font left names the fonts it lacks
                    // as a set with some left does
                    if let Some(frit) = &frit {
                        report.extend(needs_report(&name, frit, &found.left_out, &found.differs));
                    }
                    match found.sources {
                        Sources::LeftOut(left) => ("reads".to_owned(), left_out_text(&left)),
                        Sources::DrawsNothing(why, _) => {
                            (format!("no font file found: {why}"), String::new())
                        }
                        // no answer came: nothing is known of its sources,
                        // and it is not said to draw nothing
                        Sources::Unknown(why) => ("reads".to_owned(), format!("unknown: {why}")),
                    }
                }
                None => ("reads".to_owned(), String::new()),
            },
        };
        let _ = writeln!(out, "{}\t{state}\t{left_out}", font.name);
    }
    if !report.is_empty() {
        report.push(fonts_report::how_to_install());
    }
    for line in report {
        let _ = writeln!(out, "{line}");
    }
    0
}

/// The report's lines for the frit `set`: one for each font the helper
/// left out as missing, and for each font that differs from what its
/// source needs, each followed by the font's upstream page where the
/// source gives one. `left` and `differs` are the helper's answer, and
/// `frit` the frit with its layers in place, which says what each source
/// needs. A source the helper refused is not a font to install, so it is
/// not reported here.
fn needs_report(
    set: &str,
    frit: &Frit,
    left: &[crate::serve::LeftOut],
    differs: &[Differs],
) -> Vec<String> {
    // the source a line of the answer names: its line in the frit's own
    // text, where a layer's source names its layer= line, and its spelling
    let source_of = |line: usize, spelling: &str| {
        frit.sources
            .iter()
            .find(|s| frit.place(s.line).own == line && s.spelling == spelling)
    };
    let mut out = Vec::new();
    let mut with_page = |text: String, source: &crate::frit::Source| {
        out.push(text);
        if let Some(url) = &source.needs.url {
            out.push(fonts_report::page(url));
        }
    };
    // the source whose strike= line a line of the answer names, and the
    // strike's file, which the report names in place of the source's
    let strike_of = |line: usize, spelling: &str| {
        frit.sources.iter().find_map(|s| {
            s.strikes
                .iter()
                .find(|k| frit.place(k.line).own == line && k.spelling == spelling)
                .map(|k| (s, k))
        })
    };
    for l in left.iter().filter(|l| l.missing) {
        // a missing strike is named by its own file, with no version,
        // because version= describes the source's font, and with the page
        // of the source it is a size of
        let Some(source) = source_of(l.line, &l.spelling) else {
            if let Some((source, strike)) = strike_of(l.line, &l.spelling)
                && let Some((font, named)) = strike_font(&strike.origin)
            {
                with_page(fonts_report::missing(set, &font, named), source);
            }
            continue;
        };
        let Some(font) = source.font_name() else {
            continue;
        };
        let version = source.needs.version.as_ref().map(|v| v.spelling.as_str());
        let lacks = differs.iter().find_map(|d| match d.what {
            Differ::Lacks(c) if d.line == l.line && d.spelling == l.spelling => {
                Some(format!("U+{c:04X}"))
            }
            _ => None,
        });
        // a font named by its path is a file this machine lacks, not a font
        // it has not installed
        let named = match source.origin {
            Origin::File(_) | Origin::Bdf(_) | Origin::Hex(_) | Origin::Outline(_) => Named::ByPath,
            _ => Named::ByName,
        };
        let text = match (version, lacks) {
            (None, None) => fonts_report::missing(set, &font, named),
            (Some(v), None) => fonts_report::missing_version(set, &font, v, named),
            (None, Some(c)) => fonts_report::lacks(set, &font, &c, named),
            (Some(v), Some(c)) => fonts_report::lacks_version(set, &font, v, &c, named),
        };
        with_page(text, source);
    }
    for d in differs {
        let (source, font) = match source_of(d.line, &d.spelling) {
            Some(source) => (source, source.font_name()),
            None => match strike_of(d.line, &d.spelling) {
                Some((source, strike)) => {
                    (source, strike_font(&strike.origin).map(|(font, _)| font))
                }
                None => continue,
            },
        };
        let Some(font) = font else {
            continue;
        };
        let text = match &d.what {
            Differ::Lacks(_) => continue,
            Differ::Older(found) => {
                let Some(version) = &source.needs.version else {
                    continue;
                };
                fonts_report::older(set, &font, &version.spelling, found)
            }
            Differ::OtherHash(_) => fonts_report::other_file(set, &font),
        };
        with_page(text, source);
    }
    out
}

/// The font a strike names, as the report names it, and how the frit
/// names it: a `file:` or `bdf:` strike by its file's path, and a `kbd:`
/// strike by its kbd font's name.
fn strike_font(origin: &Origin) -> Option<(String, Named)> {
    match origin {
        Origin::File(path) | Origin::Bdf(path) => Some((path.display().to_string(), Named::ByPath)),
        Origin::Kbd(name) => Some((name.clone(), Named::ByName)),
        _ => None,
    }
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
/// helper's frit reader, and any other file as vtium.conf, judged by the
/// console's own `vtium check-config`, both its parts in one run
/// (ADR-1403 D3), each problem named with FILE and its line. Writes
/// nothing.
pub fn check(words: &[String], paths: &Paths, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let [file] = words else {
        return usage(err, "check takes one file");
    };
    let path = Path::new(file);
    if path.extension().is_some_and(|e| e == "vtfrit") {
        // a frit's layers are in the layers' folder of the frits'
        // directory, wherever the frit being checked is
        let frits = paths
            .frits
            .clone()
            .unwrap_or_else(|_| path.parent().unwrap_or(Path::new("/")).to_owned());
        return match Frit::load_in(path, &frits.join(layer::DIR)) {
            Ok(frit) => {
                for warning in &frit.warnings {
                    let _ = writeln!(out, "warning\t{warning}");
                }
                for note in script_notes(&frit, paths) {
                    let _ = writeln!(out, "note\t{file}:{note}");
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
    let copied = copy.path.to_string_lossy().into_owned();
    console_check(paths, &copy.path, Some(&copied), file, out, err)
}

/// The notes the glyph helper gives on `frit`'s `script-source=` lines
/// (ADR-1202 D12), each from its line on: a font whose height gives no
/// superscript or subscript in the frit's own cell, and a font the helper
/// refuses. The helper reads the
/// fonts under its own sandbox, so this process parses none (ADR-1201
/// D3). A frit with no `script-source=` line, and a helper that does not
/// answer, give none.
fn script_notes(frit: &Frit, paths: &Paths) -> Vec<String> {
    let lines: Vec<usize> = frit
        .sources
        .iter()
        .flat_map(|s| s.script_sources.iter().map(|k| k.line))
        .collect();
    if lines.is_empty() {
        return Vec::new();
    }
    let text = crate::serve::FritText {
        name: frit.name.clone(),
        text: frit.text.clone(),
    };
    let Ok(warnings) = crate::specimen::warnings_of(&paths.helper, &text) else {
        return Vec::new();
    };
    let prefix = format!("{}:", frit.name);
    warnings
        .iter()
        .filter_map(|w| w.strip_prefix(&prefix))
        .filter(|rest| {
            rest.split_once(':')
                .and_then(|(n, _)| n.parse::<usize>().ok())
                .is_some_and(|n| lines.contains(&n))
        })
        .map(str::to_owned)
        .collect()
}

/// Runs the console's `vtium check-config` on the vtium.conf `candidate`,
/// against the configuration directory, and prints each problem it
/// reports, `copied`'s path named as `file`, then `ok FILE` when it found
/// none. It exits as the check does: 0 when it found no problem,
/// [`vtium_settings::CHECK_MISSING_FONTS`] when it found only missing
/// fonts, and 1 otherwise, so that a script reads the same status from
/// both tools.
fn console_check(
    paths: &Paths,
    candidate: &Path,
    copied: Option<&str>,
    file: &str,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let mut command = std::process::Command::new(&paths.vtium);
    command
        .arg("check-config")
        .arg("--configdir")
        .arg(&paths.configdir)
        .arg(candidate);
    let ran = match command.stdin(std::process::Stdio::null()).output() {
        Ok(ran) => ran,
        Err(e) => {
            return failed(err, &format!("{}: {e}", paths.vtium.to_string_lossy()));
        }
    };
    for line in String::from_utf8_lossy(&ran.stderr).lines() {
        let line = match copied {
            Some(copied) => line.replace(copied, file),
            None => line.to_owned(),
        };
        let _ = writeln!(out, "{line}");
    }
    match ran.status.code() {
        Some(0) => {
            let _ = writeln!(out, "ok\t{file}");
            0
        }
        Some(code) if code == i32::from(vtium_settings::CHECK_MISSING_FONTS) => {
            vtium_settings::CHECK_MISSING_FONTS
        }
        _ => 1,
    }
}

/// A key's default, as `get KEY` prints it.
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

/// The value `text`, vtium.conf, gives `key`, as `get KEY` prints it,
/// and whether the file sets it. A key binding the file sets only by its
/// old name has that line's value, as vtium reads it (ADR-1005 D10).
pub fn value_text(key: &keys::Key, text: Option<&str>) -> (String, bool) {
    let old = keys::RENAMED
        .iter()
        .find(|(_, k)| k.name == key.name)
        .map(|(old, _)| *old);
    let set = match key.kind {
        Kind::Switch => text
            .and_then(|t| switch(t, key.name))
            .map(|on| if on { "on" } else { "off" }.to_owned()),
        _ => text.and_then(|t| setting(t, key.name).or_else(|| setting(t, old?))),
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
            "{name} can be set only on vtium's command line, not in vtium.conf"
        )),
        Some(key) if key.scope == Scope::Section => Err(format!(
            "{name} can be set only in a [look NAME] section. Use vtium-config look set NAME {} VALUE.",
            crate::ops::look_option(name).unwrap_or(name)
        )),
        Some(key) => Ok(key),
        None => match keys::renamed(name) {
            Some((old, key)) => Err(format!(
                "{old} is now called {}. Use {} instead.",
                key.name, key.name
            )),
            None => Err(format!(
                "there is no setting named {name:?}. The Settings view of vtium-config tui, and docs/config.md, list them all."
            )),
        },
    }
}

/// The edit that sets `key` to `value`, or why vtium would not take it.
/// A switch takes `on` or `off`, a key of a fixed set of words one of
/// them, and a path key an absolute path; any other value goes to the
/// console's own check when the change is installed.
pub fn set_edit<'a>(key: &'static keys::Key, value: &'a str) -> Result<Edit<'a>, String> {
    match key.kind {
        Kind::Switch => match value {
            "on" => Ok(Edit::Switch(key.name, true)),
            "off" => Ok(Edit::Switch(key.name, false)),
            _ => Err(format!("{} is on or off", key.name)),
        },
        Kind::Choice(words) if !words.contains(&value) => {
            Err(format!("{} takes one of {}", key.name, words.join(", ")))
        }
        Kind::Name(words) if !keys::name(words, value) => {
            Err(format!("{}={value:?}: {}", key.name, keys::NOT_A_NAME))
        }
        Kind::Path if !keys::absolute(value) => {
            Err(format!("{}={value:?}: {}", key.name, keys::NOT_ABSOLUTE))
        }
        Kind::Paths if !value.split(':').all(keys::absolute) => Err(format!(
            "{}={value:?}: each folder must be {}",
            key.name,
            keys::NOT_ABSOLUTE.trim_start_matches("an ")
        )),
        _ => Ok(Edit::Set(key.name, value)),
    }
}

/// Why neither `remove-frit` nor the TUI's `r` keeps a removal of the frit
/// `name`, with `frits` the frits' directory; `None` when a removal may be
/// kept. The two refuse a built-in font's name and a name that no frit can
/// have. They also refuse a set of vtium's catalog, which only a category
/// folder of the frits' directory holds: an install removes a frit only
/// from the frits' directory itself, so that removal would remove nothing.
/// A frit that vtium.conf names is not refused: the removal is kept, and
/// the check lists each line that names the frit.
///
/// References:
/// - ADR-1403 D8, "Every font set change is kept at once": the changes vtium-config refuses, and a removal kept although vtium.conf names the frit.
pub fn removal_refusal(frits: &Path, name: &str) -> Option<String> {
    if RESERVED.contains(&name) {
        return Some(format!("{name} is a built-in font, not a font set"));
    }
    if !valid_name(name) {
        return Some(format!("there is no font set named {name:?}"));
    }
    if frits.join(format!("{name}.vtfrit")).is_file() {
        return None;
    }
    crate::switch::category_file(frits, name).map(|held| {
        format!(
            "{name} is a font set of vtium's catalog, {}, and vtium-config does not remove a font set of the catalog",
            held.display()
        )
    })
}

/// `remove-frit NAME`: keeps the removal of the installed frit
/// `NAME.vtfrit` in the current draft's font set folder, or takes back the
/// frit NAME that only the draft adds. It refuses a built-in font's name, a
/// name a frit cannot have, and a set of vtium's catalog, which lives in a
/// category folder that vtium-config never removes from. A removal is kept
/// even when the draft's vtium.conf names the frit: the check then lists
/// each line that names it, and the install refuses the draft until those
/// lines name another font.
///
/// References:
/// - ADR-1403 D8, "Every font set change is kept at once": a removal is written even when the draft's looks name the frit.
pub fn remove_frit(
    words: &[String],
    paths: &Paths,
    devices: &crate::flowcli::Devices,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let [name] = words else {
        return usage(err, "remove-frit takes one font set's name");
    };
    if name.starts_with("--") {
        return usage(err, &format!("remove-frit has no option {name}"));
    }
    let frits = match &paths.frits {
        Ok(frits) => frits,
        Err(why) => return failed(err, why),
    };
    if let Some(why) = removal_refusal(frits, name) {
        return failed(err, &why);
    }
    crate::flowcli::font_set(
        name,
        crate::flowcli::FontSetChange::Remove,
        paths,
        devices,
        out,
        err,
    )
}

/// A frit's file read whole, at most [`FRIT_BYTES_MAX`] bytes, only a
/// regular file.
fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let shown = path.display();
    let meta = std::fs::metadata(path).map_err(|e| format!("{shown}: {e}"))?;
    if !meta.is_file() {
        return Err(format!("{shown} is not a regular file"));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| {
            f.take(u64::try_from(FRIT_BYTES_MAX).unwrap_or(0).saturating_add(1))
                .read_to_end(&mut bytes)
        })
        .map_err(|e| format!("{shown}: {e}"))?;
    if bytes.len() > FRIT_BYTES_MAX {
        return Err(format!("{shown} is larger than {FRIT_BYTES_MAX} bytes"));
    }
    Ok(bytes)
}

/// What `new-frit --template` prints: a frit with each key it takes
/// explained, to edit and add to the draft with `new-frit --from FILE`.
pub const TEMPLATE: &str = "\
# A font set (frit): a file named NAME.vtfrit in /etc/vtium/fonts, from which
# the console's font helper draws the characters. Write one key=value per
# line; # starts a comment. vtium-config new-frit --from FILE checks this file
# and adds it to your draft, and vtium-config install installs it.
#
# The font set's name, as font-name and OSC 50 name it: letters, digits, '.',
# '_', '+' and '-'. When left out, the name is the file's name without
# .vtfrit.
#name=myfont
#
# The fonts, in order: each later source= line supplies only the characters
# the fonts before it lack. A source is font:FAMILY, a font family found on
# the machine that draws it; kbd:NAME, a kbd console font; file:PATH, a PSF
# file; bdf:PATH, a BDF font; hex:PATH, a font in GNU Unifont's .hex
# format, uncompressed; outline:PATH, a TrueType or OpenType file; vconsole,
# the console font vconsole.conf names; console:VT, the font the kernel's
# console had when vtium started; or builtin:unifont or builtin:unscii-8.
source=font:DejaVu Sans Mono
#source=builtin:unifont
#
# A layer: the sources of /etc/vtium/fonts/layers/NAME.vtlayer, put in place
# of this line. A layer brings a script, symbols, emoji or the fallback, and
# a font set holds one symbols, one emoji and one fallback layer at most.
#layer=noto-emoji
#
# A bitmap file of the same design as the outline font on the source= line
# above it. A cell up to the tallest such file's height draws the tallest
# one that fits it, and the outline draws larger cells. Write it as kbd:NAME,
# file:PATH or bdf:PATH, and repeat the line for each file.
#strike=bdf:/usr/share/fonts/terminus/ter-u16n.bdf
#
# A smaller size of the bitmap font on the source= line above it, which
# draws that font's superscripts and subscripts. Its characters must be
# from half to three quarters as high as the font's own. Write it as kbd:NAME,
# file:PATH or bdf:PATH, and repeat the line for each size.
#script-source=kbd:ter-112n
#
# The character cell in pixels, WIDTHxHEIGHT, each side from 6 to 96. When
# left out, the first font sets it.
#cell=8x16
#
# The starting size in points of a font set with outline fonts, from 6 to 96.
#size=12
#
# How outline fonts are hinted: light, the default, or full.
#hinting=light
#
# Extra space between lines of text: a whole percentage of the text's
# height, from 0, the default, to 100. The space is split evenly above and
# below each line. A cell= line takes none, unless font-size overrides it.
#line-spacing=0
#
# How the characters of the source= or cover= line above it use that extra
# space: fill stretches them to the whole cell, natural keeps them at the
# font's size, and room keeps the letters at the font's size and lets their
# marks use the space. When left out, line and block characters fill;
# Arabic, Hebrew, Syriac, Thaana, N'Ko and combining marks take room; and
# the rest is natural. Color characters and emoji always keep their size.
#fit=natural
";

/// A source as `new` takes it: a source's spelling when it starts with a
/// form a frit's `source=` takes, and otherwise a font family, `font:`.
pub fn source_spelling(word: &str) -> String {
    const FORMS: [&str; 8] = [
        "font:", "kbd:", "file:", "bdf:", "hex:", "outline:", "console:", "builtin:",
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
    /// `--layer`'s names, in the order the flags came in.
    layers: Vec<String>,
    name: Option<String>,
    cell: Option<String>,
    size: Option<String>,
    from: Option<String>,
    template: bool,
}

fn new_args(words: &[String]) -> Result<NewArgs, String> {
    let mut a = NewArgs {
        fonts: Vec::new(),
        layers: Vec::new(),
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
                .ok_or_else(|| format!("new-frit's {flag} needs a value"))
        };
        match word.as_str() {
            "--template" => a.template = true,
            "--from" => a.from = Some(value("--from")?),
            "--name" => a.name = Some(value("--name")?),
            "--cell" => a.cell = Some(value("--cell")?),
            "--size" => a.size = Some(value("--size")?),
            "--layer" => a.layers.push(value("--layer")?),
            w if w.starts_with("--") => return Err(format!("new-frit has no option {w}")),
            w => a.fonts.push(w.to_owned()),
        }
    }
    Ok(a)
}

/// A new frit's text from `new`'s fonts and options, with no layer.
pub fn frit_from(name: &str, fonts: &[String], cell: Option<&str>, size: Option<&str>) -> String {
    layered_frit(name, fonts, &[], cell, size)
}

/// A new frit's text from `new`'s fonts, its `layers` in the order they
/// go in the set ([`layer_order`]), and its options.
pub fn layered_frit(
    name: &str,
    fonts: &[String],
    layers: &[String],
    cell: Option<&str>,
    size: Option<&str>,
) -> String {
    let mut text = format!("# {name}: made by vtium-config new-frit\nname={name}\n");
    for font in fonts {
        text.push_str(&format!("source={}\n", source_spelling(font)));
    }
    for layer in layers {
        text.push_str(&format!("layer={layer}\n"));
    }
    if let Some(cell) = cell {
        text.push_str(&format!("cell={cell}\n"));
    }
    if let Some(size) = size {
        text.push_str(&format!("size={size}\n"));
    }
    text
}

/// The layers `names` in the order a set holds them: script layers, then
/// the symbols layer, then the emoji layer, then the fallback layer, each
/// role's layers in the order they were named. Each layer is read from
/// the `layers/` folder of `frits`, strictly, for its role. A name with no
/// layer file, a layer the reader refuses, a name given twice and a
/// second layer of a role a set holds once are refused.
///
/// References:
/// - ADR-1204 D5: `new-frit --layer`.
/// - ADR-1204 D6: the roles' order, and one symbols, emoji and fallback
///   layer in a set at most.
pub fn layer_order(frits: &Path, names: &[String]) -> Result<Vec<String>, String> {
    let dir = frits.join(layer::DIR);
    let mut roles: Vec<(layer::Role, &str)> = Vec::new();
    for name in names {
        if !layer::valid_name(name) {
            return Err(format!(
                "{name:?} cannot be a layer's name. {}",
                layer::NAME_RULE
            ));
        }
        if roles.iter().any(|&(_, n)| n == name) {
            return Err(format!("--layer {name} is named twice"));
        }
        let file = dir.join(format!("{name}{}", layer::SUFFIX));
        if !file.exists() {
            return Err(format!(
                "there is no layer {name}: {} does not exist",
                file.display()
            ));
        }
        let bytes = read_bounded(&file)?;
        // the layer's warnings are left to frit_checked, which reads the
        // layer again in place in the new font set and warns once
        let (role, _) = Frit::parse_layer(&bytes, &file, name).map_err(|e| e.to_string())?;
        if let Some(&(_, other)) = roles.iter().find(|&&(r, _)| r == role && role.single()) {
            return Err(format!(
                "{other} and {name} are both {role} layers, and a font set holds one {role} layer"
            ));
        }
        roles.push((role, name));
    }
    // a stable sort keeps each role's layers in the order they were named
    roles.sort_by_key(|&(role, _)| role);
    Ok(roles.into_iter().map(|(_, n)| n.to_owned()).collect())
}

/// Checks the frit `text`, to be installed at `path` in the frits'
/// directory, with the layers of that directory in place, as the console
/// will hand it to the helper. A layer that this machine lacks or cannot
/// read passes with a warning, since the console draws the set without it
/// until it is there. Any other `layer=` line that would add nothing is
/// refused, such as a layer that would take the set past its 16 sources,
/// and so is a value in a layer that the helper would refuse. The answer
/// is a warning for each line vtium ignores, in the frit's own lines and
/// in its layers', and then one for each layer it lacks or cannot read,
/// as `vtium-config check` gives them.
///
/// References:
/// - ADR-1204 D5: the splice the console makes, and `new-frit --layer`.
/// - ADR-1510 D2: an unknown or misplaced key is reported with its file
///   and line, and ignored alone.
fn check_layers(text: &str, path: &Path, name: &str) -> Result<Vec<FritError>, String> {
    let layers = path.parent().unwrap_or(Path::new("/")).join(layer::DIR);
    let expanded = layer::expand(text, &mut |n| crate::frit::read_layer(&layers, n));
    if let Some(note) = expanded.notes.iter().find(|note| {
        !matches!(
            note.problem,
            layer::Problem::Missing | layer::Problem::Unread(_)
        )
    }) {
        let file = crate::frit::layer_file(&layers, &note.layer);
        return Err(format!(
            "{}:{}: {}",
            path.display(),
            note.line,
            crate::frit::layer_why(&file, note)
        ));
    }
    let frit = Frit::parse_spliced(expanded.text.as_bytes(), path, name, &layers)
        .map_err(|e| e.to_string())?;
    let ignored = expanded.ignored.iter().map(|ignored| {
        let file = crate::frit::layer_file(&layers, &ignored.layer);
        crate::frit::ignored_warning(&file, &ignored.ignored)
    });
    // every note left is a layer this machine lacks or cannot read
    let lacking = expanded.notes.iter().map(|note| FritError {
        file: path.to_owned(),
        line: note.line,
        why: crate::frit::layer_why(&crate::frit::layer_file(&layers, &note.layer), note),
    });
    Ok(frit
        .warnings
        .into_iter()
        .chain(ignored)
        .chain(lacking)
        .collect())
}

/// Checks the frit `text` that `new-frit` keeps as `name` in the current
/// draft: the name is one a frit can have, no category folder of the
/// catalog holds a set of that name, the frit reader takes the text
/// strictly, as the glyph helper reads it, its layers are there, and its
/// `name=` is `name`. The answer is a warning for each line vtium
/// ignores, in the frit's own lines and in its layers'.
///
/// References:
/// - ADR-1510 D2: an unknown or misplaced key is reported with its file
///   and line, and ignored alone.
pub fn frit_checked(paths: &Paths, name: &str, text: &str) -> Result<Vec<FritError>, String> {
    if !valid_name(name) || RESERVED.contains(&name) {
        return Err(format!(
            "{name:?} cannot be a font set's name: a name has 1 to 64 letters, digits, '.', '_', '+' or '-', and cannot be a built-in font's name"
        ));
    }
    let frits = paths.frits.as_ref().map_err(Clone::clone)?;
    crate::switch::unheld(frits, name)?;
    let path = frits.join(format!("{name}.vtfrit"));
    let frit = Frit::parse(text.as_bytes(), &path, name).map_err(|e| e.to_string())?;
    // the spliced text holds the frit's own lines too, so its warnings
    // take the place of the frit's own
    let warnings = if frit.layers.is_empty() {
        frit.warnings
    } else {
        check_layers(text, &path, name)?
    };
    if frit.name != name {
        return Err(format!(
            "the font set says name={}, but it would be installed as {name}. Make the two names the same.",
            frit.name
        ));
    }
    Ok(warnings)
}

/// `new-frit`'s non-interactive forms: `new-frit --template` prints a
/// frit to edit; `new-frit --from FILE [--name NAME]` keeps the frit in
/// FILE in the current draft; `new-frit FONT... [--name NAME] [--cell WxH]
/// [--size POINTS] [--layer NAME]...` makes a frit of the fonts, in
/// order, each a family or a source as a frit spells it, and the layers
/// in the order of their roles ([`layer_order`]), and keeps it in the
/// current draft. `install` installs the draft.
///
/// References:
/// - ADR-1200 D4 and ADR-1201 D9: the three forms.
/// - ADR-1403 D8, "Every font set change is kept at once": the new frit waits in the draft's font set folder.
pub fn new(
    words: &[String],
    paths: &Paths,
    devices: &crate::flowcli::Devices,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let a = match new_args(words) {
        Ok(a) => a,
        Err(why) => return usage(err, &why),
    };
    if a.template {
        if !a.fonts.is_empty() || a.from.is_some() || a.name.is_some() || !a.layers.is_empty() {
            return usage(err, "new-frit --template takes no other argument");
        }
        let _ = write!(out, "{TEMPLATE}");
        return 0;
    }
    let (name, text) = match (&a.from, a.fonts.is_empty()) {
        (Some(file), true) if a.cell.is_none() && a.size.is_none() && a.layers.is_empty() => {
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
            let layers = match &paths.frits {
                _ if a.layers.is_empty() => Ok(Vec::new()),
                Ok(frits) => layer_order(frits, &a.layers),
                Err(why) => Err(why.clone()),
            };
            let layers = match layers {
                Ok(layers) => layers,
                Err(why) => return failed(err, &why),
            };
            let text = layered_frit(
                &name,
                &a.fonts,
                &layers,
                a.cell.as_deref(),
                a.size.as_deref(),
            );
            (name, text)
        }
        _ => {
            return usage(
                err,
                "new-frit takes FONT... with --name, --cell, --size and --layer, or --from FILE with --name, or --template",
            );
        }
    };
    match frit_checked(paths, &name, &text) {
        Ok(warnings) => {
            for warning in &warnings {
                let _ = writeln!(err, "warning\t{warning}");
            }
        }
        Err(why) => return failed(err, &why),
    }
    crate::flowcli::font_set(
        &name,
        crate::flowcli::FontSetChange::Keep(text),
        paths,
        devices,
        out,
        err,
    )
}

/// `fonts --files`: each font file vtium-config finds on this machine, one
/// line each, tab-separated, in the columns of [`crate::discover::row`].
/// The font helper describes each file under its own sandbox, so this
/// process parses no font, and each line is printed as soon as its file
/// is described. Its state comes from the current draft, as `active`
/// reports it.
///
/// References:
/// - ADR-1205 D3: the folders, and a user's font listed as not usable.
/// - ADR-1205 D4: each font's trust mark and its state in the batch.
fn font_files(paths: &Paths, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    use crate::discover::{Folders, describe_each, row, scan};
    let text = match conf(paths) {
        Ok((_, text)) => text,
        Err(why) => return failed(err, &why),
    };
    let store = match crate::store::Store::system() {
        Ok(store) => store,
        Err(fail) => return failed(err, fail.text()),
    };
    let session = crate::drafts::Session { store, paths };
    let resources = match crate::batch::Resources::open(&session) {
        Ok(r) => r,
        Err(fail) => return failed(err, fail.text()),
    };
    let mounts = match vtium_settings::mounts::Mounts::system() {
        Ok(m) => m,
        Err(e) => {
            return failed(
                err,
                &format!(
                    "vtium-config cannot read the mount table, so it cannot judge the fonts it finds: {e}"
                ),
            );
        }
    };
    let euid = crate::batch::rule_user(&paths.targets(), &paths.sudo);
    let rule = vtium_settings::cullet::Rule::running_as(euid);
    let found = scan(
        &Folders::of_user(text.as_deref()),
        &rule,
        &mounts,
        &|p: &Path| resources.state(p),
    );
    let files: Vec<PathBuf> = found.iter().map(|f| f.path.clone()).collect();
    let mut rows = found.iter();
    describe_each(&paths.helper, &files, &mut |d| {
        if let Some(f) = rows.next() {
            let _ = writeln!(out, "{}", row(f, &d));
            let _ = out.flush();
        }
    });
    0
}

/// `fonts --install-system FILE [--replace]`: installs the font file FILE
/// as a system font, a copy in `/usr/local/share/fonts` that root owns.
///
/// vtium-config reads FILE once and writes those bytes into a draft that
/// holds this one font. The font helper then reads the draft's copy in
/// its sandbox, so the file it judges is the file that is installed, and
/// a copy it cannot describe as a font is refused and its draft removed.
/// A font that a package installed under `/usr/share/fonts` is refused,
/// because it is a system font already. When the folder already holds a
/// file of the same name with the same bytes, the font is already
/// installed and nothing changes. When that file holds other bytes, the
/// command shows both files and replaces the installed one only after a
/// yes at a terminal, or with `--replace`. The draft is installed through
/// `sudo vtium-config apply DIR`. At a terminal the command shows that
/// command and asks before it runs it, each time. Away from a terminal it
/// prints the draft and the command, as the frit commands do, and runs
/// nothing. Every path and reason it prints passes through
/// [`crate::onscreen::text`].
///
/// References:
/// - ADR-1205 D5: the install step, its clash and its identical file.
fn install_system(words: &[String], paths: &Paths, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    use crate::apply::{Placement, SystemFont};
    use crate::discover::{Described, FileFacts, describe};
    use crate::onscreen::{path as shown_path, text};
    let (file, replace) = match words {
        [file] => (file, false),
        [file, flag] | [flag, file] if flag == "--replace" => (file, true),
        _ => {
            return usage(
                err,
                "fonts --install-system takes one FILE and may take --replace",
            );
        }
    };
    let source = match std::path::absolute(file) {
        Ok(p) => p,
        Err(e) => return failed(err, &text(&format!("{file}: {e}"))),
    };
    let targets = paths.targets();
    let font = match SystemFont::read(&source, &targets) {
        Ok(font) => font,
        Err(why) => return failed(err, &text(&why)),
    };
    let placement = font.placement();
    let draft = match font.write_draft(&paths.drafts) {
        Ok(draft) => draft,
        Err(e) => {
            return failed(
                err,
                &text(&format!("the draft under {}: {e}", paths.drafts.display())),
            );
        }
    };
    // the draft is removed whenever the command installs nothing from it
    let discard = || {
        let _ = std::fs::remove_dir_all(&draft);
    };
    let mut asked = vec![font.draft_copy(&draft)];
    if placement == Placement::Replaces {
        asked.push(font.target.clone());
    }
    let described = describe(&paths.helper, &asked);
    let unknown = Described::Unknown("the font helper did not answer".to_owned());
    let yours = described.first().unwrap_or(&unknown);
    if let Described::NotAFont(why) | Described::Unknown(why) = yours {
        discard();
        return failed(
            err,
            &format!(
                "{}: vtium-config cannot install this file as a system font, because the font helper cannot read it as a font: {}",
                shown_path(&font.source),
                text(why)
            ),
        );
    }
    let terminal = std::io::IsTerminal::is_terminal(&std::io::stdin());
    match placement {
        Placement::Identical => {
            discard();
            let _ = writeln!(
                out,
                "This font is already installed as {}, and the two files are the same. Nothing changed.",
                shown_path(&font.target)
            );
            return 0;
        }
        Placement::Replaces => {
            let installed = FileFacts::of(&font.target, described.get(1).unwrap_or(&unknown));
            let yours = FileFacts::of(&font.source, yours);
            let _ = writeln!(out, "A different file of this name is already installed.");
            let _ = writeln!(out, "Installed: {}", installed.line());
            let _ = writeln!(out, "Yours: {}", yours.line());
            let yes = replace
                || (terminal
                    && crate::vts::ask_line(out, "Replace the installed file with yours?"));
            if !yes {
                discard();
                let _ = writeln!(
                    out,
                    "Nothing changed. To replace the installed file, answer y at a terminal, or run the command again with --replace."
                );
                return 1;
            }
        }
        Placement::New => {}
    }
    let program = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("vtium-config"));
    let route = crate::apply::route(&targets, &paths.sudo);
    let argv = crate::apply::save_argv(&route, &program, &targets, &draft);
    let shown = text(&crate::vts::shown(&argv));
    if !terminal {
        let _ = writeln!(out, "write\t{}", shown_path(&font.target));
        let _ = writeln!(out, "draft\t{}", shown_path(&draft));
        let _ = writeln!(out, "run\t{shown}");
        return 0;
    }
    let _ = writeln!(
        out,
        "To install {} as {}, vtium-config runs this command:\n  {shown}",
        shown_path(&font.source),
        shown_path(&font.target)
    );
    if !crate::vts::ask_line(out, "Run this command?") {
        let _ = writeln!(
            out,
            "Nothing changed. The draft stays in {}.",
            shown_path(&draft)
        );
        return 1;
    }
    match crate::drafts::run_child(&argv) {
        Ok((ok, said)) => {
            // what apply printed, a line at a time, each tab-separated
            // column shown as text
            for line in said.lines() {
                let columns: Vec<String> = line.split('\t').map(text).collect();
                let _ = writeln!(out, "{}", columns.join("\t"));
            }
            u8::from(!ok)
        }
        Err(e) => failed(err, &text(&format!("{shown}: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `vtium config set palette` takes a built-in palette's name or a
    // file's name, and refuses a value that breaks the name rule.
    #[test]
    fn a_name_key_takes_a_built_in_or_a_file_s_name() {
        for value in ["nord", "catppuccin-mocha"] {
            assert_eq!(
                set_edit(&keys::PALETTE, value),
                Ok(Edit::Set("palette", value))
            );
        }
        assert!(set_edit(&keys::PALETTE, "../nord").is_err());
    }

    // The report names a strike whose file has another SHA-256 by the
    // strike's own file, with the page of the source the strike belongs
    // to, as it names a source's file.
    //
    // References:
    // - ADR-1204 D2: a bitmap source's strikes, each its own file.
    // - ADR-1204 D7: a file with another hash still draws, and is reported.
    #[test]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn the_report_names_a_strike_whose_hash_differs_by_its_file() {
        let frit = Frit::parse(
            format!(
                "source=bdf:/f/ter-u16n.bdf.gz\nurl=https://example.org/terminus/\nstrike=bdf:/f/ter-u12n.bdf.gz\nsha256={}\n",
                "0".repeat(64)
            )
            .as_bytes(),
            Path::new("/r/terminus.vtfrit"),
            "terminus",
        )
        .expect("the frit");
        let differs = [Differs {
            line: 3,
            spelling: "bdf:/f/ter-u12n.bdf.gz".to_owned(),
            what: Differ::OtherHash("1".repeat(64)),
        }];
        assert_eq!(
            needs_report("terminus", &frit, &[], &differs),
            [
                fonts_report::other_file("terminus", "/f/ter-u12n.bdf.gz"),
                fonts_report::page("https://example.org/terminus/"),
            ]
        );
    }

    // The report names a strike this machine lacks by the strike's own
    // file, or by its kbd font's name, with the page of the source the
    // strike is a size of, as it names a missing source's font.
    //
    // References:
    // - ADR-1204 D2: a bitmap source's strikes, each its own file.
    // - ADR-1204 D7: vtium-config names each missing font with its page.
    #[test]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn the_report_names_a_missing_strike_by_its_file() {
        let frit = Frit::parse(
            b"source=bdf:/f/ter-u16n.bdf.gz\nversion=4.49\nurl=https://example.org/terminus/\nstrike=bdf:/f/ter-u20n.bdf.gz\nstrike=kbd:ter-v22n\n",
            Path::new("/r/terminus.vtfrit"),
            "terminus",
        )
        .expect("the frit");
        let missing = |line: usize, spelling: &str| crate::serve::LeftOut {
            missing: true,
            line,
            spelling: spelling.to_owned(),
            why: "not on this machine".to_owned(),
        };
        assert_eq!(
            needs_report(
                "terminus",
                &frit,
                &[
                    missing(4, "bdf:/f/ter-u20n.bdf.gz"),
                    missing(5, "kbd:ter-v22n")
                ],
                &[]
            ),
            [
                fonts_report::missing("terminus", "/f/ter-u20n.bdf.gz", Named::ByPath),
                fonts_report::page("https://example.org/terminus/"),
                fonts_report::missing("terminus", "ter-v22n", Named::ByName),
                fonts_report::page("https://example.org/terminus/"),
            ]
        );
    }

    // vtium-config installs no set of a name that a category folder of the
    // catalog already holds, and installs any other name.
    //
    // References:
    // - ADR-1204 D9: a name stays unique across every folder.
    #[test]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn a_set_named_as_a_catalog_set_is_not_installed() {
        let dir = std::env::temp_dir().join(format!("vtium-config-held-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("console")).expect("the folder");
        std::fs::write(dir.join("console/terminus.vtfrit"), "source=kbd:x\n").expect("the set");
        let why = crate::switch::unheld(&dir, "terminus").expect_err("held");
        assert!(
            why.starts_with("The font set terminus is already installed as "),
            "{why}"
        );
        assert!(
            why.contains(&dir.join("console/terminus.vtfrit").display().to_string()),
            "{why}"
        );
        assert_eq!(crate::switch::unheld(&dir, "mine"), Ok(()));
        // a link at the name holds it, wherever the link points
        std::os::unix::fs::symlink(dir.join("missing"), dir.join("console/dangling.vtfrit"))
            .expect("the link");
        let why = crate::switch::unheld(&dir, "dangling").expect_err("held");
        assert!(
            why.starts_with("The font set dangling is already installed as "),
            "{why}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ADR-1005 D10: a key binding set by its old name shows the value vtium
    // reads from that line, the new name wins where both are set, and the
    // old name is no setting of its own.
    #[test]
    fn a_key_binding_set_by_its_old_name_shows_its_value() {
        let paste = keys::find("key-paste").unwrap_or(&keys::KEY_PASTE);
        assert_eq!(
            value_text(paste, Some("grab-paste=<Alt>p\n")),
            ("<Alt>p".to_owned(), true)
        );
        assert_eq!(
            value_text(paste, Some("grab-paste=<Alt>p\nkey-paste=<Alt>n\n")),
            ("<Alt>n".to_owned(), true)
        );
        assert_eq!(value_text(paste, None), ("<Shift>Insert".to_owned(), false));
        assert_eq!(
            conf_key("grab-paste").err().as_deref(),
            Some("grab-paste is now called key-paste. Use key-paste instead.")
        );
    }

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
