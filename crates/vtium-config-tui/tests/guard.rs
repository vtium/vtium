//! The guard over the crates of vtium-config's TUI. The shell holds the
//! tabs only as trait objects that the binary hands it, and each tab is a
//! crate of its own, so no crate may reach into another tab:
//!
//! - vtium-config-tui's manifest names no tab crate, and neither does its
//!   source; it names neither vtium-config, the binary, nor any crate
//!   that would bring a tab with it.
//! - each `crates/vtium-config-tab-*` manifest names vtium-config-widgets
//!   and vtium-config-core, and never another tab crate, the shell or
//!   vtium-config; and no tab's source names another tab's crate or the
//!   shell.
//!
//! The test reads the manifests and the sources from the disk when it
//! runs, so a tab crate that lands later is checked with no change here.
//!
//! References:
//! - ADR-1206 D4, "The rules hold by Rust's privacy, by the call
//!   signatures and by one test": the test reads the source and fails on
//!   a path from one tab into another.
//! - ADR-1206 D9, "The manifests carry D4's rule": the manifests name no
//!   other tab and not the shell, and the guard reads them.

#![expect(clippy::expect_used, reason = "a test fails by panicking")]

use std::path::{Path, PathBuf};

/// The `crates/` folder.
fn crates() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crates folder")
        .to_path_buf()
}

/// The names of the crates `manifest` depends on, in any table of
/// dependencies: `[dependencies]`, `[dev-dependencies]`,
/// `[build-dependencies]` and their `target` forms.
fn dependencies(manifest: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_deps = false;
    for line in manifest.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            let table = line.trim_matches(|c| c == '[' || c == ']');
            in_deps = table.ends_with("dependencies");
            // a table of one dependency, as `[dependencies.vtium-config]`
            if let Some((_, name)) = table.split_once("dependencies.") {
                out.push(name.to_owned());
                in_deps = false;
            }
            continue;
        }
        if in_deps && let Some((key, _)) = line.split_once('=') {
            let key = key.trim();
            let name = key.split('.').next().unwrap_or(key).trim_matches('"');
            out.push(name.to_owned());
        }
    }
    out
}

/// Each `crates/vtium-config-tab-*` folder, by its crate's name.
fn tab_crates() -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = std::fs::read_dir(crates())
        .expect("the crates folder")
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.starts_with("vtium-config-tab-")
                .then(|| (name, e.path()))
        })
        .collect();
    out.sort();
    out
}

/// Every `.rs` file under `dir`.
fn sources(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.filter_map(Result::ok) {
            let p = e.path();
            if p.is_dir() {
                todo.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// The lines of the source files under `dir` that name a crate `names`
/// picks out, by its path in Rust, as `vtium_config_tab_fonts`.
fn naming(dir: &Path, names: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut out = Vec::new();
    for file in sources(dir) {
        let text = std::fs::read_to_string(&file).expect("a source file");
        for (n, line) in text.lines().enumerate() {
            let found = line
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .any(names);
            if found {
                out.push(format!("{}:{}: {}", file.display(), n + 1, line.trim()));
            }
        }
    }
    out
}

/// What is wrong with the shell's crate.
fn shell_faults() -> Vec<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).expect("the shell's manifest");
    let mut faults: Vec<String> = dependencies(&manifest)
        .into_iter()
        .filter(|d| d.starts_with("vtium-config-tab-") || d == "vtium-config")
        .map(|d| format!("vtium-config-tui's manifest names {d}"))
        .collect();
    faults.extend(
        naming(&dir.join("src"), &|w| {
            w.starts_with("vtium_config_tab_") || w == "vtium_config"
        })
        .into_iter()
        .map(|l| format!("the shell's source names a tab or the binary: {l}")),
    );
    faults
}

/// What is wrong with the tab crates.
fn tab_faults() -> Vec<String> {
    let mut faults = Vec::new();
    for (name, dir) in tab_crates() {
        let manifest =
            std::fs::read_to_string(dir.join("Cargo.toml")).unwrap_or_else(|_| String::new());
        let deps = dependencies(&manifest);
        for needed in ["vtium-config-widgets", "vtium-config-core"] {
            if !deps.iter().any(|d| d == needed) {
                faults.push(format!("{name}'s manifest does not name {needed}"));
            }
        }
        for d in &deps {
            let other_tab = d.starts_with("vtium-config-tab-") && *d != name;
            if other_tab || d == "vtium-config-tui" || d == "vtium-config" {
                faults.push(format!("{name}'s manifest names {d}"));
            }
        }
        let own = name.replace('-', "_");
        faults.extend(
            naming(&dir.join("src"), &|w| {
                w.starts_with("vtium_config_tab_") && w != own
                    || w == "vtium_config_tui"
                    || w == "vtium_config"
            })
            .into_iter()
            .map(|l| format!("{name}'s source names another tab or the shell: {l}")),
        );
    }
    faults
}

#[test]
fn the_shell_names_no_tab_crate_and_not_the_binary() {
    let faults = shell_faults();
    assert!(faults.is_empty(), "{}", faults.join("\n"));
}

#[test]
fn each_tab_crate_names_the_widgets_and_the_core_and_no_other_tab_or_the_shell() {
    let faults = tab_faults();
    assert!(faults.is_empty(), "{}", faults.join("\n"));
}

#[test]
fn the_manifest_reader_finds_every_table_of_dependencies() {
    let manifest = r#"
[package]
name = "x"

[dependencies]
vtium-config-core.workspace = true
vtium-config-widgets = { workspace = true }
# vtium-config-tab-looks = { path = "../vtium-config-tab-looks" }

[target.'cfg(unix)'.dev-dependencies]
"vtium-config-tab-fonts" = { path = "../vtium-config-tab-fonts" }

[dependencies.vtium-config]
path = "../vtium-config"

[features]
fixture = []
"#;
    assert_eq!(
        dependencies(manifest),
        [
            "vtium-config-core",
            "vtium-config-widgets",
            "vtium-config-tab-fonts",
            "vtium-config",
        ]
    );
}
