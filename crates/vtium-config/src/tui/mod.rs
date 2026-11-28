//! The TUI that `vtium config` opens at a terminal: a full-screen program
//! drawn in the terminal's text cells. Its six tabs, in the tab bar's
//! order, are Fonts, Effects, Looks, Flow, Settings and Help. Fonts lists
//! the frits (font set files) and the built-in fonts, and Flow draws the
//! configuration as paths from the displays to the input devices. A change
//! the user highlights is previewed as a picture of the console with the
//! change applied, and `t` tries a font on the live console through OSC 50
//! ([`shot`]). The TUI installs what the user keeps through `apply`.
//!
//! [`model`] is the state machine, [`nav`] where the person is in it,
//! [`tabs`] the Fonts, Effects, Looks, Settings and Help tabs, each with
//! its state and its update, [`view`] its screen, [`flow`] the flow read
//! from the draft and the devices present, [`paths`] that flow as paths,
//! [`parts`] the parts a path is made from, [`flowview`] how the Flow view
//! lays the paths and parts out, [`flowdraw`] how it draws them and
//! [`flowcolor`] their colors, [`canvas`] the rows of styled text a screen
//! is written in and the helpers that fit text to them, [`color`] their
//! colors, [`input`] the decoder of what the terminal sends, [`replies`]
//! the queue of who asked the terminal what, [`words`] the catalog of what
//! the TUI says, and [`port`] the loop and the terminal it runs on.
//! [`gather`] reads what the TUI starts from: the frits as text, never a
//! font file, and vtium.conf, which holds the heads, looks and rules too.
//!
//! References:
//! - ADR-1201 D3: the TUI, the views it opens and the frits it reads as
//!   text.
//! - ADR-1201 D4: the preview as a picture of the console, and the live
//!   trial.
//! - ADR-1201 D7: the TUI as a state machine apart from its terminal.
//! - ADR-1205 D1: the six tabs and their order.
//! - ADR-1403 D7: the Flow view and the paths it draws.

mod console;
pub mod contract;
mod contract_text;
pub mod desk;
pub mod edit;
pub mod effects_found;
#[doc(hidden)]
pub mod fixture;
pub mod flow;
pub mod flowcolor;
pub mod flowdraw;
mod flowedit;
pub mod flowlayout;
pub mod flowview;
pub mod fonts_found;
pub mod layout;
pub mod model;
pub mod nav;
pub mod parts;
pub mod paths;
pub mod port;
pub mod replies;
pub mod resources;
pub mod shot;
pub mod tabs;
pub mod view;
pub mod words;

// The TUI's toolkit, which vtium-config-widgets holds, at the paths it had
// in this module.
pub use vtium_config_widgets::{backend, canvas, color, input, listbox};

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use vtium_font::set::builtin;

use crate::frit::{Frit, RESERVED};
use model::{DpiFrom, Font, FontState, Inputs};

/// The most bytes the TUI reads of vtium.conf.
pub const CONF_MAX: u64 = 1 << 20;

/// Where the TUI reads from and saves to, as the command line names them.
#[derive(Clone, Debug)]
pub struct Paths {
    /// The directory of vtium.conf.
    pub configdir: PathBuf,
    /// The frits' directory, or why there is none.
    pub frits: crate::switch::Frits,
    /// The directory drafts go under.
    pub drafts: PathBuf,
    /// The program that runs `apply` as root: `sudo`, or a test's stand-in.
    pub sudo: OsString,
    /// The console binary `apply` checks configuration files with.
    pub vtium: OsString,
    /// The glyph helper that draws specimens and finds a frit's sources.
    pub helper: crate::specimen::Helper,
    /// sysfs, where the displays' modes and EDIDs are read: `/sys`.
    pub sys: PathBuf,
}

impl Paths {
    /// Where a save installs, as `apply` takes it.
    pub fn targets(&self) -> crate::apply::Targets {
        crate::apply::Targets {
            configdir: self.configdir.clone(),
            frits: self.frits.clone(),
            vtium: self.vtium.clone(),
        }
    }
}

/// The directory drafts go under: `$XDG_STATE_HOME/vtium/drafts`, else
/// `~/.local/state/vtium/drafts`.
pub fn default_drafts() -> PathBuf {
    let state = std::env::var_os("XDG_STATE_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from(".local/state"));
    state.join("vtium/drafts")
}

/// A configuration file's text, `None` when it does not exist.
pub fn read_conf(path: &Path) -> Result<Option<String>, String> {
    use std::io::Read;
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let mut bytes = Vec::new();
    file.take(CONF_MAX.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > CONF_MAX {
        return Err(format!(
            "{} is too large: the limit is {CONF_MAX} bytes",
            path.display()
        ));
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| format!("{} is not UTF-8 text", path.display()))
}

/// The frits in `dir` and in its category folders, and the built-in fonts,
/// sorted by name, each with its state. A frit is read as text with the
/// layers it names, as the console reads it.
pub fn fonts(dir: &Path) -> Vec<Font> {
    let mut out: Vec<Font> = RESERVED
        .iter()
        .filter(|n| builtin(n).is_some())
        .map(|n| Font {
            name: (*n).to_owned(),
            state: FontState::BuiltIn,
        })
        .collect();
    let layers = dir.join(vtium_font::set::layer::DIR);
    for (name, path) in crate::switch::frit_files(dir) {
        if RESERVED.contains(&name.as_str()) {
            continue;
        }
        let state = match Frit::load_in(&path, &layers) {
            Ok(_) => FontState::Reads,
            Err(e) if e.line == 0 => FontState::Refused(e.why.clone()),
            Err(e) => FontState::Refused(format!("line {}: {}", e.line, e.why)),
        };
        out.push(Font { name, state });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Each frit installed in `dir` or in its category folders, by its name:
/// its bytes, which a new frit of that name replaces and a removal
/// removes. A file past the frit's bound, or not a regular file, is left
/// out, as [`fonts`] leaves it.
pub fn installed(dir: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    use std::io::Read;
    let mut out = std::collections::BTreeMap::new();
    for (name, path) in crate::switch::frit_files(dir) {
        if RESERVED.contains(&name.as_str()) {
            continue;
        }
        let mut bytes = Vec::new();
        let max = u64::try_from(crate::frit::FRIT_BYTES_MAX).unwrap_or(0);
        let read = std::fs::File::open(&path)
            .and_then(|f| f.take(max.saturating_add(1)).read_to_end(&mut bytes));
        if read.is_ok() && bytes.len() <= crate::frit::FRIT_BYTES_MAX {
            out.insert(name, bytes);
        }
    }
    out
}

/// The layers in the `layers/` folder of the frits' directory `dir`, each
/// by its name with the role its `role=` line names, in the order of their
/// names. A layer the layer reader refuses is left out, because a font set
/// cannot use it.
///
/// References:
/// - ADR-1204 D5: the layers' folder, and the roles a layer fills.
pub fn layers(dir: &Path) -> Vec<(String, vtium_font::set::layer::Role)> {
    use vtium_font::set::layer;
    let folder = dir.join(layer::DIR);
    let Ok(entries) = std::fs::read_dir(&folder) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            e.file_name()
                .to_str()?
                .strip_suffix(layer::SUFFIX)
                .map(str::to_owned)
        })
        .filter(|n| layer::valid_name(n))
        .collect();
    names.sort();
    names
        .into_iter()
        .filter_map(|name| {
            let text = crate::frit::read_layer(&folder, &name).ok()??;
            let file = crate::frit::layer_file(&folder, &name);
            // the list of layers has no place to show a layer's warnings
            let (role, _) = Frit::parse_layer(text.as_bytes(), &file, &name).ok()?;
            Some((name, role))
        })
        .collect()
}

/// The density pictures are drawn at, in ADR-1200 D17's order:
/// vtium.conf's `display-dpi` where it is set within the bounds the
/// console believes; else the density of the first of `displays` whose
/// EDID states its size, at the mode `conf` sets for it; else 96, as the
/// console assumes.
pub fn density(conf: Option<&str>, displays: &[crate::display::Display]) -> (u32, DpiFrom) {
    use vtium_platform::display::{DPI_ASSUMED, DPI_MAX, DPI_MIN};
    let set = conf
        .and_then(|c| vtium_settings::setting(c, "display-dpi"))
        .and_then(|d| d.parse::<u32>().ok())
        .filter(|d| (DPI_MIN..=DPI_MAX).contains(&f64::from(*d)));
    if let Some(dpi) = set {
        return (dpi, DpiFrom::Conf);
    }
    if let Some(dpi) = displays.iter().find_map(|d| d.dpi_at(d.runs_at(conf))) {
        return (dpi, DpiFrom::Edid);
    }
    let assumed = format!("{DPI_ASSUMED}").parse::<u32>().unwrap_or(96);
    (assumed, DpiFrom::Assumed)
}

/// The density of each of `displays` as the console converts points at
/// it: `display-dpi` where `conf` sets it within the bounds the console
/// believes, else the display's own at the mode `conf` sets for it, else
/// 96. With no display connected, the one density [`density`] gives.
///
/// References:
/// - ADR-1200 D17: the density, and the lowest density among the
///   displays that share a grid.
pub fn densities(conf: Option<&str>, displays: &[crate::display::Display]) -> Vec<u32> {
    let (dpi, from) = density(conf, displays);
    if from == DpiFrom::Conf || displays.is_empty() {
        return vec![dpi];
    }
    let assumed = density(None, &[]).0;
    displays
        .iter()
        .map(|d| d.dpi_at(d.runs_at(conf)).unwrap_or(assumed))
        .collect()
}

/// The font vconsole names, read as the console reads it: the kernel
/// command line's `vconsole.font=` over vconsole.conf's `FONT=`.
fn vconsole_font() -> Option<String> {
    let places = vtium_settings::batch::Places::system();
    let text = |path: &Path| read_conf(path).ok().flatten().unwrap_or_default();
    vtium_settings::batch::vconsole(&text(&places.vconsole_conf), &text(&places.cmdline)).font
}

/// What the TUI starts from, read from `paths`, opening at `view`.
pub fn gather(paths: &Paths, view: model::View) -> Result<Inputs, String> {
    let conf_path = paths.configdir.join("vtium.conf");
    let conf = read_conf(&conf_path)?;
    let frits = paths.frits.clone()?;
    let layers = layers(&frits);
    let displays = crate::display::displays(&paths.sys);
    let dpi = density(conf.as_deref(), &displays);
    // the Flow view's colors, from the person's own file, which
    // vtium-config writes the first time
    let (flow_colors, colors_note) = flowcolor::load(&|name| std::env::var(name).ok());
    Ok(Inputs {
        fonts: fonts(&frits),
        conf_path,
        densities: densities(conf.as_deref(), &displays),
        vconsole_font: vconsole_font(),
        conf,
        dpi,
        installed: installed(&frits),
        frits,
        view,
        display: displays.first().map(|d| d.pixels),
        displays,
        // the desk reads the draft's flow once the loop runs
        flow: flow::Flow::default(),
        color: color::enabled(&|name| std::env::var(name).ok()),
        desk: false,
        hotplug: false,
        flow_colors,
        colors_note,
        head: std::env::var("VTIUM_HEAD").ok().filter(|h| !h.is_empty()),
        locale: locale(),
        // the loop reads them once the store is open, since each color
        // map's and shader's mark needs the draft's batch
        effects: effects_found::Effects::default(),
        font_files: fonts_found::Fonts::default(),
        layers,
    })
}

/// The folders the Effects tab lists from and its preview reads color
/// maps from: the system's, and the person's own under
/// `$XDG_CONFIG_HOME`, else `~/.config`.
fn effect_folders() -> effects_found::Folders {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|c| !c.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    effects_found::Folders::system(config)
}

/// What the Effects tab and the Looks tab list among the resources, read
/// from `folders`, the system's color scheme folders and the system's
/// pointer themes: each color map, shader, color scheme file and pointer
/// theme marked as the current draft's batch and the trust rule judge it
/// for an install through `paths`. When vtium-config cannot read the
/// draft's batch, every file shows as not active, and the Effects tab's
/// note says why.
fn effects(
    store: &crate::store::Store,
    paths: &Paths,
    folders: &effects_found::Folders,
) -> effects_found::Effects {
    let session = crate::drafts::Session {
        store: store.clone(),
        paths,
    };
    let active = crate::batch::Resources::open(&session);
    let euid = crate::batch::rule_user(&paths.targets(), &paths.sudo);
    let state = |path: &Path| {
        active
            .as_ref()
            .map_or(crate::batch::State::Inactive, |a| a.state(path))
    };
    let trusted = |path: &Path| crate::batch::trusted(path, euid);
    let mut found = effects_found::find(
        folders,
        &vtium_settings::Trust::running_as(euid),
        &state,
        &trusted,
    );
    found.palette_folders = vtium_settings::palette::FOLDERS
        .iter()
        .map(PathBuf::from)
        .collect();
    found.palettes = effects_found::find_palettes(&found.palette_folders, &state, &trusted, euid);
    let icons: Vec<PathBuf> = effects_found::ICONS.iter().map(PathBuf::from).collect();
    found.pointers = effects_found::find_pointers(
        Path::new(vtium_settings::batch::PACKAGE_POINTERS),
        &icons,
        &state,
        &trusted,
    );
    if let Err(fail) = &active {
        found.note = Some(words::effects_unread(fail.text()));
    }
    found
}

/// The font files the Fonts tab lists, found as `fonts --files` finds
/// them: in the folders that the installed vtium.conf's text `conf` and
/// the person's own folders give, each marked as the current draft's
/// batch and the trust rule judge it for an install through `paths`.
/// When vtium-config cannot read the draft's batch, every file shows as
/// not active, and the second value says why. `Err` says why vtium-config
/// cannot judge the files at all.
///
/// References:
/// - ADR-1205 D3: the folders, and a user's font listed as not usable.
/// - ADR-1205 D4: each font's trust mark and its state in the batch.
pub fn scan_fonts(
    store: &crate::store::Store,
    paths: &Paths,
    conf: Option<&str>,
) -> Result<(Vec<crate::discover::Found>, Option<String>), String> {
    let session = crate::drafts::Session {
        store: store.clone(),
        paths,
    };
    let active = crate::batch::Resources::open(&session);
    let mounts = vtium_settings::mounts::Mounts::system()
        .map_err(|e| words::fonts_unjudged(&e.to_string()))?;
    let euid = crate::batch::rule_user(&paths.targets(), &paths.sudo);
    let found = crate::discover::scan(
        &font_folders(conf),
        &vtium_settings::cullet::Rule::running_as(euid),
        &mounts,
        &|path| {
            active
                .as_ref()
                .map_or(crate::batch::State::Inactive, |a| a.state(path))
        },
    );
    let note = active.err().map(|fail| words::fonts_unread(fail.text()));
    Ok((found, note))
}

/// The folders the Fonts tab's font scan looks in: this machine's, with
/// those that `font-paths` names in `conf`. In a build with the feature
/// `test-hooks`, a test can aim the scan at folders of its own with
/// [`aim_font_scan`].
fn font_folders(conf: Option<&str>) -> crate::discover::Folders {
    #[cfg(feature = "test-hooks")]
    if let Some(aimed) = AIMED_FONT_SCAN.with(|a| a.borrow().clone()) {
        return aimed;
    }
    crate::discover::Folders::of_user(conf)
}

#[cfg(feature = "test-hooks")]
thread_local! {
    /// The folders [`aim_font_scan`] set on this thread.
    static AIMED_FONT_SCAN: std::cell::RefCell<Option<crate::discover::Folders>> =
        const { std::cell::RefCell::new(None) };
}

/// A test hook: aims the Fonts tab's font scan on this thread at
/// `folders`, in place of this machine's font folders, or with `None` back
/// at this machine's. The scan at start and the scan after an install both
/// read it, so a test that drives the TUI on this thread sees the font
/// files it put there. Only a build with the feature `test-hooks` has it,
/// which the workspace's `--all-features` test run turns on and no shipped
/// build does.
#[cfg(feature = "test-hooks")]
pub fn aim_font_scan(folders: Option<crate::discover::Folders>) {
    AIMED_FONT_SCAN.with(|a| *a.borrow_mut() = folders);
}

/// What the Fonts tab lists at start, from [`scan_fonts`], with the
/// tab's note when vtium-config cannot read the draft's batch or the
/// mount table. The font helper describes the files once the loop runs.
fn font_files(
    store: &crate::store::Store,
    paths: &Paths,
    conf: Option<&str>,
) -> fonts_found::Fonts {
    let (mut fonts, note) = match scan_fonts(store, paths, conf) {
        Ok((found, note)) => (fonts_found::Fonts::new(found), note),
        Err(why) => (fonts_found::Fonts::default(), Some(why)),
    };
    fonts.note = note;
    fonts
}

/// The name the specimen's frit takes: a frit's name, never a built-in
/// set's.
const SPECIMEN_FRIT: &str = "vtium-config-specimen";

/// The frit `font` names, as the helper takes it: a frit that draws the
/// font file at that path alone, for the Fonts tab's preview; the frit in
/// `frits`; or a frit of the one built-in font of that name. The helper
/// refuses a frit named for a built-in set, so a built-in font's frit and
/// a font file's take a name of their own.
pub fn frit_text(frits: &Path, font: &str) -> Result<crate::serve::FritText, String> {
    // a font file the Fonts tab previews, drawn alone
    if fonts_found::is_file_name(font) {
        let source = fonts_found::source_of(Path::new(font))
            .ok_or_else(|| words::font_file_unpreviewable(&crate::onscreen::text(font)))?;
        return Ok(crate::serve::FritText {
            name: SPECIMEN_FRIT.to_owned(),
            text: format!("source={source}\n"),
        });
    }
    if RESERVED.contains(&font) {
        return Ok(crate::serve::FritText {
            name: SPECIMEN_FRIT.to_owned(),
            text: format!("source=builtin:{font}\n"),
        });
    }
    let path = crate::switch::frit_file(frits, font)
        .ok_or_else(|| format!("there is no font set {font}"))?;
    let text = Frit::load_in(&path, &frits.join(vtium_font::set::layer::DIR))
        .map_err(|e| e.to_string())?
        .text;
    Ok(crate::serve::FritText {
        name: font.to_owned(),
        text,
    })
}

/// The locale the specimen's line and the built-in font follow: `LC_ALL`,
/// else `LC_CTYPE`, else `LANG`, as the console reads it.
pub fn locale() -> String {
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .unwrap_or_default()
}

/// The TUI's painter: a picture of the console in a font, its glyphs in
/// `locale` drawn by `helper` from the frit in `frits` or the built-in
/// font of that name (ADR-1201 D4).
pub fn painter(frits: PathBuf, helper: crate::specimen::Helper, locale: String) -> port::Painter {
    std::sync::Arc::new(move |shot| {
        let frit = frit_text(&frits, &shot.font)?;
        crate::specimen::screen(&helper, &frit, shot, &locale)
    })
}

/// The TUI's filterer: draws a picture of the console through an effect,
/// as the console's CPU path draws a frame. The vtlook is read under the
/// trust rule of the user `euid`, whose files an install trusts, and its
/// `.cube` files from `looks` through the gate every resource passes.
/// `looks` is the folder the Effects tab lists color maps from, so the
/// list and the preview agree on where they live.
///
/// References:
/// - ADR-1205 D4: the gate, the batch and the trust rule.
/// - ADR-1205 D8: the preview leaves the live console alone.
pub fn filterer(looks: PathBuf, euid: u32) -> port::Filterer {
    std::sync::Arc::new(move |picture, filtered| {
        crate::specimen::through(
            picture,
            filtered,
            &looks,
            &vtium_settings::Trust::running_as(euid),
            vtium_settings::cullet::Rule::running_as(euid),
        )
    })
}

/// The TUI's detailer: a font's detailed specimen in `locale`, drawn by
/// `helper` from the frit in `frits` or the built-in font of that name,
/// as `preview` draws it (ADR-1201 D4, D9).
pub fn detailer(frits: PathBuf, helper: crate::specimen::Helper, locale: String) -> port::Detailer {
    std::sync::Arc::new(move |font, height, max, display| {
        let frit = frit_text(&frits, font)?;
        let sample = crate::specimen::detail(&helper, &frit, height, &locale)?;
        Ok((sample.detailed(max)?, sample.summary(Some(display))))
    })
}

/// The TUI's measurer: a font's cell at a pixel height, as `helper` gives
/// it, from the frit in `frits` or the built-in font of that name.
pub fn measurer(frits: PathBuf, helper: crate::specimen::Helper) -> port::Measurer {
    std::sync::Arc::new(move |font, height| {
        let frit = frit_text(&frits, font)?;
        crate::specimen::own_cell(&helper, &frit, height)
    })
}

/// The TUI's summarizer: what a font's specimen in `locale` gives at a
/// height, on a display of `display` pixels, for a terminal that draws no
/// pictures.
pub fn summarizer(
    frits: PathBuf,
    helper: crate::specimen::Helper,
    locale: String,
    display: Option<(u32, u32)>,
) -> port::Summarizer {
    std::sync::Arc::new(move |font, height| {
        let frit = frit_text(&frits, font)?;
        crate::specimen::sample(&helper, &frit, height, &locale).map(|s| s.summary(display))
    })
}

/// The sources each frit of `names` in `frits` lacks or refuses, or why it
/// draws nothing, asked of one helper under one wait.
pub fn find_sources(
    frits: &Path,
    helper: &crate::specimen::Helper,
    active: Option<&crate::serve::ActiveAsk>,
    names: &[String],
) -> Vec<(String, model::SourcesFound)> {
    let texts: Vec<(String, Result<crate::serve::FritText, String>)> = names
        .iter()
        .map(|n| (n.clone(), frit_text(frits, n)))
        .collect();
    let readable: Vec<crate::serve::FritText> = texts
        .iter()
        .filter_map(|(_, t)| t.as_ref().ok().cloned())
        .collect();
    let mut answers = crate::specimen::sources_through(helper, active, readable).into_iter();
    texts
        .into_iter()
        .map(|(name, text)| {
            let found = match text {
                Ok(_) => answers.next().unwrap_or_else(|| {
                    crate::specimen::Sources::Unknown("the font helper did not answer".to_owned())
                }),
                Err(why) => crate::specimen::Sources::DrawsNothing(why, Vec::new()),
            };
            (name, found)
        })
        .collect()
}

/// The TUI's source finder, which runs on a thread of its own while the
/// TUI goes on reading keys. It reads each font set through the active
/// fonts of the installed batch, as the console reads them, so the Looks
/// tab and the live trial know which fonts the console leaves out because
/// they are not active.
///
/// References:
/// - ADR-1201 D3: the font helper finds the sources, never the TUI's own
///   process.
/// - ADR-1205 D4: the console draws only from the active fonts.
pub fn finder(frits: PathBuf, helper: crate::specimen::Helper) -> port::Finder {
    finder_through(frits, helper, installed_active())
}

/// [`finder`], reading each font set through `active`.
pub fn finder_through(
    frits: PathBuf,
    helper: crate::specimen::Helper,
    active: crate::serve::ActiveAsk,
) -> port::Finder {
    Box::new(move |names| find_sources(&frits, &helper, Some(&active), &names))
}

/// The active fonts of the batch the console reads: the installed batch
/// file, read as root reads it, or the list of a fresh install where
/// there is none.
///
/// References:
/// - ADR-1205 D4: one batch, which the console reads, with the list of a
///   fresh install in its place.
fn installed_active() -> crate::serve::ActiveAsk {
    let places = vtium_settings::batch::Places::system();
    let read = vtium_settings::batch::Batch::system(&places, &vtium_settings::Trust::running_as(0));
    console_active(&read.batch)
}

/// The active-fonts request that the console sends its font helper for
/// `batch`. The system console runs as root, so it names account 0, and
/// only root's files pass the trust rule. vtium-config's helper runs as
/// the person and accepts account 0 as well, so a font file the person
/// owns fails here as it fails on the console.
///
/// References:
/// - ADR-1205 D4: a resource passes the trust rule only when root owns it
///   and every folder above it.
pub fn console_active(batch: &vtium_settings::batch::Batch) -> crate::serve::ActiveAsk {
    crate::serve::ActiveAsk::of_batch(0, batch)
}

/// The TUI's describer: what each font file is, as `helper` describes it,
/// on a thread of its own while the TUI goes on reading keys.
///
/// References:
/// - ADR-1205 D3: vtium-config reads a font only through the sandboxed
///   helper.
pub fn describer(helper: crate::specimen::Helper) -> port::Describer {
    std::sync::Arc::new(move |paths, each| crate::discover::describe_each(&helper, paths, each))
}

/// Opens the TUI on the terminal at `view`, the devices present read from
/// `devices`, and runs it until the user leaves; returns the exit status.
pub fn run(
    paths: &Paths,
    devices: crate::flowcli::Devices,
    view: model::View,
    err: &mut dyn std::io::Write,
) -> u8 {
    // the TUI runs as the user from start to end (ADR-1201 D5); main
    // refuses root before this, and so does this
    if rustix::process::geteuid().is_root() {
        let _ = writeln!(
            err,
            "vtium-config: do not run this as root. Run it as yourself; it asks for sudo when it installs."
        );
        return 2;
    }
    let mut inputs = match gather(paths, view) {
        Ok(inputs) => inputs,
        Err(why) => {
            let _ = writeln!(err, "vtium-config: {why}");
            return 1;
        }
    };
    let store = match crate::store::Store::system() {
        Ok(store) => store,
        Err(fail) => {
            let _ = writeln!(err, "vtium-config: {}", fail.text());
            return fail.code();
        }
    };
    let folders = effect_folders();
    inputs.effects = effects(&store, paths, &folders);
    inputs.font_files = font_files(&store, paths, inputs.conf.as_deref());
    let mut desk = desk::Store::new(store, paths.clone(), desk::machine(devices));
    let tools = port::Tools {
        painter: painter(inputs.frits.clone(), paths.helper.clone(), locale()),
        filterer: filterer(
            folders.looks.clone(),
            crate::batch::rule_user(&paths.targets(), &paths.sudo),
        ),
        measurer: measurer(inputs.frits.clone(), paths.helper.clone()),
        detailer: detailer(inputs.frits.clone(), paths.helper.clone(), locale()),
        summarizer: summarizer(
            inputs.frits.clone(),
            paths.helper.clone(),
            locale(),
            Some(
                inputs
                    .displays
                    .first()
                    .map(|d| d.runs_at(inputs.conf.as_deref()))
                    .or(inputs.display)
                    .unwrap_or(shot::ASSUMED_DISPLAY),
            ),
        ),
        finder: Some(finder(inputs.frits.clone(), paths.helper.clone())),
        describer: describer(paths.helper.clone()),
    };
    let mut tty = match port::Tty::open(tools) {
        Ok(tty) => tty,
        Err(e) => {
            let _ = writeln!(err, "vtium-config: cannot use this terminal: {e}");
            return 1;
        }
    };
    inputs.hotplug = tty.watches_hotplug();
    let status = port::drive_with(&mut tty, inputs, Some(&mut desk));
    drop(tty);
    match status {
        Ok(status) => status,
        Err(e) => {
            let _ = writeln!(err, "vtium-config: cannot use this terminal: {e}");
            1
        }
    }
}

#[cfg(all(test, feature = "test-hooks"))]
mod tests {
    use super::*;

    /// A test that aims the font scan at its own folder makes the scan
    /// read that folder, on its own thread only, until it aims the scan
    /// back at this machine's folders.
    #[test]
    fn aim_font_scan_points_the_font_scan_at_a_test_s_folders() {
        let mine = crate::discover::Folders {
            folders: vec![crate::discover::Folder {
                path: std::path::PathBuf::from("/tmp/test-fonts"),
                place: crate::discover::Place::User,
            }],
            packages: std::path::PathBuf::from("/tmp/test-fonts/packages"),
            local: std::path::PathBuf::from("/tmp/test-fonts/local"),
        };
        aim_font_scan(Some(mine.clone()));
        assert_eq!(font_folders(None), mine);
        let other = std::thread::spawn(|| font_folders(None)).join();
        assert_eq!(other.ok(), Some(crate::discover::Folders::of_user(None)));
        aim_font_scan(None);
        assert_eq!(font_folders(None), crate::discover::Folders::of_user(None));
    }
}
