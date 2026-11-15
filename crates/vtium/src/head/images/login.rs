//! The login picture: one picture vtium.conf names with `login-image`,
//! placed each time a login prompt starts in one of the head's consoles,
//! as a background beneath the text or as a banner above the prompt.
//!
//! The console reads the file as root, read-only and at most
//! [`LOGIN_IMAGE_FILE_MAX`] bytes, and hands its bytes to the image helper
//! in a sealed memory file. The helper decodes them as the account that
//! runs before login, so no picture decoder runs as root and no logged-in
//! user's account decodes a file root's configuration named. The console
//! then fits the decoded picture to the form the keys give, once, through
//! vtium-image's [`make`], and keeps only the pixels the screen shows.
//!
//! The head asks for the picture once its first display is set up, at
//! that display's size, and its first login prompt waits until the helper
//! has answered, has failed, or has run out [`REPLY_TIMEOUT`]. A later
//! login prompt reuses the picture. When the display changes size, the
//! head asks again, so the next login prompt gets a picture of the new
//! size.
//!
//! Placing the picture is a host event before the session's program
//! starts: an ordinary kitty placement with no image id at the lowest z,
//! in the console's store. From then on it scrolls with its lines, a
//! screen clear or a kitty delete removes it, and no program can name it
//! to read it back.
//!
//! A file vtium cannot use starts the login prompt without the picture,
//! and vtium logs one line that names the file and the reason, once each
//! time it reads its configuration.
//!
//! References:
//! - ADR-1100 D17: the two forms, when the picture is placed, who reads
//!   and who decodes the file, its bounds, its budget and the log line.
//! - ADR-1101 D8: the reply timer the first login prompt waits for.

use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use vtium_image::Pixels;
use vtium_image::decode::{Medium, Reply, Request};
use vtium_image::login::{Fit, Fitting, Form, Geometry, Made, Position, place};
use vtium_platform::calloop::LoopHandle;
use vtium_platform::image_helper::Session;

use super::{Helper, Helpers, REPLY_TIMEOUT, Watch};
use crate::app::App;
use crate::config::{Config, keys};
use crate::term::Term;

/// The most bytes of a login picture's file the console reads: 64 MiB.
pub const LOGIN_IMAGE_FILE_MAX: u64 = 64 << 20;

/// The most bytes a decoded login picture may take, as RGBA: 256 MiB,
/// which a picture of 8192×8192 fills.
pub const LOGIN_IMAGE_PIXELS_MAX: u64 = 256 << 20;

/// Where the login picture of a head stands.
enum State {
    /// Not asked for yet, or asked again after the display changed size.
    Idle,
    /// The request is out with the image helper, and fails unanswered at
    /// `due`.
    Asking {
        request: Request,
        due: Instant,
        geometry: Geometry,
    },
    /// Made for `geometry`.
    Ready { geometry: Geometry, made: Rc<Made> },
    /// No picture: none is configured, or vtium cannot use the file and
    /// has logged why.
    Off,
}

/// A head's login picture.
pub(crate) struct Login {
    path: Option<PathBuf>,
    form: Form,
    /// `max-graphic-size`, the largest decoded picture each side.
    largest: (u32, u32),
    state: State,
    host: Option<Box<dyn Helper>>,
    watch: Option<Watch>,
    /// The log has said once that a console's store could not hold the
    /// picture.
    no_room_logged: bool,
    /// The turn that ended the first login prompt's wait, so the loop
    /// comes round once more and starts it.
    settled: Option<Instant>,
}

impl Login {
    /// The login picture `config`'s keys name, a look's or the top
    /// level's, with `largest` the decoded picture's bound each side.
    pub(crate) fn read(config: &Config, largest: (u32, u32)) -> Login {
        let path = config.text(&keys::LOGIN_IMAGE).map(PathBuf::from);
        let form = Form {
            fit: config
                .text(&keys::LOGIN_IMAGE_FIT)
                .and_then(Fit::parse)
                .unwrap_or(Fit::Contain),
            rows: config
                .number(&keys::LOGIN_IMAGE_ROWS)
                .and_then(|n| u32::try_from(n).ok()),
            position: config
                .text(&keys::LOGIN_IMAGE_POSITION)
                .and_then(Position::parse)
                .unwrap_or(Position::Left),
        };
        let state = if path.is_some() {
            State::Idle
        } else {
            State::Off
        };
        Login {
            path,
            form,
            largest,
            state,
            host: None,
            watch: None,
            no_room_logged: false,
            settled: None,
        }
    }

    /// Whether the head's first login prompt waits for the picture: while
    /// it has not been asked for, or its request is out.
    pub(crate) fn waits(&self) -> bool {
        matches!(self.state, State::Idle | State::Asking { .. })
    }

    /// When the loop next has to turn for the picture: when the request
    /// out fails unanswered, or at once after the turn that ended the
    /// first login prompt's wait.
    pub(crate) fn due(&self) -> Option<Instant> {
        match self.state {
            State::Asking { due, .. } => Some(due),
            _ => self.settled,
        }
    }

    /// The picture made, while there is one.
    pub(crate) fn made(&self) -> Option<&Made> {
        match &self.state {
            State::Ready { made, .. } => Some(made),
            _ => None,
        }
    }

    /// The login picture's turn at `now`, for a head whose first display
    /// gives `geometry`, or `None` while it has no display: the request
    /// sent to a host `helpers` makes once, its reply read, or the request
    /// dropped once the timer has run out. A head asks again when its
    /// display's geometry changed since the picture was made.
    pub(crate) fn turn(
        &mut self,
        geometry: Option<Geometry>,
        now: Instant,
        helpers: Option<&Helpers>,
        handle: &LoopHandle<'static, App>,
    ) {
        if let Some(fd) = self.step(geometry, now, helpers) {
            self.watch = Watch::new(fd, handle);
        }
    }

    /// [`turn`](Self::turn) without the loop: a descriptor for the loop to
    /// watch when a request has just gone out.
    pub(crate) fn step(
        &mut self,
        geometry: Option<Geometry>,
        now: Instant,
        helpers: Option<&Helpers>,
    ) -> Option<OwnedFd> {
        self.settled = None;
        let geometry = geometry?;
        let waited = self.waits();
        let mut wake = None;
        match &self.state {
            State::Off => {}
            State::Idle => wake = self.ask(geometry, now, helpers),
            State::Ready { geometry: g, .. } if *g != geometry => {
                wake = self.ask(geometry, now, helpers);
            }
            State::Ready { .. } => {}
            State::Asking { due, .. } => {
                if let Some(reply) = self.host.as_mut().and_then(|h| h.reply()) {
                    self.finish(reply);
                } else if now >= *due {
                    if let Some(host) = &mut self.host {
                        host.stop();
                    }
                    self.fail("the image helper did not answer within 2 seconds");
                }
            }
        }
        if waited && !self.waits() {
            self.settled = Some(now);
        }
        wake
    }

    /// Reads the file and sends its bytes to the image helper, for a
    /// picture made for `geometry`; the descriptor to watch while the
    /// request is out.
    fn ask(
        &mut self,
        geometry: Geometry,
        now: Instant,
        helpers: Option<&Helpers>,
    ) -> Option<OwnedFd> {
        let bytes = match self.path.as_deref().map(read) {
            Some(Ok(bytes)) => bytes,
            Some(Err(why)) => {
                self.fail(&why);
                return None;
            }
            None => {
                self.state = State::Off;
                return None;
            }
        };
        if self.host.is_none() {
            self.host = helpers.map(|make| make());
        }
        let Some(host) = self.host.as_mut().filter(|h| h.available()) else {
            self.fail("the image helper cannot start");
            return None;
        };
        let request = Request {
            medium: Medium::Direct(bytes),
            offset: 0,
            max: 0,
            min: 0,
            inflate: false,
            png: false,
            exact: false,
            discard: false,
            any_image: true,
            frames: None,
            pixels: None,
            largest: self.largest,
            largest_bytes: LOGIN_IMAGE_PIXELS_MAX,
            fit: Some(Fitting {
                form: self.form,
                geometry,
            }),
        };
        // the request asks for the account that runs before login: no
        // session's program runs yet, and the pty would belong to root
        let session = Session {
            child: None,
            pty_owner: Ok(0),
        };
        match host.send(&request, &session) {
            Err(reply) => {
                self.state = State::Asking {
                    request,
                    due: now,
                    geometry,
                };
                self.finish(reply);
                None
            }
            Ok(()) => {
                let wake = host.wake();
                self.state = State::Asking {
                    request,
                    due: now.checked_add(REPLY_TIMEOUT).unwrap_or(now),
                    geometry,
                };
                wake
            }
        }
    }

    /// Takes the helper's `reply` to the request out, and makes the
    /// picture from it.
    fn finish(&mut self, reply: Reply) {
        self.watch = None;
        let State::Asking {
            request, geometry, ..
        } = std::mem::replace(&mut self.state, State::Off)
        else {
            return;
        };
        let payload = match reply.checked(&request) {
            Reply::Ok(payload) => payload,
            Reply::Corrupt | Reply::TooShort => {
                return self.fail("the file does not decode as a PNG or JPEG picture");
            }
            Reply::TooLarge | Reply::OverBudget => {
                return self.fail(&format!(
                    "the picture is larger than {}x{} or than 256 MiB",
                    self.largest.0, self.largest.1
                ));
            }
            Reply::Unreadable | Reply::Failed => {
                return self.fail("the image helper cannot decode it");
            }
        };
        let Some(size) = payload.size() else {
            return self.fail("the image helper sent no picture");
        };
        let fitting = Fitting {
            form: self.form,
            geometry,
        };
        // the helper has fitted the picture already: a size it would not
        // make for this screen is refused before a pixel is read
        let Some((place, prompt_row)) = place(&fitting, size) else {
            return self.fail("the image helper's picture does not fit the screen");
        };
        let Some((pixels, opaque)) = payload.copy_into(Vec::new()) else {
            return self.fail("the image helper's picture cannot be read");
        };
        self.state = State::Ready {
            geometry,
            made: Rc::new(Made {
                width: size.0,
                height: size.1,
                pixels: Pixels::new(pixels),
                opaque,
                place,
                prompt_row,
            }),
        };
    }

    /// Logs why vtium cannot show the picture, once, and shows none.
    fn fail(&mut self, why: &str) {
        self.watch = None;
        self.state = State::Off;
        let path = self.path.as_deref().unwrap_or(std::path::Path::new(""));
        say(&format!(
            "Cannot show the login picture {}: {why}. vtium starts the login prompt without it.",
            path.display()
        ));
    }

    /// Places the picture in `term`, whose console is about to start its
    /// session's program, and for a banner moves the cursor to the first
    /// column of the row below it. A console whose store takes no pictures,
    /// with a budget of 0 or on a display that draws none, gets none.
    pub(crate) fn place(&mut self, term: &mut Term, stores: bool) {
        let Some(made) = self.made().cloned() else {
            return;
        };
        if !stores {
            return;
        }
        let screen = term.terminal.screen_mut();
        let placed = screen
            .add_shared_image(made.image(), made.pixels.clone())
            .and_then(|key| screen.place_image(key, made.place));
        if placed.is_err() {
            if !self.no_room_logged {
                self.no_room_logged = true;
                say(&format!(
                    "Cannot show the login picture {}: the console's image memory cannot hold it. vtium starts the login prompt without it.",
                    self.path
                        .as_deref()
                        .unwrap_or(std::path::Path::new(""))
                        .display()
                ));
            }
            return;
        }
        if let Some(row) = made.prompt_row {
            let row = row.saturating_add(1);
            term.terminal.input(format!("\x1b[{row};1H").as_bytes());
        }
    }
}

/// The bytes of the file at `path`, read-only and at most
/// [`LOGIN_IMAGE_FILE_MAX`], when it is a PNG or a JPEG file; else why
/// not, in the words of the log line.
fn read(path: &std::path::Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => "the file does not exist".to_owned(),
        std::io::ErrorKind::PermissionDenied => "vtium may not read the file".to_owned(),
        _ => format!("vtium cannot open the file ({e})"),
    })?;
    let mut bytes = Vec::new();
    file.take(LOGIN_IMAGE_FILE_MAX.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| format!("vtium cannot read the file ({e})"))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > LOGIN_IMAGE_FILE_MAX {
        return Err("the file is larger than 64 MiB".to_owned());
    }
    let png = bytes.starts_with(b"\x89PNG\r\n\x1a\n");
    let jpeg = bytes.starts_with(&[0xff, 0xd8, 0xff]);
    if !png && !jpeg {
        return Err("the file is not a PNG or JPEG picture".to_owned());
    }
    Ok(bytes)
}

/// Writes a line of the login picture's log, at warning level. Under test
/// it also keeps the line, for the tests to read back.
fn say(line: &str) {
    log::warn!(target: "image", "{line}");
    #[cfg(test)]
    super::built::keep(log::Level::Warn, line.to_owned());
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use std::cell::RefCell;
    use std::time::Duration;

    use vtium_image::Switches;
    use vtium_image::decode::Payload;
    use vtium_term::Terminal;

    use super::*;

    const SCREEN: Geometry = Geometry {
        display: (80, 48),
        cell: (8, 16),
        grid: (10, 3),
    };

    /// An image helper that answers each request at once with `answer`,
    /// or holds it out when `answer` is `None`.
    struct StandIn {
        answer: Option<Reply>,
        sent: Rc<RefCell<Vec<Request>>>,
    }

    impl Helper for StandIn {
        fn send(&mut self, request: &Request, session: &Session) -> Result<(), Reply> {
            // the login picture is decoded as the account before login
            assert_eq!((session.child, session.pty_owner.clone()), (None, Ok(0)));
            self.sent.borrow_mut().push(request.clone());
            match &self.answer {
                // the real helper fits a login picture before it answers
                Some(Reply::Ok(Payload::Rgba {
                    width,
                    height,
                    data,
                })) => {
                    let fitting = request.fit.expect("the login request asks for a fit");
                    let source = vtium_image::login::Source::Rgba(data);
                    let (w, h, px) = vtium_image::login::fit(&fitting, (*width, *height), source)
                        .ok_or(Reply::TooLarge)?;
                    let data = px
                        .iter()
                        .flat_map(|p| p.to_be_bytes().into_iter().cycle().skip(1).take(4))
                        .collect();
                    Err(Reply::Ok(Payload::Rgba {
                        width: w,
                        height: h,
                        data,
                    }))
                }
                Some(reply) => Err(reply.clone()),
                None => Ok(()),
            }
        }
        fn reply(&mut self) -> Option<Reply> {
            None
        }
        fn stop(&mut self) {}
        fn session_ended(&mut self) {}
        fn wake(&self) -> Option<OwnedFd> {
            None
        }
        fn available(&self) -> bool {
            true
        }
    }

    fn helpers(answer: Option<Reply>) -> (Helpers, Rc<RefCell<Vec<Request>>>) {
        let sent = Rc::new(RefCell::new(Vec::new()));
        let kept = Rc::clone(&sent);
        let make: Helpers = Rc::new(move || {
            Box::new(StandIn {
                answer: answer.clone(),
                sent: Rc::clone(&kept),
            })
        });
        (make, sent)
    }

    /// A picture of `w`×`h` opaque pixels, as the helper answers it.
    fn decoded(w: u32, h: u32) -> Reply {
        let data = (0..w.saturating_mul(h))
            .flat_map(|i| [u8::try_from(i % 251).unwrap_or(0), 0x40, 0x80, 0xff])
            .collect();
        Reply::Ok(Payload::Rgba {
            width: w,
            height: h,
            data,
        })
    }

    /// A scratch file holding `bytes`, in a directory of this test's own.
    fn file(name: &str, bytes: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vtium-login-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let path = dir.join("login.png");
        std::fs::write(&path, bytes).expect("scratch file");
        path
    }

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nnot really a png";

    // A vtium.conf that names a picture and no fit fits it inside the
    // display, keeping its shape, with bands of the default background.
    #[test]
    fn a_picture_with_no_fit_named_is_contained() {
        let read = |conf: &'static str| {
            let loaded = crate::config::load(&[], &mut |_: &std::path::Path| {
                Ok(Some(conf.as_bytes().to_vec()))
            });
            Login::read(&loaded.config, (10_000, 10_000)).form.fit
        };
        assert_eq!(read("login-image=/etc/vtium/login.png\n"), Fit::Contain);
        assert_eq!(
            read("login-image=/etc/vtium/login.png\nlogin-image-fit=cover\n"),
            Fit::Cover
        );
    }

    fn login(path: PathBuf, fit: Fit) -> Login {
        Login {
            path: Some(path),
            form: Form {
                fit,
                rows: None,
                position: Position::Left,
            },
            largest: (10_000, 10_000),
            state: State::Idle,
            host: None,
            watch: None,
            no_room_logged: false,
            settled: None,
        }
    }

    fn said() -> Vec<String> {
        super::super::built::said_lines()
    }

    #[expect(clippy::disallowed_methods, reason = "a test's clock is the host's")]
    fn now() -> Instant {
        Instant::now()
    }

    /// A console's terminal of 10×3 cells of 8×16, which stores pictures.
    #[expect(clippy::disallowed_methods, reason = "a test's clock is the host's")]
    fn console(limit: u64) -> Term {
        let clock: crate::term::Clock = Rc::new(Instant::now);
        let mut terminal = Terminal::new(10, 3, 100);
        super::super::install(
            &mut terminal,
            &super::super::Settings {
                limit: None,
                graphic: (10_000, 10_000),
                registers: 1024,
                private_registers: true,
                sixel: true,
                kitty: true,
            },
        );
        terminal.screen_mut().set_image_limit(limit);
        terminal.screen_mut().set_image_cell(8, 16).expect("cell");
        terminal.set_image_switches(Switches {
            sixel: true,
            kitty: true,
            iterm2: false,
        });
        Term::new(
            terminal,
            crate::sync::Sync::new(Duration::ZERO, 0),
            crate::blink::Blink::new(false),
            clock,
        )
    }

    fn placements(term: &Term) -> usize {
        term.terminal.screen().images().placement_count()
    }

    // ADR-1100 D17: a file vtium cannot use starts the login prompt
    // without the picture, and vtium logs one line naming the file and
    // the reason
    #[test]
    fn a_missing_a_foreign_and_a_corrupt_file_each_log_one_line_and_place_nothing() {
        let missing = std::env::temp_dir().join("vtium-login-no-such-file.png");
        let text = file("text", b"this is no picture");
        let corrupt = file("corrupt", PNG);
        let cases = [
            (missing.clone(), "the file does not exist"),
            (text.clone(), "the file is not a PNG or JPEG picture"),
            (
                corrupt.clone(),
                "the file does not decode as a PNG or JPEG picture",
            ),
        ];
        let (make, _) = helpers(Some(Reply::Corrupt));
        for (path, why) in cases {
            said();
            let mut login = login(path.clone(), Fit::Cover);
            assert!(login.waits());
            login.step(Some(SCREEN), now(), Some(&make));
            assert!(!login.waits(), "{why}: the first login prompt starts");
            assert!(login.made().is_none());
            // a later turn logs nothing more
            login.step(Some(SCREEN), now(), Some(&make));
            let lines = said();
            assert_eq!(lines.len(), 1, "{lines:?}");
            assert_eq!(
                lines[0],
                format!(
                    "Cannot show the login picture {}: {why}. vtium starts the login prompt without it.",
                    path.display()
                )
            );
            let mut term = console(1 << 20);
            login.place(&mut term, true);
            assert_eq!(placements(&term), 0);
        }
    }

    #[test]
    fn a_file_past_its_bound_is_never_sent() {
        let path = file("large", PNG);
        let f = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("open");
        f.set_len(LOGIN_IMAGE_FILE_MAX + 1).expect("grow");
        let (make, sent) = helpers(Some(decoded(4, 4)));
        said();
        let mut login = login(path, Fit::Cover);
        login.step(Some(SCREEN), now(), Some(&make));
        assert!(sent.borrow().is_empty());
        assert!(said()[0].contains("the file is larger than 64 MiB"));
    }

    // ADR-1101 D8: the first login prompt waits at most 2 seconds
    #[test]
    fn the_first_login_prompt_waits_for_the_helper_at_most_2_seconds() {
        let path = file("slow", PNG);
        let (make, sent) = helpers(None);
        said();
        let mut login = login(path, Fit::Cover);
        let start = now();
        login.step(Some(SCREEN), start, Some(&make));
        assert_eq!(sent.borrow().len(), 1);
        let request = sent.borrow()[0].clone();
        assert!(request.any_image && request.medium.letter() == 'd');
        assert_eq!(request.largest_bytes, LOGIN_IMAGE_PIXELS_MAX);
        // the helper is asked to fit the picture to this screen
        assert_eq!(request.fit.map(|f| f.geometry), Some(SCREEN));
        assert!(login.waits());
        assert_eq!(login.due(), Some(start + REPLY_TIMEOUT));
        login.step(
            Some(SCREEN),
            start + Duration::from_millis(1999),
            Some(&make),
        );
        assert!(login.waits());
        login.step(Some(SCREEN), start + REPLY_TIMEOUT, Some(&make));
        assert!(!login.waits());
        // the loop turns once more, at once, to start the login prompt
        assert_eq!(login.due(), Some(start + REPLY_TIMEOUT));
        login.step(Some(SCREEN), start + REPLY_TIMEOUT, Some(&make));
        assert_eq!(login.due(), None);
        assert_eq!(said().len(), 1);
    }

    // ADR-1100 D17: an ordinary kitty placement with no id, at the lowest
    // z, that no program can name, that ED 2 and a delete by z remove and
    // that the other erases leave
    // covers: image.login
    #[test]
    fn a_background_is_a_placement_no_program_can_name() {
        let path = file("good", PNG);
        let (make, _) = helpers(Some(decoded(80, 48)));
        let mut login = login(path, Fit::Cover);
        login.step(Some(SCREEN), now(), Some(&make));
        assert!(login.made().is_some());
        let mut term = console(1 << 20);
        login.place(&mut term, true);
        assert_eq!(placements(&term), 1);
        let shown = term.terminal.screen().placements();
        assert_eq!(
            (shown[0].cols, shown[0].rows, shown[0].col, shown[0].row),
            (10, 3, 0, 0)
        );
        // a program names images by id or number: the login picture has
        // neither, so no command reaches it that way
        for command in [
            "\x1b_Ga=p,i=1\x1b\\",
            "\x1b_Ga=p,I=1\x1b\\",
            "\x1b_Ga=d,d=i,i=1\x1b\\",
            "\x1b_Ga=d,d=n,I=1\x1b\\",
            "\x1b_Ga=d,d=I,i=1\x1b\\",
        ] {
            term.terminal.input(command.as_bytes());
            assert_eq!(placements(&term), 1, "{command:?}");
        }
        let replies = String::from_utf8(term.terminal.take_replies()).expect("text");
        assert!(!replies.contains("OK"), "{replies:?}");
        // ED 0, ED 1 and EL leave it
        term.terminal.input(b"\x1b[J\x1b[1J\x1b[K\x1b[2K");
        assert_eq!(placements(&term), 1);
        // ED 2 removes it
        term.terminal.input(b"\x1b[2J");
        assert_eq!(placements(&term), 0);
        // so does a delete by its z, at the next login prompt's placement
        login.place(&mut term, true);
        assert_eq!(placements(&term), 1);
        term.terminal.input(b"\x1b_Ga=d,d=z,z=-2147483648\x1b\\");
        assert_eq!(placements(&term), 0);
    }

    #[test]
    fn each_login_prompt_after_a_reset_gets_one_placement() {
        let path = file("respawn", PNG);
        let (make, sent) = helpers(Some(decoded(80, 48)));
        let mut login = login(path, Fit::Cover);
        login.step(Some(SCREEN), now(), Some(&make));
        let mut term = console(1 << 20);
        login.place(&mut term, true);
        term.terminal.reset();
        term.terminal.screen_mut().set_image_limit(1 << 20);
        login.place(&mut term, true);
        assert_eq!(placements(&term), 1);
        // the decoded picture is reused while the display keeps its size
        login.step(Some(SCREEN), now(), Some(&make));
        assert_eq!(sent.borrow().len(), 1);
        // and asked for again once it changes
        let wider = Geometry {
            display: (160, 48),
            grid: (20, 3),
            ..SCREEN
        };
        login.step(Some(wider), now(), Some(&make));
        assert_eq!(sent.borrow().len(), 2);
    }

    #[test]
    fn a_banner_moves_the_login_prompt_below_it() {
        let path = file("banner", PNG);
        let (make, _) = helpers(Some(decoded(20, 20)));
        let mut login = login(path, Fit::Banner);
        login.step(Some(SCREEN), now(), Some(&make));
        let mut term = console(1 << 20);
        login.place(&mut term, true);
        // on 3 rows the banner may take 1, so it is scaled down to 16×16
        assert_eq!(placements(&term), 1);
        assert_eq!(term.terminal.screen().cursor(), (0, 1), "column 0, row 1");
        let tall = Geometry {
            grid: (10, 5),
            ..SCREEN
        };
        login.step(Some(tall), now(), Some(&make));
        let mut term = console(1 << 20);
        term.terminal.resize(10, 5);
        login.place(&mut term, true);
        assert_eq!(placements(&term), 1);
        // 20 pixels of 16-pixel rows take 2 rows: the prompt starts on the
        // third
        assert_eq!(term.terminal.screen().cursor(), (0, 2), "column 0, row 2");
    }

    #[test]
    fn a_console_that_stores_no_pictures_gets_none_and_one_too_small_logs_once() {
        let path = file("budget", PNG);
        let (make, _) = helpers(Some(decoded(80, 48)));
        let mut login = login(path, Fit::Cover);
        login.step(Some(SCREEN), now(), Some(&make));
        let mut term = console(1 << 20);
        login.place(&mut term, false);
        assert_eq!(placements(&term), 0);
        said();
        let mut small = console(4096);
        login.place(&mut small, true);
        login.place(&mut small, true);
        assert_eq!(placements(&small), 0);
        let lines = said();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("the console's image memory cannot hold it"));
    }

    // ADR-1100 D17: the helper hands back only the fitted pixels, and the
    // consoles of a head share one copy of them
    #[test]
    fn a_4k_picture_comes_back_fitted_and_two_consoles_share_one_copy() {
        let path = file("4k", PNG);
        let (make, _) = helpers(Some(decoded(3840, 2160)));
        let mut login = login(path, Fit::Cover);
        let hd = Geometry {
            display: (1920, 1080),
            cell: (8, 16),
            grid: (240, 67),
        };
        login.step(Some(hd), now(), Some(&make));
        let made = login.made().expect("made").clone();
        // 1920×1072 pixels, 8,232,960 bytes: the only copy the console holds
        assert_eq!((made.width, made.height), (1920, 1072));
        assert_eq!(made.pixels.as_slice().len() * 4, 8_232_960);
        let mut consoles = [console(64 << 20), console(64 << 20)];
        for term in &mut consoles {
            term.terminal.resize(240, 67);
            login.place(term, true);
            let images = term.terminal.screen().images();
            let shown = term.terminal.screen().placements();
            let pixels = images
                .frame_pixels(shown[0].image, 1)
                .expect("pixels")
                .pixels;
            assert!(
                pixels.same(&made.pixels),
                "a console holds a copy of its own"
            );
        }
    }

    #[test]
    fn a_reply_the_helper_did_not_fit_is_refused() {
        let path = file("unfitted", PNG);
        // a helper that answers the decoded picture at its own size
        let sent = Rc::new(RefCell::new(Vec::new()));
        let make: Helpers = Rc::new(move || {
            Box::new(Unfitted {
                sent: Rc::clone(&sent),
            })
        });
        said();
        let mut login = login(path, Fit::Cover);
        login.step(Some(SCREEN), now(), Some(&make));
        assert!(login.made().is_none());
        assert!(said()[0].contains("the image helper's picture does not fit the screen"));
    }

    /// An image helper that ignores the fit and answers a picture of 8×8.
    struct Unfitted {
        sent: Rc<RefCell<Vec<Request>>>,
    }

    impl Helper for Unfitted {
        fn send(&mut self, request: &Request, _: &Session) -> Result<(), Reply> {
            self.sent.borrow_mut().push(request.clone());
            Err(decoded(8, 8))
        }
        fn reply(&mut self) -> Option<Reply> {
            None
        }
        fn stop(&mut self) {}
        fn session_ended(&mut self) {}
        fn wake(&self) -> Option<OwnedFd> {
            None
        }
        fn available(&self) -> bool {
            true
        }
    }
}
