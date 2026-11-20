//! The batch read again at each new login: a resource root activates
//! between two logins reaches the second login's font helper and palette,
//! with no restart. The tests count logins, the session ends that start
//! the next login, and never wait on the clock.
//!
//! References:
//! - ADR-1205 D4, its note of 2026-11-19: the console reads the batch again at each new login, where it reads the font settings again.
#![expect(
    clippy::disallowed_methods,
    clippy::unwrap_used,
    reason = "a test's clock is the host's, and a test fails by panicking"
)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

use vtium_settings::Trust;
use vtium_settings::batch::Places;
use vtium_settings::cullet::Rule;
use vtium_term::Rgb;

use super::tests::{Still, no_fonts, seat_with, settle, shown_at};
use crate::font;
use crate::scratch::Scratch;

/// Writes `text` to `path`, readable by everyone and written by its owner
/// alone, as the trust rule asks.
fn write(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644)).unwrap();
}

// A color scheme and a font file that root activates between two logins
// draw from the second login on: the batch is read again when the first
// login's session ends, the head's palette for its next session is the
// newly active scheme, and the font helper's request names the newly
// active font. A login that finds the batch unchanged changes nothing.
#[test]
fn an_activation_between_two_logins_reaches_the_next_login() {
    let dir = Scratch::new("batch-login");
    let share = dir.join("share");
    for d in [&*dir, &share] {
        let _ = std::fs::create_dir_all(d);
        std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let scheme = share.join("batch-login.palette");
    write(&scheme, "background = #282a36\n");
    let font = PathBuf::from("/usr/share/fonts/batch-login.bdf");
    let file = dir.join("batch.conf");
    write(&file, "# nothing is active\n");
    let me = rustix::process::geteuid().as_raw();
    let fonts = font::Sources {
        trust: Trust::owner(me).below(&dir),
        ..no_fonts()
    };
    let t = Instant::now();
    // the stand-in helper, so that the console's launcher opens whether or
    // not this machine has vtium's font helper installed
    let helper = format!(
        "--font-helper={}",
        crate::helper::stand_in_shell().display()
    );
    let (_lp, mut app) = seat_with(&["--palette=batch-login", helper.as_str()], None, fonts);
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    app.set_batch_places(
        Places {
            file: file.clone(),
            vconsole_conf: PathBuf::from("/dev/null"),
            cmdline: PathBuf::from("/dev/null"),
            palettes: share.clone(),
            ..Places::system()
        },
        Rule::new(Trust::owner(me)).below(&dir),
    );
    shown_at(&mut app, t);
    let kiln = Rgb::new(0x28, 0x2a, 0x36);
    let id = app.heads.first().unwrap().console.id;
    let names = |app: &super::App, path: &Path| {
        let request = app.active_fonts_request().unwrap_or_default();
        let path = path.as_os_str().as_encoded_bytes();
        request.windows(path.len()).any(|w| w == path)
    };
    // the first login's session ends with nothing activated
    app.return_font(0, id);
    assert_ne!(app.heads.first().unwrap().palette().bg(), kiln);
    assert!(!names(&app, &font));
    // root activates the scheme and the font, and the next login takes them
    write(
        &file,
        &format!("{}\n{}\n", scheme.display(), font.display()),
    );
    app.return_font(0, id);
    assert_eq!(
        app.heads.first().unwrap().palette().bg(),
        kiln,
        "the head's palette"
    );
    assert_eq!(
        app.heads
            .first()
            .unwrap()
            .console
            .term
            .terminal
            .palette()
            .bg(),
        kiln,
        "the next session's palette"
    );
    assert!(names(&app, &font), "the font helper's active fonts");
    assert!(app.gate().holds(&scheme));
    // a login that finds the batch as it was keeps it
    let gate = app.gate();
    app.return_font(0, id);
    assert!(std::sync::Arc::ptr_eq(&gate, &app.gate()));
}

/// A set at `cell` whose every glyph in `chars` is solid: a stand-in for
/// glyphs the font helper drew from a font of the batch.
fn solid(cell: vtium_font::Cell, chars: &[char]) -> std::sync::Arc<vtium_font::set::Set> {
    use vtium_font::set::{PixelFormat, Set, SetBuilder, StrikeBuilder};
    let mut s = StrikeBuilder::new(PixelFormat::R8, cell, vtium_font::Style::default());
    let size = usize::try_from(cell.width.checked_mul(cell.height).unwrap()).unwrap();
    let g = s.glyph(vtium_font::Width::Narrow, vec![255; size]);
    for &c in chars {
        s.map(c, g);
    }
    std::sync::Arc::new(Set::from_bytes(SetBuilder::new("t").strike(s).write().unwrap()).unwrap())
}

// The console's glyph store keeps no record of the font each glyph came
// from, so a login whose batch changed drops every frit's stored glyphs,
// and the font helper draws the screen again from the fonts now active.
// A font the batch gains may draw characters that another font drew or
// that no font had, and a font it loses may have drawn any of them. The
// frit's own cell is asked for again too, because a newly active first
// source can set it. A login that finds the batch unchanged keeps both.
#[test]
fn a_login_whose_batch_changed_drops_the_stored_glyphs() {
    let dir = Scratch::new("batch-glyphs");
    std::fs::set_permissions(&*dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(dir.join("t.vtfrit"), "source=builtin:unifont\n").unwrap();
    let file = dir.join("batch.conf");
    let font = "/usr/share/fonts/batch-glyphs.bdf";
    let more = "/usr/share/fonts/batch-glyphs-more.bdf";
    write(&file, &format!("{font}\n"));
    let me = rustix::process::geteuid().as_raw();
    let fonts = font::Sources {
        frits: dir.to_path_buf(),
        trust: Trust::owner(me).below(&dir),
        helper: Some(crate::helper::Command {
            program: crate::helper::stand_in_shell().to_path_buf(),
            args: vec!["-c".into(), "exec sleep 30".into()],
            account: None,
        }),
        ..no_fonts()
    };
    let t = Instant::now();
    let (_lp, mut app) = seat_with(&["--font-name=t"], None, fonts);
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    app.set_batch_places(
        Places {
            file: file.clone(),
            vconsole_conf: PathBuf::from("/dev/null"),
            cmdline: PathBuf::from("/dev/null"),
            ..Places::system()
        },
        Rule::new(Trust::owner(me)).below(&dir),
    );
    shown_at(&mut app, t);
    let id = app.heads.first().unwrap().console.id;
    let at = *app.frits.get("t").unwrap().asked.first().unwrap();
    // a glyph the helper drew from a font of the batch
    {
        let frit = app.frits.get("t").unwrap();
        let mut store = crate::deferred::lock(&frit.generated);
        let fresh = store
            .fresh(
                (at, vtium_font::Variant::default()),
                solid(at.cell, &['A']),
                &[(0x41, 1)],
                1,
            )
            .unwrap();
        assert!(store.add(fresh).is_some());
    }
    let holds =
        |app: &super::App| crate::deferred::lock(&app.frits.get("t").unwrap().generated).holds(at);
    assert!(holds(&app));
    // an own cell the helper gave under the first batch
    let planted = Ok(crate::helper::OwnCell::Bitmap(vtium_font::Cell {
        width: 9,
        height: 18,
    }));
    let plant = |app: &mut super::App| {
        let frit = app.frits.get_mut("t").unwrap();
        assert!(!frit.own.is_empty(), "the frit's own cell was asked for");
        for answer in frit.own.values_mut() {
            *answer = planted.clone();
        }
    };
    let kept = |app: &super::App| {
        let frit = app.frits.get("t").unwrap();
        frit.own.values().any(|answer| *answer == planted)
    };
    plant(&mut app);
    // the first login finds the batch as it was
    app.return_font(0, id);
    assert!(holds(&app), "an unchanged batch keeps the glyphs");
    assert!(kept(&app), "an unchanged batch keeps the own cell");
    // root activates a second font, and the next login drops the glyphs
    write(&file, &format!("{font}\n{more}\n"));
    app.return_font(0, id);
    assert!(
        !holds(&app),
        "a font the batch gains draws the characters it has"
    );
    assert!(
        !kept(&app),
        "the own cell the first batch gave is forgotten"
    );
    assert!(
        !app.frits.get("t").unwrap().own.is_empty(),
        "the head's font is resolved again, and asks for its own cell"
    );
    assert_eq!(
        app.frits.get("t").unwrap().asked,
        [at],
        "the screen's glyphs are asked for again"
    );
    // root deactivates the first font, and the next login drops them
    let fresh = |app: &super::App| {
        let frit = app.frits.get("t").unwrap();
        let mut store = crate::deferred::lock(&frit.generated);
        let fresh = store
            .fresh(
                (at, vtium_font::Variant::default()),
                solid(at.cell, &['A']),
                &[(0x41, 1)],
                1,
            )
            .unwrap();
        assert!(store.add(fresh).is_some());
    };
    fresh(&app);
    assert!(holds(&app));
    write(&file, &format!("{more}\n"));
    app.return_font(0, id);
    assert!(
        !holds(&app),
        "a font the batch no longer holds draws no glyph"
    );
    assert_eq!(
        app.frits.get("t").unwrap().asked,
        [at],
        "the screen's glyphs are asked for again"
    );
}

// A pointer file follows its place in the batch from login to login: a
// pointer file the batch does not list is not read, the login after root
// activates it loads it, and the login after root deactivates it drops its
// pictures, so the pointer draws vtium's own again.
#[test]
fn a_pointer_file_follows_its_place_in_the_batch_from_login_to_login() {
    let dir = Scratch::new("batch-pointer");
    std::fs::set_permissions(&*dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let svg = dir.join("pointer.svg");
    write(
        &svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"8\" height=\"8\"/>\n",
    );
    let file = dir.join("batch.conf");
    write(&file, "# nothing is active\n");
    let me = rustix::process::geteuid().as_raw();
    let t = Instant::now();
    let theme = format!("--pointer-theme={}", svg.display());
    let fonts = font::Sources {
        trust: Trust::owner(me).below(&dir),
        ..no_fonts()
    };
    let (mut lp, mut app) = seat_with(&[theme.as_str()], None, fonts);
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    app.set_batch_places(
        Places {
            file: file.clone(),
            vconsole_conf: PathBuf::from("/dev/null"),
            cmdline: PathBuf::from("/dev/null"),
            ..Places::system()
        },
        Rule::new(Trust::owner(me)).below(&dir),
    );
    shown_at(&mut app, t);
    settle(&mut lp, &mut app, t);
    let id = app.heads.first().unwrap().console.id;
    assert!(
        app.pointer_store.asked().is_empty(),
        "a pointer file the batch does not list is not read"
    );
    // root activates the pointer file, and the next login loads it
    write(&file, &format!("{}\n", svg.display()));
    app.return_font(0, id);
    // a turn loads the pointer file, and the display's next frame asks it
    app.turn(t);
    settle(&mut lp, &mut app, t);
    assert_eq!(
        app.pointer_store.asked().len(),
        1,
        "the display asks the loaded file for its height"
    );
    // root deactivates it, and the next login drops it
    write(&file, "# nothing is active\n");
    app.return_font(0, id);
    assert!(app.pointer_store.current().is_none());
    app.turn(t);
    settle(&mut lp, &mut app, t);
    assert!(
        app.pointer_store.asked().is_empty(),
        "a deactivated pointer file is not read again"
    );
}

// A login whose batch changed ends the running font helper, so the helper
// that draws the screen again reads the new active fonts before any ask,
// and no glyph comes through a font root just deactivated. Each stand-in
// writes what it reads to a file of its own, named by its process id.
//
// References:
// - ADR-1205 D4, its note of 2026-11-19: a deactivation takes effect at the next login.
#[test]
fn the_helper_after_a_login_reads_the_new_active_fonts_before_any_ask() {
    let dir = Scratch::new("batch-order");
    std::fs::set_permissions(&*dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(dir.join("t.vtfrit"), "source=builtin:unifont\n").unwrap();
    let read = dir.join("read");
    std::fs::create_dir_all(&read).unwrap();
    let file = dir.join("batch.conf");
    let (font, more) = (
        "/usr/share/fonts/batch-order-1.bdf",
        "/usr/share/fonts/batch-order-2.bdf",
    );
    write(&file, &format!("{font}\n{more}\n"));
    let me = rustix::process::geteuid().as_raw();
    let fonts = font::Sources {
        frits: dir.to_path_buf(),
        trust: Trust::owner(me).below(&dir),
        helper: Some(crate::helper::Command {
            program: crate::helper::stand_in_shell().to_path_buf(),
            args: vec![
                "-c".into(),
                "exec cat >\"$1/$$\"".into(),
                "stand-in".into(),
                read.clone().into(),
            ],
            account: None,
        }),
        ..no_fonts()
    };
    let t = Instant::now();
    let (mut lp, mut app) = seat_with(&["--font-name=t"], None, fonts);
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    app.set_batch_places(
        Places {
            file: file.clone(),
            vconsole_conf: PathBuf::from("/dev/null"),
            cmdline: PathBuf::from("/dev/null"),
            ..Places::system()
        },
        Rule::new(Trust::owner(me)).below(&dir),
    );
    shown_at(&mut app, t);
    let id = app.heads.first().unwrap().console.id;
    assert!(!app.frits.get("t").unwrap().asked.is_empty());
    // root deactivates the first font, and the next login draws again
    write(&file, &format!("{more}\n"));
    app.return_font(0, id);
    let after = app.active_fonts_request().unwrap();
    // a helper that read the new active fonts first, then an ask
    let first = |read: &Path| {
        std::fs::read_dir(read).unwrap().flatten().any(|e| {
            let bytes = std::fs::read(e.path()).unwrap_or_default();
            bytes.starts_with(&after) && bytes.len() > after.len()
        })
    };
    for _ in 0..1000 {
        if first(&read) {
            break;
        }
        lp.dispatch(Some(std::time::Duration::from_millis(10)), &mut app)
            .unwrap();
    }
    assert!(
        first(&read),
        "a font helper reads the new active fonts before its first ask"
    );
}

// A batch change reaches each console whose session is not running at
// once, and each running session at its next login. Here heads a and b
// both draw the color scheme kiln, and head a has two consoles, whose
// hidden one runs a program that set color 1 with OSC 4. Root activates
// kiln, and a's shown session ends: that console draws kiln, while a's
// hidden console and b's console, both still running, keep the colors they
// draw. Their color resets return to kiln. b's session then ends and finds
// the batch as it was, and b's console draws kiln. Root deactivates kiln,
// and b's next session end takes it from b's console, and from a's shown
// console, which runs no session since its own ended.
//
// References:
// - ADR-1205 D4, its note of 2026-11-19: an activation or a deactivation takes effect at the next login.
#[test]
fn a_batch_change_reaches_a_running_session_at_its_next_login() {
    let dir = Scratch::new("batch-heads");
    let share = dir.join("share");
    for d in [&*dir, &share] {
        let _ = std::fs::create_dir_all(d);
        std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let scheme = share.join("batch-heads.palette");
    write(&scheme, "background = #282a36\n");
    let file = dir.join("batch.conf");
    write(&file, "# nothing is active\n");
    let me = rustix::process::geteuid().as_raw();
    let fonts = font::Sources {
        trust: Trust::owner(me).below(&dir),
        ..no_fonts()
    };
    let heads = "version = 1\n[head a]\n[head b]\n[rules]\ndisplay connector:mem/A -> a\ndisplay connector:mem/B -> b\n";
    let t = Instant::now();
    let (_lp, mut app) = seat_with(&["--palette=batch-heads"], Some(heads), fonts);
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A"), (2, "B")])),
    );
    app.set_batch_places(
        Places {
            file: file.clone(),
            vconsole_conf: PathBuf::from("/dev/null"),
            cmdline: PathBuf::from("/dev/null"),
            palettes: share.clone(),
            ..Places::system()
        },
        Rule::new(Trust::owner(me)).below(&dir),
    );
    shown_at(&mut app, t);
    assert!(app.open_console(0, t), "head a's second console");
    let kiln = Rgb::new(0x28, 0x2a, 0x36);
    let one = Rgb::new(1, 1, 1);
    let shown = |app: &super::App, h: usize| app.heads.get(h).unwrap().console.id;
    let a_shown = shown(&app, 0);
    let a_hidden = app
        .heads
        .first()
        .unwrap()
        .consoles()
        .map(|c| c.id)
        .find(|&c| c != a_shown)
        .unwrap();
    let bg = |app: &mut super::App, h: usize, id: u64| terminal(app, h, id).palette().bg();
    let head_kiln = |app: &super::App, h: usize| app.heads.get(h).unwrap().palette().bg() == kiln;
    // every console runs a session; /bin/true exits at once, but the loop
    // never runs, so vtium never learns of the exit
    let idle: Vec<(usize, u64)> = app
        .heads
        .iter()
        .enumerate()
        .flat_map(|(h, head)| {
            head.consoles()
                .filter(|c| !c.running())
                .map(move |c| (h, c.id))
        })
        .collect();
    for (h, id) in idle {
        app.spawn(h, id);
    }
    assert!(
        app.heads
            .iter()
            .all(|head| head.consoles().all(|c| c.running()))
    );
    terminal(&mut app, 0, a_hidden).input(b"\x1b]4;1;#010101\x07");
    let before = bg(&mut app, 0, a_hidden);
    assert_ne!(before, kiln);
    // root activates kiln, and a's shown session ends
    write(&file, &format!("{}\n", scheme.display()));
    end_session(&mut app, 0, a_shown);
    assert!(head_kiln(&app, 0) && head_kiln(&app, 1));
    assert_eq!(
        bg(&mut app, 0, a_shown),
        kiln,
        "a's shown console, whose session ended"
    );
    let b = shown(&app, 1);
    for (h, id, name) in [(0, a_hidden, "a's hidden console"), (1, b, "b's console")] {
        assert_eq!(
            bg(&mut app, h, id),
            before,
            "{name}, whose session still runs"
        );
    }
    assert_eq!(terminal(&mut app, 0, a_hidden).palette().color(1), one);
    terminal(&mut app, 0, a_hidden).input(b"\x1b]111\x07");
    assert_eq!(
        bg(&mut app, 0, a_hidden),
        kiln,
        "a color reset returns to kiln"
    );
    assert_eq!(terminal(&mut app, 0, a_hidden).palette().color(1), one);
    // b's session ends and finds the batch as it was
    end_session(&mut app, 1, b);
    assert_eq!(bg(&mut app, 1, b), kiln, "b's console, whose session ended");
    // root deactivates kiln, and b's next session end finds the change
    write(&file, "# nothing is active\n");
    end_session(&mut app, 1, b);
    assert!(!head_kiln(&app, 0) && !head_kiln(&app, 1));
    assert_ne!(bg(&mut app, 1, b), kiln, "b's console, whose session ended");
    assert_ne!(
        bg(&mut app, 0, a_shown),
        kiln,
        "a's shown console, which runs no session"
    );
    assert_eq!(
        bg(&mut app, 0, a_hidden),
        kiln,
        "a's hidden console, whose session still runs"
    );
}

/// The terminal of head `h`'s console `id`.
fn terminal(app: &mut super::App, h: usize, id: u64) -> &mut vtium_term::Terminal {
    &mut app
        .heads
        .get_mut(h)
        .unwrap()
        .console_mut(id)
        .unwrap()
        .term
        .terminal
}

/// Ends the session of head `h`'s console `id` as a program's exit does:
/// the console takes its hard reset, and the next login reads the batch
/// again. No new session starts, so the console runs none afterwards.
fn end_session(app: &mut super::App, h: usize, id: u64) {
    app.heads
        .get_mut(h)
        .unwrap()
        .exited(id, std::time::Duration::from_secs(60), false)
        .unwrap();
    app.return_font(h, id);
}

// A `.cube` file that a head's vtlook names follows its place in the batch
// from login to login, as every resource does: the head draws its vtlook
// with the file's color map from the login after root activates the file,
// and draws the built-in vtlook from the login after root deactivates it.
//
// References:
// - ADR-1205 D4, its note of 2026-11-19: every resource passes one gate, and an activation or a deactivation takes effect at the next login.
#[cfg(feature = "filters")]
#[test]
fn a_cube_file_follows_its_place_in_the_batch_from_login_to_login() {
    let dir = Scratch::new("batch-cube");
    std::fs::set_permissions(&*dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let cube = dir.join("warm.cube");
    write(
        &cube,
        "LUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 0.95 0\n1 0.95 0\n0 0 0.85\n1 0 0.85\n0 0.95 0.85\n1 0.95 0.85\n",
    );
    write(
        &dir.join("film.vtlook"),
        "version = 1\n[color cube]\nfile = warm.cube\n",
    );
    let file = dir.join("batch.conf");
    write(&file, "# nothing is active\n");
    let me = rustix::process::geteuid().as_raw();
    crate::head::look::read_from(Some((dir.to_path_buf(), Trust::owner(me).below(&dir))));
    let heads = "version = 1\n[head a]\n[rules]\ndisplay connector:mem/A -> a\n";
    let t = Instant::now();
    let fonts = font::Sources {
        trust: Trust::owner(me).below(&dir),
        ..no_fonts()
    };
    let (_lp, mut app) = seat_with(&["--vtlook=film"], Some(heads), fonts);
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Still::new(&[(1, "A")])),
    );
    app.set_batch_places(
        Places {
            file: file.clone(),
            vconsole_conf: PathBuf::from("/dev/null"),
            cmdline: PathBuf::from("/dev/null"),
            ..Places::system()
        },
        Rule::new(Trust::owner(me)).below(&dir),
    );
    shown_at(&mut app, t);
    let id = app.heads.first().unwrap().console.id;
    let mapped = |app: &super::App| {
        let head = app.heads.first().unwrap();
        let views: Vec<bool> = head.views.values().map(|v| v.colors().is_some()).collect();
        assert_eq!(views.len(), 1);
        (head.vtlook.table.is_some(), views.first().copied().unwrap())
    };
    assert_eq!(mapped(&app), (false, false), "an inactive .cube file");
    // root activates the file, and the next login draws its color map
    write(&file, &format!("{}\n", cube.display()));
    app.return_font(0, id);
    assert_eq!(mapped(&app), (true, true), "the file the batch gains");
    // root deactivates it, and the next login draws without it
    write(&file, "# nothing is active\n");
    app.return_font(0, id);
    assert_eq!(mapped(&app), (false, false), "the file the batch drops");
    crate::head::look::read_from(None);
}
