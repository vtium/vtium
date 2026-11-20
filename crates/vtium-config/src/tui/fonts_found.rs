//! The fonts the Fonts tab lists, and what the font helper said of each.
//! The scan finds the font files in the system's folders, vtium's own,
//! kbd's console-font folders, the folders that `font-paths` names, and
//! the person's own folders. Each file carries the marks every resource
//! carries: whether it is active, whether it passes the trust rule, and
//! whether it sits in the person's own folder, where vtium cannot use it.
//! A font in the administrator's folder that a package also installed
//! carries a duplicate mark.
//!
//! The font helper describes the files while the TUI goes on reading
//! keys, and each answer joins the list as it comes. The descriptions
//! give the tab's filters what they test, a font's detail what it covers,
//! and a look's card the sizes its text font supplies.
//!
//! References:
//! - ADR-1205 D3: the folders, and the font helper as the one reader of a
//!   font's family, coverage and sizes.
//! - ADR-1205 D5: a font in a user's folder is not usable until it is
//!   installed as a system font, and the duplicate mark.
//! - ADR-1205 D7: a look offers the sizes its fonts supply.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::batch::State;
use crate::describe::{Face, Group, SCRIPTS, Strike};
use crate::discover::{Described, Duplicate, Found, Place};
use crate::frit::{Frit, Origin};

use super::model::{Font, FontState};
use super::resources::Resource;

/// What a list calls a font, as the other resources name their kind.
pub const KIND: &str = "font";

/// One font file the scan found, with its marks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontFile {
    pub resource: Resource,
    /// The kind of folder it was found under.
    pub place: Place,
    /// The file a package installed that this font repeats.
    pub duplicate: Option<Duplicate>,
    /// The font the previews draw the file in, named by the file's path.
    pub specimen: Font,
}

impl FontFile {
    /// The file `found` as the list shows it.
    pub fn of(found: Found) -> FontFile {
        let own = !found.usable();
        let specimen = Font {
            name: found.path.to_string_lossy().into_owned(),
            state: FontState::Reads,
        };
        FontFile {
            resource: Resource {
                path: found.path,
                kind: KIND,
                state: found.state,
                trust: found.trust,
                own,
                hidden: false,
            },
            place: found.folder.place,
            duplicate: found.duplicate,
            specimen,
        }
    }

    /// The file's path.
    pub fn path(&self) -> &Path {
        &self.resource.path
    }
}

/// The fonts found, and the descriptions that have come so far.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fonts {
    pub files: Vec<FontFile>,
    /// What the helper said of each file it has answered for.
    described: BTreeMap<PathBuf, Described>,
    /// Each file the font helper has been asked to describe, answered or
    /// not, so no file is asked for twice.
    asked: std::collections::BTreeSet<PathBuf>,
    /// Why the marks may be wrong, such as a batch the TUI cannot read.
    pub note: Option<String>,
}

impl Fonts {
    /// The list of `found`, with nothing described yet.
    pub fn new(found: Vec<Found>) -> Fonts {
        Fonts {
            files: found.into_iter().map(FontFile::of).collect(),
            described: BTreeMap::new(),
            asked: std::collections::BTreeSet::new(),
            note: None,
        }
    }

    /// Takes a new scan's list in place of the old one, keeping what the
    /// helper said of each file that is still there.
    pub fn rescanned(&mut self, found: Vec<Found>) {
        self.files = found.into_iter().map(FontFile::of).collect();
        let paths: std::collections::BTreeSet<&Path> =
            self.files.iter().map(FontFile::path).collect();
        self.described.retain(|p, _| paths.contains(p.as_path()));
    }

    /// Forgets what the helper said of the file at `path`, and that it was
    /// asked, so the next [`Fonts::ask`] asks for the file again. An
    /// install that replaces a file keeps the file's path, so the list
    /// would otherwise show the replaced file's description.
    pub fn forget(&mut self, path: &Path) {
        self.described.remove(path);
        self.asked.remove(path);
    }

    /// Adds the system font `target`, just installed, to the list when the
    /// scan after the install failed with `why`: the list keeps the files
    /// it had, the new font joins the system's group, and the note says
    /// why the list may be out of date. The new font is not checked
    /// against the trust rule, so its trust mark says so.
    pub fn added_unscanned(&mut self, target: &Path, why: &str) {
        if self.file(target).is_none() {
            let found = Found {
                path: target.to_path_buf(),
                folder: crate::discover::Folder {
                    path: target.parent().map(Path::to_path_buf).unwrap_or_default(),
                    place: Place::System,
                },
                trust: Err(super::words::FONT_UNCHECKED.to_owned()),
                state: State::Inactive,
                duplicate: None,
            };
            let at = self
                .files
                .iter()
                .rposition(|f| f.place == Place::System)
                .map_or(0, |i| i.saturating_add(1));
            self.files.insert(at, FontFile::of(found));
        }
        self.note = Some(super::words::fonts_rescan_failed(why));
    }

    /// Gives each file its state in the draft from `states`, as
    /// [`super::resources::set_states`] does for the other resources.
    pub fn set_states(&mut self, states: &[(PathBuf, State)]) {
        for f in &mut self.files {
            f.resource.state = states
                .iter()
                .find(|(p, _)| *p == f.resource.path)
                .map_or(State::Inactive, |(_, s)| *s);
        }
    }

    /// Records what the helper said of each file in `answers`.
    pub fn describe(&mut self, answers: Vec<(PathBuf, Described)>) {
        self.described.extend(answers);
    }

    /// What the helper said of the file at `path`, once it has answered.
    pub fn described(&self, path: &Path) -> Option<&Described> {
        self.described.get(path)
    }

    /// The files the helper has not been asked to describe yet, in the
    /// list's order, which are marked as asked. A file asked for earlier
    /// and not answered yet is not asked for again, so an install while
    /// the first descriptions are coming asks only for the new file.
    pub fn ask(&mut self) -> Vec<PathBuf> {
        let new: Vec<PathBuf> = self
            .files
            .iter()
            .map(|f| f.path().to_path_buf())
            .filter(|p| !self.described.contains_key(p) && !self.asked.contains(p))
            .collect();
        self.asked.extend(new.iter().cloned());
        new
    }

    /// The files the helper has not answered for yet, in the list's order.
    pub fn waiting(&self) -> Vec<PathBuf> {
        self.files
            .iter()
            .map(|f| f.path().to_path_buf())
            .filter(|p| !self.described.contains_key(p))
            .collect()
    }

    /// The faces the helper found in the file at `path`: none until it
    /// answers, and none for a file it cannot read as a font.
    pub fn faces(&self, path: &Path) -> &[Face] {
        match self.described.get(path) {
            Some(Described::Font(d)) => &d.faces,
            _ => &[],
        }
    }

    /// The file at `path`, where the scan found it.
    pub fn file(&self, path: &Path) -> Option<&FontFile> {
        self.files.iter().find(|f| f.resource.path == path)
    }

    /// The sizes the text font of `frit` supplies, from the descriptions:
    /// the sizes of the one file the console draws the text font from,
    /// and of each file the set's `strike=` lines add. A path names its
    /// file. A kbd name names the first file found of that name with any
    /// of kbd's suffixes. A family names the first described file with a
    /// Regular face of that family, else the first with any face of it,
    /// with case and blanks ignored as the console ignores them. An
    /// outline face supplies any size, and a bitmap face exactly its
    /// strikes. Other files of the same design are not counted, because
    /// the console never draws them: a size only they supply would fall
    /// back to the built-in font.
    ///
    /// References:
    /// - ADR-1205 D7: a look offers only the sizes its fonts supply; a
    ///   bitmap font supplies its strikes, and an outline font its range
    ///   of sizes.
    /// - ADR-1204 D2: only `strike=` lines add strikes to a set.
    pub fn supplied(&self, frit: &Frit) -> Supplied {
        let Some(text) = frit.sources.iter().find(|s| s.layer.is_none()) else {
            return Supplied::Unknown(Unknown::NoTextFont);
        };
        let origins = std::iter::once(&text.origin).chain(text.strikes.iter().map(|s| &s.origin));
        let mut matched: Vec<&Face> = Vec::new();
        let mut files = 0usize;
        let mut waiting = false;
        for origin in origins {
            let path: Option<&Path> = match origin {
                Origin::Builtin(_) | Origin::Vconsole | Origin::Console(_) => {
                    return Supplied::Unknown(Unknown::BuiltIn);
                }
                Origin::File(p) | Origin::Bdf(p) | Origin::Hex(p) | Origin::Outline(p) => {
                    Some(p.as_path())
                }
                Origin::Kbd(name) => self
                    .files
                    .iter()
                    .map(FontFile::path)
                    .find(|p| kbd_name(p) == Some(name.as_str())),
                Origin::Font(family) => {
                    let (path, unanswered) = self.family_file(family);
                    waiting |= unanswered && path.is_none();
                    path
                }
            };
            let Some(path) = path else { continue };
            files = files.saturating_add(1);
            match self.described.get(path) {
                None => waiting = true,
                Some(Described::Font(d)) => {
                    matched.extend(d.faces.iter().filter(|face| match origin {
                        Origin::Font(family) => squashed(&face.family) == squashed(family),
                        _ => true,
                    }))
                }
                Some(_) => {}
            }
        }
        let family = matched
            .first()
            .map_or_else(|| text.spelling.clone(), |f| f.family.clone());
        if matched.iter().any(|f| f.outline) {
            return Supplied::Any { family };
        }
        if matched.is_empty() {
            return Supplied::Unknown(if waiting {
                Unknown::Describing
            } else if files == 0 {
                Unknown::NotFound(text.spelling.clone())
            } else {
                Unknown::NotAFont(text.spelling.clone())
            });
        }
        let mut strikes: Vec<Strike> = matched
            .iter()
            .flat_map(|f| f.strikes.iter().copied())
            .collect();
        strikes.sort_unstable_by_key(|s| (s.height, s.width));
        strikes.dedup();
        if strikes.is_empty() {
            return Supplied::Unknown(Unknown::NotAFont(text.spelling.clone()));
        }
        Supplied::Strikes { family, strikes }
    }

    /// The file a `font:` source of `family` draws from, among the files
    /// described so far: the first with a Regular face of the family,
    /// else the first with any face of it. The second value says whether
    /// some file is not described yet, so that it could still be the one.
    fn family_file(&self, family: &str) -> (Option<&Path>, bool) {
        let want = squashed(family);
        let mut waiting = false;
        let mut any = None;
        for f in &self.files {
            match self.described.get(f.path()) {
                None => waiting = true,
                Some(Described::Font(d)) => {
                    let mut faces = d.faces.iter().filter(|face| squashed(&face.family) == want);
                    let first = faces.next();
                    if first.is_some_and(|face| face.style == "Regular")
                        || faces.any(|face| face.style == "Regular")
                    {
                        return (Some(f.path()), waiting);
                    }
                    if first.is_some() && any.is_none() {
                        any = Some(f.path());
                    }
                }
                Some(_) => {}
            }
        }
        (any, waiting)
    }
}

/// The font set the console draws when no look and no `font-name` names
/// one and vconsole names no font.
///
/// References:
/// - ADR-1204 D2: `terminus` is the default set, after the font that a
///   head's look, `font-name` or vconsole names.
pub const DEFAULT_SET: &str = "terminus";

/// The sizes the text font of the font set `name` supplies, from what the
/// Fonts tab's descriptions say: the font set as the install would leave
/// it, a new one that waits for the install, else the installed file. A
/// built-in font's name is the font itself, and so is any name while
/// `font-engine` names a built-in font. With no font set named, the
/// console draws vconsole's font, a font set of that name or else the kbd
/// font, and with none of those the default set.
///
/// References:
/// - ADR-1200 D9: `font-name`, else vconsole's font.
/// - ADR-1204 D2: the default set comes after vconsole's font.
pub fn supplied_by(shared: &super::model::Shared, name: Option<&str>) -> Supplied {
    if matches!(
        shared.drafted("font-engine").as_deref(),
        Some("unifont" | "8x16")
    ) {
        return Supplied::Unknown(Unknown::BuiltIn);
    }
    let named = name.filter(|n| !n.is_empty());
    let vconsole = shared
        .inputs
        .vconsole_font
        .as_deref()
        .filter(|_| named.is_none());
    let name = named.or(vconsole).unwrap_or(DEFAULT_SET);
    if crate::frit::RESERVED.contains(&name) {
        return Supplied::Unknown(Unknown::BuiltIn);
    }
    let text = shared
        .pending
        .frits
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, t)| t.as_bytes().to_vec())
        .or_else(|| shared.inputs.installed.get(name).cloned())
        // vconsole's font with no font set of its name is a kbd font
        .or_else(|| vconsole.map(|v| format!("source=kbd:{v}\n").into_bytes()));
    match text.and_then(|t| Frit::parse(&t, Path::new(name), name).ok()) {
        Some(frit) => shared.inputs.font_files.supplied(&frit),
        None => Supplied::Unknown(Unknown::NoSet(name.to_owned())),
    }
}

/// The density a look's card converts point sizes at, and whether the
/// displays differ, so that one point size draws a different strike on
/// some of them. The card takes the lowest density, as the console does
/// for displays that share one grid. vtium-config cannot yet tell which
/// displays each head draws on, so every connected display counts.
///
/// References:
/// - ADR-1200 D17: the lowest density among the displays converts the
///   points.
pub fn sizes_density(shared: &super::model::Shared) -> (u32, bool) {
    let densities = &shared.inputs.densities;
    let lowest = densities
        .iter()
        .copied()
        .min()
        .unwrap_or(shared.inputs.dpi.0);
    (lowest, densities.iter().any(|d| *d != lowest))
}

/// What sizes a look's text font supplies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Supplied {
    /// The descriptions do not say, and why.
    Unknown(Unknown),
    /// An outline font of this family, which draws at any size.
    Any { family: String },
    /// A bitmap font of this family, with these strikes, shortest first.
    Strikes {
        family: String,
        strikes: Vec<Strike>,
    },
}

/// Why the descriptions do not say which sizes a text font supplies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unknown {
    /// The font set has no source of its own.
    NoTextFont,
    /// No font set of this name is installed or waits for the install.
    NoSet(String),
    /// The text font is built into vtium, or is the console's own font.
    BuiltIn,
    /// The font helper has not described every file that could be it.
    Describing,
    /// No font file found on this system is the text font, by this
    /// spelling.
    NotFound(String),
    /// The files found by this spelling are not fonts the helper reads.
    NotAFont(String),
}

/// The point sizes that draw each of `strikes` at `dpi`, each with the
/// strike it draws: for each strike, the smallest whole point size whose
/// height in pixels reaches the strike's height and stays below the next
/// strike's. A cell draws the tallest strike that fits it, so that size
/// draws this strike. A strike that no whole point size reaches before
/// the next strike's height is left out.
///
/// References:
/// - ADR-1200 D17: points × DPI ÷ 72 pixels, rounded to the nearest pixel.
/// - ADR-1204 D2: each cell draws the tallest strike that fits it.
pub fn strike_points(strikes: &[Strike], dpi: u32) -> Vec<(u32, Strike)> {
    let dpi = u64::from(dpi.max(1));
    let pixels = |points: u32| (u64::from(points) * dpi * 2 + 72) / 144;
    let mut heights: Vec<u32> = strikes.iter().map(|s| s.height).collect();
    heights.dedup();
    let mut out = Vec::new();
    for (i, strike) in strikes.iter().enumerate() {
        if strikes
            .get(i.wrapping_add(1))
            .is_some_and(|n| n.height == strike.height)
        {
            continue;
        }
        let next = heights
            .iter()
            .find(|h| **h > strike.height)
            .map_or(u64::MAX, |h| u64::from(*h));
        let points = (1..=super::model::SIZES.iter().copied().max().unwrap_or(32) * 4)
            .find(|p| pixels(*p) >= u64::from(strike.height));
        if let Some(points) = points
            && pixels(points) < next
            && !out.iter().any(|(p, _)| *p == points)
        {
            out.push((points, *strike));
        }
    }
    out
}

/// Whether `name`, a name the previews draw a font by, names a font file:
/// an absolute path, which no font set's name can be.
pub fn is_file_name(name: &str) -> bool {
    name.starts_with('/')
}

/// The `source=` value of a font set that draws the file at `path` alone,
/// by its extension: `bdf:` for a BDF font, `hex:` for a `.hex` font,
/// `file:` for a PSF font and `outline:` for an OpenType file. `None` for
/// a path that is not UTF-8 or that holds a control character, which
/// would end the `source=` line and start another.
pub fn source_of(path: &Path) -> Option<String> {
    let text = path.to_str()?;
    if text.chars().any(char::is_control) {
        return None;
    }
    let name = path.file_name()?.to_str()?.to_ascii_lowercase();
    let mut parts = name.rsplit('.');
    let mut ext = parts.next()?;
    if crate::discover::COMPRESSED.contains(&ext) {
        ext = parts.next()?;
    }
    let kind = match ext {
        "bdf" => "bdf",
        "hex" => "hex",
        "psf" | "psfu" => "file",
        _ => "outline",
    };
    Some(format!("{kind}:{text}"))
}

/// A family with case and blanks ignored, as the console matches one.
pub fn squashed(family: &str) -> String {
    family
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The kbd name of a console font's file: its name before the first dot,
/// as `ter-v16n` for `ter-v16n.psf.gz`.
fn kbd_name(path: &Path) -> Option<&str> {
    path.file_name()?.to_str()?.split('.').next()
}

/// The filters of the Fonts tab, in the order the list of filters shows
/// them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    /// Every font, described or not.
    #[default]
    All,
    Bitmap,
    Outline,
    Monospaced,
    Proportional,
    Group(Group),
    /// A script, by its name in Unicode's Scripts.txt.
    Script(&'static str),
}

impl Filter {
    /// Every filter, in the list's order: all fonts, the kinds, the
    /// groups, then each script.
    pub fn choices() -> Vec<Filter> {
        let mut out = vec![
            Filter::All,
            Filter::Bitmap,
            Filter::Outline,
            Filter::Monospaced,
            Filter::Proportional,
        ];
        out.extend(Group::ALL.iter().map(|g| Filter::Group(*g)));
        out.extend(SCRIPTS.iter().map(|s| Filter::Script(s.name)));
        out
    }

    /// Whether the filter keeps a font of which the helper said
    /// `described`. Only All keeps a font not described yet, or one the
    /// helper cannot read, since nothing is known of it. A font is kept
    /// when any of its faces passes.
    pub fn keeps(self, described: Option<&Described>) -> bool {
        if self == Filter::All {
            return true;
        }
        let Some(Described::Font(d)) = described else {
            return false;
        };
        d.faces.iter().any(|f| match self {
            Filter::All => true,
            Filter::Bitmap => !f.outline,
            Filter::Outline => f.outline,
            Filter::Monospaced => f.monospaced,
            Filter::Proportional => !f.monospaced,
            Filter::Group(g) => f.groups.contains(&g),
            Filter::Script(s) => f.scripts.contains(&s),
        })
    }
}

#[cfg(test)]
#[expect(clippy::panic, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use crate::describe::{Description, Format};

    fn face(family: &str, style: &str, outline: bool, strikes: &[(u32, u32)]) -> Face {
        Face {
            family: family.into(),
            style: style.into(),
            version: None,
            outline,
            monospaced: true,
            scripts: vec!["Latin"],
            groups: Vec::new(),
            strikes: strikes
                .iter()
                .map(|&(width, height)| Strike { width, height })
                .collect(),
            codepoints: 95,
            coverage: vec![(0x20, 0x7e)],
            coverage_cut: false,
        }
    }

    fn found(path: &str, place: Place) -> Found {
        Found {
            path: path.into(),
            folder: crate::discover::Folder {
                path: "/".into(),
                place,
            },
            trust: Ok(()),
            state: State::Inactive,
            duplicate: None,
        }
    }

    fn found_at(path: &str) -> Found {
        found(path, Place::System)
    }

    fn fonts(files: &[(&str, Option<Face>)]) -> Fonts {
        let mut out = Fonts::new(files.iter().map(|(p, _)| found(p, Place::System)).collect());
        out.describe(
            files
                .iter()
                .filter_map(|(p, f)| {
                    f.clone().map(|f| {
                        (
                            PathBuf::from(p),
                            Described::Font(Description {
                                format: Format::OpenType,
                                faces: vec![f],
                            }),
                        )
                    })
                })
                .collect(),
        );
        out
    }

    fn frit(text: &str) -> Frit {
        Frit::parse(text.as_bytes(), Path::new("t"), "t").unwrap_or_else(|e| panic!("{e}"))
    }

    // An outline text font supplies any size; a bitmap source supplies the
    // strikes of its own file and of its `strike=` files, never those of
    // other files of the same design; a family no file has, or one not
    // described yet, says why
    #[test]
    fn a_look_s_sizes_come_from_its_text_font() {
        let found = fonts(&[
            ("/f/mono.ttf", Some(face("Mono Sans", "Regular", true, &[]))),
            (
                "/f/ter-u12n.otb",
                Some(face("Terminus", "Regular", false, &[(6, 12)])),
            ),
            (
                "/f/ter-u16n.otb",
                Some(face("Terminus", "Regular", false, &[(8, 16)])),
            ),
            (
                "/f/ter-u16b.otb",
                Some(face("Terminus", "Bold", false, &[(8, 17)])),
            ),
            (
                "/f/ter-u14n.bdf",
                Some(face("Terminus", "Regular", false, &[(7, 14)])),
            ),
            ("/f/later.ttf", None),
        ]);
        let strikes = |cells: &[(u32, u32)]| Supplied::Strikes {
            family: "Terminus".into(),
            strikes: cells
                .iter()
                .map(|&(width, height)| Strike { width, height })
                .collect(),
        };
        assert_eq!(
            found.supplied(&frit("source=font:monosans\n")),
            Supplied::Any {
                family: "Mono Sans".into()
            }
        );
        // the console draws this one file, so the 12-pixel strike of
        // another Terminus file is no size of this look
        assert_eq!(
            found.supplied(&frit("source=outline:/f/ter-u16n.otb\n")),
            strikes(&[(8, 16)])
        );
        assert_eq!(
            found.supplied(&frit(
                "source=outline:/f/ter-u16n.otb\nstrike=bdf:/f/ter-u14n.bdf\n"
            )),
            strikes(&[(7, 14), (8, 16)])
        );
        // a family draws from its first file with a Regular face
        assert_eq!(
            found.supplied(&frit("source=font:Terminus\n")),
            strikes(&[(6, 12)])
        );
        assert_eq!(
            found.supplied(&frit("source=font:Nowhere\n")),
            Supplied::Unknown(Unknown::Describing)
        );
        let all = {
            let mut f = found.clone();
            f.describe(vec![(
                "/f/later.ttf".into(),
                Described::NotAFont("no".into()),
            )]);
            f
        };
        assert_eq!(
            all.supplied(&frit("source=font:Nowhere\n")),
            Supplied::Unknown(Unknown::NotFound("font:Nowhere".into()))
        );
        assert_eq!(
            all.supplied(&frit("source=builtin:unifont\n")),
            Supplied::Unknown(Unknown::BuiltIn)
        );
    }

    // At 96 DPI, 9 points draw 12 pixels; 11 points draw 15 pixels, a cell
    // that draws the 14-pixel strike; 12 points draw 16 and 15 draw 20
    #[test]
    fn each_strike_takes_the_smallest_point_size_that_draws_it() {
        let strikes: Vec<Strike> = [(6, 12), (7, 14), (8, 16), (10, 20)]
            .iter()
            .map(|&(width, height)| Strike { width, height })
            .collect();
        let points: Vec<u32> = strike_points(&strikes, 96)
            .iter()
            .map(|(p, _)| *p)
            .collect();
        assert_eq!(points, [9, 11, 12, 15]);
        let at_72: Vec<u32> = strike_points(&strikes, 72)
            .iter()
            .map(|(p, _)| *p)
            .collect();
        assert_eq!(at_72, [12, 14, 16, 20]);
    }

    // At 192 DPI no whole point size draws the 7×14 strike before the
    // 16-pixel one, so the size note names only the strikes the card
    // offers a size for
    #[test]
    fn the_size_note_names_only_the_strikes_a_size_draws() {
        let strikes: Vec<Strike> = [(7, 14), (8, 16)]
            .iter()
            .map(|&(width, height)| Strike { width, height })
            .collect();
        let supplied = Supplied::Strikes {
            family: "Terminus".into(),
            strikes: strikes.clone(),
        };
        let offered = strike_points(&strikes, 192);
        assert_eq!(offered.len(), 1);
        let note = super::super::words::sizes_note(&supplied, &offered, false);
        assert!(note.ends_with("bitmap sizes: 8×16."), "{note}");
        let none = super::super::words::sizes_note(&supplied, &[], false);
        assert!(none.contains("offers only Automatic"), "{none}");
    }

    // A file asked for once is not asked for again, answered or not, so a
    // scan after an install asks only for the new file
    #[test]
    fn each_file_is_asked_for_once() {
        let mut found = fonts(&[("/f/a.ttf", None), ("/f/b.ttf", None)]);
        assert_eq!(
            found.ask(),
            [PathBuf::from("/f/a.ttf"), PathBuf::from("/f/b.ttf")]
        );
        assert!(found.ask().is_empty());
        let mut files: Vec<Found> = ["/f/a.ttf", "/f/new.ttf", "/f/b.ttf"]
            .iter()
            .map(|p| found_at(p))
            .collect();
        found.rescanned(std::mem::take(&mut files));
        assert_eq!(found.ask(), [PathBuf::from("/f/new.ttf")]);
        assert_eq!(found.waiting().len(), 3);
    }

    // Only All keeps a font not described yet; a kind, a group or a
    // script keeps a font when any of its faces passes
    #[test]
    fn a_filter_keeps_what_its_faces_pass() {
        let d = Described::Font(Description {
            format: Format::OpenType,
            faces: vec![face("Mono", "Regular", true, &[])],
        });
        assert!(Filter::All.keeps(None));
        assert!(!Filter::Outline.keeps(None));
        assert!(Filter::Outline.keeps(Some(&d)));
        assert!(!Filter::Bitmap.keeps(Some(&d)));
        assert!(Filter::Script("Latin").keeps(Some(&d)));
        assert!(!Filter::Script("Greek").keeps(Some(&d)));
        assert!(!Filter::Group(Group::NerdFonts).keeps(Some(&d)));
        assert_eq!(
            Filter::choices().len(),
            5 + Group::ALL.len() + SCRIPTS.len()
        );
    }
}
