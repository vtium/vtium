thread_local! {
    /// How many times this thread's `Changes` walked their whole list
    /// to build the index again.
    pub(super) static LIST_PASSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

use super::*;

fn batch(paths: &[&str]) -> Batch {
    Batch::new(
        paths
            .iter()
            .map(|p| Entry::Path(PathBuf::from(p)))
            .collect(),
    )
}

/// A start of the batch from the list of a fresh install `fresh`, with
/// folders of lists that do not exist.
fn bare(fresh: Batch) -> Start {
    Start {
        fresh,
        folders: Folders {
            package: PathBuf::from("/nonexistent/package-lists"),
            admin: PathBuf::from("/nonexistent/batch.d"),
        },
        trust: vtium_settings::Trust::running_as(0),
    }
}

#[test]
fn a_changes_file_reads_back_as_it_was_written() {
    let mut changes = Changes::default();
    changes.set("/a/x.ttf".into(), Some(Change::Activate));
    changes.set("/a/y.ttf".into(), Some(Change::Deactivate));
    changes.set("/a/x.ttf".into(), Some(Change::Deactivate));
    let text = changes.text("desk");
    assert!(
        text.ends_with("deactivate /a/x.ttf\ndeactivate /a/y.ttf\n"),
        "{text}"
    );
    assert_eq!(
        Changes::parse(&text, Path::new("d.batch")),
        Ok(changes.clone())
    );
    // taking a change back keeps the others found by their paths
    changes.set("/a/x.ttf".into(), None);
    assert_eq!(changes.get(Path::new("/a/x.ttf")), None);
    assert_eq!(changes.get(Path::new("/a/y.ttf")), Some(Change::Deactivate));
    changes.set("/a/z.ttf".into(), Some(Change::Activate));
    assert_eq!(changes.get(Path::new("/a/z.ttf")), Some(Change::Activate));
    assert_eq!(changes.len(), 2);
    for bad in [
        "activate x.ttf",
        "activate /a/../b",
        "install /a",
        "activate",
        "activate /a/\tb.ttf",
    ] {
        let fail = Changes::parse(bad, Path::new("d.batch")).unwrap_err();
        assert!(fail.text().starts_with("d.batch line 1: "), "{bad}");
    }
}

/// A large `NAME.batch` reads in one pass: each line finds its path
/// through the index, so the read never walks the list again. The test
/// counts the passes over the list instead of timing the read.
#[test]
fn a_large_changes_file_reads_without_a_scan_for_each_line() {
    let text: String = (0..26_000)
        .map(|n| format!("activate /fonts/font-{n:05}.ttf\ndeactivate /fonts/font-{n:05}.ttf\n"))
        .collect();
    LIST_PASSES.with(|n| n.set(0));
    let changes = Changes::parse(&text, Path::new("d.batch")).expect("the file reads");
    assert_eq!(LIST_PASSES.with(std::cell::Cell::get), 0);
    assert_eq!(changes.len(), 26_000);
    assert_eq!(changes.index.len(), 26_000);
    for (at, (path, change)) in changes.list.iter().enumerate() {
        assert_eq!(changes.index.get(path), Some(&at));
        assert_eq!(*change, Change::Deactivate);
    }
}

#[test]
fn the_changes_apply_to_the_installed_file_and_keep_its_other_lines() {
    let fresh = bare(batch(&["/f/one", "/f/two"]));
    let mut changes = Changes::default();
    changes.set("/n/new".into(), Some(Change::Activate));
    changes.set("/k/kept".into(), Some(Change::Deactivate));
    let installed = "# mine\n/k/kept\n\n/k//other\n";
    assert_eq!(
        applied(Some(installed), &fresh, &changes).as_deref(),
        Some("# mine\n\n/k//other\n/n/new\n")
    );
    // none installed: the fresh list, then the change
    let text = applied(None, &fresh, &changes).expect("a new file");
    assert!(text.starts_with(HEADER), "{text}");
    assert!(text.ends_with("/f/one\n/f/two\n/n/new\n"), "{text}");
    // changes the system already matches change nothing
    let mut same = Changes::default();
    same.set("/f/one".into(), Some(Change::Activate));
    same.set("/gone".into(), Some(Change::Deactivate));
    assert_eq!(applied(None, &fresh, &same), None);
    assert_eq!(applied(Some("/f/one\n"), &fresh, &same), None);
    same.settle(&fresh.fresh);
    assert!(same.is_empty());
}

#[test]
fn the_built_in_font_is_named_by_its_names() {
    for word in ["unifont", "unscii-8", "builtin", "builtin:unifont"] {
        assert!(builtin(word), "{word}");
    }
    assert!(!builtin("/usr/share/fonts/unifont.ttf"));
    assert_eq!(plain("/a//b/./c"), Some(PathBuf::from("/a/b/c")));
    assert_eq!(plain("a/b"), None);
    assert_eq!(plain("/a/../b"), None);
    assert_eq!(plain("/a\n/b"), None);
    // a control character would break `active`'s tab-separated fields
    // or reach the terminal raw
    for bad in [
        "/a\tb.ttf",
        "/a\u{1b}[31mb.ttf",
        "/a\rb",
        "/a\u{7f}b",
        "/a\u{9b}b",
    ] {
        assert_eq!(plain(bad), None, "{bad:?}");
    }
    assert_eq!(plain("/a b.ttf"), Some(PathBuf::from("/a b.ttf")));
}

// A look that names no vtlook draws the top level's, and one that
// names `default` draws the built-in vtlook. The top level itself is
// named only where a head draws it, because the head names no look or
// the text has no heads.
#[test]
fn a_look_that_takes_the_top_level_s_vtlook_uses_its_files() {
    let sepia = Path::new("/etc/vtium/looks/sepia.cube");
    let files = |name: &str| match name {
        "sepia" => vec![sepia.to_path_buf()],
        _ => Vec::new(),
    };
    let places = Places {
        frits: None,
        palettes: &[],
        vtlook: &files,
    };
    let looks = "vtlook = sepia\nversion = 1\n[look desk]\n[look plain]\nvtlook = default\n[look own]\nvtlook = sepia\n";
    let named = |conf: &str| -> Vec<Option<String>> {
        uses(sepia, conf, &places)
            .into_iter()
            .map(|u| u.look)
            .collect()
    };
    let some = |n: &str| Some(n.to_owned());
    assert_eq!(named(looks), [None, some("desk"), some("own")]);
    let headed = format!("{looks}[head h]\nlook = desk\n");
    assert_eq!(named(&headed), [some("desk"), some("own")]);
    let bare = format!("{looks}[head h]\nlook = desk\n[head g]\n");
    assert_eq!(named(&bare), [None, some("desk"), some("own")]);
    assert_eq!(
        uses(sepia, &headed, &places)
            .first()
            .map(|u| u.through.clone()),
        Some(Through::Vtlook("sepia".to_owned()))
    );
    assert_eq!(
        drawing_vtlook(&headed, "sepia"),
        [some("desk"), some("own")]
    );
    assert_eq!(
        users(sepia, &headed, &places).first().map(String::as_str),
        Some(
            "Look desk uses this color map through the vtlook sepia. From the next login after the install, look desk renders with no filter."
        )
    );
}

#[test]
fn a_look_that_names_a_color_scheme_is_warned_of() {
    // vtium reads a look's `palette = NAME` from NAME.palette in the
    // first folder that holds one, so the administrator's file
    // shadows the package's
    let root = std::env::temp_dir().join(format!("vtium-config-users-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (etc, pkg) = (root.join("etc"), root.join("pkg"));
    std::fs::create_dir_all(&etc).unwrap();
    std::fs::create_dir_all(&pkg).unwrap();
    for file in [pkg.join("dracula.palette"), pkg.join("top.palette")] {
        std::fs::write(file, "palette = 0=#000000\n").unwrap();
    }
    let conf = "palette = top\nversion = 1\n[look big]\npalette = dracula\n";
    let none = |_: &str| Vec::new();
    let places = Places {
        frits: None,
        palettes: &[&etc, &pkg],
        vtlook: &none,
    };
    let big = [
        "Look big uses this color scheme. From the next login after the install, look big renders without it.",
    ];
    assert_eq!(users(&pkg.join("dracula.palette"), conf, &places), big);
    // the text has no heads, so the console draws the top level
    assert_eq!(
        users(&pkg.join("top.palette"), conf, &places),
        [
            "The top level of vtium.conf uses this color scheme. From the next login after the install, each head that uses the top level renders without it."
        ]
    );
    // a path that is not the file vtium reads warns of nothing
    assert!(users(&pkg.join("dracula"), conf, &places).is_empty());
    assert!(users(&root.join("q/dracula.palette"), conf, &places).is_empty());
    // once the administrator's copy exists, vtium reads that one
    std::fs::write(etc.join("dracula.palette"), "palette = 0=#000000\n").unwrap();
    assert!(users(&pkg.join("dracula.palette"), conf, &places).is_empty());
    assert_eq!(users(&etc.join("dracula.palette"), conf, &places), big);
    let _ = std::fs::remove_dir_all(&root);
}

/// A machine with both folders of lists, under a temporary folder `R`:
/// the package's list vtium-fonts names `R/fonts/tiny5.bdf`, which is
/// installed, `R/fonts/cjk.ttc`, which is not, and `R/fonts/bold.ttf`;
/// the package's list other-fonts names `R/fonts/other.ttf`, and an empty
/// file of its name in root's folder turns it off.
struct Lists {
    root: PathBuf,
    start: Start,
}

impl Lists {
    fn new(name: &str) -> Lists {
        let root =
            std::env::temp_dir().join(format!("vtium-config-lists-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (package, admin) = (root.join("package-lists"), root.join("batch.d"));
        for dir in [&package, &admin, &root.join("fonts")] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let r = root.display();
        std::fs::write(
            package.join("vtium-fonts.conf"),
            format!("# vtium-fonts\n{r}/fonts/tiny5.bdf\n{r}/fonts/cjk.ttc\n{r}/fonts/bold.ttf\n"),
        )
        .unwrap();
        std::fs::write(
            package.join("other-fonts.conf"),
            format!("{r}/fonts/other.ttf\n"),
        )
        .unwrap();
        std::fs::write(admin.join("other-fonts.conf"), "").unwrap();
        for font in ["tiny5.bdf", "bold.ttf", "other.ttf", "own.ttf"] {
            std::fs::write(root.join("fonts").join(font), "a font\n").unwrap();
        }
        let euid = rustix::process::geteuid().as_raw();
        let start = Start {
            fresh: batch(&["/f/one"]),
            folders: Folders { package, admin },
            trust: vtium_settings::Trust::running_as(euid).below(&root),
        };
        Lists { root, start }
    }

    /// The path `p` under the machine's folder.
    fn path(&self, p: &str) -> PathBuf {
        self.root.join(p)
    }

    /// The path `p` under the machine's folder, as a line names it.
    fn line(&self, p: &str) -> String {
        self.path(p).display().to_string()
    }

    fn changes(&self, made: &[(&str, Change)]) -> Changes {
        let mut out = Changes::default();
        for (p, c) in made {
            out.set(self.path(p), Some(*c));
        }
        out
    }
}

impl Drop for Lists {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

// The batch holds what batch.conf and the package's list name, less the
// path a `-` line turns off; root's empty file turns the other package's
// list off, and the view says how root turned each path off.
#[test]
fn a_package_s_list_activates_its_paths_less_what_root_turns_off() {
    let m = Lists::new("view");
    let conf = format!(
        "{}\n-{}\n",
        m.line("fonts/own.ttf"),
        m.line("fonts/bold.ttf")
    );
    let view = View::new(Some(conf.as_bytes()), &m.start);
    let batch = view.batch();
    for held in ["fonts/own.ttf", "fonts/tiny5.bdf", "fonts/cjk.ttc"] {
        assert!(batch.holds(&m.path(held)), "{held}");
    }
    assert!(!batch.holds(&m.path("fonts/bold.ttf")));
    assert!(!batch.holds(&m.path("fonts/other.ttf")));
    assert_eq!(view.off(&m.path("fonts/bold.ttf")), Some(Off::BatchFile));
    assert_eq!(
        view.off(&m.path("fonts/other.ttf")),
        Some(Off::List(m.path("batch.d/other-fonts.conf")))
    );
    assert_eq!(view.off(&m.path("fonts/tiny5.bdf")), None);
    assert_eq!(view.off(&m.path("fonts/nowhere.ttf")), None);
    assert_eq!(
        list_lines(&view),
        [
            format!(
                "list\t{0}/package-lists/other-fonts.conf\tturned off\t{0}/batch.d/other-fonts.conf",
                m.root.display()
            ),
            format!(
                "list\t{0}/batch.d/other-fonts.conf\tread\t{0}/batch.d/other-fonts.conf",
                m.root.display()
            ),
            format!(
                "list\t{0}/package-lists/vtium-fonts.conf\tread\t{0}/package-lists/vtium-fonts.conf",
                m.root.display()
            ),
        ]
    );
}

// Deactivating a path that a list names writes a `-` line, since taking
// out a line batch.conf does not hold would leave the list activating it;
// a path only batch.conf names loses its line and gains no `-` line.
#[test]
fn deactivating_a_listed_path_writes_a_dash_line() {
    let m = Lists::new("deactivate");
    let conf = format!("# mine\n{}\n", m.line("fonts/own.ttf"));
    let changes = m.changes(&[
        ("fonts/tiny5.bdf", Change::Deactivate),
        ("fonts/own.ttf", Change::Deactivate),
    ]);
    assert_eq!(
        applied(Some(&conf), &m.start, &changes),
        Some(format!("# mine\n-{}\n", m.line("fonts/tiny5.bdf")))
    );
    // a `-` line already there is not written twice
    let off = format!("-{}\n", m.line("fonts/tiny5.bdf"));
    let again = m.changes(&[("fonts/tiny5.bdf", Change::Deactivate)]);
    assert_eq!(applied(Some(&off), &m.start, &again), None);
}

// Activating a path that a `-` line turns off takes the line out, and
// the list then activates the path, so no line of its own is added. A
// path whose list root turned off gets a line of its own, and so does a
// path no list names.
#[test]
fn activating_a_turned_off_path_takes_the_dash_line_out() {
    let m = Lists::new("activate");
    let conf = format!("# mine\n-{}\n", m.line("fonts/bold.ttf"));
    let changes = m.changes(&[
        ("fonts/bold.ttf", Change::Activate),
        ("fonts/other.ttf", Change::Activate),
        ("fonts/own.ttf", Change::Activate),
    ]);
    assert_eq!(
        applied(Some(&conf), &m.start, &changes),
        Some(format!(
            "# mine\n{}\n{}\n",
            m.line("fonts/other.ttf"),
            m.line("fonts/own.ttf")
        ))
    );
    // activating what a list already activates changes nothing
    let listed = m.changes(&[("fonts/tiny5.bdf", Change::Activate)]);
    assert_eq!(applied(Some("# mine\n"), &m.start, &listed), None);
}

// The first batch file written from the list of a fresh install holds the
// fresh list and the change, and none of the lists' paths, so removing a
// package still deactivates its fonts.
#[test]
fn the_first_write_from_a_fresh_install_copies_no_list() {
    let m = Lists::new("fresh");
    let changes = m.changes(&[("fonts/tiny5.bdf", Change::Deactivate)]);
    let text = applied(None, &m.start, &changes).expect("a new file");
    assert_eq!(
        text,
        format!("{HEADER}/f/one\n-{}\n", m.line("fonts/tiny5.bdf"))
    );
}
