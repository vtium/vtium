//! Defects planted in a copy of vtium-parse, each of which the run must
//! turn red on. The self-test builds the tool against each copy and runs it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A plant: one exact replacement in a file of vtium-parse.
pub struct Plant {
    pub name: &'static str,
    file: &'static str,
    from: &'static str,
    to: &'static str,
}

pub const PLANTS: &[Plant] = &[
    Plant {
        name: "gr-mapping-drops-csi-ignore",
        file: "src/parser.rs",
        from: "                | State::CsiInt\n                | State::CsiIgnore\n",
        to: "                | State::CsiInt\n",
    },
    Plant {
        name: "csi-int-takes-digits",
        file: "src/parser.rs",
        from: "            0x30..=0x3f => (Some(S::CsiIgnore), A::None),\n            0x40..=0x7e => (Some(S::Ground), A::CsiDispatch),\n            _ => (Some(S::CsiIgnore), A::None),\n        },\n        S::CsiIgnore",
        to: "            0x30..=0x3f => (None, A::None),\n            0x40..=0x7e => (Some(S::Ground), A::CsiDispatch),\n            _ => (Some(S::CsiIgnore), A::None),\n        },\n        S::CsiIgnore",
    },
    Plant {
        name: "dcs-run-takes-del",
        file: "src/parser.rs",
        from: "    b < 0x7f && b != 0x18",
        to: "    b <= 0x7f && b != 0x18",
    },
    Plant {
        name: "osc-max-off-by-one",
        file: "src/bounds.rs",
        from: "pub const OSC_MAX: usize = OSC.value;",
        to: "pub const OSC_MAX: usize = OSC.value - 1;",
    },
    Plant {
        name: "xtgettcap-max-off-by-one",
        file: "src/bounds.rs",
        from: "pub const XTGETTCAP_MAX: usize = XTGETTCAP.value;",
        to: "pub const XTGETTCAP_MAX: usize = XTGETTCAP.value - 1;",
    },
    Plant {
        name: "subparams-off-by-one",
        file: "src/params.rs",
        from: "if usize::from(*sub_len) < SUBPARAMS_MAX {",
        to: "if usize::from(*sub_len) + 1 < SUBPARAMS_MAX {",
    },
    Plant {
        name: "value-cap-off-by-one",
        file: "src/params.rs",
        from: "v if v > PARAM_VALUE_CAP => v,",
        to: "v if v >= PARAM_VALUE_CAP => v,",
    },
    Plant {
        name: "utf8-accepts-c0-c1",
        file: "src/utf8.rs",
        from: "            0xc2..=0xdf => (1, 0x80, 0xbf),",
        to: "            0xc0..=0xdf => (1, 0x80, 0xbf),",
    },
    Plant {
        name: "marker-as-intermediate",
        file: "src/parser.rs",
        from: "                    self.params.set_marker(b);",
        to: "                    let _ = self.intermediates.push(b);",
    },
    // shows only with pauses
    Plant {
        name: "pause-ignored",
        file: "src/parser.rs",
        from: "            if consumer.paused() {",
        to: "            if consumer.paused() && rest.is_empty() {",
    },
    // shows only with resets
    Plant {
        name: "reset-keeps-pending-end",
        file: "src/parser.rs",
        from: "        self.pending_end = None;\n        self.state = State::Ground;",
        to: "        self.state = State::Ground;",
    },
    // shows only with a mode switch inside a call
    Plant {
        name: "mode-read-once-per-call",
        file: "src/parser.rs",
        from: "        let mut rest = bytes;\n        while let Some((&b, after_one)) = rest.split_first() {\n            let mode = consumer.input_mode();\n",
        to: "        let mut rest = bytes;\n        let mode = consumer.input_mode();\n        while let Some((&b, after_one)) = rest.split_first() {\n",
    },
];

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    if from.is_dir() {
        fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
        for entry in fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
            let entry = entry.map_err(|e| e.to_string())?;
            copy(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        if let Some(dir) = to.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        fs::copy(from, to)
            .map(|_| ())
            .map_err(|e| format!("{} to {}: {e}", from.display(), to.display()))
    }
}

/// Builds the tool against a copy of vtium-parse with `plant`, under
/// `out/plants/`, and returns the binary.
pub fn build(plant: &Plant, out: &Path) -> Result<PathBuf, String> {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo = here.join("../..");
    let root = out.join("plants").join(plant.name);
    let _ = fs::remove_dir_all(&root);
    let tool = root.join("tools/parse-oracle");
    for name in [
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "pin.tsv",
        "extract.tsv",
        "departures.tsv",
        "harness",
        "src",
    ] {
        copy(&here.join(name), &tool.join(name))?;
    }
    let parse = root.join("crates/vtium-parse");
    for name in ["Cargo.toml", "README.md", "src"] {
        copy(
            &repo.join("crates/vtium-parse").join(name),
            &parse.join(name),
        )?;
    }
    copy(&repo.join("Cargo.toml"), &root.join("Cargo.toml"))?;

    let path = parse.join(plant.file);
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if text.matches(plant.from).count() != 1 {
        return Err(format!(
            "plant {}: its text is not in {} exactly once",
            plant.name, plant.file
        ));
    }
    fs::write(&path, text.replace(plant.from, plant.to)).map_err(|e| e.to_string())?;

    let target = out.join("plants/target");
    let status = Command::new("cargo")
        .args(["build", "--quiet", "--release", "--manifest-path"])
        .arg(tool.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", &target)
        // a plant may leave code unused; its warnings are not the test's
        .env("RUSTFLAGS", "-A warnings")
        .env("LIBTSM", env!("ORACLE_LIBTSM_GIT"))
        .status()
        .map_err(|e| format!("cargo: {e}"))?;
    if !status.success() {
        return Err(format!("plant {}: the build failed", plant.name));
    }
    Ok(target.join("release/parse-oracle"))
}
