//! The engine that draws a frit's glyphs, where a frit is a font set file:
//! where the helper finds the fonts a frit names, the sources the engine
//! reads once for the helper's life, the sources it leaves out, and what it
//! found as it read them.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use vtium_font::spacing::Fit;
use vtium_font::{Cell, Style};
use vtium_platform::vt::font::ConsoleFont;

use crate::font::Font;
use crate::frit::{Cover, Frit, Hinting, Source};
use crate::frit::{Render, RenderLine};
use crate::made::Made;
use crate::outline::{OutlineRequest, family_key};
use crate::sources::{
    Consoles, FONT_DIRS, Gate, Host, Input, KBD_DIRS, SourceError, resolve_hashed,
};
use vtium_term::ClusterWidth;

use super::wire::FritText;

pub(super) mod draw;
mod metrics;

/// Where the helper finds what a frit names.
#[derive(Clone, Debug)]
pub struct Paths {
    pub kbd_dirs: Vec<PathBuf>,
    pub font_dirs: Vec<PathBuf>,
    pub vconsole_conf: PathBuf,
    pub kernel_cmdline: PathBuf,
    /// The console's locale, in `LANG`'s form, which the console passes
    /// with `--locale` when it starts the helper. It picks the built-in
    /// font that draws the text of a set whose text font is missing, as
    /// the console picks its own ([`vtium_font::set::builtin_name`]).
    pub locale: String,
}

impl Default for Paths {
    /// The system's.
    fn default() -> Paths {
        Paths {
            kbd_dirs: KBD_DIRS.iter().map(PathBuf::from).collect(),
            font_dirs: FONT_DIRS.iter().map(PathBuf::from).collect(),
            vconsole_conf: PathBuf::from("/etc/vconsole.conf"),
            kernel_cmdline: PathBuf::from("/proc/cmdline"),
            locale: "C".to_owned(),
        }
    }
}

/// The kernel's font as the console handed it, for every `console:VT`.
pub(super) struct Handed(pub(super) Option<ConsoleFont>);

impl Consoles for Handed {
    fn font(&self, _vt: u16) -> Result<ConsoleFont, String> {
        self.0
            .clone()
            .ok_or_else(|| "the console read no font from its VT at start".to_owned())
    }
}

/// A source read and parsed once for the helper's life: a bitmap font's
/// glyphs, or an outline font's bytes with the codepoints it maps.
pub(super) enum Loaded {
    Bitmap(Font),
    Outline {
        font: Arc<[u8]>,
        family: String,
        mapped: Vec<u32>,
    },
}

/// A frit's own cell, as its first source sets it (ADR-1200 D17).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnCell {
    /// A bitmap font's own cell, which the console takes at the largest
    /// whole multiple not over the height it would draw at, and a zoom
    /// steps by its height.
    Bitmap(Cell),
    /// An outline font's cell at the height asked, taken as it is; a zoom
    /// asks again at the new height.
    Outline(Cell),
}

/// One step of what a style's glyphs are drawn from (ADR-1200 D21): the
/// sources the frit gave the style `face`, in the frit's order, with the
/// style `synth` drawn over their glyphs (ADR-1302 D4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub face: Style,
    pub synth: Style,
}

pub(super) const REGULAR: Style = Style {
    bold: false,
    italic: false,
};

/// Whether `source` is one of the frit's own regular sources, which may be
/// its text font and set its cell; a layer's source never is.
pub(super) fn own_regular(source: &Source) -> bool {
    source.style == REGULAR && source.layer.is_none()
}

/// The built-in font `face` as a bitmap font with no glyphs: its cell at
/// the scale it draws at, and its regular strike's baseline at that scale.
/// It stands for the built-in font where the built-in font draws the text
/// in place of a missing text font, so that the other sources' glyphs
/// share its baseline and lines.
fn built_in_metrics(face: &vtium_font::set::SetFace) -> Font {
    use vtium_font::Face;
    let cell = face.cell();
    let baseline = face
        .set()
        .strikes()
        .filter(|s| s.style == Style::default() && s.cell.height > 0)
        .find_map(|s| {
            let k = cell.height / s.cell.height;
            (k > 0 && s.cell.height * k == cell.height && s.cell.width * k == cell.width)
                .then_some(s.baseline * k)
        });
    Font {
        cell,
        baseline,
        glyphs: Vec::new(),
        map: BTreeMap::new(),
        format: "builtin".to_owned(),
    }
}
const BOLD: Style = Style {
    bold: true,
    italic: false,
};

/// What the glyphs of `style` are drawn from, in order: first the sources
/// the frit gave that style, drawn as they are; then, for a codepoint none
/// of them covers, the regular sources with the style synthesised over
/// them (ADR-1200 D21). Bold italic with no bold-italic face is the
/// regular glyph emboldened and then slanted, even where the frit names a
/// bold or an italic face (the maintainer, 2026-11-04).
pub fn steps(style: Style) -> Vec<Step> {
    let mut out = vec![Step {
        face: style,
        synth: REGULAR,
    }];
    if style != REGULAR {
        out.push(Step {
            face: REGULAR,
            synth: style,
        });
    }
    out
}

/// How `c` uses a row made taller by line spacing when the frit's set
/// that gives it has no `fit=`. The console places the glyphs it draws
/// itself by the same function ([`Fit::default_for`]), so the helper and
/// the console agree on every character's default, combining marks
/// included: vtium-font's one table of marks decides which characters are
/// marks for both.
///
/// References:
/// - ADR-1302 D13: the default fit of each kind of character.
pub fn default_fit(c: u32) -> Fit {
    Fit::default_for(c)
}

/// Whether `c` is a private-use character: of the Private Use Area,
/// U+E000 to U+F8FF, or of Supplementary Private Use Area-A or -B. Unicode
/// assigns these no meaning, and symbol fonts such as Nerd Fonts and
/// Powerline put their symbols there.
fn private_use(c: u32) -> bool {
    matches!(c, 0xe000..=0xf8ff | 0xf_0000..=0xf_fffd | 0x10_0000..=0x10_fffd)
}

/// A source [`Engine::load`] left out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeftOut {
    /// This machine does not have it; otherwise it has it and refused it.
    pub missing: bool,
    /// Its line in the frit.
    pub line: usize,
    /// Its value as the frit spells it, such as `font:JetBrains Mono`.
    pub spelling: String,
    pub why: String,
}

impl LeftOut {
    /// The line a sources query answers for it.
    pub fn line_text(&self) -> String {
        let kind = if self.missing { "missing" } else { "refused" };
        let clean = |t: &str| t.replace(['\t', '\n'], " ");
        format!(
            "{kind}\t{}\t{}\t{}",
            self.line,
            clean(&self.spelling),
            clean(&self.why)
        )
    }

    /// The source a sources query's line `text` names, `None` for a line
    /// that is not one.
    pub fn parse(text: &str) -> Option<LeftOut> {
        let mut f = text.splitn(4, '\t');
        let missing = match f.next()? {
            "missing" => true,
            "refused" => false,
            _ => return None,
        };
        Some(LeftOut {
            missing,
            line: f.next()?.parse().ok()?,
            spelling: f.next()?.to_owned(),
            why: f.next()?.to_owned(),
        })
    }
}

/// Why an engine draws nothing from a frit, and what reading the frit
/// found.
pub struct DrawsNothing {
    /// Why: the frit does not read, it names no source, or no source of it
    /// is left.
    pub why: String,
    /// What reading the frit's sources found, `None` when its text does not
    /// read.
    pub read: Option<Found>,
}

/// What the engine found as it read a frit's sources, which a frit
/// with no source left keeps for the console and a sources query.
pub struct Found {
    /// The warnings the frit's text and its sources gave, as
    /// [`Engine::warnings`] holds them.
    pub warnings: Vec<String>,
    /// The sources left out, as [`Engine::left_out`] holds them.
    pub left_out: Vec<LeftOut>,
    /// The fonts that differ from what their sources need, as
    /// [`Engine::differs`] holds them.
    pub differs: Vec<crate::needs::Differs>,
}

impl DrawsNothing {
    /// The line a sources query answers first for a frit that reads and
    /// has no source left: the kind `nothing`, line 0, no spelling, and
    /// why. A reader that knows only the `missing` and `refused` lines
    /// skips it, as it skips any kind it does not know.
    pub fn line_text(&self) -> String {
        format!("nothing\t0\t\t{}", self.why.replace(['\t', '\n'], " "))
    }

    /// Why the frit draws nothing, from a sources query's answer `text`:
    /// its `nothing` line's why, `None` when the answer has no such line.
    pub fn parse(text: &str) -> Option<String> {
        text.lines()
            .find_map(|l| l.strip_prefix("nothing\t0\t\t"))
            .map(str::to_owned)
    }
}

/// A frit's sources, ready to draw at any cell.
pub struct Engine {
    name: String,
    pub(super) sources: Vec<(Source, Loaded)>,
    /// Every codepoint some source gives, sorted.
    coverage: Vec<u32>,
    /// The sources that failed as they drew, by their place in `sources`,
    /// and why: left out for the engine's life (ADR-1200 D14).
    pub(super) failed: RefCell<BTreeMap<usize, String>>,
    /// What the frit's text gave as warnings, each naming its line.
    pub(super) warnings: Vec<String>,
    /// The sources [`Engine::load`] left out, each as a sources query
    /// answers it.
    pub(super) left_out: Vec<LeftOut>,
    /// The fonts that differ from what their sources need, each as a
    /// sources query answers it ([`crate::needs`]).
    differs: Vec<crate::needs::Differs>,
    /// Each outline source's baseline at a cell's height, by its place in
    /// `sources` and the height (ADR-1302 D3): found once, from the
    /// source's printable ASCII, so every request draws it on one
    /// baseline whatever the request holds.
    baselines: RefCell<BTreeMap<(usize, u32), u32>>,
    /// How the outline sources' glyphs are hinted: the frit's `hinting=`
    /// (ADR-1200 D1).
    hinting: Hinting,
    /// The cells the console's terminal gives a cluster with a spacing
    /// mark or a virama conjunct (ADR-1006 D8), which the ask being
    /// answered names ([`Engine::set_cluster_width`]).
    cluster_width: std::cell::Cell<ClusterWidth>,
    /// Each bitmap source's smaller sizes by its place in `sources`, in
    /// the order D3 names them: a CPI source's other heights, then the
    /// fonts its `script-source=` lines name (ADR-1202 D3, D12). Its own
    /// strike, which comes first, is not repeated here.
    smaller: Vec<Vec<SmallerFont>>,
    /// Each source's strikes by its place in `sources`, each a
    /// [`Loaded::Bitmap`]: the bitmap files its `strike=` lines name that
    /// this machine has and that read ([`Engine::strike_at`]).
    pub(super) strikes: Vec<Vec<Loaded>>,
    /// The frit's `cell=`, which sets its own cell where it states one.
    cell: Option<Cell>,
    /// Why the frit has no text font to draw from, when it has none: its
    /// text font, its own first regular source, was left out by
    /// [`Engine::load`], or the frit has no source of its own, only
    /// layers. Then the built-in font draws the text in the text font's
    /// place ([`Engine::built_in_at`]), and the frit has no own cell.
    text_left_out: Option<TextOut>,
    /// The console's locale ([`Paths::locale`]), which picks the built-in
    /// font that stands in for a missing text font.
    locale: String,
    /// The frit's `render=` lines, by the line of the source each is for
    /// ([`Engine::draws_as_bitmap`]).
    render: Vec<RenderLine>,
    /// Whether the frit's first source is a bitmap font, which decides how
    /// the outline sources with no `render=` line draw. The engine decides
    /// it once, when [`Engine::own_cell`] picks the source that sets the
    /// cell or at the first draw before that, and keeps it for its life.
    /// A first source that fails later in the session hands the cell's
    /// glyphs to the next source, but the outline sources keep drawing as
    /// they began, so that one font's glyphs never mix 1-bit and
    /// antialiased ones, and each baseline measured for them stays right.
    first_is_bitmap: std::cell::Cell<Option<bool>>,
}

/// Why a frit has no text font to draw from ([`Engine::own_cell`]).
#[derive(Clone, PartialEq, Eq, Debug)]
enum TextOut {
    /// The text font, by its spelling and the frit's line that names it,
    /// is missing or cannot be read.
    LeftOut(String, usize),
    /// The console left the text font out, by its spelling and the frit's
    /// line that names it, because the font made an earlier helper crash,
    /// hang or reach its CPU limit.
    Dropped(String, usize),
    /// The frit has no source of its own, only layers' sources.
    NoneOwn,
}

/// The frit's first regular source as it draws in a cell
/// ([`Engine::first_drawn`]): the text font, or its strike, at its place
/// in `sources`; or the built-in font that stands in for a text font the
/// frit lacks, of which only the cell and the baseline matter.
enum FirstDrawn<'a> {
    Source(usize, &'a Loaded),
    BuiltIn(Loaded),
}

impl FirstDrawn<'_> {
    fn loaded(&self) -> &Loaded {
        match self {
            FirstDrawn::Source(_, loaded) => loaded,
            FirstDrawn::BuiltIn(loaded) => loaded,
        }
    }

    /// Its place in `sources`; `None` for the built-in font.
    fn at(&self) -> Option<usize> {
        match self {
            FirstDrawn::Source(i, _) => Some(*i),
            FirstDrawn::BuiltIn(_) => None,
        }
    }
}

/// A vitrified set of glyphs, `None` when no source gives any, and how the
/// engine made each superscript or subscript glyph in it, by its codepoint
/// ([`Engine::draw_made`]).
pub type MadeSet = (Option<Vec<u8>>, BTreeMap<u32, Made>);

/// A smaller size of a bitmap source (ADR-1202 D3): its font, and for one
/// that `script-source=` names, that line and the font's spelling.
struct SmallerFont {
    font: Font,
    named: Option<(usize, String)>,
}

/// The face of `family` in `font`, a font file or a collection, matched on
/// its family names as fontconfig matches them, or its first face for an
/// empty family.
fn outline_face<'a>(font: &'a [u8], family: &str) -> Result<swash::FontRef<'a>, String> {
    use swash::StringId;
    let fonts = crate::names::swash_fonts(font)?;
    let want = family_key(family);
    fonts
        .fonts()
        .take(crate::names::FACES_MAX as usize)
        .find(|f| {
            want.is_empty() || {
                let strings = f.localized_strings();
                [StringId::TypographicFamily, StringId::Family]
                    .into_iter()
                    .filter_map(|id| strings.find_by_id(id, Some("en")))
                    .any(|s| family_key(&s.to_string()) == want)
            }
        })
        .ok_or_else(|| format!("the font holds no face of family {family}"))
}

/// The version string of the face of `family` in `font` (its first face
/// for an empty family), from its name table's version entry (name ID 5),
/// in English where the font gives one; `None` when the font states none.
/// It reads the name table with read-fonts, as the search for a family
/// does, which checks each offset against the file, so a malformed file
/// has no version rather than a panic.
///
/// References:
/// - ADR-1204 D7: `version=` is matched against the font's version.
pub fn outline_version(font: &[u8], family: &str) -> Option<String> {
    let faces = crate::names::faces(font).ok()?;
    let face = if family.is_empty() {
        faces.first()
    } else {
        crate::names::choose(&faces, family)
    }?;
    face.version.clone()
}

/// The codepoints `font` maps in the face of `family` (its first face for
/// an empty family), as the rasteriser takes them: no control character
/// and no glyph 0.
pub(super) fn outline_coverage(font: &[u8], family: &str) -> Result<Vec<u32>, String> {
    let face = outline_face(font, family)?;
    Ok(crate::describe::mapped(face.charmap(), |_| true)
        .into_iter()
        .map(|(c, _)| c)
        .collect())
}

/// Whether `key` is the key of one piece of a run of ASCII symbols that
/// may draw as a ligature, rather than of a syllable's piece.
fn is_ligature_piece(key: &[u32]) -> bool {
    vtium_font::syllable::piece_of(key)
        .is_some_and(|(text, _)| vtium_font::syllable::is_ligature_run(text))
}

/// Sorted codepoints as ranges, runs joined.
pub(super) fn ranges(cs: &[u32]) -> Vec<Cover> {
    let mut out: Vec<Cover> = Vec::new();
    for &c in cs {
        match out.last_mut() {
            Some((_, b)) if b.checked_add(1) == Some(c) => *b = c,
            _ => out.push((c, c)),
        }
    }
    out
}

/// The line the console logs, once a boot, for a font file that the set
/// `set` names in `source` and that the font helper refuses: the set, the
/// file, the layer the source came from when it came from one, and why
/// vtium treats the file as missing. The font's characters then draw from
/// the next font that has them.
///
/// References:
/// - ADR-1205 D4: a refused file counts as missing, and vtium logs one
///   line for it, naming the file and the check it failed.
/// - ADR-1204 D7: a missing font's characters draw from the next source.
fn refused_line(set: &str, source: &Source, refused: &SourceError) -> String {
    let from = source
        .layer
        .as_ref()
        .map(|layer| format!(" from the layer {layer}"))
        .unwrap_or_default();
    match refused {
        SourceError::Refused { path, because } => format!(
            "{set} needs the font file {}{from}, which vtium treats as missing, because {because} vtium draws the font's characters from the next font that has them.",
            path.display()
        ),
        other => format!("{set}: {other}"),
    }
}

/// The smaller sizes of the bitmap source `source` (ADR-1202 D3, D12),
/// read and parsed as [`crate::sources::smaller`] names them. A font this
/// machine does not have is left out quietly, as a source is. A font that
/// is here and is refused is left out with a warning naming its line in
/// the frit `name`. A `script-source=` font keeps its own Unicode table
/// where it has one, and takes the source's `unimap=` where it has none.
fn smaller_fonts(
    source: &Source,
    host: &Host<'_>,
    frit: &Frit,
    warnings: &mut Vec<String>,
) -> Vec<SmallerFont> {
    let mut fonts = Vec::new();
    for (named, input) in crate::sources::smaller(source, host) {
        let font = input.and_then(|input| {
            bitmap_of(input, "this is an outline font, not a smaller bitmap font")
                .map_err(SourceError::Failed)
        });
        match (font, &named) {
            (Ok(font), _) => fonts.push(SmallerFont { font, named }),
            (Err(SourceError::Failed(e)), Some((line, spelling))) => warnings.push(format!(
                "{}: script-source={spelling}: {e}; the font is left out",
                frit.at(*line)
            )),
            // a font the gate refuses is logged, as a source's is
            (Err(refused @ SourceError::Refused { .. }), _) => {
                warnings.push(refused_line(&frit.name, source, &refused));
            }
            // a font this machine lacks, and a CPI height that does not
            // decode, are left out quietly
            (Err(_), _) => {}
        }
    }
    fonts
}

/// The glyphs of a bitmap font that a source names besides its own: a
/// smaller size or a strike. A font with a Unicode table of its own keeps
/// it, and one with none takes the source's `unimap=`, else kbd's
/// `def.uni`. An outline font is refused with `outline`.
fn bitmap_of(input: Input, outline: &str) -> Result<Font, String> {
    match input {
        Input::Bitmap(font) => Ok(font),
        Input::Bdf(bytes) => crate::bdf::font(&bytes),
        Input::Hex(bytes) => crate::hex::font(&bytes),
        Input::Font(req) => req
            .unimap
            .as_ref()
            .and_then(|_| crate::decode::font(&req.font, None, None).ok())
            .map_or_else(
                || crate::decode::font(&req.font, req.unimap.as_deref(), req.def_uni.as_deref()),
                Ok,
            )
            .map(Font::from_glyphs),
        Input::Outline(_) => Err(outline.to_owned()),
    }
}

/// What [`Engine::read`] gathers about the frit's fonts as it reads them:
/// the warnings the console logs, the fonts it leaves out, and the fonts
/// that differ from what their sources need.
struct Report<'a> {
    warnings: &'a mut Vec<String>,
    left_out: &'a mut Vec<LeftOut>,
    differs: &'a mut Vec<crate::needs::Differs>,
}

/// The strikes that `source`'s `strike=` lines name, read and parsed,
/// each a [`Loaded::Bitmap`]. A strike this machine does not have is left
/// out and reported as a missing font is: a warning that the console logs
/// once a boot, and a missing font in the answer to a sources query, under
/// the strike's own line and spelling. A strike that is here and is
/// refused is left out with a warning that names its line in `frit`. A
/// strike whose file has another SHA-256 than its `sha256=` line gives
/// still draws, and goes to `differs` under its own line and spelling.
///
/// References:
/// - ADR-1204 D2: the `strike=` key under an outline and a bitmap source.
/// - ADR-1204 D7: a missing font is logged and reported, and a file with
///   another hash still draws and is reported.
fn strike_fonts(
    source: &Source,
    host: &Host<'_>,
    frit: &Frit,
    report: &mut Report<'_>,
) -> Vec<Loaded> {
    let mut fonts = Vec::new();
    for strike in &source.strikes {
        let file = Source {
            spelling: strike.spelling.clone(),
            origin: strike.origin.clone(),
            unimap: None,
            cover: Vec::new(),
            cp_size: None,
            script_sources: Vec::new(),
            strikes: Vec::new(),
            style: source.style,
            fits: Vec::new(),
            layer: source.layer.clone(),
            line: strike.line,
            needs: crate::frit::Needs {
                sha256: strike.sha256.clone(),
                ..crate::frit::Needs::default()
            },
        };
        let font = resolve_hashed(&file, host).and_then(|(input, hash)| {
            bitmap_of(input, "this is an outline font, not a bitmap strike")
                .map(|font| (font, hash))
                .map_err(SourceError::Failed)
        });
        let own_line = frit.place(strike.line).own;
        match font {
            Ok((font, hash)) => {
                if let Some(wanted) = &strike.sha256
                    && let Some(found) = hash
                    && &found != wanted
                {
                    report.differs.push(crate::needs::Differs {
                        line: own_line,
                        spelling: strike.spelling.clone(),
                        what: crate::needs::Differ::OtherHash(found),
                    });
                }
                fonts.push(Loaded::Bitmap(font));
            }
            Err(SourceError::Failed(e)) => report.warnings.push(format!(
                "{}: strike={}: {e}. Because of this error, vtium leaves the strike out.",
                frit.at(strike.line),
                strike.spelling
            )),
            Err(SourceError::Missing(why)) => {
                // the console logs this line once a boot
                report
                    .warnings
                    .extend(crate::needs::missing_strike_line(&frit.name, &file, source));
                report.left_out.push(LeftOut {
                    missing: true,
                    line: own_line,
                    spelling: strike.spelling.clone(),
                    why,
                });
            }
            Err(refused @ SourceError::Refused { .. }) => {
                // the console logs this line once a boot
                report
                    .warnings
                    .push(refused_line(&frit.name, &file, &refused));
                report.left_out.push(LeftOut {
                    missing: true,
                    line: own_line,
                    spelling: strike.spelling.clone(),
                    why: refused.to_string(),
                });
            }
        }
    }
    fonts
}

impl Engine {
    /// Places the clusters the engine draws next in the cells that `rule`
    /// gives a cluster with a spacing mark or a virama conjunct, as the
    /// console's terminal gives them (ADR-1006 D8): the ask being answered
    /// names it, and every other drawing takes `double`, the default.
    pub fn set_cluster_width(&self, rule: ClusterWidth) {
        self.cluster_width.set(rule);
    }

    /// Parses `frit` and reads each source it names, as this process's
    /// account; a source missing or unreadable is left out, and the frit
    /// fails only when none is left. A source that is on this machine and
    /// is refused, such as a BDF file the reader refuses, is also a
    /// warning when the frit loads without it, so the console logs it. A
    /// font this machine does not have, or one that lacks a character its
    /// `check=` names, is a warning that names the set, the font and the
    /// version the set asks for. Each font that loads is matched against
    /// its `version=` and `sha256=`, and a difference is kept for the
    /// sources query ([`crate::needs`]). Its messages name the frit by its
    /// name.
    ///
    /// References:
    /// - ADR-1204 D7: a font that fails `check=` is missing, a lower
    ///   version or another hash still draws, and the console logs each
    ///   missing font.
    pub fn load(
        frit: &FritText,
        paths: &Paths,
        console: Option<ConsoleFont>,
    ) -> Result<Engine, String> {
        Engine::load_through(frit, paths, console, &Gate::Open)
    }

    /// [`Engine::load`], its sources reading only the font files `gate`
    /// lets through, as the console's font helper reads them.
    pub fn load_through(
        frit: &FritText,
        paths: &Paths,
        console: Option<ConsoleFont>,
        gate: &Gate,
    ) -> Result<Engine, String> {
        Engine::read(frit, paths, console, (gate, 0)).map_err(|nothing| nothing.why)
    }

    /// [`Engine::load`], which for a frit it draws nothing from keeps what
    /// the read found: the warnings, so the console still logs each
    /// missing font, and the sources left out and the fonts that differ,
    /// so a sources query still names them.
    ///
    /// It leaves out each source whose bit `leave` sets, a bit for each
    /// `source=` line in the order of the frit's text, the first line the
    /// lowest bit, as if the frit had no such line. The console names
    /// those sources when one of them made an earlier helper crash, hang or
    /// reach its CPU limit, and logs that itself, so the read adds no
    /// warning for them.
    ///
    /// References:
    /// - ADR-1204 D7: the console logs each missing font, and vtium-config
    ///   names each one, whether or not the set has a font left.
    /// - ADR-1200 D14: a crash, a fault or a timeout fails one source and
    ///   nothing else.
    pub(super) fn read(
        frit: &FritText,
        paths: &Paths,
        console: Option<ConsoleFont>,
        (gate, leave): (&Gate, u32),
    ) -> Result<Engine, DrawsNothing> {
        let file = Path::new(&frit.name);
        // the console hands the frit with its layers in place, so each
        // message names the file and line a line came from
        let frit = Frit::parse_spliced(
            frit.text.as_bytes(),
            file,
            &frit.name,
            Path::new(vtium_font::set::layer::DIR),
        )
        .map_err(|e| DrawsNothing {
            why: e.to_string(),
            read: None,
        })?;
        let mut warnings: Vec<String> = frit.warnings.iter().map(ToString::to_string).collect();
        let handed = Handed(console);
        let host = Host {
            kbd_dirs: paths.kbd_dirs.clone(),
            font_dirs: paths.font_dirs.clone(),
            vconsole_conf: paths.vconsole_conf.clone(),
            kernel_cmdline: paths.kernel_cmdline.clone(),
            consoles: &handed,
            gate,
        };
        let mut sources = Vec::new();
        let mut smaller = Vec::new();
        let mut strikes = Vec::new();
        let mut why = Vec::new();
        let mut left_out = Vec::new();
        let mut differs = Vec::new();
        for (i, source) in frit.sources.iter().enumerate() {
            if u32::try_from(i)
                .ok()
                .and_then(|i| 1u32.checked_shl(i))
                .is_some_and(|bit| leave & bit != 0)
            {
                continue;
            }
            let own_line = frit.place(source.line).own;
            // the version the font states, read only where the set asks
            // for one
            let mut version = None;
            // the SHA-256 of the bytes this read takes from the file, when
            // the source has a sha256= line
            let mut hash = None;
            let loaded = resolve_hashed(source, &host).and_then(|(input, found)| {
                hash = found;
                let wants = source.needs.version.is_some();
                // a BDF font gives its version as it is read, from one
                // decompression of its bytes
                if wants && !matches!(input, Input::Bdf(_)) {
                    version = crate::needs::version_of(&input);
                }
                match input {
                    Input::Bitmap(font) => Ok(Loaded::Bitmap(font)),
                    Input::Bdf(bytes) => crate::bdf::read(&bytes, wants).map(|(font, stated)| {
                        version = stated;
                        Loaded::Bitmap(font)
                    }),
                    Input::Hex(bytes) => crate::hex::font(&bytes).map(Loaded::Bitmap),
                    Input::Font(req) => crate::decode::font(
                        &req.font,
                        req.unimap.as_deref(),
                        req.def_uni.as_deref(),
                    )
                    .map(|g| Loaded::Bitmap(Font::from_glyphs(g))),
                    Input::Outline(req) => {
                        outline_coverage(&req.font, &req.family).map(|mapped| Loaded::Outline {
                            font: req.font,
                            family: req.family,
                            mapped,
                        })
                    }
                }
                .map_err(SourceError::Failed)
            });
            // a font that lacks a character its check= line names is a
            // different font of a similar name, and is missing
            let mut lacks = None;
            let loaded = loaded.and_then(|l| {
                let lacked = match &l {
                    Loaded::Bitmap(f) => crate::needs::lacking(&source.needs.check, |a, b| {
                        f.map.range(a..=b).count()
                    }),
                    Loaded::Outline { mapped, .. } => {
                        crate::needs::lacking(&source.needs.check, |a, b| {
                            mapped
                                .partition_point(|&c| c <= b)
                                .saturating_sub(mapped.partition_point(|&c| c < a))
                        })
                    }
                };
                match lacked {
                    None => Ok(l),
                    Some(c) => {
                        lacks = Some(c);
                        differs.push(crate::needs::Differs {
                            line: own_line,
                            spelling: source.spelling.clone(),
                            what: crate::needs::Differ::Lacks(c),
                        });
                        Err(SourceError::Missing(format!(
                            "the font of that name on this machine lacks U+{c:04X}, which check= names, so it is not the font the set needs"
                        )))
                    }
                }
            });
            match loaded {
                Ok(l) => {
                    differs.extend(crate::needs::differences(
                        source,
                        (version.as_deref(), hash.as_deref()),
                        own_line,
                    ));
                    strikes.push(strike_fonts(
                        source,
                        &host,
                        &frit,
                        &mut Report {
                            warnings: &mut warnings,
                            left_out: &mut left_out,
                            differs: &mut differs,
                        },
                    ));
                    let small = match l {
                        Loaded::Bitmap(_) => smaller_fonts(source, &host, &frit, &mut warnings),
                        Loaded::Outline { .. } => {
                            if let Some(named) = source.script_sources.first() {
                                warnings.push(format!(
                                    "{}: script-source={} is ignored: the source above it is an outline font, which draws its own superscripts and subscripts at a smaller size",
                                    frit.at(named.line),
                                    named.spelling
                                ));
                            }
                            Vec::new()
                        }
                    };
                    sources.push((source.clone(), l));
                    smaller.push(small);
                }
                Err(refused @ SourceError::Refused { .. }) => {
                    let e = refused.to_string();
                    why.push(format!("source={}: {e}", source.spelling));
                    // the console logs this line once a boot
                    warnings.push(refused_line(&frit.name, source, &refused));
                    left_out.push(LeftOut {
                        missing: true,
                        line: own_line,
                        spelling: source.spelling.clone(),
                        why: e,
                    });
                }
                Err(SourceError::Missing(e)) => {
                    why.push(format!("source={}: {e}", source.spelling));
                    // the console logs this line once a boot
                    warnings.extend(crate::needs::missing_line(&frit.name, source, lacks));
                    left_out.push(LeftOut {
                        missing: true,
                        line: own_line,
                        spelling: source.spelling.clone(),
                        why: e,
                    });
                }
                Err(SourceError::Failed(e)) => {
                    left_out.push(LeftOut {
                        missing: false,
                        line: own_line,
                        spelling: source.spelling.clone(),
                        why: e.clone(),
                    });
                    let line = format!("source={}: {e}", source.spelling);
                    warnings.push(format!(
                        "{}: {line}; the source is left out",
                        frit.at(source.line)
                    ));
                    why.push(line);
                }
            }
        }
        // a font: source that turns out to be a console font draws its
        // pixels fully on or off whatever its render= line says, so the
        // line does nothing, and the warning names the render= line
        for (source, _) in sources
            .iter()
            .filter(|(_, l)| matches!(l, Loaded::Bitmap(_)))
        {
            if let Some(render) = frit.render_for(source.line) {
                warnings.push(format!(
                    "{}: render= is ignored, because {} is a bitmap font, whose characters always have sharp edges",
                    frit.at(render.line),
                    source.spelling
                ));
            }
        }
        // the text font is the frit's own first regular source, never a
        // layer's
        let text_left_out = match frit
            .sources
            .iter()
            .enumerate()
            .find(|(_, s)| s.style == REGULAR && s.layer.is_none())
        {
            None => Some(TextOut::NoneOwn),
            // the console left it out, after it made a helper fail
            Some((i, text))
                if u32::try_from(i)
                    .ok()
                    .and_then(|i| 1u32.checked_shl(i))
                    .is_some_and(|bit| leave & bit != 0) =>
            {
                Some(TextOut::Dropped(text.spelling.clone(), text.line))
            }
            Some((_, text))
                if !sources
                    .iter()
                    .any(|(s, _): &(Source, _)| s.line == text.line) =>
            {
                Some(TextOut::LeftOut(text.spelling.clone(), text.line))
            }
            Some(_) => None,
        };
        if sources.is_empty() {
            return Err(DrawsNothing {
                why: if why.is_empty() {
                    format!("{}: the frit names no source", file.display())
                } else {
                    why.join("; ")
                },
                read: Some(Found {
                    warnings,
                    left_out,
                    differs,
                }),
            });
        }
        // the coverage holds each source's own characters within its
        // cover= lines, and never the characters that only its strikes
        // hold. An outline source draws its own characters at every
        // height. A bitmap source with strikes draws them only where its
        // own font is the tallest that fits the cell, and elsewhere draws
        // the strike's characters, so at those heights the coverage can
        // name a character the frit does not draw.
        let mut coverage: Vec<u32> = sources
            .iter()
            .flat_map(|(source, l)| {
                let cs: Vec<u32> = match l {
                    Loaded::Bitmap(f) => f.map.keys().copied().collect(),
                    Loaded::Outline { mapped, .. } => mapped.clone(),
                };
                cs.into_iter().filter(|&c| source.covers(c))
            })
            .collect();
        coverage.sort_unstable();
        coverage.dedup();
        // `Frit::parse` refuses a name that is not valid
        Ok(Engine {
            cluster_width: std::cell::Cell::default(),
            name: frit.name,
            sources,
            coverage,
            failed: RefCell::default(),
            warnings,
            left_out,
            differs,
            baselines: RefCell::default(),
            hinting: frit.hinting,
            smaller,
            strikes,
            cell: frit.cell,
            text_left_out,
            locale: paths.locale.clone(),
            render: frit.render,
            first_is_bitmap: std::cell::Cell::new(None),
        })
    }

    /// Whether the glyphs of the outline source at `i` in `sources` draw as
    /// a bitmap font's glyphs draw: 1-bit and hinted to the pixel grid both
    /// ways. The source's `render=` line decides where it has one. A source
    /// with none draws as a bitmap when the frit's first source is a bitmap
    /// font, so that the outline glyphs that fill its gaps look like the
    /// rest of it ([`Engine::first_is_bitmap`]).
    ///
    /// References:
    /// - ADR-1200 D1, the `render=` row: the values and the default.
    pub(super) fn draws_as_bitmap(&self, i: usize) -> bool {
        let Some((source, _)) = self.sources.get(i) else {
            return false;
        };
        if let Some(render) = self.render.iter().find(|r| r.source == source.line) {
            return render.value == Render::Bitmap;
        }
        self.first_is_bitmap()
    }

    /// Whether the frit's first source is a bitmap font. The first source
    /// is the frit's own first regular source that the engine can still
    /// draw from, the one that sets the frit's cell ([`Engine::own_cell`]),
    /// and never a layer's. Where the built-in font draws the text in place
    /// of a missing text font, the first source is the built-in font, a
    /// bitmap font. Otherwise the answer is decided at the first call,
    /// unless [`Engine::own_cell`] decided it before, and stays the same for
    /// the engine's life.
    fn first_is_bitmap(&self) -> bool {
        if let Some(known) = self.first_is_bitmap.get() {
            return known;
        }
        if self.text_left_out.is_some() {
            return true;
        }
        let failed = self.failed.borrow();
        let known = self
            .sources
            .iter()
            .enumerate()
            .filter(|(at, (s, _))| own_regular(s) && !failed.contains_key(at))
            .map(|(_, (_, loaded))| loaded)
            .next()
            .is_some_and(|loaded| matches!(loaded, Loaded::Bitmap(_)));
        self.first_is_bitmap.set(Some(known));
        known
    }

    /// The text font's place in `sources`: the frit's own first regular
    /// source. `None` when the built-in font draws the text in its place,
    /// because the text font is missing or the frit has only layers.
    fn text_font(&self) -> Option<usize> {
        if self.text_left_out.is_some() {
            return None;
        }
        self.sources.iter().position(|(s, _)| own_regular(s))
    }

    /// The built-in font that the console draws in a cell `cell` high, when
    /// it draws the text in place of the frit's missing text font; else
    /// `None`. It is the one the console picks, by the cell's height and
    /// the console's locale ([`vtium_font::set::builtin_name`]), which the
    /// console passes the helper as it starts it.
    ///
    /// References:
    /// - ADR-1204 D5 and D7: the built-in font draws the text when the
    ///   set's text font is missing.
    fn built_in_at(&self, cell: Cell) -> Option<vtium_font::set::SetFace> {
        self.text_left_out.as_ref()?;
        let name = vtium_font::set::builtin_name(cell.height, &self.locale);
        vtium_font::set::SetFace::new(vtium_font::set::builtin(name)?, cell.height)
    }

    /// Records that the frit's first source is a bitmap font, or is not,
    /// once [`Engine::own_cell`] has picked the source that sets the cell,
    /// unless a draw before it already decided ([`Engine::first_is_bitmap`]).
    fn decide_first(&self, bitmap: bool) {
        if self.first_is_bitmap.get().is_none() {
            self.first_is_bitmap.set(Some(bitmap));
        }
    }

    /// How the glyphs of the outline source at `i` in `sources` are hinted:
    /// [`Hinting::Bitmap`] where they draw as a bitmap
    /// ([`Engine::draws_as_bitmap`]), else the frit's `hinting=`. Each
    /// request for the source's glyphs carries it, and the source's
    /// baseline and lines are measured with it, so that they fit the
    /// glyphs as they are drawn.
    fn hinting_for(&self, i: usize) -> Hinting {
        match self.draws_as_bitmap(i) {
            true => Hinting::Bitmap,
            false => self.hinting,
        }
    }

    /// `request`, for the outline source at `i` in `sources`, hinted as
    /// that source's glyphs are drawn ([`Engine::hinting_for`]).
    fn rendered(&self, request: OutlineRequest, i: usize) -> OutlineRequest {
        OutlineRequest {
            hinting: self.hinting_for(i),
            ..request
        }
    }

    /// The frit text's warnings and one for each source the helper
    /// refused, each naming its line.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// The sources the engine left out: those this machine does not have,
    /// and those it has and refused.
    pub fn left_out(&self) -> &[LeftOut] {
        &self.left_out
    }

    /// The fonts that differ from what their sources need: one that lacks
    /// a character its `check=` names, which is also left out, one of a
    /// lower version than its `version=`, and a file whose SHA-256 is not
    /// its `sha256=`.
    pub fn differs(&self) -> &[crate::needs::Differs] {
        &self.differs
    }

    /// Every codepoint the frit's sources give, sorted.
    pub fn coverage(&self) -> &[u32] {
        &self.coverage
    }
}
