//! `make tier2` builds tier 2 with its `gpu` feature, so every run judges
//! the GPU path's screens too: a build without it adds no `@gpu` case, and
//! the run would pass with the GPU path unjudged. It runs `make
//! tier2-fonts` first, so the outline screens find their fonts.

use std::fs;
use std::path::PathBuf;

#[test]
fn make_tier2_builds_tier2_with_the_gpu_path() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Makefile");
    let make = fs::read_to_string(&path).unwrap();
    let target = make
        .split("\ntier2:")
        .nth(1)
        .expect("the Makefile has a tier2 target");
    let recipe: Vec<&str> = target
        .lines()
        .skip(1)
        .take_while(|l| l.starts_with('\t'))
        .collect();
    let build = recipe
        .iter()
        .find(|l| l.contains("cargo build") && l.contains("vtium-tier2"))
        .expect("the tier2 target builds tier 2");
    assert!(
        build.contains("--features gpu"),
        "make tier2 builds tier 2 without the GPU path: {build:?}"
    );
}

#[test]
fn make_tier2_fetches_the_outline_fonts_first() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Makefile");
    let make = fs::read_to_string(&path).unwrap();
    let rule = make
        .lines()
        .find(|l| l.starts_with("tier2:"))
        .expect("the Makefile has a tier2 target");
    let prerequisites = rule
        .trim_start_matches("tier2:")
        .split("##")
        .next()
        .unwrap_or_default();
    assert!(
        prerequisites.split_whitespace().any(|p| p == "tier2-fonts"),
        "make tier2 does not fetch the outline fonts first: {rule:?}"
    );
    let fetch = make
        .split("\ntier2-fonts:")
        .nth(1)
        .expect("the Makefile has a tier2-fonts target");
    assert!(
        fetch
            .lines()
            .skip(1)
            .take_while(|l| l.starts_with('\t'))
            .any(|l| l.contains("tier2 fetch-fonts")),
        "make tier2-fonts does not run tier2 fetch-fonts"
    );
}
