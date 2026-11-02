//! Declares the `fuzzing` configuration cargo-fuzz builds the crate with,
//! under which `sandbox::Locked::unsandboxed` exists for the fuzz targets.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(fuzzing)");
}
