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

use super::tests::{Still, no_fonts, seat_with, shown_at};
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
