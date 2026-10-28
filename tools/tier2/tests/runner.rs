//! The run's verdicts on a set of small screens and goldens written here:
//! each status the report gives, and the lists' checks before any case
//! runs.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use vtium_tier2::draw::{self, Screen};
use vtium_tier2::ppm::Image;
use vtium_tier2::report::Status;
use vtium_tier2::runner::{self, Departure, Pending, Run};
use vtium_tier2::screens::{Case, Source};
use vtium_tier2::tol::Rect;

const fn case(name: &'static str, source: Source) -> Case {
    Case {
        screen: Screen::new(name, "two letters", b"ab", 2, 1),
        source,
        ported: None,
    }
}

const CASES: &[Case] = &[
    case("k-same", Source::Kmscon),
    case("v-same", Source::Vtium),
    case("v-diff", Source::Vtium),
    case("v-tol", Source::Vtium),
    case("v-dep", Source::Vtium),
    case("v-stale", Source::Vtium),
    case("v-prov", Source::Vtium),
    case("v-none", Source::Vtium),
];

fn dir(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("runner")
        .join(name);
    if d.exists() {
        fs::remove_dir_all(&d).unwrap();
    }
    fs::create_dir_all(&d).unwrap();
    d
}

/// The two letters' image, with pixel 3,5 changed when `changed`.
fn golden(changed: bool) -> Image {
    let img = draw::draw(&CASES[0].screen).unwrap();
    let mut rgb = img.rgb().to_vec();
    if changed {
        let i = (5 * img.width() as usize + 3) * 3;
        rgb[i] ^= 0x40;
    }
    Image::new(img.width(), img.height(), rgb).unwrap()
}

fn put(dir: &Path, name: &str, img: &Image) {
    fs::write(dir.join(format!("{name}.ppm")), img.to_ppm()).unwrap();
}

struct Setup {
    kmscon: PathBuf,
    vtium: PathBuf,
    out: PathBuf,
    departures: Vec<Departure>,
    pending: Vec<Pending>,
    provisional: Vec<String>,
    c_goldens: Vec<String>,
}

impl Setup {
    fn new(name: &str) -> Setup {
        let base = dir(name);
        let (kmscon, vtium, out) = (base.join("k"), base.join("v"), base.join("out"));
        for d in [&kmscon, &vtium, &out] {
            fs::create_dir_all(d).unwrap();
        }
        put(&kmscon, "k-same", &golden(false));
        put(&kmscon, "k-orphan", &golden(false));
        for n in ["v-same", "v-stale"] {
            put(&vtium, n, &golden(false));
        }
        for n in ["v-diff", "v-tol", "v-dep", "v-prov", "v-orphan"] {
            put(&vtium, n, &golden(true));
        }
        fs::write(vtium.join("v-tol.tol"), "max_delta=64\nmax_pixels=1\n").unwrap();
        let near = |case: &str, rect: Rect| Departure {
            case: case.to_owned(),
            title: "A departure".to_owned(),
            rects: vec![rect],
        };
        Setup {
            kmscon,
            vtium,
            out,
            departures: vec![
                near(
                    "v-dep",
                    Rect {
                        x0: 0,
                        y0: 0,
                        x1: 8,
                        y1: 8,
                    },
                ),
                near(
                    "v-stale",
                    Rect {
                        x0: 0,
                        y0: 0,
                        x1: 8,
                        y1: 8,
                    },
                ),
            ],
            pending: vec![Pending {
                golden: "k-pending".to_owned(),
                issue: "vtium/vtium#24".to_owned(),
                why: "outline-font text".to_owned(),
            }],
            provisional: vec!["v-prov".to_owned()],
            c_goldens: ["k-orphan.ppm", "k-pending.ppm", "k-same.ppm"]
                .map(str::to_owned)
                .to_vec(),
        }
    }

    fn run(&self) -> Run<'_> {
        Run {
            cases: CASES,
            kmscon_goldens: &self.kmscon,
            vtium_goldens: &self.vtium,
            departures: &self.departures,
            pending: &self.pending,
            provisional: &self.provisional,
            c_goldens: &self.c_goldens,
            out: &self.out,
            root: &self.out,
        }
    }
}

#[test]
fn each_case_gets_its_status() {
    let s = Setup::new("statuses");
    let titles = ["A departure".to_owned()];
    s.run().check_lists(&titles).unwrap();
    let got: Vec<(String, Status)> = s
        .run()
        .cases()
        .unwrap()
        .into_iter()
        .map(|c| (c.name, c.status))
        .collect();
    let want = [
        ("k-same", Status::Pass),
        ("v-same", Status::Pass),
        ("v-diff", Status::Fail),
        ("v-tol", Status::Near),
        ("v-dep", Status::Departure),
        ("v-stale", Status::Fail),
        ("v-prov", Status::Provisional),
        ("v-none", Status::Missing),
        ("k-orphan", Status::Missing),
        ("k-pending", Status::Pending),
        ("v-orphan", Status::Missing),
    ];
    let want: Vec<(String, Status)> = want.iter().map(|(n, st)| (n.to_string(), *st)).collect();
    assert_eq!(got, want);
    assert!(s.out.join("actual/v-diff.ppm").exists());
    assert!(s.out.join("diff/v-diff.ppm").exists());
    assert!(!s.out.join("diff/v-same.ppm").exists());
}

#[test]
fn a_listed_golden_neither_copied_nor_pending_is_missing() {
    let mut s = Setup::new("dropped");
    s.pending.clear();
    s.run().check_lists(&["A departure".to_owned()]).unwrap();
    let cases = s.run().cases().unwrap();
    let dropped = cases.iter().find(|c| c.name == "k-pending").unwrap();
    assert_eq!(dropped.status, Status::Missing);
    assert!(!dropped.status.passes());
}

#[test]
fn a_provisional_case_reports_the_verdict_it_would_get() {
    let mut s = Setup::new("provisional");
    s.provisional = vec!["v-diff".to_owned(), "v-dep".to_owned()];
    let cases = s.run().cases().unwrap();
    let note = |name: &str| {
        let c = cases.iter().find(|c| c.name == name).unwrap();
        assert_eq!(c.status, Status::Provisional);
        c.note.clone().unwrap()
    };
    assert!(
        note("v-diff").ends_with("it would be fail"),
        "{}",
        note("v-diff")
    );
    assert!(
        note("v-dep").ends_with("it would be departure"),
        "{}",
        note("v-dep")
    );
}

#[test]
fn a_difference_outside_the_departures_rectangles_fails() {
    let mut s = Setup::new("outside");
    s.departures[0].rects = vec![Rect {
        x0: 4,
        y0: 0,
        x1: 8,
        y1: 8,
    }];
    let cases = s.run().cases().unwrap();
    let dep = cases.iter().find(|c| c.name == "v-dep").unwrap();
    assert_eq!(dep.status, Status::Fail);
    assert!(
        dep.note
            .as_deref()
            .unwrap()
            .contains("outside the departure")
    );
}

#[test]
fn a_departure_the_readme_does_not_list_is_refused() {
    let s = Setup::new("titles");
    let e = s.run().check_lists(&["Another".to_owned()]).unwrap_err();
    assert!(e.contains("lists no departure"), "{e}");
}

#[test]
fn lists_that_name_the_wrong_cases_are_refused() {
    let titles = ["A departure".to_owned()];
    let mut s = Setup::new("lists");
    s.pending[0].golden = "k-same".to_owned();
    assert!(
        s.run()
            .check_lists(&titles)
            .unwrap_err()
            .contains("which a screen draws")
    );
    s.pending[0].golden = "k-orphan".to_owned();
    assert!(
        s.run()
            .check_lists(&titles)
            .unwrap_err()
            .contains("which is copied")
    );
    s.pending[0].golden = "k-unlisted".to_owned();
    assert!(
        s.run()
            .check_lists(&titles)
            .unwrap_err()
            .contains("c-goldens.list")
    );
    s.pending[0].golden = "k-pending".to_owned();
    s.c_goldens.retain(|n| n != "k-orphan.ppm");
    assert!(
        s.run()
            .check_lists(&titles)
            .unwrap_err()
            .contains("c-goldens.list")
    );
    s.c_goldens.push("k-orphan.ppm".to_owned());
    let mut cases = CASES.to_vec();
    cases.push(case("k-pending", Source::Vtium));
    let run = Run {
        cases: &cases,
        ..s.run()
    };
    assert!(
        run.check_lists(&titles)
            .unwrap_err()
            .contains("the C reference's name")
    );
    s.pending.clear();
    s.provisional = vec!["k-same".to_owned()];
    assert!(
        s.run()
            .check_lists(&titles)
            .unwrap_err()
            .contains("provisional.tsv")
    );
    s.provisional.clear();
    s.departures[0].case = "nothing".to_owned();
    assert!(
        s.run()
            .check_lists(&titles)
            .unwrap_err()
            .contains("no screen draws")
    );
}

#[test]
fn a_malformed_golden_or_sidecar_is_an_error() {
    let s = Setup::new("malformed");
    fs::write(s.vtium.join("v-same.ppm"), b"P6 4 4 255\n").unwrap();
    fs::write(s.vtium.join("v-diff.tol"), "max_delta=3\n").unwrap();
    fs::write(s.vtium.join("v-tol.tol"), "mask=0,0,999,1\n").unwrap();
    let cases = s.run().cases().unwrap();
    for name in ["v-same", "v-diff", "v-tol"] {
        let c = cases.iter().find(|c| c.name == name).unwrap();
        assert_eq!(c.status, Status::Error, "{name}: {:?}", c.note);
    }
}

#[test]
fn a_sprites_only_screen_holding_text_is_refused() {
    let sprites = |bytes: &'static [u8]| Screen {
        font: draw::FontKind::Sprites {
            width: 9,
            height: 16,
        },
        ..Screen::new("s", "sprites", bytes, 4, 1)
    };
    assert!(draw::draw(&sprites("█ ┼".as_bytes())).is_ok());
    let e = draw::draw(&sprites("█A┼".as_bytes())).unwrap_err();
    assert!(e.contains("U+0041"), "{e}");
}

#[test]
fn a_sprites_only_screen_holding_a_combining_mark_is_refused() {
    let sprites = |bytes: &'static [u8]| Screen {
        font: draw::FontKind::Sprites {
            width: 9,
            height: 16,
        },
        ..Screen::new("s", "sprites", bytes, 4, 1)
    };
    for bytes in [" \u{301}┼", "█\u{301}┼"] {
        let e = draw::draw(&sprites(bytes.as_bytes())).unwrap_err();
        assert!(e.contains("U+0301"), "{bytes:?}: {e}");
    }
}

#[test]
fn the_list_of_c_goldens_is_the_pins() {
    let names = |n: &[&str]| n.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>();
    let both = names(&["blocks.ppm", "ascii.ppm"]);
    assert_eq!(runner::check_pin(&both, &both), Vec::<String>::new());
    assert_eq!(
        runner::check_pin(
            &names(&["blocks.ppm", "new.ppm"]),
            &names(&["blocks.ppm", "old.ppm"])
        ),
        [
            "new.ppm is at the pin but not in c-goldens.list",
            "c-goldens.list names old.ppm, which the pin lacks",
        ]
    );
}

#[test]
fn malformed_lists_of_c_goldens_are_refused() {
    assert_eq!(
        runner::c_goldens("# c\nb.ppm\na.ppm\n").unwrap(),
        ["b.ppm", "a.ppm"]
    );
    for text in ["a.ppm\na.ppm", "x/a.ppm", "a.ppm\tb"] {
        assert!(runner::c_goldens(text).is_err(), "{text:?} was taken");
    }
}

/// A tool tree under a repository root: one copied golden and the copied
/// screens, each with its row, and empty lists.
fn tool_tree(name: &str) -> PathBuf {
    let root = dir(name);
    let tool = root.join("tools/tier2");
    fs::create_dir_all(tool.join("reference/goldens")).unwrap();
    fs::create_dir_all(root.join("crates/vtium-render")).unwrap();
    fs::write(root.join("crates/vtium-render/README.md"), "# render\n").unwrap();
    let golden = b"P6 1 1 255\n\x00\x00\x00";
    let screens = b"static const struct screen screens[] = {\n};\n";
    fs::write(tool.join("reference/goldens/blocks.ppm"), golden).unwrap();
    fs::write(tool.join("reference/test_pixels.c"), screens).unwrap();
    let commit = "b1f7a7c5df563f3378584db7298753848a7a9a13";
    let row = |path: &str, source: &str, data: &[u8]| {
        format!(
            "tools/tier2/reference/{path}\tkmscon\t{commit}\t{source}\t{}\n",
            vtium_conformance::sha256::hex(data)
        )
    };
    let rows = row(
        "goldens/blocks.ppm",
        "tests/goldens/tier2/blocks.ppm",
        golden,
    ) + &row("test_pixels.c", "tests/test_pixels.c", screens);
    fs::write(tool.join("provenance.tsv"), rows).unwrap();
    for list in ["departures.tsv", "pending.tsv", "provisional.tsv"] {
        fs::write(tool.join(list), "").unwrap();
    }
    fs::write(tool.join("c-goldens.list"), "blocks.ppm\n").unwrap();
    root
}

#[test]
fn the_lists_load_once_every_copy_matches_its_row() {
    let root = tool_tree("load");
    let lists = runner::load(&root, &[]).unwrap();
    assert_eq!(lists.commit, "b1f7a7c5df563f3378584db7298753848a7a9a13");
    assert_eq!(lists.c_goldens, ["blocks.ppm"]);
    let copy = root.join("tools/tier2/reference/goldens/blocks.ppm");

    fs::write(&copy, b"P6 1 1 255\n\x00\x00\x01").unwrap();
    let e = runner::load(&root, &[]).err().unwrap();
    assert!(e.contains("blocks.ppm differs from kmscon"), "{e}");

    fs::remove_file(&copy).unwrap();
    let e = runner::load(&root, &[]).err().unwrap();
    assert!(e.contains("goldens/blocks.ppm: "), "{e}");

    let root = tool_tree("load-stray");
    let stray = root.join("tools/tier2/reference/goldens/stray.ppm");
    fs::write(&stray, b"P6 1 1 255\n\x00\x00\x00").unwrap();
    let e = runner::load(&root, &[]).err().unwrap();
    assert!(e.contains("stray.ppm has no row in provenance.tsv"), "{e}");
}
