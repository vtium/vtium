//! The conversion of the probes (ADR-1504 D9, slice 3): each stanza of the
//! probe files written as a capability scenario under `scenarios/`, and
//! the proof that each scenario's replay input equals its probe's, bytes
//! and cuts.
//!
//! A scenario takes its probe's name as its file's name and in its
//! `# capability:` line. The comments just above the stanza become its
//! header, with `# spec: ADR-1000 ROWS` from the ROWS column of
//! `capabilities.list`. Every other line is copied, except that a `cmd`
//! line becomes the scenario word for its test command when the word
//! compiles to the same test command, and a placeholder's reason that
//! named kmscon says what vtium lacks instead. vtium's probe of a name
//! takes the place of the copied placeholder of that name, as the probe
//! reader's `merge` has it.
//!
//! `VTIUM_CONFORMANCE_CONVERT=1 cargo test -p vtium-conformance convert --
//! --nocapture` writes the scenarios and prints what became of each `cmd`
//! line. The converter runs once, and git keeps it after it goes.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use super::judge::input;
use super::{Probe, Step, merge, read};
use crate::coverage::{Capability, capabilities};
use crate::scenario::{self, Scenario, keys, quoted};
use crate::spec::Spec;

const TOOL: &str = env!("CARGO_MANIFEST_DIR");
/// The copied probe file, then vtium's own.
const FILES: [&str; 2] = ["probes/probes.replay", "probes/vtium.replay"];

/// The placeholders whose reasons named kmscon, each with the reason that
/// says what vtium lacks. vtium-term keeps OSC 8's links and OSC 133's
/// marks (ADR-1002); the replay interface has no way to show either.
const REASONS: [(&str, &str); 2] = [
    (
        "osc-8-hyperlink",
        "the replay interface has no line that shows a cell's OSC 8 link, so vtium-replay cannot show the links vtium keeps (#40)",
    ),
    (
        "osc-133-marks",
        "the replay interface has no command or line that shows OSC 133's prompt marks, so vtium-replay cannot show the marks vtium keeps (#40)",
    ),
];

/// A stanza as its file wrote it: the comments just above its `probe`
/// line, and its lines after it.
#[derive(Clone, Debug)]
struct Stanza {
    name: String,
    header: Vec<String>,
    body: Vec<String>,
}

/// The stanzas of a probe file. A comment belongs to the stanza below it
/// when no blank line stands between them, so a section's heading, which
/// a blank line follows, belongs to none.
fn stanzas(text: &str) -> Vec<Stanza> {
    let mut out: Vec<Stanza> = Vec::new();
    let mut comments: Vec<String> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix("probe ") {
            out.push(Stanza {
                name: name.trim().to_owned(),
                header: std::mem::take(&mut comments),
                body: Vec::new(),
            });
        } else if trimmed.starts_with('#') {
            comments.push(trimmed.to_owned());
        } else if trimmed.is_empty() {
            comments.clear();
        } else if let Some(s) = out.last_mut() {
            s.body.append(&mut comments);
            s.body.push(trimmed.to_owned());
        }
    }
    out
}

/// The copied stanzas with vtium's: one of vtium's takes the place of the
/// copied placeholder of its name, and any other is added after them.
fn merged(copied: Vec<Stanza>, ours: Vec<Stanza>) -> Vec<Stanza> {
    let mut out = copied;
    for s in ours {
        match out.iter_mut().find(|c| c.name == s.name) {
            Some(c) => {
                assert!(
                    c.body.iter().any(|l| l.starts_with("placeholder ")),
                    "{} replaces a copied probe that is not a placeholder",
                    s.name
                );
                *c = s;
            }
            None => out.push(s),
        }
    }
    out
}

/// The name a `key` line gives `sym`: the character of a printable ASCII
/// key that reads as one word, the key table's name, or `0x` and the
/// keysym in hex.
fn key_name(sym: u32) -> String {
    match char::from_u32(sym) {
        Some(c) if c.is_ascii_graphic() && !matches!(c, '#' | '"') => c.to_string(),
        _ => keys::name_of(sym).map_or_else(|| format!("0x{sym:x}"), str::to_owned),
    }
}

/// The modifiers of the interface's `mods` bits, as `mods=` names them.
fn mod_names(bits: u8) -> Option<String> {
    let names: Vec<&str> = keys::MODS
        .iter()
        .filter(|(_, b)| bits & b != 0)
        .map(|(n, _)| *n)
        .collect();
    let all: u8 = keys::MODS
        .iter()
        .filter(|(_, b)| bits & b != 0)
        .map(|(_, b)| b)
        .sum();
    (all == bits && !names.is_empty()).then(|| names.join("+"))
}

fn unhex(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(hex.get(i..i.saturating_add(2))?, 16).ok())
        .collect()
}

/// The scenario word that may stand for the test command `text`, or None
/// when the language has no word for its command. Whether the word
/// compiles to `text` is [`compiles_to`]'s question.
fn word(text: &str) -> Option<String> {
    let (name, rest) = text.split_once(';').unwrap_or((text, ""));
    let kv: BTreeMap<&str, &str> = rest.split(';').filter_map(|p| p.split_once('=')).collect();
    let get = |k: &str| kv.get(k).copied();
    let mods = |prefix: &str| -> Option<String> {
        match get("mods") {
            Some(m) => Some(format!("{prefix}mods={}", mod_names(m.parse().ok()?)?)),
            None => Some(String::new()),
        }
    };
    Some(match name {
        "key" => {
            let sym = u32::from_str_radix(get("sym")?, 16).ok()?;
            format!("key {}{}", key_name(sym), mods(" ")?)
        }
        "mouse" => {
            let at = format!("at {},{}", get("col")?, get("row")?);
            let px = match (get("px"), get("py")) {
                (Some(x), Some(y)) => format!(" px={x},{y}"),
                _ => String::new(),
            };
            let mods = mods(" ")?;
            match (get("button")?, get("event")?) {
                ("wheel-up", "press") => format!("wheel up {at}{px}{mods}"),
                ("wheel-down", "press") => format!("wheel down {at}{px}{mods}"),
                ("none", "move") => format!("mouse move {at}{px}{mods}"),
                (b, e @ ("press" | "release" | "move")) => format!("mouse {e} {b} {at}{px}{mods}"),
                _ => return None,
            }
        }
        "paste" => format!("paste {}", quoted(&unhex(get("hex")?)?)),
        "focus" => format!(
            "focus {}",
            match get("in")? {
                "1" => "in",
                _ => "out",
            }
        ),
        "palette" => format!("palette {}", get("name")?),
        "resize" => format!("resize {} {}", get("c")?, get("r")?),
        "sb-up" => format!("scroll up {}", get("n")?),
        "sb-down" => format!("scroll down {}", get("n")?),
        "sb-reset" => "scroll bottom".to_owned(),
        "sb-max" => format!("scrollback {}", get("n")?),
        "clipboard" => "clipboard".to_owned(),
        _ => return None,
    })
}

/// The test commands the scenario line `line` compiles to, or why it does
/// not read.
fn compiles_to(line: &str) -> Result<Vec<String>, String> {
    let s = scenario::parse(
        "key-convert",
        "convert",
        &format!("# spec: x\nsize 1 1\n{line}\n"),
    )?;
    Ok(s.commands()
        .last()
        .map(|c| c.tests.clone())
        .unwrap_or_default())
}

/// What became of one `cmd` line.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Cmd {
    /// The word it became.
    Word(String),
    /// It stays, with why.
    Stays(String),
}

/// The line a `cmd` line becomes, and what became of it.
fn convert_cmd(text: &str) -> (String, Cmd) {
    let stays = |why: String| (format!("cmd {text}"), Cmd::Stays(why));
    let Some(word) = word(text) else {
        return stays("the language has no word for it".to_owned());
    };
    match compiles_to(&word) {
        Ok(tests) if tests == [text] => (word.clone(), Cmd::Word(word)),
        Ok(tests) => stays(format!("`{word}` compiles to {tests:?}")),
        Err(e) => stays(format!("`{word}` does not read: {e}")),
    }
}

/// One converted scenario: its name, its text, and what became of each
/// of its `cmd` lines and its placeholder's reason.
#[derive(Clone, Debug)]
struct Converted {
    name: String,
    text: String,
    cmds: Vec<(String, Cmd)>,
    reason: Option<(String, String)>,
}

fn convert(s: &Stanza, cap: &Capability) -> Converted {
    let mut text = String::new();
    for line in &s.header {
        let _ = writeln!(text, "{line}");
    }
    let _ = writeln!(text, "# spec: ADR-1000 {}", cap.rows.join(", "));
    let _ = writeln!(text, "# capability: {}", s.name);
    let mut cmds = Vec::new();
    let mut reason = None;
    for line in &s.body {
        let out = if let Some(cmd) = line.strip_prefix("cmd ") {
            let (out, what) = convert_cmd(cmd);
            cmds.push((cmd.to_owned(), what));
            out
        } else if let Some(why) = line.strip_prefix("placeholder ") {
            let new = REASONS
                .iter()
                .find(|(n, _)| *n == s.name)
                .map_or(why, |(_, r)| r);
            assert!(
                !new.contains("kmscon"),
                "{}: the placeholder's reason names kmscon and has no rewrite",
                s.name
            );
            reason = Some((why.to_owned(), new.to_owned()));
            format!("placeholder {new}")
        } else {
            line.clone()
        };
        let _ = writeln!(text, "{out}");
    }
    Converted {
        name: s.name.clone(),
        text,
        cmds,
        reason,
    }
}

fn read_file(file: &str) -> String {
    std::fs::read_to_string(Path::new(TOOL).join(file)).unwrap_or_else(|e| panic!("{file}: {e}"))
}

fn spec() -> Spec {
    Spec::read(&[
        &read_file("corpus/INTERFACE.md"),
        &read_file("INTERFACE-vtium.md"),
    ])
    .unwrap_or_else(|e| panic!("{e}"))
}

/// Every probe as the probe runner reads it, each with its scenario as the
/// converter writes it.
fn conversion() -> Vec<(Probe, Converted)> {
    let spec = spec();
    let caps = capabilities(&read_file("capabilities.list")).unwrap_or_else(|e| panic!("{e}"));
    let [copied, ours] = FILES.map(read_file);
    let probes = merge(
        read(FILES[0], &copied, &spec).unwrap_or_else(|e| panic!("{e}")),
        read(FILES[1], &ours, &spec).unwrap_or_else(|e| panic!("{e}")),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let stanzas = merged(stanzas(&copied), stanzas(&ours));
    assert_eq!(
        probes.iter().map(|p| &p.name).collect::<Vec<_>>(),
        stanzas.iter().map(|s| &s.name).collect::<Vec<_>>(),
        "the converter's stanzas and the probe reader's probes"
    );
    probes
        .into_iter()
        .zip(&stanzas)
        .map(|(p, s)| {
            let cap = caps
                .iter()
                .find(|c| c.name == s.name)
                .unwrap_or_else(|| panic!("capabilities.list does not name {}", s.name));
            (p, convert(s, cap))
        })
        .collect()
}

/// A probe's steps, with each assertion, example and the size, view,
/// needs and fixtures, as text to compare: a placeholder's reason is left
/// out, since the conversion may rewrite it.
fn shape(p: &Probe) -> Vec<String> {
    let mut out = vec![
        format!("size {:?}", p.size),
        format!("view {}", p.view),
        format!("needs {:?}", p.needs),
        format!("fixtures {:?}", p.fixtures),
        format!("placeholder {}", p.placeholder.is_some()),
    ];
    for (i, (step, _)) in p.steps.iter().enumerate() {
        out.push(match step {
            Step::Send(b) => format!("send {}", quoted(b)),
            Step::Cmd(t) => format!("cmd {t}"),
            Step::Check => "check".to_owned(),
            Step::Want(l) => format!("want {l}"),
            Step::Reject(l) => format!("reject {l}"),
            Step::WantRe(re) => format!("want-re {}", re.as_str()),
            Step::RejectRe(re) => format!("reject-re {}", re.as_str()),
            Step::Reply(b) => format!("reply {}", quoted(b)),
            Step::ReplyRe(_, source) => format!("reply-re {source}"),
        });
        if let Some((example, _)) = p.examples.get(&i) {
            out.push(format!("example {}", quoted(example)));
        }
    }
    out
}

/// Whether the scenario `s` plays what the probe `p` played: the same
/// bytes and cuts, through the probe runner's reading of the scenario and
/// through the scenario's own compiler, and the same assertions at the
/// same checks.
fn proof(p: &Probe, s: &Scenario) -> Result<(), String> {
    let want = input(p);
    let through_runner = input(&s.probe());
    let compiled = s.compile();
    if through_runner != want {
        return Err(format!(
            "{}: the replay input differs from the probe's: bytes {} against {}, cuts {:?} against {:?}",
            s.name,
            quoted(&through_runner.0),
            quoted(&want.0),
            through_runner.1,
            want.1
        ));
    }
    if (compiled.data, compiled.cuts) != want {
        return Err(format!(
            "{}: the scenario compiles to another replay input than its probe's",
            s.name
        ));
    }
    let (a, b) = (shape(p), shape(&s.probe()));
    if let Some((x, y)) = a.iter().zip(&b).find(|(x, y)| x != y) {
        return Err(format!(
            "{}: the probe has {x:?} where the scenario has {y:?}",
            s.name
        ));
    }
    if a.len() != b.len() {
        return Err(format!(
            "{}: the probe has {} steps and the scenario {}",
            s.name,
            a.len(),
            b.len()
        ));
    }
    Ok(())
}

#[test]
fn convert_each_probe_to_a_capability_scenario_that_replays_its_input() {
    let conversion = conversion();
    let write = std::env::var_os("VTIUM_CONFORMANCE_CONVERT").is_some_and(|v| v == "1");
    let mut words = 0;
    let mut stays = 0;
    for (probe, c) in &conversion {
        let file = format!("scenarios/{}.scenario", c.name);
        let s = scenario::parse(&c.name, &file, &c.text).unwrap_or_else(|e| panic!("{e}"));
        proof(probe, &s).unwrap_or_else(|e| panic!("{e}"));
        for (cmd, what) in &c.cmds {
            match what {
                Cmd::Word(w) => {
                    words += 1;
                    println!("word  {}: cmd {cmd} -> {w}", c.name);
                }
                Cmd::Stays(why) => {
                    stays += 1;
                    println!("stays {}: cmd {cmd}: {why}", c.name);
                }
            }
        }
        if let Some((old, new)) = &c.reason {
            println!("placeholder {}: {old:?} -> {new:?}", c.name);
        }
        if write {
            let path = Path::new(TOOL).join(&file);
            std::fs::write(&path, &c.text).unwrap_or_else(|e| panic!("{file}: {e}"));
        }
    }
    println!(
        "{} probes converted, each replaying its probe's bytes and cuts; {words} cmd lines became words and {stays} stayed{}",
        conversion.len(),
        if write {
            "; written under scenarios/"
        } else {
            ""
        }
    );
}
