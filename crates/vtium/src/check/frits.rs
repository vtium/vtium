//! The font sets a draft adds or takes away, and how check-config judges
//! each font set a look names.

use std::path::{Path, PathBuf};

use vtium_settings::{Parsed, Trust, at};

use crate::config::{Config, keys};
use crate::font;
use crate::helper;
use crate::notice::check_config;

use super::conf::{HeadCheck, Settings, Squeeze};
use super::resources::{font_files, font_needs};
#[cfg(doc)]
use super::{WITH_FRIT, WITHOUT_FRIT};

/// The folders where the font helper finds a kbd console font by its
/// name, in the order it searches them: the `consolefonts` folder of kbd's
/// own data folder, of Debian's console-setup and of the Red Hat family's.
pub const CONSOLE_FONT_FOLDERS: [&str; 3] = [
    "/usr/share/kbd/consolefonts",
    "/usr/share/consolefonts",
    "/usr/lib/kbd/consolefonts",
];

/// The font sets of vtium's catalog that every vtium console package
/// installs, the Makefile's `CONSOLE_FRITS`.
pub(super) const CONSOLE_PACKAGE_SETS: [&str; 5] = [
    "terminus",
    "cullet-mono",
    "jetbrains-mono",
    "unifont-icons",
    "unscii-icons",
];

/// The rest of vtium's catalog, the font sets the vtium-fonts package
/// installs beside the fonts they name.
///
/// References:
/// - ADR-1204 D9: vtium-fonts ships the catalog beside the fonts it names.
pub(super) const FONTS_PACKAGE_SETS: [&str; 33] = [
    "dec-vt220",
    "ibm-vga",
    "sun-gallant",
    "cascadia-code",
    "dejavu-sans-mono",
    "fira-code",
    "hack",
    "ibm-plex-mono",
    "monoid",
    "source-code-pro",
    "default8x16",
    "linux-sun-12x22",
    "linux-sun-8x16",
    "linux-ter-16x32",
    "linux-vga-8x16",
    "linux-vga-8x8",
    "caskaydia-cove",
    "fira-code-nerd",
    "hack-nerd",
    "jetbrains-mono-nerd",
    "monoid-nerd",
    "sauce-code-pro",
    "terminess",
    "noto-arabic",
    "noto-cjk-jp",
    "noto-cjk-kr",
    "noto-cjk-sc",
    "noto-cjk-tc",
    "noto-hebrew",
    "noto-indic",
    "noto-thai-lao",
    "noto-vietnamese",
    "tiny5",
];

/// The font sets a draft adds, replaces and removes, as the subcommand's
/// [`WITH_FRIT`] and [`WITHOUT_FRIT`] give them. vtium-config keeps a
/// draft's font sets in a folder beside the draft until the install, and
/// passes them here, so the check judges the draft against the frits the
/// install leaves. A frit named here takes the place of the one of its
/// name in the frits' directory, and a removed frit counts as missing.
///
/// References:
/// - ADR-1403 D8, "A draft's font set changes": the two options, and that the check counts a frit as installed when the directory holds it or `--with-frit` names it, and not when `--without-frit` names it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DraftFrits {
    /// Each frit the draft adds or replaces: its name and its file.
    pub with: Vec<(String, PathBuf)>,
    /// Each installed frit the draft removes, by name.
    pub without: Vec<String>,
}

/// Where the check finds a frit by its name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Found {
    pub(super) file: PathBuf,
    /// The file is the draft's, which the install copies into the frits'
    /// directory as root's.
    draft: bool,
}

impl DraftFrits {
    /// Takes `name`'s file as a frit the draft adds or replaces, in place
    /// of any earlier option for that name.
    pub fn add(&mut self, name: String, file: PathBuf) {
        self.forget(&name);
        self.with.push((name, file));
    }

    /// Takes `name` as a frit the draft removes, in place of any earlier
    /// option for that name.
    pub fn remove(&mut self, name: String) {
        self.forget(&name);
        self.without.push(name);
    }

    fn forget(&mut self, name: &str) {
        self.with.retain(|(n, _)| n != name);
        self.without.retain(|n| n != name);
    }

    /// The frit `name` as the check sees it in `frits` with the draft's
    /// changes: the draft's file for a frit it adds or replaces, nothing
    /// for one it removes, and otherwise [`font::frit`]'s lookup.
    pub(super) fn find(&self, frits: &Path, name: &str) -> Option<Found> {
        if self.without.iter().any(|n| n == name) {
            return None;
        }
        if let Some((_, file)) = self.with.iter().find(|(n, _)| n == name) {
            return Some(Found {
                file: file.clone(),
                draft: true,
            });
        }
        font::frit(frits, name).map(|file| Found { file, draft: false })
    }
}

/// The frit name of the font set file `path` that [`WITH_FRIT`] names:
/// its file name without `.vtfrit`, when that is a name a frit can have
/// and `path` is a regular file. The frits' own lookup decides both, in
/// the folder that holds `path`, so a name the lookup refuses, such as
/// one that reaches outside a folder, never names a draft's frit.
pub fn with_frit_name(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?.strip_suffix(".vtfrit")?;
    let folder = path.parent()?;
    (font::frit(folder, name)? == folder.join(format!("{name}.vtfrit"))).then(|| name.to_owned())
}

/// Whether a `font-name` whose value is `name` gives a font that vtium
/// loads, found as the start finds it: a built-in font, a frit in
/// `settings`' directory with the draft's changes, or else a kbd console
/// font of that name in one of `settings`' console font folders, which
/// the font helper reads for the frit vtium makes on the fly. A name that
/// starts with `/` is a font file's path, which the start takes as it is,
/// so it counts as found here. A name with an empty, `.` or `..`
/// component reaches outside the folders, and the font helper refuses
/// such a font, so it is not found.
pub(super) fn font_found(name: &str, settings: &Settings) -> bool {
    name.starts_with('/')
        || font::builtin_named(name, None).is_some()
        || settings.draft.find(settings.frits, name).is_some()
        || settings
            .console_fonts
            .iter()
            .any(|folder| kbd_font(name, folder))
}

/// Whether `folder` holds the kbd console font `name` as the font helper
/// finds it, which is how kbd's `setfont` finds it: the name, then the
/// name with each suffix `setfont` tries, each plain and then compressed.
/// A name with `/` in it names a font in a folder below `folder`.
fn kbd_font(name: &str, folder: &Path) -> bool {
    use vtium_settings::batch::{COMPRESSED, FONT_SUFFIXES};
    if name
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return false;
    }
    FONT_SUFFIXES.iter().any(|suffix| {
        COMPRESSED
            .iter()
            .any(|z| folder.join(format!("{name}{suffix}{z}")).is_file())
    })
}

/// The warning on a `font-name` whose value, `name`, gives no font that
/// vtium loads ([`font_found`]), as `set` says where it is set, with the
/// places the check looked in and what to do: install the package that
/// ships the font set when `name` is one of vtium's catalog, and
/// otherwise correct the name or install the font set.
pub(super) fn font_not_found(
    set: check_config::FontNameSet<'_>,
    name: &str,
    settings: &Settings,
) -> String {
    let package = if CONSOLE_PACKAGE_SETS.contains(&name) {
        check_config::SetPackage::Console
    } else if FONTS_PACKAGE_SETS.contains(&name) {
        check_config::SetPackage::Fonts
    } else {
        check_config::SetPackage::None
    };
    check_config::font_not_found(set, name, settings.frits, &settings.console_fonts, package)
}

/// The warning on the top level's `font-name` in `config` when it names
/// a font that vtium does not find ([`font_found`]), headed by where it
/// is set; none when it is unset, names a font vtium finds, or names a
/// font family under `font-engine=freetype` or `pango`, which the font
/// helper looks up among the installed fonts.
pub(super) fn font_name_not_found(config: &Config, settings: &Settings) -> Option<String> {
    let wanted = font::wanted(config, "", "").filter(|w| w.came == font::Came::Config)?;
    if font_found(&wanted.name, settings) {
        return None;
    }
    let origin = config.origin(&keys::FONT_NAME)?;
    let set = match origin {
        crate::config::Origin::CommandLine { .. } => check_config::FontNameSet::CommandLine,
        _ => check_config::FontNameSet::TopLevel,
    };
    Some(format!(
        "{origin}: {}",
        font_not_found(set, &wanted.name, settings)
    ))
}

/// What the frit `font-name` names in `frits` says of its cell, as
/// [`font::frit_warnings`] reads it, once the ownership check `trust`
/// makes passes; nothing when the name is no frit's. A frit that cannot be
/// read says nothing here: at the start the font's own read logs why, for
/// the font-name frit and every look's alike, and check-config reports it
/// with [`font_name_frit`] and [`look_frits`].
pub(super) fn frit(config: &Config, frits: &Path, trust: &Trust) -> Vec<String> {
    let Some((name, Found { file: path, .. })) = named_frit(config, frits, &DraftFrits::default())
    else {
        return Vec::new();
    };
    font::frit_text(&path, trust)
        .map_or_else(|_| Vec::new(), |text| font::frit_warnings(&name, &text))
}

/// The name of the frit `font-name` names in `frits` with the draft's
/// changes `draft`, and where it is; none when the name is no frit's
/// there.
pub(super) fn named_frit(
    config: &Config,
    frits: &Path,
    draft: &DraftFrits,
) -> Option<(String, Found)> {
    let wanted = font::wanted(config, "", "").filter(|w| w.came == font::Came::Config)?;
    let found = draft.find(frits, &wanted.name)?;
    Some((wanted.name, found))
}

/// What check-config finds of a frit's file read under `trust`, the
/// ownership check of the account the check runs as (ADR-1401 D10).
#[derive(Debug, PartialEq, Eq)]
enum Judged {
    /// The account reads it, and so would a start as root.
    Read,
    /// The account's own check refuses it, or it cannot be read: why.
    Refused(String),
    /// The account reads it, but a start as root, which reads only root's
    /// frits, would refuse it: why. Only a check run as another account
    /// than root finds this.
    RootRefuses(String),
}

/// How the frit `found` reads under `trust`, and, where `trust` is not
/// root's, under root's too, as `helper_note` judges the font helper. A
/// draft's frit is read under `trust` alone: the install copies it into
/// the frits' directory as root's, so who owns the draft's file says
/// nothing of the start.
fn judge(found: &Found, trust: &Trust) -> Judged {
    let file = &found.file;
    match font::frit_text(file, trust) {
        Err(why) => Judged::Refused(why),
        Ok(_) if *trust == Trust::ROOT || found.draft => Judged::Read,
        Ok(_) => font::frit_text(file, &Trust::ROOT)
            .err()
            .map_or(Judged::Read, Judged::RootRefuses),
    }
}

/// What check-config says of the layers the frit `name` at `file` adds,
/// read from the frits' directory `frits` under `trust` as the start reads
/// them: a sentence for each
/// `layer=` line that adds nothing, such as a missing layer, a layer the
/// ownership check refuses or a layer whose `role=` is wrong. Each counts
/// against the configuration, as a look's missing frit does, since the
/// start draws the set without that layer. A frit the account cannot read
/// says nothing here: [`judge`] reports it.
///
/// References:
/// - ADR-1204 D5: `vtium check-config` reads a set's layers too.
pub(super) fn frit_layers(name: &str, file: &Path, frits: &Path, trust: &Trust) -> Vec<String> {
    font::frit_text(file, trust).map_or_else(
        |_| Vec::new(),
        |text| font::with_layers(name, frits, &text, trust).1,
    )
}

/// What check-config says of the frit `font-name` names in `frits`, read
/// under `trust`: the refusal, which counts against the configuration, or
/// the refusal of a start as root, which is a note.
pub(super) fn font_name_frit(
    config: &Config,
    frits: &Path,
    trust: &Trust,
    draft: &DraftFrits,
) -> (Option<String>, Option<String>) {
    let Some((name, found)) = named_frit(config, frits, draft) else {
        return (None, None);
    };
    let file = &found.file;
    match judge(&found, trust) {
        Judged::Read => (None, None),
        Judged::Refused(why) => (Some(check_config::frit_unreadable(&name, file, &why)), None),
        Judged::RootRefuses(why) => (
            None,
            Some(check_config::frit_refused_as_root(&name, file, &why)),
        ),
    }
}

/// Each look's frit in `settings`' directory as check-config judges it
/// (ADR-1401 D10, ADR-1403 D5), on the look's `font-name` line: one the
/// ownership check of the account the check runs as refuses, or that
/// cannot be read, counts against the file, since the start draws that
/// look in its built-in font; one a start as root would refuse though the
/// account reads it is a note. A name that gives no font vtium loads
/// ([`font_found`]) counts against the file with [`font_not_found`]'s
/// warning, which takes the place of `vtium_settings::warnings`' warning
/// on a look's missing frit; a built-in font or a kbd console font is no
/// frit, so nothing more is judged of it.
pub(super) fn look_frits(out: &mut HeadCheck, path: &Path, parsed: &Parsed, settings: &Settings) {
    for look in &parsed.config.looks {
        let Some(named) = look
            .settings
            .iter()
            .find(|s| s.key.name == keys::FONT_NAME.name)
        else {
            continue;
        };
        let line = Some(named.line);
        if !font_found(&named.value, settings) {
            let set = check_config::FontNameSet::Look(&look.name);
            let said = font_not_found(set, &named.value, settings);
            out.counted.push(at(path, line, said).to_string());
            continue;
        }
        // a built-in font or a kbd console font is no frit to judge
        let Some(found) = settings.draft.find(settings.frits, &named.value) else {
            continue;
        };
        let judged = judge(&found, &settings.trust);
        let file = found.file;
        // a font set the account reads is checked for its layers and for
        // the locale's letters
        if matches!(judged, Judged::Read | Judged::RootRefuses(_)) {
            for said in frit_layers(&named.value, &file, settings.frits, &settings.trust) {
                out.counted.push(at(path, line, said).to_string());
            }
            let said = settings
                .squeeze
                .and_then(|sq| squeeze_note(&look.name, &named.value, &file, settings, sq));
            out.notes
                .extend(said.map(|said| at(path, line, said).to_string()));
            let needs = font_needs(&named.value, &file, settings);
            out.missing_fonts = out.missing_fonts.saturating_add(needs.warnings.len());
            for said in needs.warnings {
                out.counted.push(at(path, line, said).to_string());
            }
            // each line names its font set's file and line, so a font set
            // that several looks use is said once
            for said in font_files(&file, settings) {
                if !out.counted.contains(&said) {
                    out.missing_fonts = out.missing_fonts.saturating_add(1);
                    out.counted.push(said);
                }
            }
            for said in needs.notes {
                out.notes.push(at(path, line, said).to_string());
            }
        }
        match judged {
            Judged::Read => {}
            Judged::Refused(why) => {
                let said =
                    check_config::look_frit_unreadable(&look.name, &named.value, &file, &why);
                out.counted.push(at(path, line, said).to_string());
            }
            Judged::RootRefuses(why) => {
                let said =
                    check_config::look_frit_refused_as_root(&look.name, &named.value, &file, &why);
                out.notes.push(at(path, line, said).to_string());
            }
        }
    }
}

/// The note on a look, `look`, whose font set `name` at `file` leaves
/// everyday letters of the system locale's language, CLDR's main exemplar
/// characters, to the squeeze (ADR-1401 D7, ADR-1302 D14): the letters the
/// built-in fonts draw squeezed in the font set's cell, of those the font
/// set does not cover, as the glyph helper answers. The note names the
/// script they are written in, and an installed font that covers them
/// where the helper finds one. `None` when the locale's language is not in
/// the table, when the built-in fonts squeeze none of its letters, when
/// the font set covers every such letter, or when the helper cannot
/// answer.
fn squeeze_note(
    look: &str,
    name: &str,
    file: &Path,
    settings: &Settings,
    sq: Squeeze<'_>,
) -> Option<String> {
    let script = vtium_font::letters::script_of(sq.locale)?;
    // the layers' sentences are frit_layers' to report, so none is logged
    let (frit, _) = helper::FritText::read_noting(file, settings.frits, &settings.trust).ok()?;
    let cell = font::frit_cell(&frit.text)
        .ok()
        .flatten()
        .unwrap_or(vtium_font::Cell {
            width: 8,
            height: 16,
        });
    let squeezed: Vec<u32> = vtium_font::letters::of_locale(sq.locale)
        .into_iter()
        .filter(|&c| font::builtin_squeezes(c, cell, sq.locale))
        .map(u32::from)
        .collect();
    if squeezed.is_empty() {
        return None;
    }
    let (covered, installed) = (sq.ask)(&frit, &squeezed).ok()?;
    let left = squeezed.iter().filter(|c| !covered.contains(c)).count();
    (left > 0)
        .then(|| check_config::squeezed_script(look, name, (script, left), installed.as_deref()))
}
