//! The resources found on the fixtures' machines. [`padnoir`] holds the
//! screen map's, as the old TUI's fixture lists them: five font files,
//! five vtlooks, four color maps, five color scheme files and three
//! pointer themes, each with the marks a test needs to see. [`large`]
//! holds 3,000 font files and 200 vtlooks, besides color maps, shaders,
//! color schemes and pointer themes enough to scroll every list. Neither
//! reads a folder: every entry is made in memory, and every name and
//! number of the large set comes from its place in a loop, so both are
//! the same each time they are built.
//!
//! References:
//! - ADR-1206 D10, "Tests at both ends": each card that rebuilds a tab tests its screens with the screen map's fixture and with a large fixture of many displays, inputs and fonts.

use std::path::{Path, PathBuf};

use vtium_settings::batch::Folder as ListFolder;

use crate::describe::{Description, Face, Format, Group, SCRIPTS, Strike};
use crate::discover::{Described, Duplicate, Place};
use crate::found::effects::{Built, Dialect, Filter, From, Shader, Vtlook};
use crate::found::fonts::FontFile;
use crate::found::palettes::Scheme;
use crate::found::{Effects, Fonts, Found, Kind, Origin, Palettes, Resource};

/// The font in the person's own folder that the screen map's machine
/// lists.
pub const OWN_FONT: &str = "/home/user/.local/share/fonts/Mine-Regular.ttf";

/// The package list that activates vtium's own fonts on both machines.
pub const PACKAGE_LIST: &str = "vtium-fonts";

/// How many font files the large set holds.
pub const LARGE_FONTS: usize = 3_000;

/// How many vtlooks the large set holds besides No filter.
pub const LARGE_VTLOOKS: usize = 200;

/// The trust rule's refusal of a file that belongs to user 1000.
fn not_root(path: &str) -> Result<(), String> {
    Err(format!(
        "{path}: the file belongs to user 1000, and only root may own it."
    ))
}

/// A resource at `path` of `kind`, trusted, from `origin`.
fn resource(path: &str, kind: Kind, origin: Origin) -> Resource {
    Resource {
        path: PathBuf::from(path),
        kind,
        trust: Ok(()),
        own: false,
        hidden: false,
        origin,
    }
}

/// The list of vtium-fonts, the package that activates vtium's own fonts.
fn package_list() -> Origin {
    Origin::List {
        name: PACKAGE_LIST.to_owned(),
        folder: ListFolder::Package,
    }
}

/// The screen map's machine's resources: the fonts of [`padnoir_fonts`],
/// each described as [`described`] says; the effects of
/// [`padnoir_effects`]; the color schemes of [`padnoir_palettes`]; and the
/// pointer themes of [`padnoir_pointers`].
pub fn padnoir() -> Found {
    let mut fonts = padnoir_fonts();
    let answers = fonts
        .files()
        .iter()
        .map(|f| (f.path().to_path_buf(), described(f.path())))
        .collect();
    fonts.describe(answers);
    Found {
        fonts,
        effects: padnoir_effects(),
        palettes: padnoir_palettes(),
        pointers: padnoir_pointers(),
        places: None,
    }
}

/// The screen map's font files, in the scan's order, none described yet:
/// DejaVu Sans Mono, which the batch file activates; a Terminus strike,
/// which nothing activates; Noto Color Emoji in the administrator's
/// folder, which fails the trust rule and repeats the package's file of
/// the same bytes; vtium's own Cullet Mono, which vtium-fonts's list
/// activates; and a font in the person's own folder, which vtium cannot
/// use.
pub fn padnoir_fonts() -> Fonts {
    let file = |path: &str, place, origin| FontFile {
        resource: Resource {
            own: place == Place::User,
            ..resource(path, Kind::Font, origin)
        },
        place,
        duplicate: None,
        described: None,
        asked: false,
    };
    let emoji_path = "/usr/local/share/fonts/NotoColorEmoji.ttf";
    let mut emoji = file(emoji_path, Place::System, Origin::Nothing);
    emoji.resource.trust = not_root(emoji_path);
    emoji.duplicate = Some(Duplicate {
        of: PathBuf::from("/usr/share/fonts/noto/NotoColorEmoji.ttf"),
        same_bytes: true,
    });
    Fonts::new(vec![
        file(
            "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
            Place::System,
            Origin::BatchFile,
        ),
        file(
            "/usr/share/fonts/misc/ter-u16n.otb",
            Place::System,
            Origin::Nothing,
        ),
        emoji,
        file(
            "/usr/share/vtium/fonts/cullet-mono.otf",
            Place::Vtium,
            package_list(),
        ),
        file(OWN_FONT, Place::User, Origin::Nothing),
    ])
}

/// One face of `family`, Regular, version 2.37, covering printable ASCII.
fn face(
    family: &str,
    outline: bool,
    monospaced: bool,
    scripts: &[&'static str],
    groups: &[Group],
) -> Face {
    Face {
        family: family.to_owned(),
        style: "Regular".to_owned(),
        version: Some("2.37".to_owned()),
        outline,
        monospaced,
        scripts: scripts.to_vec(),
        groups: groups.to_vec(),
        strikes: Vec::new(),
        codepoints: 3_000,
        coverage: vec![(0x20, 0x7e)],
        coverage_cut: false,
    }
}

/// What a stand-in font helper says of the screen map's font files: each
/// face as the real font helper would describe it, and that any other
/// path is no font.
pub fn described(path: &Path) -> Described {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let faces = match name {
        "DejaVuSansMono.ttf" => vec![face(
            "DejaVu Sans Mono",
            true,
            true,
            &["Latin", "Greek", "Cyrillic"],
            &[Group::BoxDrawing, Group::Powerline],
        )],
        "ter-u16n.otb" => vec![Face {
            strikes: vec![Strike {
                width: 8,
                height: 16,
            }],
            ..face(
                "Terminus",
                false,
                true,
                &["Latin", "Cyrillic"],
                &[Group::BoxDrawing],
            )
        }],
        "NotoColorEmoji.ttf" => vec![face(
            "Noto Color Emoji",
            true,
            false,
            &[],
            &[Group::ColorEmoji],
        )],
        "cullet-mono.otf" => vec![face(
            "Cullet Mono",
            true,
            true,
            &["Latin", "Greek", "Cyrillic"],
            &[Group::BoxDrawing, Group::NerdFonts],
        )],
        "Mine-Regular.ttf" => vec![face("Mine", true, false, &["Latin"], &[])],
        _ => return Described::NotAFont("there is no such file".to_owned()),
    };
    Described::Font(Description {
        format: Format::OpenType,
        faces,
    })
}

/// A vtlook of the fixtures: `name` from `from`, with `filters`, as vtium
/// reads it or with its first error.
fn vtlook(
    name: &str,
    from: From,
    filters: Vec<Filter>,
    reads: Result<(), String>,
    maps: &[&str],
) -> Vtlook {
    let file = match from {
        From::BuiltIn => None,
        From::Preset => Some(format!("/usr/share/vtium/looks/{name}.vtlook")),
        From::Administrator => Some(format!("/etc/vtium/looks/{name}.vtlook")),
    };
    Vtlook {
        name: name.to_owned(),
        file: file.map(PathBuf::from),
        from,
        hidden: false,
        filters,
        reads,
        files: maps.iter().map(PathBuf::from).collect(),
    }
}

/// A filter named `name` with `settings`, a spatial filter where
/// `spatial` says so and a color filter otherwise.
fn filter(spatial: bool, name: &str, settings: &[(&str, &str)], built: Built) -> Filter {
    use vtium_render::filter::Layer;
    Filter {
        layer: if spatial {
            Layer::Spatial
        } else {
            Layer::Color
        },
        name: name.to_owned(),
        settings: settings
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        built,
    }
}

/// A tint of `color`, as vtium reads one with its defaults.
fn tint(color: &str) -> Filter {
    filter(
        false,
        "tint",
        &[("color", color), ("amount", "1"), ("floor", "0")],
        Built::Built,
    )
}

/// The screen map's effects: No filter; the presets amber and green; the
/// administrator's green, which does not read and hides the preset, and
/// sepia, which names the color map sepia.cube; the color maps sepia.cube,
/// which the batch file activates, warm.cube, which nothing activates,
/// loose.cube, which fails the trust rule, and mine.cube in the person's
/// own folder; and no shader.
pub fn padnoir_effects() -> Effects {
    let mut green = vtlook("green", From::Preset, vec![tint("51,255,102")], Ok(()), &[]);
    green.hidden = true;
    let map =
        |name: &str, origin| resource(&format!("/etc/vtium/looks/{name}"), Kind::ColorMap, origin);
    let mut loose = map("loose.cube", Origin::Nothing);
    loose.trust = not_root("/etc/vtium/looks/loose.cube");
    let mine = Resource {
        own: true,
        ..resource(
            "/home/user/.config/vtium/looks/mine.cube",
            Kind::ColorMap,
            Origin::Nothing,
        )
    };
    Effects {
        vtlooks: vec![
            vtlook("default", From::BuiltIn, Vec::new(), Ok(()), &[]),
            vtlook("amber", From::Preset, vec![tint("255,176,0")], Ok(()), &[]),
            green,
            vtlook(
                "green",
                From::Administrator,
                vec![filter(false, "glow", &[], Built::Unknown)],
                Err("/etc/vtium/looks/green.vtlook:2: color filter 1 of 1, glow: checking: found glow, needs gamma, contrast, tint, color-blind or cube".to_owned()),
                &[],
            ),
            vtlook(
                "sepia",
                From::Administrator,
                vec![filter(false, "cube", &[("file", "sepia.cube")], Built::Built)],
                Ok(()),
                &["/etc/vtium/looks/sepia.cube"],
            ),
        ],
        maps: vec![
            map("sepia.cube", Origin::BatchFile),
            map("warm.cube", Origin::Nothing),
            loose,
            mine,
        ],
        shaders: Vec::new(),
    }
}

/// A color scheme of the fixtures: its 16 colors stepping from `dark` to
/// `light`, with `accent` as its red, and its foreground and background.
fn colors(dark: u8, light: u8, accent: [u8; 3]) -> vtium_term::Palette {
    use vtium_term::Rgb;
    let mut sixteen = [Rgb::default(); 16];
    for (i, c) in sixteen.iter_mut().enumerate() {
        let step = u8::try_from(i)
            .unwrap_or(0)
            .saturating_mul(light.saturating_sub(dark) / 16);
        let v = dark.saturating_add(step);
        *c = Rgb::new(v, v.saturating_add(8), v.saturating_add(16));
    }
    if let Some(red) = sixteen.get_mut(1) {
        *red = Rgb::new(accent[0], accent[1], accent[2]);
    }
    vtium_term::Palette::custom(
        sixteen,
        Rgb::new(light, light, light),
        Rgb::new(dark, dark, dark),
    )
}

/// A color scheme file `name` in `dir`, from `origin`, with `colors`.
fn scheme(
    dir: &str,
    name: &str,
    origin: Origin,
    colors: Result<vtium_term::Palette, String>,
) -> Scheme {
    Scheme {
        name: name.to_owned(),
        resource: resource(&format!("{dir}/{name}.palette"), Kind::ColorScheme, origin),
        colors,
    }
}

/// The screen map's color schemes: the built-in ones; the package's dusk,
/// which the administrator's dusk hides, and paper; the administrator's
/// dusk; loose, which fails the trust rule; and broken, which does not
/// read. The batch file activates both dusks and paper. No folder is
/// named, so no look of the fixture uses a scheme file: the rule that
/// names those looks reads the files on the disk, and a test that needs
/// one writes its own folder.
///
/// Every scheme here is made up for the tests, and none is a scheme vtium
/// ships. paper is a light scheme of the tests' own: vtium's e-ink scheme
/// is ink, in crates/vtium/install/palettes.
pub fn padnoir_palettes() -> Palettes {
    let (pkg, etc) = ("/usr/share/vtium/palettes", "/etc/vtium/palettes");
    let mut dusk = scheme(
        pkg,
        "dusk",
        Origin::BatchFile,
        Ok(colors(20, 200, [200, 60, 60])),
    );
    dusk.resource.hidden = true;
    let mut loose = scheme(
        etc,
        "loose",
        Origin::Nothing,
        Ok(colors(0, 160, [255, 0, 0])),
    );
    loose.resource.trust = not_root("/etc/vtium/palettes/loose.palette");
    Palettes {
        built_in: vtium_term::PALETTE_NAMES.to_vec(),
        files: vec![
            dusk,
            scheme(pkg, "paper", Origin::BatchFile, Ok(colors(16, 240, [180, 40, 40]))),
            scheme(etc, "dusk", Origin::BatchFile, Ok(colors(24, 216, [220, 80, 40]))),
            loose,
            scheme(
                etc,
                "broken",
                Origin::Nothing,
                Err("/etc/vtium/palettes/broken.palette:3: This line is not a setting. Each line of a color scheme reads key = value, or is a comment that starts with #.".to_owned()),
            ),
        ],
        folders: Vec::new(),
    }
}

/// The screen map's pointer themes: vtium's own pictures, which the batch
/// file activates; the cursor theme Adwaita, which nothing activates; and
/// stray, whose link leaves its folder, so it fails the trust rule.
pub fn padnoir_pointers() -> Vec<Resource> {
    let mut stray = resource(
        "/usr/share/icons/stray",
        Kind::PointerTheme,
        Origin::Nothing,
    );
    stray.trust = Err(
        "/usr/share/icons/stray/cursors/left_ptr: this symbolic link leaves the theme's folder."
            .to_owned(),
    );
    vec![
        resource(
            "/usr/share/vtium/pointers.svg",
            Kind::PointerTheme,
            Origin::BatchFile,
        ),
        resource(
            "/usr/share/icons/Adwaita",
            Kind::PointerTheme,
            Origin::Nothing,
        ),
        stray,
    ]
}

/// The large machine's resources: [`LARGE_FONTS`] font files across every
/// kind of folder, each described, a few of them failing the trust rule;
/// [`LARGE_VTLOOKS`] vtlooks, half presets and half the administrator's,
/// some of which hide a preset and some of which name a planned spatial
/// filter; 40 color maps; 12 shaders; 60 color schemes; and 40 pointer
/// themes.
pub fn large() -> Found {
    let mut fonts = large_fonts();
    let answers = fonts
        .files()
        .iter()
        .enumerate()
        .map(|(i, f)| (f.path().to_path_buf(), large_description(i)))
        .collect();
    fonts.describe(answers);
    Found {
        fonts,
        effects: large_effects(),
        palettes: large_palettes(),
        pointers: (0..40)
            .map(|i| {
                resource(
                    &format!("/usr/share/icons/Theme-{i:02}"),
                    Kind::PointerTheme,
                    if i % 5 == 0 {
                        Origin::BatchFile
                    } else {
                        Origin::Nothing
                    },
                )
            })
            .collect(),
        places: None,
    }
}

/// Where the large set's font `i` sits, and the kind of folder.
fn large_font_path(i: usize) -> (String, Place) {
    match i % 10 {
        0 => (
            format!("/usr/share/kbd/consolefonts/console-{i:04}.psf.gz"),
            Place::Console,
        ),
        1 => (
            format!("/usr/share/vtium/fonts/vtium-{i:04}.otf"),
            Place::Vtium,
        ),
        2 => (
            format!("/home/user/.local/share/fonts/Mine-{i:04}.ttf"),
            Place::User,
        ),
        3 => (
            format!("/usr/local/share/fonts/Local-{i:04}.ttf"),
            Place::System,
        ),
        _ => (
            format!(
                "/usr/share/fonts/family-{:03}/Family{i:04}-Regular.ttf",
                i / 10
            ),
            Place::System,
        ),
    }
}

/// The large set's font files, none described yet.
pub fn large_fonts() -> Fonts {
    Fonts::new(
        (0..LARGE_FONTS)
            .map(|i| {
                let (path, place) = large_font_path(i);
                let origin = match (place, i % 4) {
                    (Place::User, _) => Origin::Nothing,
                    (Place::Vtium, _) => package_list(),
                    (_, 0) => Origin::BatchFile,
                    _ => Origin::Nothing,
                };
                let mut file = FontFile {
                    resource: Resource {
                        own: place == Place::User,
                        ..resource(&path, Kind::Font, origin)
                    },
                    place,
                    duplicate: None,
                    described: None,
                    asked: false,
                };
                if i % 50 == 13 {
                    file.resource.trust = not_root(&path);
                }
                file
            })
            .collect(),
    )
}

/// What the stand-in font helper says of the large set's font `i`: a
/// family of its own, outline or bitmap, monospaced or proportional, with
/// scripts and groups that step through every filter.
fn large_description(i: usize) -> Described {
    let outline = !i.is_multiple_of(10) && !i.is_multiple_of(3);
    let scripts: Vec<&'static str> = SCRIPTS
        .iter()
        .skip(i % SCRIPTS.len())
        .take(1 + i % 3)
        .map(|s| s.name)
        .collect();
    let groups: Vec<Group> = Group::ALL
        .iter()
        .copied()
        .filter(|g| (*g as usize + i).is_multiple_of(4))
        .collect();
    let mut f = face(
        &format!("Family {i:04}"),
        outline,
        i.is_multiple_of(2),
        &scripts,
        &groups,
    );
    if !outline {
        let height = u32::try_from(12 + 2 * (i % 8)).unwrap_or(16);
        f.strikes = vec![Strike {
            width: height / 2,
            height,
        }];
    }
    Described::Font(Description {
        format: if outline {
            Format::OpenType
        } else {
            Format::Psf
        },
        faces: vec![f],
    })
}

/// The large set's effects.
fn large_effects() -> Effects {
    let mut vtlooks = vec![vtlook("default", From::BuiltIn, Vec::new(), Ok(()), &[])];
    let half = LARGE_VTLOOKS / 2;
    for i in 0..LARGE_VTLOOKS {
        let (from, n) = if i < half {
            (From::Preset, i)
        } else {
            (From::Administrator, i - half)
        };
        // every tenth administrator's vtlook takes a preset's name
        let name = if from == From::Preset || n.is_multiple_of(10) {
            format!("look-{n:03}")
        } else {
            format!("admin-{n:03}")
        };
        let mut filters = vec![
            filter(false, "gamma", &[("gamma", "1.2")], Built::Built),
            tint("255,176,0"),
        ];
        let mut reads = Ok(());
        if n % 5 == 4 {
            filters.push(filter(
                true,
                "scanlines",
                &[("period", "3")],
                Built::Planned { slice: 4 },
            ));
            reads = Err(format!(
                "/etc/vtium/looks/{name}.vtlook:6: spatial filter 1 of 1, scanlines: checking: found scanlines, but this build of vtium has no spatial filters"
            ));
        }
        let mut look = vtlook(&name, from, filters, reads, &[]);
        look.hidden = from == From::Preset && n % 10 == 0;
        vtlooks.push(look);
    }
    Effects {
        vtlooks,
        maps: (0..40)
            .map(|i| {
                resource(
                    &format!("/etc/vtium/looks/map-{i:02}.cube"),
                    Kind::ColorMap,
                    if i % 3 == 0 {
                        Origin::BatchFile
                    } else {
                        Origin::Nothing
                    },
                )
            })
            .collect(),
        shaders: (0..12)
            .map(|i| {
                let (dir, dialect, suffix) = if i % 4 == 3 {
                    ("/etc/vtium/shaders", Dialect::Ghostty, "glsl")
                } else {
                    ("/usr/share/vtium/shaders", Dialect::Vtium, "vtshader")
                };
                Shader {
                    resource: resource(
                        &format!("{dir}/shader-{i:02}.{suffix}"),
                        Kind::Shader,
                        Origin::Nothing,
                    ),
                    dialect,
                }
            })
            .collect(),
    }
}

/// The large set's color schemes: 30 of the package's and 30 of the
/// administrator's, every sixth of which hides the package's of its name.
fn large_palettes() -> Palettes {
    let (pkg, etc) = ("/usr/share/vtium/palettes", "/etc/vtium/palettes");
    let mut files = Vec::new();
    for (dir, admin) in [(pkg, false), (etc, true)] {
        for i in 0..30u8 {
            let name = format!("scheme-{i:02}");
            let mut s = scheme(
                dir,
                &name,
                if i % 2 == 0 {
                    Origin::BatchFile
                } else {
                    Origin::Nothing
                },
                Ok(colors(i, 200u8.saturating_add(i), [200, 40, i])),
            );
            s.resource.hidden = !admin && i % 6 == 0;
            if admin && i % 6 != 0 {
                s.name = format!("own-{i:02}");
                s.resource.path = PathBuf::from(format!("{etc}/own-{i:02}.palette"));
            }
            files.push(s);
        }
    }
    Palettes {
        built_in: vtium_term::PALETTE_NAMES.to_vec(),
        files,
        folders: Vec::new(),
    }
}
