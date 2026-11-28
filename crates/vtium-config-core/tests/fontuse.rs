//! The fonts a configuration uses but has not activated: each of the five
//! states found on a test machine's folders, with a package's list, a `-`
//! line, root's file that turns a list off, a font on removable storage
//! and a listed file that is not installed; and Activate missing, one
//! change and one undo step that leaves the fonts turned off on purpose
//! off and counts them apart.
//!
//! References:
//! - ADR-1205 D4, "Fonts a configuration uses but has not activated": the function, the five states and Activate missing.
//! - ADR-1403 D11, "Fonts the configuration uses but has not activated": the marks, the action's count and its message.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use vtium_config_core::batch::{Folders, Start, View};
use vtium_config_core::discover::{Described, Place};
use vtium_config_core::document::{
    Base, Content, Document, Documents, InMemory, Installed as Sets,
};
use vtium_config_core::fontuse::{self, Machine, Off, Scope, State, Used};
use vtium_config_core::found::fonts::{FontFile, Fonts};
use vtium_config_core::found::{Kind, Origin, Resource};
use vtium_font_helper::describe::{Description, Face, Format};

/// A fresh, empty folder for one test's machine, by a path with no `..`
/// and no link in it, as the trust rule asks of a list.
fn scratch(name: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("fontuse-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap_or_else(|e| panic!("{e}"));
    root.canonicalize().unwrap_or_else(|e| panic!("{e}"))
}

/// Writes `text` to `path`, making its folders.
fn put(path: &Path, text: &str) {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap_or_else(|e| panic!("{e}"));
    }
    std::fs::write(path, text).unwrap_or_else(|e| panic!("{e}"));
}

/// One Regular face of `family`, as the font helper describes it.
fn face(family: &str) -> Face {
    Face {
        family: family.to_owned(),
        style: "Regular".to_owned(),
        version: None,
        outline: true,
        monospaced: true,
        scripts: vec!["Latin"],
        groups: Vec::new(),
        strikes: Vec::new(),
        codepoints: 95,
        coverage: vec![(0x20, 0x7e)],
        coverage_cut: false,
    }
}

/// The test machine under `root`, as its folders and the scan give it.
///
/// - batch.conf lists `fonts/active.ttf` and turns `fonts/off.ttf` off
///   with a `-` line.
/// - The package's list vtium-fonts names `fonts/off.ttf`,
///   `fonts/listed.ttf`, which is installed, and `fonts/cjk.ttc`, which
///   is not.
/// - The package's list other-fonts names `fonts/masked.ttf`, and root's
///   empty file of that name turns the list off.
/// - `fonts/stick.ttf` sits on removable storage, so the scan marks it as
///   failing the trust rule.
/// - `fonts/plain.ttf` and two faces of JetBrains Mono are installed and
///   trusted, and nothing activates them.
/// - The person's own folder holds `Mine.ttf`.
/// - kbd's folder holds the console font `lat1-16`.
struct Machine0 {
    root: PathBuf,
    fonts: Fonts,
    view: View,
    kbd: Vec<PathBuf>,
}

impl Machine0 {
    fn new(name: &str) -> Machine0 {
        let root = scratch(name);
        let r = root.display();
        for font in [
            "active.ttf",
            "plain.ttf",
            "off.ttf",
            "masked.ttf",
            "stick.ttf",
            "listed.ttf",
            "JetBrainsMono-Regular.ttf",
            "JetBrainsMono-Bold.ttf",
        ] {
            put(&root.join("fonts").join(font), "a font\n");
        }
        put(&root.join("home/fonts/Mine.ttf"), "a font\n");
        put(&root.join("kbd/consolefonts/lat1-16.psfu.gz"), "a font\n");
        put(
            &root.join("package-lists/vtium-fonts.conf"),
            &format!("# vtium-fonts\n{r}/fonts/off.ttf\n{r}/fonts/listed.ttf\n{r}/fonts/cjk.ttc\n"),
        );
        put(
            &root.join("package-lists/other-fonts.conf"),
            &format!("{r}/fonts/masked.ttf\n"),
        );
        put(&root.join("batch.d/other-fonts.conf"), "");
        put(
            &root.join("frits/layers/fallback.vtlayer"),
            &format!("role=fallback\nsource=outline:{r}/fonts/plain.ttf\n"),
        );
        let conf = format!("{r}/fonts/active.ttf\n-{r}/fonts/off.ttf\n");
        let euid = rustix::process::geteuid().as_raw();
        let start = Start {
            fresh: vtium_settings::batch::Batch::default(),
            folders: Folders {
                package: root.join("package-lists"),
                admin: root.join("batch.d"),
            },
            trust: vtium_settings::Trust::running_as(euid),
        };
        let view = View::new(Some(conf.as_bytes()), &start);
        let file = |path: PathBuf, place: Place, trust: Result<(), String>, family: &str| {
            let described = Described::Font(Description {
                format: Format::OpenType,
                faces: vec![face(family)],
            });
            FontFile {
                resource: Resource {
                    own: place == Place::User,
                    path,
                    kind: Kind::Font,
                    trust,
                    hidden: false,
                    origin: Origin::Nothing,
                },
                place,
                duplicate: None,
                described: Some(described),
                asked: true,
            }
        };
        let f = |n: &str| root.join("fonts").join(n);
        let stick = f("stick.ttf");
        let removable = Err(format!(
            "{}: vtium treats this file as missing, because the filesystem at /media/stick is on removable storage, the disk /dev/sdb.",
            stick.display()
        ));
        let mine = root.join("home/fonts/Mine.ttf");
        let own = Err(format!(
            "{}: the file belongs to user 1000, and only root may own it.",
            mine.display()
        ));
        let fonts = Fonts::new(vec![
            file(f("active.ttf"), Place::System, Ok(()), "Active"),
            file(f("plain.ttf"), Place::System, Ok(()), "Plain"),
            file(f("off.ttf"), Place::System, Ok(()), "Off"),
            file(f("masked.ttf"), Place::System, Ok(()), "Masked"),
            file(stick, Place::FontPaths, removable, "Stick"),
            file(f("listed.ttf"), Place::System, Ok(()), "Listed"),
            file(
                f("JetBrainsMono-Regular.ttf"),
                Place::System,
                Ok(()),
                "JetBrains Mono",
            ),
            file(
                f("JetBrainsMono-Bold.ttf"),
                Place::System,
                Ok(()),
                "JetBrains Mono",
            ),
            file(mine, Place::User, own, "Mine"),
        ]);
        Machine0 {
            kbd: vec![root.join("kbd")],
            root,
            fonts,
            view,
        }
    }

    fn path(&self, p: &str) -> PathBuf {
        self.root.join(p)
    }

    /// The machine as the walk reads it. A file the scan did not find,
    /// here only kbd's font, passes the trust rule.
    fn walk(&self, document: &Document) -> Used {
        let judge = |_: &Path| Ok(());
        let layers = self.root.join("frits/layers");
        let machine = Machine {
            fonts: &self.fonts,
            view: &self.view,
            layers: Some(&layers),
            kbd_dirs: &self.kbd,
            judge: &judge,
        };
        Used::of(document, &machine)
    }

    /// The open configuration: the top level draws the font set desk,
    /// the look big the font set panel, and the look small the top
    /// level's set.
    fn document(&self) -> Document {
        let r = self.root.display();
        let text = "font-name = desk\nversion = 1\n[look big]\nfont-name = panel\n[look small]\n";
        let sets: BTreeMap<String, Vec<u8>> = [
            (
                "desk",
                format!(
                    "source=outline:{r}/fonts/active.ttf\nsource=font:JetBrains Mono\nlayer=fallback\n"
                ),
            ),
            (
                "panel",
                format!(
                    "source=outline:{r}/fonts/off.ttf\nsource=outline:{r}/fonts/masked.ttf\nsource=outline:{r}/fonts/stick.ttf\nsource=outline:{r}/fonts/cjk.ttc\nsource=outline:{r}/fonts/listed.ttf\nsource=font:Mine\nsource=kbd:lat1-16\nsource=font:Symbols Nerd Font Mono\nsource=builtin:unifont\n"
                ),
            ),
        ]
        .into_iter()
        .map(|(n, t)| (n.to_owned(), t.into_bytes()))
        .collect();
        let content = Content {
            text: text.to_owned(),
            base: Base::of(Some(text)),
            ..Content::default()
        };
        Document::new(
            "padnoir",
            content,
            Sets {
                text: Some(text.to_owned()),
                frits: sets,
            },
        )
    }
}

impl Drop for Machine0 {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Each font's name and state, in the walk's order.
fn states(used: &Used) -> Vec<(String, State)> {
    used.fonts
        .iter()
        .map(|f| (f.name(), f.state.clone()))
        .collect()
}

// The walk finds every font file each font set would draw, the layers'
// too, each once, and judges each in one of the five states.
#[test]
fn each_font_the_configuration_uses_is_in_one_of_five_states() {
    let m = Machine0::new("states");
    let used = m.walk(&m.document());
    let stick = m.path("fonts/stick.ttf");
    let fails = |path: &Path, why: &str, own| State::FailsTrust {
        check: format!("{}: {why}", path.display()),
        own,
    };
    assert_eq!(
        states(&used),
        [
            ("active.ttf".to_owned(), State::Active),
            ("JetBrainsMono-Regular.ttf".to_owned(), State::NotActive),
            ("JetBrainsMono-Bold.ttf".to_owned(), State::NotActive),
            ("plain.ttf".to_owned(), State::NotActive),
            ("off.ttf".to_owned(), State::TurnedOff(Off::BatchFile)),
            (
                "masked.ttf".to_owned(),
                State::TurnedOff(Off::List(m.path("batch.d/other-fonts.conf")))
            ),
            (
                "stick.ttf".to_owned(),
                fails(
                    &stick,
                    "vtium treats this file as missing, because the filesystem at /media/stick is on removable storage, the disk /dev/sdb.",
                    false
                )
            ),
            ("cjk.ttc".to_owned(), State::Missing),
            ("listed.ttf".to_owned(), State::Active),
            (
                "Mine.ttf".to_owned(),
                fails(
                    &m.path("home/fonts/Mine.ttf"),
                    "the file belongs to user 1000, and only root may own it.",
                    true
                )
            ),
            ("lat1-16.psfu.gz".to_owned(), State::NotActive),
            ("font:Symbols Nerd Font Mono".to_owned(), State::Missing),
        ]
    );
    // the font from the set's layer names its layer, and the top level's
    // set has two users: the top level and the look that takes it
    let plain = used
        .fonts
        .iter()
        .find(|f| f.name() == "plain.ttf")
        .expect("the layer's font");
    let looks: Vec<(Option<&str>, Option<&str>)> = plain
        .users
        .iter()
        .map(|u| (u.look.as_deref(), u.layer.as_deref()))
        .collect();
    assert_eq!(
        looks,
        [(None, Some("fallback")), (Some("small"), Some("fallback"))]
    );
    // a listed file that is not installed is missing, as not installed
    assert_eq!(
        State::Missing.reason(),
        Some(fontuse::NOT_INSTALLED),
        "the reason a missing font gives"
    );
    let counts = used.counts();
    assert_eq!(
        (
            counts.not_active,
            counts.turned_off,
            counts.fails_trust,
            counts.missing
        ),
        (4, 2, 2, 2)
    );
    assert!(!counts.all_active());
    assert_eq!(used.of_look(Some("big")).counts().not_active, 1);
    assert!(used.of_look(Some("nowhere")).counts().all_active());
}

// The marks read the same without color: the words carry the meaning.
#[test]
fn each_state_shows_its_mark_in_words() {
    let words: Vec<String> = [
        State::Active,
        State::NotActive,
        State::TurnedOff(Off::BatchFile),
        State::FailsTrust {
            check: String::new(),
            own: false,
        },
        State::FailsTrust {
            check: String::new(),
            own: true,
        },
        State::Missing,
    ]
    .iter()
    .map(State::words)
    .collect();
    assert_eq!(
        words,
        [
            "● active",
            "○ not active",
            "○ turned off on purpose",
            "○ not active · fails trust",
            "not usable yet",
            "✗ missing",
        ]
    );
}

// Activate missing puts every font that is not active into the
// document's batch as one change: one undo step, which Undo takes back.
// The fonts turned off on purpose stay off, and those that fail the trust
// rule or are missing are never activated, each counted apart.
#[test]
fn activate_missing_is_one_change_that_leaves_fonts_turned_off_on_purpose_off() {
    use vtium_config_core::batch::Change as Batch;
    let m = Machine0::new("activate");
    let document = m.document();
    let desk = InMemory::new().with_draft("padnoir", document.content().clone());
    let mut documents = Documents::new(document, Box::new(desk));
    let used = m.walk(documents.document());
    let done = fontuse::activate(&mut documents, &used, Scope::All).expect("the change is made");
    let activated = [
        m.path("fonts/JetBrainsMono-Regular.ttf"),
        m.path("fonts/JetBrainsMono-Bold.ttf"),
        m.path("fonts/plain.ttf"),
        m.path("kbd/consolefonts/lat1-16.psfu.gz"),
    ];
    assert_eq!(done.activated, activated);
    assert_eq!(done.turned_off, 2);
    let cannot: Vec<&str> = done.cannot.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        cannot,
        [
            "stick.ttf",
            "cjk.ttc",
            "Mine.ttf",
            "font:Symbols Nerd Font Mono"
        ]
    );
    // one step, holding each activation, and nothing turned off turned on
    assert_eq!(documents.history().steps().len(), 1);
    let active: Vec<(PathBuf, Batch)> = documents
        .document()
        .content()
        .active
        .iter()
        .map(|(p, c)| (p.to_path_buf(), c))
        .collect();
    assert_eq!(
        active,
        activated
            .iter()
            .map(|p| (p.clone(), Batch::Activate))
            .collect::<Vec<_>>()
    );
    let after = m.walk(documents.document());
    let counts = after.counts();
    assert_eq!((counts.not_active, counts.turned_off), (0, 2));
    for off in ["off.ttf", "masked.ttf"] {
        let font = after.fonts.iter().find(|f| f.name() == off).expect(off);
        assert!(matches!(font.state, State::TurnedOff(_)), "{off}");
    }
    // the message counts each kind apart, in one line at 80 columns, and
    // F1 says why each one that cannot be activated cannot
    let message = done.message();
    assert_eq!(
        message.line,
        "Activated 4 fonts. 2 stay off on purpose, and 4 cannot be activated ([F1])."
    );
    let more = message.more.expect("the reasons");
    assert!(more.contains("cjk.ttc: not installed"), "{more}");
    assert!(more.contains("is on removable storage"), "{more}");
    // Undo takes the whole change back in one step
    assert!(documents.undo().expect("the undo is written").is_some());
    assert!(documents.document().content().active.is_empty());
    assert_eq!(m.walk(documents.document()).counts().not_active, 4);
    // with nothing to activate, nothing changes and no step is recorded
    documents.redo().expect("the redo is written");
    let again = m.walk(documents.document());
    let none = fontuse::activate(&mut documents, &again, Scope::All).expect("no change");
    assert!(none.activated.is_empty());
    assert_eq!(documents.history().steps().len(), 1);
    // a look's own action activates that look's fonts only
    documents.undo().expect("the undo is written");
    let look = fontuse::activate(&mut documents, &used, Scope::Look(Some("big")))
        .expect("the look's change");
    assert_eq!(look.activated, [m.path("kbd/consolefonts/lat1-16.psfu.gz")]);
}

// The message is one line at 80 columns whatever it counts, and each kind
// of font is counted apart.
#[test]
fn the_message_of_activate_missing_fits_one_line() {
    use fontuse::Activated;
    let line = |n: usize, off: usize, cannot: usize| {
        Activated {
            activated: vec![PathBuf::from("/f"); n],
            turned_off: off,
            cannot: vec![("f".to_owned(), "why".to_owned()); cannot],
        }
        .message()
        .line
    };
    assert_eq!(
        line(3, 0, 0),
        "Activated 3 fonts the configuration uses. [Ctrl+Z] undoes it."
    );
    assert_eq!(
        line(1, 1, 0),
        "Activated 1 font the configuration uses. 1 turned off on purpose stays off."
    );
    assert_eq!(
        line(3, 0, 2),
        "Activated 3 fonts the configuration uses. 2 more cannot be activated ([F1])."
    );
    assert_eq!(
        line(0, 0, 0),
        "Every font the configuration uses is active."
    );
    for n in [0, 1, 2, 99] {
        for off in [0, 1, 99] {
            for cannot in [0, 1, 99] {
                let text = line(n, off, cannot);
                assert!(text.chars().count() <= 80, "{text}");
            }
        }
    }
}

#[cfg(feature = "fixture")]
mod fixtures {
    use super::*;
    use vtium_config_core::fixture::found::{self as fixture, LARGE_FONTS};

    /// A machine whose batch is the fixtures': nothing in its folders of
    /// lists, and an empty batch file.
    fn empty_view() -> View {
        let start = Start {
            fresh: vtium_settings::batch::Batch::default(),
            folders: Folders {
                package: PathBuf::from("/nonexistent/package-lists"),
                admin: PathBuf::from("/nonexistent/batch.d"),
            },
            trust: vtium_settings::Trust::running_as(0),
        };
        View::new(Some(b""), &start)
    }

    // On the screen map's machine the open configuration uses the font
    // set proof, JetBrains Mono, which the fixture's scan does not hold, so
    // the walk names it missing by its family.
    #[test]
    fn the_screen_map_s_configuration_names_its_missing_family() {
        let shared = vtium_config_core::fixture::padnoir::shared();
        let found = fixture::padnoir();
        let view = empty_view();
        let judge = |_: &Path| Ok(());
        let machine = Machine {
            fonts: &found.fonts,
            view: &view,
            layers: None,
            kbd_dirs: &[],
            judge: &judge,
        };
        let used = Used::of(shared.document(), &machine);
        let named: Vec<(String, State)> = states(&used);
        assert_eq!(named, [("font:JetBrains Mono".to_owned(), State::Missing)]);
        assert_eq!(used.fonts[0].users.len(), 1);
    }

    // On the large machine, a family the configuration names is found
    // among 3,000 font files once per walk, and every face of it is
    // listed.
    #[test]
    fn the_large_machine_s_families_are_found_among_its_fonts() {
        let found = fixture::large();
        assert_eq!(found.fonts.files().len(), LARGE_FONTS);
        let family = found
            .fonts
            .files()
            .iter()
            .find_map(|f| f.family().map(str::to_owned))
            .expect("a described font");
        let faces = found
            .fonts
            .files()
            .iter()
            .filter(|f| f.family() == Some(family.as_str()))
            .count();
        let text = "font-name = big\nversion = 1\n";
        let content = Content {
            text: text.to_owned(),
            ..Content::default()
        };
        let sets = [(
            "big".to_owned(),
            format!("source=font:{family}\n").into_bytes(),
        )]
        .into_iter()
        .collect();
        let document = Document::new(
            "large",
            content,
            Sets {
                text: Some(text.to_owned()),
                frits: sets,
            },
        );
        let view = empty_view();
        let judge = |_: &Path| Ok(());
        let machine = Machine {
            fonts: &found.fonts,
            view: &view,
            layers: None,
            kbd_dirs: &[],
            judge: &judge,
        };
        let used = Used::of(&document, &machine);
        assert_eq!(used.fonts.len(), faces);
        assert!(used.fonts.iter().all(|f| f.path.is_some()));
    }
}
