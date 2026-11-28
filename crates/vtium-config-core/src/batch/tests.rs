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
    let fresh = batch(&["/f/one", "/f/two"]);
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
    same.settle(&fresh);
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
