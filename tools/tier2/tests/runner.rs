//! The run's verdicts on a set of small screens and goldens written here:
//! each status the report gives, and the lists' checks before any case
//! runs.

use std::fs;
use std::path::{Path, PathBuf};

use vtium_tier2::draw::{self, Screen};
use vtium_tier2::ppm::Image;
use vtium_tier2::report::Status;
use vtium_tier2::runner::{Departure, Pending, Run};
use vtium_tier2::screens::{Case, Source};
use vtium_tier2::tol::Rect;

const fn case(name: &'static str, source: Source) -> Case {
    Case {
        screen: Screen::new(name, "two letters", b"ab", 2, 1),
        source,
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
}

impl Setup {
    fn new(name: &str) -> Setup {
        let base = dir(name);
        let (kmscon, vtium, out) = (base.join("k"), base.join("v"), base.join("out"));
        for d in [&kmscon, &vtium, &out] {
            fs::create_dir_all(d).unwrap();
        }
        put(&kmscon, "k-same", &golden(false));
        put(&kmscon, "k-pending", &golden(false));
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
    s.pending[0].golden = "k-nothing".to_owned();
    assert!(
        s.run()
            .check_lists(&titles)
            .unwrap_err()
            .contains("no golden")
    );
    s.pending.clear();
    s.provisional = vec!["k-same".to_owned()];
    assert!(
        s.run()
            .check_lists(&titles)
            .unwrap_err()
            .contains("provisional.tsv")
    );
    s.provisional = vec!["v-dep".to_owned()];
    assert!(
        s.run()
            .check_lists(&titles)
            .unwrap_err()
            .contains("both provisional")
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
