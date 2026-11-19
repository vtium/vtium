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
// from, so a login whose batch no longer holds a font that was active
// drops every frit's stored glyphs, and the font helper draws the screen
// again from the fonts still active. A login whose batch only gains a
// font, or did not change, keeps the store.
#[test]
fn a_login_whose_batch_drops_a_font_drops_the_stored_glyphs() {
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
    // the first login finds the batch as it was
    app.return_font(0, id);
    assert!(holds(&app), "an unchanged batch keeps the glyphs");
    // root activates a second font, and the next login keeps the glyphs
    write(&file, &format!("{font}\n{more}\n"));
    app.return_font(0, id);
    assert!(
        holds(&app),
        "a batch that only gains a font keeps the glyphs"
    );
    // root deactivates the first font, and the next login drops them
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
