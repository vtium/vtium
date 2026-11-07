//! The scenario language (ADR-1504) on scenarios written here: the parser
//! on good and bad files, the compiler's replay input for each command,
//! the transcript printer on blocks written here, the judge's verdicts,
//! and the capability rules on lists and scenarios written here.

use std::path::{Path, PathBuf};

use vtium_conformance::coverage::Capability;
use vtium_conformance::probes::Outcome;
use vtium_conformance::scenario::run::Runner;
use vtium_conformance::scenario::{self, Dump, Sort, capability, transcript};
use vtium_conformance::spec::Spec;

const TOOL: &str = env!("CARGO_MANIFEST_DIR");

fn read(path: &str) -> String {
    std::fs::read_to_string(Path::new(TOOL).join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn spec() -> Spec {
    Spec::read(&[&read("corpus/INTERFACE.md"), &read("INTERFACE-vtium.md")])
        .unwrap_or_else(|e| panic!("{e}"))
}

fn fixtures() -> PathBuf {
    Path::new(TOOL).join("fixtures")
}

fn parse(name: &str, text: &str) -> scenario::Scenario {
    scenario::parse(name, &format!("{name}.scenario"), text).unwrap_or_else(|e| panic!("{e}"))
}

fn refusal(name: &str, text: &str) -> String {
    match scenario::parse(name, &format!("{name}.scenario"), text) {
        Ok(_) => panic!("{name} parsed:\n{text}"),
        Err(e) => e,
    }
}

/// ADR-1504 D2's behaviour scenario.
const CURSOR: &str = concat!(
    "# Cursor keys in normal and application mode, with Shift.
# spec: ctlseqs, PC-Style Function Keys; DECCKM
# covers",
    ": DEC-1
size 10 2
key Up
send \"\\e[?1h\"
key Up
key Up mods=shift
send \"\\e[?1$p\"
dump
"
);

/// ADR-1504 D3's transcript of [`CURSOR`].
const CURSOR_TRANSCRIPT: &str = "> size 10 2
> key Up
< reply \"\\e[A\"
> send \"\\e[?1h\"
> key Up
< reply \"\\eOA\"
> key Up mods=shift
< reply \"\\e[1;2A\"
> send \"\\e[?1$p\"
< reply \"\\e[?1;1$y\"
> dump
size 10x2
cursor 0,0
flags AUTO_WRAP
row 0 ||
row 1 ||
";

/// ADR-1504 D2's capability scenario.
const BRACKETED: &str = "# spec: ADR-1000 DEC-2004
# capability: bracketed-paste
size 80 4
send \"\\e[?2004h\"
check
paste \"x\"
check
reply \"\\e[200~x\\e[201~\"
";

#[test]
fn a_behaviour_scenario_reads_with_its_header_and_echoes_each_command() {
    let s = parse("key-cursor-decckm", CURSOR);
    assert_eq!(s.sort, Sort::Behaviour);
    assert_eq!(s.spec, "ctlseqs, PC-Style Function Keys; DECCKM");
    assert_eq!(s.row_ids, ["DEC-1"]);
    assert_eq!((s.cols, s.rows), (10, 2));
    let echoes: Vec<&str> = s.commands().map(|c| c.echo.as_str()).collect();
    assert_eq!(
        echoes,
        [
            "size 10 2",
            "key Up",
            r#"send "\e[?1h""#,
            "key Up",
            "key Up mods=shift",
            r#"send "\e[?1$p""#,
            "dump"
        ]
    );
}

#[test]
fn a_command_echoes_without_its_comment_and_with_its_spaces_collapsed_outside_strings() {
    let s = parse(
        "key-echo",
        "# spec: x\nsize 4 1\nsend   \"a  # b\"   # a comment\nkey   a    mods=ctrl\n",
    );
    let echoes: Vec<&str> = s.commands().map(|c| c.echo.as_str()).collect();
    assert_eq!(echoes, ["size 4 1", r#"send "a  # b""#, "key a mods=ctrl"]);
}

#[test]
fn a_capability_scenario_names_its_capability_and_no_rows() {
    let s = parse("bracketed-paste", BRACKETED);
    assert_eq!(s.sort, Sort::Capability("bracketed-paste".to_owned()));
    assert!(s.row_ids.is_empty());
}

#[test]
fn the_parser_refuses_bad_files_and_says_what_a_good_one_has() {
    let cases: &[(&str, &str, &str)] = &[
        ("key-a", "size 4 1\n", "has no `# spec:` line"),
        ("key-a", "# spec: x\nkey Up\n", "The first command is `key`"),
        (
            "key-a",
            "# spec: x\nsize 4 1\nsize 4 1\n",
            "second `size` line",
        ),
        (
            "key-a",
            "# spec: x\nsize 0 1\n",
            "outside the replay's bounds",
        ),
        (
            "key-a",
            "# spec: x\nsize 4 1\nkye Up\n",
            "`kye` is not a command",
        ),
        (
            "key-a",
            "# spec: x\nsize 4 1\nkey Upp\n",
            "`Upp` is not a key name",
        ),
        (
            "key-a",
            "# spec: x\nsize 4 1\nkey Up mods=meta\n",
            "does not name modifiers",
        ),
        (
            "key-a",
            "# spec: x\nsize 4 1\nkey Up mods=ctrl mods=alt\n",
            "gives mods= twice",
        ),
        (
            "key-a",
            "# spec: x\nsize 4 1\nsend \"open\n",
            "has no closing quote",
        ),
        (
            "key-a",
            "# spec: x\nsize 4 1\nsend \"\\q\"\n",
            "takes one string",
        ),
        (
            "key-a",
            "# spec: x\nsize 4 1\nsend \"a\" \"b\"\n",
            "has 2 words",
        ),
        (
            "key-a",
            "# spec: x\nsize 4 1\ntype \"a\\rb\"\n",
            "control character",
        ),
        (
            "mouse-a",
            "# spec: x\nsize 4 1\nmouse press left 1,0\n",
            "has no `at COL,ROW`",
        ),
        (
            "mouse-a",
            "# spec: x\nsize 4 1\nmouse press wheel at 1,0\n",
            "reads neither",
        ),
        (
            "mouse-a",
            "# spec: x\nsize 4 1\nmouse press left at 1,0 px=70000,0\n",
            "is not a pixel",
        ),
        (
            "paste-a",
            "# spec: x\nsize 4 1\npaste \"a\\x00\"\n",
            "holds the byte 00",
        ),
        (
            "palette-a",
            "# spec: x\nsize 4 1\npalette mauve\n",
            "not a palette vtium has",
        ),
        (
            "key-a",
            "# spec: x\nsize 4 1\nsend \"a\"\nview frame\n",
            "comes too late",
        ),
        (
            "key-a",
            "# spec: x\nsize 4 1\ndump screen screen\n",
            "names screen twice",
        ),
        (
            "key-a",
            "# spec: x\nsize 4 1\ndump picture\n",
            "`picture` is none of them",
        ),
        (
            "key-a",
            "# spec: x\nsize 4 1\nsend \"a\"\nwant row 0 |a|\n",
            "has no `check` before it",
        ),
        (
            "key-a",
            "# spec: x\nsize 4 1\ncheck\nsend \"a\"\nwant row 0 |a|\n",
            "`send \"a\"` comes after the last check",
        ),
        (
            "key-a",
            "# spec: x\nsize 4 1\ncheck\nwant row 0 |a|\nexample \"x\"\n",
            "does not follow a `want-re`",
        ),
        (
            "key-a",
            "# spec: x\nsize 4 1\ncheck\nwant-re row (\n",
            "does not compile",
        ),
        (
            "key-a",
            "# spec: x\nplaceholder vtium lacks it\n",
            "has a `placeholder`",
        ),
        (
            "key-a",
            "# spec: x\nsize 4 1\n# spec: y\n",
            "comes after the first command",
        ),
        (
            "keys-a",
            "# spec: x\nsize 4 1\n",
            "does not start with an area",
        ),
        (
            "Key-a",
            "# spec: x\nsize 4 1\n",
            "does not start with an area",
        ),
        (
            "key",
            "# spec: x\nsize 4 1\n",
            "does not start with an area",
        ),
        (
            "da1",
            concat!(
                "# spec: x\n# capability: da1\n# covers",
                ": ANSI-4\nsize 4 1\ncheck\nreply \"x\"\n"
            ),
            "has a `# covers` line",
        ),
        (
            "da1",
            "# spec: x\n# capability: da2\nsize 4 1\ncheck\nreply \"x\"\n",
            "declares the capability da2",
        ),
        (
            "da1",
            "# spec: x\n# capability: da1\nsize 4 1\ncheck\nreject row 0 |a|\n",
            "nothing vtium prints could fail it",
        ),
        (
            "da1",
            "# spec: x\n# capability: da1\nsize 4 1\ndump\ncheck\nreply \"x\"\n",
            "has a `dump`",
        ),
        (
            "da1",
            "# spec: x\n# capability: da1\nsize 4 1\nplaceholder vtium lacks it\n",
            "has commands",
        ),
        (
            "key-a",
            "# spec: x\nsize 4 1\nkey #\n",
            "Write the key # as `key numbersign`",
        ),
    ];
    for (name, text, want) in cases {
        let e = refusal(name, text);
        assert!(e.contains(want), "{name}: {text:?}\nwant {want:?} in:\n{e}");
        assert!(
            e.starts_with(&format!("{name}.scenario")),
            "{e}: names the file first"
        );
    }
}

#[test]
fn a_placeholder_capability_scenario_has_nothing_to_run() {
    let s = parse(
        "kitty-keyboard",
        "# spec: x\n# capability: kitty-keyboard\nplaceholder vtium has no kitty keyboard protocol\n",
    );
    assert_eq!(
        s.placeholder.as_deref(),
        Some("vtium has no kitty keyboard protocol")
    );
}

/// The test command each command compiles to.
fn tests_of(line: &str) -> Vec<String> {
    let s = parse("key-c", &format!("# spec: x\nsize 4 2\n{line}\n"));
    s.commands()
        .nth(1)
        .map(|c| c.tests.clone())
        .unwrap_or_default()
}

#[test]
fn each_command_compiles_to_its_test_command() {
    let cases: &[(&str, &[&str])] = &[
        ("key Up", &["key;sym=ff52"]),
        ("key Up mods=shift", &["key;sym=ff52;mods=1"]),
        ("key F5 mods=ctrl+alt", &["key;sym=ffc2;mods=12"]),
        ("key KP_Enter", &["key;sym=ff8d"]),
        ("key a mods=alt", &["key;sym=61;ascii=61;unicode=61;mods=8"]),
        (
            "key 1 mods=ctrl",
            &["key;sym=31;ascii=31;unicode=31;mods=4"],
        ),
        ("key space", &["key;sym=20;ascii=20;unicode=20"]),
        (
            "key Cyrillic_es mods=ctrl ascii=c",
            &["key;sym=6d3;ascii=63;unicode=441;mods=4"],
        ),
        ("key 0x1000441", &["key;sym=1000441;unicode=441"]),
        ("key a char=none", &["key;sym=61;ascii=61"]),
        ("key a char=\"с\"", &["key;sym=61;ascii=61;unicode=441"]),
        (
            "key BackSpace mods=shift+lock+ctrl+alt+logo",
            &["key;sym=ff08;mods=31"],
        ),
        (
            "type \"aZ\"",
            &[
                "key;sym=61;ascii=61;unicode=61",
                "key;sym=5a;ascii=5a;unicode=5a",
            ],
        ),
        ("type \"с\"", &["key;sym=1000441;unicode=441"]),
        (
            "mouse press left at 2,1",
            &["mouse;col=2;row=1;button=left;event=press"],
        ),
        (
            "mouse release right at 0,0 px=4,8 mods=shift",
            &["mouse;col=0;row=0;px=4;py=8;button=right;event=release;mods=1"],
        ),
        (
            "mouse move at 3,1",
            &["mouse;col=3;row=1;button=none;event=move"],
        ),
        (
            "mouse move middle at 3,1",
            &["mouse;col=3;row=1;button=middle;event=move"],
        ),
        (
            "wheel up at 1,1",
            &["mouse;col=1;row=1;button=wheel-up;event=press"],
        ),
        (
            "wheel down at 1,1 mods=ctrl",
            &["mouse;col=1;row=1;button=wheel-down;event=press;mods=4"],
        ),
        ("paste \"a\\nb\"", &["paste;hex=610a62"]),
        ("focus in", &["focus;in=1"]),
        ("focus out", &["focus;in=0"]),
        ("select start at 1,0", &["select;how=start;col=1;row=0"]),
        ("select word at 2,1", &["select;how=word;col=2;row=1"]),
        ("select clear", &["select;how=clear"]),
        ("copy", &["copy"]),
        ("clipboard", &["clipboard"]),
        ("scroll up 3", &["sb-up;n=3"]),
        ("scroll down 1", &["sb-down;n=1"]),
        ("scroll bottom", &["sb-reset"]),
        ("scroll prompt-up", &["prompt;dir=up"]),
        ("scroll prompt-down", &["prompt;dir=down"]),
        ("scrollback 100", &["sb-max;n=100"]),
        ("resize 20 4", &["resize;c=20;r=4"]),
        ("palette solarized", &["palette;name=solarized"]),
        ("backspace del", &["backspace;del=1"]),
        ("backspace bs", &["backspace;del=0"]),
        ("cmd version;text=x", &["version;text=x"]),
        ("dump state", &["state"]),
        ("dump", &[]),
    ];
    for (line, want) in cases {
        assert_eq!(tests_of(line), *want, "{line}");
    }
}

#[test]
fn a_behaviour_scenario_cuts_after_each_command() {
    let s = parse("key-cursor-decckm", CURSOR);
    let c = s.compile();
    let mut want = Vec::new();
    want.extend_from_slice(b"\x1b_tsm-test;key;sym=ff52\x1b\\");
    let after_first_key = want.len();
    want.extend_from_slice(b"\x1b[?1h");
    let after_send = want.len();
    want.extend_from_slice(b"\x1b_tsm-test;key;sym=ff52\x1b\\");
    let after_second = want.len();
    want.extend_from_slice(b"\x1b_tsm-test;key;sym=ff52;mods=1\x1b\\");
    let after_shift = want.len();
    want.extend_from_slice(b"\x1b[?1$p");
    assert_eq!(c.data, want);
    let end = want.len();
    assert_eq!(
        c.cuts,
        [
            0,
            after_first_key,
            after_send,
            after_second,
            after_shift,
            end,
            end
        ]
    );
    assert_eq!(c.command_cuts, [0, 1, 2, 3, 4, 5, 6]);
}

#[test]
fn a_capability_scenario_cuts_at_each_check_and_nowhere_else() {
    let s = parse("bracketed-paste", BRACKETED);
    let c = s.compile();
    assert_eq!(
        c.data,
        b"\x1b[?2004h\x1b_tsm-test;paste;hex=78\x1b\\".to_vec()
    );
    assert_eq!(c.cuts, [8, c.data.len()]);
    assert!(c.command_cuts.is_empty());
}

/// [`CURSOR`]'s cuts, each its command's.
fn cursor_tags() -> Vec<Option<usize>> {
    (0..7).map(Some).collect()
}

/// A block of the screen view at 10 by 2, empty, with `between` after its
/// head.
fn block(cut: usize, cursor: &str, between: &str) -> String {
    format!(
        "checkpoint {cut}\nsize 10x2\ncursor {cursor}\nflags AUTO_WRAP\n{between}row 0 ||\nrow 1 ||\n"
    )
}

/// What the replay prints for [`CURSOR`] in the screen view, written here.
fn cursor_output() -> String {
    [
        block(0, "0,0", ""),
        block(26, "0,0", "reply 1b5b41\n"),
        block(31, "0,0", ""),
        block(57, "0,0", "reply 1b4f41\n"),
        block(90, "0,0", "reply 1b5b313b3241\n"),
        block(96, "0,0", "reply 1b5b3f313b312479\n"),
        block(96, "0,0", ""),
        block(96, "0,0", ""),
    ]
    .concat()
}

#[test]
fn the_printer_writes_d3s_transcript_from_blocks() {
    let s = parse("key-cursor-decckm", CURSOR);
    let out = cursor_output();
    let got = transcript::print(&s, &cursor_tags(), &[(Dump::Screen, &out)])
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(got, CURSOR_TRANSCRIPT);
}

#[test]
fn the_printer_joins_a_reply_that_an_extra_cut_split() {
    let s = parse(
        "reply-split",
        "# spec: x\nsize 10 2\nsend \"\\e[c\\e[5n\"\n",
    );
    let out = [
        block(0, "0,0", ""),
        block(3, "0,0", "reply 1b5b3f36323b3463\n"),
        block(7, "0,0", "reply 1b5b306e\n"),
        block(7, "0,0", ""),
    ]
    .concat();
    let got = transcript::print(&s, &[Some(0), None, Some(1)], &[(Dump::Screen, &out)])
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        got,
        "> size 10 2\n> send \"\\e[c\\e[5n\"\n< reply \"\\e[?62;4c\\e[0n\"\n"
    );
}

#[test]
fn the_printer_writes_each_side_effect_in_its_own_form() {
    let s = parse(
        "reply-effects",
        "# spec: x\nsize 10 2\nclipboard\nclipboard\ncmd version;text=x\nsend \"\\e[c\"\n",
    );
    let out = [
        block(0, "0,0", ""),
        format!("test clipboard 68c3a9\n{}", block(20, "0,0", "")),
        format!("test clipboard none\n{}", block(40, "0,0", "")),
        format!("test version error -22\n{}", block(70, "0,0", "")),
        block(
            73,
            "0,0",
            "reply 1b5b3f3663\nhost: png 0 fixture.png\nreply 41\n",
        ),
        block(73, "0,0", ""),
    ]
    .concat();
    let tags: Vec<Option<usize>> = (0..5).map(Some).collect();
    let got =
        transcript::print(&s, &tags, &[(Dump::Screen, &out)]).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        got,
        "> size 10 2\n> clipboard\n< clipboard \"h\\xc3\\xa9\"\n> clipboard\n< clipboard none\n> cmd version;text=x\n< error -22\n> send \"\\e[c\"\n< reply \"\\e[?6c\"\n< host: png 0 fixture.png\n< reply \"A\"\n"
    );
}

#[test]
fn a_dump_prints_each_view_it_names_masked_and_without_its_replies() {
    let s = parse(
        "view-both",
        "# spec: x\nsize 10 2\nsend \"\\e[c\"\ndump frame screen\n",
    );
    let screen = [
        block(0, "0,0", ""),
        block(3, "0,0", "reply 41\n"),
        block(3, "0,0", ""),
        block(3, "0,0", ""),
    ]
    .concat();
    let frame_block = |cut: usize, changed: usize, reply: &str| {
        format!(
            "checkpoint {cut}\nsize 10x2\ncursor 0,0\nflags AUTO_WRAP\n{reply}palette legacy fg=#e5e5e5 bg=#000000\nchanged {changed}\nframe-cursor 0,0 default visible w=1 fg=#000000 bg=#e5e5e5 ul=#000000\nrow 0 ||\nrow 1 ||\n"
        )
    };
    let frame = [
        frame_block(0, 20, ""),
        frame_block(3, 0, "reply 41\n"),
        frame_block(3, 7, ""),
        frame_block(3, 0, ""),
    ]
    .concat();
    let tags = [Some(0), Some(1), Some(2)];
    let got = transcript::print(&s, &tags, &[(Dump::Screen, &screen), (Dump::Frame, &frame)])
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        got,
        "> size 10 2\n> send \"\\e[c\"\n< reply \"A\"\n> dump frame screen\nsize 10x2\ncursor 0,0\nflags AUTO_WRAP\npalette legacy fg=#e5e5e5 bg=#000000\nchanged N\nframe-cursor 0,0 default visible w=1 fg=#000000 bg=#e5e5e5 ul=#000000\nrow 0 ||\nrow 1 ||\nsize 10x2\ncursor 0,0\nflags AUTO_WRAP\nrow 0 ||\nrow 1 ||\n"
    );
}

#[test]
fn the_printer_refuses_blocks_that_do_not_match_the_cuts() {
    let s = parse("key-cursor-decckm", CURSOR);
    let out = block(0, "0,0", "");
    let e = transcript::print(&s, &cursor_tags(), &[(Dump::Screen, &out)])
        .err()
        .unwrap_or_default();
    assert!(e.contains("printed 1 blocks"), "{e}");
}

fn runner(spec: &Spec) -> Runner<'_> {
    Runner::new(spec, &vtium_conformance::describe(), fixtures()).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn played_through_vtium_d2s_scenario_prints_d3s_transcript() {
    let spec = spec();
    let s = parse("key-cursor-decckm", CURSOR);
    let played = runner(&spec)
        .play(&s, &[])
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(played.transcript, CURSOR_TRANSCRIPT);
    assert_eq!(played.assertions, Outcome::Pass);
    runner(&spec)
        .chunking(&s, 7, 4)
        .unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn a_difference_names_the_command_and_the_reply_it_expected_beside_the_one_it_got() {
    let got = CURSOR_TRANSCRIPT.replace(r#"< reply "\e[1;2A""#, r#"< reply "\e[1;5A""#);
    let why = transcript::differs(CURSOR_TRANSCRIPT, &got).unwrap_or_default();
    assert_eq!(
        why,
        "The transcript differs after `key Up mods=shift`.\n    want < reply \"\\e[1;2A\"\n    got  < reply \"\\e[1;5A\""
    );
    assert_eq!(
        transcript::differs(CURSOR_TRANSCRIPT, CURSOR_TRANSCRIPT),
        None
    );
    let short = CURSOR_TRANSCRIPT.replace("> key Up mods=shift\n< reply \"\\e[1;2A\"\n", "");
    let why = transcript::differs(CURSOR_TRANSCRIPT, &short).unwrap_or_default();
    assert!(
        why.starts_with("The transcript has `key Up mods=shift` where the scenario now has `send"),
        "{why}"
    );
}

#[test]
fn a_behaviour_scenarios_assertions_judge_the_block_at_the_check_with_the_replies_since_the_last() {
    let spec = spec();
    let r = runner(&spec);
    let pass = parse(
        "reply-da",
        "# spec: x\nsize 4 1\nsend \"ab\"\nsend \"\\e[c\"\ncheck\nwant row 0 |ab|\nreply-re \"\\e\\\\[\\\\?6[0-9;]*c\"\nsend \"\\e[5n\"\ncheck\nreply \"\\e[0n\"\n",
    );
    assert_eq!(r.play(&pass, &[]).map(|p| p.assertions), Ok(Outcome::Pass));
    let fail = parse(
        "reply-da",
        "# spec: x\nsize 4 1\nsend \"ab\"\ncheck\nwant row 0 |abc|\n",
    );
    let got = r.play(&fail, &[]).map(|p| p.assertions);
    assert!(
        matches!(&got, Ok(Outcome::Fail(why)) if why.contains("no \"row 0 |abc|\"; it has \"row 0 |ab|\"")),
        "{got:?}"
    );
    let wrong = parse(
        "reply-dsr",
        "# spec: x\nsize 4 1\nsend \"\\e[5n\"\ncheck\nreply \"\\e[3n\"\n",
    );
    assert!(matches!(
        r.play(&wrong, &[]).map(|p| p.assertions),
        Ok(Outcome::Fail(_))
    ));
}

#[test]
fn a_scenario_that_needs_what_vtium_replay_lacks_is_not_run() {
    let spec = spec();
    let r = runner(&spec);
    let cases = [
        (
            "select-a",
            "select start at 0,0",
            "does not declare the feature selection",
        ),
        ("view-a", "dump state", "does not declare the feature state"),
        (
            "key-a",
            "needs damage-exact",
            "does not declare the feature damage-exact",
        ),
        (
            "key-a",
            "cmd nonsense;x=1",
            "offers no test command nonsense",
        ),
        ("bidi-a", "dump display", "has no display view"),
    ];
    for (name, line, want) in cases {
        let s = parse(name, &format!("# spec: x\nsize 4 1\n{line}\n"));
        let got = r.play(&s, &[]);
        assert!(
            matches!(&got, Err(e) if e.starts_with("not run: ") && e.contains(want)),
            "{line}: {got:?}"
        );
    }
    let s = parse("key-a", "# spec: x\nsize 4 1\nfixture no-such.termcap\n");
    assert!(
        matches!(r.play(&s, &[]), Err(e) if e.starts_with("not judged: the fixture no-such.termcap"))
    );
}

#[test]
fn the_interface_check_reads_each_dump_of_a_transcript_against_the_grammar() {
    let spec = spec();
    let s = parse("key-cursor-decckm", CURSOR);
    assert_eq!(transcript::check(&spec, &s, CURSOR_TRANSCRIPT), None);
    let bad = CURSOR_TRANSCRIPT.replace("cursor 0,0", "cursor zero");
    let fault = transcript::check(&spec, &s, &bad).unwrap_or_default();
    assert!(fault.contains("not a cursor line"), "{fault}");
    let stray = CURSOR_TRANSCRIPT.replace("> key Up\n< reply \"\\e[A\"\n", "> key Up\nrow 0 ||\n");
    let fault = transcript::check(&spec, &s, &stray).unwrap_or_default();
    assert!(fault.contains("neither a side effect"), "{fault}");
    let numbering = CURSOR_TRANSCRIPT.replace("row 1 ||\n", "");
    assert!(transcript::check(&spec, &s, &numbering).is_some());
}

fn cap(name: &str, status: &str, issue: &str) -> Capability {
    Capability {
        name: name.to_owned(),
        status: status.to_owned(),
        issue: issue.to_owned(),
        rows: Vec::new(),
        note: String::new(),
    }
}

fn capability_scenario(name: &str, body: &str) -> scenario::Scenario {
    parse(name, &format!("# spec: x\n# capability: {name}\n{body}"))
}

const DSR: &str = "size 4 1\nsend \"\\e[5n\"\ncheck\nreply \"\\e[0n\"\n";

#[test]
fn a_working_capability_passes_and_a_missing_one_that_passes_fails_the_run() {
    let spec = spec();
    let r = runner(&spec);
    let s = [capability_scenario("dsr", DSR)];
    let works = capability::judge(&[cap("dsr", "works", "-")], &s, &r, &[])
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        (works.failures, works.lines.as_slice()),
        (0, ["ok   dsr: works".to_owned()].as_slice())
    );
    let missing = capability::judge(&[cap("dsr", "missing", "#1")], &s, &r, &[])
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(missing.failures, 1);
    assert!(
        missing.lines[0].contains("listed as missing (#1) but passes"),
        "{:?}",
        missing.lines
    );
}

#[test]
fn a_working_capability_whose_assertion_fails_or_that_is_a_placeholder_fails_the_run() {
    let spec = spec();
    let r = runner(&spec);
    let wrong = [capability_scenario(
        "dsr",
        "size 4 1\nsend \"\\e[5n\"\ncheck\nreply \"\\e[3n\"\n",
    )];
    let report = capability::judge(&[cap("dsr", "works", "-")], &wrong, &r, &[])
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(report.failures, 1);
    assert!(
        report.lines[0].starts_with("FAIL dsr: listed as working, but"),
        "{:?}",
        report.lines
    );
    let placeholder = [capability_scenario(
        "dsr",
        "placeholder vtium cannot see it\n",
    )];
    let report = capability::judge(&[cap("dsr", "works", "-")], &placeholder, &r, &[])
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(report.failures, 1);
    let missing = capability::judge(&[cap("dsr", "missing", "#9")], &placeholder, &r, &[])
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(missing.failures, 0, "{:?}", missing.lines);
}

#[test]
fn the_list_and_the_scenarios_must_name_each_capability_once() {
    let spec = spec();
    let r = runner(&spec);
    let report = capability::judge(
        &[cap("da1", "works", "-")],
        &[capability_scenario("dsr", DSR)],
        &r,
        &[],
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(report.failures, 2, "{:?}", report.lines);
    assert!(report.lines.iter().any(|l| l.contains("FAIL dsr: The scenario dsr.scenario declares a capability that capabilities.list does not name")));
    assert!(report.lines.iter().any(|l| {
        l.contains("FAIL da1: capabilities.list names a capability that no scenario declares")
    }));
    let twice = [
        capability_scenario("dsr", DSR),
        capability_scenario("dsr", DSR),
    ];
    let report = capability::judge(&[cap("dsr", "works", "-")], &twice, &r, &[])
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        report
            .lines
            .iter()
            .any(|l| l.contains("Two scenarios declare this capability")),
        "{:?}",
        report.lines
    );
}

#[test]
fn a_capability_that_needs_a_build_feature_is_judged_by_the_build() {
    let spec = spec();
    let r = runner(&spec);
    let s = [capability_scenario("dsr", DSR)];
    let off = capability::judge(
        &[cap("dsr", "works", "-Dimages")],
        &s,
        &r,
        &[("images", false)],
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        off.lines[0].contains("passes in a build without -Dimages"),
        "{:?}",
        off.lines
    );
    let unnamed = capability::judge(
        &[cap("dsr", "works", "-Dsound")],
        &s,
        &r,
        &[("images", true)],
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        unnamed.lines[0].contains("which the build does not name"),
        "{:?}",
        unnamed.lines
    );
}

#[test]
fn a_capability_scenario_whose_fixture_is_missing_is_not_judged_and_fails() {
    let spec = spec();
    let r = runner(&spec);
    let s = [capability_scenario(
        "dsr",
        &DSR.replace("size 4 1\n", "size 4 1\nfixture no-such.termcap\n"),
    )];
    let report = capability::judge(&[cap("dsr", "works", "-")], &s, &r, &[])
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(report.failures, 1);
    assert!(
        report.lines[0].contains("no fixture no-such.termcap"),
        "{:?}",
        report.lines
    );
}

#[test]
fn the_capability_rules_refuse_a_scenario_before_anything_runs() {
    let spec = spec();
    let r = runner(&spec);
    let cases = [
        (
            "size 4 1\nsend \"x\"\ncheck\nwant row 0 |x| extra\n",
            "works",
            "not a screen view line",
        ),
        (
            "size 4 1\nsend \"x\"\ncheck\nwant changed 3\n",
            "works",
            "write N",
        ),
        (
            "size 4 1\nsend \"x\"\ncheck\nreply-re \"\\e.*\"\n",
            "missing",
            "needs an example",
        ),
        (
            "size 4 1\nsend \"x\"\ncheck\nreply-re \"\\e.*\"\nexample \"x\"\n",
            "works",
            "does not match",
        ),
    ];
    for (body, status, want) in cases {
        let s = [capability_scenario("dsr", body)];
        let issue = if status == "missing" { "#2" } else { "-" };
        let e = capability::judge(&[cap("dsr", status, issue)], &s, &r, &[])
            .err()
            .unwrap_or_default();
        assert!(e.contains(want), "{body:?}: {e}");
    }
}

#[test]
fn a_capability_scenario_replays_the_bytes_and_cuts_its_probe_had() {
    // the probe `bracketed-paste` as the probe file writes it
    let probe = "probe bracketed-paste\nsize 80 4\nsend \"\\e[?2004h\"\ncheck\ncmd paste;hex=78\ncheck\nreply \"\\e[200~x\\e[201~\"\n";
    let spec = spec();
    let probes =
        vtium_conformance::probes::read("t.replay", probe, &spec).unwrap_or_else(|e| panic!("{e}"));
    let r = runner(&spec);
    let s = parse("bracketed-paste", BRACKETED);
    let replay = vtium_conformance::probes::Replay::new(&spec, &vtium_conformance::describe())
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(r.capability(&s), replay.run(&probes[0]));
    assert_eq!(r.capability(&s), Outcome::Pass);
}

#[test]
fn every_scenario_in_the_tree_reads() {
    let dir = Path::new(TOOL).join("scenarios");
    let mut count = 0;
    for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
        let path = entry.map(|e| e.path()).unwrap_or_default();
        if path.extension().is_some_and(|x| x == "scenario") {
            let name = path.file_stem().unwrap_or_default().to_string_lossy();
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            scenario::parse(&name, &path.display().to_string(), &text)
                .unwrap_or_else(|e| panic!("{e}"));
            count += 1;
        }
    }
    assert!(count > 0, "no scenarios in {}", dir.display());
}

#[test]
fn a_provisional_transcript_that_differs_judges_nothing_but_its_assertions_still_do() {
    use vtium_conformance::scenario::run::{Played, Verdict, verdict};
    let played = |assertions: Outcome| -> Result<Played, String> {
        Ok(Played {
            transcript: CURSOR_TRANSCRIPT.to_owned(),
            assertions,
        })
    };
    let moved = CURSOR_TRANSCRIPT.replace(r#""\eOA""#, r#""\e[A""#);
    assert_eq!(
        verdict(&played(Outcome::Pass), Some(CURSOR_TRANSCRIPT), false),
        Verdict::Pass
    );
    assert!(
        matches!(verdict(&played(Outcome::Pass), Some(&moved), false), Verdict::Fail(why) if why.contains("after `key Up`"))
    );
    assert!(
        matches!(verdict(&played(Outcome::Pass), Some(&moved), true), Verdict::Provisional(why) if why.contains("after `key Up`"))
    );
    let failing = played(Outcome::Fail("line 9: no reply".to_owned()));
    assert!(
        matches!(verdict(&failing, Some(CURSOR_TRANSCRIPT), true), Verdict::Fail(why) if why.contains("an assertion fails"))
    );
    assert!(
        matches!(verdict(&played(Outcome::Pass), None, true), Verdict::Fail(why) if why.contains("has no transcript"))
    );
    assert!(
        matches!(verdict(&Err("not run: x".to_owned()), Some(CURSOR_TRANSCRIPT), true), Verdict::Fail(why) if why == "not run: x")
    );
}

#[test]
fn play_shows_each_command_its_side_effects_and_each_dumped_screen_then_waits() {
    use vtium_conformance::scenario::play::{Step, play};
    let spec = spec();
    let s = parse(
        "palette-play",
        "# spec: x\nsize 6 2\nsend \"\\e[31mred\"\nsend \"\\e[5n\"\ndump\n",
    );
    let mut out = Vec::new();
    let mut waits = 0;
    let done = play(&runner(&spec), &s, None, Some("new"), &mut out, &mut || {
        waits += 1;
        Step::Next
    })
    .unwrap_or_else(|e| panic!("{e}"));
    let text = String::from_utf8_lossy(&out);
    assert!(done);
    assert_eq!(waits, 4, "one wait after each command");
    for want in [
        "provisional: new",
        "no transcript on file yet",
        "> send \"\\e[31mred\"",
        "< reply \"\\e[0n\"",
        "┌──────┐",
        "\x1b[0;38;2;205;0;0;48;2;0;0;0mr",
        "cursor 3,0",
        "Its assertions: pass",
    ] {
        assert!(text.contains(want), "{want:?} not in:\n{text}");
    }
    let mut out = Vec::new();
    let done = play(
        &runner(&spec),
        &s,
        Some(CURSOR_TRANSCRIPT),
        None,
        &mut out,
        &mut || Step::Quit,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let text = String::from_utf8_lossy(&out);
    assert!(!done);
    assert!(text.contains("differs from this run"), "{text}");
    assert!(
        !text.contains("> send"),
        "it stops after the first command:\n{text}"
    );
}

#[test]
fn write_refuses_an_existing_transcript_without_replace_and_lists_a_new_scenario() {
    use vtium_conformance::scenario::run::{NEW_SCENARIO, write_transcript};
    let dir = std::env::temp_dir().join(format!("vtium-scenarios-write-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
    let list = dir.join("provisional.tsv");
    let head = "# NAME<TAB>REASON\nkey-old\treviewed later\n";
    std::fs::write(&list, head).unwrap_or_else(|e| panic!("{e}"));
    let read = |p: &Path| std::fs::read_to_string(p).unwrap_or_default();

    assert_eq!(
        write_transcript(&dir, &list, "key-new", "> size 4 1\n", false),
        Ok(true)
    );
    assert_eq!(read(&dir.join("key-new.transcript")), "> size 4 1\n");
    assert_eq!(read(&list), format!("{head}key-new\t{NEW_SCENARIO}\n"));

    let refused = write_transcript(&dir, &list, "key-new", "> size 5 1\n", false);
    assert!(
        matches!(&refused, Err(e) if e.contains("exists") && e.contains("--write --replace key-new")),
        "{refused:?}"
    );
    assert_eq!(
        read(&dir.join("key-new.transcript")),
        "> size 4 1\n",
        "the refusal writes nothing"
    );

    assert_eq!(
        write_transcript(&dir, &list, "key-new", "> size 5 1\n", true),
        Ok(false)
    );
    assert_eq!(read(&dir.join("key-new.transcript")), "> size 5 1\n");
    assert_eq!(
        read(&list),
        format!("{head}key-new\t{NEW_SCENARIO}\n"),
        "--replace leaves the list"
    );

    assert_eq!(
        write_transcript(&dir, &list, "key-old", "> size 4 1\n", false),
        Ok(false),
        "already listed"
    );
    assert_eq!(read(&list).matches("key-old").count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
