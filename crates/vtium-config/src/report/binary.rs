//! The vtium console binaries: each one's version, revision and features,
//! from the lines its `--version` prints, or from marker text in an older
//! binary whose `--version` prints only its version.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use super::Machine;
use super::read::{BINARY_MAX, Ran};

/// The vtium console binaries the report looks for, and whether one that
/// is missing is named.
const BINARIES: [(&str, bool); 2] = [
    ("/usr/bin/vtium", true),
    ("/usr/local/bin/vtium-dev", false),
];

/// The features a vtium build adds on top of minspec, as
/// crates/vtium/Cargo.toml names them (a test holds them equal).
pub const FEATURES: [&str; 4] = ["bidi", "gpu", "images", "logind"];

/// For each feature that has one, a line of text that only a build with
/// the feature carries, with a `|` cut into it: what vtium says when it
/// cannot open its image helper, from code built only with `images`, and
/// what it says when logind is not on the bus, from code built only with
/// `logind`. [`marker`] takes the `|` out only when the report searches a
/// binary, so neither this source nor vtium-config's binary holds the line
/// itself, which must stay in the one place a build with the feature
/// carries it (the development install's script and its tests rely on
/// that for `images`). The tests hold each marker to the file it comes
/// from, and check that vtium-config's binary does not carry it.
pub const MARKERS: [(&str, &str); 2] = [
    ("images", "cannot open the image| helper "),
    ("logind", "logind is not on| the bus ("),
];

/// The marker `cut` with its `|` taken out, as a build with its feature
/// carries it.
pub fn marker(cut: &str) -> String {
    cut.replace('|', "")
}

/// What `--version` says of a binary: its version line, the revision and
/// the features where it lists them.
struct Version {
    line: String,
    revision: Option<String>,
    features: Option<Vec<String>>,
}

fn version(ran: &Ran) -> Version {
    let mut lines = ran.out.lines();
    let line = lines.next().unwrap_or_default().trim().to_owned();
    let mut v = Version {
        line,
        revision: None,
        features: None,
    };
    for l in lines {
        if let Some(r) = l.strip_prefix("revision ") {
            v.revision = Some(r.trim().to_owned());
        } else if let Some(f) = l.strip_prefix("features") {
            v.features = Some(
                f.split_whitespace()
                    .filter(|w| *w != "none")
                    .map(str::to_owned)
                    .collect(),
            );
        }
    }
    v
}

/// Which of [`MARKERS`] the file at `path` carries, reading at most
/// [`BINARY_MAX`] bytes a block at a time.
fn carries(path: &Path) -> io::Result<Vec<bool>> {
    let markers: Vec<String> = MARKERS.iter().map(|(_, m)| marker(m)).collect();
    let longest = markers.iter().map(String::len).max().unwrap_or(0);
    let mut found = vec![false; markers.len()];
    let mut file = File::open(path)?.take(BINARY_MAX);
    let mut window: Vec<u8> = Vec::new();
    let mut block = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut block)?;
        if n == 0 {
            break;
        }
        window.extend_from_slice(block.get(..n).unwrap_or_default());
        for (f, marker) in found.iter_mut().zip(&markers) {
            *f = *f || window.windows(marker.len()).any(|w| w == marker.as_bytes());
        }
        // keep what a marker could start in and end in the next block
        let keep = window.len().saturating_sub(longest.saturating_sub(1));
        window.drain(..keep);
    }
    Ok(found)
}

/// What the features line says of a binary that `--version` printed
/// `v` for.
fn features(path: &Path, v: &Version) -> String {
    if let Some(listed) = &v.features {
        let listed = if listed.is_empty() {
            "none".to_owned()
        } else {
            listed.join(" ")
        };
        return format!("{listed}, as its --version lists them");
    }
    let found = match carries(path) {
        Ok(found) => found,
        Err(e) => return format!("unknown: {e}"),
    };
    let mut said: Vec<String> = MARKERS
        .iter()
        .zip(found)
        .map(|((name, _), on)| format!("{name} {}", if on { "yes" } else { "no" }))
        .collect();
    said.extend(
        FEATURES
            .iter()
            .filter(|f| !MARKERS.iter().any(|(m, _)| m == *f))
            .map(|f| format!("{f} unknown")),
    );
    format!(
        "{}, found in the binary, because its --version lists none",
        said.join(", ")
    )
}

/// The vtium part: each console binary, its version and features, and
/// vtium-config's own version.
pub fn vtium(m: &Machine, _: &super::units::Units) -> String {
    let mut s = String::new();
    for (abs, named) in BINARIES {
        let path = m.path(abs);
        if !path.is_file() {
            if named {
                s.push_str(&format!("{abs}: not installed\n"));
            }
            continue;
        }
        s.push_str(&format!("{abs}\n"));
        match m.run(&path, &["--version"]) {
            Ok(ran) => {
                let v = version(&ran);
                s.push_str(&format!("  version: {}\n", v.line));
                if let Some(r) = &v.revision {
                    s.push_str(&format!("  revision: {r}\n"));
                }
                s.push_str(&format!("  features: {}\n", features(&path, &v)));
            }
            Err(e) => s.push_str(&format!("  cannot run it: {e}\n")),
        }
    }
    s.push_str(&format!("vtium-config: {}\n", env!("CARGO_PKG_VERSION")));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [`FEATURES`] is crates/vtium's own list of features but `default`,
    /// so a feature added there fails here until the report knows it.
    #[test]
    fn the_features_are_vtium_s() {
        let manifest = include_str!("../../../vtium/Cargo.toml");
        let section = manifest
            .split("\n[features]\n")
            .nth(1)
            .and_then(|s| s.split("\n[").next())
            .unwrap_or_default();
        let mut named: Vec<&str> = section
            .lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| l.split_once('=').map(|(k, _)| k.trim()))
            .filter(|k| !k.is_empty() && *k != "default")
            .collect();
        named.sort_unstable();
        assert_eq!(named, FEATURES);
    }

    #[test]
    fn version_reads_the_revision_and_the_features_where_listed() {
        let ran = |out: &str| Ran {
            code: Some(0),
            out: out.to_owned(),
            err: String::new(),
        };
        let v = version(&ran(
            "vtium 1.0\nrevision v1-2-gabc\nfeatures images logind\n",
        ));
        assert_eq!(v.line, "vtium 1.0");
        assert_eq!(v.revision.as_deref(), Some("v1-2-gabc"));
        assert_eq!(
            v.features,
            Some(vec!["images".to_owned(), "logind".to_owned()])
        );
        let v = version(&ran("vtium 1.0\nfeatures none\n"));
        assert_eq!(v.features, Some(vec![]));
        let v = version(&ran("vtium 1.0\n"));
        assert_eq!((v.revision, v.features), (None, None));
    }
}
