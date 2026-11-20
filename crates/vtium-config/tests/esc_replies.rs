//! The Esc key pressed while the console's replies to a live trial are due.
//!
//! Each test drives the TUI's model and its decoder the way the loop in
//! port.rs does, with a console that answers every batch the TUI writes. The
//! person presses Esc as the replies arrive, so the Esc key's byte shares a
//! read with a reply, or comes in a read of its own just before one. The end
//! must be the same as when the Esc key comes on its own after the replies:
//! every reply reaches the part of the TUI that asked, no query stays
//! outstanding, and exactly one Esc key reaches the model.
//!
//! References:
//! - ADR-1206 D2, "Every batch of queries ends with DA1" and "Esc stays the
//!   key that goes back": the queue of askers and the lone ESC.

use vtium_config::tui::fixture;
use vtium_config::tui::input::{Decoder, Input, Key, Reply};
use vtium_config::tui::model::{App, Effect, Event};
use vtium_config::tui::nav;

const ESC: &[u8] = b"\x1b";

/// A vtium console's answers to the bytes the TUI wrote: a font switch
/// changes the font, the font query gets the font in use, and DA1 gets its
/// reply, in the order of the requests.
struct Console {
    font: String,
}

impl Console {
    fn answer(&mut self, written: &[u8]) -> Vec<u8> {
        let text = String::from_utf8_lossy(written);
        let mut out = Vec::new();
        let mut rest = &text[..];
        loop {
            let osc = rest.find("\x1b]50;");
            let da1 = rest.find("\x1b[c");
            match (osc, da1) {
                (Some(o), d) if d.is_none_or(|d| o < d) => {
                    let body = &rest[o + 5..];
                    let end = body.find("\x1b\\").expect("an OSC 50 ends with ST");
                    match &body[..end] {
                        "?" => out.extend(format!("\x1b]50;{}\x1b\\", self.font).bytes()),
                        name => self.font = name.to_owned(),
                    }
                    rest = &body[end + 2..];
                }
                (_, Some(d)) => {
                    out.extend(b"\x1b[?62;4c");
                    rest = &rest[d + 3..];
                }
                _ => return out,
            }
        }
    }
}

/// The TUI, its decoder and the console behind them, plus what the TUI
/// wrote and the console has not answered yet.
struct Run {
    app: App,
    decoder: Decoder,
    console: Console,
    unanswered: Vec<u8>,
    escapes: usize,
    /// The status the TUI left with, once it leaves.
    exit: Option<u8>,
}

impl Run {
    /// The TUI after its start, with the live trial of tiny5 begun, so the
    /// switch's batch is outstanding.
    fn trial() -> Run {
        let (mut app, _) = App::new(fixture::inputs(), 80, 25);
        for reply in [
            Reply::Version("vtium(test)".into()),
            Reply::Font("unifont".into()),
            Reply::Attributes,
        ] {
            app.event(Event::Input(Input::Reply(reply)));
        }
        let mut run = Run {
            app,
            decoder: Decoder::keys(),
            console: Console {
                font: "unifont".into(),
            },
            unanswered: Vec::new(),
            escapes: 0,
            exit: None,
        };
        // tiny5 is the fourth font set below the Looks tab's one look
        for key in [Key::Char('3'), Key::Down, Key::Down, Key::Down, Key::Down] {
            run.event(Event::Input(Input::Key(key)));
        }
        run.event(Event::Input(Input::Key(Key::Char('t'))));
        assert!(run.app.awaiting(), "the trial's switch is outstanding");
        assert_eq!(run.app.shared.in_flight.as_deref(), Some("tiny5"));
        run
    }

    fn event(&mut self, event: Event) {
        for effect in self.app.event(event) {
            match effect {
                Effect::Write(bytes) => self.unanswered.extend(bytes),
                Effect::Exit(status) => self.exit = Some(status),
                _ => {}
            }
        }
    }

    /// The console's answers to what the TUI wrote so far.
    fn answers(&mut self) -> Vec<u8> {
        let written = std::mem::take(&mut self.unanswered);
        self.console.answer(&written)
    }

    /// One read, decoded as the loop decodes it.
    fn read(&mut self, bytes: &[u8]) {
        self.decoder.push(bytes);
        while let Some(input) = self.decoder.next(self.app.awaiting()) {
            if input == Input::Key(Key::Escape) {
                self.escapes += 1;
            }
            self.event(Event::Input(input));
        }
    }

    /// The console answers each batch the TUI writes, each in a read of
    /// its own, until the TUI writes nothing more.
    fn settle(&mut self) {
        loop {
            let answers = self.answers();
            if answers.is_empty() {
                return;
            }
            self.read(&answers);
        }
    }

    /// What the test compares: where the person is, the font the console
    /// draws as the TUI knows it, the switch in flight, the refused fonts
    /// and whether a reply is still due.
    fn end(&self) -> String {
        format!(
            "node {} shown {:?} in flight {:?} refused {:?} awaiting {}",
            nav::node(&self.app),
            self.app.shared.shown_font,
            self.app.shared.in_flight,
            self.app.shared.refused,
            self.app.awaiting(),
        )
    }
}

/// The end when the replies come first and the keys after them, each in a
/// read of its own: no reply is due when the Esc key comes.
fn after_the_replies(keys: &[&[u8]]) -> String {
    let mut run = Run::trial();
    run.settle();
    assert!(!run.app.awaiting());
    for key in keys {
        run.read(key);
        run.settle();
    }
    assert_eq!(run.escapes, 1);
    run.end()
}

/// Checks a run that raced the Esc key with the replies against the
/// run that did not.
fn check(run: &mut Run, keys: &[&[u8]]) {
    run.settle();
    assert_eq!(run.escapes, 1, "one Esc key reached the model");
    assert!(run.app.shared.refused.is_empty(), "{}", run.end());
    assert!(!run.app.awaiting(), "{}", run.end());
    assert_eq!(run.end(), after_the_replies(keys));
}

#[test]
fn the_esc_key_and_the_whole_batch_in_one_read() {
    let mut run = Run::trial();
    let mut read = ESC.to_vec();
    read.extend(run.answers());
    run.read(&read);
    check(&mut run, &[ESC]);
}

#[test]
fn the_esc_key_between_the_font_reply_and_da1_in_one_read() {
    let mut run = Run::trial();
    let answers = run.answers();
    let da1 = answers
        .windows(3)
        .position(|w| w == b"\x1b[?")
        .expect("DA1's reply");
    let mut read = answers[..da1].to_vec();
    read.extend(ESC);
    read.extend(&answers[da1..]);
    run.read(&read);
    check(&mut run, &[ESC]);
}

#[test]
fn the_esc_key_in_a_read_of_its_own_before_the_batch() {
    let mut run = Run::trial();
    run.read(ESC);
    assert_eq!(run.escapes, 0, "the ESC waits for its next byte");
    let answers = run.answers();
    run.read(&answers);
    check(&mut run, &[ESC]);
}

/// The Esc key, then a key whose byte could continue a reply, each in a
/// read of its own, then the replies.
#[test]
fn the_esc_key_and_a_bracket_before_the_batch() {
    for typed in [b"[", b"]", b"_", b"P"] {
        let mut run = Run::trial();
        run.read(ESC);
        run.read(typed);
        let answers = run.answers();
        run.read(&answers);
        check(&mut run, &[ESC, typed]);
    }
}

/// Alt+[ sends `ESC [`, which opens a control sequence, and Ctrl-C right
/// after it still makes the TUI leave: while the trial's replies are due,
/// and once they are in.
#[test]
fn ctrl_c_after_alt_bracket_leaves() {
    for replies_first in [false, true] {
        let mut run = Run::trial();
        if replies_first {
            run.settle();
        }
        run.read(b"\x1b[");
        run.read(b"\x03");
        run.settle();
        assert!(
            run.exit.is_some(),
            "replies first {replies_first}: {}",
            run.end()
        );
        assert!(!run.app.awaiting(), "{}", run.end());
    }
}
