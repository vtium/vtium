use std::ffi::OsString;

use std::path::PathBuf;

use super::frits::frit;
#[cfg(feature = "images")]
use super::notes::image_helper_note;
use super::notes::{font_path_note, helper_note, semantics_under};
use super::*;
use crate::config;
use crate::scratch::Scratch;

/// What [`notes`] says for `args`, less the notes that begin with
/// `left_out`, which other tests judge.
fn notes_but(left_out: &[&str], args: &[&str]) -> Vec<String> {
    let args: Vec<OsString> = [SUBCOMMAND]
        .iter()
        .chain(args)
        .map(OsString::from)
        .collect();
    notes(&config::load(&args, &mut |_: &Path| Ok(None)).config)
        .into_iter()
        .filter(|n| !left_out.iter().any(|l| n.starts_with(l)))
        .collect()
}

// check-config notes helper-sandbox=off and what it gives up, and says
// nothing of the sandbox while it is on.
//
// References:
// - ADR-1200 D15, "A setting turns the sandbox off": `vtium
//   check-config` notes the setting.
#[test]
fn check_config_notes_the_helpers_sandbox_off() {
    let about = |args: &[&str]| -> Vec<String> {
        notes_but(&[], args)
            .into_iter()
            .filter(|n| n.starts_with("helper-sandbox"))
            .collect()
    };
    let off = about(&["--helper-sandbox=off"]);
    assert_eq!(off.len(), 1, "{off:?}");
    let note = off.first().map(String::as_str).unwrap_or_default();
    assert!(
        note.starts_with("helper-sandbox: the sandbox is off, so the font helper and the image helper run without their seccomp filters and landlock domains."),
        "{note}"
    );
    assert!(note.contains("open network connections"), "{note}");
    assert!(about(&["--helper-sandbox=on"]).is_empty());
    assert!(about(&[]).is_empty());
}

/// `/bin/sh`, a program root owns, for the assertions that root's own
/// helper draws no note. `make arm-test` runs the tests in a user
/// namespace that does not map root (tools/arm/README.md), where every
/// file of root's belongs to the overflow uid, 65534. No file there is
/// root's, so those assertions cannot mean anything, and they skip and
/// say so. Anywhere else, a `/bin/sh` that root does not own fails.
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn roots_program() -> Option<&'static Path> {
    let sh = Path::new("/bin/sh");
    let owner = std::fs::metadata(sh)
        .map(|m| std::os::unix::fs::MetadataExt::uid(&m))
        .unwrap();
    if owner == 0 {
        return Some(sh);
    }
    let qemu = std::env::var_os("VTIUM_QEMU_USER");
    assert!(
        qemu.is_some(),
        "/bin/sh belongs to uid {owner}, and these tests need a program root owns"
    );
    eprintln!(
        "under {}, /bin/sh belongs to uid {owner}, so no file is root's: the assertions about root's own helper are skipped",
        qemu.unwrap_or_default().display()
    );
    None
}

fn run_with(args: &[&str], conf: &str) -> (Vec<String>, i32) {
    let args: Vec<OsString> = [SUBCOMMAND]
        .iter()
        .chain(args)
        .map(OsString::from)
        .collect();
    let conf = conf.as_bytes().to_vec();
    let loaded = config::load(&args, &mut |p: &Path| {
        Ok((p == Path::new("/etc/vtium/vtium.conf")).then(|| conf.clone()))
    });
    assert!(loaded.check);
    run(&loaded)
}

#[test]
fn a_clean_configuration_exits_zero() {
    let (lines, status) = run_with(&[], "font-size=16\nno-blink\n");
    assert_eq!((lines, status), (Vec::<String>::new(), 0));
}

#[test]
fn each_warning_is_printed_and_the_status_is_one() {
    let (lines, status) = run_with(&["--bogus"], "sb-size=200000\nfrob=1\n");
    assert_eq!(status, 1);
    assert_eq!(
        lines,
        [
            "argument 2: unknown key \"--bogus\", ignored",
            "/etc/vtium/vtium.conf:1: sb-size=\"200000\" is out of range, so 100000 is used",
            "/etc/vtium/vtium.conf:2: unknown key \"frob\", ignored",
        ]
    );
}

#[test]
fn what_the_values_mean_together_is_checked_too() {
    let (lines, status) = run_with(&[], "mode=1024x768\nuse-original-mode\n");
    assert_eq!(status, 1);
    assert_eq!(lines, ["mode is set, so use-original-mode is off"]);
}

#[test]
fn a_palette_name_that_could_name_no_file_is_reported() {
    let (lines, status) = run_with(&["--palette=../frob"], "palette=nord\n");
    assert_eq!(status, 1);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines
            .first()
            .is_some_and(|l| l.starts_with("argument 2: palette=\"../frob\"")
                && l.contains(keys::NOT_A_NAME)),
        "{lines:?}"
    );
    for name in vtium_term::PALETTE_NAMES
        .iter()
        .chain(&["custom", "dracula"])
    {
        let conf = format!("palette={name}\n");
        assert_eq!(run_with(&[], &conf), (Vec::new(), 0), "{name}");
    }
}

// check-config says what it finds of the batch file, the list of active
// fonts, color schemes, effects and pointer themes: a missing file is a note that says what vtium
// uses instead and how to create the file, while a file others can
// write, and a line vtium ignores, count against the configuration.
//
// References:
// - ADR-1205 D4, its notes of 2026-11-18 and 2026-11-19: what check-config says of a missing batch file and of one vtium cannot use.
#[test]
fn check_config_says_what_it_finds_of_the_batch_file() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, file, _) = heads_fixture(
        "batch",
        "version = 1\n[head console]\n[rules]\ndisplay * -> console\ninput * -> roaming\n",
        0o644,
    );
    let batch = dir.join("batch.conf");
    let host = Host {
        conf_file: file.with_file_name("installed.conf"),
        udev_control: file.clone(),
        roots: nowhere(),
        frits: PathBuf::from("/nonexistent/vtium/frits"),
        frit_trust: font::trust().below(&dir),
        systemd_root: PathBuf::from("/nonexistent/vtium/root"),
        batch: batch.clone(),
        drawer: None,
    };
    let f = file.display().to_string();
    let b = batch.display().to_string();
    let out = outcome_on(&["--no-devices", f.as_str()], &host);
    assert_eq!(out.status, 0, "{:#?}", out.stderr);
    assert_eq!(
        out.stdout.first().map(String::as_str),
        Some(format!(
            "{b}: The list of active fonts, color schemes, effects and pointer themes does not exist. vtium uses the list of a fresh install instead: the built-in font, Cullet Mono, Terminus, Unifont, the console font that vconsole.conf names, the color schemes vtium ships, and vtium's pointer file. To activate a resource, press Space on it in vtium-config's Fonts, Effects or Looks tab, or run vtium-config activate PATH, then install the change. vtium-config then writes this file with the fresh-install list and your change. If you write the file by hand instead, it replaces the fresh-install list, so list every path that stays active, vtium's own fonts, color schemes and pointer file included."
        ).as_str()),
        "{:#?}",
        out.stdout
    );
    let _ = std::fs::write(
        &batch,
        "/usr/share/vtium/palettes/kiln.palette\nrelative/path\n",
    );
    let _ = std::fs::set_permissions(&batch, std::fs::Permissions::from_mode(0o644));
    let out = outcome_on(&["--no-devices", f.as_str()], &host);
    assert_eq!(out.status, 1, "{:#?}", out.stderr);
    assert!(
        out.stderr.contains(&format!(
            "{b}:2: This line does not hold an absolute path, one that starts with /. Because of this, vtium ignores the line."
        )),
        "{:#?}",
        out.stderr
    );
    // past the error bound, the lines vtium does not list are counted
    let _ = std::fs::write(&batch, "relative\n".repeat(vtium_settings::ERRORS_MAX + 3));
    let out = outcome_on(&["--no-devices", f.as_str()], &host);
    assert_eq!(out.status, 1, "{:#?}", out.stderr);
    let listed = out
        .stderr
        .iter()
        .filter(|l| l.ends_with("Because of this, vtium ignores the line."))
        .count();
    assert_eq!(listed, vtium_settings::ERRORS_MAX, "{:#?}", out.stderr);
    assert!(
        out.stderr.contains(&format!(
            "{b}: vtium also ignores 3 more lines of this file. Only the first {} are listed.",
            vtium_settings::ERRORS_MAX
        )),
        "{:#?}",
        out.stderr
    );
    assert!(
        !out.stdout.iter().any(|l| l.starts_with(&b)),
        "{:#?}",
        out.stdout
    );
    let _ = std::fs::set_permissions(&batch, std::fs::Permissions::from_mode(0o666));
    let out = outcome_on(&["--no-devices", f.as_str()], &host);
    assert_eq!(out.status, 1, "{:#?}", out.stderr);
    let refused = format!(
        "{b}: vtium ignores this list of active fonts, color schemes, effects and pointer themes and uses the list of a fresh install, because "
    );
    assert!(
        out.stderr.iter().any(|l| l.starts_with(&refused)),
        "{refused}\n{:#?}",
        out.stderr
    );
}

// A color scheme that fails the trust rule, or that the batch does not
// list, names its file and the check it failed, and each place that
// names it.
// covers: color.palette.named
#[test]
fn check_config_names_a_color_scheme_the_gate_refuses() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new("check-palette-gate");
    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755));
    let write = |name: &str| {
        let path = dir.join(format!("{name}.palette"));
        std::fs::write(&path, "background = #000000\n").unwrap_or_default();
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644));
        path
    };
    let shut = write("shut");
    let open = write("open");
    let gate = palette_gate(&dir);
    let gate = vtium_settings::cullet::Gate {
        batch: vtium_settings::batch::Batch::new(vec![vtium_settings::batch::Entry::Path(
            open.clone(),
        )]),
        ..gate
    };
    let _ = std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o666));
    let conf = Path::new("/etc/vtium/vtium.conf");
    let looks = [("warm".to_owned(), "open".to_owned(), 4)];
    let (counted, _) = palette_findings(
        Some(("shut", "/etc/vtium/vtium.conf:1".to_owned())),
        conf,
        &looks,
        &[&*dir],
        &gate,
    );
    let batch = vtium_settings::batch::BATCH_FILE;
    assert_eq!(
        counted,
        [
            format!(
                "{}: The color scheme is not active, so vtium treats the file as missing. A color scheme is active while {batch} lists the file's path. To activate it, press Space on it in vtium-config's Looks tab, or run vtium-config activate with the file's path, then install the change.",
                shut.display()
            ),
            "/etc/vtium/vtium.conf:1: The color scheme shut is not active, so the heads whose look names no color scheme of its own draw the color scheme legacy instead.".to_owned(),
            format!(
                "{o}: vtium treats the file as missing, because other users can change \"{o}\" (mode 666). Run `chmod go-w {o}` so that only its owner can change it.",
                o = open.display()
            ),
            "/etc/vtium/vtium.conf:4: vtium treats the color scheme open as missing, so the heads that use look warm draw the color scheme that vtium.conf's top level names instead, or legacy when that one fails too.".to_owned(),
        ]
    );
}

// check-config and OSC 50 judge a folder of `font-paths` by one rule,
// so a folder whose fonts OSC 50 refuses gets check-config's note that
// vtium draws no font from it, and a folder OSC 50 accepts gets none.
// The test runs as an ordinary user, so its own scratch folders are
// folders a user owns: one plain, one reached through a symbolic link.
// The check is root's, since the console runs as root.
//
// References:
// - ADR-1200 D5, "A qualifying font": OSC 50's fourth check, the trust rule on the font's folder.
// - ADR-1205 D4: the trust rule on a resource and every folder above it.
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn check_config_notes_each_font_folder_that_osc_50_refuses() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new("check-font-folder-rule");
    let mounts = vtium_settings::mounts::Mounts::system().unwrap_or_default();
    let (plain, linked) = (dir.join("fonts"), dir.join("linked"));
    std::fs::create_dir(&plain).unwrap();
    std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::os::unix::fs::symlink(&plain, &linked).unwrap();
    for folder in [&plain, &linked] {
        let refused = font::trusted_folder(
            folder,
            vtium_settings::cullet::Rule::new(Trust::ROOT),
            Some(mounts.clone()),
        )
        .is_err();
        let note = font_path_note(folder, &mounts);
        let untrusted = note
            .as_deref()
            .is_some_and(|n| n.contains("vtium draws no font from"));
        assert_eq!(untrusted, refused, "{}: {note:?}", folder.display());
        assert!(refused, "OSC 50 takes fonts from {}", folder.display());
    }
}

// check-config names each pointer file vtium would not read and the
// check it failed: an SVG file reached through a symbolic link, and a
// cursor theme's file whose link leads out of the theme's folder. A
// link inside the theme passes, and a path that names neither is
// reported where vtium.conf names it. A theme folder the batch does not
// list is one line, not one for each of its files. A shape whose first
// file passes reads no further, as the console reads no further, so a
// broken file after it is not named.
//
// References:
// - ADR-1205 D4: pointer themes are activated by their folder, their links, and check-config naming the file and the check it failed.
#[test]
fn check_config_names_each_pointer_file_the_gate_refuses() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new("check-pointers");
    let me = rustix::process::geteuid().as_raw();
    let rule = vtium_settings::cullet::Rule::new(Trust::owner(me)).below(&dir);
    let mounts = vtium_settings::mounts::Mounts::system().unwrap_or_default();
    let (theme, outside) = (dir.join("theme"), dir.join("outside"));
    let (svg, linked) = (dir.join("pointers.svg"), dir.join("linked.svg"));
    let listed = vtium_settings::batch::Batch::new(
        [&svg, &linked, &theme, &outside]
            .into_iter()
            .map(|p| vtium_settings::batch::Entry::Path(p.clone()))
            .collect(),
    );
    let gate = vtium_settings::cullet::Gate::new(rule.clone(), Some(mounts.clone()), listed);
    let unlisted = vtium_settings::cullet::Gate::new(
        rule,
        Some(mounts),
        vtium_settings::batch::Batch::default(),
    );
    for d in [&*dir, &theme, &theme.join("cursors"), &outside] {
        let _ = std::fs::create_dir_all(d);
        let _ = std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o755));
    }
    let write = |path: &Path| {
        let _ = std::fs::write(path, "x");
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644));
    };
    write(&svg);
    let _ = std::os::unix::fs::symlink(&svg, &linked);
    let cursors = theme.join("cursors");
    write(&cursors.join("xterm"));
    let _ = std::os::unix::fs::symlink("xterm", cursors.join("text"));
    write(&outside.join("left_ptr"));
    let _ = std::os::unix::fs::symlink("../../outside/left_ptr", cursors.join("default"));
    // the pointer shape's first file passes, so its broken second file
    // is never opened
    write(&cursors.join("pointer"));
    let _ = std::os::unix::fs::symlink("../../outside/left_ptr", cursors.join("hand2"));
    let at = || "/etc/vtium/vtium.conf:2".to_owned();
    let said_through = |value: &Path, gate: &vtium_settings::cullet::Gate| {
        pointer_findings(
            Some((value.to_str().unwrap_or(""), at())),
            Path::new("/etc/vtium/vtium.conf"),
            &[],
            gate,
        )
    };
    let said = |value: &Path| said_through(value, &gate);
    assert_eq!(said(&svg), Vec::<String>::new());
    assert_eq!(
        said(&linked),
        [format!(
            "{l}: vtium treats this pointer file as missing, because {l} is a symbolic link, and vtium uses no resource reached through one. So vtium draws its own pointer pictures.",
            l = linked.display()
        )]
    );
    let default = cursors.join("default");
    assert_eq!(
        said(&theme),
        [format!(
            "{d}: vtium treats this file of the pointer theme {t} as missing, because the symbolic link {d} leads outside the pointer theme's folder {t}. vtium tries the shape's next file, or draws its own picture of the shape.",
            d = default.display(),
            t = theme.display()
        )]
    );
    assert_eq!(
        said(&outside),
        [format!(
            "/etc/vtium/vtium.conf:2: The pointer file {} is neither an SVG file nor a folder that holds a cursors folder, so vtium draws its own pointer pictures.",
            outside.display()
        )]
    );
    assert_eq!(
        said_through(&theme, &unlisted),
        [format!(
            "{t}: vtium treats this pointer theme as missing, because it is not active: {b} does not list it. So vtium draws its own pointer pictures.",
            t = theme.display(),
            b = vtium_settings::batch::BATCH_FILE
        )]
    );
    assert_eq!(
        pointer_findings(None, Path::new("/etc/vtium/vtium.conf"), &[], &gate),
        Vec::<String>::new()
    );
}

// A look's pointer theme is checked as the top level's is: a theme the
// batch does not list says so, and a path that names neither an SVG
// file nor a theme's folder is reported on the look's own line. A
// theme that the top level and a look both name is said once.
//
// References:
// - ADR-1205 D6: each look picks its pointer theme, which passes the same trust rule as the top level's.
#[test]
fn check_config_checks_each_look_s_pointer_theme_as_the_top_level_s() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new("check-look-pointers");
    let me = rustix::process::geteuid().as_raw();
    let rule = vtium_settings::cullet::Rule::new(Trust::owner(me)).below(&dir);
    let mounts = vtium_settings::mounts::Mounts::system().unwrap_or_default();
    let theme = dir.join("theme");
    for d in [&*dir, &theme, &theme.join("cursors")] {
        let _ = std::fs::create_dir_all(d);
        let _ = std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o755));
    }
    let nothing = dir.join("nothing");
    let unlisted = vtium_settings::cullet::Gate::new(
        rule,
        Some(mounts),
        vtium_settings::batch::Batch::default(),
    );
    let conf = Path::new("/etc/vtium/vtium.conf");
    let t = theme.display().to_string();
    let looks = [
        ("arrows".to_owned(), t.clone(), 5),
        ("lost".to_owned(), nothing.display().to_string(), 8),
    ];
    let inactive = format!(
        "{t}: vtium treats this pointer theme as missing, because it is not active: {b} does not list it. So vtium draws its own pointer pictures.",
        b = vtium_settings::batch::BATCH_FILE
    );
    let unusable = format!(
        "/etc/vtium/vtium.conf:8: The pointer file {} is neither an SVG file nor a folder that holds a cursors folder, so vtium draws its own pointer pictures.",
        nothing.display()
    );
    assert_eq!(
        pointer_findings(None, conf, &looks, &unlisted),
        [inactive.clone(), unusable.clone()]
    );
    assert_eq!(
        pointer_findings(
            Some((t.as_str(), "/etc/vtium/vtium.conf:1".to_owned())),
            conf,
            &looks,
            &unlisted
        ),
        [inactive, unusable],
        "the theme both name is said once"
    );
}

// check-config names each font file a font set names by its path that
// a start would treat as missing: one that fails the trust rule, with
// the check it failed, and one the batch does not list. A font named by
// its family is left to the font helper, and an active, trusted file
// has nothing to say.
//
// References:
// - ADR-1205 D4: a font draws only while the batch holds it and it passes the trust rule.
#[test]
fn check_config_names_each_font_file_the_gate_refuses() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new("check-font-files");
    let _ = std::fs::set_permissions(&*dir, std::fs::Permissions::from_mode(0o755));
    let me = rustix::process::geteuid().as_raw();
    let font = |name: &str, mode: u32| {
        let path = dir.join(name);
        let _ = std::fs::write(&path, "x");
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode));
        path
    };
    let (good, open, idle, italic) = (
        font("good.bdf", 0o644),
        font("open.bdf", 0o666),
        font("idle.otf", 0o644),
        font("idle-italic.otf", 0o644),
    );
    let frit = dir.join("desk.vtfrit");
    let _ = std::fs::write(
        &frit,
        format!(
            "source=bdf:{}\nsource=font:Some Family\nsource=bdf:{}\nsource=outline:{}\nstrike=file:{}\nsource=bdf:{}\nsource=outline:{} # the italic face\n",
            good.display(),
            open.display(),
            idle.display(),
            good.display(),
            dir.join("gone.bdf").display(),
            italic.display()
        ),
    );
    let _ = std::fs::set_permissions(&frit, std::fs::Permissions::from_mode(0o644));
    let gate = vtium_settings::cullet::Gate::new(
        vtium_settings::cullet::Rule::new(Trust::owner(me)).below(&dir),
        Some(vtium_settings::mounts::Mounts::system().unwrap_or_default()),
        vtium_settings::batch::Batch::new(
            [&good, &open]
                .map(|p| vtium_settings::batch::Entry::Path(p.clone()))
                .to_vec(),
        ),
    );
    let settings = Settings {
        trust: Trust::owner(me).below(&dir),
        gate: Some(&gate),
        ..caps(4)
    };
    let f = frit.display();
    let b = vtium_settings::batch::BATCH_FILE;
    assert_eq!(
        font_files(&frit, &settings),
        [
            format!(
                "{f}:3: vtium treats the font {o} as missing, because other users can change \"{o}\" (mode 666). Run `chmod go-w {o}` so that only its owner can change it. vtium draws its characters from the next font in the set.",
                o = open.display()
            ),
            format!(
                "{f}:4: The font {} is not active, so vtium treats it as missing and draws its characters from the next font in the set. A font is active while {b} lists the file's path. To activate it, press Space on it in vtium-config's Fonts tab, or run vtium-config activate with the file's path, then install the change.",
                idle.display()
            ),
            format!(
                "{f}:7: The font {} is not active, so vtium treats it as missing and draws its characters from the next font in the set. A font is active while {b} lists the file's path. To activate it, press Space on it in vtium-config's Fonts tab, or run vtium-config activate with the file's path, then install the change.",
                italic.display()
            ),
        ]
    );
    assert_eq!(font_files(&frit, &caps(4)), Vec::<String>::new());
}

/// The gate a palette file in the scratch folder `dir` passes: its
/// owner's files under `dir`, this machine's mount table, and a batch
/// that lists each file in `dir` now.
fn palette_gate(dir: &Path) -> vtium_settings::cullet::Gate {
    use std::os::unix::fs::MetadataExt;
    let me = std::fs::metadata(dir).map_or(0, |m| m.uid());
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();
    files.sort();
    vtium_settings::cullet::Gate::new(
        vtium_settings::cullet::Rule::new(Trust::owner(me)).below(dir),
        Some(vtium_settings::mounts::Mounts::system().unwrap_or_default()),
        vtium_settings::batch::Batch::new(
            files
                .into_iter()
                .map(vtium_settings::batch::Entry::Path)
                .collect(),
        ),
    )
}

// A color scheme that sets many keys vtium ignores gives one note for
// each of the first ERRORS_MAX keys and one note that counts the rest.
#[test]
fn check_config_counts_the_ignored_keys_past_the_bound() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new("check-palette-keys");
    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755));
    let path = dir.join("many.palette");
    let text: String = (0..vtium_settings::ERRORS_MAX + 7)
        .map(|i| format!("k{i} = 1\n"))
        .collect();
    std::fs::write(&path, text).unwrap_or_default();
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644));
    let gate = palette_gate(&dir);
    let (counted, notes) = palette_findings(
        Some(("many", "/etc/vtium/vtium.conf:1".to_owned())),
        Path::new("/etc/vtium/vtium.conf"),
        &[],
        &[&*dir],
        &gate,
    );
    assert_eq!(counted, Vec::<String>::new());
    assert_eq!(notes.len(), vtium_settings::ERRORS_MAX + 1);
    assert_eq!(
        notes.last().map(String::as_str),
        Some(
            format!(
                "{}: The color scheme many sets 7 more keys that vtium ignores.",
                path.display()
            )
            .as_str()
        )
    );
}

// check-config reads each color scheme vtium.conf names, at the top
// level and in a look, as the start reads it: a scheme with errors
// lists each error with its file and line and then each place that
// names it; a name no folder holds and no built-in palette has is a
// warning; a file named like a built-in palette is listed; and each
// key a scheme sets that vtium ignores is a note, once.
// covers: color.palette.named
#[test]
fn check_config_reports_the_color_schemes_vtium_conf_names() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new("check-palettes");
    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755));
    let write = |name: &str, text: &str| {
        let path = dir.join(format!("{name}.palette"));
        std::fs::write(&path, text).unwrap_or_default();
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644));
        path
    };
    let good = write(
        "good",
        "# a theme\ncursor-text = #000000\nbackground = #000000\ncursor-text = #111111\nconfig-file = /etc/x\n",
    );
    let broken = write(
        "broken",
        "background = #000000\nnope\nforeground = notacolor\n",
    );
    let legacy = write("legacy", "background = #000000\n");
    let gate = palette_gate(&dir);
    let conf = Path::new("/etc/vtium/vtium.conf");
    let looks = [
        ("warm".to_owned(), "broken".to_owned(), 4),
        ("cool".to_owned(), "absent".to_owned(), 6),
        ("plain".to_owned(), "good".to_owned(), 8),
    ];
    let (counted, notes) = palette_findings(
        Some(("broken", "/etc/vtium/vtium.conf:1".to_owned())),
        conf,
        &looks,
        &[&*dir],
        &gate,
    );
    let (b, l) = (broken.display(), legacy.display());
    assert_eq!(
        counted,
        [
            format!(
                "{b}:2: This line is not a setting. Each line of a color scheme reads key = value, or is a comment that starts with #."
            ),
            format!(
                "{b}:3: The color \"notacolor\" of foreground is not one vtium can read. A color is #rrggbb, rrggbb or an X11 color name such as DarkSlateGray."
            ),
            "/etc/vtium/vtium.conf:1: The color scheme broken has errors, so the heads whose look names no color scheme of its own draw the color scheme legacy instead.".to_owned(),
            "/etc/vtium/vtium.conf:4: The color scheme broken has errors, so the heads that use look warm draw the color scheme that vtium.conf's top level names instead, or legacy when that one fails too.".to_owned(),
            format!(
                "/etc/vtium/vtium.conf:6: The color scheme absent does not exist. No built-in color scheme has that name, and there is no file absent.palette in {}. So the heads that use look cool draw the color scheme that vtium.conf's top level names instead, or legacy when that one fails too.",
                dir.display()
            ),
            format!(
                "{l}: The file takes the name of the built-in color scheme legacy, so vtium never reads it. Give the file another name."
            ),
        ]
    );
    let g = good.display();
    assert_eq!(
        notes,
        [
            format!(
                "{g}:2: The color scheme good sets \"cursor-text\", which vtium ignores: it is not a color vtium reads."
            ),
            format!(
                "{g}:5: The color scheme good sets \"config-file\", which vtium ignores: vtium never follows it, so a color scheme sets colors and nothing else."
            ),
        ]
    );
    // a built-in palette and custom name no file
    let (counted, notes) = palette_findings(
        Some(("nord", "argument 2".to_owned())),
        conf,
        &[("warm".to_owned(), "custom".to_owned(), 4)],
        &[Path::new("/nonexistent/vtium/palettes")],
        &gate,
    );
    assert_eq!(
        (counted, notes),
        (Vec::<String>::new(), Vec::<String>::new())
    );
}

#[test]
fn hwaccel_warns_only_in_a_build_without_the_gpu_path() {
    let (lines, status) = run_with(&[], "hwaccel\n");
    if cfg!(feature = "gpu") {
        assert_eq!((lines, status), (Vec::<String>::new(), 0));
    } else {
        assert_eq!(status, 1);
        assert_eq!(
            lines,
            ["hwaccel needs a build with GPU support, so vtium draws in software"]
        );
    }
    assert_eq!(run_with(&[], "no-hwaccel\n"), (Vec::new(), 0));
}

#[test]
fn a_key_not_built_yet_warns() {
    // Every other key not built yet warns once a source sets it, to
    // any value it takes, in the file or on the command line.
    // `font-line-spacing` is built (ADR-1302 D13), and says nothing.
    let (lines, status) = run_with(
        &["--font-line-spacing=20"],
        "display-buffers=3\nfont-hinting=light\n",
    );
    assert_eq!(status, 1);
    assert_eq!(
        lines,
        ["font-hinting has no effect yet, so vtium ignores it"]
    );
    assert_eq!(run_with(&["--font-line-spacing=20"], ""), (Vec::new(), 0));
    assert_eq!(run_with(&[], "font-line-spacing=100\n"), (Vec::new(), 0));
    assert_eq!(
        run_with(&["--font-hinting=full"], ""),
        (
            vec!["font-hinting has no effect yet, so vtium ignores it".to_owned()],
            1
        )
    );
    // A value vtium cannot take leaves the key unset: its report
    // alone.
    let (lines, _) = run_with(&[], "display-buffers=4\n");
    assert_eq!(
        lines,
        [
            "/etc/vtium/vtium.conf:1: display-buffers=\"4\" is not a value this setting takes. It takes 1, 2, or 3. Because of this error, vtium ignores this setting."
        ]
    );
    // ADR-1401 D15: kernel-messages takes its two values alone.
    assert_eq!(run_with(&[], "kernel-messages=always\n"), (Vec::new(), 0));
    assert_eq!(
        run_with(&[], "kernel-messages=sometimes\n"),
        (
            vec![
                "/etc/vtium/vtium.conf:1: kernel-messages=\"sometimes\" is not a value this setting takes. It takes after-plymouth or always. Because of this error, vtium ignores this setting.".to_owned()
            ],
            1
        )
    );
}

/// The text of `/proc/1/status` for a PID 1 whose caught signals are
/// `cgt` and whose blocked signals are `blk`.
fn status(cgt: &str, blk: &str) -> Result<String, String> {
    Ok(format!(
        "Name:\tinit\nSigBlk:\t{blk}\nSigIgn:\t0000000000001000\nSigCgt:\t{cgt}\nCapInh:\t0\n"
    ))
}

fn power_off_warnings(conf: &str, status: &Result<String, String>) -> Vec<String> {
    let args = [OsString::from(SUBCOMMAND)];
    let conf = conf.as_bytes().to_vec();
    let loaded = config::load(&args, &mut |_: &Path| Ok(Some(conf.clone())));
    semantics_under(&loaded.config, status, 38)
}

const BOUND: &str = "key-power-off=<Ctrl><Alt>End\n";

#[test]
fn key_power_off_is_silent_where_init_takes_the_signal() {
    // systemd blocks SIGRTMIN+4 and reads it from a signalfd, so bit 37
    // (signal 38) is set in its SigBlk and not in its SigCgt; these are
    // the masks of systemd 262 as PID 1. sysvinit catches the signal
    // with a handler, so the bit is set in its SigCgt.
    for (cgt, blk) in [
        ("00000000000004ec", "7fefc1fe28014a03"),
        ("00000021fdc1fefb", "0000000000000000"),
    ] {
        assert_eq!(
            power_off_warnings(BOUND, &status(cgt, blk)),
            [] as [&str; 0],
            "SigCgt {cgt}, SigBlk {blk}"
        );
    }
}

#[test]
fn key_power_off_warns_where_init_neither_catches_nor_blocks_it() {
    assert_eq!(
        power_off_warnings(BOUND, &status("0000000000000000", "0000000000000000")),
        [
            "the key-power-off key will not work: the init system (PID 1) neither \
             handles nor blocks SIGRTMIN+4 (signal 38, from SigCgt and SigBlk in \
             /proc/1/status). Unbind the key, or use an init system that powers off on \
             that signal, such as systemd or sysvinit."
        ]
    );
    // Bit 38 (signal 39) is a neighbour, not the signal.
    let neighbour = "0000004000000000";
    assert_eq!(
        power_off_warnings(BOUND, &status(neighbour, neighbour)).len(),
        1
    );
}

#[test]
fn key_power_off_names_why_an_unreadable_status_does_not_answer() {
    let unreadable = Err("Permission denied (os error 13)".to_owned());
    assert_eq!(
        power_off_warnings(BOUND, &unreadable),
        [
            "the key-power-off key may not work: vtium cannot tell whether the init \
             system (PID 1) handles SIGRTMIN+4 (signal 38), because /proc/1/status could \
             not be read: Permission denied (os error 13)"
        ]
    );
    let says = |status: Result<String, String>, what: &str| {
        let lines = power_off_warnings(BOUND, &status);
        matches!(lines.as_slice(), [line] if line.contains(what))
    };
    let no_line = Ok("Name:\tinit\n".to_owned());
    assert!(says(no_line, "because /proc/1/status has no SigCgt line"));
    // A SigCgt without the bit sends the check on to SigBlk.
    let no_blk = Ok("Name:\tinit\nSigCgt:\t0\n".to_owned());
    assert!(says(no_blk, "because /proc/1/status has no SigBlk line"));
    assert!(says(
        status("zz", "0"),
        "because the SigCgt line of /proc/1/status is not a hex mask: \"zz\""
    ));
    assert!(says(
        status("0", "zz"),
        "because the SigBlk line of /proc/1/status is not a hex mask: \"zz\""
    ));
}

#[test]
fn an_unbound_key_power_off_checks_nothing() {
    let unbound = "key-reboot=<Ctrl><Alt>Delete\n";
    assert_eq!(
        power_off_warnings(unbound, &status("0", "0")),
        [] as [&str; 0]
    );
    assert_eq!(
        power_off_warnings(unbound, &Err("x".to_owned())),
        [] as [&str; 0]
    );
}

// #145: a frit's cell that does not suit it, where no font need be
// parsed to tell
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn the_frit_font_name_names_is_checked() {
    let dir = Scratch::new("check-frit");
    std::fs::write(
        dir.join("small.vtfrit"),
        "source=builtin:unifont\ncell=8x12\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("fine.vtfrit"),
        "source=outline:/f.ttf\ncell=10x20\n",
    )
    .unwrap();
    let config = |name: &str| {
        let args: Vec<OsString> = [SUBCOMMAND, "--font-name", name]
            .iter()
            .map(OsString::from)
            .collect();
        config::load(&args, &mut |_: &Path| Ok(None)).config
    };
    let trust = font::trust().below(&dir);
    assert_eq!(
        frit(&config("small"), &dir, &trust),
        [
            "font set small: line 1: the characters of source=builtin:unifont (8x16) do not fit the 8x12 character cell, so vtium does not use that source"
        ]
    );
    assert!(frit(&config("fine"), &dir, &trust).is_empty());
    assert!(
        frit(&config("ter-v16n"), &dir, &trust).is_empty(),
        "no frit"
    );
}

// check-config reads a set's layers as the start reads them, and
// reports a missing layer, a layer the ownership check refuses and a
// layer whose role= is wrong.
//
// References:
// - ADR-1204 D5: vtium check-config reads a set's layers too.
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn check_config_reports_a_sets_layers() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new("check-layers");
    let layers = dir.join("layers");
    std::fs::create_dir_all(&layers).unwrap();
    let write = |path: &Path, text: &str, mode: u32| {
        std::fs::write(path, text).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    };
    for d in [dir.as_ref(), layers.as_path()] {
        std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let set = dir.join("set.vtfrit");
    write(
        &set,
        "source=builtin:unifont\nlayer=fine\nlayer=gone\nlayer=theirs\nlayer=icons\n",
        0o644,
    );
    write(
        &layers.join("fine.vtlayer"),
        "role=emoji\nsource=font:E\n",
        0o644,
    );
    write(
        &layers.join("theirs.vtlayer"),
        "role=script\nsource=font:T\n",
        0o664,
    );
    write(
        &layers.join("icons.vtlayer"),
        "role=icons\nsource=font:I\n",
        0o644,
    );
    let trust = font::trust().below(&dir);
    let said = frit_layers("set", &set, &dir, &trust);
    let file = |name: &str| font::quoted(&layers.join(format!("{name}.vtlayer")));
    assert_eq!(
        said,
        [
            format!(
                "font set set: line 3: the layer gone is missing, because there is no file {}. vtium draws the font set without that layer.",
                file("gone")
            ),
            format!(
                "font set set: line 4: vtium does not read the layer theirs from {}, because other users can change {} (mode 664). Run `chmod go-w {}` so that only its owner can change it. vtium draws the font set without that layer.",
                file("theirs"),
                file("theirs"),
                layers.join("theirs.vtlayer").display()
            ),
            format!(
                "font set set: line 5: the layer icons in {} has an error: line 1: role=icons is not a role: use script, symbols, emoji or fallback. vtium draws the font set without that layer.",
                file("icons")
            ),
        ]
    );
    // a look naming the set counts each sentence against the file
    let text =
        "version = 1\n[look l]\nfont-name = set\n[head a]\nlook = l\n[rules]\ndisplay * -> a\n";
    let (_heads, conf, conf_trust) = heads_fixture("look-layers", text, 0o644);
    let settings = Settings {
        frits: &dir,
        trust,
        ..caps(4)
    };
    let out = conf_file(&conf, false, &conf_trust, &machine(true), &settings);
    let counted: Vec<&String> = out.counted.iter().filter(|l| l.contains("layer")).collect();
    let at = |line: &String| format!("{}:3: {line}", conf.display());
    assert_eq!(
        counted,
        said.iter()
            .map(at)
            .collect::<Vec<_>>()
            .iter()
            .collect::<Vec<_>>()
    );
}

// ADR-1401 D10, ADR-1403 D3: check-config reads the frit font-name
// names under the ownership check the start makes for the account it
// runs as, and counts a frit the check refuses, with the path that
// fails and the command that mends it. Run as another account than
// root, it notes where a start as root refuses a frit the account
// reads, as it notes for the font helper. The start's warnings say
// nothing of a refusal, which the font's own read logs at the start
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn a_frit_the_ownership_check_refuses_is_reported() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let dir = Scratch::new("check-trust");
    let open = dir.join("open");
    std::fs::create_dir_all(&open).unwrap();
    let mode = |path: &Path, mode: u32| {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    };
    for path in [
        dir.join("t.vtfrit"),
        dir.join("g.vtfrit"),
        open.join("t.vtfrit"),
    ] {
        std::fs::write(&path, "source=builtin:unifont\n").unwrap();
        mode(&path, 0o644);
    }
    mode(&dir, 0o755);
    mode(&dir.join("g.vtfrit"), 0o664);
    mode(&open, 0o775);
    let config = |name: &str| {
        let args: Vec<OsString> = [SUBCOMMAND, "--font-name", name]
            .iter()
            .map(OsString::from)
            .collect();
        config::load(&args, &mut |_: &Path| Ok(None)).config
    };
    let uid = std::fs::metadata(&dir).unwrap().uid();
    let mine = font::trust().below(&dir);
    let results = [
        font_name_frit(&config("t"), &dir, &mine, &DraftFrits::default()),
        font_name_frit(&config("g"), &dir, &mine, &DraftFrits::default()),
        font_name_frit(&config("t"), &open, &mine, &DraftFrits::default()),
        font_name_frit(&config("ter-v16n"), &dir, &mine, &DraftFrits::default()),
    ];
    let at_start = [
        frit(&config("g"), &dir, &mine),
        frit(&config("t"), &open, &mine),
    ];
    let refused = |name: &str, file: &Path, failed: &Path, mode: &str| {
        let (file, failed) = (file.display(), failed.display());
        format!(
            "vtium does not read the font set {name} from \"{file}\", because other users can change \"{failed}\" (mode {mode}). Run `chmod go-w {failed}` so that only its owner can change it. vtium uses its built-in font instead."
        )
    };
    let (g, t) = (dir.join("g.vtfrit"), dir.join("t.vtfrit"));
    let as_root = format!(
        "When vtium runs as root, it does not read the font set t from \"{t}\", because \"{t}\" belongs to user {uid}, not to root. Run `chown root:root {t}` to make root the owner. vtium would then use its built-in font.",
        t = t.display()
    );
    assert_eq!(
        results,
        [
            (None, Some(as_root)),
            (Some(refused("g", &g, &g, "664")), None),
            (
                Some(refused("t", &open.join("t.vtfrit"), &open, "775")),
                None
            ),
            (None, None),
        ]
    );
    assert_eq!(at_start, [[] as [String; 0], []]);
}

// the operator on #145, 2026-10-28: the density points convert at,
// display-dpi's, else each display's measured or assumed, which only
// the start can read, since check-config opens no display
#[test]
fn the_density_source_is_noted_outside_the_status() {
    let note = |args: &[&str]| notes_but(&["font-helper:", "image-helper:"], args);
    assert_eq!(
        note(&["--display-dpi=109"]),
        ["display-dpi=109: every display uses 109 DPI for font sizes, whatever density it reports"]
    );
    assert_eq!(
        note(&[]),
        [
            "each display uses the density it reports from its physical size for font sizes, or 96 DPI with a warning if it reports none; vtium logs which at start"
        ]
    );
    assert_eq!(run_with(&["--display-dpi=109"], ""), (Vec::new(), 0));
}

// a helper vtium would refuse is noted outside the status, since the
// start may run as another account and checks it again
#[test]
fn a_helper_vtium_would_refuse_is_noted() {
    let note = |args: &[&str]| notes_but(&["image-helper:"], args);
    let density = check_config::MEASURED_DENSITY;
    if let Some(sh) = roots_program() {
        let helper = format!("--font-helper={}", sh.display());
        assert_eq!(note(&[&helper]), [density]);
    }
    assert_eq!(
        note(&["--font-helper=bin/vtium-config"]),
        [
            density,
            "font-helper: vtium does not start the font helper bin/vtium-config: the path is not absolute. vtium would use its built-in font. vtium checks again at start, for the account it runs as."
        ]
    );
    assert_eq!(
        note(&["--font-helper=/"]),
        [
            density,
            "font-helper: vtium does not start the font helper /: it is not a regular file. vtium would use its built-in font. vtium checks again at start, for the account it runs as."
        ]
    );
    assert_eq!(
        run_with(&["--font-helper=bin/vtium-config"], ""),
        (Vec::new(), 0)
    );
}

// ADR-1101 D1: image-helper defaults to the path vtium was built
// with, the one vtium-platform starts by default too
#[cfg(feature = "images")]
// covers: image.helper
#[test]
fn image_helper_defaults_to_the_built_path() {
    let loaded = config::load(&[OsString::from(SUBCOMMAND)], &mut |_: &Path| Ok(None));
    assert_eq!(
        crate::head::images::program(&loaded.config),
        Path::new(crate::head::images::PROGRAM)
    );
    assert_eq!(
        crate::head::images::PROGRAM,
        vtium_platform::image_helper::PROGRAM
    );
    let args: Vec<OsString> = [SUBCOMMAND, "--image-helper=/opt/vtium-image"]
        .iter()
        .map(OsString::from)
        .collect();
    let loaded = config::load(&args, &mut |_: &Path| Ok(None));
    assert_eq!(
        crate::head::images::program(&loaded.config),
        Path::new("/opt/vtium-image")
    );
}

// ADR-1101 D1: an image helper vtium would refuse is noted outside the
// status, as the font helper is, since the start may run as another
// account and checks it again
#[cfg(feature = "images")]
// covers: image.helper
#[test]
fn an_image_helper_vtium_would_refuse_is_noted() {
    let note = |args: &[&str]| notes_but(&["font-helper:"], args);
    let density = check_config::MEASURED_DENSITY;
    if let Some(sh) = roots_program() {
        let helper = format!("--image-helper={}", sh.display());
        assert_eq!(note(&[&helper]), [density]);
    }
    assert_eq!(
        note(&["--image-helper=bin/vtium-image"]),
        [
            density,
            "image-helper: the image helper's path bin/vtium-image is not absolute, so vtium does not start it. vtium would refuse images sent as files or shared memory, and PNG and zlib images. vtium checks again at start, for the account it runs as."
        ]
    );
    assert_eq!(
        note(&["--image-helper=/"]),
        [
            density,
            "image-helper: the image helper / is not a regular file, so vtium does not start it. vtium would refuse images sent as files or shared memory, and PNG and zlib images. vtium checks again at start, for the account it runs as."
        ]
    );
    assert_eq!(
        run_with(&["--image-helper=bin/vtium-image"], ""),
        (Vec::new(), 0)
    );
}

// A login picture that vtium could not use, at the top level or in a
// look, is noted on standard output with the reason the start would
// log, outside the exit status; a PNG file draws no note.
#[cfg(feature = "images")]
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn a_login_picture_vtium_cannot_use_is_noted() {
    let dir = Scratch::new("check-login-image");
    let png = dir.join("login.png");
    std::fs::write(&png, b"\x89PNG\r\n\x1a\nrest").unwrap();
    let text = dir.join("login.txt");
    std::fs::write(&text, b"no picture").unwrap();
    let missing = dir.join("missing.png");
    let note = |path: &Path| {
        notes_but(
            &["font-helper:", "image-helper:"],
            &[&format!("--login-image={}", path.display())],
        )
    };
    let density = check_config::MEASURED_DENSITY;
    assert_eq!(note(&png), [density]);
    assert_eq!(
        note(&missing),
        [
            density.to_owned(),
            format!(
                "login-image: vtium cannot show the login picture \"{}\", because the file does not exist. vtium would start the login prompt without it.",
                missing.display()
            )
        ]
    );
    assert_eq!(
        note(&dir),
        [
            density.to_owned(),
            format!(
                "login-image: vtium cannot show the login picture \"{}\", because the file is not a regular file. vtium would start the login prompt without it.",
                dir.display()
            )
        ]
    );
    let look = format!(
        "version = 1\n[look big]\nlogin-image = {}\n[head a]\nlook = big\n[rules]\ndisplay * -> a\n",
        text.display()
    );
    let (_heads, file, trust) = heads_fixture("login-image", &look, 0o644);
    let out = conf_file(&file, false, &trust, &machine(true), &caps(4));
    assert_eq!(out.counted, [] as [String; 0]);
    let said: Vec<&String> = out
        .notes
        .iter()
        .filter(|n| n.contains("login picture"))
        .collect();
    assert_eq!(
        said,
        [&format!(
            "{}:3: Look \"big\": vtium cannot show its login picture \"{}\", because the file is not a PNG or JPEG picture. vtium would start the login prompt without it.",
            file.display(),
            text.display()
        )]
    );
}

// ADR-1101 D1: check-config run as another account notes an image
// helper the root start refuses
#[cfg(feature = "images")]
// covers: image.helper
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn an_image_helper_the_root_start_refuses_is_noted() {
    let dir = Scratch::new("check-image");
    let program = dir.join("vtium-image");
    std::fs::write(&program, "").unwrap();
    let owner = std::fs::metadata(&program)
        .map(|m| std::os::unix::fs::MetadataExt::uid(&m))
        .unwrap();
    if let Some(sh) = roots_program() {
        assert_eq!(image_helper_note(sh, 1000), None);
    }
    let note = image_helper_note(&program, owner);
    if owner == 0 {
        assert_eq!(note, None, "root's file");
    } else {
        assert_eq!(
            note.unwrap(),
            format!(
                "image-helper: when vtium runs as root, it refuses this helper: the image helper {} is owned by uid {owner}, which is neither root nor the account vtium runs as, so vtium does not start it. vtium would then refuse images sent as files or shared memory, and PNG and zlib images.",
                program.display()
            )
        );
    }
}

// check-config run as another account notes a helper the root start
// refuses
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn a_helper_the_root_start_refuses_is_noted() {
    let dir = Scratch::new("check-helper");
    let program = dir.join("vtium-config");
    std::fs::write(&program, "").unwrap();
    // owned by the test's account, as a build is
    let owner = std::fs::metadata(&program)
        .map(|m| std::os::unix::fs::MetadataExt::uid(&m))
        .unwrap();
    if let Some(sh) = roots_program() {
        assert_eq!(helper_note(sh, 1000), None);
        assert_eq!(helper_note(sh, 0), None);
    }
    let note = helper_note(&program, owner);
    if owner == 0 {
        assert_eq!(note, None, "root's file");
    } else {
        assert_eq!(
            note.unwrap(),
            format!(
                "font-helper: when vtium runs as root, it refuses this helper: vtium does not start the font helper {}: it belongs to uid {owner}, which is neither root nor vtium's own account. vtium would then use its built-in font.",
                program.display()
            )
        );
        let other = helper_note(&program, owner.wrapping_add(1)).unwrap();
        assert!(
            other.starts_with("font-helper: vtium does not start the font helper"),
            "{other}"
        );
    }
}

// font-frits names the directory check-config finds a profile's frit in
#[test]
fn check_config_finds_the_frits_in_font_frits() {
    let args: Vec<OsString> = [SUBCOMMAND, "--font-frits=/srv/frits"]
        .iter()
        .map(OsString::from)
        .collect();
    let loaded = config::load(&args, &mut |_: &Path| Ok(None));
    assert_eq!(
        Host::system(&loaded.config, None).frits,
        Path::new("/srv/frits")
    );
    let loaded = config::load(&[OsString::from(SUBCOMMAND)], &mut |_: &Path| Ok(None));
    assert_eq!(
        Host::system(&loaded.config, None).frits,
        Path::new(font::FRITS)
    );
}

/// A fixture directory of its own, which the test removes when it
/// ends, a head file in it, and the trust that reads the file as the
/// start reads root's files.
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn heads_fixture(name: &str, text: &str, mode: u32) -> (Scratch, PathBuf, Trust) {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new(&format!("check-heads-{name}"));
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let file = dir.join("heads.conf");
    std::fs::write(&file, text).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).unwrap();
    let trust = Trust::owner(rustix::process::geteuid().as_raw()).below(&dir);
    (dir, file, trust)
}

/// vtium.conf's caps with `head_cap` and the default total, and frits
/// nowhere.
fn caps(head_cap: u32) -> Settings<'static> {
    Settings {
        head_cap,
        total: vtium_settings::CONSOLES_TOTAL_DEFAULT,
        frits: Path::new("/nonexistent/vtium/frits"),
        trust: font::trust(),
        squeeze: None,
        sources: None,
        gate: None,
        draft: DraftFrits::default(),
    }
}

/// A machine where a udev daemon runs or not, whose devices are not
/// read.
fn machine(udev_daemon: bool) -> Machine<'static> {
    Machine {
        udev_daemon,
        devices: None,
        vts: VtsEnabled::FileAlone,
    }
}

fn checked(name: &str, text: &str, udev: bool) -> (HeadCheck, String) {
    let (_dir, file, trust) = heads_fixture(name, text, 0o644);
    let out = conf_file(&file, false, &trust, &machine(udev), &caps(4));
    (out, file.display().to_string())
}

// ADR-1401 D7: a warning about the file itself counts against it
#[test]
fn a_head_file_warning_counts_and_its_notes_do_not() {
    let (out, f) = checked(
        "warn",
        "version = 1\n[head desk]\n[rules]\ndisplay * -> desk\ninput * -> roaming\ninput class:keyboard -> desk\n",
        true,
    );
    assert_eq!(
        out.counted,
        [format!(
            "{f}:6: this rule never matches: the rule on line 5 matches every device it would"
        )]
    );
    assert_eq!(verdict(&out, &f), [AS_WRITTEN, UNSET]);
    assert_eq!(
        out.notes,
        [RULES_ALONE, AS_WRITTEN, UNSET].map(|n| format!("{f}: {n}"))
    );
}

/// The last two notes of `out`, the outcome and the policy, without
/// the path `f` before them.
fn verdict<'a>(out: &'a HeadCheck, f: &str) -> Vec<&'a str> {
    let prefix = format!("{f}: ");
    let at = out.notes.len().saturating_sub(2);
    out.notes
        .get(at..)
        .unwrap_or_default()
        .iter()
        .map(|n| n.strip_prefix(&prefix).unwrap_or(n))
        .collect()
}

const OPEN: &str = "The policy is open: an input device no rule places roams, a head is isolated only where it says `isolated = yes`, and the keyboard safeguards (anti-lockout and the rescue key) are on.";
const UNSET: &str = "The file sets no `policy`, so its policy is open: an input device no rule places roams, a head is isolated only where it says `isolated = yes`, and the keyboard safeguards (anti-lockout and the rescue key) are on.";
const CLOSED: &str = "The policy is closed: an input device no rule places is ignored, a head without an `isolated` line is isolated, and the keyboard safeguards (anti-lockout and the rescue key) are off.";
const AS_WRITTEN: &str = "vtium starts with this file as written and shows no notice.";
/// What a check with no devices says of how it judged the route
/// (ADR-1403 D3).
const RULES_ALONE: &str =
    "vtium checked the routes by the rules alone, without the connected devices.";
const NO_DISPLAY: &str = "No display is connected, so vtium checked the routes by the rules alone.";
const PARTLY_OPEN: &str = "vtium starts with the heads and rules that have no errors, then the built-in rules. Every head acts as not isolated, whatever its `isolated` says. It shows `vtium.conf has errors: running the parts without errors.`.";
const PARTLY_UNSET_POLICY: &str = "The file sets no `policy`, so its policy is open. Because parts of the file have errors, every head acts as not isolated, whatever its `isolated` says; an input device that no working rule places goes on to the built-in rules; and the keyboard safeguards (anti-lockout and the rescue key) are on.";
const PARTLY_OPEN_POLICY: &str = "The policy is open. Because parts of the file have errors, every head acts as not isolated, whatever its `isolated` says; an input device that no working rule places goes on to the built-in rules; and the keyboard safeguards (anti-lockout and the rescue key) are on.";

// ADR-1401 D7, level 3: each error names its line and what it drops,
// and the file is read on past it, so what survives is checked too
// covers: platform.heads.rules
#[test]
fn each_error_names_its_line_and_what_it_drops() {
    let (out, f) = checked(
        "drops",
        "version = 1\n\
         [head desk]\n\
         frob = 1\n\
         isolated = maybe\n\
         [head kiosk]\n\
         [head kiosk]\n\
         consoles = 2\n\
         [rules]\n\
         display * -> kiosk\n\
         input path:pci-0000:15:00.0-usb-0:3.3 -> desc\n\
         input * -> roaming\n\
         input * -> desk\n\
         input class:keyboard -> roaming\n",
        true,
    );
    let at = |line: usize, what: &str| format!("{f}:{line}: {what}");
    assert_eq!(
        out.counted,
        [
            at(
                3,
                "This section has no key named \"frob\". Check the spelling, or remove the line."
            ),
            at(3, "Because of this error, vtium ignores this line."),
            at(
                4,
                "`isolated` cannot be \"maybe\". Change it to one of `yes`, `no`."
            ),
            at(
                4,
                "Because of this error, vtium ignores this head and every rule that names it."
            ),
            at(
                6,
                "The file already has a head named \"kiosk\". Rename one of the two."
            ),
            at(
                6,
                "Because of this error, vtium ignores this whole section."
            ),
            at(
                10,
                "This rule sends devices to head \"desc\", but the file has no head of that name. Correct the name, or add a `[head NAME]` section for it."
            ),
            at(
                10,
                "Because of this error, vtium ignores this rule, and each device goes on to the next rule."
            ),
            // the survivors' own warning: line 12 went with head desk
            at(
                13,
                "this rule never matches: the rule on line 11 matches every device it would"
            ),
        ]
    );
    assert_eq!(verdict(&out, &f), [PARTLY_OPEN, PARTLY_UNSET_POLICY]);
}

// ADR-1401 D7: after its errors the report gives what a start does
// with the file, by D7's outcomes table, and the policy it states
// covers: platform.heads.policy
#[test]
fn the_outcome_and_the_policy_close_the_report() {
    let head = "[head a]\n[rules]\ndisplay * -> a\n";
    let rows = [
        (
            format!("version = 1\npolicy = open\n{head}"),
            "vtium starts with this file as written and shows no notice.",
            OPEN,
        ),
        (
            format!("policy = closed\nversion = 1\n{head}"),
            "vtium starts with this file as written and shows no notice.",
            CLOSED,
        ),
        ("version = 1\n".to_owned(), check_config::NO_FLOW, UNSET),
        (
            "version = 1\n[rules]\ninput * -> roaming\n".to_owned(),
            "This file has rules but no head, so vtium starts with its rules. Each display that no rule turns off shows a notice that it is not configured for any terminal.",
            UNSET,
        ),
        (
            "version = 1\npolicy = closed\n".to_owned(),
            "This file has no head and its policy is closed, so vtium starts with every display off, takes no input and shows nothing.",
            CLOSED,
        ),
        (
            format!("version = 1\n{head}display * -> b\n"),
            PARTLY_OPEN,
            PARTLY_UNSET_POLICY,
        ),
        (
            format!("version = 1\npolicy = closed\n{head}display * -> b\n"),
            "vtium starts with the heads and rules that have no errors, and the rescue key off. Each head keeps its `isolated` setting, and a head without one is isolated. It shows `vtium.conf has errors: running the parts without errors.`.",
            CLOSED,
        ),
        (
            format!("version = 2\n{head}"),
            "Every head in this file has errors, so vtium starts with its built-in configuration and shows `vtium.conf not used: running the built-in configuration.`.",
            UNSET,
        ),
        (
            format!("version = 2\npolicy = closed\n{head}"),
            "Every head in this file has errors and the policy is closed, so vtium takes no input and shows an error screen on the boot display, starting `vtium.conf has errors and counts as closed: all input is off.`.",
            CLOSED,
        ),
        (
            format!("version = 1\npolicy = maybe\n{head}"),
            "vtium cannot read a `policy` line of this file, so at start it treats the whole file as broken with the closed policy: it takes no input and shows an error screen on the boot display, starting `vtium.conf has errors and counts as closed: all input is off.`.",
            "vtium cannot read the `policy` line above the first section, so the file's policy is closed. Write that line as `policy = open` or `policy = closed`.",
        ),
    ];
    for (n, (text, outcome, policy)) in rows.iter().enumerate() {
        let (out, f) = checked(&format!("row-{n}"), text, true);
        assert_eq!(verdict(&out, &f), [*outcome, *policy], "{text}");
        // an error counts against the file, and so does a flow that
        // wires no complete route, judged here by its rules alone
        // (ADR-1403 D3): only the open file whose head has a display
        // rule, and the file with no flow, count nothing
        let clean =
            *text == format!("version = 1\npolicy = open\n{head}") || text == "version = 1\n";
        assert_eq!(out.counted.is_empty(), clean, "{text}: {:?}", out.counted);
    }
}

// ADR-1401 D7: a line that is not UTF-8 drops only its rule, and the
// file runs what survives
// covers: platform.heads.rules
#[test]
fn a_line_that_is_not_utf8_drops_only_its_rule() {
    let (_dir, file, trust) = heads_fixture("bytes", "", 0o644);
    std::fs::write(
        &file,
        b"version = 1\n[head a]\n[rules]\ndisplay * -> a\ninput name:\xff -> a\n",
    )
    .ok();
    let f = file.display().to_string();
    let out = conf_file(&file, false, &trust, &machine(true), &caps(4));
    assert_eq!(
        out.counted,
        [
            format!("{f}:5: This line is not UTF-8 text. Save the file as UTF-8."),
            format!(
                "{f}:5: Because of this error, vtium ignores this rule, and each device goes on to the next rule."
            ),
        ]
    );
    assert_eq!(verdict(&out, &f), [PARTLY_OPEN, PARTLY_UNSET_POLICY]);
}

// ADR-1401 D10: a head's `consoles` past vtium.conf's
// `head-consoles-max` gets the cap, and counts against the file, once,
// at its `consoles` line, past the hard cap of 50 too
// covers: platform.heads.rules
#[test]
fn consoles_past_the_head_cap_count_against_the_file() {
    let past = |n: u32| {
        format!(
            "Head \"a\" asks for more consoles than `head-consoles-max` allows, so it gets {n}. Lower its `consoles` to {n} or fewer, or raise `head-consoles-max` in vtium.conf."
        )
    };
    for (consoles, cap, want) in [
        (5, 4, Some(past(4))),
        (60, 4, Some(past(4))),
        (5, 6, None),
        (
            60,
            50,
            Some(
                "`consoles` is too high: a head can have at most 50 consoles, so this head gets 50. Lower it to 50 or fewer."
                    .to_owned(),
            ),
        ),
    ] {
        let text =
            format!("version = 1\n[head a]\nconsoles = {consoles}\n[rules]\ndisplay * -> a\n");
        let (_dir, file, trust) = heads_fixture(&format!("cap-{consoles}-{cap}"), &text, 0o644);
        let f = file.display().to_string();
        let settings = Settings {
            total: 64,
            ..caps(cap)
        };
        let out = conf_file(&file, false, &trust, &machine(true), &settings);
        let want: Vec<String> = want.iter().map(|w| format!("{f}:3: {w}")).collect();
        assert_eq!(out.counted, want, "consoles {consoles}, cap {cap}");
    }
}

// ADR-1401 D10: with vtium.conf's caps unset, check-config caps as the
// seat does, 4 consoles a head and 8 in all
#[test]
fn the_caps_unset_are_the_seats() {
    let k = Settings::of(&config::Config::default(), Path::new("/f"), Trust::ROOT);
    assert_eq!((k.head_cap, k.total), (4, 8));
    assert_eq!(
        (k.head_cap, k.total),
        (
            vtium_settings::HEAD_CONSOLES_DEFAULT,
            vtium_settings::CONSOLES_TOTAL_DEFAULT
        )
    );
}

// ADR-1401 D10: past `consoles-max`, each head keeps its first console
// and the others go in the file's order while the total lasts; a head
// left short counts against the file
// covers: platform.heads.rules
#[test]
fn consoles_past_the_total_count_against_the_file() {
    let (out, f) = checked(
        "total",
        "version = 1\n[head a]\nconsoles = 4\n[head b]\nconsoles = 4\n[head c]\nconsoles = 4\n\
         [rules]\ndisplay * -> a\ninput * -> roaming\n",
        true,
    );
    let short = |line: usize, head: &str, gets: u32| {
        format!(
            "{f}:{line}: The heads together ask for more consoles than `consoles-max` (8) allows, so head \"{head}\" gets {gets}. Lower the heads' `consoles`, or raise `consoles-max` in vtium.conf."
        )
    };
    assert_eq!(out.counted, [short(5, "b", 3), short(7, "c", 1)]);
}

// ADR-1401 D7, ADR-1302 D14: a look whose font set leaves everyday
// letters of the locale's language to the squeeze is noted, outside the exit
// status, naming the look, the font set, the script, how many letters
// and the installed font the glyph helper names. The stand-in answers
// as the helper does: it refuses a frit it cannot read, covers what
// the frit's sources hold, and names the font that covers the rest.
// GNU Unifont, the built-in font at 8x16, draws Devanagari 16 pixels
// wide, so it squeezes them. A font set that covers the locale's
// script takes no note, and neither does the C locale
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn a_font_set_that_leaves_the_locales_script_to_the_squeeze_is_noted() {
    let text = "version = 1\n[look big]\nfont-name = desk\n[head a]\nlook = big\n[rules]\ndisplay * -> a\n";
    let (_dir, file, trust) = heads_fixture("squeeze", text, 0o644);
    let f = file.display().to_string();
    let frits = file.parent().unwrap().join("frits");
    std::fs::create_dir_all(&frits).unwrap();
    std::fs::write(frits.join("desk.vtfrit"), "source=font:Latin Only\n").unwrap();
    let ask = |frit: &helper::FritText, letters: &[u32]| {
        if frit.name != "desk" {
            return Err("no such font set".to_owned());
        }
        // a font set covers every everyday letter of the language its
        // one source is named for: Latin English's, Devanagari Hindi's
        let covers = |(script, locale): (&str, &str)| {
            let all = vtium_font::letters::of_locale(locale);
            frit.text.contains(script).then_some(all)
        };
        let held: Vec<char> = [("Latin", "en"), ("Devanagari", "hi")]
            .into_iter()
            .filter_map(covers)
            .flatten()
            .collect();
        let covered = letters
            .iter()
            .copied()
            .filter(|&c| char::from_u32(c).is_some_and(|c| held.contains(&c)))
            .collect();
        Ok((covered, Some("Noto Sans Devanagari".to_owned())))
    };
    let check = |locale: &str| {
        let settings = Settings {
            frits: &frits,
            trust: font::trust().below(file.parent().unwrap()),
            squeeze: Some(Squeeze { locale, ask: &ask }),
            ..caps(4)
        };
        conf_file(&file, false, &trust, &machine(true), &settings)
    };
    let left = vtium_font::letters::of_locale("hi_IN")
        .into_iter()
        .filter(|&c| {
            font::builtin_squeezes(
                c,
                vtium_font::Cell {
                    width: 8,
                    height: 16,
                },
                "hi_IN",
            )
        })
        .count();
    // 67 of Hindi's exemplar characters are two cells wide in GNU Unifont
    assert_eq!(left, 67);
    let out = check("hi_IN.UTF-8");
    assert!(out.counted.is_empty(), "{:?}", out.counted);
    let want = format!(
        "{f}:3: Look \"big\": the font set desk has no glyph for {left} letters that the system's language writes in the Devanagari script. vtium draws those letters from its built-in font squeezed into their character cells, which keeps them whole but narrow. The installed font Noto Sans Devanagari covers them; add it to the font set with a source=font:Noto Sans Devanagari line."
    );
    assert!(out.notes.contains(&want), "{:#?}", out.notes);
    let squeezed = |out: &HeadCheck| out.notes.iter().any(|n| n.contains("squeezed"));
    assert!(!squeezed(&check("en_US.UTF-8")));
    assert!(!squeezed(&check("C")));
    std::fs::write(
        frits.join("desk.vtfrit"),
        "source=font:Noto Sans Devanagari\n",
    )
    .unwrap();
    assert!(!squeezed(&check("hi_IN.UTF-8")));
}

// A look's font set that needs a font this machine lacks, or has in an
// older version, is reported with the font, the version the set asks
// for, the font's page and the help page on installing fonts, as the
// glyph helper answers. A missing font, and one that fails its check=,
// is a warning, which counts against the file. An older version and a
// file with another hash are notes, because vtium still uses the font.
// A source that names the system's font gets no line, and a helper
// that cannot answer gives one note that says so.
//
// References:
// - ADR-1204 D7: check-config names each missing font as vtium config
//   fonts does.
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn a_font_a_looks_font_set_lacks_is_noted_with_its_page() {
    let text = "version = 1\n[look big]\nfont-name = desk\n[head a]\nlook = big\n[rules]\ndisplay * -> a\n";
    let (_dir, file, trust) = heads_fixture("font-needs", text, 0o644);
    let f = file.display().to_string();
    let frits = file.parent().unwrap().join("frits");
    std::fs::create_dir_all(frits.join("layers")).unwrap();
    std::fs::write(
        frits.join("desk.vtfrit"),
        "source=font:Gone Mono\nversion=2.304\nurl=https://example.org/gone\nsource=font:Old Mono\nversion=3.1\nlayer=nerd\nsource=vconsole\n",
    )
    .unwrap();
    std::fs::write(
        frits.join("layers/nerd.vtlayer"),
        "role=symbols\nsource=font:Plain\nsource=font:Nerd\ncheck=U+E0B0\n",
    )
    .unwrap();
    let answer = "missing\t1\tfont:Gone Mono\tno font of family Gone Mono\n\
                  missing\t6\tfont:Nerd\tit lacks U+E0B0\n\
                  missing\t7\tvconsole\tno FONT=\n\
                  older\t4\tfont:Old Mono\t3.000\n\
                  lacks\t6\tfont:Nerd\tU+E0B0";
    let ask = |frit: &helper::FritText| {
        if frit.name == "desk" && frit.text.contains("source=font:Nerd") {
            Ok(answer.to_owned())
        } else {
            Err("the layer is not in place".to_owned())
        }
    };
    let check = |sources: Option<FontSources<'_>>| {
        let settings = Settings {
            frits: &frits,
            trust: font::trust().below(file.parent().unwrap()),
            sources,
            ..caps(4)
        };
        conf_file(&file, false, &trust, &machine(true), &settings)
    };
    let out = check(Some(FontSources { ask: &ask }));
    let page = "To install a font that a font set needs, see https://vtium.github.io/vtium/fonts/#install-a-missing-font.";
    let warned = [
        format!(
            "{f}:3: Font set desk needs the font Gone Mono, version 2.304 or later, and no font of that name is installed. You can get the font from https://example.org/gone. {page}"
        ),
        format!(
            "{f}:3: Font set desk needs the font Nerd. The installed font of that name lacks the character U+E0B0, so it is a different font, and vtium does not use it. {page}"
        ),
    ];
    assert_eq!(out.counted, warned, "{out:#?}");
    let noted: Vec<&String> = out
        .notes
        .iter()
        .filter(|n| n.contains("Font set desk"))
        .collect();
    assert_eq!(
        noted,
        [&format!(
            "{f}:3: Font set desk asks for the font Old Mono, version 3.1 or later, and the installed font is version 3.000. vtium still uses it. {page}"
        )],
        "{:#?}",
        out.notes
    );
    let none = check(None);
    assert!(!none.notes.iter().any(|n| n.contains("Font set desk")));
    // a helper that gives no answer is said, so the check never passes
    // in silence on fonts it could not ask about
    let failing = |_: &helper::FritText| Err("the helper did not start".to_owned());
    let failed = check(Some(FontSources { ask: &failing }));
    assert!(failed.counted.is_empty(), "{:?}", failed.counted);
    let named: Vec<&String> = failed
        .notes
        .iter()
        .filter(|n| n.contains("Font set desk"))
        .collect();
    assert_eq!(
        named,
        [&format!(
            "{f}:3: Font set desk: vtium cannot check whether the fonts it needs are installed: the helper did not start."
        )]
    );
}

/// The glyph helper's answer to a sources query for `frit`:
/// vtium-config's serve mode, run in this process on the request the
/// console's helper sends, so the lines are the ones the real helper
/// sends. A frit the helper draws nothing from, and that does not
/// read, is its reason.
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
fn served_sources(frit: &helper::FritText) -> Result<String, String> {
    use vtium_config::serve;
    let request = serve::ServeRequest {
        kind: serve::SOURCES,
        frit: serve::FritText {
            name: frit.name.clone(),
            text: frit.text.clone(),
        },
        cell: vtium_font::Cell {
            width: 0,
            height: 0,
        },
        band: vtium_font::spacing::Band::NONE,
        style: vtium_font::Style::default(),
        script: vtium_font::Script::Baseline,
        doubled: None,
        cluster_width: Default::default(),
        codepoints: Vec::new(),
        clusters: Vec::new(),
        held: Vec::new(),
        console: None,
    };
    let mut out = Vec::new();
    let status = serve::serve(
        std::io::Cursor::new(request.encode()),
        &mut out,
        &serve::Paths::default(),
        std::time::Duration::from_secs(5),
    );
    assert_eq!(status, 0);
    let mut rest = out.as_slice();
    while let [k0, k1, k2, k3, l0, l1, l2, l3, tail @ ..] = rest {
        let kind = u32::from_le_bytes([*k0, *k1, *k2, *k3]);
        let len = u32::from_le_bytes([*l0, *l1, *l2, *l3]) as usize;
        let text = String::from_utf8(tail[..len].to_vec()).unwrap();
        rest = &tail[len..];
        match kind {
            serve::FRAME_SOURCES => return Ok(text),
            serve::FRAME_DRAWS_NOTHING => return Err(text),
            _ => {}
        }
    }
    Err("the helper sent no answer".to_owned())
}

// A look's font set none of whose fonts this machine has draws nothing,
// and check-config still names each font it needs, with the version
// and the page the set gives, a layer's font among them. The lines come
// from the glyph helper's own answer, so a layer's font is found by the
// line the helper names for it, the set's layer= line.
//
// References:
// - ADR-1204 D7: check-config names each missing font as vtium config
//   fonts does, whether or not the set has a font left.
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn a_set_with_no_font_left_names_each_font_from_the_helpers_answer() {
    let text = "version = 1\n[look big]\nfont-name = desk\n[head a]\nlook = big\n[rules]\ndisplay * -> a\n";
    let (_dir, file, trust) = heads_fixture("font-needs-none-left", text, 0o644);
    let f = file.display().to_string();
    let frits = file.parent().unwrap().join("frits");
    std::fs::create_dir_all(frits.join("layers")).unwrap();
    std::fs::write(
        frits.join("desk.vtfrit"),
        "source=font:Gone Mono Nowhere\nversion=2.304\nurl=https://example.org/gone\nlayer=nerd\n",
    )
    .unwrap();
    std::fs::write(
        frits.join("layers/nerd.vtlayer"),
        "role=symbols\nsource=font:Nerd Gone Nowhere\nversion=1.2\nurl=https://example.org/nerd\n",
    )
    .unwrap();
    let settings = Settings {
        frits: &frits,
        trust: font::trust().below(file.parent().unwrap()),
        sources: Some(FontSources {
            ask: &served_sources,
        }),
        ..caps(4)
    };
    let out = conf_file(&file, false, &trust, &machine(true), &settings);
    let page = "To install a font that a font set needs, see https://vtium.github.io/vtium/fonts/#install-a-missing-font.";
    let want = [
        format!(
            "{f}:3: Font set desk needs the font Gone Mono Nowhere, version 2.304 or later, and no font of that name is installed. You can get the font from https://example.org/gone. {page}"
        ),
        format!(
            "{f}:3: Font set desk needs the font Nerd Gone Nowhere, version 1.2 or later, and no font of that name is installed. You can get the font from https://example.org/nerd. {page}"
        ),
    ];
    assert_eq!(out.counted, want, "{out:#?}");
    assert!(!out.notes.iter().any(|n| n.contains("Font set desk")));
}

/// What check-config says of the look `big`'s font set `desk`, given
/// as `frit`, with each `layers` entry, a name and its text, written to
/// the font sets' `layers` folder. The fonts this machine lacks come
/// from the glyph helper's own answer. Returns the vtium.conf's path
/// and the result, with the folder kept alive.
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn desk_needs(fixture: &str, frit: &str, layers: &[(&str, &str)]) -> (Scratch, String, HeadCheck) {
    let text = "version = 1\n[look big]\nfont-name = desk\n[head a]\nlook = big\n[rules]\ndisplay * -> a\n";
    let (dir, file, trust) = heads_fixture(fixture, text, 0o644);
    let frits = file.parent().unwrap().join("frits");
    std::fs::create_dir_all(frits.join("layers")).unwrap();
    std::fs::write(frits.join("desk.vtfrit"), frit).unwrap();
    for (name, body) in layers {
        std::fs::write(frits.join(format!("layers/{name}.vtlayer")), body).unwrap();
    }
    let settings = Settings {
        frits: &frits,
        trust: font::trust().below(file.parent().unwrap()),
        sources: Some(FontSources {
            ask: &served_sources,
        }),
        ..caps(4)
    };
    let out = conf_file(&file, false, &trust, &machine(true), &settings);
    (dir, file.display().to_string(), out)
}

/// A BDF font of one glyph, `A`, in an 8×16 cell.
const ONE_GLYPH_BDF: &str = "STARTFONT 2.1\nFONT -test-\nFONTBOUNDINGBOX 8 16 0 -4\nSTARTPROPERTIES 3\nFONT_ASCENT 12\nFONT_DESCENT 4\nCHARSET_REGISTRY \"ISO10646\"\nENDPROPERTIES\nCHARS 1\nSTARTCHAR A\nENCODING 65\nDWIDTH 8 0\nBBX 8 16 0 -4\nBITMAP\n18\n18\n18\n18\n18\n18\n18\n18\n18\n18\n18\n18\n18\n18\n18\n18\nENDCHAR\nENDFONT\n";

// A layer's font named by the path of a .hex file, such as the unifont
// layer's, is a font this machine can lack like any other file, so
// check-config names it with its version and its page.
//
// References:
// - ADR-1204 D5: the unifont layer's hex: source.
// - ADR-1204 D7: check-config names each missing font as vtium config
//   fonts does.
#[test]
fn a_missing_hex_font_is_named_with_its_page() {
    let (_dir, f, out) = desk_needs(
        "font-needs-hex",
        "source=builtin:unifont\nlayer=uni\n",
        &[(
            "uni",
            "role=fallback\nsource=hex:/nonesuch/unifont_all-17.0.03.hex\nversion=17.0.03\nurl=https://unifoundry.com/pub/unifont/unifont-17.0.03/\n",
        )],
    );
    let page = "To install a font that a font set needs, see https://vtium.github.io/vtium/fonts/#install-a-missing-font.";
    assert_eq!(
        out.counted,
        [format!(
            "{f}:3: Font set desk needs the font /nonesuch/unifont_all-17.0.03.hex, version 17.0.03 or later, and there is no file at that path. You can get the font from https://unifoundry.com/pub/unifont/unifont-17.0.03/. {page}"
        )],
        "{out:#?}"
    );
}

// A strike is a file of its source's font at one more height, so a
// missing strike is named with its source's version= and url=, whether
// those lines come before the strike= line or after it. A sha256= line
// after a strike= line belongs to the strike, and ends nothing.
//
// References:
// - ADR-1204 D2: the strike= key, and the lines that belong to a strike.
// - ADR-1204 D7: check-config names each missing font as vtium config
//   fonts does.
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn a_missing_strike_is_named_with_its_sources_version_and_page() {
    let fonts = Scratch::new("font-needs-strike-files");
    let (regular, bold) = (fonts.join("ter-u16n.bdf"), fonts.join("ter-u16b.bdf"));
    std::fs::write(&regular, ONE_GLYPH_BDF).unwrap();
    std::fs::write(&bold, ONE_GLYPH_BDF).unwrap();
    let hash = "0".repeat(64);
    let frit = format!(
        "source=bdf:{}\nversion=4.49.1\nurl=https://example.org/terminus\nstrike=bdf:/nonesuch/ter-u12n.bdf.gz\nsha256={hash}\nstrike=bdf:/nonesuch/ter-u14n.bdf.gz\nsource=bdf:{}\nstyle=bold\nstrike=bdf:/nonesuch/ter-u12b.bdf.gz\nsha256={hash}\nversion=2.0\nurl=https://example.org/bold\n",
        regular.display(),
        bold.display()
    );
    let (_dir, f, out) = desk_needs("font-needs-strike", &frit, &[]);
    let page = "To install a font that a font set needs, see https://vtium.github.io/vtium/fonts/#install-a-missing-font.";
    let missing = |font: &str, version: &str, url: &str| {
        format!(
            "{f}:3: Font set desk needs the font {font}, version {version} or later, and there is no file at that path. You can get the font from {url}. {page}"
        )
    };
    assert_eq!(
        out.counted,
        [
            missing(
                "/nonesuch/ter-u12n.bdf.gz",
                "4.49.1",
                "https://example.org/terminus"
            ),
            missing(
                "/nonesuch/ter-u14n.bdf.gz",
                "4.49.1",
                "https://example.org/terminus"
            ),
            missing(
                "/nonesuch/ter-u12b.bdf.gz",
                "2.0",
                "https://example.org/bold"
            ),
        ],
        "{out:#?}"
    );
}

// ADR-1401 D7, ADR-1403 D5: a look whose font-name names a frit not
// installed is a warning about the file, and an installed one is none
// covers: platform.heads.rules
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn a_look_naming_a_missing_frit_counts_against_the_file() {
    let text = "version = 1\n[look big]\nfont-name = desk\n[head a]\nlook = big\n[rules]\ndisplay * -> a\n";
    let (_dir, file, trust) = heads_fixture("frit", text, 0o644);
    let f = file.display().to_string();
    let frits = file.parent().unwrap().join("frits");
    std::fs::create_dir_all(&frits).unwrap();
    let settings = Settings {
        frits: &frits,
        trust: font::trust().below(file.parent().unwrap()),
        ..caps(4)
    };
    let missing = conf_file(&file, false, &trust, &machine(true), &settings);
    std::fs::write(frits.join("desk.vtfrit"), "source=builtin:unifont\n").unwrap();
    let installed = conf_file(&file, false, &trust, &machine(true), &settings);
    let dir = frits.display();
    assert_eq!(
        missing.counted,
        [format!(
            "{f}:3: Look \"big\" uses the font set \"desk\", but \"{dir}\" has no \"desk\".vtfrit. vtium then looks for a kbd font named \"desk\", and uses its built-in font if there is none. Put the font set in \"{dir}\", or change the look's `font-name`."
        )]
    );
    assert_eq!(installed.counted, [] as [String; 0]);
}

// ADR-1401 D10, ADR-1403 D5: a look's frit the ownership check refuses
// counts against the file on its `font-name` line, as the font-name
// frit's refusal counts, with the path that fails and the command that
// mends it; a frit the account reads but a start as root would refuse
// is noted on that line
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn a_looks_frit_the_ownership_check_refuses_is_reported() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let text = "version = 1\n[look big]\nfont-name = g\n[look small]\nfont-name = t\n[head a]\nlook = big\n[head b]\nlook = small\n[rules]\ndisplay * -> a\n";
    let (_dir, file, trust) = heads_fixture("look-trust", text, 0o644);
    let f = file.display().to_string();
    let frits = file.parent().unwrap().join("frits");
    std::fs::create_dir_all(&frits).unwrap();
    std::fs::set_permissions(&frits, std::fs::Permissions::from_mode(0o755)).unwrap();
    for (name, mode) in [("g", 0o664), ("t", 0o644)] {
        let path = frits.join(format!("{name}.vtfrit"));
        std::fs::write(&path, "source=builtin:unifont\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
    }
    let uid = std::fs::metadata(&frits).unwrap().uid();
    let settings = Settings {
        frits: &frits,
        trust: font::trust().below(file.parent().unwrap()),
        ..caps(4)
    };
    let out = conf_file(&file, false, &trust, &machine(true), &settings);
    let (g, t) = (frits.join("g.vtfrit"), frits.join("t.vtfrit"));
    let refused: Vec<&String> = out
        .counted
        .iter()
        .filter(|l| l.contains("vtfrit"))
        .collect();
    let (g, t) = (g.display(), t.display());
    assert_eq!(
        refused,
        [&format!(
            "{f}:3: Look \"big\": vtium does not read its font set g from \"{g}\", because other users can change \"{g}\" (mode 664). Run `chmod go-w {g}` so that only its owner can change it. vtium draws that look in its built-in font."
        )]
    );
    let noted: Vec<&String> = out.notes.iter().filter(|l| l.contains("vtfrit")).collect();
    assert_eq!(
        noted,
        [&format!(
            "{f}:5: Look \"small\": when vtium runs as root, it does not read its font set t from \"{t}\", because \"{t}\" belongs to user {uid}, not to root. Run `chown root:root {t}` to make root the owner. vtium would then draw that look in its built-in font."
        )]
    );
}

// ADR-1401 D7: the two drops the rest of the tests leave unsaid: an
// unknown version drops the whole file but its policy, and a `policy`
// line inside a section drops that line alone
// covers: platform.heads.rules
#[test]
fn a_whole_file_error_and_a_line_error_say_what_they_drop() {
    let (out, f) = checked("whole", "version = 2\n[head a]\n", true);
    assert_eq!(
        out.counted,
        [
            format!(
                "{f}:1: vtium does not know version \"2\"; it reads only `version = 1`. Change the version line to `version = 1`."
            ),
            format!(
                "{f}:1: Because of this error, vtium ignores the whole file except its `policy` line."
            ),
        ]
    );
    let (out, f) = checked(
        "line",
        "version = 1\n[head a]\npolicy = closed\n[rules]\ndisplay * -> a\n",
        true,
    );
    assert_eq!(
        out.counted,
        [
            format!(
                "{f}:3: `policy` must be at the top of vtium.conf, above the first section, so vtium ignores this line. Move it there, beside `version`."
            ),
            format!("{f}:3: Because of this error, vtium ignores this line."),
        ]
    );
}

// ADR-1401 D7: a `policy` line inside a section states nothing, so the
// top-level line's policy stands
// covers: platform.heads.policy
#[test]
fn a_policy_line_inside_a_section_leaves_the_top_levels_standing() {
    let (out, f) = checked(
        "in-section",
        "version = 1\npolicy = open\n[head a]\npolicy = closed\n[rules]\ndisplay * -> a\n",
        true,
    );
    assert_eq!(verdict(&out, &f), [PARTLY_OPEN, PARTLY_OPEN_POLICY]);
}

// ADR-1401 D7: under a partly broken open file `isolated` is ignored,
// and the report says so where a head writes `isolated = yes`
// covers: platform.heads.isolated
#[test]
fn a_partly_broken_open_file_says_isolated_is_ignored() {
    let (out, f) = checked(
        "isolated",
        "version = 1\n[head a]\nisolated = yes\n[rules]\ndisplay * -> a\ndisplay * -> b\n",
        true,
    );
    assert_eq!(verdict(&out, &f), [PARTLY_OPEN, PARTLY_UNSET_POLICY]);
}

// ADR-1401 D3: a head no rule names is valid, and noted
// covers: platform.heads.rules
#[test]
fn a_head_no_rule_names_is_a_note() {
    let (out, f) = checked(
        "unnamed",
        "version = 1\n[head a]\n[head b]\n[rules]\ndisplay * -> a\n",
        true,
    );
    assert_eq!(out.counted, [] as [String; 0]);
    assert_eq!(
        out.notes,
        [
            format!(
                "{f}:3: Head \"b\" is not used: no rule sends a display or input device to it. Add a rule that sends a display to it, or remove the head."
            ),
            format!("{f}: {RULES_ALONE}"),
            format!("{f}: {AS_WRITTEN}"),
            format!("{f}: {UNSET}"),
        ]
    );
}

/// `text` checked against the devices `devices`, its notes but the
/// outcome and the policy, without the path before them: a note on a
/// line starts with the line.
fn remarked(name: &str, text: &str, devices: &[Identity]) -> (Vec<String>, Vec<String>) {
    let (_dir, file, trust) = heads_fixture(name, text, 0o644);
    let f = format!("{}:", file.display());
    let m = Machine {
        udev_daemon: true,
        devices: Some(devices),
        vts: VtsEnabled::FileAlone,
    };
    let out = conf_file(&file, false, &trust, &m, &caps(4));
    let bare = |n: &String| {
        let n = n.strip_prefix(&f).unwrap_or(n);
        n.strip_prefix(' ').unwrap_or(n).to_owned()
    };
    let notes = out.notes.len().saturating_sub(2);
    let notes = out
        .notes
        .get(..notes)
        .unwrap_or_default()
        .iter()
        .map(bare)
        .collect();
    (out.counted.iter().map(bare).collect(), notes)
}

/// What a check against the devices present counts of a file whose
/// display rules send no display present to a head (ADR-1403 D3).
const NO_DISPLAY_SENT: &str = "No connected display reaches a head: no rule matches display \"display 1\", so it stays off. Add a display rule that sends it to a head.";

/// What it counts of a file whose only head is isolated, with no
/// input rule that sends a keyboard to it.
const ISOLATED: &str = "Head \"a\" is isolated, so roaming keyboards do not reach it. Add an input rule that sends a keyboard to \"a\", or write `isolated = no` in its section.";

/// A display, a keyboard and a joystick, each named.
fn present() -> Vec<Identity> {
    use vtium_settings::Class;
    let named = |mut id: Identity, name: &str| {
        id.name = Some(name.to_owned());
        id
    };
    vec![
        named(Identity::display(), "DP-1"),
        named(Identity::input().with_class(Class::Keyboard), "Keyboard A"),
        named(Identity::input().with_class(Class::Joystick), "Pad"),
    ]
}

// ADR-1401 D9, "What fell to the default": each device present no rule
// places is noted with what the policy gives it
// covers: platform.heads.policy
#[test]
fn what_falls_to_the_open_policy_is_noted() {
    let (counted, notes) = remarked(
        "fell-open",
        "version = 1\n[head a]\n[rules]\ndisplay * -> a\ninput class:joystick -> a\n",
        &present(),
    );
    assert_eq!(counted, [] as [String; 0]);
    assert_eq!(
        notes,
        [
            "No rule matches input device \"Keyboard A\", so with the open policy it types into the active head. Add an `input` rule to send it somewhere else.",
        ]
    );
}

// ADR-1401 D5, D9: under closed no head is given a display the file does
// not give it, so a head with no display rule leaves no head that can be
// the active head, and the display stays dark
// covers: platform.heads.focus
#[test]
fn no_head_that_can_be_active_is_noted() {
    let (counted, notes) = remarked(
        "no-active",
        "version = 1\npolicy = closed\n[head a]\nisolated = no\n[rules]\n\
         input class:keyboard -> a\ninput * -> ignore\n",
        &present(),
    );
    assert_eq!(counted, [NO_DISPLAY_SENT]);
    assert_eq!(
        notes,
        [
            "head a has only one keyboard, \"Keyboard A\"",
            "No head that can be active has a connected display, so a roaming keyboard reaches no head until one turns on. Connect a display for a head that is not isolated.",
            "No rule matches display \"DP-1\", so it stays off. Add a `display` rule for it if a head should use it.",
        ]
    );
}

// ADR-1401 D9: under closed, a keyboard no rule places is ignored, and
// the lockout that leaves is the first thing said
// covers: platform.heads.policy
#[test]
fn a_closed_lockout_is_noted_first() {
    let (counted, notes) = remarked(
        "fell-closed",
        "version = 1\npolicy = closed\n[head a]\nisolated = no\n[rules]\ndisplay * -> a\n",
        &present(),
    );
    assert_eq!(
        counted,
        [
            "No keyboard reaches head \"a\", which shows on \"display 1\". Add an input rule that sends a keyboard to \"a\" or to `roaming`."
        ]
    );
    assert_eq!(
        notes,
        [
            "No keyboard reaches any head, so with the closed policy nobody can type at this machine. Before you install the file, send a keyboard to a head, or to `roaming` with a head that is not isolated.",
            "every keyboard is ignored",
            "No rule matches input device \"Keyboard A\", so with the closed policy vtium ignores it. Add an `input` rule for it if it should reach a head.",
            "No rule matches input device \"Pad\", so with the closed policy vtium ignores it. Add an `input` rule for it if it should reach a head.",
        ]
    );
}

// ADR-1401 D5, D9: a closed file in which every head is isolated draws
// the note, and a keyboard a head claims is no lockout
// covers: platform.heads.isolated
#[test]
fn every_head_isolated_is_noted() {
    let (counted, notes) = remarked(
        "isolated-all",
        "version = 1\npolicy = closed\n[head a]\n[rules]\ndisplay * -> a\n\
         input class:keyboard -> a\ninput * -> ignore\n",
        &present(),
    );
    assert_eq!(counted, [] as [String; 0]);
    assert_eq!(
        notes,
        [
            "head a has only one keyboard, \"Keyboard A\"",
            "Every head is isolated, so no keyboard can switch the VT and no head can be the active head. A head gets only the keyboards its own rules send to it. Write `isolated = no` in a head's section if a keyboard should roam to it."
        ]
    );
}

// ADR-1401 D7: the device notes of a partly broken open file walk the
// shipped rules' tail a start runs after the surviving rules
// covers: platform.heads.rules
#[test]
fn a_partly_broken_open_files_devices_walk_the_shipped_tail() {
    let (counted, notes) = remarked(
        "fell-tail",
        "version = 1\n[head a]\n[rules]\ndisplay * -> a\ndisplay * -> b\n",
        &present(),
    );
    assert_eq!(counted.len(), 2, "{counted:?}");
    assert_eq!(
        notes,
        [
            "No working rule matches input device \"Keyboard A\", so it goes on to the built-in rules and types into the active head. Fix the file's errors, or add an `input` rule to send it somewhere else.",
            "No working rule matches input device \"Pad\", so it goes on to the built-in rules, which ignore it. Fix the file's errors, or add an `input` rule for it if it should reach a head.",
        ]
    );
}

// ADR-1401 D3, D9: for a valid file the table decides, with or without
// a VT number, so the first head no rule names gets no display and the
// display no rule matches stays off, with the devices present or away
// from them. Only what survives of an open file with errors runs the
// display guard, which gives the first head the boot display while no
// head has one.
// covers: platform.heads.guards
#[test]
fn the_first_head_no_rule_names_is_lit_by_the_display_guard_only_for_a_file_with_errors() {
    let file = "version = 1\n[head a]\n[rules]\ninput * -> roaming\n";
    let unused = "Head \"a\" is not used: no rule sends a display or input device to it. Add a rule that sends a display to it, or remove the head.";
    let (counted, notes) = remarked("unnamed", file, &present());
    assert_eq!(counted, [NO_DISPLAY_SENT]);
    assert_eq!(
        notes,
        [
            "No head that can be active has a connected display, so a roaming keyboard reaches no head until one turns on. Connect a display for a head that is not isolated.".to_owned(),
            "No rule matches display \"DP-1\", so it stays off. Add a `display` rule for it if a head should use it.".to_owned(),
            format!("2: {unused}"),
        ]
    );
    let lit = "No rule names head \"a\". Because the file has errors, vtium gives it the boot display under the open policy when no other head has a display. Add a rule that sends a display to it, or remove the head.";
    let broken = format!("{file}input path:usb-0:9 -> nowhere\n");
    let (_, notes) = remarked("unnamed-guard", &broken, &present());
    assert!(notes.contains(&format!("2: {lit}")), "{notes:?}");
    let (out, f) = checked("unnamed-away", file, true);
    assert_eq!(
        out.notes,
        [
            format!("{f}:2: {unused}"),
            format!("{f}: {RULES_ALONE}"),
            format!("{f}: {AS_WRITTEN}"),
            format!("{f}: {UNSET}")
        ]
    );
}

/// A keyboard and a joystick, each named, and no display.
fn dark() -> Vec<Identity> {
    present().into_iter().skip(1).collect()
}

// ADR-1401 D5, D9: an input that roams while no head can be the active
// head reaches no head, under the file's rules and under the shipped
// rules' tail alike
// covers: platform.heads.focus
#[test]
fn an_input_that_roams_with_no_active_head_is_noted_as_reaching_none() {
    let (counted, notes) = remarked(
        "roams-nowhere",
        "version = 1\n[head a]\nisolated = yes\n[head b]\n[rules]\ndisplay * -> a\n\
         input class:joystick -> a\n",
        &present(),
    );
    assert_eq!(counted, [ISOLATED]);
    assert_eq!(
        notes,
        [
            "No head that can be active has a connected display, so a roaming keyboard reaches no head until one turns on. Connect a display for a head that is not isolated.",
            "Input device \"Keyboard A\" reaches no head: no rule matches it, so with the open policy it roams, but no head that can be active has a connected display. Add an `input` rule that sends it to a head.",
            "4: Head \"b\" is not used: no rule sends a display or input device to it. Add a rule that sends a display to it, or remove the head.",
        ]
    );
    let (counted, notes) = remarked(
        "roams-nowhere-tail",
        "version = 1\n[head a]\n[rules]\ndisplay * -> a\ndisplay * -> b\n",
        &dark(),
    );
    assert_eq!(counted.len(), 2, "{counted:?}");
    assert!(
        notes.contains(&"Input device \"Keyboard A\" reaches no head: no working rule matches it, so it goes on to the built-in rules and roams, but no head that can be active has a connected display. Fix the file's errors, or add an `input` rule that sends it to a head.".to_owned()),
        "{notes:?}"
    );
}

// ADR-1401 D5, D9: under closed a keyboard sent to `roaming` while no
// head can be the active head reaches none, which is a lockout
// covers: platform.heads.policy
#[test]
fn a_closed_keyboard_roaming_with_no_active_head_is_a_lockout() {
    let (counted, notes) = remarked(
        "roams-closed",
        "version = 1\npolicy = closed\n[head a]\n[rules]\ndisplay * -> a\ninput * -> roaming\n",
        &present(),
    );
    assert_eq!(counted, [ISOLATED]);
    assert_eq!(
        notes.first().map(String::as_str),
        Some(
            "No keyboard reaches any head, so with the closed policy nobody can type at this machine. Before you install the file, send a keyboard to a head, or to `roaming` with a head that is not isolated."
        ),
        "{notes:?}"
    );
}

// ADR-1401 D7: a start that runs the shipped default runs none of the
// file, so a broken file checked on its machine draws no remark and no
// device note
#[test]
fn a_file_the_start_drops_draws_no_device_notes() {
    for (name, file) in [
        (
            "dropped-version",
            "version = 2\n[head a]\n[rules]\ninput * -> a\n",
        ),
        (
            "dropped-heads",
            "version = 1\n[head kiosk]\nconsoles = 0\n[rules]\ninput * -> kiosk\n",
        ),
    ] {
        let (counted, notes) = remarked(name, file, &present());
        assert!(!counted.is_empty(), "{name}");
        assert_eq!(notes, [] as [String; 0], "{name}");
    }
}

// The head file has no `[guards]` and no `rescue`, so a file in the
// first cut's names counts against itself, each error naming the word
// that replaced the old one. The old `rescue` key drops only its own
// line and head kiosk keeps running, while the unknown `[guards]`
// section drops whole, so the start runs the parts without errors.
//
// References:
// - ADR-1401 D3, D9: the names that replaced `[guards]` and `rescue`.
// - ADR-1510 D2: an unknown or renamed key drops only its line.
#[test]
fn the_first_cuts_guards_and_rescue_count_against_the_file() {
    let (out, f) = checked(
        "guards",
        "version = 1\n[head kiosk]\nrescue = off\n[rules]\ndisplay * -> kiosk\n\
         [guards]\nanti-lockout = off\n",
        true,
    );
    let counted: Vec<&str> = out.counted.iter().map(String::as_str).collect();
    let at = |line: usize, word: &str| {
        counted
            .iter()
            .any(|c| c.starts_with(&format!("{f}:{line}: ")) && c.contains(word))
    };
    assert!(at(3, "`isolated = yes`"), "{counted:?}");
    assert!(at(3, "vtium ignores this line."), "{counted:?}");
    assert!(at(6, "`policy`"), "{counted:?}");
    assert_eq!(
        out.notes,
        [RULES_ALONE, PARTLY_OPEN, PARTLY_UNSET_POLICY].map(|n| format!("{f}: {n}"))
    );
}

// ADR-1401 D4, probe 4 (the recommendation, WEAK_MATCH_COUNTS): input
// sent to a head by `id` or `name` alone is a note
#[test]
fn input_reaching_a_head_by_id_or_name_alone_is_a_note() {
    let (out, f) = checked(
        "weak",
        "version = 1\n[head desk]\n[rules]\ndisplay * -> desk\n\
         input id:046d:c52b -> desk\ninput name:Foo* -> desk\n\
         input id:046d:c52c path:pci-0000:00:14.0-usb-0:1 -> desk\n",
        true,
    );
    assert_eq!(out.counted, [] as [String; 0]);
    let weak = |line: usize| {
        format!(
            "{f}:{line}: weak match: this rule matches by `id` or `name` alone, which any device can claim"
        )
    };
    let shown = |what: &str| format!("{f}: {what}");
    assert_eq!(
        out.notes,
        [
            weak(5),
            weak(6),
            shown(RULES_ALONE),
            shown(AS_WRITTEN),
            shown(UNSET)
        ]
    );
}

// ADR-1401 D4: with no udev daemon, each rule using a key that reads
// udev's database is noted, naming the key
#[test]
fn without_a_udev_daemon_each_rule_needing_one_is_noted_with_its_key() {
    let text = "version = 1\n[head desk]\n[rules]\n\
                display card:pci-0000:03:00.0 -> desk\n\
                display connector:pci-0000:03:00.0/DP-* -> desk\n\
                display edid:AAA:0x0001:LARGE00001 -> desk\n\
                input path:pci-0000:15:00.0-usb-0:3.3 serial:aaaa:0001:X1 -> desk\n\
                input udev:VTIUM_HEAD=desk -> desk\n\
                input class:keyboard -> roaming\n";
    let (out, f) = checked("udev", text, false);
    assert_eq!(out.counted, [] as [String; 0]);
    let no = |line: usize, key: &str| {
        format!("{f}:{line}: `{key}:` matches nothing, because udev is not running")
    };
    let udev: Vec<&String> = out
        .notes
        .iter()
        .filter(|n| n.contains("udev is not running"))
        .collect();
    assert_eq!(
        udev,
        [
            &no(4, "card"),
            &no(5, "connector"),
            &no(7, "path"),
            &no(7, "serial"),
            &no(8, "udev")
        ]
    );
    let (out, _) = checked("udevd", text, true);
    assert!(
        !out.notes.iter().any(|n| n.contains("udev daemon")),
        "{:?}",
        out.notes
    );
}

// ADR-1401 D4, vtium/vtium#157: check-config opens no card, so no
// listed display carries a KMS connector's PATH; each `connector-path:`
// rule is noted as unchecked, with or without a udev daemon
#[test]
fn each_connector_path_rule_is_noted_as_unchecked_against_the_displays_present() {
    let text = "version = 1\n[head desk]\n[head hub]\n[rules]\n\
                display connector-path:mst:63-1 -> hub\n\
                display edid:AAA:0x0001:LARGE00001 -> desk\n\
                display connector-path:mst:63-2 edid:BBB:0x0002:X -> hub\n\
                display * -> desk\n";
    for udev in [true, false] {
        let (out, f) = checked(&format!("mst-{udev}"), text, udev);
        assert_eq!(out.counted, [] as [String; 0]);
        let mst: Vec<&String> = out
            .notes
            .iter()
            .filter(|n| n.contains("connector-path"))
            .collect();
        let none = |line: usize| {
            format!(
                "{f}:{line}: `connector-path:` is not checked against the connected \
                 displays, because check-config does not open the graphics card; vtium \
                 matches it when it finds the display"
            )
        };
        assert_eq!(mst, [&none(5), &none(7)], "udev {udev}");
    }
}

// ADR-1401 D6, D7: the installed head file absent is the shipped
// rules' case, a candidate absent is an error
#[test]
fn an_absent_head_file_is_a_note_and_an_absent_candidate_an_error() {
    let path = std::env::temp_dir().join(format!("vtium-check-heads-none-{}", std::process::id()));
    let trust = Trust::ROOT;
    let shown = path.display();
    assert_eq!(
        conf_file(&path, false, &trust, &machine(true), &caps(4)),
        HeadCheck {
            counted: Vec::new(),
            notes: vec![format!(
                "{shown}: There is no vtium.conf here, so vtium starts with its built-in configuration and shows no notice."
            )],
            refused: false,
            missing_fonts: 0,
            vtlooks: Vec::new(),
            palettes: Vec::new(),
            pointers: Vec::new(),
        }
    );
    assert_eq!(
        conf_file(&path, true, &trust, &machine(true), &caps(4)),
        HeadCheck {
            counted: vec![format!("{shown}: does not exist")],
            notes: Vec::new(),
            refused: false,
            missing_fonts: 0,
            vtlooks: Vec::new(),
            palettes: Vec::new(),
            pointers: Vec::new(),
        }
    );
}

// ADR-1401 D10: the installed file failing the ownership check is
// invalid; a candidate failing it where it sits is noted and checked
// (the recommendation, CANDIDATE_OWNERSHIP_COUNTS)
#[test]
fn ownership_counts_for_the_installed_file_and_is_a_note_for_a_candidate() {
    let text = "version = 1\n[head desk]\n[rules]\ndisplay * -> desk\ninput * -> desc\n";
    let (_dir, file, trust) = heads_fixture("owner", text, 0o666);
    let f = file.display().to_string();
    // ADR-1403 D3: the refusal names the path, its mode and the
    // command that mends it
    let untrusted = format!(
        "{f}: vtium ignores this file and uses its built-in configuration, because other users can change \"{f}\" (mode 666). Run `chmod go-w {f}` so that only its owner can change it."
    );
    let installed = conf_file(&file, false, &trust, &machine(true), &caps(4));
    assert_eq!(installed.counted, [untrusted]);
    assert_eq!(
        installed.notes,
        [format!(
            "{f}: vtium does not read a file it does not trust, so it starts with its built-in configuration and shows `vtium.conf not used: running the built-in configuration.`."
        )]
    );
    let candidate = conf_file(&file, true, &trust, &machine(true), &caps(4));
    assert_eq!(
        candidate.notes,
        [
            format!(
                "{f}: Other users can change \"{f}\" (mode 666). That is fine for a check: vtium reads the file once it is installed, owned by root and writable only by root."
            ),
            format!("{f}: {RULES_ALONE}"),
            format!("{f}: {PARTLY_OPEN}"),
            format!("{f}: {PARTLY_UNSET_POLICY}"),
        ]
    );
    assert_eq!(
        candidate.counted,
        [
            format!(
                "{f}:5: This rule sends devices to head \"desc\", but the file has no head of that name. Correct the name, or add a `[head NAME]` section for it."
            ),
            format!(
                "{f}:5: Because of this error, vtium ignores this rule, and each device goes on to the next rule."
            ),
        ]
    );
}

// ADR-1401 D7, D9: what depends on the devices present names each
// rule no device matches and each layer 1 warning, with the device as
// it reports itself, escaped
#[test]
fn device_findings_name_idle_rules_and_lockouts_with_the_device_escaped() {
    use vtium_settings::{Class, parse};
    let text = "version = 1\n[head desk]\n[head panel]\n[rules]\n\
                display * -> desk\n\
                input name:Keyb* -> desk\n\
                input class:mouse -> panel\n\
                input name:Gone -> panel\n";
    let config = parse(text).map(|p| p.config).unwrap_or_default();
    let mut keyboard = Identity::input().with_class(Class::Keyboard);
    keyboard.name = Some("Keyb\u{1b}[2J".to_owned());
    let mouse = Identity::input().with_class(Class::Mouse);
    let path = Path::new("/etc/vtium/heads.conf");
    assert_eq!(
        device_findings(path, &config, &[Identity::display(), keyboard, mouse], &[]),
        [
            "/etc/vtium/heads.conf:8: the rule matches no connected device",
            "/etc/vtium/heads.conf: head desk has only one keyboard, \"Keyb\\u{1b}[2J\"",
            "/etc/vtium/heads.conf: head panel has mice but no keyboard: every keyboard goes to another head",
        ]
    );
}

// ADR-1401 D7: the word after the subcommand is the candidate, and a
// second is no option
#[test]
fn the_word_after_the_subcommand_is_the_candidate() {
    let load = |args: &[&str]| {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        config::load(&args, &mut |_: &Path| Ok(None))
    };
    let l = load(&[SUBCOMMAND, "/tmp/heads.conf"]);
    assert_eq!(l.candidate.as_deref(), Some(Path::new("/tmp/heads.conf")));
    assert!(l.reports.is_empty(), "{:?}", l.reports);
    let l = load(&["-c", "/etc/vtium", SUBCOMMAND, "a.conf", "b.conf"]);
    assert_eq!(l.candidate.as_deref(), Some(Path::new("a.conf")));
    assert_eq!(l.reports.len(), 1, "{:?}", l.reports);
    let l = load(&["a.conf", SUBCOMMAND]);
    assert_eq!(l.candidate, None);
    assert_eq!(l.reports.len(), 1, "{:?}", l.reports);
    let l = load(&[SUBCOMMAND]);
    assert_eq!((l.candidate, l.check), (None, true));
}

// a head file root keeps from other users, such as root's 0600 file in
// a 0700 /etc/vtium: only root can check it, and the start, root,
// reads it
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn a_head_file_the_user_may_not_read_can_be_checked_only_as_root() {
    use std::os::unix::fs::PermissionsExt;
    if rustix::process::geteuid().is_root() {
        eprintln!(
            "running as root, which can read every file, so the readability check is skipped"
        );
        return;
    }
    let mode =
        |p: &Path, m: u32| std::fs::set_permissions(p, std::fs::Permissions::from_mode(m)).unwrap();
    let (dir, file, trust) = heads_fixture("denied", "version = 1\n", 0o000);
    let denied = HeadCheck {
        counted: vec![format!(
            "{}: permission denied: only root can check this file",
            file.display()
        )],
        notes: Vec::new(),
        refused: false,
        missing_fonts: 0,
        vtlooks: Vec::new(),
        palettes: Vec::new(),
        pointers: Vec::new(),
    };
    assert_eq!(
        conf_file(&file, false, &trust, &machine(true), &caps(4)),
        denied,
        "the file"
    );
    assert_eq!(
        conf_file(&file, true, &trust, &machine(true), &caps(4)),
        denied,
        "a candidate"
    );
    mode(&file, 0o644);
    mode(&dir, 0o000);
    assert_eq!(
        conf_file(&file, false, &trust, &machine(true), &caps(4)),
        denied,
        "its directory"
    );
    mode(&dir, 0o700);
}

#[test]
fn a_udev_daemon_runs_where_its_control_socket_is_present() {
    let (_dir, file, _) = heads_fixture("udev-control", "", 0o644);
    assert!(udev_daemon_runs(&file));
    assert!(!udev_daemon_runs(&file.with_file_name("absent")));
}

/// The subcommand on the command line `args` and a host of fixtures.
fn outcome_on(args: &[&str], host: &Host) -> Outcome {
    let args: Vec<OsString> = [SUBCOMMAND]
        .iter()
        .chain(args)
        .map(OsString::from)
        .collect();
    outcome(&config::load(&args, &mut |_: &Path| Ok(None)), host)
}

use std::collections::BTreeSet;

use crate::console::{Enabled, Mode};

/// A laptop's file: a head with no `vt` and a head on `vts`.
fn two_heads_on(vts: &str) -> String {
    format!(
        "version = 1\n[head console]\n[head terminal-1]\nvt = {vts}\n[rules]\ndisplay card:card1 -> terminal-1\ndisplay * -> console\n"
    )
}

/// `text`'s notes against a machine whose VTs are `vts`.
fn vt_notes(name: &str, text: &str, vts: VtsEnabled) -> (HeadCheck, String) {
    let (_dir, file, trust) = heads_fixture(name, text, 0o644);
    let machine = Machine {
        udev_daemon: true,
        devices: None,
        vts,
    };
    let out = conf_file(&file, false, &trust, &machine, &caps(4));
    (out, file.display().to_string())
}

// ADR-1401 D3, "Enter on a head's own keyboard": a head whose VTs have
// no vtium enabled is noted at its line, with the command that enables
// one, outside the exit status; a head with one such VT, or with no
// `vt`, is not
// covers: platform.heads.rules
#[test]
fn a_head_on_vts_where_vtium_is_not_enabled_is_noted_with_the_command() {
    let boot = Enabled {
        vts: BTreeSet::from([2, 3, 4, 5, 6]),
        mode: Mode::Direct,
    };
    let (out, f) = vt_notes("vt-off", &two_heads_on("12"), VtsEnabled::On(&boot));
    assert_eq!(out.counted, Vec::<String>::new());
    assert!(
        out.notes.contains(&format!(
            "{f}:3: Head terminal-1 runs only on VT 12, and vtium is not enabled on VT 12, so nothing shows this head. To enable vtium there, run sudo vtium console enable --direct 12."
        )),
        "{:#?}",
        out.notes
    );
    let seat = Enabled {
        vts: BTreeSet::from([2]),
        mode: Mode::Seat,
    };
    let (out, f) = vt_notes("vt-two", &two_heads_on("7 12"), VtsEnabled::On(&seat));
    assert!(
        out.notes.contains(&format!(
            "{f}:3: Head terminal-1 runs only on VTs 7 and 12, and vtium is enabled on none of them, so nothing shows this head. To enable vtium on VT 7, run sudo vtium console enable --seat 7."
        )),
        "{:#?}",
        out.notes
    );
    let with_12 = Enabled {
        vts: BTreeSet::from([2, 12]),
        mode: Mode::Direct,
    };
    let (out, _) = vt_notes("vt-on", &two_heads_on("7 12"), VtsEnabled::On(&with_12));
    assert!(
        !out.notes
            .iter()
            .any(|n| n.contains("sudo vtium console enable")),
        "{:#?}",
        out.notes
    );
}

// where the VTs vtium is enabled on were not read, a file whose heads
// name VTs says so once, and a file whose heads name none says nothing
// covers: platform.heads.rules
#[test]
fn where_the_enabled_vts_were_not_read_the_check_says_so() {
    for (vts, line) in [
        (
            VtsEnabled::FileAlone,
            check_config::VTS_UNCHECKED_FILE_ALONE,
        ),
        (
            VtsEnabled::NoSystemd,
            check_config::VTS_UNCHECKED_NO_SYSTEMD,
        ),
    ] {
        let (out, f) = vt_notes("vt-unread", &two_heads_on("12"), vts);
        assert!(
            out.notes.contains(&format!("{f}: {line}")),
            "{:#?}",
            out.notes
        );
        let text = "version = 1\n[head console]\n[rules]\ndisplay * -> console\n";
        let (out, _) = vt_notes("vt-none", text, vts);
        assert!(
            !out.notes.iter().any(|n| n.contains(line)),
            "{:#?}",
            out.notes
        );
    }
}

// the subcommand reads the machine's systemd links unless --no-devices
// asks it to check the file alone, and the note leaves the status 0
// covers: platform.heads.rules
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn check_config_reads_the_systemd_links_of_its_host() {
    let (_dir, file, _) = heads_fixture("vt-host", &two_heads_on("12"), 0o644);
    let f = file.display().to_string();
    let root = file.with_file_name("root");
    std::fs::create_dir_all(root.join("run/systemd/system")).unwrap();
    let wants = root.join("etc/systemd/system/getty.target.wants");
    std::fs::create_dir_all(&wants).unwrap();
    let host = Host {
        conf_file: file.with_file_name("installed.conf"),
        udev_control: file.clone(),
        roots: nowhere(),
        frits: PathBuf::from("/nonexistent/vtium/frits"),
        frit_trust: font::trust(),
        systemd_root: root.clone(),
        batch: PathBuf::from("/nonexistent/vtium/batch.conf"),
        drawer: None,
    };
    let off = "Head terminal-1 runs only on VT 12, and vtium is not enabled on VT 12";
    let out = outcome_on(&[f.as_str()], &host);
    assert_eq!(out.status, 0, "{:#?}", out.stderr);
    assert!(
        out.stdout.iter().any(|n| n.contains(off)),
        "{:#?}",
        out.stdout
    );
    let out = outcome_on(&["--no-devices", f.as_str()], &host);
    assert!(
        out.stdout
            .contains(&format!("{f}: {}", check_config::VTS_UNCHECKED_FILE_ALONE)),
        "{:#?}",
        out.stdout
    );
    std::os::unix::fs::symlink(
        "/usr/lib/systemd/system/vtium@.service",
        wants.join("vtium@tty12.service"),
    )
    .unwrap();
    let out = outcome_on(&[f.as_str()], &host);
    assert!(
        !out.stdout.iter().any(|n| n.contains(off)),
        "{:#?}",
        out.stdout
    );
}

// ADR-1403 D8: the check counts a frit the draft adds as installed and
// one it removes as missing, so a look that names a font set only the
// draft holds passes, and a look that names one the draft removes gets
// the warning on a missing font set
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn the_drafts_font_sets_count_in_place_of_the_installed_ones() {
    let text = "version = 1\n[look big]\nfont-name = desk\n[look small]\nfont-name = old\n[head a]\nlook = big\n[head b]\nlook = small\n[rules]\ndisplay * -> a\n";
    let (dir, file, _) = heads_fixture("draft-frits", text, 0o644);
    let f = file.display().to_string();
    let frits = dir.join("frits");
    let folder = dir.join("mine.frits");
    std::fs::create_dir_all(&frits).unwrap();
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(frits.join("old.vtfrit"), "source=builtin:unifont\n").unwrap();
    let desk = folder.join("desk.vtfrit");
    std::fs::write(&desk, "source=builtin:unscii-8\n").unwrap();
    let host = Host {
        conf_file: file.with_file_name("installed.conf"),
        udev_control: file.clone(),
        roots: nowhere(),
        frits: frits.clone(),
        frit_trust: font::trust().below(&dir),
        systemd_root: dir.join("root"),
        batch: PathBuf::from("/nonexistent/vtium/batch.conf"),
        drawer: None,
    };
    let missing = |name: &str, line: usize| {
        format!(
            "{f}:{line}: Look \"{}\" uses the font set \"{name}\", but \"{}\" has no \"{name}\".vtfrit. vtium then looks for a kbd font named \"{name}\", and uses its built-in font if there is none. Put the font set in \"{}\", or change the look's `font-name`.",
            if name == "desk" { "big" } else { "small" },
            frits.display(),
            frits.display()
        )
    };
    let frit_lines = |out: &Outcome| -> Vec<String> {
        out.stderr
            .iter()
            .filter(|l| l.contains(".vtfrit"))
            .cloned()
            .collect()
    };
    let out = outcome_on(&["--no-devices", f.as_str()], &host);
    assert_eq!(frit_lines(&out), [missing("desk", 3)]);
    let with = desk.display().to_string();
    let out = outcome_on(&["--no-devices", "--with-frit", &with, f.as_str()], &host);
    assert_eq!(frit_lines(&out), [] as [String; 0]);
    assert_eq!(out.status, 0, "{:#?}", out.stderr);
    let out = outcome_on(
        &[
            "--no-devices",
            &format!("--with-frit={with}"),
            "--without-frit",
            "old",
            f.as_str(),
        ],
        &host,
    );
    assert_eq!(frit_lines(&out), [missing("old", 5)]);
    // a later option for the same font set takes the place of the
    // earlier one
    let out = outcome_on(
        &[
            "--no-devices",
            "--without-frit=desk",
            "--with-frit",
            &with,
            f.as_str(),
        ],
        &host,
    );
    assert_eq!(frit_lines(&out), [] as [String; 0]);
}

// ADR-1403 D8: the two options belong to the subcommand, `--with-frit`
// names a font set file, `NAME.vtfrit`, and each needs a value
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn the_font_set_options_belong_to_check_config_and_name_a_font_set_file() {
    let dir = Scratch::new("with-frit-words");
    let good = dir.join("desk.vtfrit");
    std::fs::write(&good, "source=builtin:unifont\n").unwrap();
    std::fs::create_dir_all(dir.join("folder.vtfrit")).unwrap();
    let load = |args: &[&str]| {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        config::load(&args, &mut |_: &Path| Ok(None))
    };
    let reported = |l: &config::Loaded| -> Vec<String> {
        l.reports.iter().map(|r| r.problem.to_string()).collect()
    };
    let good = good.display().to_string();
    let l = load(&[SUBCOMMAND, WITH_FRIT, &good, WITHOUT_FRIT, "old"]);
    assert_eq!(reported(&l), [] as [String; 0]);
    assert_eq!(
        l.draft_frits,
        DraftFrits {
            with: vec![("desk".to_owned(), PathBuf::from(&good))],
            without: vec!["old".to_owned()],
        }
    );
    let l = load(&[WITH_FRIT, &good, "--without-frit=old"]);
    assert_eq!(
        reported(&l),
        ["unknown key \"--with-frit\", ignored"],
        "the first of the options is the one reported"
    );
    assert_eq!(l.draft_frits, DraftFrits::default());
    // without the subcommand, vtium reads no file the option names: a
    // plain start reports the unknown option once, and nothing more
    let absent = dir.join("absent.vtfrit").display().to_string();
    let l = load(&[WITH_FRIT, &absent]);
    assert_eq!(reported(&l), ["unknown key \"--with-frit\", ignored"]);
    for bad in [
        dir.join("desk.conf"),
        dir.join("absent.vtfrit"),
        dir.join("folder.vtfrit"),
        dir.join(".hidden.vtfrit"),
    ] {
        let bad = bad.display().to_string();
        let l = load(&[SUBCOMMAND, WITH_FRIT, &bad]);
        assert_eq!(
            reported(&l),
            [format!(
                "--with-frit \"{}\" is not a font set file. It needs a regular file named NAME.vtfrit, where NAME is a font set's name. Because of this error, check-config ignores this option.",
                bad.escape_debug()
            )],
            "{bad}"
        );
        assert_eq!(l.draft_frits, DraftFrits::default(), "{bad}");
    }
    let l = load(&[SUBCOMMAND, WITHOUT_FRIT]);
    assert_eq!(reported(&l), ["--without-frit needs a value, ignored"]);
}

// ADR-1005 D10: a vtium.conf written for an earlier release binds its
// keys by their old names. check-config notes each line on standard
// output, where it does not count, so the file passes, and so does
// vtium-config's install of it.
#[test]
fn an_old_key_binding_name_is_a_note_and_the_check_passes() {
    let (_dir, file, _) = heads_fixture(
        "renamed",
        "grab-paste=<Alt>p\ngrab-reboot=<Ctrl><Alt>Delete\nversion = 1\n[head a]\n[rules]\ndisplay * -> a\ninput path:pci-0000:00:14.0-usb-0:1 -> a\n",
        0o644,
    );
    let f = file.display().to_string();
    let host = Host {
        conf_file: file.with_file_name("installed.conf"),
        udev_control: file.clone(),
        roots: nowhere(),
        frits: PathBuf::from("/nonexistent/vtium/frits"),
        frit_trust: font::trust(),
        systemd_root: PathBuf::from("/nonexistent/vtium/root"),
        batch: PathBuf::from("/nonexistent/vtium/batch.conf"),
        drawer: None,
    };
    let args: Vec<OsString> = [SUBCOMMAND, "--no-devices", f.as_str()]
        .iter()
        .map(OsString::from)
        .collect();
    let out = outcome(&config::load(&args, &mut config::read_bytes), &host);
    assert_eq!((out.status, out.stderr), (0, Vec::<String>::new()));
    for (n, old, new) in [
        (1, "grab-paste", "key-paste"),
        (2, "grab-reboot", "key-reboot"),
    ] {
        let note = format!(
            "{f}:{n}: {old} is now called {new}. vtium reads {old} as {new} for now; rename it"
        );
        assert!(out.stdout.contains(&note), "{note}: {:?}", out.stdout);
    }
}

// the installed head file is the host's, read as the start reads it,
// and the udev daemon's socket is the host's too
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn the_installed_head_file_and_the_udev_socket_are_the_hosts() {
    let (_dir, file, _) = heads_fixture(
        "host",
        "version = 1\n[head a]\n[rules]\ndisplay * -> a\ninput path:pci-0000:00:14.0-usb-0:1 -> a\n",
        0o644,
    );
    let f = file.display().to_string();
    let host = Host {
        conf_file: file.clone(),
        udev_control: file.with_file_name("no-udev"),
        roots: nowhere(),
        frits: PathBuf::from("/nonexistent/vtium/frits"),
        frit_trust: font::trust(),
        systemd_root: PathBuf::from("/nonexistent/vtium/root"),
        batch: PathBuf::from("/nonexistent/vtium/batch.conf"),
        drawer: None,
    };
    let out = outcome_on(&[], &host);
    assert_eq!(out.status, 1);
    assert_eq!(
        out.stderr,
        [format!(
            "{f}: vtium ignores this file and uses its built-in configuration, because \"{f}\" belongs to user {}, not to root. Run `chown root:root {f}` to make root the owner.",
            rustix::process::geteuid().as_raw()
        ),]
    );
    assert!(
        out.stdout.contains(&format!(
            "{f}: vtium does not read a file it does not trust, so it starts with its built-in configuration and shows `vtium.conf not used: running the built-in configuration.`."
        )),
        "{:?}",
        out.stdout
    );
    // a copy is a candidate, which the ownership check notes alone
    let copy = file.with_file_name("candidate.conf");
    std::fs::copy(&file, &copy).unwrap();
    let c = copy.display().to_string();
    let out = outcome_on(&[copy.to_str().unwrap()], &host);
    assert_eq!((out.status, out.stderr), (0, Vec::new()));
    assert!(
        out.stdout.contains(&format!(
            "{c}:5: `path:` matches nothing, because udev is not running"
        )),
        "{:?}",
        out.stdout
    );
    let host = Host {
        udev_control: file.clone(),
        ..host
    };
    let out = outcome_on(&[copy.to_str().unwrap()], &host);
    assert!(
        !out.stdout.iter().any(|n| n.contains("udev daemon")),
        "{:?}",
        out.stdout
    );
    let gone = Host {
        conf_file: file.with_file_name("absent.conf"),
        ..host
    };
    let out = outcome_on(&[], &gone);
    assert_eq!((out.status, out.stderr), (0, Vec::new()));
    assert!(
        out.stdout
            .iter()
            .any(|n| n.ends_with(
                "absent.conf: There is no vtium.conf here, so vtium starts with its built-in configuration and shows no notice."
            ))
    );
}

// ADR-1401 D7: the installed file named on the command line, by its
// own path or another, is checked as the start reads it, so the next
// step a notice names never turns its skipped file into a candidate
// covers: platform.heads.rules
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn the_installed_file_named_as_the_candidate_is_checked_as_installed() {
    let (_dir, file, _) = heads_fixture(
        "same",
        "version = 1\n[head a]\n[rules]\ndisplay * -> a\n",
        0o644,
    );
    let link = file.with_file_name("link.conf");
    std::os::unix::fs::symlink(&file, &link).unwrap();
    let host = Host {
        conf_file: file.clone(),
        udev_control: file.clone(),
        roots: nowhere(),
        frits: PathBuf::from("/nonexistent/vtium/frits"),
        frit_trust: font::trust(),
        systemd_root: PathBuf::from("/nonexistent/vtium/root"),
        batch: PathBuf::from("/nonexistent/vtium/batch.conf"),
        drawer: None,
    };
    for named in [&file, &link] {
        let out = outcome_on(&[named.to_str().unwrap()], &host);
        assert_eq!(out.status, 1, "{named:?}");
        let n = named.display();
        assert_eq!(
            out.stderr,
            [format!(
                "{n}: vtium ignores this file and uses its built-in configuration, because \"{n}\" belongs to user {}, not to root. Run `chown root:root {n}` to make root the owner.",
                rustix::process::geteuid().as_raw()
            )],
            "{named:?}"
        );
    }
}

// Each question to the glyph helper starts or waits on a helper, so
// check-config asks it once for each distinct font set, though
// font-name and two looks name the same set.
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn each_font_set_is_asked_each_question_once() {
    let text = "font-name=desk\nversion = 1\n[look big]\nfont-name = desk\n[look small]\nfont-name = desk\n[head a]\nlook = big\n[head b]\nlook = small\n[rules]\ndisplay * -> a\ndisplay * -> b\n";
    let (_dir, file, _) = heads_fixture("asked-once", text, 0o644);
    let frits = file.with_file_name("frits");
    std::fs::create_dir_all(&frits).unwrap();
    std::fs::write(frits.join("desk.vtfrit"), "source=builtin:unscii-8\n").unwrap();
    let host = Host {
        conf_file: file.with_file_name("absent.conf"),
        udev_control: file.clone(),
        roots: nowhere(),
        frits: frits.clone(),
        frit_trust: font::trust().below(file.parent().unwrap()),
        systemd_root: PathBuf::from("/nonexistent/vtium/root"),
        batch: PathBuf::from("/nonexistent/vtium/batch.conf"),
        drawer: None,
    };
    let (covered, lacked) = (std::cell::Cell::new(0), std::cell::Cell::new(0));
    let cover = |_: &helper::FritText, _: &[u32]| {
        covered.set(covered.get() + 1);
        Ok((Vec::new(), None))
    };
    let sources = |_: &helper::FritText| {
        lacked.set(lacked.get() + 1);
        Ok(String::new())
    };
    let args: Vec<OsString> = [SUBCOMMAND, "--no-devices", file.to_str().unwrap()]
        .iter()
        .map(OsString::from)
        .collect();
    let out = outcome_with(
        &config::load(&args, &mut config::read_bytes),
        &host,
        Some(Squeeze {
            locale: "hi_IN.UTF-8",
            ask: &cover,
        }),
        Some(FontSources { ask: &sources }),
    );
    let squeezed = out.stdout.iter().filter(|l| l.contains("squeezed")).count();
    assert_eq!(squeezed, 2, "a note for each look: {:#?}", out.stdout);
    assert_eq!((covered.get(), lacked.get()), (1, 1));
}

// A font file that font-name's set names and that a start treats as
// missing is said once, with the font set's file and line, though
// font-name and two looks name the same set.
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn a_font_file_line_is_said_once_for_a_set_the_top_level_and_looks_share() {
    let text = "font-name=desk\nversion = 1\n[look big]\nfont-name = desk\n[look small]\nfont-name = desk\n[head a]\nlook = big\n[head b]\nlook = small\n[rules]\ndisplay * -> a\ndisplay * -> b\n";
    let (_dir, file, _) = heads_fixture("font-file-once", text, 0o644);
    let frits = file.with_file_name("frits");
    std::fs::create_dir_all(&frits).unwrap();
    let idle = file.with_file_name("idle.bdf");
    std::fs::write(&idle, "x").unwrap();
    std::fs::write(
        frits.join("desk.vtfrit"),
        format!("source=bdf:{}\nsource=builtin:unscii-8\n", idle.display()),
    )
    .unwrap();
    let host = Host {
        conf_file: file.with_file_name("absent.conf"),
        udev_control: file.clone(),
        roots: nowhere(),
        frits: frits.clone(),
        frit_trust: font::trust().below(file.parent().unwrap()),
        systemd_root: PathBuf::from("/nonexistent/vtium/root"),
        batch: PathBuf::from("/nonexistent/vtium/batch.conf"),
        drawer: None,
    };
    let args: Vec<OsString> = [SUBCOMMAND, "--no-devices", file.to_str().unwrap()]
        .iter()
        .map(OsString::from)
        .collect();
    let out = outcome_with(
        &config::load(&args, &mut config::read_bytes),
        &host,
        None,
        None,
    );
    let said: Vec<&String> = out
        .stderr
        .iter()
        .filter(|l| l.contains("is not active"))
        .collect();
    assert_eq!(said.len(), 1, "{:#?}", out.stderr);
    assert!(
        said.first()
            .is_some_and(|l| l.contains(&format!("desk.vtfrit:1: The font {}", idle.display())))
    );
}

// A font that font-name's set needs and this machine lacks is a
// warning: check-config prints it on standard error and exits with
// vtium_settings::CHECK_MISSING_FONTS. An
// older version of a font is a note, which leaves the status alone.
//
// References:
// - ADR-1204 D7: check-config names each missing font.
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn a_missing_font_is_a_warning_and_an_older_one_a_note() {
    let text = "font-name=desk\nversion = 1\n[head a]\n[rules]\ndisplay * -> a\n";
    let (_dir, file, _) = heads_fixture("missing-warns", text, 0o644);
    let frits = file.with_file_name("frits");
    std::fs::create_dir_all(&frits).unwrap();
    std::fs::write(
        frits.join("desk.vtfrit"),
        "source=font:Gone Mono\nsource=font:Old Mono\nversion=3.1\nsource=builtin:unscii-8\n",
    )
    .unwrap();
    let host = Host {
        conf_file: file.with_file_name("absent.conf"),
        udev_control: file.clone(),
        roots: nowhere(),
        frits: frits.clone(),
        frit_trust: font::trust().below(file.parent().unwrap()),
        systemd_root: PathBuf::from("/nonexistent/vtium/root"),
        batch: PathBuf::from("/nonexistent/vtium/batch.conf"),
        drawer: None,
    };
    let args: Vec<OsString> = [SUBCOMMAND, "--no-devices", file.to_str().unwrap()]
        .iter()
        .map(OsString::from)
        .collect();
    let check = |answer: &'static str| {
        let ask = move |_: &helper::FritText| Ok(answer.to_owned());
        outcome_with(
            &config::load(&args, &mut config::read_bytes),
            &host,
            None,
            Some(FontSources { ask: &ask }),
        )
    };
    let page = "To install a font that a font set needs, see https://vtium.github.io/vtium/fonts/#install-a-missing-font.";
    let missing = check("missing\t1\tfont:Gone Mono\tno font of family Gone Mono");
    assert_eq!(missing.status, vtium_settings::CHECK_MISSING_FONTS);
    assert_eq!(
        missing.stderr,
        [format!(
            "Font set desk needs the font Gone Mono, and no font of that name is installed. {page}"
        )]
    );
    // a font named by its path is a file this machine lacks
    let file = check("missing\t4\tbdf:/nonexistent/gone.bdf\tnot on this machine");
    assert_eq!(
        file.stderr,
        [format!(
            "Font set desk needs the font /nonexistent/gone.bdf, and there is no file at that path. {page}"
        )]
    );
    // a font this machine has and vtium cannot read is a note, with why
    let refused = check("refused\t1\tfont:Gone Mono\tthe file is not a font");
    assert_eq!(
        (refused.status, refused.stderr.clone()),
        (0, Vec::<String>::new())
    );
    assert!(
        refused.stdout.contains(&"Font set desk: vtium cannot use the font Gone Mono: the file is not a font. It draws that font's characters from the next font that has them.".to_owned()),
        "{:#?}",
        refused.stdout
    );
    // a font this machine has and the font helper refuses, by name,
    // is a warning on a file a start treats as missing, with what to
    // do in place of installing it
    let b = vtium_settings::batch::BATCH_FILE;
    let idle = check(
        "missing\t1\tfont:Gone Mono\t/usr/share/fonts/gone.otf: vtium treats this file as missing, because it is not among the active fonts.",
    );
    assert_eq!(idle.status, vtium_settings::CHECK_MISSING_FONTS);
    assert_eq!(
        idle.stderr,
        [format!(
            "Font set desk needs the font Gone Mono, and its file /usr/share/fonts/gone.otf is not active, so vtium treats the font as missing and draws its characters from the next font in the set. A font is active while {b} lists the file's path. To activate it, press Space on it in vtium-config's Fonts tab, or run vtium-config activate with the file's path, then install the change."
        )]
    );
    let open = check(
        "missing\t1\tfont:Gone Mono\t/usr/share/fonts/gone.otf: vtium treats this file as missing, because other users can change \"/usr/share/fonts/gone.otf\" (mode 666). Run `chmod go-w /usr/share/fonts/gone.otf` so that only its owner can change it.",
    );
    assert_eq!(
        open.stderr,
        [
            "Font set desk needs the font Gone Mono, and vtium treats its file /usr/share/fonts/gone.otf as missing, because other users can change \"/usr/share/fonts/gone.otf\" (mode 666). Run `chmod go-w /usr/share/fonts/gone.otf` so that only its owner can change it. vtium draws the font's characters from the next font in the set."
        ]
    );
    // a font named by its path that the helper refuses is the font
    // file line's to report, with the font set's file and line
    let by_path = check(
        "missing\t4\tbdf:/usr/share/fonts/gone.bdf\t/usr/share/fonts/gone.bdf: vtium treats this file as missing, because it is not among the active fonts.",
    );
    assert_eq!((by_path.status, by_path.stderr), (0, Vec::<String>::new()));
    let older = check("older\t2\tfont:Old Mono\t3.000");
    assert_eq!((older.status, older.stderr), (0, Vec::<String>::new()));
    assert!(
        older.stdout.contains(&format!(
            "Font set desk asks for the font Old Mono, version 3.1 or later, and the installed font is version 3.000. vtium still uses it. {page}"
        )),
        "{:#?}",
        older.stdout
    );
}

// When every problem check-config finds is a font that a font set
// needs and this machine lacks, it exits with a status of its own, so
// vtium-config can install the file without reading the warnings'
// words. The warnings may come from font-name's set or from a look's.
// Any other problem beside them makes the status 1, and notes alone
// leave it 0.
//
// References:
// - ADR-1204 D7: check-config names each missing font.
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn only_missing_fonts_exit_with_their_own_status() {
    let look = "version = 1\n[look big]\nfont-name = desk\n[head a]\nlook = big\n[rules]\ndisplay * -> a\n";
    let status = |name: &str, text: &str, answer: &'static str| {
        let (_dir, file, _) = heads_fixture(name, text, 0o644);
        let frits = file.with_file_name("frits");
        std::fs::create_dir_all(&frits).unwrap();
        std::fs::write(
            frits.join("desk.vtfrit"),
            "source=font:Gone Mono\nsource=font:Old Mono\nversion=3.1\nsource=builtin:unscii-8\n",
        )
        .unwrap();
        let host = Host {
            conf_file: file.with_file_name("absent.conf"),
            udev_control: file.clone(),
            roots: nowhere(),
            frits: frits.clone(),
            frit_trust: font::trust().below(file.parent().unwrap()),
            systemd_root: PathBuf::from("/nonexistent/vtium/root"),
            batch: PathBuf::from("/nonexistent/vtium/batch.conf"),
            drawer: None,
        };
        let args: Vec<OsString> = [SUBCOMMAND, "--no-devices", file.to_str().unwrap()]
            .iter()
            .map(OsString::from)
            .collect();
        let ask = move |_: &helper::FritText| Ok(answer.to_owned());
        let out = outcome_with(
            &config::load(&args, &mut config::read_bytes),
            &host,
            None,
            Some(FontSources { ask: &ask }),
        );
        (out.status, out.stderr.len())
    };
    let missing = "missing\t1\tfont:Gone Mono\tno font of family Gone Mono";
    let only = vtium_settings::CHECK_MISSING_FONTS;
    // font-name's set, at the top level
    let top = "font-name=desk\nversion = 1\n[head a]\n[rules]\ndisplay * -> a\n";
    assert_eq!(status("only-top", top, missing), (only, 1));
    // a look's set, among the sections, and both at once
    assert_eq!(status("only-look", look, missing), (only, 1));
    let both = format!("font-name=desk\n{look}");
    assert_eq!(status("only-both", &both, missing), (only, 2));
    // a font that lacks a character its check= names counts the same
    let lacks = "missing\t1\tfont:Gone Mono\tit lacks U+E0B0\nlacks\t1\tfont:Gone Mono\tU+E0B0";
    assert_eq!(status("only-lacks", look, lacks), (only, 1));
    // a missing font beside a top-level warning, or an error in a
    // section, is 1
    let unknown = format!("no-such-key=1\n{both}");
    assert_eq!(status("and-top", &unknown, missing).0, 1);
    let broken = look.replace("look = big\n", "look = big\nno-such-key = 1\n");
    assert_eq!(status("and-section", &broken, missing).0, 1);
    // notes alone: an older font, and a font the helper cannot read
    assert_eq!(
        status("notes", &both, "older\t2\tfont:Old Mono\t3.000"),
        (0, 0)
    );
    let refused = "refused\t1\tfont:Gone Mono\tthe file is not a font";
    assert_eq!(status("refused", &both, refused), (0, 0));
}

// heads.md: each file docs/heads holds checks with exit status 0, the
// frits its looks name installed, away from any machine, but off.conf
// covers: platform.heads.rules
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn each_documented_head_file_checks_with_exit_0() {
    let docs = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/heads"));
    let (_dir, anchor, _) = heads_fixture("docs", "", 0o644);
    let frits = anchor.with_file_name("frits");
    std::fs::create_dir_all(&frits).unwrap();
    let host = Host {
        conf_file: anchor.with_file_name("absent.conf"),
        udev_control: anchor.clone(),
        roots: nowhere(),
        frits: frits.clone(),
        frit_trust: font::trust().below(anchor.parent().unwrap()),
        systemd_root: PathBuf::from("/nonexistent/vtium/root"),
        batch: PathBuf::from("/nonexistent/vtium/batch.conf"),
        drawer: None,
    };
    let mut files: Vec<PathBuf> = std::fs::read_dir(docs)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "conf"))
        .collect();
    files.sort();
    // the frits the files' looks name are installed
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        let frits_named = text.lines().filter_map(|l| {
            let (key, value) = l.split('#').next()?.split_once('=')?;
            (key.trim() == "font-name").then(|| value.trim().to_owned())
        });
        for p in frits_named {
            std::fs::write(
                frits.join(format!("{p}.vtfrit")),
                "source=builtin:unifont\n",
            )
            .unwrap();
        }
    }
    assert!(files.len() >= 7, "{files:?}");
    for f in &files {
        let out = outcome_on(&[f.to_str().unwrap()], &host);
        // off.conf lights nothing and opens nothing on purpose, so it
        // wires no complete route, which check-config refuses
        // (ADR-1403 D3)
        let want = match f.file_name().and_then(|n| n.to_str()) {
            Some("off.conf") => (
                1,
                vec![format!(
                    "{}: No display rule sends a display to a head. Add a rule `display MATCH -> TARGET` whose target is a head.",
                    f.display()
                )],
            ),
            _ => (0, Vec::new()),
        };
        assert_eq!((out.status, out.stderr), want, "{f:?}");
    }
}

/// Roots under which nothing is listed.
fn nowhere() -> Roots {
    Roots {
        sys: PathBuf::from("/nonexistent/vtium/sys"),
        udev_data: PathBuf::from("/nonexistent/vtium/udev"),
    }
}

// the devices vtium-platform lists, through the mapping the start uses:
// each card's displays, each framebuffer's as `fb`, then each input,
// and a display's EDID as an `edid:` rule matches it
#[test]
fn listed_devices_map_as_the_start_maps_them() {
    use vtium_platform::display::{Edid, Identity as Display};
    use vtium_platform::input::{Class as Udev, Classes as UdevClasses};
    use vtium_platform::listing::{Card, Framebuffer, Listing};
    use vtium_platform::udev::InputIdentity;

    let input = |class: Udev, name: &str| InputIdentity {
        classes: UdevClasses::empty().with(class),
        tagged: true,
        name: Some(name.to_owned()),
        ..InputIdentity::default()
    };
    let display = |connector: &str| Display {
        connector: connector.to_owned(),
        card: Some("pci-0000:03:00.0".to_owned()),
        edid: None,
        connector_path: None,
        lit_at_start: false,
    };
    // an EDID naming maker `AAA`, product 0x0001 and serial number 7
    let named = |connector: &str| {
        let mut base = [0u8; 128];
        base[..8].copy_from_slice(&[0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0]);
        base[8..16].copy_from_slice(&[0x04, 0x21, 0x01, 0x00, 7, 0, 0, 0]);
        base[127] = 0u8.wrapping_sub(base.iter().fold(0u8, |sum, &b| sum.wrapping_add(b)));
        Display {
            edid: Edid::parse(&base).map(|e| e.id().clone()),
            ..display(connector)
        }
    };
    let listing = Listing {
        inputs: vec![
            input(Udev::Keyboard, "Keyboard A"),
            input(Udev::Mouse, "A mouse"),
        ],
        cards: vec![Card {
            node: PathBuf::from("/dev/dri/card1"),
            id_path: Some("pci-0000:03:00.0".to_owned()),
            primary: true,
            displays: vec![display("DP-2"), named("DP-1")],
        }],
        framebuffers: vec![Framebuffer {
            node: PathBuf::from("/dev/fb2"),
            primary: false,
        }],
    };
    // a device's connector else name, card, boot card, card order, order
    type Shown<'a> = (Option<&'a str>, Option<&'a str>, bool, u32, u32);
    let devices = devices(&listing);
    let shown: Vec<Shown> = devices
        .iter()
        .map(|d| {
            (
                d.connector.as_deref().or(d.name.as_deref()),
                d.card.as_deref(),
                d.on_boot_card,
                d.card_order,
                d.order,
            )
        })
        .collect();
    let gpu = Some("pci-0000:03:00.0");
    assert_eq!(
        shown,
        [
            (Some("DP-2"), gpu, true, 1, 0),
            (Some("DP-1"), gpu, true, 1, 1),
            (Some("fb"), None, false, 2, 0),
            (Some("Keyboard A"), None, false, 0, 0),
            (Some("A mouse"), None, false, 0, 0),
        ]
    );
    let edids: Vec<Option<&vtium_settings::Edid>> =
        devices.iter().map(|d| d.edid.as_ref()).collect();
    let seven = vtium_settings::Edid {
        maker: "AAA".to_owned(),
        product: 1,
        serial: "0x00000007".to_owned(),
    };
    assert_eq!(edids, [None, Some(&seven), None, None, None]);
}

/// A stand-in machine under `dir`: a card with one lit connector, a
/// keyboard udev has not tagged, and a mouse it has.
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn stand_in_machine(dir: &Path) -> Roots {
    let sys = dir.join("sys");
    let write = |path: &str, text: &str| {
        let path = sys.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    let link = |path: &str, target: &str| {
        let path = sys.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(target, path).unwrap();
    };
    let card = "devices/platform/gpu/drm/card0";
    write(&format!("{card}/dev"), "226:0\n");
    write(&format!("{card}/card0-HDMI-A-1/status"), "connected\n");
    write(&format!("{card}/card0-HDMI-A-1/modes"), "1920x1080\n");
    link("class/drm/card0", &format!("../../{card}"));
    for (input, event, name, caps) in [
        ("input0", "event0", "Keyboard A", "fffffffe"),
        ("input1", "event1", "A mouse", "0"),
    ] {
        let dir = format!("devices/platform/{input}/input/{input}");
        link(&format!("{dir}/subsystem"), "../../../../class/input");
        write(&format!("{dir}/name"), &format!("{name}\n"));
        write(&format!("{dir}/capabilities/ev"), "3\n");
        write(&format!("{dir}/capabilities/key"), &format!("{caps}\n"));
        let minor = if event == "event0" { 64 } else { 65 };
        write(&format!("{dir}/{event}/dev"), &format!("13:{minor}\n"));
        link(
            &format!("class/input/{event}"),
            &format!("../../{dir}/{event}"),
        );
    }
    let udev = dir.join("udev");
    std::fs::create_dir_all(&udev).unwrap();
    std::fs::write(udev.join("c13:65"), "E:ID_INPUT=1\nE:ID_INPUT_MOUSE=1\n").unwrap();
    Roots {
        sys,
        udev_data: udev,
    }
}

// ADR-1401 D7, D9: the devices listed from sysfs and udev's database
// give the hardware findings on standard output, outside the status
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn the_devices_present_give_hardware_findings_outside_the_status() {
    let (_dir, file, _) = heads_fixture(
        "devices",
        "version = 1\n[head desk]\n[head panel]\n[rules]\n\
         display * -> desk\n\
         input class:keyboard -> desk\n\
         input class:mouse -> panel\n\
         input class:joystick -> panel\n",
        0o644,
    );
    let f = file.display().to_string();
    let host = Host {
        conf_file: file.with_file_name("absent.conf"),
        udev_control: file.clone(),
        roots: stand_in_machine(file.parent().unwrap()),
        frits: PathBuf::from("/nonexistent/vtium/frits"),
        frit_trust: font::trust(),
        systemd_root: PathBuf::from("/nonexistent/vtium/root"),
        batch: PathBuf::from("/nonexistent/vtium/batch.conf"),
        drawer: None,
    };
    let out = outcome_on(&[file.to_str().unwrap()], &host);
    assert_eq!((out.status, out.stderr), (0, Vec::new()));
    let heads: Vec<&String> = out.stdout.iter().filter(|n| n.starts_with(&f)).collect();
    assert_eq!(
        heads,
        [
            &format!(
                "{f}: \"{f}\" belongs to user {}, not to root. That is fine for a check: vtium reads the file once it is installed, owned by root and writable only by root.",
                rustix::process::geteuid().as_raw()
            ),
            &format!("{f}:8: the rule matches no connected device"),
            &format!("{f}: head desk has only one keyboard, \"Keyboard A\""),
            &format!(
                "{f}: head panel has mice but no keyboard: every keyboard goes to another head"
            ),
            &format!("{f}: {AS_WRITTEN}"),
            &format!("{f}: {UNSET}"),
        ]
    );
}

// a rule noted as matching nothing here for want of a udev daemon, or
// as unchecked for want of a connector's PATH, takes no second note as
// idle
#[test]
fn a_rule_noted_as_matching_nothing_is_noted_once() {
    let text = "version = 1\n[head desk]\n[rules]\n\
                display connector-path:mst:63-1 -> desk\n\
                input path:pci-0000:15:00.0-usb-0:3.3 -> desk\n\
                input class:joystick -> desk\n";
    let (_dir, file, trust) = heads_fixture("once", text, 0o644);
    let f = file.display().to_string();
    let machine = Machine {
        udev_daemon: false,
        devices: Some(&[]),
        vts: VtsEnabled::FileAlone,
    };
    let out = conf_file(&file, false, &trust, &machine, &caps(4));
    assert_eq!(
        out.notes,
        [
            format!("{f}:5: `path:` matches nothing, because udev is not running"),
            format!(
                "{f}:4: `connector-path:` is not checked against the connected displays, because check-config does not open the graphics card; vtium matches it when it finds the display"
            ),
            format!("{f}:6: the rule matches no connected device"),
            format!(
                "{f}: No head that can be active has a connected display, so a roaming keyboard reaches no head until one turns on. Connect a display for a head that is not isolated."
            ),
            format!("{f}: {NO_DISPLAY}"),
            format!("{f}: {AS_WRITTEN}"),
            format!("{f}: {UNSET}"),
        ]
    );
}

// a newline or an escape in FILE never splits a line or reaches the
// terminal as itself
#[test]
fn a_control_character_in_the_path_is_shown_escaped() {
    let path = Path::new("/tmp/vtium-check-heads-a\nb\u{1b}[2J.conf");
    assert_eq!(
        conf_file(path, true, &Trust::ROOT, &machine(true), &caps(4)).counted,
        ["/tmp/vtium-check-heads-a\\nb\\u{1b}[2J.conf: does not exist"]
    );
}

#[test]
fn help_says_it_only_checks() {
    let (lines, status) = run_with(&["--help"], "");
    assert_eq!(status, 0);
    assert_eq!(lines, [HELP.trim_end()]);
    assert!(HELP.contains("It only reads"));
    assert!(
        HELP.replace('\n', " ")
            .contains("It opens nothing under /dev and changes nothing.")
    );
}

// check-config reports a look's pointer theme it would not read on the
// look's own line of the file it checks, and the line counts.
#[test]
fn check_config_reports_a_look_s_pointer_theme_on_the_look_s_line() {
    let (dir, file, _) = heads_fixture(
        "look-pointer",
        "version = 1\n[look lost]\npointer-theme = /nonexistent/vtium/pointers\n[head console]\nlook = lost\n[rules]\ndisplay * -> console\ninput * -> roaming\n",
        0o644,
    );
    let host = Host {
        conf_file: file.with_file_name("installed.conf"),
        udev_control: file.clone(),
        roots: nowhere(),
        frits: PathBuf::from("/nonexistent/vtium/frits"),
        frit_trust: font::trust().below(&dir),
        systemd_root: PathBuf::from("/nonexistent/vtium/root"),
        batch: dir.join("batch.conf"),
        drawer: None,
    };
    let f = file.display().to_string();
    let out = outcome_on(&["--no-devices", f.as_str()], &host);
    assert_eq!(
        out.stderr,
        [format!(
            "{f}:3: The pointer file /nonexistent/vtium/pointers is neither an SVG file nor a folder that holds a cursors folder, so vtium draws its own pointer pictures."
        )]
    );
    assert_eq!(out.status, 1);
}

// The looks of a file that runs each name their pointer file for the
// check to read, and a look that names none is not listed.
#[test]
fn each_look_s_pointer_theme_is_listed_for_the_check() {
    let text = "version = 1\n[look arrows]\npointer-theme = /usr/share/icons/Adwaita\n[look none]\n[head desk]\nlook = arrows\n[rules]\ndisplay * -> desk\ninput * -> desk\n";
    let (out, _) = checked("pointers", text, true);
    assert_eq!(
        out.pointers,
        [(
            "arrows".to_owned(),
            "/usr/share/icons/Adwaita".to_owned(),
            3
        )]
    );
}

// The looks of a file that runs each name their vtlook for the check
// to read, but for the built-in one, which no file holds.
#[test]
fn each_look_s_vtlook_is_listed_for_the_check() {
    let text = "version = 1\n[look warm]\nvtlook = amber\n[look plain]\nvtlook = default\n[look none]\n[head desk]\nlook = warm\n[rules]\ndisplay * -> desk\ninput * -> desk\n";
    let (out, _) = checked("vtlooks", text, true);
    assert_eq!(out.vtlooks, [("warm".to_owned(), "amber".to_owned(), 3)]);
}

// A build without frame filters reads no vtlook: each place that names
// one says which heads draw the built-in vtlook instead, and counts.
#[cfg(not(feature = "filters"))]
#[test]
fn a_build_without_filters_warns_of_each_vtlook_named() {
    let conf = Path::new("/etc/vtium/vtium.conf");
    let looks = [("warm".to_owned(), "amber".to_owned(), 3)];
    let top = Some(("crt", "/etc/vtium/vtium.conf:1".to_owned()));
    assert_eq!(
        vtlook_findings(
            top,
            conf,
            &looks,
            &[Path::new("/nonexistent")],
            &font::trust(),
            &palette_gate(Path::new("/nonexistent"))
        ),
        [
            "/etc/vtium/vtium.conf:1: This build of vtium has no frame filters, so the heads whose look names no vtlook of its own draw the built-in vtlook instead of vtlook crt.",
            "/etc/vtium/vtium.conf:3: This build of vtium has no frame filters, so the heads that use look warm draw the built-in vtlook instead of vtlook amber.",
        ]
    );
    assert!(
        vtlook_findings(
            None,
            conf,
            &[],
            &[Path::new("/nonexistent")],
            &font::trust(),
            &palette_gate(Path::new("/nonexistent"))
        )
        .is_empty()
    );
}

// check-config lists every error of every vtlook named, each vtlook
// read once however many looks name it, each line starting with the
// vtlook's file and line, and then each place that names the vtlook.
#[cfg(feature = "filters")]
#[test]
fn check_config_lists_every_error_of_each_vtlook_once() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new("check-vtlooks");
    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    let trust = Trust::owner(rustix::process::geteuid().as_raw()).below(&dir);
    let write = |name: &str, text: &str| {
        let file = dir.join(format!("{name}.vtlook"));
        let _ = std::fs::write(&file, text);
        let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644));
        file.display().to_string()
    };
    let old = write("old", "# old.vtlook\n[color sepia]\namount = 0.5\n");
    write("plain", "version = 1\n[requires]\ngpu = any\n");
    let conf = Path::new("/etc/vtium/vtium.conf");
    let looks = [
        ("a".to_owned(), "old".to_owned(), 3),
        ("b".to_owned(), "old".to_owned(), 5),
        ("c".to_owned(), "plain".to_owned(), 7),
    ];
    let top = Some(("gone", "argument 2".to_owned()));
    assert_eq!(
        vtlook_findings(top, conf, &looks, &[&dir], &trust, &palette_gate(&dir)),
        [
            format!(
                "{}: reading: There is no file gone.vtlook in {}.",
                dir.join("gone.vtlook").display(),
                dir.display()
            ),
            "argument 2: The vtlook gone has errors, so the heads whose look names no vtlook of its own draw the built-in vtlook instead.".to_owned(),
            format!(
                "{old}:2: reading: found [color sepia] before version = 1, needs version = 1 first"
            ),
            format!(
                "{old}:2: color filter 1 of 1, sepia: checking: found sepia, needs gamma, contrast, tint, color-blind or cube"
            ),
            "/etc/vtium/vtium.conf:3: The vtlook old has errors, so the heads that use look a draw the built-in vtlook instead.".to_owned(),
            "/etc/vtium/vtium.conf:5: The vtlook old has errors, so the heads that use look b draw the built-in vtlook instead.".to_owned(),
        ]
    );
    assert!(
        vtlook_findings(
            None,
            conf,
            &looks[2..],
            &[&dir],
            &trust,
            &palette_gate(&dir)
        )
        .is_empty()
    );
    // past the bound, the rest of a vtlook's errors are counted
    let many = write("many", &"a\n".repeat(100));
    let said = vtlook_findings(
        None,
        conf,
        &[("m".to_owned(), "many".to_owned(), 9)],
        &[&dir],
        &trust,
        &palette_gate(&dir),
    );
    assert_eq!(said.len(), vtium_settings::ERRORS_MAX + 2, "{said:#?}");
    assert_eq!(
        said.get(vtium_settings::ERRORS_MAX).map(String::as_str),
        Some(format!("{many}: More errors: 37").as_str())
    );
    // a file named default.vtlook is reported though nothing names it
    write("default", "version = 1\n");
    assert_eq!(
        vtlook_findings(
            None,
            conf,
            &looks[2..],
            &[&dir],
            &trust,
            &palette_gate(&dir)
        ),
        [format!(
            "{}: reading: vtium never reads a file named default.vtlook, because default is the name of the built-in vtlook. Give the file another name.",
            dir.join("default.vtlook").display()
        )]
    );
}

// check-config reads a vtlook from the administrator's folder and then
// the package's, as the start does: a preset with no copy in the
// administrator's folder has nothing to report, and a broken copy there
// is the file check-config names, though the preset reads cleanly.
//
// References:
// - ADR-1304 D4, its note of 2026-11-19: the two folders, and the administrator's file winning on a name.
#[cfg(feature = "filters")]
#[test]
fn check_config_reads_a_preset_and_names_the_administrator_s_copy() {
    use std::os::unix::fs::PermissionsExt;
    let top = Scratch::new("check-vtlook-presets");
    let (etc, share) = (top.join("etc"), top.join("share"));
    for dir in [&*top, &etc, &share] {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755));
    }
    let trust = Trust::owner(rustix::process::geteuid().as_raw()).below(&top);
    let write = |dir: &Path, text: &str| {
        let file = dir.join("amber.vtlook");
        let _ = std::fs::write(&file, text);
        let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644));
        file.display().to_string()
    };
    write(&share, "version = 1\n[color tint]\ncolor = 255,176,0\n");
    let conf = Path::new("/etc/vtium/vtium.conf");
    let looks = [("warm".to_owned(), "amber".to_owned(), 3)];
    let folders = [etc.as_path(), share.as_path()];
    assert_eq!(
        vtlook_findings(None, conf, &looks, &folders, &trust, &palette_gate(&etc)),
        Vec::<String>::new()
    );
    let copy = write(&etc, "version = 1\n[color sepia]\n");
    assert_eq!(
        vtlook_findings(None, conf, &looks, &folders, &trust, &palette_gate(&etc)),
        [
            format!(
                "{copy}:2: color filter 1 of 1, sepia: checking: found sepia, needs gamma, contrast, tint, color-blind or cube"
            ),
            "/etc/vtium/vtium.conf:3: The vtlook amber has errors, so the heads that use look warm draw the built-in vtlook instead.".to_owned(),
        ]
    );
}

// check-config reads each `.cube` file a vtlook's cube filter names as
// the start reads it, through the batch and the trust rule, and names
// the file of each one the start would refuse: one others can change,
// one the batch does not list, and one whose lines do not read. An
// active file that reads cleanly gives nothing to report.
//
// References:
// - ADR-1205 D4: a `.cube` file counts as missing unless the batch lists it and it passes the trust rule, and check-config names the check it failed.
// - ADR-1304 D6, D14: check-config lists a `.cube` file's faults on its lines.
#[cfg(feature = "filters")]
#[test]
fn check_config_names_each_cube_file_a_start_refuses() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new("check-vtlook-cubes");
    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755));
    let trust = Trust::owner(rustix::process::geteuid().as_raw()).below(&dir);
    let write = |name: &str, text: &str, mode: u32| {
        let file = dir.join(name);
        let _ = std::fs::write(&file, text);
        let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode));
        file.display().to_string()
    };
    for name in ["good", "loose", "bad", "off"] {
        write(
            &format!("{name}.vtlook"),
            &format!("version = 1\n[color cube]\nfile = {name}.cube\n"),
            0o644,
        );
    }
    write("good.cube", "LUT_1D_SIZE 2\n0 0 0\n1 1 1\n", 0o644);
    let loose = write("loose.cube", "LUT_1D_SIZE 2\n0 0 0\n1 1 1\n", 0o664);
    let bad = write("bad.cube", "LUT_3D_SIZE 129\n", 0o644);
    // the batch lists every file written so far, and not off.cube
    let gate = palette_gate(&dir);
    let off = write("off.cube", "LUT_1D_SIZE 2\n0 0 0\n1 1 1\n", 0o644);
    let conf = Path::new("/etc/vtium/vtium.conf");
    let looks: Vec<(String, String, usize)> = ["good", "loose", "bad", "off"]
        .iter()
        .zip([3, 5, 7, 9])
        .map(|(name, line)| (format!("look-{name}"), (*name).to_owned(), line))
        .collect();
    let fails = |name: &str, line: usize| {
        format!(
            "/etc/vtium/vtium.conf:{line}: The vtlook {name} has errors, so the heads that use look look-{name} draw the built-in vtlook instead."
        )
    };
    assert_eq!(
        vtlook_findings(None, conf, &looks, &[&dir], &trust, &gate),
        [
            format!(
                "{loose}: color filter 1 of 1, cube: reading: vtium treats the file as missing, because other users can change \"{loose}\" (mode 664). Run `chmod go-w {loose}` so that only its owner can change it."
            ),
            fails("loose", 5),
            format!(
                "{bad}:1: color filter 1 of 1, cube: checking: found LUT_3D_SIZE 129, needs 2 to 65"
            ),
            fails("bad", 7),
            format!(
                "{off}: color filter 1 of 1, cube: reading: vtium treats the file as missing, because it is not active: {} does not list it.",
                vtium_settings::batch::BATCH_FILE
            ),
            fails("off", 9),
        ]
    );
}

// `vtium check-config` reads the vtlook that the top level names and
// the one each look names, prints what it finds on standard error, and
// exits 1. The vtlooks are named so that no machine has them, so the
// check reads the real looks folder and finds neither. A file that
// fails the ownership check has no top level a start reads, so the
// check reads no top-level vtlook from it.
#[test]
fn check_config_reports_the_vtlooks_vtium_conf_names() {
    let top = "vtium-check-config-test-top";
    let look = "vtium-check-config-test-look";
    let text = format!(
        "vtlook = {top}\nversion = 1\n[look warm]\nvtlook = {look}\n[head a]\nlook = warm\n[rules]\ndisplay * -> a\ninput path:pci-0000:00:14.0-usb-0:1 -> a\n"
    );
    let (_dir, file, _) = heads_fixture("vtlooks-out", &text, 0o644);
    let f = file.display().to_string();
    let host = Host {
        conf_file: file.with_file_name("installed.conf"),
        udev_control: file.clone(),
        roots: nowhere(),
        frits: PathBuf::from("/nonexistent/vtium/frits"),
        frit_trust: font::trust(),
        systemd_root: PathBuf::from("/nonexistent/vtium/root"),
        batch: PathBuf::from("/nonexistent/vtium/batch.conf"),
        drawer: None,
    };
    let args: Vec<OsString> = [SUBCOMMAND, "--no-devices", f.as_str()]
        .iter()
        .map(OsString::from)
        .collect();
    let out = outcome(&config::load(&args, &mut config::read_bytes), &host);
    assert_eq!(out.status, 1, "{:#?}", out.stderr);
    let looks = vtium_settings::vtlook::LOOKS;
    let names_top = if cfg!(feature = "filters") {
        format!(
            "{f}:1: The vtlook {top} has errors, so the heads whose look names no vtlook of its own draw the built-in vtlook instead."
        )
    } else {
        format!(
            "{f}:1: This build of vtium has no frame filters, so the heads whose look names no vtlook of its own draw the built-in vtlook instead of vtlook {top}."
        )
    };
    let names_look = if cfg!(feature = "filters") {
        format!(
            "{f}:4: The vtlook {look} has errors, so the heads that use look warm draw the built-in vtlook instead."
        )
    } else {
        format!(
            "{f}:4: This build of vtium has no frame filters, so the heads that use look warm draw the built-in vtlook instead of vtlook {look}."
        )
    };
    for line in [&names_top, &names_look] {
        assert!(out.stderr.contains(line), "{line}\n{:#?}", out.stderr);
    }
    if cfg!(feature = "filters") {
        for name in [top, look] {
            let read = format!("{looks}/{name}.vtlook: reading: ");
            assert!(
                out.stderr.iter().any(|l| l.starts_with(&read)),
                "{read}\n{:#?}",
                out.stderr
            );
        }
    }
    // an installed file the ownership check refuses: a start reads no
    // top-level vtlook from it, but takes one from the command line
    let (_dir, file, _) = heads_fixture("vtlooks-refused", &text, 0o644);
    let host = Host {
        conf_file: file.clone(),
        ..host
    };
    let installed = |args: &[&str]| {
        let args: Vec<OsString> = [SUBCOMMAND, "--no-devices"]
            .iter()
            .chain(args)
            .map(OsString::from)
            .collect();
        let text = text.clone();
        let loaded = config::load(&args, &mut |p: &Path| {
            Ok((p == Path::new("/etc/vtium/vtium.conf")).then(|| text.as_bytes().to_vec()))
        });
        outcome(&loaded, &host)
    };
    let out = installed(&[]);
    assert_eq!(out.status, 1);
    assert!(
        !out.stderr
            .iter()
            .any(|l| l.contains("vtium-check-config-test")),
        "{:#?}",
        out.stderr
    );
    let out = installed(&["--vtlook=vtium-check-config-test-arg"]);
    assert!(
        out.stderr
            .iter()
            .any(|l| l.starts_with("argument 3: ") && l.contains("vtium-check-config-test-arg")),
        "{:#?}",
        out.stderr
    );
}

// A top-level name that cannot name a vtlook, such as a path, is an
// error of vtium.conf at its line, or of the command-line argument that
// gives it, and check-config reports it once, in every build.
//
// References:
// - ADR-1304 D5: a vtlook's name, and the top level's `vtlook`.
// - ADR-1304 D6: check-config reports each error whole, once.
#[test]
fn a_top_level_vtlook_that_is_no_name_is_reported_once_where_it_is_given() {
    let text = "vtlook = ../x\nversion = 1\n[head a]\n[rules]\ndisplay * -> a\n";
    let (_dir, file, _) = heads_fixture("vtlook-no-name", text, 0o644);
    let f = file.display().to_string();
    let host = Host {
        conf_file: file.with_file_name("installed.conf"),
        udev_control: file.clone(),
        roots: nowhere(),
        frits: PathBuf::from("/nonexistent/vtium/frits"),
        frit_trust: font::trust(),
        systemd_root: PathBuf::from("/nonexistent/vtium/root"),
        batch: PathBuf::from("/nonexistent/vtium/batch.conf"),
        drawer: None,
    };
    let check = |args: &[&str]| {
        let args: Vec<OsString> = [SUBCOMMAND, "--no-devices"]
            .iter()
            .chain(args)
            .chain([&f.as_str()])
            .map(OsString::from)
            .collect();
        outcome(&config::load(&args, &mut config::read_bytes), &host)
    };
    // every line that names the bad name, so a second report of it in
    // any words, such as a failed read of that vtlook, counts
    let said = |out: &Outcome, name: &str| -> Vec<String> {
        out.stderr
            .iter()
            .filter(|l| l.contains(name))
            .cloned()
            .collect()
    };
    let out = check(&[]);
    assert_eq!(out.status, 1, "{:#?}", out.stderr);
    let reported = said(&out, "../x");
    assert_eq!(reported.len(), 1, "{:#?}", out.stderr);
    assert!(
        reported.iter().all(|l| l.starts_with(&format!("{f}:1: "))),
        "{reported:#?}"
    );
    let out = check(&["--vtlook=a/b"]);
    let reported = said(&out, "a/b");
    assert_eq!(reported.len(), 1, "{:#?}", out.stderr);
    assert!(
        reported.iter().all(|l| l.starts_with("argument 3: ")),
        "{reported:#?}"
    );
}

// check-config warns of a vtium.conf that the account of vtium's drawing
// process cannot read, such as a file of mode 0600, and says nothing of one
// of mode 0644, or where no drawing process runs as another account.
//
// References:
// - ADR-1400 D4, "vtium.conf under the split": check-config warns when the
//   drawer's account cannot read vtium.conf.
#[test]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
fn a_vtium_conf_the_drawing_process_cannot_read_is_warned_of() {
    use std::os::unix::fs::PermissionsExt;
    let me = rustix::process::geteuid().as_raw();
    let other = if me == 990 { 991 } else { 990 };
    let (_dir, file, _) = heads_fixture("drawer-reads", "version = 1\n", 0o600);
    let f = file.display().to_string();
    let host = |drawer: Option<Reader>| Host {
        conf_file: file.with_file_name("installed.conf"),
        udev_control: file.with_file_name("no-udev"),
        roots: nowhere(),
        frits: PathBuf::from("/nonexistent/vtium/frits"),
        frit_trust: font::trust(),
        systemd_root: PathBuf::from("/nonexistent/vtium/root"),
        batch: PathBuf::from("/nonexistent/vtium/batch.conf"),
        drawer,
    };
    let vtium = Reader {
        name: "vtium".to_owned(),
        uid: other,
        gid: other,
        groups: vec![other],
    };
    let warning = format!(
        "{f}: vtium's drawing process could not read this file once it is installed. The drawing process runs as the account vtium, and the file's mode 0600 does not let that account read it. Install the file with mode 644 so that every account can read it."
    );
    let out = outcome_on(&["--no-devices", f.as_str()], &host(Some(vtium.clone())));
    assert_eq!((out.status, out.stderr), (1, vec![warning]));
    // without a drawing process of its own account, the file is fine
    let out = outcome_on(&["--no-devices", f.as_str()], &host(None));
    assert_eq!((out.status, out.stderr), (0, Vec::new()));
    // readable by every account
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    let out = outcome_on(&["--no-devices", f.as_str()], &host(Some(vtium)));
    assert_eq!((out.status, out.stderr), (0, Vec::new()));
}
