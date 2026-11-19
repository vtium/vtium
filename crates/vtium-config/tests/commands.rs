//! The commands that list, check and change the console's fonts and
//! settings (ADR-1201 D2), run as the binary in a temporary directory
//! standing in for `/etc/vtium` through the tool's path options, never as
//! root: each change a user makes is a draft, which `apply` installs.

#[path = "common/home.rs"]
mod home;

use std::path::{Path, PathBuf};

#[path = "common/console.rs"]
mod console;
use console::vtium;

use vtium_testscript as script;

const BIN: &str = env!("CARGO_BIN_EXE_vtium-config");

/// A configuration directory, a frits directory holding `tiny` and
/// `spare`, and a state directory for drafts, all the test's own.
struct Fixture {
    root: PathBuf,
}

fn fixture(name: &str, conf: Option<&str>) -> Fixture {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("commands")
        .join(name);
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("a fresh fixture");
    }
    let fonts = root.join("etc/fonts");
    std::fs::create_dir_all(&fonts).expect("the fixture");
    std::fs::create_dir_all(root.join("state")).expect("the state");
    std::fs::write(fonts.join("tiny.vtfrit"), "source=builtin:unscii-8\n").expect("a frit");
    std::fs::write(fonts.join("spare.vtfrit"), "source=builtin:unifont\n").expect("a frit");
    if let Some(conf) = conf {
        std::fs::write(root.join("etc/vtium.conf"), conf).expect("vtium.conf");
    }
    Fixture { root }
}

/// What a command printed: its status, its standard output and its
/// standard error.
struct Ran {
    status: i32,
    out: String,
    err: String,
}

impl Fixture {
    fn etc(&self) -> PathBuf {
        self.root.join("etc")
    }

    /// The tool's path options for the fixture.
    fn options(&self) -> Vec<std::ffi::OsString> {
        vec![
            "--configdir".into(),
            self.etc().into(),
            "--frits".into(),
            self.etc().join("fonts").into(),
            "--vtium".into(),
            vtium().into(),
        ]
    }

    /// `vtium-config` with the fixture's options and `args`, its drafts
    /// under the fixture's state directory.
    fn run(&self, args: &[&str]) -> Ran {
        let o = home::command(BIN)
            .args(self.options())
            .args(args)
            .env("XDG_STATE_HOME", self.root.join("state"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .output()
            .expect("the binary runs");
        Ran {
            status: o.status.code().unwrap_or(-1),
            out: String::from_utf8_lossy(&o.stdout).into_owned(),
            err: String::from_utf8_lossy(&o.stderr).into_owned(),
        }
    }

    /// `vtium-config` with the fixture's configuration directory and
    /// console but no `--frits`, run in `cwd`, its drafts under the
    /// fixture's state directory.
    fn run_bare(&self, cwd: &Path, args: &[&str]) -> Ran {
        let o = home::command(BIN)
            .arg("--configdir")
            .arg(self.etc())
            .arg("--vtium")
            .arg(vtium())
            .args(args)
            .current_dir(cwd)
            .env("XDG_STATE_HOME", self.root.join("state"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .output()
            .expect("the binary runs");
        Ran {
            status: o.status.code().unwrap_or(-1),
            out: String::from_utf8_lossy(&o.stdout).into_owned(),
            err: String::from_utf8_lossy(&o.stderr).into_owned(),
        }
    }

    /// The draft a change printed, after checking that nothing is
    /// installed yet, then `apply` run on it as the printed command runs it.
    fn apply_printed(&self, ran: &Ran) -> Ran {
        assert_eq!(ran.status, 0, "{}{}", ran.out, ran.err);
        let draft = ran
            .out
            .lines()
            .find_map(|l| l.strip_prefix("draft\t"))
            .expect("the draft's line");
        let run = ran
            .out
            .lines()
            .find_map(|l| l.strip_prefix("run\t"))
            .expect("the command's line");
        assert!(run.ends_with(&format!("apply {draft}")), "{run}");
        assert!(
            draft.starts_with(
                self.root
                    .join("state/vtium/drafts")
                    .to_str()
                    .expect("a path")
            ),
            "{draft}"
        );
        self.run(&["apply", draft])
    }
}

#[test]
fn remove_frit_removes_a_frit_but_not_the_one_font_name_names() {
    let f = fixture("remove", Some("font-name=tiny\n"));
    let fonts = f.etc().join("fonts");
    for (name, says) in [
        ("tiny", "font-name in vtium.conf uses it"),
        ("unifont", "built-in font"),
        ("nosuch", "nosuch.vtfrit"),
        ("../x", "there is no font set named"),
    ] {
        let ran = f.run(&["remove-frit", name]);
        assert_eq!(ran.status, 1, "{name}: {}", ran.out);
        assert!(ran.err.contains(says), "{name}: {}", ran.err);
    }
    // a look's font-name keeps its frit, named with its line
    let conf = f.etc().join("vtium.conf");
    let kept = std::fs::read_to_string(&conf).expect("vtium.conf");
    std::fs::write(
        &conf,
        format!("{kept}version = 1\n[look spare]\nfont-name = spare\n[head a]\nlook = spare\n"),
    )
    .expect("vtium.conf");
    let ran = f.run(&["remove-frit", "spare"]);
    assert_eq!(ran.status, 1, "{}", ran.out);
    assert!(ran.err.contains("`[look spare]`"), "{}", ran.err);
    std::fs::write(&conf, kept).expect("vtium.conf");
    let ran = f.run(&["remove-frit", "spare"]);
    assert!(
        ran.out.starts_with(&format!(
            "remove\t{}\n",
            fonts.join("spare.vtfrit").display()
        )),
        "{}",
        ran.out
    );
    assert!(
        fonts.join("spare.vtfrit").exists(),
        "a user's removal only drafts"
    );
    let applied = f.apply_printed(&ran);
    assert_eq!(applied.status, 0, "{}{}", applied.out, applied.err);
    assert!(!fonts.join("spare.vtfrit").exists());
    assert_eq!(
        std::fs::read_to_string(fonts.join("spare.vtfrit.bak")).expect("kept as .bak"),
        "source=builtin:unifont\n"
    );
    assert!(fonts.join("tiny.vtfrit").exists());
}

#[test]
fn new_makes_a_frit_of_fonts_and_installs_one_from_a_file() {
    let f = fixture("new", None);
    let fonts = f.etc().join("fonts");
    let ran = f.run(&[
        "new-frit",
        "builtin:unscii-8",
        "Some Family",
        "--name",
        "mine",
        "--cell",
        "8x16",
    ]);
    let applied = f.apply_printed(&ran);
    assert_eq!(applied.status, 0, "{}{}", applied.out, applied.err);
    let made = std::fs::read_to_string(fonts.join("mine.vtfrit")).expect("the new frit");
    assert!(made.contains("\nname=mine\n"), "{made}");
    assert!(
        made.contains("\nsource=builtin:unscii-8\nsource=font:Some Family\ncell=8x16\n"),
        "{made}"
    );
    // the template prints a frit that --from installs, named by its file
    let template = f.run(&["new-frit", "--template"]);
    assert_eq!(template.status, 0, "{}", template.err);
    let file = f.root.join("edited.vtfrit");
    std::fs::write(&file, &template.out).expect("the template");
    let from = f.run(&["new-frit", "--from", file.to_str().expect("a path")]);
    assert_eq!(f.apply_printed(&from).status, 0);
    assert_eq!(
        std::fs::read_to_string(fonts.join("edited.vtfrit")).expect("installed"),
        template.out
    );
    // a name the console keeps for a built-in font, a frit the reader
    // refuses and words new does not take are refused, and draft nothing
    for (args, status, says) in [
        (
            &["new-frit", "x", "--name", "unifont"][..],
            1,
            "cannot be a font set's name",
        ),
        (&["new-frit", "x", "--cell", "200x200"], 1, "cell="),
        (
            &["new-frit", "--from", "/nonexistent.vtfrit"],
            1,
            "nonexistent",
        ),
        (&["new-frit", "--frobnicate"], 2, "--frobnicate"),
        (&["new-frit", "x", "--from", "y"], 2, "takes"),
    ] {
        let ran = f.run(args);
        assert_eq!(ran.status, status, "{args:?}: {}", ran.out);
        assert!(ran.err.contains(says), "{args:?}: {}", ran.err);
        assert!(ran.out.is_empty(), "{args:?}: {}", ran.out);
    }
}

// new-frit --layer, once for each layer, writes the layers after the
// fonts in the order of their roles, script, symbols, emoji and fallback,
// whatever order the flags come in. An unknown layer, a layer named twice,
// a second layer of a role a set holds once, and layers that would take
// the set past its 16 sources are refused, and draft nothing.
//
// References:
// - ADR-1204 D5: new-frit --layer.
// - ADR-1204 D6: the roles' order, and one symbols, emoji and fallback
//   layer at most.
#[test]
fn new_frit_writes_its_layers_in_the_order_of_their_roles() {
    let f = fixture("new-layers", None);
    let fonts = f.etc().join("fonts");
    let layers = fonts.join("layers");
    std::fs::create_dir_all(&layers).expect("the layers' folder");
    for (name, role) in [
        ("unifont", "fallback"),
        ("noto-emoji", "emoji"),
        ("other-emoji", "emoji"),
        ("nerd-symbols", "symbols"),
        ("arabic", "script"),
        ("hebrew", "script"),
    ] {
        std::fs::write(
            layers.join(format!("{name}.vtlayer")),
            format!("role={role}\nsource=font:{name}\n"),
        )
        .expect("a layer");
    }
    let ran = f.run(&[
        "new-frit",
        "Iosevka Term",
        "--layer",
        "unifont",
        "--layer",
        "noto-emoji",
        "--layer",
        "hebrew",
        "--layer",
        "nerd-symbols",
        "--layer",
        "arabic",
        "--name",
        "mine",
    ]);
    let applied = f.apply_printed(&ran);
    assert_eq!(applied.status, 0, "{}{}", applied.out, applied.err);
    let made = std::fs::read_to_string(fonts.join("mine.vtfrit")).expect("the new frit");
    assert!(
        made.ends_with(
            "\nsource=font:Iosevka Term\nlayer=hebrew\nlayer=arabic\nlayer=nerd-symbols\nlayer=noto-emoji\nlayer=unifont\n"
        ),
        "{made}"
    );
    for (args, says) in [
        (
            &["new-frit", "x", "--layer", "gone"][..],
            "there is no layer gone",
        ),
        (
            &["new-frit", "x", "--layer", "arabic", "--layer", "arabic"],
            "--layer arabic is named twice",
        ),
        (
            &[
                "new-frit",
                "x",
                "--layer",
                "noto-emoji",
                "--layer",
                "other-emoji",
            ],
            "noto-emoji and other-emoji are both emoji layers",
        ),
    ] {
        let ran = f.run(args);
        assert_eq!(ran.status, 1, "{args:?}: {}", ran.out);
        assert!(ran.err.contains(says), "{args:?}: {}", ran.err);
        assert!(ran.out.is_empty(), "{args:?}: {}", ran.out);
    }
    let ran = f.run(&["new-frit", "--template", "--layer", "arabic"]);
    assert_eq!(ran.status, 2, "{}", ran.err);
    // three script layers of six sources each give the set 19 sources
    let six: String = (0..6).map(|i| format!("source=font:S{i}\n")).collect();
    for name in ["s1", "s2", "s3"] {
        std::fs::write(
            layers.join(format!("{name}.vtlayer")),
            format!("role=script\n{six}"),
        )
        .expect("a layer");
    }
    let args = [
        "new-frit", "x", "--layer", "s1", "--layer", "s2", "--layer", "s3",
    ];
    let ran = f.run(&args);
    assert_eq!(ran.status, 1, "{}", ran.out);
    assert!(
        ran.err.contains(
            "the layer s3 would give the font set 19 sources, and a font set holds 16 at most"
        ),
        "{}",
        ran.err
    );
    assert!(ran.out.is_empty(), "{}", ran.out);
}

// check FILE reads a frit's layers from the layers' folder of the frits'
// directory, wherever the frit being checked is.
//
// References:
// - ADR-1204 D5 and D9: the layers live in the frits' directory.
#[test]
fn check_reads_a_drafts_layers_from_the_frits_directory() {
    let f = fixture("check-layers", None);
    let layers = f.etc().join("fonts").join("layers");
    std::fs::create_dir_all(&layers).expect("the layers' folder");
    std::fs::write(
        layers.join("noto-emoji.vtlayer"),
        "role=emoji\nsource=font:Noto Color Emoji\n",
    )
    .expect("a layer");
    let draft = f.etc().join("draft.vtfrit");
    std::fs::write(&draft, "source=font:Text\nlayer=noto-emoji\n").expect("the draft");
    let ran = f.run(&["check", &draft.to_string_lossy()]);
    assert_eq!(ran.status, 0, "{}{}", ran.out, ran.err);
    assert!(!ran.out.contains("warning"), "{}", ran.out);
}

/// vtium.conf's `font-frits` names the frits' directory the console
/// reads, so with no `--frits` every command lists, writes and removes
/// frits there, and never in the build's default directory.
#[test]
fn font_frits_names_the_directory_every_command_acts_in() {
    let f = fixture("font-frits", None);
    let elsewhere = f.root.join("elsewhere");
    std::fs::write(
        f.etc().join("vtium.conf"),
        format!("font-frits={}\n", elsewhere.display()),
    )
    .expect("vtium.conf");
    every_command_acts_in(&f, &elsewhere);
}

// ADR-1403 D2: vtium-config reads vtium.conf alone, as the console does,
// so a seat file beside it is never read, whatever it sets
#[test]
fn a_seat_file_beside_vtium_conf_is_never_read() {
    let f = fixture("font-frits-seat", None);
    let elsewhere = f.root.join("elsewhere");
    let decoy = f.root.join("decoy");
    std::fs::create_dir_all(&decoy).expect("the seat file's directory");
    std::fs::write(decoy.join("decoy.vtfrit"), "source=builtin:unifont\n").expect("a frit");
    std::fs::write(
        f.etc().join("vtium.conf"),
        format!("font-frits={}\n", elsewhere.display()),
    )
    .expect("vtium.conf");
    std::fs::write(
        f.etc().join("seat0.seat.conf"),
        format!("font-frits={}\n", decoy.display()),
    )
    .expect("the seat's file");
    every_command_acts_in(&f, &elsewhere);
    assert!(decoy.join("decoy.vtfrit").is_file());
    assert!(!decoy.join("made.vtfrit").exists());
    // a relative font-frits in the seat file fails nothing
    std::fs::write(f.etc().join("seat0.seat.conf"), "font-frits=fonts\n").expect("the seat's file");
    let listed = home::command(BIN)
        .arg("--configdir")
        .arg(f.etc())
        .arg("fonts")
        .output()
        .expect("the binary runs");
    assert_eq!(
        listed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
}

/// `fonts`, `new`, `remove-frit` and `apply`, run with no `--frits`, list,
/// write and remove frits in `elsewhere`, the directory the configuration
/// files in `f` name, and leave the fixture's own frits alone.
fn every_command_acts_in(f: &Fixture, elsewhere: &Path) {
    std::fs::create_dir_all(elsewhere).expect("the frits' directory");
    std::fs::write(elsewhere.join("gone.vtfrit"), "source=builtin:unifont\n").expect("a frit");
    // the fixture's options less --frits
    let run = |args: &[&str]| {
        let o = home::command(BIN)
            .arg("--configdir")
            .arg(f.etc())
            .arg("--vtium")
            .arg(vtium())
            .args(args)
            .env("XDG_STATE_HOME", f.root.join("state"))
            .output()
            .expect("the binary runs");
        Ran {
            status: o.status.code().unwrap_or(-1),
            out: String::from_utf8_lossy(&o.stdout).into_owned(),
            err: String::from_utf8_lossy(&o.stderr).into_owned(),
        }
    };
    // apply, run as the drafting command printed it would be run, with no
    // --frits either
    let apply = |ran: &Ran| {
        assert_eq!(ran.status, 0, "{}{}", ran.out, ran.err);
        let line = |tag: &str| {
            ran.out
                .lines()
                .find_map(|l| l.strip_prefix(tag))
                .expect("the line")
                .to_owned()
        };
        assert!(
            line("run\t").contains(&format!("--frits {} ", elsewhere.display())),
            "{}",
            ran.out
        );
        run(&["apply", &line("draft\t")])
    };
    let listed = run(&["fonts"]);
    assert_eq!(listed.status, 0, "{}", listed.err);
    let names: Vec<&str> = listed
        .out
        .lines()
        .filter_map(|l| l.split('\t').next())
        .collect();
    assert_eq!(names, ["gone", "unifont", "unscii-8"], "{}", listed.out);
    let made = run(&["new-frit", "builtin:unscii-8", "--name", "made"]);
    assert!(
        made.out.starts_with(&format!(
            "write\t{}\n",
            elsewhere.join("made.vtfrit").display()
        )),
        "{}{}",
        made.out,
        made.err
    );
    assert_eq!(apply(&made).status, 0);
    assert!(elsewhere.join("made.vtfrit").is_file());
    let removed = run(&["remove-frit", "gone"]);
    assert!(
        removed.out.starts_with(&format!(
            "remove\t{}\n",
            elsewhere.join("gone.vtfrit").display()
        )),
        "{}{}",
        removed.out,
        removed.err
    );
    assert_eq!(apply(&removed).status, 0);
    assert!(!elsewhere.join("gone.vtfrit").exists());
    // the fixture's own frits, in the directory --frits would name, are
    // untouched
    assert!(f.etc().join("fonts/tiny.vtfrit").is_file());
    assert!(!f.etc().join("fonts/made.vtfrit").exists());
}

/// A PSF2 font of 128 glyphs 8 wide and `height` high, each glyph mapping
/// the codepoint of its number.
fn psf2(height: u32) -> Vec<u8> {
    let mut f = Vec::new();
    for v in [0x864a_b572u32, 0, 32, 1, 128, height, height, 8] {
        f.extend_from_slice(&v.to_le_bytes());
    }
    for g in 0..128 * height {
        f.push((g % 251) as u8);
    }
    for g in 0..128u8 {
        f.extend_from_slice(&[g, 0xff]);
    }
    f
}

// ADR-1202 D12: check notes a script-source= font whose height gives no
// superscript or subscript in the frit's own cell, naming its line, and
// says nothing of one that does
// covers: sgr.script
#[test]
fn check_notes_a_script_source_outside_the_range() {
    let f = fixture("check-script-source", None);
    let small = f.root.join("small.psf");
    let tiny = f.root.join("tiny.psf");
    std::fs::write(&small, psf2(8)).expect("a font");
    std::fs::write(&tiny, psf2(7)).expect("a font");
    let frit = f.root.join("t.vtfrit");
    std::fs::write(
        &frit,
        format!(
            "source=builtin:unifont\nscript-source=file:{}\nscript-source=file:{}\n",
            small.display(),
            tiny.display()
        ),
    )
    .expect("a frit");
    let ran = f.run(&["check", frit.to_str().expect("a path")]);
    assert_eq!(ran.status, 0, "{}", ran.err);
    let notes: Vec<&str> = ran
        .out
        .lines()
        .filter(|l| l.starts_with("note\t"))
        .collect();
    assert_eq!(notes.len(), 1, "{}", ran.out);
    assert!(
        notes[0].starts_with(&format!("note\t{}:3: script-source=file:", frit.display())),
        "{}",
        ran.out
    );
    assert!(
        notes[0].contains("8x16 character cell") && notes[0].contains("from 8 to 12 pixels"),
        "{}",
        ran.out
    );
    assert!(
        ran.out.ends_with(&format!("ok\t{}\n", frit.display())),
        "{}",
        ran.out
    );
}

/// `check FILE` exits 6, as `vtium check-config` does, when the only
/// warnings are about fonts that a font set needs and this machine lacks,
/// so a script reads the same status from both tools.
#[test]
fn check_passes_the_missing_fonts_status_through() {
    let f = fixture("check-missing-font", None);
    std::fs::write(
        f.etc().join("fonts").join("lacking.vtfrit"),
        "source=font:Vtium Test Font That No Machine Has\nsource=builtin:unifont\n",
    )
    .expect("a frit");
    // vtium check-config asks the font helper, this binary, which fonts a
    // look's font set lacks
    let conf = f.root.join("mine.conf");
    let text = format!(
        "font-helper={BIN}\nfont-frits={}\nversion = 1\n[look big]\nfont-name = lacking\n[head a]\nlook = big\n[rules]\ndisplay * -> a\ninput * -> a\n",
        f.etc().join("fonts").display()
    );
    std::fs::write(&conf, text).expect("a conf");
    let ran = f.run(&["check", conf.to_str().expect("a path")]);
    assert_eq!(ran.status, 6, "{}{}", ran.out, ran.err);
    assert!(
        ran.out.contains("Vtium Test Font That No Machine Has"),
        "{}",
        ran.out
    );
    assert!(!ran.out.contains("ok\t"), "{}", ran.out);
}

#[test]
fn check_judges_a_frit_and_a_vtium_conf_and_writes_nothing() {
    let f = fixture("check", None);
    let good = f.root.join("good.vtfrit");
    std::fs::write(&good, "source=builtin:unifont\ncompress=yes\n").expect("a frit");
    let ran = f.run(&["check", good.to_str().expect("a path")]);
    assert_eq!(ran.status, 0, "{}", ran.err);
    assert!(
        ran.out.contains("warning\t"),
        "compress= warns: {}",
        ran.out
    );
    assert!(
        ran.out.ends_with(&format!("ok\t{}\n", good.display())),
        "{}",
        ran.out
    );
    let bad = f.root.join("bad.vtfrit");
    std::fs::write(&bad, "source=builtin:unifont\ncolour=red\n").expect("a frit");
    let ran = f.run(&["check", bad.to_str().expect("a path")]);
    assert_eq!(ran.status, 1, "{}", ran.out);
    assert!(ran.err.contains("bad.vtfrit:2"), "{}", ran.err);
    // vtium.conf is judged by the console's own check, under its own name
    let conf = f.root.join("mine.conf");
    std::fs::write(&conf, "font-size=12\nfont-size=abc\n").expect("a conf");
    let ran = f.run(&["check", conf.to_str().expect("a path")]);
    assert_eq!(ran.status, 1, "{}{}", ran.out, ran.err);
    assert!(
        ran.out
            .contains(&format!("{}:2: font-size", conf.display())),
        "{}",
        ran.out
    );
    std::fs::write(&conf, "font-size=12\n").expect("a conf");
    let ran = f.run(&["check", conf.to_str().expect("a path")]);
    assert_eq!(ran.status, 0, "{}{}", ran.out, ran.err);
    assert_eq!(ran.out, format!("ok\t{}\n", conf.display()));
    assert!(!f.root.join("state/vtium").exists(), "check writes nothing");
}

/// A file holding only sections, as `heads.conf` did, is a vtium.conf like
/// any other, checked by the console's own `vtium check-config` as a
/// candidate, both its parts in one run (ADR-1403 D3): each error names
/// the file and its line, and a file that reads is ok.
#[test]
fn check_judges_a_file_of_sections_as_the_console_reads_it() {
    let f = fixture("check-heads", None);
    let dir = f.root.join("candidate");
    std::fs::create_dir_all(&dir).expect("the directory");
    let heads = dir.join("heads.conf");
    std::fs::write(
        &heads,
        "version = 1\n[head desk]\n[rules]\ndisplay * -> desk\ninput * -> desk\n",
    )
    .expect("a head file");
    let ran = f.run(&["check", heads.to_str().expect("a path")]);
    assert_eq!(ran.status, 0, "{}{}", ran.out, ran.err);
    assert_eq!(ran.out, format!("ok\t{}\n", heads.display()));
    std::fs::write(
        &heads,
        "version = 1\n[head desk]\nfrobnicate = 2\n[rules]\ndisplay * -> desk\n",
    )
    .expect("a head file");
    let ran = f.run(&["check", heads.to_str().expect("a path")]);
    assert_eq!(ran.status, 1, "{}{}", ran.out, ran.err);
    assert!(
        ran.out.contains(&format!("{}:3:", heads.display())),
        "{}{}",
        ran.out,
        ran.err
    );
    assert!(!ran.out.contains("ok\t"), "{}", ran.out);
    assert!(!f.root.join("state/vtium").exists(), "check writes nothing");
}

#[test]
fn fonts_lists_each_font_its_state_and_the_sources_this_machine_lacks() {
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped: {why}");
        return;
    }
    let f = fixture("fonts", None);
    let fonts = f.etc().join("fonts");
    std::fs::write(
        fonts.join("half.vtfrit"),
        "source=font:No Such Family Anywhere\nsource=builtin:unifont\n",
    )
    .expect("a frit");
    std::fs::write(
        fonts.join("none.vtfrit"),
        "source=font:No Such Family Anywhere\nversion=2.304\nurl=https://example.org/nsfa\n",
    )
    .expect("a frit");
    std::fs::write(fonts.join("broken.vtfrit"), "colour=red\n").expect("a frit");
    let ran = f.run(&["--font-dir", "/nonexistent", "fonts"]);
    assert_eq!(ran.status, 0, "{}", ran.err);
    let lines: Vec<&str> = ran.out.lines().collect();
    assert_eq!(lines.len(), 11, "{}", ran.out);
    assert!(
        lines[0].starts_with("broken\tcannot be read: "),
        "{}",
        lines[0]
    );
    assert_eq!(lines[1], "half\treads\tfont:No Such Family Anywhere");
    assert!(
        lines[2].starts_with("none\tno font file found: "),
        "{}",
        lines[2]
    );
    assert_eq!(lines[3], "spare\treads\t");
    assert_eq!(lines[4], "tiny\treads\t");
    assert_eq!(lines[5], "unifont\tbuilt in\t");
    assert_eq!(lines[6], "unscii-8\tbuilt in\t");
    // the report after the list names the font half needs
    assert_eq!(
        lines[7],
        "Font set half needs the font No Such Family Anywhere, and no font of that name is installed."
    );
    // a set with no font left names its fonts as a set with some left
    // does, with the version it asks for and the font's page
    assert_eq!(
        lines[8],
        "Font set none needs the font No Such Family Anywhere, version 2.304 or later, and no font of that name is installed."
    );
    assert_eq!(
        lines[9],
        "You can get the font from https://example.org/nsfa."
    );
    assert_eq!(
        lines[10],
        "To install a font that a font set needs, see https://vtium.github.io/vtium/fonts/#install-a-missing-font."
    );
}

/// A BDF font of one character, `A`, whose `FONT_VERSION` is 3.000.
const BDF_3_000: &str = "STARTFONT 2.1\n\
    FONT -test-\n\
    SIZE 6 75 75\n\
    FONTBOUNDINGBOX 8 6 0 -1\n\
    STARTPROPERTIES 4\n\
    FONT_VERSION \"3.000\"\n\
    FONT_ASCENT 5\n\
    FONT_DESCENT 1\n\
    CHARSET_REGISTRY \"ISO10646\"\n\
    ENDPROPERTIES\n\
    CHARS 1\n\
    STARTCHAR A\n\
    ENCODING 65\n\
    DWIDTH 8 0\n\
    BBX 3 5 1 0\n\
    BITMAP\n\
    40\nA0\nE0\nA0\nA0\n\
    ENDCHAR\n\
    ENDFONT\n";

/// After its list, `fonts` names each font a set needs that is not
/// installed, with the version the set asks for and the font's page; an
/// installed font of that name that lacks a character `check=` names; an
/// older version; and a font file with a different SHA-256. A last line
/// points to the help page on installing fonts, and no line of the report
/// holds a tab, so none reads as a font's line.
///
/// References:
/// - ADR-1204 D7: vtium-config's report of missing fonts and of fonts that
///   differ.
#[test]
fn fonts_names_each_missing_or_different_font_and_its_page() {
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped: {why}");
        return;
    }
    let f = fixture("fonts-needs", None);
    let fonts = f.etc().join("fonts");
    let (older, plain) = (f.root.join("older.bdf"), f.root.join("plain.bdf"));
    std::fs::write(&older, BDF_3_000).expect("a font");
    std::fs::write(&plain, BDF_3_000).expect("a font");
    std::fs::write(
        fonts.join("want.vtfrit"),
        format!(
            "source=font:No Such Family Anywhere\n\
             version=2.304\n\
             url=https://example.org/nsfa\n\
             source=bdf:{}\n\
             version=3.1\n\
             sha256={}\n\
             url=https://example.org/older\n\
             source=bdf:{}\n\
             check=U+0041\n\
             check=U+E0B0\n\
             source=builtin:unifont\n\
             source=bdf:/nonexistent/vtium-gone.bdf\n\
             version=1.0\n\
             layer=gone\n",
            older.display(),
            "0".repeat(64),
            plain.display()
        ),
    )
    .expect("a frit");
    // a layer's font is named with the version and page its layer gives
    std::fs::create_dir_all(fonts.join("layers")).expect("the layers' folder");
    std::fs::write(
        fonts.join("layers/gone.vtlayer"),
        "role=symbols\n\
         source=font:Gone Layer Nowhere\n\
         version=1.5\n\
         url=https://example.org/layer\n",
    )
    .expect("a layer");
    let ran = f.run(&["--font-dir", "/nonexistent", "fonts"]);
    assert_eq!(ran.status, 0, "{}", ran.err);
    let report: Vec<&str> = ran.out.lines().filter(|l| !l.contains('\t')).collect();
    assert_eq!(
        report,
        [
            "Font set want needs the font No Such Family Anywhere, version 2.304 or later, and no font of that name is installed.".to_owned(),
            "You can get the font from https://example.org/nsfa.".to_owned(),
            format!(
                "Font set want needs the font {}. The font file at that path lacks the character U+E0B0, so it is a different font, and vtium does not use it.",
                plain.display()
            ),
            // a font named by its path is a file, not an installed font
            "Font set want needs the font /nonexistent/vtium-gone.bdf, version 1.0 or later, and there is no file at that path.".to_owned(),
            "Font set want needs the font Gone Layer Nowhere, version 1.5 or later, and no font of that name is installed.".to_owned(),
            "You can get the font from https://example.org/layer.".to_owned(),
            format!(
                "Font set want asks for the font {}, version 3.1 or later, and the installed font is version 3.000. vtium still uses it.",
                older.display()
            ),
            "You can get the font from https://example.org/older.".to_owned(),
            format!(
                "The font file {} that font set want names has a different SHA-256 from the one the font set gives, so it may be another version of the font. vtium still uses it.",
                older.display()
            ),
            "You can get the font from https://example.org/older.".to_owned(),
            "To install a font that a font set needs, see https://vtium.github.io/vtium/fonts/#install-a-missing-font.".to_owned(),
        ]
    );
    // the font's own line still lists the sources left out
    assert!(
        ran.out.lines().any(|l| l
            == format!(
                "want\treads\tfont:No Such Family Anywhere, bdf:{}, bdf:/nonexistent/vtium-gone.bdf, font:Gone Layer Nowhere",
                plain.display()
            )),
        "{}",
        ran.out
    );
}

/// A font-frits that is no absolute path names no directory vtium-config
/// can act in: relative, it would be read against the directory the tool
/// runs in, and empty it names that directory itself. Every command that
/// lists, writes or removes frits fails, naming vtium.conf and the key,
/// and drafts nothing, while `config` still reads and sets the key so the
/// user can mend it. A vtium.conf that cannot be read fails them the same
/// way, and never falls back to the build's directory.
#[test]
fn a_font_frits_that_is_no_absolute_path_fails_every_frits_command() {
    for (name, conf) in [
        ("relative", &b"font-frits=fonts\n"[..]),
        ("empty", b"font-frits=\n"),
        ("unreadable", b"font-frits=/srv/\xff\n"),
    ] {
        let f = fixture(&format!("frits-{name}"), None);
        std::fs::write(f.etc().join("vtium.conf"), conf).expect("vtium.conf");
        let cwd = f.root.join("cwd");
        std::fs::create_dir_all(cwd.join("fonts")).expect("the directory");
        std::fs::write(cwd.join("fonts/tiny.vtfrit"), "source=builtin:unscii-8\n").expect("a frit");
        for args in [
            &["new-frit", "builtin:unscii-8", "--name", "made"][..],
            &["remove-frit", "tiny"],
            &["fonts"],
            &["preview", "tiny"],
            &["use-frit", "tiny"],
        ] {
            let ran = f.run_bare(&cwd, args);
            assert_eq!(ran.status, 1, "{name} {args:?}: {}{}", ran.out, ran.err);
            // preview says each font's failure on its own line of output
            let said = format!("{}{}", ran.out, ran.err);
            let names = if name == "unreadable" {
                "UTF-8"
            } else {
                "font-frits"
            };
            assert!(
                said.contains("vtium.conf") && said.contains(names),
                "{name} {args:?}: {said}"
            );
            assert!(
                args[0] == "preview" || ran.out.is_empty(),
                "{name} {args:?}: {}",
                ran.out
            );
        }
        assert!(!cwd.join("fonts/made.vtfrit").exists(), "{name}");
        assert!(!cwd.join("made.vtfrit").exists(), "{name}");
        assert!(
            !f.root.join("state/vtium").exists(),
            "{name}: nothing drafted"
        );
        if name != "unreadable" {
            // the draft takes the key without the frits' directory
            let set = f.run_bare(&cwd, &["set", "font-frits", "/srv/frits"]);
            assert_eq!(set.status, 0, "{name}: {}{}", set.out, set.err);
            assert!(set.out.starts_with("wrote "), "{name}: {}", set.out);
        }
    }
}

/// A frame of a kind that answers no sources query, as from a helper of
/// another version, is the helper's failure: it never shifts the next
/// frit's answer onto the frit before it.
#[test]
fn a_frame_of_an_unknown_kind_marks_every_frit_unknown() {
    let f = fixture("fonts-unknown-kind", None);
    let helper = f.root.join("other");
    // kind 99 with no payload, then a sources answer, kind 11, with none
    script::executable(
        &helper,
        "#!/bin/sh\nprintf '\\143\\000\\000\\000\\000\\000\\000\\000\\013\\000\\000\\000\\000\\000\\000\\000'\nexec sleep 30\n",
    );
    let ran = f.run(&["--helper", helper.to_str().expect("a path"), "fonts"]);
    assert_eq!(ran.status, 0, "{}", ran.err);
    let lines: Vec<&str> = ran.out.lines().collect();
    assert!(
        lines[0].starts_with("spare\treads\tunknown: ")
            && lines[0].contains("the font helper sent an unexpected answer"),
        "{}",
        ran.out
    );
    assert!(
        lines[1].starts_with("tiny\treads\tunknown: "),
        "{}",
        ran.out
    );
}

/// A helper that fails as it starts, as when it cannot set
/// no-new-privileges, sends one failed frame and exits before it reads a
/// request. That failure is the helper's, never the first frit's: every
/// frit is unknown, with the helper's reason, and none draws nothing.
#[test]
fn a_helper_that_fails_at_start_marks_every_frit_unknown() {
    let f = fixture("fonts-start-fails", None);
    let helper = f.root.join("fails");
    // kind 3, a failure, of 4 bytes, then the end
    script::executable(
        &helper,
        "#!/bin/sh\nprintf '\\003\\000\\000\\000\\004\\000\\000\\000boom'\n",
    );
    let ran = f.run(&["--helper", helper.to_str().expect("a path"), "fonts"]);
    assert_eq!(ran.status, 0, "{}", ran.err);
    assert!(!ran.out.contains("no font file found"), "{}", ran.out);
    let lines: Vec<&str> = ran.out.lines().collect();
    for (at, name) in [(0, "spare"), (1, "tiny")] {
        assert!(
            lines[at].starts_with(&format!("{name}\treads\tunknown: "))
                && lines[at].contains("boom"),
            "{}",
            ran.out
        );
    }
}

/// A helper that fails as it starts may close its end before this process
/// has written its requests, so a request finds the pipe closed. The
/// helper's failed frame is still the reason. Here the helper closes its
/// input before it writes, and three frits that together outgrow a pipe make
/// the requests outlast it, so the write fails every time.
#[test]
fn a_helper_that_fails_before_the_requests_are_written_gives_its_own_reason() {
    let f = fixture("fonts-start-fails-closed", None);
    let big = format!("source=builtin:unifont\n{}", "# padding\n".repeat(5_000));
    for name in ["big1", "big2", "big3"] {
        std::fs::write(f.etc().join(format!("fonts/{name}.vtfrit")), &big).expect("a large frit");
    }
    let helper = f.root.join("fails");
    script::executable(
        &helper,
        "#!/bin/sh\nexec 0<&-\nprintf '\\003\\000\\000\\000\\004\\000\\000\\000boom'\n",
    );
    let ran = f.run(&["--helper", helper.to_str().expect("a path"), "fonts"]);
    assert_eq!(ran.status, 0, "{}", ran.err);
    let lines: Vec<&str> = ran.out.lines().collect();
    for (at, name) in [
        (0, "big1"),
        (1, "big2"),
        (2, "big3"),
        (3, "spare"),
        (4, "tiny"),
    ] {
        assert!(
            lines[at].starts_with(&format!("{name}\treads\tunknown: "))
                && lines[at].contains("boom"),
            "{}",
            ran.out
        );
    }
}

/// A helper that never answers, or answers the first frit and then stalls,
/// marks no frit as drawing nothing: the answers that came are kept, and
/// each frit still unanswered after the helper's wait is unknown. Each
/// frit is asked of a helper of its own, so the stalling helper answers
/// its first start and stalls from its second.
#[test]
fn fonts_keeps_the_answers_that_came_and_marks_the_rest_unknown() {
    let f = fixture("fonts-stalled", None);
    for (name, script, first) in [
        ("silent", "exec sleep 30", "spare\treads\tunknown: "),
        (
            // one sources answer, kind 11 with no payload, at its first
            // start, then nothing at the next
            "one-then-stalls",
            "[ -e \"$0.ran\" ] && exec sleep 30\n: > \"$0.ran\"\nprintf '\\013\\000\\000\\000\\000\\000\\000\\000'\nexec sleep 30",
            "spare\treads\t\n",
        ),
    ] {
        let helper = f.root.join(name);
        script::executable(&helper, &format!("#!/bin/sh\n{script}\n"));
        let ran = f.run(&["--helper", helper.to_str().expect("a path"), "fonts"]);
        assert_eq!(ran.status, 0, "{name}: {}", ran.err);
        assert!(
            !ran.out.contains("no font file found"),
            "{name}: {}",
            ran.out
        );
        let lines: Vec<&str> = ran.out.lines().collect();
        assert!(
            format!("{}\n", lines[0]).starts_with(first),
            "{name}: {}",
            ran.out
        );
        assert!(
            lines[1].starts_with("tiny\treads\tunknown: the font helper did not answer in time"),
            "{name}: {}",
            ran.out
        );
    }
}

/// A font file of the test's own: `Test Mono` at `version`.
fn test_font(path: &Path, version: &str) {
    std::fs::create_dir_all(path.parent().expect("a folder")).expect("the font's folder");
    std::fs::write(
        path,
        vtium_testfont::ttf::test_mono().build_with_version(version),
    )
    .expect("the font");
}

/// "Install as a system font" copies a font of the user's into the system
/// fonts folder, `local-fonts` beside the fixture's vtium.conf in place
/// of `/usr/local/share/fonts`, through a draft that `apply` installs:
/// mode 0644, checked against the trust rule afterwards. The same file
/// again changes nothing, and nothing outside that folder is written.
#[test]
fn a_font_of_yours_installs_as_a_system_font_once() {
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped: {why}");
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    let f = fixture("system-font", None);
    let mine = f.root.join("home/.local/share/fonts/MyMono.ttf");
    test_font(&mine, "Version 1.0");
    let ran = f.run(&["fonts", "--install-system", mine.to_str().expect("a path")]);
    let target = f.etc().join("local-fonts/MyMono.ttf");
    assert!(
        ran.out.contains(&format!("write\t{}", target.display())),
        "{}{}",
        ran.out,
        ran.err
    );
    let applied = f.apply_printed(&ran);
    assert_eq!(applied.status, 0, "{}{}", applied.out, applied.err);
    assert_eq!(applied.out, format!("wrote\t{}\n", target.display()));
    assert_eq!(
        std::fs::read(&target).expect("the copy"),
        std::fs::read(&mine).expect("the font")
    );
    let mode = std::fs::metadata(&target)
        .expect("the copy")
        .permissions()
        .mode()
        & 0o7777;
    assert_eq!(mode, 0o644);
    // the same file a second time is already installed
    let again = f.run(&["fonts", "--install-system", mine.to_str().expect("a path")]);
    assert_eq!(again.status, 0, "{}{}", again.out, again.err);
    assert!(again.out.contains("already installed"), "{}", again.out);
    assert!(!again.out.contains("draft\t"), "{}", again.out);
}

/// A different file of the same name in the system fonts folder is shown
/// beside the person's, each with its size, date, family and version, and
/// replaced only when the person says so. The replaced file is not kept
/// in the fonts folder, where fontconfig would read it as one more font.
#[test]
fn a_clash_shows_both_files_and_replaces_only_when_asked() {
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped: {why}");
        return;
    }
    let f = fixture("system-font-clash", None);
    let installed = f.etc().join("local-fonts/MyMono.ttf");
    test_font(&installed, "Version 1.0");
    let mine = f.root.join("home/.fonts/MyMono.ttf");
    test_font(&mine, "Version 2.0");
    let path = mine.to_str().expect("a path");
    let ran = f.run(&["fonts", "--install-system", path]);
    assert_eq!(ran.status, 1, "{}{}", ran.out, ran.err);
    for said in [
        "A different file of this name is already installed.",
        "Installed: ",
        "Test Mono, version Version 1.0",
        "Yours: ",
        "Test Mono, version Version 2.0",
        "Nothing changed.",
    ] {
        assert!(ran.out.contains(said), "{said}: {}", ran.out);
    }
    assert!(!ran.out.contains("draft\t"), "{}", ran.out);
    let replaced = f.run(&["fonts", "--install-system", path, "--replace"]);
    let applied = f.apply_printed(&replaced);
    assert_eq!(applied.status, 0, "{}{}", applied.out, applied.err);
    assert_eq!(
        std::fs::read(&installed).expect("the copy"),
        std::fs::read(&mine).expect("the font")
    );
    let left: Vec<String> = std::fs::read_dir(installed.parent().expect("the folder"))
        .expect("the folder")
        .map(|e| {
            e.expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(left, ["MyMono.ttf"]);
}

/// A file the font helper cannot read as a font is never installed, and
/// a draft whose target changed after it was made is refused.
#[test]
fn a_file_that_is_no_font_or_a_target_changed_since_installs_nothing() {
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped: {why}");
        return;
    }
    let f = fixture("system-font-refused", None);
    let notes = f.root.join("home/.fonts/Notes.ttf");
    std::fs::create_dir_all(notes.parent().expect("a folder")).expect("the folder");
    std::fs::write(&notes, "just text").expect("the file");
    let ran = f.run(&["fonts", "--install-system", notes.to_str().expect("a path")]);
    assert_eq!(ran.status, 1, "{}{}", ran.out, ran.err);
    assert!(ran.err.contains("cannot read it as a font"), "{}", ran.err);
    assert!(!f.etc().join("local-fonts").exists());
    let mine = f.root.join("home/.fonts/MyMono.ttf");
    test_font(&mine, "Version 1.0");
    let ran = f.run(&["fonts", "--install-system", mine.to_str().expect("a path")]);
    // someone installs a file of that name before the draft is applied
    test_font(&f.etc().join("local-fonts/MyMono.ttf"), "Version 0.9");
    let applied = f.apply_printed(&ran);
    assert_eq!(applied.status, 1, "{}{}", applied.out, applied.err);
    assert!(
        applied.err.contains("changed after the draft was made"),
        "{}",
        applied.err
    );
}
