//! Builds the C side: extracts the C reference's parser from the pinned
//! libtsm commit, refusing a moved range, and compiles it with
//! `harness/shim.c`. Embeds the C reference's conformance inputs at the pin
//! as the corpus.
//!
//! The commit is read from `$LIBTSM` (default: `../libtsm` beside this
//! repository) with `git show`, nothing built or written there. Without the
//! commit there, the pin's URL is cloned into the build's `OUT_DIR`.

#[allow(
    dead_code,
    reason = "the self-test's part of the module goes unused here"
)]
#[path = "src/pin.rs"]
mod pin;

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

fn main() {
    if let Err(e) = build() {
        eprintln!("parse-oracle: refused: {e}");
        std::process::exit(1);
    }
}

fn build() -> Result<(), String> {
    let here = PathBuf::from(env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?);
    let out = PathBuf::from(env::var("OUT_DIR").map_err(|e| e.to_string())?);
    for path in ["pin.tsv", "extract.tsv", "harness/shim.c", "src/pin.rs"] {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-env-changed=LIBTSM");

    let read = |name: &str| fs::read_to_string(here.join(name)).map_err(|e| format!("{name}: {e}"));
    let pin = pin::read_pin(&read("pin.tsv")?)?;
    let ranges = pin::read_ranges(&read("extract.tsv")?)?;
    let libtsm = env::var_os("LIBTSM")
        .map(PathBuf::from)
        .unwrap_or_else(|| here.join("../../../libtsm"));
    let gitdir = pin::source(&libtsm, &out.join("libtsm.git"), &pin)?;
    let gitdir = gitdir.canonicalize().map_err(|e| e.to_string())?;

    let extract = out.join("extract");
    pin::extract(&gitdir, &pin.commit, &ranges, &extract)?;
    cc::Build::new()
        .file(here.join("harness/shim.c"))
        .include(&extract)
        .std("gnu11")
        .warnings(true)
        .extra_warnings(true)
        .warnings_into_errors(true)
        .compile("parse_oracle_shim");

    let corpus_dir = out.join("corpus");
    fs::create_dir_all(&corpus_dir).map_err(|e| e.to_string())?;
    let mut corpus = String::from("pub static CORPUS: &[(&str, &[u8])] = &[\n");
    for (i, (name, bytes)) in pin::corpus(&gitdir, &pin.commit)?.iter().enumerate() {
        let file = corpus_dir.join(format!("{i}.bin"));
        fs::write(&file, bytes).map_err(|e| e.to_string())?;
        writeln!(
            corpus,
            "    ({name:?}, include_bytes!({:?})),",
            file.display().to_string()
        )
        .map_err(|e| e.to_string())?;
    }
    corpus.push_str("];\n");
    fs::write(out.join("corpus.rs"), corpus).map_err(|e| e.to_string())?;

    println!("cargo:rustc-env=ORACLE_LIBTSM_GIT={}", gitdir.display());
    println!("cargo:rustc-env=ORACLE_COMMIT={}", pin.commit);
    Ok(())
}
