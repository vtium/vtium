//! Links sd-bus for the `logind` feature (ADR-1400's L2 layer): libsystemd,
//! else libelogind, whichever pkg-config finds first. Without the feature
//! it links nothing.

#[cfg(feature = "logind")]
#[path = "src/logind/library.rs"]
mod library;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=src/logind/library.rs");
    #[cfg(feature = "logind")]
    if let Err(message) = library::choose(|package| {
        pkg_config::Config::new()
            .atleast_version(package.min_version)
            .probe(package.name)
    }) {
        eprintln!("{message}");
        std::process::exit(1);
    }
}
