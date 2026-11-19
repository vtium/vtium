//! A vtlook's color filters as one color table, which both drawing paths
//! read.
//!
//! The reader has checked every value against its parameter's kind and
//! range, so building the table from the filters' formulas can fail only
//! for want of memory. A `cube` filter takes its table from a `.cube` file,
//! which vtium reads here, through the [`crate::cullet::Gate`]: the batch must hold the
//! file and it must pass the trust rule, or it counts as missing, a fault
//! of the reading stage. A file that does not read as a `.cube` table is
//! a fault of the checking stage, with the file's line. Each fault names
//! the `.cube` file and the filter, and vtium reads every `cube` filter's
//! file before it reports, so a check names the faults of all of them.
//! Any fault means the head draws the built-in vtlook.
//!
//! References:
//! - ADR-1304 D2: the look's color filters merge into one table, built on the CPU once for each head.
//! - ADR-1304 D6: the stages and their one outcome.
//! - ADR-1304 D9, D14: the `.cube` file, its faults at the checking stage, and a file vtium may not read at the reading stage.
//! - ADR-1205 D4: a `.cube` file counts as missing unless the batch holds it and it passes the trust rule.

use std::sync::Arc;

use vtium_render::filter::{Given, Layer, Map, Table};

use super::cube::{Cubes, Unread, load};
use super::read::{Filter, Value, Vtlook};
use super::report::{Failure, Part, Report, Stage};
use crate::bounds::ERRORS_MAX;

/// The color table of `look`'s color filters, applied in the order of
/// their sections, reading each `cube` filter's file in `gate.dir`
/// through `gate`, or the report of why vtium cannot build it.
pub fn table_with(look: &Vtlook, gate: &Cubes) -> Result<Table, Failure> {
    build(look, Some(gate))
}

/// The color table of `look`'s color filters, for a vtlook that names no
/// `.cube` file. With no [`Cubes`], vtium reads no file, so a `cube` filter
/// counts as missing and the vtlook fails at its reading stage.
pub fn table(look: &Vtlook) -> Result<Table, Failure> {
    build(look, None)
}

fn build(look: &Vtlook, gate: Option<&Cubes>) -> Result<Table, Failure> {
    let of = look.color.len();
    let mut maps = Vec::new();
    let mut reports = Vec::new();
    let mut more = 0_usize;
    for (i, f) in look.color.iter().enumerate() {
        let part = Part::Filter {
            layer: Layer::Color,
            name: f.declaration.name.to_owned(),
            place: i.saturating_add(1),
            of,
        };
        let built = match cube_file(f) {
            Some(name) => cube_map(look, name, part, gate),
            None => formula_map(look, f, part),
        };
        match built {
            Ok(map) => maps.push(map),
            Err((failed, past)) => {
                reports.extend(failed);
                more = more.saturating_add(past);
            }
        }
    }
    if !reports.is_empty() {
        // the reports stay in the filters' order and each file's line
        // order, since lines of two files do not compare
        let past = reports.len().saturating_sub(ERRORS_MAX);
        reports.truncate(ERRORS_MAX);
        return Err(Failure {
            reports,
            more: more.saturating_add(past),
        });
    }
    Table::build(&maps).map_err(|e| fails(look, format!("{e}")))
}

/// The map of a `cube` filter, `part` of `look`, whose file is `name`: the
/// table of the `.cube` file read through `gate`, or the reports of why
/// vtium cannot read it with the count of faults past them.
fn cube_map(
    look: &Vtlook,
    name: &str,
    part: Part,
    gate: Option<&Cubes>,
) -> Result<Map, (Vec<Report>, usize)> {
    let dir = gate.map_or(std::path::Path::new(super::LOOKS), |g| g.dir);
    let path = dir.join(name);
    let report = |line, stage, message| Report {
        vtlook: look.name.clone(),
        file: path.clone(),
        line,
        part: Some(part.clone()),
        stage,
        message,
    };
    let read = match gate {
        Some(cubes) => load(&path, cubes.gate),
        None => Err(Unread::Missing(
            "vtium treats the file as missing, because it reads a .cube file only after checking the file against the batch and the trust rule, and this table was built without either.".to_owned(),
        )),
    };
    match read {
        Ok(cube) => Ok(Map::cube(Arc::new(cube))),
        Err(Unread::Missing(message)) => Err((vec![report(None, Stage::Reading, message)], 0)),
        Err(Unread::Faults(faults)) => Err((
            faults
                .faults
                .into_iter()
                .map(|fault| report(fault.line, Stage::Checking, fault.message))
                .collect(),
            faults.more,
        )),
    }
}

/// The map of the formula filter `f`, `part` of `look`, or the report of
/// the building stage when one of its values has no part in the table.
fn formula_map(look: &Vtlook, f: &Filter, part: Part) -> Result<Map, (Vec<Report>, usize)> {
    map(f).ok_or_else(|| {
        let report = Report {
            vtlook: look.name.clone(),
            file: look.file.clone(),
            line: Some(f.line),
            part: Some(part),
            stage: Stage::Building,
            message: format!("{} has no color table", f.declaration.name),
        };
        (vec![report], 0)
    })
}

/// The `.cube` file a `cube` filter `f` names, `None` for any other
/// filter.
fn cube_file(f: &Filter) -> Option<&str> {
    if f.declaration.name != "cube" {
        return None;
    }
    f.values.iter().find_map(|(_, v)| match v {
        Value::File(name) => Some(name.as_str()),
        _ => None,
    })
}

/// The filter `f` as a map from one color to another, `None` when one of
/// its values has no part in the table.
fn map(f: &Filter) -> Option<Map> {
    let values: Option<Vec<Given>> = f.values.iter().map(|(_, v)| given(v)).collect();
    Map::new(f.declaration, &values?)
}

/// A value as the table takes it. A file's name is not one: the filter
/// that reads a file builds its table from the file.
fn given(v: &Value) -> Option<Given> {
    Some(match *v {
        Value::Number(n) => Given::Number(n),
        Value::Integer(n) => Given::Integer(n),
        Value::Color(c) => Given::Color(c),
        Value::Choice(w) => Given::Choice(w),
        Value::Switch(b) => Given::Switch(b),
        Value::File(_) => return None,
    })
}

/// The failure of the building stage for the vtlook as a whole: vtium
/// has no memory for the table.
fn fails(look: &Vtlook, message: String) -> Failure {
    Failure {
        reports: vec![Report {
            vtlook: look.name.clone(),
            file: look.file.clone(),
            line: None,
            part: None,
            stage: Stage::Building,
            message,
        }],
        more: 0,
    }
}

#[cfg(test)]
#[expect(clippy::panic, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use crate::vtlook::read_text;
    use std::path::Path;

    fn look(text: &str) -> Vtlook {
        read_text(
            "amber",
            Path::new("/etc/vtium/looks/amber.vtlook"),
            text.as_bytes(),
        )
        .unwrap_or_else(|e| panic!("{e:?}"))
    }

    // A vtlook's color filters build the table the filters give in the
    // order of their sections, each parameter left out at its default.
    #[test]
    fn a_vtlook_builds_the_table_of_its_color_filters_in_order() {
        let text =
            "version = 1\n[color tint]\ncolor = 255,176,0\n[color contrast]\ncontrast = 1.15\n";
        let got = table(&look(text)).unwrap_or_else(|e| panic!("{e:?}"));
        let decl = |name| vtium_render::filter::color(name).unwrap_or_else(|| panic!("{name}"));
        let tint = Map::defaults(decl("tint")).unwrap_or_else(|| panic!("tint"));
        let contrast = Map::new(
            decl("contrast"),
            &[
                Given::Number(vtium_render::filter::Fixed::of(1, 1500)),
                Given::Number(vtium_render::filter::Fixed::of(0, 0)),
            ],
        )
        .unwrap_or_else(|| panic!("contrast"));
        let want =
            Table::build(&[tint.clone(), contrast.clone()]).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(got, want);
        let reversed = Table::build(&[contrast, tint]).unwrap_or_else(|e| panic!("{e}"));
        assert_ne!(got, reversed);
        // a vtlook with no color filter builds the identity
        let none = table(&look("version = 1\n")).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(none, Table::build(&[]).unwrap_or_else(|e| panic!("{e}")));
    }

    // A filter whose value has no part in the table is an error of the
    // building stage, which names the filter and its place.
    #[test]
    fn a_filter_with_no_table_fails_at_the_building_stage() {
        let mut l = look("version = 1\n[color gamma]\n");
        if let Some(f) = l.color.first_mut() {
            f.values = vec![("gamma", Value::File("warm.cube".to_owned()))];
        }
        let Err(failure) = table(&l) else {
            panic!("built");
        };
        let said: Vec<String> = failure.reports.iter().map(ToString::to_string).collect();
        assert_eq!(
            said,
            ["color filter 1 of 1, gamma: building the color table: gamma has no color table"]
        );
        assert_eq!(failure.reports.first().and_then(|r| r.line), Some(2));
    }

    /// A fixture looks folder of its own under the temporary folder, the
    /// test's own user trusted below it, the batch listing `active`, and
    /// the running system's mount table.
    struct Fixture {
        dir: crate::scratch::Scratch,
        rule: crate::cullet::Rule,
        mounts: crate::mounts::Mounts,
    }

    impl Fixture {
        fn new(name: &str) -> Fixture {
            use std::os::unix::fs::MetadataExt;
            let dir = crate::scratch::Scratch::new(name);
            let uid = std::fs::metadata(&*dir)
                .map(|m| m.uid())
                .unwrap_or_else(|e| panic!("{e}"));
            let rule = crate::cullet::Rule::new(crate::Trust::owner(uid)).below(&dir);
            let mounts = crate::mounts::Mounts::system().unwrap_or_else(|e| panic!("{e}"));
            Fixture { dir, rule, mounts }
        }

        /// Writes `name` into the folder with mode `mode`.
        fn write(&self, name: &str, bytes: &[u8], mode: u32) {
            use std::os::unix::fs::PermissionsExt;
            let path = self.dir.join(name);
            std::fs::write(&path, bytes).unwrap_or_else(|e| panic!("{e}"));
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))
                .unwrap_or_else(|e| panic!("{e}"));
        }

        /// The batch listing the files `active` of the folder.
        fn batch(&self, active: &[&str]) -> crate::batch::Batch {
            crate::batch::Batch::new(
                active
                    .iter()
                    .map(|n| crate::batch::Entry::Path(self.dir.join(n)))
                    .collect(),
            )
        }

        /// The table of `text` with `batch`, or the failure as the log and
        /// `vtium check-config` show each report.
        fn table(&self, text: &str, batch: &crate::batch::Batch) -> Result<Table, Vec<String>> {
            let active = crate::cullet::Gate::new(
                self.rule.clone(),
                Some(self.mounts.clone()),
                batch.clone(),
            );
            let gate = Cubes {
                dir: &self.dir,
                gate: &active,
            };
            table_with(&look(text), &gate).map_err(|f| {
                let mut said: Vec<String> = f
                    .reports
                    .iter()
                    .map(|r| format!("{}: {r}", r.at()))
                    .collect();
                if f.more > 0 {
                    said.push(format!("{} more", f.more));
                }
                said
            })
        }
    }

    const WARM: &str = "# warm.cube\nTITLE \"warm\"\nLUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 0.95 0\n1 0.95 0\n0 0 0.85\n1 0 0.85\n0 0.95 0.85\n1 0.95 0.85\n";

    // An active .cube file that passes the trust rule builds its table,
    // in the order of the vtlook's sections: a tint after the warm table
    // gives what the two give in that order.
    #[test]
    fn an_active_cube_builds_its_table() {
        let f = Fixture::new("cube-active");
        f.write("warm.cube", WARM.as_bytes(), 0o644);
        let text = "version = 1\n[color cube]\nfile = warm.cube\n[color tint]\n";
        let got = f
            .table(text, &f.batch(&["warm.cube"]))
            .unwrap_or_else(|e| panic!("{e:?}"));
        let cube = super::super::read_cube(WARM.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
        let tint =
            Map::defaults(vtium_render::filter::color("tint").unwrap_or_else(|| panic!("tint")))
                .unwrap_or_else(|| panic!("tint"));
        let want =
            Table::build(&[Map::cube(Arc::new(cube)), tint]).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(got, want);
    }

    // A .cube file the batch does not list counts as missing, whatever
    // the file holds, and the report names the file and the check it
    // failed, at the reading stage.
    #[test]
    fn a_cube_outside_the_batch_counts_as_missing() {
        let f = Fixture::new("cube-inactive");
        f.write("warm.cube", WARM.as_bytes(), 0o644);
        let text = "version = 1\n[color tint]\n[color cube]\nfile = warm.cube\n";
        let file = f.dir.join("warm.cube");
        assert_eq!(
            f.table(text, &f.batch(&["other.cube"])),
            Err(vec![format!(
                "{}: color filter 2 of 2, cube: reading: vtium treats the file as missing, because it is not active: {} does not list it.",
                file.display(),
                crate::batch::BATCH_FILE
            )])
        );
    }

    // A .cube file in the batch that fails the trust rule counts as
    // missing too: one its group can write, one reached through a
    // symbolic link, and one that does not exist.
    #[test]
    fn a_cube_that_fails_the_trust_rule_counts_as_missing() {
        let f = Fixture::new("cube-untrusted");
        f.write("group.cube", WARM.as_bytes(), 0o664);
        f.write("real.cube", WARM.as_bytes(), 0o644);
        std::os::unix::fs::symlink(f.dir.join("real.cube"), f.dir.join("link.cube"))
            .unwrap_or_else(|e| panic!("{e}"));
        let batch = f.batch(&["group.cube", "link.cube", "gone.cube"]);
        let at = |n: &str| f.dir.join(n).display().to_string();
        let said = |n: &str| {
            let text = format!("version = 1\n[color cube]\nfile = {n}\n");
            let Err(said) = f.table(&text, &batch) else {
                panic!("{n} built");
            };
            said
        };
        let missing = |n: &str| {
            format!(
                "{}: color filter 1 of 1, cube: reading: vtium treats the file as missing, because ",
                at(n)
            )
        };
        assert_eq!(
            said("group.cube"),
            [format!(
                "{}other users can change \"{}\" (mode 664). Run `chmod go-w {}` so that only its owner can change it.",
                missing("group.cube"),
                at("group.cube"),
                at("group.cube")
            )]
        );
        assert_eq!(
            said("link.cube"),
            [format!(
                "{}{} is a symbolic link, and vtium uses no resource reached through one.",
                missing("link.cube"),
                at("link.cube")
            )]
        );
        assert_eq!(
            said("gone.cube"),
            [format!(
                "{}no file exists at that path.",
                missing("gone.cube")
            )]
        );
    }

    // A .cube file's faults are of the checking stage, each on its line
    // of the .cube file, and the log line names the first and counts the
    // rest: a size out of range and a keyword vtium does not read.
    //
    // References:
    // - ADR-1304 D14: the table of faults and the log line of each.
    #[test]
    fn a_cube_s_faults_are_reported_on_its_lines() {
        let f = Fixture::new("cube-faults");
        f.write(
            "panel.cube",
            b"TITLE \"panel\"\nLUT_3D_SIZE 129\nLUT_3D_INPUT_RANGE 0 1\n",
            0o644,
        );
        let text = "version = 1\n[color cube]\nfile = panel.cube\n[color gamma]\n";
        let at = f.dir.join("panel.cube").display().to_string();
        assert_eq!(
            f.table(text, &f.batch(&["panel.cube"])),
            Err(vec![
                format!(
                    "{at}:2: color filter 1 of 2, cube: checking: found LUT_3D_SIZE 129, needs 2 to 65"
                ),
                format!(
                    "{at}:3: color filter 1 of 2, cube: checking: found LUT_3D_INPUT_RANGE, needs TITLE, LUT_3D_SIZE, LUT_1D_SIZE, DOMAIN_MIN or DOMAIN_MAX"
                ),
            ])
        );
        let active = crate::cullet::Gate::new(
            f.rule.clone(),
            Some(f.mounts.clone()),
            f.batch(&["panel.cube"]),
        );
        let gate = Cubes {
            dir: &f.dir,
            gate: &active,
        };
        let Err(failure) = table_with(&look(text), &gate) else {
            panic!("built");
        };
        assert_eq!(
            super::super::log_line(&failure, "console"),
            Some(format!(
                "vtlook amber ({at}:2): color filter 1 of 2, cube: checking: found LUT_3D_SIZE 129, needs 2 to 65; 1 more error; head console draws the built-in vtlook instead"
            ))
        );
    }

    // A .cube file's faults past those a report keeps are counted, with
    // the faults of the vtlook's other filters, in the failure's count.
    #[test]
    fn a_cube_s_faults_past_the_kept_ones_are_counted() {
        let f = Fixture::new("cube-many");
        let text = format!("LUT_1D_SIZE 2\n{}", "x,y\n".repeat(100));
        f.write("many.cube", text.as_bytes(), 0o644);
        let active = crate::cullet::Gate::new(
            f.rule.clone(),
            Some(f.mounts.clone()),
            f.batch(&["many.cube"]),
        );
        let gate = Cubes {
            dir: &f.dir,
            gate: &active,
        };
        let look = look("version = 1\n[color cube]\nfile = many.cube\n");
        let Err(failure) = table_with(&look, &gate) else {
            panic!("built");
        };
        // 100 lines that do not read and the count of data lines
        assert_eq!(failure.reports.len(), ERRORS_MAX);
        assert_eq!(failure.reports.len() + failure.more, 101);
    }

    // A .cube file past CUBE_FILE_MAX counts as missing at the reading
    // stage, before vtium reads its lines; one at the bound is read.
    #[test]
    fn a_cube_past_its_bound_counts_as_missing() {
        let f = Fixture::new("cube-large");
        let mut big = WARM.as_bytes().to_vec();
        let fill = crate::bounds::CUBE_FILE_MAX - big.len();
        big.extend(std::iter::repeat_n(b'#', fill - 1));
        big.push(b'\n');
        f.write("big.cube", &big, 0o644);
        let text = "version = 1\n[color cube]\nfile = big.cube\n";
        let batch = f.batch(&["big.cube"]);
        assert!(f.table(text, &batch).is_ok(), "a file at the bound reads");
        big.insert(0, b'#');
        f.write("big.cube", &big, 0o644);
        assert_eq!(
            f.table(text, &batch),
            Err(vec![format!(
                "{}: color filter 1 of 1, cube: reading: vtium treats the file as missing, because it is larger than 16 MiB (16,777,216 bytes), the most a .cube file may hold.",
                f.dir.join("big.cube").display()
            )])
        );
    }

    // With no gate, vtium reads no .cube file, so a cube filter counts as
    // missing; a vtlook without one builds as before.
    #[test]
    fn with_no_gate_a_cube_counts_as_missing() {
        let Err(failure) = table(&look("version = 1\n[color cube]\nfile = warm.cube\n")) else {
            panic!("built");
        };
        let said: Vec<String> = failure
            .reports
            .iter()
            .map(|r| format!("{}: {r}", r.at()))
            .collect();
        assert_eq!(
            said,
            [
                "/etc/vtium/looks/warm.cube: color filter 1 of 1, cube: reading: vtium treats the file as missing, because it reads a .cube file only after checking the file against the batch and the trust rule, and this table was built without either."
            ]
        );
    }
}
