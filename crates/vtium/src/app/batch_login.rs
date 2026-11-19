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
    let (_lp, mut app) = seat_with(&["--palette=batch-login"], None, fonts);
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
    assert!(!kept(&app), "the own cell is asked for again");
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

// Every session end gives the ended head's consoles, all of them, the
// palette the batch in force allows, whether or not that session end is
// the one that found the batch changed. Here heads a and b both draw the
// color scheme kiln, and head a has two consoles. Root activates kiln, a's
// session ends and finds the change, then b's ends and finds the batch as
// it was; both heads draw kiln. Root then deactivates kiln, and the same
// two session ends take it from both.
//
// References:
// - ADR-1205 D4, its note of 2026-11-19: an activation or a deactivation takes effect at the next login.
#[test]
fn every_session_end_gives_its_heads_consoles_the_palette_the_batch_allows() {
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
    let id = |app: &super::App, h: usize| app.heads.get(h).unwrap().console.id;
    // whether every console of head `h` and the head itself draw kiln
    let draws = |app: &super::App, h: usize| {
        let head = app.heads.get(h).unwrap();
        let mut all = head
            .consoles()
            .map(|c| c.term.terminal.palette().bg() == kiln);
        let first = all.next().unwrap();
        assert!(all.all(|c| c == first), "head {h}'s consoles agree");
        assert_eq!(
            head.palette().bg() == kiln,
            first,
            "head {h} and its consoles agree"
        );
        first
    };
    assert_eq!(app.heads.first().unwrap().console_count(), 2);
    assert!(!draws(&app, 0) && !draws(&app, 1));
    // root activates kiln
    write(&file, &format!("{}\n", scheme.display()));
    app.return_font(0, id(&app, 0));
    assert!(draws(&app, 0), "head a, whose session end found the change");
    app.return_font(1, id(&app, 1));
    assert!(
        draws(&app, 1),
        "head b, whose session end found the batch as it was"
    );
    // root deactivates kiln
    write(&file, "# nothing is active\n");
    app.return_font(1, id(&app, 1));
    assert!(
        !draws(&app, 1),
        "head b, whose session end found the change"
    );
    app.return_font(0, id(&app, 0));
    assert!(
        !draws(&app, 0),
        "head a, whose session end found the batch as it was"
    );
}
