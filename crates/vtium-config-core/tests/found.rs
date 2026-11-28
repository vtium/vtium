//! The resources found on the machine: each kind found from a machine's
//! folders as the command line's scan finds it, each marked with where its
//! activation comes from, read once at start and never on a key, and the
//! fixtures' sets, the large one built and listed in under a second.

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use vtium_config_core::batch::{Change, Changes};
use vtium_config_core::discover::{self, Described, Folder, Place};
use vtium_config_core::found::effects::{self, Built, Dialect, From};
use vtium_config_core::found::fonts::Filter;
use vtium_config_core::found::{self, Activation, Found, Origin, Places, Scans, pointers};
use vtium_render::filter::Layer;
use vtium_settings::batch::Folder as ListFolder;

/// A fresh, empty folder for one test's machine, by a path with no `..`
/// and no link in it, as the trust rule asks of a resource.
fn scratch(name: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("found-{name}-{}", std::process::id()));
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

/// A machine's folders under `root`: font files in the packages' folder,
/// the administrator's (one the same bytes as a package's), vtium's own
/// and the person's own; vtlooks, color maps and shaders; color schemes;
/// pointer themes; and a batch file with a package's list beside it. The
/// batch file activates DejaVuSansMono.ttf and sepia.cube and turns off
/// cullet-bold.otf, which the package's list names with cullet-mono.otf.
fn machine(root: &Path) -> Places {
    let fonts = root.join("share/fonts");
    let local = root.join("local/fonts");
    let vtium = root.join("share/vtium/fonts");
    let mine = root.join("home/fonts");
    put(&fonts.join("TTF/DejaVuSansMono.ttf"), "dejavu");
    put(&fonts.join("misc/ter-u16n.otb"), "terminus");
    put(&fonts.join("misc/notes.txt"), "not a font");
    put(&local.join("DejaVuSansMono.ttf"), "dejavu");
    put(&vtium.join("cullet-mono.otf"), "cullet");
    put(&vtium.join("cullet-bold.otf"), "cullet bold");
    put(&mine.join("Mine-Regular.ttf"), "mine");
    // a file that others can write fails the trust rule
    let loose = fonts.join("misc/loose.bdf");
    put(&loose, "loose");
    std::fs::set_permissions(&loose, std::fs::Permissions::from_mode(0o666))
        .unwrap_or_else(|e| panic!("{e}"));

    let (presets, looks) = (root.join("share/vtium/looks"), root.join("etc/looks"));
    let tint = "version = 1\n[color tint]\ncolor = 255,176,0\n";
    put(&presets.join("amber.vtlook"), tint);
    put(&presets.join("green.vtlook"), tint);
    put(
        &presets.join("film.vtlook"),
        "version = 1\n[color gamma]\ngamma = 1.2\n[spatial scanlines]\nperiod = 3\n[spatial bloom]\n",
    );
    put(&looks.join("green.vtlook"), "version = 1\n[color glow]\n");
    put(
        &looks.join("sepia.vtlook"),
        "version = 1\n[color cube]\nfile = sepia.cube\n",
    );
    put(&looks.join("sepia.cube"), "LUT_3D_SIZE 2\n");
    put(&looks.join("warm.cube"), "LUT_3D_SIZE 2\n");
    std::fs::create_dir_all(looks.join("folder.cube")).unwrap_or_else(|e| panic!("{e}"));
    let own_looks = root.join("home/config/vtium/looks");
    put(&own_looks.join("mine.cube"), "LUT_3D_SIZE 2\n");
    let (pkg_shaders, etc_shaders) = (root.join("share/vtium/shaders"), root.join("etc/shaders"));
    put(&pkg_shaders.join("crt.vtshader"), "");
    put(&pkg_shaders.join("glow.vtshader"), "");
    put(&pkg_shaders.join("glow.glsl"), "");
    put(&etc_shaders.join("crt.vtshader"), "");
    put(&etc_shaders.join("vhs.glsl"), "");

    let (etc_palettes, pkg_palettes) =
        (root.join("etc/palettes"), root.join("share/vtium/palettes"));
    let good = "background = #101010\nforeground = #e0e0e0\npalette = 1=#ff0000\n";
    put(&pkg_palettes.join("dusk.palette"), good);
    put(&pkg_palettes.join("paper.palette"), good);
    put(&etc_palettes.join("dusk.palette"), good);
    put(
        &etc_palettes.join("broken.palette"),
        "background = #101010\nnonsense\n",
    );

    let svg = root.join("share/vtium/pointers.svg");
    put(&svg, "<svg/>");
    let icons = root.join("share/icons");
    put(&icons.join("Adwaita/cursors/left_ptr"), "cursor");
    std::fs::create_dir_all(icons.join("NoCursors")).unwrap_or_else(|e| panic!("{e}"));

    let batch_file = root.join("etc/batch.conf");
    put(
        &batch_file,
        &format!(
            "{}\n{}\n-{}\n",
            fonts.join("TTF/DejaVuSansMono.ttf").display(),
            looks.join("sepia.cube").display(),
            vtium.join("cullet-bold.otf").display(),
        ),
    );
    put(
        &root.join("share/vtium/batch.d/vtium-fonts.conf"),
        &format!(
            "{}\n{}\n",
            vtium.join("cullet-mono.otf").display(),
            vtium.join("cullet-bold.otf").display(),
        ),
    );
    let folder = |path: &Path, place| Folder {
        path: path.to_path_buf(),
        place,
    };
    Places {
        fonts: discover::Folders {
            folders: vec![
                folder(&fonts, Place::System),
                folder(&local, Place::System),
                folder(&vtium, Place::Vtium),
                folder(&mine, Place::User),
            ],
            packages: fonts.clone(),
            local,
        },
        effects: effects::Folders {
            presets,
            looks,
            shaders: vec![pkg_shaders, etc_shaders],
            own_looks: Some(own_looks),
            own_shaders: None,
        },
        palettes: vec![etc_palettes, pkg_palettes.clone()],
        pointers: pointers::Folders {
            svg,
            icons: vec![icons],
        },
        batch: vtium_settings::batch::Places {
            file: batch_file,
            vconsole_conf: root.join("etc/vconsole.conf"),
            cmdline: root.join("cmdline"),
            fonts: vtium,
            palettes: pkg_palettes,
            pointers: root.join("share/vtium/pointers.svg"),
            console_font_folders: Vec::new(),
        },
        euid: std::fs::metadata(root).map_or(0, |m| m.uid()),
    }
}

/// The file names of `paths`, for a short assertion.
fn names<'a>(paths: impl Iterator<Item = &'a Path>) -> Vec<String> {
    paths
        .map(|p| {
            p.file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
        })
        .collect()
}

// The fonts are the files the command line's scan finds in the same
// folders, in its order and with its marks, and the built-in fonts beside
// them; nothing is described until the font helper answers
#[test]
fn the_fonts_are_the_files_the_command_line_s_scan_finds() {
    let root = scratch("fonts");
    let places = machine(&root);
    let found = Found::find(places.clone());
    let mounts = vtium_settings::mounts::Mounts::system().unwrap_or_else(|e| panic!("{e}"));
    let rule = vtium_settings::cullet::Rule::running_as(places.euid);
    let today = discover::scan(&places.fonts, &rule, &mounts, &|_| {
        vtium_config_core::batch::State::Inactive
    });
    let ours: Vec<_> = found
        .fonts
        .files()
        .iter()
        .map(|f| {
            (
                f.path().to_path_buf(),
                f.place,
                f.resource.trust.clone(),
                f.duplicate.clone(),
                f.resource.own,
            )
        })
        .collect();
    let theirs: Vec<_> = today
        .iter()
        .map(|f| {
            (
                f.path.clone(),
                f.folder.place,
                f.trust.clone(),
                f.duplicate.clone(),
                !f.usable(),
            )
        })
        .collect();
    assert_eq!(ours, theirs);
    assert_eq!(
        names(found.fonts.files().iter().map(|f| f.path())),
        [
            "DejaVuSansMono.ttf",
            "loose.bdf",
            "ter-u16n.otb",
            "DejaVuSansMono.ttf",
            "cullet-bold.otf",
            "cullet-mono.otf",
            "Mine-Regular.ttf",
        ]
    );
    let loose = found.fonts.files().get(1).map(|f| f.resource.trust.clone());
    assert!(matches!(loose, Some(Err(_))), "{loose:?}");
    assert!(found.fonts.files().iter().all(|f| f.described.is_none()));
    // the built-in fonts are those the font list of the command line names
    let built_in: Vec<String> = vtium_config_core::fontlist::fonts(&root.join("no-font-sets"))
        .into_iter()
        .filter(|f| f.state == vtium_config_core::fontlist::FontState::BuiltIn)
        .map(|f| f.name)
        .collect();
    assert!(!built_in.is_empty());
    assert_eq!(found.fonts.built_in, built_in);
    let _ = std::fs::remove_dir_all(&root);
}

// Each resource says where its activation comes from: the batch file, a
// package's list in batch.d, the batch file's `-` line that turns a
// listed path off, nothing at all, or the open document's draft
#[test]
fn each_resource_says_where_its_activation_comes_from() {
    let root = scratch("origins");
    let places = machine(&root);
    let found = Found::find(places.clone());
    let origin = |path: PathBuf| {
        found
            .resource(&path)
            .map(|r| r.origin.clone())
            .unwrap_or_else(|| panic!("{} is not found", path.display()))
    };
    let fonts = &places.fonts.packages;
    let vtium = &places.batch.fonts;
    assert_eq!(
        origin(fonts.join("TTF/DejaVuSansMono.ttf")),
        Origin::BatchFile
    );
    assert_eq!(origin(fonts.join("misc/ter-u16n.otb")), Origin::Nothing);
    assert_eq!(
        origin(vtium.join("cullet-mono.otf")),
        Origin::List {
            name: "vtium-fonts".to_owned(),
            folder: ListFolder::Package,
        }
    );
    assert_eq!(
        origin(vtium.join("cullet-bold.otf")),
        Origin::TurnedOff {
            list: Some("vtium-fonts".to_owned())
        }
    );
    let looks = &places.effects.looks;
    assert_eq!(origin(looks.join("sepia.cube")), Origin::BatchFile);
    assert_eq!(origin(looks.join("warm.cube")), Origin::Nothing);
    // the draft's change comes before the installed batch's origin, and a
    // resource the draft leaves keeps the installed one
    let mut changes = Changes::default();
    changes.set(looks.join("warm.cube"), Some(Change::Activate));
    changes.set(looks.join("sepia.cube"), Some(Change::Deactivate));
    let activation = |path: PathBuf| {
        found
            .resource(&path)
            .map(|r| r.activation(&changes))
            .map(|a| format!("{a:?}"))
    };
    assert_eq!(
        activation(looks.join("warm.cube")),
        Some(format!("{:?}", Activation::Draft(Change::Activate)))
    );
    assert_eq!(
        activation(looks.join("sepia.cube")),
        Some(format!("{:?}", Activation::Draft(Change::Deactivate)))
    );
    assert_eq!(
        activation(fonts.join("TTF/DejaVuSansMono.ttf")),
        Some(format!("{:?}", Activation::Installed(&Origin::BatchFile)))
    );
    let _ = std::fs::remove_dir_all(&root);
}

// The vtlooks come No filter first, then the presets, then the
// administrator's, one of which hides the preset of its name; each lists
// its filters, with a color filter built, a spatial filter planned for its
// slice of the plan, and a filter no plan names unknown; the color maps
// leave out a folder named like one; and a packaged shader that the
// administrator's folder also holds is hidden, as a Ghostty shader is
// behind a vtium shader of its name
#[test]
fn the_effects_are_found_with_their_filters_built_or_planned() {
    let root = scratch("effects");
    let places = machine(&root);
    let found = Found::find(places.clone());
    let looks: Vec<(&str, From, bool, bool)> = found
        .effects
        .vtlooks
        .iter()
        .map(|v| (v.name.as_str(), v.from, v.hidden, v.reads.is_ok()))
        .collect();
    assert_eq!(
        looks,
        [
            ("default", From::BuiltIn, false, true),
            ("amber", From::Preset, false, true),
            ("film", From::Preset, false, false),
            ("green", From::Preset, true, true),
            ("green", From::Administrator, false, false),
            ("sepia", From::Administrator, false, true),
        ]
    );
    let filters = |name: &str, from| {
        found
            .effects
            .vtlooks
            .iter()
            .find(|v| v.name == name && v.from == from)
            .map(|v| {
                v.filters
                    .iter()
                    .map(|f| (f.layer, f.name.clone(), f.built))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    assert_eq!(
        filters("film", From::Preset),
        [
            (Layer::Color, "gamma".to_owned(), Built::Built),
            (
                Layer::Spatial,
                "scanlines".to_owned(),
                Built::Planned { slice: 4 }
            ),
            (
                Layer::Spatial,
                "bloom".to_owned(),
                Built::Planned { slice: 4 }
            ),
        ]
    );
    assert_eq!(
        filters("green", From::Administrator),
        [(Layer::Color, "glow".to_owned(), Built::Unknown)]
    );
    // a vtlook that reads lists every setting, the defaults included, and
    // the color maps it names; one that does not keeps its file's lines
    // and vtium's first error, file and line first
    let amber = found.effects.vtlook("amber").map(|v| v.filters.clone());
    let settings: Vec<Vec<(String, String)>> = amber
        .unwrap_or_default()
        .into_iter()
        .map(|f| f.settings)
        .collect();
    let pair = |k: &str, v: &str| (k.to_owned(), v.to_owned());
    assert_eq!(
        settings,
        [vec![
            pair("color", "255,176,0"),
            pair("amount", "1"),
            pair("floor", "0")
        ]]
    );
    let film = found.effects.vtlook("film");
    let scanlines = film
        .and_then(|v| v.filters.get(1))
        .map(|f| f.settings.clone());
    assert_eq!(scanlines, Some(vec![pair("period", "3")]));
    let said = film.map(|v| v.reads.clone());
    let film_file = places.effects.presets.join("film.vtlook");
    assert!(
        matches!(&said, Some(Err(e)) if e.starts_with(&format!("{}:", film_file.display())) && e.contains("no spatial filters")),
        "{said:?}"
    );
    assert_eq!(
        found.effects.vtlook("sepia").map(|v| v.files.clone()),
        Some(vec![places.effects.looks.join("sepia.cube")])
    );
    let maps: Vec<(String, bool)> = found
        .effects
        .maps
        .iter()
        .map(|m| (m.name(), m.own))
        .collect();
    let map = |n: &str, own| (n.to_owned(), own);
    assert_eq!(
        maps,
        [
            map("sepia.cube", false),
            map("warm.cube", false),
            map("mine.cube", true)
        ]
    );
    let shaders: Vec<(String, Dialect, bool)> = found
        .effects
        .shaders
        .iter()
        .map(|s| {
            (
                s.resource
                    .path
                    .strip_prefix(&root)
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                s.dialect,
                s.resource.hidden,
            )
        })
        .collect();
    let shader = |p: &str, d, hidden| (p.to_owned(), d, hidden);
    assert_eq!(
        shaders,
        [
            shader("share/vtium/shaders/crt.vtshader", Dialect::Vtium, true),
            shader("share/vtium/shaders/glow.glsl", Dialect::Ghostty, true),
            shader("share/vtium/shaders/glow.vtshader", Dialect::Vtium, false),
            shader("etc/shaders/crt.vtshader", Dialect::Vtium, false),
            shader("etc/shaders/vhs.glsl", Dialect::Ghostty, false),
        ]
    );
    let _ = std::fs::remove_dir_all(&root);
}

// The color schemes list the built-in ones, then the package's files and
// the administrator's, the package's dusk hidden by the administrator's,
// each with its colors or vtium's error; the pointer themes are vtium's
// own pictures and each icon folder's cursor theme, and a folder with no
// cursors folder is none
#[test]
fn the_color_schemes_and_the_pointer_themes_are_found() {
    let root = scratch("looks");
    let places = machine(&root);
    let found = Found::find(places);
    assert_eq!(found.palettes.built_in, vtium_term::PALETTE_NAMES);
    let schemes: Vec<(String, bool, bool)> = found
        .palettes
        .files
        .iter()
        .map(|s| {
            let dir = s.resource.path.parent().and_then(Path::file_name);
            let at = format!(
                "{}/{}",
                dir.map_or_else(String::new, |d| d.to_string_lossy().into_owned()),
                s.name
            );
            (at, s.resource.hidden, s.colors.is_ok())
        })
        .collect();
    let scheme = |at: &str, hidden, reads| (at.to_owned(), hidden, reads);
    assert_eq!(
        schemes,
        [
            scheme("palettes/dusk", true, true),
            scheme("palettes/paper", false, true),
            scheme("palettes/broken", false, false),
            scheme("palettes/dusk", false, true),
        ]
    );
    let broken = found.palettes.files.get(2).map(|s| s.colors.clone());
    assert!(
        matches!(&broken, Some(Err(e)) if e.contains("broken.palette:2")),
        "{broken:?}"
    );
    assert_eq!(
        found.palettes.file_named("dusk").map(|s| s.resource.hidden),
        Some(false)
    );
    assert_eq!(
        names(found.pointers.iter().map(|p| p.path.as_path())),
        ["pointers.svg", "Adwaita"]
    );
    assert!(
        found.pointers.iter().all(|p| p.trust.is_ok()),
        "{:?}",
        found.pointers
    );
    let _ = std::fs::remove_dir_all(&root);
}

// The folders are read once at start, and a thousand keys that filter and
// search the fonts read none of them; reading the devices again reads each
// kind once more and keeps what the font helper said of the files still
// there
#[test]
fn no_key_reads_a_folder() {
    let root = scratch("keys");
    let places = machine(&root);
    let before = found::scans();
    let mut found = Found::find(places.clone());
    let once = Scans {
        fonts: before.fonts + 1,
        effects: before.effects + 1,
        palettes: before.palettes + 1,
        pointers: before.pointers + 1,
    };
    assert_eq!(found::scans(), once);
    let asked = found.fonts.ask();
    assert_eq!(asked.len(), found.fonts.files().len());
    assert!(found.fonts.ask().is_empty());
    let answers = asked
        .iter()
        .map(|p| (p.clone(), Described::NotAFont("stand-in".to_owned())))
        .collect();
    found.fonts.describe(answers);
    let choices = Filter::choices();
    let mut kept = 0usize;
    let typed = "dejavu mono";
    for key in 0..1_000usize {
        let filter = choices
            .get(key % choices.len())
            .copied()
            .unwrap_or_default();
        let pattern = typed.get(..key % typed.len()).unwrap_or_default();
        kept += found
            .fonts
            .kept(filter)
            .filter(|f| f.fields().any(|s| s.to_lowercase().contains(pattern)))
            .count();
    }
    assert!(kept > 0);
    assert_eq!(found::scans(), once);
    // a new file appears before the devices are read again
    put(&places.fonts.packages.join("TTF/New.ttf"), "new");
    found.again();
    let twice = Scans {
        fonts: once.fonts + 1,
        effects: once.effects + 1,
        palettes: once.palettes + 1,
        pointers: once.pointers + 1,
    };
    assert_eq!(found::scans(), twice);
    let new = places.fonts.packages.join("TTF/New.ttf");
    assert_eq!(found.fonts.ask(), std::slice::from_ref(&new));
    assert!(found.fonts.waiting().all(|f| f.path() == new));
    // an install that replaces a file forgets what was said of it
    let dejavu = places.fonts.packages.join("TTF/DejaVuSansMono.ttf");
    found.fonts.forget(&dejavu);
    assert_eq!(found.fonts.ask(), [dejavu]);
    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(feature = "fixture")]
mod fixtures {
    use super::*;
    use vtium_config_core::fixture::found::{self as fixture, LARGE_FONTS, LARGE_VTLOOKS};

    // The screen map's set holds what the old TUI's fixture lists, every
    // font described, and both sets are the same each time they are built
    #[test]
    fn the_screen_map_s_set_holds_the_old_fixture_s_resources() {
        let found = fixture::padnoir();
        assert_eq!(found, fixture::padnoir());
        assert_eq!(fixture::large(), fixture::large());
        assert_eq!(
            names(found.fonts.files().iter().map(|f| f.path())),
            [
                "DejaVuSansMono.ttf",
                "ter-u16n.otb",
                "NotoColorEmoji.ttf",
                "cullet-mono.otf",
                "Mine-Regular.ttf",
            ]
        );
        let families: Vec<Option<&str>> = found.fonts.files().iter().map(|f| f.family()).collect();
        assert_eq!(
            families,
            [
                Some("DejaVu Sans Mono"),
                Some("Terminus"),
                Some("Noto Color Emoji"),
                Some("Cullet Mono"),
                Some("Mine"),
            ]
        );
        let vtlooks: Vec<(&str, From, bool)> = found
            .effects
            .vtlooks
            .iter()
            .map(|v| (v.name.as_str(), v.from, v.hidden))
            .collect();
        assert_eq!(
            vtlooks,
            [
                ("default", From::BuiltIn, false),
                ("amber", From::Preset, false),
                ("green", From::Preset, true),
                ("green", From::Administrator, false),
                ("sepia", From::Administrator, false),
            ]
        );
        assert_eq!(found.effects.maps.len(), 4);
        assert_eq!(found.palettes.files.len(), 5);
        assert_eq!(found.pointers.len(), 3);
        // Monospaced keeps DejaVu Sans Mono, Terminus and Cullet Mono
        assert_eq!(found.fonts.kept(Filter::Monospaced).count(), 3);
    }

    // The large set of 3,000 fonts and 200 vtlooks builds, and every
    // filter lists it and a search goes through it, in under a second
    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "the test times the build and the listing against the bound of a second"
    )]
    fn the_large_set_builds_and_lists_in_under_a_second() {
        let started = std::time::Instant::now();
        let found = fixture::large();
        let built = started.elapsed();
        let mut listed = 0usize;
        for filter in Filter::choices() {
            listed += found.fonts.kept(filter).count();
        }
        listed += found
            .fonts
            .files()
            .iter()
            .filter(|f| f.fields().any(|s| s.to_lowercase().contains("family 12")))
            .count();
        listed += found
            .effects
            .vtlooks
            .iter()
            .filter(|v| v.reads.is_ok())
            .count();
        let elapsed = started.elapsed();
        println!("the large set built in {built:?}, and built and listed in {elapsed:?}");
        assert_eq!(found.fonts.files().len(), LARGE_FONTS);
        assert_eq!(found.effects.vtlooks.len(), LARGE_VTLOOKS + 1);
        assert_eq!(found.fonts.kept(Filter::All).count(), LARGE_FONTS);
        assert!(listed > LARGE_FONTS);
        let planned = found
            .effects
            .vtlooks
            .iter()
            .flat_map(|v| &v.filters)
            .filter(|f| matches!(f.built, Built::Planned { .. }))
            .count();
        assert_eq!(planned, LARGE_VTLOOKS / 5);
        assert!(elapsed < std::time::Duration::from_secs(1), "{elapsed:?}");
    }
}
