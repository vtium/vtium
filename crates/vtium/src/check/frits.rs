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
/// account reads it is a note. A name no frit bears there is
/// `vtium_settings::warnings`' to report.
pub(super) fn look_frits(out: &mut HeadCheck, path: &Path, parsed: &Parsed, settings: &Settings) {
    for look in &parsed.config.looks {
        let Some(named) = look
            .settings
            .iter()
            .find(|s| s.key.name == keys::FONT_NAME.name)
        else {
            continue;
        };
        let Some(found) = settings.draft.find(settings.frits, &named.value) else {
            continue;
        };
        let line = Some(named.line);
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
