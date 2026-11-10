//! The parts the Flow view ([`super::flowview`]) offers in PARTS. A
//! part is what a person puts into a path: a display or an input, matched
//! the way a rule matches it; a font set; or a terminal. Each of the four
//! stages of a path has a template, which is never placed in a path.
//! Enter on a template lists the ways to make a part of its kind
//! ([`makes`]), and a choice that filters by a match key lists the values
//! the connected devices give for that key ([`picker`]), with an entry to
//! type a value last.
//!
//! When the view opens, vtium-config makes parts from three sources
//! ([`enumerate`]): every connected device; every device or match the
//! draft's rules name; and the special parts the draft uses, such as "no
//! input" or "the active terminal". When the devices change, it enumerates
//! again and [`merge`]s the result into the parts it holds, by each part's
//! match. A new device adds a part. A part whose device left stays, marked
//! not connected, and becomes normal again when the device comes back.

use super::flow::{Device, Flow, Kind};
use super::flowview::{Sort, sort};
use super::paths::{self, ANY_KEYBOARD, ANY_POINTER, Doc, Piece};
use super::words;

/// The four stages of a path, each with its template, in the order PARTS
/// shows their groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    Display,
    Terminal,
    Input,
    Font,
}

impl Stage {
    pub const ALL: [Stage; 4] = [Stage::Display, Stage::Terminal, Stage::Input, Stage::Font];

    /// The stage a piece fills in a path.
    pub const fn of(piece: &Piece) -> Stage {
        match piece {
            Piece::Display(_) => Stage::Display,
            Piece::Input(_) | Piece::NoInput => Stage::Input,
            Piece::Font(_) => Stage::Font,
            Piece::Active | Piece::NoTerminal | Piece::Terminal(_) => Stage::Terminal,
        }
    }

    /// The template's title.
    pub const fn name(self) -> &'static str {
        match self {
            Stage::Display => "Display",
            Stage::Terminal => "Terminal",
            Stage::Input => "Input",
            Stage::Font => "Font set",
        }
    }
}

/// One part of PARTS.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub piece: Piece,
    /// Its name: the device's title, the value typed, or the special
    /// part's name. A part whose device left keeps the name it had.
    pub title: String,
    /// A device it matches is connected, or it matches no device at all.
    pub connected: bool,
    /// The person made it from a template.
    pub made: bool,
}

/// The name of a part that matches no device: "no input", "the active
/// terminal", "no terminal", a new terminal's name, or a font set's.
pub fn special_title(piece: &Piece) -> String {
    match piece {
        Piece::NoInput => words::NO_INPUT.to_owned(),
        Piece::NoTerminal => words::NO_TERMINAL.to_owned(),
        Piece::Active => words::ACTIVE_TERMINAL.to_owned(),
        Piece::Terminal(n) => n.clone(),
        Piece::Font(f) => f
            .clone()
            .unwrap_or_else(|| words::DEFAULT_FONT_SET.to_owned()),
        Piece::Display(s) | Piece::Input(s) => s.clone(),
    }
}

/// A part's name, as [`paths::title`] gives a display's or an input's.
pub fn title_of(flow: &Flow, piece: &Piece) -> String {
    // a connected device picked by its own match goes by its own name
    let own = |kind: Kind, sel: &str| {
        flow.devices(kind)
            .iter()
            .find(|d| d.found && paths::own_key(flow, kind, d) == sel)
            .map(|d| d.name.clone())
    };
    match piece {
        Piece::Display(s) if let Some(n) = own(Kind::Display, s) => n,
        Piece::Input(s) if let Some(n) = own(Kind::Input, s) => n,
        Piece::Display(s) => paths::title(flow, Kind::Display, s).0,
        Piece::Input(s) => paths::title(flow, Kind::Input, s).0,
        _ => special_title(piece),
    }
}

/// Whether the device `dev` holds every token of the match `sel`, as far
/// as vtium-config can tell from the keys it lists: a path takes the paths
/// below it, a name and a connector are globs, and a udev property, which
/// the list of keys does not carry, is taken to hold.
pub fn matches(dev: &Device, sel: &str) -> bool {
    sel.split_whitespace().all(|t| {
        if t == "*" {
            return true;
        }
        if t == ANY_POINTER {
            return sort(dev) == Sort::Pointer;
        }
        if t == ANY_KEYBOARD {
            return dev
                .keys
                .iter()
                .any(|k| k == "class:keyboard" || k == "class:key");
        }
        if let Some(glob) = t.strip_prefix("name:") {
            return vtium_settings::glob(glob, &dev.name);
        }
        if t.starts_with("udev:") {
            return true;
        }
        dev.keys.iter().any(|k| {
            k == t || (t.starts_with("path:") && k.starts_with(t)) || vtium_settings::glob(t, k)
        })
    })
}

/// Whether the part `piece` is connected: a device it names is, or it
/// names no one device, as "every keyboard" or "no input" do.
pub fn connected(flow: &Flow, piece: &Piece) -> bool {
    let wide = |s: &str| s == "*" || s.starts_with("class:");
    let (kind, sel) = match piece {
        Piece::Display(s) if s != "*" => (Kind::Display, s),
        Piece::Input(s) if !wide(s) => (Kind::Input, s),
        _ => return true,
    };
    flow.devices(kind)
        .iter()
        .any(|d| d.found && matches(d, sel))
}

/// The parts of the draft's flow `flow`, whose configuration is `doc`:
/// each connected display, keyboard and pointer, by the match it is picked
/// by; each display and input the configuration's paths name or labels;
/// and the special parts and font sets the paths use.
pub fn enumerate(flow: &Flow, doc: &Doc) -> Vec<Part> {
    let mut pieces: Vec<Piece> = Vec::new();
    let mut add = |p: Piece| {
        if !pieces.contains(&p) {
            pieces.push(p);
        }
    };
    for d in flow.displays.iter().filter(|d| d.found) {
        add(Piece::Display(paths::own_key(flow, Kind::Display, d)));
    }
    for d in doc.paths.iter().flat_map(|p| &p.displays).chain(&doc.off) {
        add(Piece::Display(d.clone()));
    }
    for d in flow
        .inputs
        .iter()
        .filter(|d| d.found && sort(d) != Sort::Other)
    {
        add(Piece::Input(paths::own_key(flow, Kind::Input, d)));
    }
    let named = doc.paths.iter().flat_map(|p| &p.inputs);
    for i in named.chain(&doc.roaming).chain(&doc.ignored) {
        add(Piece::Input(i.clone()));
    }
    // a labelled part comes back whether or not a rule uses it
    for (kind, sel, _) in &doc.labels {
        add(match kind {
            Kind::Display => Piece::Display(sel.clone()),
            Kind::Input => Piece::Input(sel.clone()),
        });
    }
    if doc.paths.iter().any(|p| p.no_input) {
        add(Piece::NoInput);
    }
    if !doc.roaming.is_empty() {
        add(Piece::Active);
    }
    if !doc.off.is_empty() || !doc.ignored.is_empty() {
        add(Piece::NoTerminal);
    }
    for p in &doc.paths {
        add(Piece::Font(p.font.clone()));
    }
    pieces
        .into_iter()
        .map(|piece| Part {
            title: title_of(flow, &piece),
            connected: connected(flow, &piece),
            piece,
            made: false,
        })
        .collect()
}

/// Merges the parts `now`, enumerated again from `flow`, into the parts
/// `known`, by each part's match: a part is added only for what is new,
/// and every part held is marked connected or not by `flow`. A part keeps
/// its place and, while its device is away, its name.
pub fn merge(known: &mut Vec<Part>, flow: &Flow, now: Vec<Part>) {
    for k in known.iter_mut() {
        k.connected = connected(flow, &k.piece);
        if k.connected && !k.made {
            k.title = title_of(flow, &k.piece);
        }
    }
    for n in now {
        if !known.iter().any(|k| k.piece == n.piece) {
            known.push(n);
        }
    }
}

/// Makes the part `piece` with the name `title`, unless a part with the
/// same match is held already, and says which part it is.
pub fn make(known: &mut Vec<Part>, flow: &Flow, piece: Piece, title: String) -> usize {
    if let Some(at) = known.iter().position(|k| k.piece == piece) {
        return at;
    }
    known.push(Part {
        connected: connected(flow, &piece),
        piece,
        title,
        made: true,
    });
    known.len().saturating_sub(1)
}

/// One way to make a part from a template.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Make {
    /// The part itself, as "every keyboard" or "no input".
    Part(Piece),
    /// A part that matches by this match key, chosen from a picker.
    Filter(&'static str),
    /// A new terminal, whose name, font set and size the editor asks.
    NewTerminal,
}

/// The choices that match by one key, each its key, its label and what it
/// covers: a display's, then an input's.
const FILTERS: [(&str, &str, &str); 11] = [
    (
        "edid",
        "This monitor",
        "One monitor by its EDID, on any port",
    ),
    (
        "connector",
        "Connector",
        "Display on one connector, such as HDMI-A-1",
    ),
    (
        "connector-path",
        "Hub port",
        "Display on one port of a DisplayPort hub",
    ),
    (
        "card",
        "Graphics card",
        "Every display on one graphics card",
    ),
    ("path", "USB port", "Devices plugged into one USB port"),
    (
        "serial",
        "This one device",
        "One device by its serial number, in any port",
    ),
    ("id", "Model", "Every device of one make and model"),
    (
        "bluetooth",
        "Bluetooth device",
        "One paired Bluetooth device",
    ),
    ("name", "Device name", "Devices that report a given name"),
    (
        "class",
        "Device type",
        "Touchscreens, tablets, joysticks and other kinds",
    ),
    (
        "udev",
        "udev property",
        "Devices with a given udev property (advanced)",
    ),
];

/// The keys a display's choices match by; every other key is an input's.
const DISPLAY_KEYS: [&str; 4] = ["edid", "connector", "connector-path", "card"];

/// The ways to make a part of `stage`, each its label, what it covers, and
/// what choosing it does: the whole-kind and special parts first, then the
/// choices that match by one key. `fonts` are the font sets the draft can
/// draw with.
pub fn makes(stage: Stage, fonts: &[String]) -> Vec<(String, String, Make)> {
    let one = |w: &str, d: &str, m: Make| (w.to_owned(), d.to_owned(), m);
    let input = |s: &str| Make::Part(Piece::Input(s.to_owned()));
    let filters = |display: bool| {
        FILTERS
            .iter()
            .filter(move |(key, _, _)| DISPLAY_KEYS.contains(key) == display)
            .map(|(key, label, about)| one(label, about, Make::Filter(key)))
    };
    match stage {
        Stage::Display => std::iter::once(one(
            words::ANY_DISPLAY,
            words::EVERY_DISPLAY,
            Make::Part(Piece::Display("*".to_owned())),
        ))
        .chain(filters(true))
        .collect(),
        Stage::Input => [
            one(
                words::NO_INPUT,
                words::NO_INPUT_DETAIL,
                Make::Part(Piece::NoInput),
            ),
            one(
                words::ANY_KEYBOARD_TITLE,
                words::EVERY_KEYBOARD,
                input(ANY_KEYBOARD),
            ),
            one(
                words::ANY_POINTER_TITLE,
                words::EVERY_POINTER,
                input(ANY_POINTER),
            ),
            one(words::ANY_INPUT_TITLE, words::EVERY_INPUT, input("*")),
        ]
        .into_iter()
        .chain(filters(false))
        .collect(),
        Stage::Terminal => vec![
            one(
                words::NEW_TERMINAL_LABEL,
                words::NEW_TERMINAL_DETAIL,
                Make::NewTerminal,
            ),
            one(
                words::ACTIVE_TERMINAL,
                words::ACTIVE_DETAIL,
                Make::Part(Piece::Active),
            ),
            one(
                words::NO_TERMINAL,
                words::NO_TERMINAL_DETAIL,
                Make::Part(Piece::NoTerminal),
            ),
        ],
        Stage::Font => std::iter::once(one(
            words::DEFAULT_FONT_SET,
            words::DEFAULT_FONT_SET_DETAIL,
            Make::Part(Piece::Font(None)),
        ))
        .chain(
            fonts
                .iter()
                .map(|f| one(f, words::FONT_SET, Make::Part(Piece::Font(Some(f.clone()))))),
        )
        .collect(),
    }
}

/// The label of the choice that matches by `key`, which its picker opens
/// with.
pub fn filter_words(key: &str) -> &'static str {
    FILTERS
        .iter()
        .find(|(k, _, _)| *k == key)
        .map_or("udev property", |(_, label, _)| label)
}

/// One line of a filter's picker: a value with the devices it matches, or
/// the entry where the person types a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    Value { sel: String, devices: Vec<String> },
    Typed,
}

/// The picker of the filter `key` for parts of `stage`: every value the
/// connected devices give for that key, each with the devices it matches,
/// in the order they first come; then the entry to type a value. A USB
/// port is a path without its interface, and a name is written with `?` in
/// each space, as a rule's match holds no space.
pub fn picker(flow: &Flow, stage: Stage, key: &str) -> Vec<Pick> {
    let kind = match stage {
        Stage::Display => Kind::Display,
        _ => Kind::Input,
    };
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let prefix = format!("{key}:");
    for d in flow.devices(kind).iter().filter(|d| d.found) {
        let values: Vec<String> = if key == "name" {
            vec![format!("name:{}", d.name.replace(' ', "?"))]
        } else {
            d.keys
                .iter()
                .filter(|k| k.starts_with(&prefix))
                .map(|k| {
                    if key == "path" {
                        paths::port_of(k)
                    } else {
                        k.clone()
                    }
                })
                .collect()
        };
        for v in values {
            match out.iter_mut().find(|(s, _)| *s == v) {
                Some((_, names)) if !names.contains(&d.name) => names.push(d.name.clone()),
                Some(_) => {}
                None => out.push((v, vec![d.name.clone()])),
            }
        }
    }
    out.into_iter()
        .map(|(sel, devices)| Pick::Value { sel, devices })
        .chain(std::iter::once(Pick::Typed))
        .collect()
}

/// Checks a match the person typed for a part of `stage`, as vtium.conf's
/// parser checks a rule's match, and gives it back with single spaces; or
/// says in a sentence what is wrong with it.
pub fn check_typed(stage: Stage, typed: &str) -> Result<String, String> {
    let sel = typed.split_whitespace().collect::<Vec<_>>().join(" ");
    if sel.is_empty() {
        return Err(words::TYPED_EMPTY.to_owned());
    }
    if sel.contains('#') || sel.contains("->") {
        return Err(words::typed_bad_mark(&sel));
    }
    let kind = match stage {
        Stage::Display => "display",
        _ => "input",
    };
    let text = format!("version = 1\n[head t]\n[rules]\n{kind} {sel} -> t\n");
    let read = vtium_settings::reading(text.as_bytes());
    // the parser's first sentence says what is wrong; a typed value
    // makes a part, so the sentence names the part
    match read.errors.errors.first() {
        Some(e) => {
            let why = e.problem.to_string();
            let first = why.split_inclusive(". ").next().unwrap_or(&why).trim_end();
            Err(first.replacen("A rule cannot", "A part cannot", 1))
        }
        None => Ok(sel),
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::expect_used,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::super::paths::tests::{EDID_A, KBD_PATH, PORT_33, flow};
    use super::*;

    fn values(f: &Flow, stage: Stage, key: &str) -> Vec<(String, Vec<String>)> {
        let all = picker(f, stage, key);
        assert_eq!(
            all.last(),
            Some(&Pick::Typed),
            "{key}: the entry to type is last"
        );
        all.into_iter()
            .filter_map(|p| match p {
                Pick::Value { sel, devices } => Some((sel, devices)),
                Pick::Typed => None,
            })
            .collect()
    }

    fn own(v: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
        v.iter()
            .map(|(s, d)| ((*s).to_owned(), d.iter().map(|x| (*x).to_owned()).collect()))
            .collect()
    }

    #[test]
    fn each_filter_lists_the_values_the_devices_give_and_the_typed_entry_last() {
        let f = flow();
        let r = "Logitech USB Receiver";
        let m = "Logitech USB Receiver Mouse";
        let k = "8BitDo Keyboard";
        let p = "Power Button";
        assert_eq!(
            values(&f, Stage::Input, "path"),
            own(&[
                (PORT_33, &[r, m]),
                ("path:acpi-LNXPWRBN:00", &[p]),
                ("path:pci-0000:00:14.0-usb-0:3.4", &[k]),
            ])
        );
        assert_eq!(
            values(&f, Stage::Input, "id"),
            own(&[("id:046d:c31c", &[r, m]), ("id:2dc8:5200", &[k])])
        );
        assert_eq!(
            values(&f, Stage::Input, "class"),
            own(&[
                ("class:keyboard", &[r, k]),
                ("class:mouse", &[m]),
                ("class:key", &[p]),
            ])
        );
        assert_eq!(
            values(&f, Stage::Input, "name")[0],
            ("name:Logitech?USB?Receiver".to_owned(), vec![r.to_owned()])
        );
        assert_eq!(
            values(&f, Stage::Display, "edid")[0],
            (EDID_A.to_owned(), vec!["DP-8".to_owned()])
        );
        assert_eq!(
            values(&f, Stage::Display, "connector"),
            own(&[
                ("connector:card1/DP-8", &["DP-8"]),
                ("connector:card1/HDMI-A-2", &["HDMI-A-2"])
            ])
        );
        // the demo's devices carry no serial, Bluetooth address or udev
        // property, so those pickers hold the entry to type alone
        for key in ["serial", "bluetooth", "udev"] {
            assert_eq!(values(&f, Stage::Input, key), [], "{key}");
        }
        // every filter a template offers has a picker
        for stage in Stage::ALL {
            for (_, _, m) in makes(stage, &[]) {
                if let Make::Filter(key) = m {
                    assert_eq!(picker(&f, stage, key).last(), Some(&Pick::Typed), "{key}");
                }
            }
        }
    }

    #[test]
    fn a_typed_match_is_checked_as_vtium_conf_checks_a_rule_s_match() {
        assert_eq!(
            check_typed(Stage::Input, "  serial:046d:c52b:0   class:keyboard "),
            Ok("serial:046d:c52b:0 class:keyboard".to_owned())
        );
        assert_eq!(check_typed(Stage::Input, "*"), Ok("*".to_owned()));
        let why = check_typed(Stage::Input, "colour:red").expect_err("an unknown key");
        assert_eq!(why, "A part cannot match on \"colour:red\".");
        let why = check_typed(Stage::Input, "id:zz").expect_err("a bad value");
        assert!(why.contains("id:zz"), "{why}");
        let why = check_typed(Stage::Display, "edid:aaa:1:2").expect_err("a bad EDID");
        assert!(why.contains("edid:aaa:1:2"), "{why}");
        assert_eq!(
            check_typed(Stage::Input, "  "),
            Err(words::TYPED_EMPTY.to_owned())
        );
        assert!(check_typed(Stage::Input, "id:046d:c31c -> desk").is_err());
    }

    #[test]
    fn a_device_plugged_in_adds_one_part_and_its_part_greys_and_comes_back() {
        let mut f = flow();
        let doc = paths::read(&f);
        let mut known = enumerate(&f, &doc);
        let kbd = Piece::Input("id:2dc8:5200".to_owned());
        let count = known.len();
        let at = known
            .iter()
            .position(|p| p.piece == kbd)
            .expect("the 8BitDo's part");
        assert!(known[at].connected);
        assert_eq!(known[at].title, "8BitDo Keyboard");
        // the receiver's keyboard by its path, the unplugged keyboard a rule
        // names, and the receiver by the id its roaming rule names
        assert!(
            known
                .iter()
                .any(|p| p.piece == Piece::Input(KBD_PATH.into()) && p.connected)
        );
        let gone = Piece::Input("serial:046d:c52b:0".into());
        assert!(known.iter().any(|p| p.piece == gone && !p.connected));
        assert!(known.iter().any(|p| p.piece == Piece::Active));
        assert!(known.iter().any(|p| p.piece == Piece::NoTerminal));
        // unplugged: the same part, grey, with its name
        f.inputs.retain(|d| d.name != "8BitDo Keyboard");
        merge(&mut known, &f, enumerate(&f, &paths::read(&f)));
        assert_eq!(known.len(), count);
        assert!(!known[at].connected);
        assert_eq!(known[at].title, "8BitDo Keyboard");
        // plugged in again: the same part, connected, and no second one
        let mut back = flow();
        merge(&mut known, &back, enumerate(&back, &paths::read(&back)));
        assert_eq!(known.len(), count);
        assert!(known[at].connected);
        // a new device adds one part, at the end
        let mut tablet = back.inputs[4].clone();
        tablet.name = "Pen Tablet".into();
        tablet.keys = vec![
            "path:pci-0000:00:14.0-usb-0:3.5:1.0".into(),
            "id:056a:0001".into(),
            "class:tablet".into(),
        ];
        back.inputs.push(tablet);
        merge(&mut known, &back, enumerate(&back, &paths::read(&back)));
        assert_eq!(known.len(), count + 1);
        assert_eq!(known[count].title, "Pen Tablet");
        // a part the person made stays when nothing names it
        let made = make(
            &mut known,
            &back,
            Piece::Input("name:Pen*".into()),
            "name:Pen*".into(),
        );
        merge(
            &mut known,
            &flow(),
            enumerate(&flow(), &paths::read(&flow())),
        );
        assert!(known[made].made && !known[made].connected);
        assert_eq!(
            make(
                &mut known,
                &back,
                Piece::Input("name:Pen*".into()),
                String::new()
            ),
            made
        );
    }

    #[test]
    fn the_templates_offer_the_ways_to_make_each_kind_of_part() {
        let words = |s: Stage| -> Vec<String> {
            makes(s, &["monoid".to_owned()])
                .into_iter()
                .map(|(w, _, _)| w)
                .collect()
        };
        assert_eq!(
            words(Stage::Display),
            [
                "All displays",
                "This monitor",
                "Connector",
                "Hub port",
                "Graphics card"
            ]
        );
        assert_eq!(
            words(Stage::Input),
            [
                "No input",
                "All keyboards",
                "All mice and touchpads",
                "All input devices",
                "USB port",
                "This one device",
                "Model",
                "Bluetooth device",
                "Device name",
                "Device type",
                "udev property",
            ]
        );
        assert_eq!(
            words(Stage::Terminal),
            ["New terminal…", "Active terminal", "Nowhere"]
        );
        assert_eq!(words(Stage::Font), ["default font set", "monoid"]);
    }

    // The maintainer's try-out accepted a table of labels and
    // descriptions: a noun phrase for each, with no leading article
    #[test]
    fn each_choice_is_described_by_the_accepted_table() {
        let about =
            |s: Stage| -> Vec<String> { makes(s, &[]).into_iter().map(|(_, d, _)| d).collect() };
        assert_eq!(
            about(Stage::Display),
            [
                "Every display, connected now or later",
                "One monitor by its EDID, on any port",
                "Display on one connector, such as HDMI-A-1",
                "Display on one port of a DisplayPort hub",
                "Every display on one graphics card",
            ]
        );
        assert_eq!(
            about(Stage::Input),
            [
                "Display only, with no keyboard or mouse",
                "Every keyboard, connected now or later",
                "Mice, touchpads and trackballs",
                "Keyboards, pointers and every other kind of input",
                "Devices plugged into one USB port",
                "One device by its serial number, in any port",
                "Every device of one make and model",
                "One paired Bluetooth device",
                "Devices that report a given name",
                "Touchscreens, tablets, joysticks and other kinds",
                "Devices with a given udev property (advanced)",
            ]
        );
        assert_eq!(
            about(Stage::Terminal),
            [
                "Separate terminal with its own consoles and look",
                "Terminal on screen, following you as you switch",
                "Displays off, inputs ignored",
            ]
        );
    }
}
