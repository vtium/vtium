//! The run's verdicts on a set of small screens and goldens written here:
//! each status the report gives, and the lists' checks before any case
//! runs.

use std::fs;
use std::path::{Path, PathBuf};

use vtium_tier2::draw::{self, Screen};
use vtium_tier2::ppm::Image;
use vtium_tier2::report::Status;
use vtium_tier2::runner::{self, Pending, Run};
use vtium_tier2::screens::Case;

const fn case(name: &'static str) -> Case {
    Case {
        screen: Screen::new(name, "two letters", b"ab", 2, 1),
    }
}

const CASES: &[Case] = &[
    case("v-same"),
    case("v-diff"),
    case("v-tol"),
    case("v-prov"),
    case("v-none"),
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
    vtium: PathBuf,
    out: PathBuf,
    pending: Vec<Pending>,
    provisional: Vec<String>,
}

impl Setup {
    fn new(name: &str) -> Setup {
        let base = dir(name);
        let (vtium, out) = (base.join("v"), base.join("out"));
        for d in [&vtium, &out] {
            fs::create_dir_all(d).unwrap();
        }
        put(&vtium, "v-same", &golden(false));
        for n in ["v-diff", "v-tol", "v-prov", "v-orphan"] {
            put(&vtium, n, &golden(true));
        }
        fs::write(vtium.join("v-tol.tol"), "max_delta=64\nmax_pixels=1\n").unwrap();
        Setup {
            vtium,
            out,
            pending: vec![Pending {
                golden: "k-pending".to_owned(),
                issue: "vtium/vtium#24".to_owned(),
                why: "outline-font text".to_owned(),
            }],
            provisional: vec!["v-prov".to_owned()],
        }
    }

    fn run(&self) -> Run<'_> {
        Run {
            cases: CASES,
            goldens: &self.vtium,
            pending: &self.pending,
            provisional: &self.provisional,
            out: &self.out,
            root: &self.out,
        }
    }
}

#[test]
fn each_case_gets_its_status() {
    let s = Setup::new("statuses");
    s.run().check_lists().unwrap();
    let got: Vec<(String, Status)> = s
        .run()
        .cases()
        .unwrap()
        .into_iter()
        .map(|c| (c.name, c.status))
        .collect();
    let want = [
        ("v-same", Status::Pass),
        ("v-diff", Status::Fail),
        ("v-tol", Status::Near),
        ("v-prov", Status::Provisional),
        ("v-none", Status::Missing),
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
fn a_provisional_case_reports_the_verdict_it_would_get() {
    let mut s = Setup::new("provisional");
    s.provisional = vec!["v-diff".to_owned(), "v-tol".to_owned()];
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
        note("v-tol").ends_with("it would be near"),
        "{}",
        note("v-tol")
    );
}

#[test]
fn lists_that_name_the_wrong_cases_are_refused() {
    let mut s = Setup::new("lists");
    s.pending[0].golden = "v-same".to_owned();
    assert!(
        s.run()
            .check_lists()
            .unwrap_err()
            .contains("which a screen draws")
    );
    s.pending[0].golden = "v-orphan".to_owned();
    assert!(
        s.run()
            .check_lists()
            .unwrap_err()
            .contains("which has a golden")
    );
    let mut cases = CASES.to_vec();
    cases.push(case("v-same"));
    let run = Run {
        cases: &cases,
        ..s.run()
    };
    assert!(
        run.check_lists()
            .unwrap_err()
            .contains("two screens are named")
    );
    s.pending.clear();
    s.provisional = vec!["v-orphan".to_owned()];
    assert!(
        s.run()
            .check_lists()
            .unwrap_err()
            .contains("provisional.tsv")
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

/// A tool tree under a repository root: one golden and the lists.
fn tool_tree(name: &str) -> PathBuf {
    let root = dir(name);
    let tool = root.join("tools/tier2");
    fs::create_dir_all(tool.join("goldens")).unwrap();
    fs::write(tool.join("goldens/blocks.ppm"), b"P6 1 1 255\n\x00\x00\x00").unwrap();
    fs::write(
        tool.join("pending.tsv"),
        "# a comment\nascii\tvtium/vtium#24\toutline-font text\n",
    )
    .unwrap();
    fs::write(tool.join("provisional.tsv"), "v-prov\tnot yet reviewed\n").unwrap();
    root
}

#[test]
fn the_lists_load_from_the_tool_tree() {
    let root = tool_tree("load");
    let lists = runner::load(&root).unwrap();
    assert_eq!(lists.pending.len(), 1);
    assert_eq!(lists.pending[0].golden, "ascii");
    assert_eq!(lists.provisional, ["v-prov"]);

    fs::remove_file(root.join("tools/tier2/pending.tsv")).unwrap();
    let e = runner::load(&root).err().unwrap();
    assert!(e.contains("pending.tsv: "), "{e}");
}
