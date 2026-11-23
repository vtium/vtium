//! What check-config says of the resources a start would read through the
//! console's gate: the vtlooks and their `.cube` files, the pointer files,
//! the batch file that lists the active resources, the palette files, and
//! the font files each font set names.
//!
//! References:
//! - ADR-1205 D4: one gate for every kind of resource, and the batch that lists the active ones.

use std::path::{Path, PathBuf};

use vtium_settings::{Trust, at};

use super::Settings;
use crate::font;
use crate::helper;
use crate::notice::check_config;

/// The heads that a vtlook named at the top level are for.
pub(super) const TOP_HEADS: &str = "the heads whose look names no vtlook of its own";

/// What check-config says of the vtlooks a start would read: `top`, the
/// one vtium.conf's top level or the command line names, with where it is
/// named, and those its looks name in `looks`, each with its look's name
/// and its line in the vtium.conf at `conf`. Each line counts against the
/// file, since the heads that take a vtlook with an error draw the
/// built-in vtlook.
///
/// `top` is always a vtlook's name: vtium refuses a top-level `vtlook`
/// that is not one, such as a path, when it reads vtium.conf or the
/// command line, and check-config reports that refusal at its line with
/// vtium.conf's other errors. A look's name passes the same rule when
/// vtium.conf's sections are read.
///
/// A build with frame filters reads each distinct vtlook once from the
/// first of `folders` that holds it, under `trust`, as the start reads it:
/// the administrator's folder and then the package's, so a preset the
/// package ships is found with no copy in the administrator's folder. It
/// then builds the vtlook's color table, reading each `cube` filter's
/// `.cube` file from the first of `folders` through `gate`'s batch, trust
/// rule and mount table, as the start does. For each vtlook that fails,
/// it lists the vtlook's errors, each starting with the file and line at
/// fault, the vtlook's or a `.cube` file's, and then each place in
/// vtium.conf that names the vtlook. It also reports a file named
/// `default.vtlook` in any of `folders`, which vtium never reads.
///
/// References:
/// - ADR-1304 D4: vtium refuses a file named default.vtlook, and its note of 2026-11-19 gives the two folders and the administrator's file winning on a name.
/// - ADR-1304 D5: a vtlook's name, and the top level's `vtlook`.
/// - ADR-1304 D6: check-config lists every error of the reading and checking stages, each report whole.
/// - ADR-1304 D9: a `.cube` file sits beside the administrator's vtlooks.
/// - ADR-1205 D4: a `.cube` file counts as missing unless the batch lists it and it passes the trust rule.
#[cfg(feature = "filters")]
pub(super) fn vtlook_findings(
    top: Option<(&str, String)>,
    conf: &Path,
    looks: &[(String, String, usize)],
    folders: &[&Path],
    trust: &Trust,
    gate: &vtium_settings::cullet::Gate,
) -> Vec<String> {
    use vtium_settings::vtlook::{self, DEFAULT, SUFFIX};
    // a `.cube` file sits in the administrator's folder, the first one
    let cubes = vtlook::Cubes {
        dir: folders.first().copied().unwrap_or(Path::new(vtlook::LOOKS)),
        gate,
    };
    let mut names: Vec<&str> = top.iter().map(|(name, _)| *name).collect();
    for (_, name, _) in looks {
        if !names.contains(&name.as_str()) {
            names.push(name);
        }
    }
    let mut out = Vec::new();
    for name in names {
        let failure = match vtlook::find(name, folders, trust) {
            Err(failure) => failure,
            Ok(look) => match vtlook::table_with(&look, &cubes) {
                Ok(_) => continue,
                Err(failure) => failure,
            },
        };
        out.extend(failure.reports.iter().map(|r| format!("{}: {r}", r.at())));
        if failure.more > 0 {
            let file = failure.reports.first().map(|r| r.file.clone());
            out.push(format!(
                "{}: {}",
                file.unwrap_or_default().display(),
                crate::notice::more_errors(failure.more)
            ));
        }
        let fails = |heads: &str| {
            format!("The vtlook {name} has errors, so {heads} draw the built-in vtlook instead.")
        };
        out.extend(
            top.iter()
                .filter(|(top, _)| *top == name)
                .map(|(_, at)| format!("{at}: {}", fails(TOP_HEADS))),
        );
        out.extend(
            looks
                .iter()
                .filter(|(_, named, _)| named == name)
                .map(|(look, _, line)| {
                    let heads = format!("the heads that use look {look}");
                    at(conf, Some(*line), fails(&heads)).to_string()
                }),
        );
    }
    // a file that takes the built-in vtlook's name is never read, so only
    // the check can say so
    for dir in folders {
        if dir
            .join(format!("{DEFAULT}{SUFFIX}"))
            .symlink_metadata()
            .is_err()
        {
            continue;
        }
        let refused = vtlook::read(DEFAULT, dir, trust).err();
        out.extend(
            refused
                .iter()
                .flat_map(|f| &f.reports)
                .map(|r| format!("{}: {r}", r.at())),
        );
    }
    out
}

/// [`vtlook_findings`] in a build without frame filters, which reads no
/// vtlook: for each place that names one, it says that the heads that
/// place chooses for draw the built-in vtlook.
///
/// References:
/// - ADR-1304 D10: check-config warns of a vtlook in a build without `filters`, as it warns of `hwaccel` in a build without `gpu`.
#[cfg(not(feature = "filters"))]
pub(super) fn vtlook_findings(
    top: Option<(&str, String)>,
    conf: &Path,
    looks: &[(String, String, usize)],
    _folders: &[&Path],
    _trust: &Trust,
    _gate: &vtium_settings::cullet::Gate,
) -> Vec<String> {
    let none = |heads: &str, name: &str| {
        format!(
            "This build of vtium has no frame filters, so {heads} draw the built-in vtlook instead of vtlook {name}."
        )
    };
    let top = top.map(|(name, at)| format!("{at}: {}", none(TOP_HEADS, name)));
    let looks = looks.iter().map(|(look, name, line)| {
        let heads = format!("the heads that use look {look}");
        at(conf, Some(*line), none(&heads, name)).to_string()
    });
    top.into_iter().chain(looks).collect()
}

/// What check-config says of the pointer files that `pointer-theme` names:
/// at the top level, `top`, with where it is named, and in each of
/// `looks`, a look's name, its pointer file and the line of vtium.conf
/// `conf` that names it. Each file is read through `gate` as the console
/// reads it, and each line counts, since vtium draws its own pictures in
/// place of what it cannot read. A line two places share, such as a theme
/// that the top level and a look both name, is said once.
///
/// References:
/// - ADR-1205 D6: each look picks its pointer theme, which passes the same trust rule as the top level's.
pub(super) fn pointer_findings(
    top: Option<(&str, String)>,
    conf: &Path,
    looks: &[(String, String, usize)],
    gate: &vtium_settings::cullet::Gate,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |said: Vec<String>| {
        for line in said {
            if !out.contains(&line) {
                out.push(line);
            }
        }
    };
    if let Some((value, named)) = top {
        add(pointer_file_findings(
            value,
            &|said| format!("{named}: {said}"),
            gate,
        ));
    }
    for (_, value, line) in looks {
        add(pointer_file_findings(
            value,
            &|said| at(conf, Some(*line), said).to_string(),
            gate,
        ));
    }
    out
}

/// What check-config says of the pointer file at `value`, read through
/// `gate` as the console reads it. A path that names neither an SVG file
/// nor a cursor theme's folder is reported where it is named, the line
/// that `named` makes of the sentence. An SVG file that fails
/// the gate names the check it failed. A cursor theme's folder that fails
/// the gate is one line, since every file in it fails the same way. In a
/// theme that passes, check-config walks each shape's file names in the
/// console's order and stops at the first file that passes the gate. Each
/// file before it that fails is named once, with the check it failed, such
/// as a symbolic link that leads out of the theme's folder. check-config
/// does not parse the files. The console stops at the first file it can
/// draw, so a file that passes the gate but does not parse leads the
/// console on to the shape's next name, which check-config does not check.
///
/// References:
/// - ADR-1205 D4: a pointer theme is a resource activated by its folder, its links pass while they stay inside the theme's folder, and check-config names the file and the check it failed.
/// - ADR-1309 D2, D3: what the path names, and which files of a cursor theme serve each shape.
fn pointer_file_findings(
    value: &str,
    named: &dyn Fn(String) -> String,
    gate: &vtium_settings::cullet::Gate,
) -> Vec<String> {
    use crate::pointer_file::{Located, locate, theme_names, theme_refused_line};
    use vtium_settings::cullet::Check;
    let path = Path::new(value);
    match locate(path) {
        Located::Unusable(_) => vec![named(crate::pointer_file::unusable_line(path))],
        Located::Svg(_) => match gate.open(path) {
            Err(r) if r.check != Check::Missing => vec![format!(
                "{}: vtium treats this pointer file as missing, because {} So vtium draws its own pointer pictures.",
                path.display(),
                r.check
            )],
            _ => Vec::new(),
        },
        Located::Theme(_) => {
            if let Err(r) = gate.theme(path) {
                return vec![theme_refused_line(path, &r.check)];
            }
            let mut out: Vec<String> = Vec::new();
            let mut seen: Vec<PathBuf> = Vec::new();
            for shape in vtium_term::PointerShape::ALL {
                for name in theme_names(shape) {
                    let file = path.join("cursors").join(name);
                    match gate.open_in_theme(path, &file) {
                        Ok(_) => break,
                        Err(r) if r.check == Check::Missing => {}
                        Err(r) => {
                            if !seen.contains(&file) {
                                out.push(format!(
                                    "{}: vtium treats this file of the pointer theme {} as missing, because {} vtium tries the shape's next file, or draws its own picture of the shape.",
                                    file.display(),
                                    path.display(),
                                    r.check
                                ));
                                seen.push(file);
                            }
                        }
                    }
                }
            }
            out
        }
    }
}

/// What check-config says of the batch file, the list of active fonts,
/// color schemes, effects and pointer themes: the first list counts
/// against the file, and the second is notes. A missing file is a note
/// that says what vtium uses in its place and how to create the file. A
/// file vtium cannot use, because it fails the owner and mode check, is
/// too large or cannot be read, counts, and so does each line vtium
/// ignores. Each finding reads as the console's log writes it. When vtium
/// ignores more lines than it lists, the line that counts the rest comes
/// last.
///
/// References:
/// - ADR-1205 D4, its notes of 2026-11-18 and 2026-11-19: check-config says the batch file is missing and how to create it, and names a file vtium ignores and why.
pub(super) fn batch_findings(read: &vtium_settings::batch::Read) -> (Vec<String>, Vec<String>) {
    use vtium_settings::batch::Finding;
    let (mut counted, mut notes) = (Vec::new(), Vec::new());
    for finding in &read.findings {
        match finding {
            Finding::Missing { .. } => notes.push(finding.to_string()),
            Finding::Line { .. } | Finding::Refused { .. } => counted.push(finding.to_string()),
        }
    }
    counted.extend(read.more_line());
    (counted, notes)
}

/// The heads that a color scheme named at the top level are for.
pub(super) const TOP_SCHEME_HEADS: &str = "the heads whose look names no color scheme of its own";

/// What the heads draw in place of a color scheme the top level names
/// that fails, and of one a look names.
pub(super) const TOP_INSTEAD: &str = "draw the color scheme legacy instead";
pub(super) const LOOK_INSTEAD: &str = "draw the color scheme that vtium.conf's top level names instead, or legacy when that one fails too";

/// What check-config says of the palette files, the color schemes, that a
/// start would read: `top`, the one vtium.conf's top level or the command
/// line names, with where it is named, and those its looks name in
/// `looks`, each with its look's name and its line in the vtium.conf at
/// `conf`. Only a name that is not a built-in palette names a file. The
/// first list counts against the file, and the second is notes.
///
/// It reads each named file once from `folders` through `gate`, as the
/// start reads it: a file the trust rule refuses, or one the batch does not
/// list, names the file and why, as a file with errors does. A name no folder holds is a warning, since the heads
/// that name it draw the palette below it. A file with errors lists each
/// error with its file and line, then each place in vtium.conf that names
/// the file. A file that takes the name of a built-in palette, which vtium
/// never reads, is listed too. Each key a file sets that vtium ignores is
/// a note, once for each file.
///
/// References:
/// - ADR-1205 D10: what check-config says of a palette file: its errors, the keys vtium ignores, a file named like a built-in palette, and a name that matches no palette.
pub(super) fn palette_findings(
    top: Option<(&str, String)>,
    conf: &Path,
    looks: &[(String, String, usize)],
    folders: &[&Path],
    gate: &vtium_settings::cullet::Gate,
) -> (Vec<String>, Vec<String>) {
    use vtium_settings::palette;
    let mut names: Vec<&str> = top.iter().map(|(name, _)| *name).collect();
    for (_, name, _) in looks {
        if !names.contains(&name.as_str()) {
            names.push(name);
        }
    }
    let (mut counted, mut notes) = (Vec::new(), Vec::new());
    for name in names.into_iter().filter(|n| !palette::built_in(n)) {
        // a line for each place that names the scheme, saying `said` of
        // the heads that place is for and what they draw instead
        let places = |said: &dyn Fn(&str, &str) -> String| {
            let top = top
                .iter()
                .filter(|(top, _)| *top == name)
                .map(|(_, at)| format!("{at}: {}", said(TOP_SCHEME_HEADS, TOP_INSTEAD)));
            let looks =
                looks
                    .iter()
                    .filter(|(_, named, _)| named == name)
                    .map(|(look, _, line)| {
                        let heads = format!("the heads that use look {look}");
                        at(conf, Some(*line), said(&heads, LOOK_INSTEAD)).to_string()
                    });
            top.chain(looks).collect::<Vec<_>>()
        };
        match palette::read(name, folders, gate) {
            Ok(Some(scheme)) => {
                notes.extend(scheme.ignored.iter().map(|i| {
                    let follows = matches!(i.key.as_str(), "config-file" | "theme");
                    let why = if follows {
                        "vtium never follows it, so a color scheme sets colors and nothing else"
                    } else {
                        "it is not a color vtium reads"
                    };
                    format!(
                        "{}:{}: The color scheme {name} sets {}, which vtium ignores: {why}.",
                        scheme.file.display(),
                        i.line,
                        i.quoted()
                    )
                }));
                // the reader lists the first keys it ignores and counts the
                // rest, so one note stands for all of those
                let keys = match scheme.more_ignored {
                    0 => None,
                    1 => Some("1 more key".to_owned()),
                    n => Some(format!("{n} more keys")),
                };
                notes.extend(keys.map(|keys| {
                    format!(
                        "{}: The color scheme {name} sets {keys} that vtium ignores.",
                        scheme.file.display()
                    )
                }));
            }
            Ok(None) => {
                let folders: Vec<String> =
                    folders.iter().map(|f| f.display().to_string()).collect();
                counted.extend(places(&|heads, instead| {
                    format!(
                        "The color scheme {name} does not exist. No built-in color scheme has that name, and there is no file {name}{} in {}. So {heads} {instead}.",
                        palette::SUFFIX,
                        folders.join(" or ")
                    )
                }));
            }
            Err(failure) => {
                let file = failure.file.display().to_string();
                counted.extend(failure.faults.iter().map(|f| match f.line {
                    Some(line) => format!("{file}:{line}: {}", f.reason),
                    None => format!("{file}: {}", f.reason),
                }));
                if failure.more > 0 {
                    counted.push(format!(
                        "{file}: {}",
                        crate::notice::more_errors(failure.more)
                    ));
                }
                // a file the gate refuses has no errors of its own, so the
                // places that name it say what vtium does with it
                let said = match failure.faults.first().map(|f| &f.reason) {
                    Some(palette::Reason::Inactive) => {
                        format!("The color scheme {name} is not active")
                    }
                    Some(palette::Reason::Refused(_)) => {
                        format!("vtium treats the color scheme {name} as missing")
                    }
                    _ => format!("The color scheme {name} has errors"),
                };
                counted.extend(places(&|heads, instead| {
                    format!("{said}, so {heads} {instead}.")
                }));
            }
        }
    }
    // a file that takes a built-in palette's name is never read, so only
    // the check can say so
    counted.extend(palette::shadowing(folders).iter().flat_map(|f| {
        f.faults
            .iter()
            .map(move |fault| format!("{}: {}", f.file.display(), fault.reason))
    }));
    (counted, notes)
}

/// What check-config says of each font file that the font set at `file`
/// names by its path, in a `source=`, `script-source=` or `strike=` line of
/// the kinds `file:`, `bdf:`, `hex:` and `outline:`, that a start would
/// treat as missing: a file that fails the trust rule, with the check it
/// failed, and a file the batch does not list, which is not active. Each
/// line starts with the font set's file and line. A file that does not
/// exist is left to [`font_needs`], which asks the font helper. A font
/// named by its family or by kbd's name is found by the font helper, so
/// the check does not judge it here.
///
/// References:
/// - ADR-1205 D4: a font draws only while the batch holds it and it passes the trust rule, and check-config names the file and the check it failed.
pub(super) fn font_files(file: &Path, settings: &Settings) -> Vec<String> {
    let Some(gate) = settings.gate else {
        return Vec::new();
    };
    let Ok(text) = font::frit_text(file, &settings.trust) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        // A line may end in a `#` comment, which is not part of the path.
        let line = line.split('#').next().unwrap_or_default();
        let Some((key, value)) = line.trim().split_once('=') else {
            continue;
        };
        if !matches!(key.trim(), "source" | "script-source" | "strike") {
            continue;
        }
        let Some((kind, font)) = value.trim().split_once(':') else {
            continue;
        };
        let font = Path::new(font);
        if !matches!(kind, "file" | "bdf" | "hex" | "outline") || !font.is_absolute() {
            continue;
        }
        let at = format!("{}:{}", file.display(), n.saturating_add(1));
        match gate.open(font) {
            Err(r) if r.check == vtium_settings::cullet::Check::Missing => {}
            Err(r) if r.check == vtium_settings::cullet::Check::Inactive => out.push(format!(
                "{at}: The font {} is not active, so vtium treats it as missing and draws its characters from the next font in the set. A font is active while {b} lists the file's path. To activate it, press Space on it in vtium-config's Fonts tab, or run vtium-config activate with the file's path, then install the change.",
                font.display(),
                b = vtium_settings::batch::BATCH_FILE
            )),
            Err(r) => out.push(format!(
                "{at}: vtium treats the font {} as missing, because {} vtium draws its characters from the next font in the set.",
                font.display(),
                r.check
            )),
            Ok(_) => {}
        }
    }
    out
}

/// What [`font_needs`] says of a font set's fonts: the warnings on fonts
/// this machine lacks, which count against the file with a status of
/// their own, and the notes, which do not count.
#[derive(Debug, Default)]
pub(super) struct Needs {
    pub(super) warnings: Vec<String>,
    pub(super) notes: Vec<String>,
}

/// What check-config says of the fonts that the font set `name` at `file`
/// needs and that this machine lacks or has in another form, as the glyph
/// helper answers a sources query. A font that is not installed, and an
/// installed font of that name that lacks a character its `check=` names,
/// is a warning, because vtium draws the set without it. An older version
/// than `version=` asks for, and a file whose SHA-256 is not its
/// `sha256=`, are notes, because vtium still uses the font. Each line
/// names the font set, the font and the version the set asks for, then the
/// font's page where the set gives `url=`, then the help page on
/// installing fonts. A font this machine has and the helper refused to
/// read is a note that says why. A source that names the system's font
/// rather than a font of its own gets no line. When the
/// helper gives no answer, as when it cannot start, stalls or cannot read
/// the font set, one note says so and why, so the check never passes in
/// silence. Nothing when the check asks the helper nothing, or when the
/// font set's file is refused, which the check reports on its own.
///
/// References:
/// - ADR-1204 D7: `vtium check-config` names each missing font with its
///   version and page, as `vtium config fonts` does.
pub(super) fn font_needs(name: &str, file: &Path, settings: &Settings) -> Needs {
    use check_config::font_needs;
    let Some(ask) = settings.sources else {
        return Needs::default();
    };
    // the layers' sentences are frit_layers' to report, so none is logged
    let Ok((frit, _)) = helper::FritText::read_noting(file, settings.frits, &settings.trust) else {
        return Needs::default();
    };
    let answer = match (ask.ask)(&frit) {
        Ok(answer) => answer,
        Err(why) => {
            return Needs {
                warnings: Vec::new(),
                notes: vec![font_needs::not_checked(name, &why)],
            };
        }
    };
    let lines: Vec<[&str; 4]> = answer
        .lines()
        .filter_map(|l| {
            let mut f = l.splitn(4, '\t');
            Some([f.next()?, f.next()?, f.next()?, f.next()?])
        })
        .collect();
    let mut out = Needs::default();
    for &[kind, line, spelling, value] in &lines {
        let Ok(line) = line.parse::<usize>() else {
            continue;
        };
        let Some((font, named)) = font_of(spelling) else {
            continue;
        };
        // a font this machine has and cannot read is no font to install:
        // a note that says why, with no page
        if kind == "refused" {
            out.notes.push(font_needs::refused(name, font, value));
            continue;
        }
        // a font this machine has and the font helper refuses, because the
        // batch does not list its file or the file fails the trust rule,
        // is a font a start treats as missing, though installing it again
        // would not help; a font named by its path is the font file line's
        // to report, with the font set's file and line
        if kind == "missing"
            && let Some((path, because)) = value.split_once(vtium_settings::batch::MISSING_BECAUSE)
        {
            if named == check_config::font_needs::Named::ByName {
                out.warnings.push(refused_font(name, font, path, because));
            }
            continue;
        }
        let (version, url) = needs_of(&frit.text, line, spelling);
        let version = version.as_deref();
        let (said, missing) = match kind {
            "missing" => {
                let lacks = lines.iter().find_map(|&[k, l, s, v]| {
                    (k == "lacks" && l.parse() == Ok(line) && s == spelling).then_some(v)
                });
                let said = match (version, lacks) {
                    (None, None) => font_needs::missing(name, font, named),
                    (Some(v), None) => font_needs::missing_version(name, font, v, named),
                    (None, Some(c)) => font_needs::lacks(name, font, c, named),
                    (Some(v), Some(c)) => font_needs::lacks_version(name, font, v, c, named),
                };
                (said, true)
            }
            "older" => match version {
                Some(v) => (font_needs::older(name, font, v, value), false),
                None => continue,
            },
            "sha256" => (font_needs::other_file(name, font), false),
            _ => continue,
        };
        let page = url.as_deref().map(font_needs::page);
        let mut note = said;
        for more in page.into_iter().chain([font_needs::how_to_install()]) {
            note.push(' ');
            note.push_str(&more);
        }
        if missing {
            out.warnings.push(note);
        } else {
            out.notes.push(note);
        }
    }
    out
}

/// check-config's warning on the font `font` that the font set `set`
/// needs and that the font helper treats as missing, though this machine
/// has its file at `path`, `because` the batch does not list the file or
/// the file fails the trust rule. A file that is not active comes with how
/// to activate it.
///
/// References:
/// - ADR-1205 D4: a font draws only while the batch lists it and it passes the trust rule.
pub(super) fn refused_font(set: &str, font: &str, path: &str, because: &str) -> String {
    if because == vtium_settings::batch::NOT_ACTIVE {
        format!(
            "Font set {set} needs the font {font}, and its file {path} is not active, so vtium treats the font as missing and draws its characters from the next font in the set. A font is active while {b} lists the file's path. To activate it, press Space on it in vtium-config's Fonts tab, or run vtium-config activate with the file's path, then install the change.",
            b = vtium_settings::batch::BATCH_FILE
        )
    } else {
        format!(
            "Font set {set} needs the font {font}, and vtium treats its file {path} as missing, because {because} vtium draws the font's characters from the next font in the set."
        )
    }
}

/// The font a source's or a strike's `spelling` names, as a person knows
/// it: a family's or a kbd font's name, or a file's path, and which of the
/// two it is. `None` for `vconsole`, `console:` and `builtin:`, which name
/// no font a person installs.
pub(super) fn font_of(spelling: &str) -> Option<(&str, check_config::font_needs::Named)> {
    let (kind, rest) = spelling.split_once(':')?;
    match kind {
        "font" | "kbd" => Some((rest, check_config::font_needs::Named::ByName)),
        "file" | "bdf" | "hex" | "outline" => Some((rest, check_config::font_needs::Named::ByPath)),
        _ => None,
    }
}

/// The `version=` and `url=` lines of the font that `spelling` names on
/// the font set's own line `line`, in `text`, the font set with its layers
/// in place. A layer's source names the font set's `layer=` line, as the
/// helper's answer does, so its spelling tells it from the layer's other
/// sources.
///
/// `spelling` is a `source=` line's or a `strike=` line's. A strike is a
/// file of its source's font at one more height, so a strike takes the
/// `version=` and `url=` of the source it follows. Those lines describe
/// the source wherever they stand among its lines, before a `strike=` line
/// or after one, and only a `sha256=` line belongs to the strike above it.
///
/// References:
/// - ADR-1204 D2: the `strike=` key, and which lines after it belong to the strike.
/// - ADR-1204 D7: check-config names a missing font's version and page.
pub(super) fn needs_of(
    text: &str,
    line: usize,
    spelling: &str,
) -> (Option<String>, Option<String>) {
    let own = vtium_font::set::layer::lines(text);
    // the version= and url= of the source being read, and whether that
    // source or one of its strikes is the font asked about
    let mut found = (None, None);
    let mut named = false;
    for (body, at) in text.split('\n').zip(own) {
        let at = match at {
            vtium_font::set::layer::Line::Set(n) => n,
            vtium_font::set::layer::Line::Layer { at, .. } => at,
        };
        let body = body.split('#').next().unwrap_or_default();
        let Some((key, value)) = body.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        let this = at == line && value == spelling;
        match key {
            "source" | "layer" if named => break,
            "source" | "layer" => {
                found = (None, None);
                named = key == "source" && this;
            }
            "strike" => named |= this,
            "version" => found.0 = Some(value.to_owned()),
            "url" => found.1 = Some(value.to_owned()),
            _ => {}
        }
    }
    if named { found } else { (None, None) }
}
