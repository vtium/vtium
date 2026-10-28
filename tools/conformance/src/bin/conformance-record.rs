#![forbid(unsafe_code)]
//! `conformance-record OUT [NAME...]`: records real programs through a pty
//! for the corpus (`vtium_conformance::record`), each into
//! `OUT/<name>.bin` with its cuts in `OUT/<name>.cuts`, and prints one
//! `<name> <cols> <rows>` line per recording, which belongs in cases.list.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use vtium_conformance::record::{self, RECORDINGS, Recording, Scratch};

const USAGE: &str = "usage: conformance-record OUT [NAME...]";

fn run(out: &Path, recs: &[&Recording]) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    for rec in recs {
        let tmp = std::env::temp_dir();
        let home =
            Scratch::new(&tmp, "vtium-record").map_err(|e| format!("{}: {e}", tmp.display()))?;
        let (data, cuts) = record::demo_files(home.path())
            .and_then(|cwd| record::record(rec, home.path(), &cwd))
            .map_err(|e| format!("{}: {e}", rec.name))?;
        drop(home);
        let write = |path: PathBuf, bytes: &[u8]| {
            std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))
        };
        write(out.join(format!("{}.bin", rec.name)), &data)?;
        let text: String = cuts.iter().map(|c| format!("{c}\n")).collect();
        write(out.join(format!("{}.cuts", rec.name)), text.as_bytes())?;
        println!("{} {} {}", rec.name, rec.cols, rec.rows);
        eprintln!("{}: {} bytes", rec.name, data.len());
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((out, names)) = args.split_first() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let recs: Result<Vec<&Recording>, String> = if names.is_empty() {
        Ok(RECORDINGS.iter().collect())
    } else {
        names
            .iter()
            .map(|n| record::find(n).ok_or_else(|| format!("no recording {n}\n{USAGE}")))
            .collect()
    };
    match recs.and_then(|recs| run(Path::new(out), &recs)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("conformance-record: {why}");
            ExitCode::from(2)
        }
    }
}
