//! The shared state: what every tab reads and no tab changes itself. A tab
//! asks for a change with a [`super::Command`], and update makes it here.
//!
//! It also holds what the Problems list lists: what `vtium check-config`
//! found in the open document ([`Check`]), and the fonts the open document
//! uses, each in one of five states ([`InUse`]). Both are kept with the
//! document they judged, so a change that fixes a problem takes the
//! problem off the list at once, and Undo brings back what the check said
//! of the document as it was.
//!
//! References:
//! - ADR-1206 D2: the model's four parts, and what `shared` holds: the document and its changes not saved, the live console's state, the queue of askers, the pictures, the pending installs and the message line.
//! - ADR-1206 D3, "The undo history lives in `shared`": one history for each document open while vtium-config runs.
//! - ADR-1403 D11, "The frame": the Problems list lists each problem the check finds, and the problems that stop Save come first.
//! - ADR-1205 D4, "Fonts a configuration uses but has not activated": the walk, its five states, and walking again after each change, never on a keypress.

use std::cell::RefCell;
use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use crate::document::{self, Documents};
use crate::drafts::Checked;
use crate::fontuse::Used;

/// What every tab reads.
#[derive(Debug)]
pub struct Shared {
    /// The open document, its undo history, the histories of the
    /// documents opened before it, and the desk its draft is written
    /// through.
    pub documents: Documents,
    /// The live console's state.
    pub console: Console,
    /// The queries sent to the terminal and not yet answered, each with
    /// who asked it.
    pub askers: Askers,
    /// The pictures of the console the font helper draws.
    pub pictures: Pictures,
    /// The installs under way or waiting.
    pub installs: Installs,
    /// The line for the person on the viewer's last row.
    pub message: Option<Message>,
    /// The devices present, which the Flow tab places on the document.
    pub present: crate::flowmodel::Present,
    /// The fonts, effects, color schemes and pointer themes found on the
    /// machine, read at start and again on Read devices again.
    pub found: crate::found::Found,
    /// What the check last found, and in which document.
    pub check: Option<Check>,
    /// The fonts the open document uses, walked once for each state of the
    /// document.
    pub in_use: InUse,
}

impl Shared {
    /// The shared state at start: `documents`, with the devices `present`,
    /// and the terminal not yet asked what it is.
    pub fn new(documents: Documents, present: crate::flowmodel::Present) -> Shared {
        Shared {
            documents,
            console: Console::default(),
            askers: Askers::default(),
            pictures: Pictures::default(),
            installs: Installs::default(),
            message: None,
            present,
            found: crate::found::Found::default(),
            check: None,
            in_use: InUse::default(),
        }
    }

    /// The open document.
    pub fn document(&self) -> &document::Document {
        self.documents.document()
    }

    /// Makes `change` on the open document through the one change
    /// function. A refused change leaves the document as it was, and its
    /// reason goes on the message line too.
    pub fn change(&mut self, change: document::Change) -> Result<document::Done, String> {
        let done = self.documents.change(change);
        if let Err(why) = &done {
            self.say(why.clone());
        }
        done
    }

    /// Ctrl+Z: takes the last change back and says what it undid, or that
    /// there is nothing to undo.
    pub fn undo(&mut self) {
        let said = match self.documents.undo() {
            Ok(Some(step)) => document::words::undid(&step),
            Ok(None) => document::words::NOTHING_TO_UNDO.to_owned(),
            Err(why) => why,
        };
        self.say(said);
    }

    /// Ctrl+Y: makes again the change the last undo took back and says
    /// what it redid, or that there is nothing to redo.
    pub fn redo(&mut self) {
        let said = match self.documents.redo() {
            Ok(Some(step)) => document::words::redid(&step),
            Ok(None) => document::words::NOTHING_TO_REDO.to_owned(),
            Err(why) => why,
        };
        self.say(said);
    }

    /// Puts `line` on the message line, with nothing behind it.
    pub fn say(&mut self, line: String) {
        self.message = Some(Message { line, more: None });
    }

    /// Records `checked`, what `vtium check-config` found in the open
    /// document as it stands now, whose lines name the draft as `shown`.
    /// Whoever performs the desk's [`super::Op::Check`] calls this with
    /// its answer.
    pub fn checked(&mut self, checked: Checked, shown: PathBuf) {
        self.check = Some(Check {
            judged: self.document().clone(),
            shown,
            checked,
        });
    }

    /// What the check found in the open document as it stands now, each
    /// problem once; `None` when the check has not judged the document as
    /// it stands, as before its first run and after a change.
    pub fn findings(&self) -> Option<Vec<Finding>> {
        let check = self.check.as_ref()?;
        (&check.judged == self.document()).then(|| check.findings())
    }

    /// Puts `reads` in place of what the walk of the fonts in use reads
    /// besides the document and the scan, and forgets the last walk. The
    /// loop calls this at start and again after each scan of the font
    /// folders, since the scan's trust marks stand in the walk.
    pub fn read_fonts(&mut self, reads: FontReads) {
        self.in_use = InUse {
            reads: Some(reads),
            walked: RefCell::default(),
        };
    }

    /// The fonts the open document uses, each with its state. The walk
    /// runs again only when the document changed since the last walk, so
    /// a frame drawn after each key reads it without walking. Empty until
    /// [`Shared::read_fonts`] has given the walk what it reads.
    pub fn fonts_used(&self) -> Used {
        let Some(reads) = &self.in_use.reads else {
            return Used::default();
        };
        let document = self.document();
        if let Ok(walked) = self.in_use.walked.try_borrow()
            && let Some(walked) = walked.as_ref().filter(|w| &w.document == document)
        {
            return walked.used.clone();
        }
        let judge = |path: &Path| crate::batch::trusted(path, reads.euid);
        let machine = crate::fontuse::Machine {
            fonts: &self.found.fonts,
            view: &reads.view,
            layers: reads.layers.as_deref(),
            kbd_dirs: &reads.kbd_dirs,
            judge: &judge,
        };
        let used = Used::of(document, &machine);
        if let Ok(mut walked) = self.in_use.walked.try_borrow_mut() {
            *walked = Some(Walked {
                document: document.clone(),
                used: used.clone(),
            });
        }
        used
    }
}

/// What `vtium check-config` found in one state of the open document.
///
/// References:
/// - ADR-1403 D8, "What `install` does": the check, its exit status, and the one problem Install lets the person accept, a configuration that wires no complete route.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    /// The document it judged, as it stood: its name, what it held, and
    /// what was installed beside it.
    judged: document::Document,
    /// The path the check's lines name the draft by.
    shown: PathBuf,
    checked: Checked,
}

impl Check {
    /// The check's answer as it came.
    pub fn checked(&self) -> &Checked {
        &self.checked
    }

    /// Each problem the check counts, in the check's order. A line that
    /// says what the error before it drops goes with that error, as its
    /// `more`. With any problem that stops Save, every problem stops it,
    /// since the check's exit status judges the file and not each line.
    /// A check that finds only fonts this machine lacks, or only that no
    /// complete route is wired, stops nothing: Save saves the one and asks
    /// about the other.
    pub fn findings(&self) -> Vec<Finding> {
        let shown = self.shown.to_string_lossy();
        let checked = &self.checked;
        let stops = !checked.clears() && !checked.no_route_only(&shown);
        let mut out: Vec<Finding> = Vec::new();
        for line in &checked.problems {
            let (file, at, text) = locate(line, &shown);
            if drops(text) {
                if let Some(last) = out.last_mut() {
                    let more = last.more.get_or_insert_with(String::new);
                    if !more.is_empty() {
                        more.push('\n');
                    }
                    more.push_str(text);
                }
                continue;
            }
            let route = file.is_none() && at.is_none() && crate::drafts::says_no_route(text);
            out.push(Finding {
                stops,
                file,
                line: at,
                text: text.to_owned(),
                more: None,
                route,
            });
        }
        out
    }
}

/// Whether `text` is one of vtium's own sentences for what an error drops.
fn drops(text: &str) -> bool {
    vtium_settings::Drops::ALL
        .iter()
        .any(|d| text.ends_with(&d.to_string()))
}

/// Where the check's `line` is: the file it names, `None` for the draft
/// at `shown`, the line of that file, and the problem's words. The check
/// writes `PATH:LINE: words`, or `PATH: words` for the file as a whole. A
/// line in no such form is the draft's, as a whole.
fn locate<'a>(line: &'a str, shown: &str) -> (Option<String>, Option<usize>, &'a str) {
    let (file, rest) = match line.strip_prefix(shown) {
        Some(rest) if rest.starts_with(':') => (None, rest),
        _ => match line.find(": ") {
            Some(at) => {
                let (head, rest) = line.split_at(at);
                match head.rsplit_once(':') {
                    Some((path, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => (
                        Some(path.to_owned()),
                        line.get(path.len()..).unwrap_or(rest),
                    ),
                    _ => (Some(head.to_owned()), rest),
                }
            }
            None => return (None, None, line),
        },
    };
    if let Some(words) = rest.strip_prefix(": ") {
        return (file, None, words);
    }
    let numbered = rest.strip_prefix(':').and_then(|r| r.split_once(": "));
    match numbered {
        Some((n, words)) => match n.parse() {
            Ok(n) => (file, Some(n), words),
            Err(_) => (file, None, rest),
        },
        None => (file, None, rest),
    }
}

/// One problem the check found, as the Problems list and Save's review
/// list it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// Whether the problem stops Save, rather than only warning.
    pub stops: bool,
    /// The file the problem is in where it is not the draft's vtium.conf,
    /// such as a font set's file, by the path the check names.
    pub file: Option<String>,
    /// The line of the file the problem names, `None` for the file as a
    /// whole.
    pub line: Option<usize>,
    /// The problem, in the check's words.
    pub text: String,
    /// What the check says the error drops, from the lines that follow it.
    pub more: Option<String>,
    /// Whether the problem says the configuration wires no complete
    /// route, which the Flow tab is where to fix.
    pub route: bool,
}

/// What the walk of the fonts in use reads besides the document and the
/// scan in [`Shared::found`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontReads {
    /// The batch the system holds, with the packages' and root's lists.
    pub view: crate::batch::View,
    /// The layers' folder beside the font sets, `None` where there is no
    /// font sets' directory.
    pub layers: Option<PathBuf>,
    /// kbd's data folders, where a `kbd:` source is looked up.
    pub kbd_dirs: Vec<PathBuf>,
    /// The user vtium-config runs as, whose trust rule judges a font file
    /// the scan did not find.
    pub euid: u32,
}

/// The fonts the open document uses: what the walk reads, and the last
/// walk with the document it walked.
#[derive(Debug, Default)]
pub struct InUse {
    reads: Option<FontReads>,
    walked: RefCell<Option<Walked>>,
}

impl InUse {
    /// What the walk reads, `None` until the loop has read the batch.
    pub fn reads(&self) -> Option<&FontReads> {
        self.reads.as_ref()
    }
}

/// One walk of the fonts in use, and the document it walked, as it stood.
#[derive(Debug)]
struct Walked {
    document: document::Document,
    used: Used,
}

/// A line on the message line, and the full text behind it, which F1
/// opens, where the line could not say everything.
///
/// References:
/// - ADR-1206 D3, "The navigation module owns F1's help and Find": F1 asks first whether the message line holds a message with more to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub line: String,
    pub more: Option<String>,
}

/// What the TUI found the terminal to be.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Live {
    /// The answers at start have not all come.
    #[default]
    Asking,
    /// A vtium console that answered the font query: font sets are tried
    /// live, and the answer is the font to give back.
    On,
    /// A vtium console with `allow-font-ops` off.
    OpsOff,
    /// Another terminal, which the TUI never asks to switch its font.
    NotVtium,
}

/// The live console's state.
///
/// References:
/// - ADR-1201 D4: the live trial, and finding out what the terminal is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Console {
    pub live: Live,
    /// What the terminal said it is, in its answer to XTVERSION.
    pub version: Option<String>,
    /// The terminal answered the kitty graphics query.
    pub graphics: bool,
    /// The font to give back when the TUI leaves: the answer at start, or
    /// what a save made the console's.
    pub start_font: Option<String>,
    /// The font the console last said it draws.
    pub shown_font: Option<String>,
    /// The font the console is to draw, which a switch is sent for once
    /// no other is in flight: the live trial's, or the one it puts back.
    pub want: Option<String>,
    /// The switch sent whose answer has not come.
    pub in_flight: Option<String>,
    /// The font sets the console refused, each marked in its list.
    pub refused: BTreeSet<String>,
}

/// A query the TUI sends the terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Query {
    /// XTVERSION, which says what the terminal is.
    Version,
    /// OSC 50's query, which names the font in use.
    Font,
    /// kitty graphics' query, which says whether the terminal shows
    /// pictures.
    Graphics,
    /// XTWINOPS 14, the text area's size in pixels.
    TextArea,
    /// DA1, which ends every batch of queries.
    Attributes,
}

/// The part of the TUI that asked a query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asker {
    /// The requests at start, which find out what the terminal is.
    Start,
    /// A font switch while the TUI runs: the live trial's, or the one that
    /// puts the start font back.
    Switch,
    /// The switch that gives the start font back as the TUI leaves.
    Leave,
    /// The question of a vtium console's text area after a resize.
    TextArea,
    /// A tab, by its node's id.
    Tab(&'static str),
}

/// The queue of who asked the terminal what, oldest first. A terminal's
/// reply does not name the query it answers, and every terminal of the
/// VT100 line answers in the order of the queries, so each reply goes to
/// the asker at the front. The shell's live console module sends the
/// queries and settles them.
///
/// References:
/// - ADR-1206 D2, "Replies go back to whoever asked" and "Every batch of queries ends with DA1".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Askers {
    waiting: VecDeque<(Asker, Query)>,
}

impl Askers {
    /// Puts `asker`'s `query` at the back of the queue.
    pub fn push(&mut self, asker: Asker, query: Query) {
        self.waiting.push_back((asker, query));
    }

    /// The oldest query not yet answered, and who asked it.
    pub fn front(&self) -> Option<(Asker, Query)> {
        self.waiting.front().copied()
    }

    /// Takes the oldest query off the queue.
    pub fn pop(&mut self) -> Option<(Asker, Query)> {
        self.waiting.pop_front()
    }

    /// Whether a query is outstanding, while which the decoder keeps an
    /// ESC that ends a read, since it may begin a reply.
    pub fn awaiting(&self) -> bool {
        !self.waiting.is_empty()
    }

    /// Forgets every query, for a terminal that did not answer DA1 by the
    /// deadline at start.
    pub fn clear(&mut self) {
        self.waiting.clear();
    }

    /// How many queries are outstanding.
    pub fn len(&self) -> usize {
        self.waiting.len()
    }

    pub fn is_empty(&self) -> bool {
        self.waiting.is_empty()
    }
}

/// The pictures of the console: one is drawn at a time, and the one asked
/// for last is not asked for again.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pictures {
    /// The picture asked for last.
    pub asked: Option<crate::specimen::Shot>,
    /// A picture is being drawn, so the next is asked for once it comes.
    pub drawing: bool,
    /// The picture on the screen, while one shows.
    pub shown: Option<crate::specimen::Picture>,
    /// Why no picture shows for the one asked for last, if none does.
    pub unpictured: Option<String>,
}

/// The installs under way or waiting.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Installs {
    /// The privileged step is running.
    pub running: bool,
    /// A save that waits for the terminal's answers still due, which the
    /// loop runs once no query is outstanding.
    pub waiting: Option<crate::draft::Plan>,
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use crate::document::{Base, Change, Content, Document, Edit, InMemory, Installed};
    use crate::drafts::Verdict;

    const TEXT: &str = "version = 1\n[look big]\nfnot-size = 11\n";
    const SHOWN: &str = "/home/user/.config/vtium/drafts/north.conf";

    /// The shared state with north open, holding `TEXT`.
    fn north() -> Shared {
        let content = Content {
            text: TEXT.to_owned(),
            base: Base::of(Some(TEXT)),
            ..Content::default()
        };
        let desk = InMemory::new().with_draft("north", content.clone());
        let installed = Installed {
            text: Some(TEXT.to_owned()),
            frits: std::collections::BTreeMap::new(),
        };
        let document = Document::new("north", content, installed);
        Shared::new(
            Documents::new(document, Box::new(desk)),
            crate::flowmodel::Present::default(),
        )
    }

    /// What the check says, in its own words, of `TEXT` at `SHOWN`: the
    /// misspelt key and what that error drops, no display rule, and a
    /// line of a font set's file.
    fn failed() -> Checked {
        let key = vtium_settings::Problem::UnknownKey(vtium_settings::Quoted::path(Path::new(
            "fnot-size",
        )));
        Checked {
            verdict: Verdict::Failed,
            problems: vec![
                format!("{SHOWN}:3: {key}"),
                format!("{SHOWN}:3: {}", vtium_settings::Drops::Line),
                format!("{SHOWN}: {}", vtium_settings::Missing::NoDisplayRule),
                "/etc/vtium/fonts/proof.vtfrit:2: unknown source kind".to_owned(),
            ],
            notes: vec!["a note on standard output".to_owned()],
        }
    }

    #[test]
    fn each_line_the_check_counts_is_a_finding_with_its_place_and_what_it_drops() {
        let mut shared = north();
        assert_eq!(shared.findings(), None, "the check has not run");
        shared.checked(failed(), PathBuf::from(SHOWN));
        let found = shared.findings().unwrap();
        let places: Vec<(Option<&str>, Option<usize>, bool, bool)> = found
            .iter()
            .map(|f| (f.file.as_deref(), f.line, f.stops, f.route))
            .collect();
        assert_eq!(
            places,
            [
                (None, Some(3), true, false),
                (None, None, true, true),
                (Some("/etc/vtium/fonts/proof.vtfrit"), Some(2), true, false),
            ]
        );
        let first = &found[0];
        assert!(
            first.text.starts_with("This section has no key named"),
            "{}",
            first.text
        );
        assert_eq!(
            first.more.as_deref(),
            Some(vtium_settings::Drops::Line.to_string().as_str())
        );
        assert_eq!(found[2].text, "unknown source kind");
    }

    #[test]
    fn missing_fonts_and_a_route_alone_only_warn() {
        let mut shared = north();
        let missing = vtium_settings::fonts_report::missing(
            "proof",
            "JetBrains Mono",
            vtium_settings::fonts_report::Named::ByName,
        );
        let fonts = Checked {
            verdict: Verdict::MissingFonts,
            problems: vec![format!("{SHOWN}:2: {missing}")],
            notes: Vec::new(),
        };
        shared.checked(fonts, PathBuf::from(SHOWN));
        let found = shared.findings().unwrap();
        assert_eq!(found.len(), 1);
        assert!(!found[0].stops && !found[0].route);
        let route = Checked {
            verdict: Verdict::Failed,
            problems: vec![format!(
                "{SHOWN}: {}",
                vtium_settings::Missing::NoDisplayRule
            )],
            notes: Vec::new(),
        };
        shared.checked(route, PathBuf::from(SHOWN));
        let found = shared.findings().unwrap();
        assert_eq!(found.len(), 1);
        assert!(!found[0].stops, "Save asks before it saves with no route");
        assert!(found[0].route);
    }

    #[test]
    fn the_findings_hold_only_for_the_document_the_check_judged() {
        let mut shared = north();
        shared.checked(failed(), PathBuf::from(SHOWN));
        let fixed = TEXT.replace("fnot-size", "font-size");
        shared
            .change(Change::new("the key", Edit::Text(fixed)))
            .unwrap();
        assert_eq!(shared.findings(), None, "the change may have fixed them");
        shared.undo();
        assert_eq!(shared.findings().map(|f| f.len()), Some(3));
        let other = Document::new(
            "other",
            shared.document().content().clone(),
            Installed::default(),
        );
        shared.documents.switch(other);
        assert_eq!(shared.findings(), None, "another configuration");
    }

    #[cfg(feature = "fixture")]
    #[test]
    fn the_fonts_in_use_are_walked_again_after_a_change() {
        use crate::fontuse::State;
        let mut shared = crate::fixture::padnoir::shared();
        let terminus = PathBuf::from("/usr/share/fonts/misc/ter-u16n.otb");
        let emoji = "/usr/local/share/fonts/NotoColorEmoji.ttf";
        let states = |shared: &Shared| -> Vec<(String, State)> {
            shared
                .fonts_used()
                .fonts
                .into_iter()
                .map(|f| (f.named, f.state))
                .collect()
        };
        assert_eq!(
            states(&shared),
            [("font:JetBrains Mono".to_owned(), State::Missing)],
            "the fixture's walk"
        );
        let set = format!(
            "source=outline:{}\nsource=outline:{emoji}\n",
            terminus.display()
        );
        shared
            .change(Change::new(
                "proof",
                Edit::FontSet {
                    frit: "proof".to_owned(),
                    edit: crate::document::FontSetEdit::Keep(set),
                },
            ))
            .unwrap();
        let walked = states(&shared);
        assert_eq!(walked.len(), 2, "{walked:?}");
        assert_eq!(walked[0].1, State::NotActive);
        assert!(matches!(walked[1].1, State::FailsTrust { own: false, .. }));
        shared
            .change(Change::new(
                "activate",
                Edit::Active {
                    path: terminus,
                    change: Some(crate::batch::Change::Activate),
                },
            ))
            .unwrap();
        assert_eq!(states(&shared)[0].1, State::Active);
        shared.undo();
        assert_eq!(states(&shared)[0].1, State::NotActive);
    }
}
