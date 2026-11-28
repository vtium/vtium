//! The live console: what the TUI asks the terminal, how each reply reaches
//! the part of the TUI that asked, the live trial of a font, and the font
//! given back when the TUI leaves.
//!
//! At start the TUI asks the terminal what it is ([`start`]): XTVERSION,
//! the OSC 50 query and kitty's graphics query, with DA1 last. The answers
//! settle whether this terminal is a vtium console that switches its font
//! for vtium-config, one with `allow-font-ops` off, or another terminal.
//! The wait for them is at most one second. [`queue`] is the queue of
//! askers that hands each reply to whoever asked.
//!
//! The console shows the font in [`Console::want`] once no other switch is
//! in flight ([`follow`]), so at most one switch is outstanding. A reply
//! that names another font than the one asked for means the console
//! refused it, and the font is marked refused. [`trial`] tries a font or a
//! look for ten seconds, and [`leave`] gives the console its start font
//! back as the TUI leaves. Nothing here changes the document: the trial is
//! a preview. [`machine`] reads the machine for the Actions menu's items.
//!
//! Every function here takes the shared state and this module's own
//! [`State`], and returns the effects for the loop, so the module stays a
//! part of the one update and writes nothing itself.
//!
//! References:
//! - ADR-1201 D4, "Finding out what the terminal is": the requests at
//!   start, the one-second wait, and what the answers decide.
//! - ADR-1201 D4, "The live trial, on a key": one switch outstanding at a
//!   time, and a reply naming another font as the console's refusal.
//! - ADR-1206 D2: a reply goes to the live console's module, and the
//!   module returns effects.

pub mod leave;
pub mod machine;
pub mod queue;
pub mod sheet;
pub mod trial;
pub mod words;

use std::time::Duration;

use vtium_config_core::ui::{Asker, Console, Effect, Live, Query, Shared};
use vtium_config_widgets::keys::Reply;

use queue::{Answer, Batch};

/// The action that a yes to the question before a system font hands the
/// shell, which the install's sheet is drawn for.
pub const INSTALL_FONT: &str = "y.question.system-font";

/// How long the TUI waits for the terminal's answers at start, and for the
/// replies still due when it leaves.
pub const WAIT: Duration = Duration::from_secs(1);

/// The live console's module's own state: whether the questions at start
/// went out, what came back to them, the trial under way, and the exit
/// status once the TUI is leaving.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    started: bool,
    answers: Answers,
    /// The trial that counts down, while one does.
    pub trial: Option<trial::Trial>,
    /// The exit status, once the TUI leaves and waits for the replies
    /// still due.
    pub leaving: Option<u8>,
    /// The font file readied as a system font, which the install copies.
    pub font: Option<machine::Ready>,
}

/// The answers to the questions at start, until DA1's reply settles what
/// the terminal is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Answers {
    version: Option<String>,
    font: Option<String>,
    graphics: bool,
}

/// The bytes of one font switch to `name`: `OSC 50 ; NAME ST`, then the
/// query that asks which font the console draws.
fn switch(asker: Asker, name: &str) -> Batch {
    Batch::new(asker)
        .write(format!("\x1b]50;{name}\x1b\\").as_bytes())
        .ask(Query::Font)
}

/// Asks the terminal what it is, once: the requests at start and the wait
/// for their answers.
pub fn start(shared: &mut Shared, state: &mut State) -> Vec<Effect> {
    if state.started {
        return Vec::new();
    }
    state.started = true;
    let batch = Batch::new(Asker::Start)
        .ask(Query::Version)
        .ask(Query::Font)
        .ask(Query::Graphics);
    let bytes = queue::send(&mut shared.askers, batch);
    vec![Effect::Write(bytes), Effect::Wait(WAIT)]
}

/// A reply from the terminal, which the queue hands to whoever asked. Once
/// the TUI is leaving and the last reply due has come, it leaves.
pub fn reply(shared: &mut Shared, state: &mut State, reply: Reply) -> Vec<Effect> {
    let heard = queue::answer(&mut shared.askers, reply);
    let settled = !heard.is_empty();
    let mut out = Vec::new();
    for (asker, answer) in heard {
        match asker {
            Asker::Start => start_answer(&mut shared.console, state, answer),
            Asker::Switch => out.extend(switch_answer(shared, state, answer)),
            Asker::Leave => {
                if let Answer::Reply(Reply::Font(name)) = answer {
                    shared.console.shown_font = Some(name);
                }
            }
            // no part of the TUI asks these yet, so their replies settle
            // the queue and go no further
            Asker::TextArea | Asker::Tab(_) => {}
        }
    }
    if let Some(status) = state.leaving
        && settled
        && !shared.askers.awaiting()
    {
        out.push(Effect::Exit(status));
    }
    out
}

/// What the requests at start hear back. DA1's reply settles what the
/// terminal is.
fn start_answer(console: &mut Console, state: &mut State, answer: Answer) {
    if console.live != Live::Asking {
        return;
    }
    match answer {
        Answer::Reply(Reply::Version(v)) => state.answers.version = Some(v),
        Answer::Reply(Reply::Font(f)) => state.answers.font = Some(f),
        Answer::Reply(Reply::Graphics(ok)) => state.answers.graphics |= ok,
        Answer::Reply(Reply::Attributes) => decide(console, state),
        Answer::Reply(Reply::TextArea(_)) | Answer::Unanswered(_) => {}
    }
}

/// Settles what the terminal is from the answers that came: a vtium
/// console that names its font, one that does not, which has
/// `allow-font-ops` off, or another terminal.
fn decide(console: &mut Console, state: &mut State) {
    let answers = std::mem::take(&mut state.answers);
    let vtium = answers
        .version
        .as_deref()
        .is_some_and(|v| v.starts_with("vtium("));
    console.version = answers.version;
    console.graphics = answers.graphics;
    console.live = match (vtium, answers.font) {
        (true, Some(font)) => {
            console.start_font = Some(font.clone());
            console.shown_font = Some(font);
            Live::On
        }
        (true, None) => Live::OpsOff,
        (false, _) => Live::NotVtium,
    };
}

/// What the switch in flight hears back: the font the console draws now,
/// or that the console left the query unanswered. A font the console did
/// not take is marked refused, a trial of it ends, and the next switch
/// goes now that none is in flight.
fn switch_answer(shared: &mut Shared, state: &mut State, answer: Answer) -> Vec<Effect> {
    let shown = match answer {
        Answer::Reply(Reply::Font(name)) => Some(name),
        Answer::Unanswered(Query::Font) => None,
        _ => return Vec::new(),
    };
    let console = &mut shared.console;
    let asked = console.in_flight.take();
    if state.leaving.is_some() {
        console.shown_font = shown.or(console.shown_font.take());
        return Vec::new();
    }
    if let Some(asked) = asked
        && shown.as_ref() != Some(&asked)
    {
        console.refused.insert(asked.clone());
        if console.want.as_deref() == Some(asked.as_str()) {
            console.want = shown.clone().or_else(|| console.shown_font.clone());
        }
        if state.trial.take_if(|t| t.font == asked).is_some() {
            shared.message = Some(words::refused(&asked));
        }
    }
    if shown.is_some() {
        shared.console.shown_font = shown;
    }
    follow(shared)
}

/// The end of a wait. At start it is the deadline for a terminal that does
/// not answer DA1: the TUI settles what the terminal is from the answers
/// that came, and forgets the queries, which no reply will settle now.
/// While the TUI leaves it is the end of the wait for the replies due.
/// Otherwise it is one second of the trial's countdown.
pub fn timeout(shared: &mut Shared, state: &mut State) -> Vec<Effect> {
    if shared.console.live == Live::Asking {
        shared.askers.clear();
        decide(&mut shared.console, state);
    }
    match state.leaving {
        Some(status) => vec![Effect::Exit(status)],
        None => trial::tick(shared, state),
    }
}

/// Sends the font the console is to draw, where no switch is outstanding
/// and the console draws another: one switch at a time, naming whatever
/// is wanted when the last reply came.
pub fn follow(shared: &mut Shared) -> Vec<Effect> {
    let console = &shared.console;
    if console.live != Live::On || console.in_flight.is_some() {
        return Vec::new();
    }
    let Some(name) = console.want.clone() else {
        return Vec::new();
    };
    if console.shown_font.as_deref() == Some(name.as_str()) || console.refused.contains(&name) {
        return Vec::new();
    }
    let bytes = queue::send(&mut shared.askers, switch(Asker::Switch, &name));
    shared.console.in_flight = Some(name);
    vec![Effect::Write(bytes)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared() -> Shared {
        vtium_config_core::fixture::padnoir::shared()
    }

    /// A vtium console that answers the questions at start with `font`.
    fn on_vtium(font: &str) -> (Shared, State) {
        let mut shared = shared();
        let mut state = State::default();
        start(&mut shared, &mut state);
        reply(
            &mut shared,
            &mut state,
            Reply::Version("vtium(0.1.0)".into()),
        );
        reply(&mut shared, &mut state, Reply::Font(font.into()));
        reply(&mut shared, &mut state, Reply::Attributes);
        (shared, state)
    }

    #[test]
    fn the_questions_at_start_go_once_and_wait_a_second() {
        let mut shared = shared();
        let mut state = State::default();
        let out = start(&mut shared, &mut state);
        assert_eq!(
            out,
            [
                Effect::Write(
                    b"\x1b[>q\x1b]50;?\x1b\\\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[c"
                        .to_vec()
                ),
                Effect::Wait(WAIT)
            ]
        );
        assert!(start(&mut shared, &mut state).is_empty());
    }

    #[test]
    fn the_answers_at_start_settle_what_the_terminal_is() {
        let (shared, _) = on_vtium("unifont");
        assert_eq!(shared.console.live, Live::On);
        assert_eq!(shared.console.start_font.as_deref(), Some("unifont"));
        // a vtium console with allow-font-ops off answers XTVERSION and DA1
        let mut shared = super::tests::shared();
        let mut state = State::default();
        start(&mut shared, &mut state);
        reply(
            &mut shared,
            &mut state,
            Reply::Version("vtium(0.1.0)".into()),
        );
        reply(&mut shared, &mut state, Reply::Attributes);
        assert_eq!(shared.console.live, Live::OpsOff);
        assert!(!shared.askers.awaiting());
        // another terminal answers DA1 alone
        let mut shared = super::tests::shared();
        let mut state = State::default();
        start(&mut shared, &mut state);
        reply(&mut shared, &mut state, Reply::Attributes);
        assert_eq!(shared.console.live, Live::NotVtium);
        // a terminal that answers nothing is settled by the deadline
        let mut shared = super::tests::shared();
        let mut state = State::default();
        start(&mut shared, &mut state);
        assert!(timeout(&mut shared, &mut state).is_empty());
        assert_eq!(shared.console.live, Live::NotVtium);
        assert!(!shared.askers.awaiting());
    }

    #[test]
    fn one_switch_is_outstanding_at_a_time() {
        let (mut shared, mut state) = on_vtium("unifont");
        shared.console.want = Some("tiny5".into());
        let out = follow(&mut shared);
        assert_eq!(
            out,
            [Effect::Write(
                b"\x1b]50;tiny5\x1b\\\x1b]50;?\x1b\\\x1b[c".to_vec()
            )]
        );
        shared.console.want = Some("proof".into());
        assert!(
            follow(&mut shared).is_empty(),
            "tiny5's switch is in flight"
        );
        // the answer to tiny5's switch sends the font wanted since
        let out = reply(&mut shared, &mut state, Reply::Font("tiny5".into()));
        assert_eq!(
            out,
            [Effect::Write(
                b"\x1b]50;proof\x1b\\\x1b]50;?\x1b\\\x1b[c".to_vec()
            )]
        );
        assert_eq!(shared.console.shown_font.as_deref(), Some("tiny5"));
    }

    #[test]
    fn a_reply_naming_another_font_is_the_console_s_refusal() {
        let (mut shared, mut state) = on_vtium("unifont");
        shared.console.want = Some("tiny5".into());
        follow(&mut shared);
        reply(&mut shared, &mut state, Reply::Font("unifont".into()));
        assert!(shared.console.refused.contains("tiny5"));
        assert_eq!(shared.console.want.as_deref(), Some("unifont"));
        // a refused font is not sent again
        reply(&mut shared, &mut state, Reply::Attributes);
        shared.console.want = Some("tiny5".into());
        assert!(follow(&mut shared).is_empty());
    }
}
