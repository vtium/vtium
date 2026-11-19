//! Who may call whom in the TUI: a test that reads every source file under
//! `src/tui/` and fails on a path that crosses a line the TUI's structure
//! draws.
//!
//! Each tab of the TUI is a module under `src/tui/tabs/` that holds the
//! tab's state and its update. Privacy keeps a tab's fields to its own
//! module, but each tab's state type is public so that the model can hold
//! it, so privacy alone does not stop one tab from naming another. The
//! test checks five rules:
//!
//! - A tab's module names no other tab's module.
//! - Outside the tabs, only update, the view, the layout and the
//!   navigation module name a tab's module. Every other module of the TUI
//!   is shared by the tabs, and a path to a tab there would let one tab
//!   reach another through it.
//! - No module re-exports an item of a tab, with `pub use` or with a
//!   public type alias such as `pub type Plant = super::tabs::fonts::Fonts;`,
//!   which would let a tab reach another through the alias.
//! - No `macro_rules!` body names a tab's module.
//! - Only the view, the loop and the TUI's root module name the view's
//!   modules, so update, the layout and the tabs never draw.
//!
//! The test leaves two rules to the call signatures: a tab's update never
//! touches the navigation module's fields, and it never writes to the
//! terminal. Update hands a tab's update only the tab's own state and the
//! shared part, read-only, so neither is in reach. The test looks for no
//! path to `tui::nav` or `tui::port` in a tab's file.
//!
//! The test resolves each path the way rustc does for the forms the TUI
//! uses: `crate::`, `super::`, `self::`, a name a `use` brought in, and a
//! child module. It strips comments and string literals first, and it
//! expands grouped `use` trees, so `use super::{fonts, looks}` counts as
//! two paths.
//!
//! References:
//! - ADR-1206 D4, "who may call whom": the table of who may call whom.
//! - ADR-1206 D4, "Three rules close the paths the test cannot see": the
//!   rules on re-exports and macros, and that the test reads every file.
//!   A public type alias re-exports as `pub use` does, so the rule on
//!   re-exports covers it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The tabs' modules, each a file under `src/tui/tabs/`.
const TABS: [&str; 5] = ["flow", "fonts", "help", "looks", "settings"];

/// The files outside the tabs that may name a tab's module: update
/// (`App::event` and the `impl App` blocks it calls), the view, the
/// layout that update and the view both read, and the navigation module.
const NAMERS: [&str; 9] = [
    "tui/model.rs",
    "tui/flowedit.rs",
    "tui/console.rs",
    "tui/shot.rs",
    "tui/view.rs",
    "tui/flowdraw.rs",
    "tui/layout.rs",
    "tui/flowlayout.rs",
    "tui/nav.rs",
];

/// The view's modules, which draw.
const VIEWS: [&str; 2] = ["view", "flowdraw"];

/// The files that may name the view's modules: the view itself, the loop
/// that draws each frame and each picture of the console, and the TUI's
/// root module, which declares the modules.
const DRAWERS: [&str; 4] = [
    "tui/view.rs",
    "tui/flowdraw.rs",
    "tui/port.rs",
    "tui/mod.rs",
];

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    Ident(String),
    Sep,
    Open(char),
    Close(char),
    Comma,
    Semi,
    Star,
    Bang,
    Other,
}

/// `src` as tokens, with comments, string literals and character literals
/// left out. `$crate` becomes `crate`.
fn tokens(src: &str) -> Vec<Tok> {
    let c: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let ident = |ch: char| ch.is_alphanumeric() || ch == '_';
    while let Some(&ch) = c.get(i) {
        let next = c.get(i + 1).copied();
        if ch == '/' && next == Some('/') {
            while c.get(i).is_some_and(|&x| x != '\n') {
                i += 1;
            }
        } else if ch == '/' && next == Some('*') {
            let mut depth = 0usize;
            loop {
                match (c.get(i), c.get(i + 1)) {
                    (Some('/'), Some('*')) => {
                        depth += 1;
                        i += 2;
                    }
                    (Some('*'), Some('/')) => {
                        depth -= 1;
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    }
                    (Some(_), _) => i += 1,
                    (None, _) => break,
                }
            }
        } else if ch == '"' {
            i += 1;
            while let Some(&x) = c.get(i) {
                i += if x == '\\' { 2 } else { 1 };
                if x == '"' {
                    break;
                }
            }
        } else if ch == 'r'
            && matches!(next, Some('"' | '#'))
            && !c.get(i.wrapping_sub(1)).is_some_and(|&p| ident(p))
        {
            // a raw string: r"..." or r#"..."#, as many #s at each end
            i += 1;
            let mut hashes = 0;
            while c.get(i) == Some(&'#') {
                hashes += 1;
                i += 1;
            }
            i += 1;
            while i < c.len() {
                if c[i] == '"' && (1..=hashes).all(|h| c.get(i + h) == Some(&'#')) {
                    i += 1 + hashes;
                    break;
                }
                i += 1;
            }
        } else if ch == '\'' {
            // a character literal, or a lifetime, which is left as a name
            if next == Some('\\') {
                i += 2;
                while c.get(i).is_some_and(|&x| x != '\'') {
                    i += 1;
                }
                i += 1;
            } else if c.get(i + 2) == Some(&'\'') {
                i += 3;
            } else {
                i += 1;
            }
        } else if ch.is_whitespace() || (ch == '$' && next.is_some_and(ident)) {
            i += 1;
        } else if ident(ch) {
            let start = i;
            while c.get(i).is_some_and(|&x| ident(x)) {
                i += 1;
            }
            out.push(Tok::Ident(c[start..i].iter().collect()));
        } else {
            out.push(match ch {
                ':' if next == Some(':') => {
                    i += 1;
                    Tok::Sep
                }
                '{' | '(' | '[' => Tok::Open(ch),
                '}' | ')' | ']' => Tok::Close(ch),
                ',' => Tok::Comma,
                ';' => Tok::Semi,
                '*' => Tok::Star,
                '!' => Tok::Bang,
                _ => Tok::Other,
            });
            i += 1;
        }
    }
    out
}

/// The paths a `use` tree names, from `toks[*i]` to the end of the tree,
/// each with the name it brings into scope: its last segment, the name
/// after `as`, `*` for a glob, or the segment before a final `self`.
fn use_tree(toks: &[Tok], i: &mut usize, prefix: &[String], out: &mut Vec<(Vec<String>, String)>) {
    let mut path = prefix.to_vec();
    loop {
        match toks.get(*i) {
            Some(Tok::Ident(name)) if name == "as" => {
                *i += 1;
                if path.last().is_some_and(|l| l == "self") {
                    path.pop();
                }
                if let Some(Tok::Ident(alias)) = toks.get(*i) {
                    *i += 1;
                    out.push((path.clone(), alias.clone()));
                }
                return;
            }
            Some(Tok::Ident(name)) => {
                path.push(name.clone());
                *i += 1;
            }
            Some(Tok::Sep) => *i += 1,
            Some(Tok::Star) => {
                *i += 1;
                path.push("*".into());
                out.push((path, "*".into()));
                return;
            }
            Some(Tok::Open('{')) => {
                *i += 1;
                loop {
                    match toks.get(*i) {
                        Some(Tok::Close('}')) => {
                            *i += 1;
                            return;
                        }
                        Some(Tok::Comma) => *i += 1,
                        None => return,
                        _ => use_tree(toks, i, &path, out),
                    }
                }
            }
            _ => {
                if path.last().is_some_and(|l| l == "self") {
                    path.pop();
                }
                if let Some(last) = path.last().cloned() {
                    out.push((path, last));
                }
                return;
            }
        }
    }
}

/// One path a file names, resolved from the crate's root, and whether it
/// stands in a `pub use`, on the right of a public type alias, or in a
/// `macro_rules!` body.
#[derive(Clone, Debug)]
struct Named {
    path: Vec<String>,
    public_use: bool,
    public_type: bool,
    in_macro: bool,
}

/// Whether the item whose keyword is at `toks[i]` is public in any way:
/// `pub`, `pub(crate)`, `pub(super)` and the like.
fn public_before(toks: &[Tok], i: usize) -> bool {
    match i.checked_sub(1).map(|p| &toks[p]) {
        Some(Tok::Ident(p)) => p == "pub",
        Some(Tok::Close(')')) => {
            let open = toks[..i].iter().rposition(|t| *t == Tok::Open('('));
            open.and_then(|o| o.checked_sub(1))
                .is_some_and(|p| toks[p] == Tok::Ident("pub".into()))
        }
        _ => false,
    }
}

/// `segs` from the crate's root, as a path written in `module` resolves,
/// or `None` for a path into another crate.
fn resolve(
    segs: &[String],
    module: &[String],
    aliases: &BTreeMap<String, Vec<String>>,
    modules: &BTreeSet<Vec<String>>,
) -> Option<Vec<String>> {
    let first = segs.first()?;
    let mut base: Vec<String> = match first.as_str() {
        "crate" => return Some(segs[1..].to_vec()),
        "self" => return Some([module, &segs[1..]].concat()),
        "super" => module.to_vec(),
        _ => {
            if let Some(to) = aliases.get(first) {
                return Some([to.as_slice(), &segs[1..]].concat());
            }
            let child = [module, std::slice::from_ref(first)].concat();
            return modules.contains(&child).then(|| [module, segs].concat());
        }
    };
    let mut rest = segs;
    while rest.first().is_some_and(|s| s == "super") {
        base.pop();
        rest = &rest[1..];
    }
    Some([base.as_slice(), rest].concat())
}

/// Every path the file at `module` names, from `src`.
fn named(src: &str, module: &[String], modules: &BTreeSet<Vec<String>>) -> Vec<Named> {
    let toks = tokens(src);
    let mut out = Vec::new();
    let mut aliases: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // the inline modules open at this point, each with the depth of braces
    // its body opened at; and the macro body open, likewise
    let mut inline: Vec<(String, usize)> = Vec::new();
    let mut depth = 0usize;
    let mut macro_at: Option<usize> = None;
    // the right side of a public type alias, open until its `;`: a path
    // there re-exports what it names, as a `pub use` does, and so does a
    // bare name that a `use` brought in
    let mut public_type = false;
    let mut i = 0;
    while i < toks.len() {
        let here: Vec<String> = module
            .iter()
            .cloned()
            .chain(inline.iter().map(|(m, _)| m.clone()))
            .collect();
        match &toks[i] {
            Tok::Open('{') => {
                depth += 1;
                i += 1;
            }
            Tok::Close('}') => {
                depth = depth.saturating_sub(1);
                if inline.last().is_some_and(|(_, d)| *d == depth) {
                    inline.pop();
                }
                if macro_at == Some(depth) {
                    macro_at = None;
                }
                i += 1;
            }
            Tok::Ident(kw) if kw == "macro_rules" && toks.get(i + 1) == Some(&Tok::Bang) => {
                // the body's opening brace comes after the macro's name
                macro_at = Some(depth);
                i += 2;
            }
            Tok::Ident(kw) if kw == "mod" => {
                if let (Some(Tok::Ident(name)), Some(Tok::Open('{'))) =
                    (toks.get(i + 1), toks.get(i + 2))
                {
                    inline.push((name.clone(), depth));
                    depth += 1;
                    i += 3;
                } else {
                    i += 1;
                }
            }
            Tok::Semi => {
                public_type = false;
                i += 1;
            }
            Tok::Ident(kw) if kw == "type" && public_before(&toks, i) => {
                // `pub type Name = path;`: the alias's own name is no path
                public_type = true;
                i += 2;
            }
            Tok::Ident(kw) if kw == "use" => {
                // `pub use`, `pub(crate) use` and the like re-export
                let public_use = public_before(&toks, i);
                i += 1;
                let mut tree = Vec::new();
                use_tree(&toks, &mut i, &[], &mut tree);
                for (segs, alias) in tree {
                    let Some(path) = resolve(&segs, &here, &aliases, modules) else {
                        continue;
                    };
                    if alias != "*" {
                        aliases.insert(alias, path.clone());
                    }
                    out.push(Named {
                        path,
                        public_use,
                        public_type: false,
                        in_macro: macro_at.is_some(),
                    });
                }
            }
            Tok::Ident(first) if toks.get(i + 1) == Some(&Tok::Sep) => {
                let mut segs = vec![first.clone()];
                i += 1;
                while toks.get(i) == Some(&Tok::Sep) {
                    match toks.get(i + 1) {
                        Some(Tok::Ident(s)) => {
                            segs.push(s.clone());
                            i += 2;
                        }
                        _ => break,
                    }
                }
                if let Some(path) = resolve(&segs, &here, &aliases, modules) {
                    out.push(Named {
                        path,
                        public_use: false,
                        public_type,
                        in_macro: macro_at.is_some(),
                    });
                }
            }
            Tok::Ident(name) if public_type => {
                if let Some(path) = aliases.get(name) {
                    out.push(Named {
                        path: path.clone(),
                        public_use: false,
                        public_type,
                        in_macro: macro_at.is_some(),
                    });
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

/// The tab a path leads into, if any: `tui::tabs::<tab>`, or every tab for
/// a glob of `tui::tabs`.
fn tabs_of(path: &[String]) -> Vec<&'static str> {
    match path {
        [tui, tabs, rest @ ..] if tui == "tui" && tabs == "tabs" => match rest.first() {
            Some(t) if t == "*" => TABS.to_vec(),
            Some(t) => TABS.iter().copied().filter(|n| n == t).collect(),
            None => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// Each `.rs` file under `dir`, as its path from `src/`.
fn sources(dir: &Path, src: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("the TUI's source directory") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            sources(&path, src, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path.strip_prefix(src).expect("under src").to_path_buf());
        }
    }
}

/// The module a file under `src/` defines: `tui/tabs/flow.rs` is
/// `tui::tabs::flow`, and `tui/mod.rs` is `tui`.
fn module_of(file: &Path) -> Vec<String> {
    let mut segs: Vec<String> = file
        .with_extension("")
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    if segs.last().is_some_and(|s| s == "mod") {
        segs.pop();
    }
    segs
}

/// The rules' findings over the TUI's files, each a line naming the file,
/// the rule and the path; and every path each file names, for the test's
/// positive controls.
fn check(files: &[(String, String)]) -> (Vec<String>, BTreeMap<String, Vec<Named>>) {
    let modules: BTreeSet<Vec<String>> =
        files.iter().map(|(f, _)| module_of(Path::new(f))).collect();
    let mut bad = Vec::new();
    let mut all = BTreeMap::new();
    for (file, src) in files {
        let module = module_of(Path::new(file));
        let own_tab = tabs_of(&module).first().copied();
        let found = named(src, &module, &modules);
        for n in &found {
            let path = n.path.join("::");
            for tab in tabs_of(&n.path) {
                if own_tab.is_some_and(|own| own != tab) {
                    bad.push(format!("{file}: a tab names another tab: {path}"));
                }
                if own_tab.is_none() && !NAMERS.contains(&file.as_str()) {
                    bad.push(format!("{file}: a shared module names a tab: {path}"));
                }
                if n.public_use {
                    bad.push(format!("{file}: a pub use re-exports a tab's item: {path}"));
                }
                if n.public_type {
                    bad.push(format!("{file}: a pub type names a tab's item: {path}"));
                }
                if n.in_macro {
                    bad.push(format!("{file}: a macro_rules! body names a tab: {path}"));
                }
            }
            let draws = matches!(n.path.as_slice(), [tui, view, ..]
                if tui == "tui" && VIEWS.contains(&view.as_str()))
                || n.path == ["tui", "console", "draw"];
            if draws && !DRAWERS.contains(&file.as_str()) {
                bad.push(format!(
                    "{file}: update, a layout or a tab names the view: {path}"
                ));
            }
        }
        all.insert(file.clone(), found);
    }
    bad.sort();
    bad.dedup();
    (bad, all)
}

/// The TUI's files, each its path from `src/` and its text.
fn tui_files() -> Vec<(String, String)> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&src.join("tui"), &src, &mut files);
    files.sort();
    files
        .into_iter()
        .map(|f| {
            let text = std::fs::read_to_string(src.join(&f)).expect("a source file");
            (f.to_string_lossy().into_owned(), text)
        })
        .collect()
}

#[test]
fn no_module_of_the_tui_crosses_the_lines_of_who_may_call_whom() {
    let files = tui_files();
    let (bad, all) = check(&files);
    // positive controls: the test reads every tab and the paths it knows
    // are there, so a finding of nothing means the rules hold
    for tab in TABS {
        assert!(
            all.contains_key(&format!("tui/tabs/{tab}.rs")),
            "the test reads tabs/{tab}.rs"
        );
    }
    let names = |file: &str, path: &[&str]| {
        all.get(file).is_some_and(|ns| {
            ns.iter().any(|n| {
                n.path
                    .starts_with(&path.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>())
            })
        })
    };
    assert!(names("tui/model.rs", &["tui", "tabs", "flow", "Flow"]));
    assert!(names("tui/layout.rs", &["tui", "model", "App"]));
    assert!(names("tui/tabs/looks.rs", &["tui", "shot", "Preview"]));
    assert!(names("tui/port.rs", &["tui", "view"]));
    assert!(names(
        "tui/tabs/flow.rs",
        &["tui", "flowlayout", "layout_of"]
    ));
    assert!(bad.is_empty(), "who may call whom:\n{}", bad.join("\n"));
}

/// The rules catch each kind of crossing they claim to, on files written
/// to cross them: a tab that names another through `super` and through a
/// group, a shared module that names a tab, a re-export with `pub use` and
/// with a public type alias, a macro, and update calling the view. A
/// private alias in a file that may name a tab is no crossing.
#[test]
fn each_rule_catches_the_crossing_it_names() {
    let file = |path: &str, text: &str| (path.to_owned(), text.to_owned());
    let files = vec![
        file(
            "tui/mod.rs",
            "pub mod tabs; pub mod model; pub mod view; pub mod words;",
        ),
        file(
            "tui/tabs/mod.rs",
            "pub mod flow; pub mod fonts; pub mod looks;",
        ),
        file("tui/tabs/flow.rs", "use super::fonts::Fonts;"),
        file(
            "tui/tabs/looks.rs",
            "use super::{self as tabs, looks, flow};\nfn f() { let s = \"super::fonts\"; }",
        ),
        file(
            "tui/tabs/fonts.rs",
            "// super::flow::Flow in a comment is no path\nfn f() {}",
        ),
        file(
            "tui/words.rs",
            "pub fn f() -> crate::tui::tabs::looks::Looks { todo!() }",
        ),
        file(
            "tui/model.rs",
            "pub use super::tabs::flow::Flow;\nfn f() { super::view::screen(); }",
        ),
        file(
            "tui/view.rs",
            "macro_rules! m { () => { $crate::tui::tabs::fonts::X }; }",
        ),
        file(
            "tui/layout.rs",
            "use super::tabs::looks::Looks;\npub type Plant = super::tabs::fonts::Fonts;\npub(crate) type Seen = Looks;\ntype Own = super::tabs::flow::Flow;",
        ),
    ];
    let (bad, _) = check(&files);
    let expected = [
        "tui/layout.rs: a pub type names a tab's item: tui::tabs::fonts::Fonts",
        "tui/layout.rs: a pub type names a tab's item: tui::tabs::looks::Looks",
        "tui/model.rs: a pub use re-exports a tab's item: tui::tabs::flow::Flow",
        "tui/model.rs: update, a layout or a tab names the view: tui::view::screen",
        "tui/tabs/flow.rs: a tab names another tab: tui::tabs::fonts::Fonts",
        "tui/tabs/looks.rs: a tab names another tab: tui::tabs::flow",
        "tui/view.rs: a macro_rules! body names a tab: tui::tabs::fonts::X",
        "tui/words.rs: a shared module names a tab: tui::tabs::looks::Looks",
    ];
    assert_eq!(bad, expected);
}
