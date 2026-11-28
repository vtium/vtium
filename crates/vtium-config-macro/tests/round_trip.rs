//! The canonical text of any macro parses back into the same macro, and
//! any text that parses writes canonical text that parses the same way.
//! The macros and texts come from a small generator seeded in the test,
//! so a failure names its seed and replays.
//!
//! References:
//! - ADR-1208 D7: the property test that the written text parses back.

use vtium_config_macro::bounds::{COUNT_MAX, LABEL_MAX, TYPE_MAX};
use vtium_config_macro::{
    Character, Comment, Count, Expectation, Key, KeyPress, KeyStep, Label, Macro, Mods, Name, Node,
    Pause, Step, Text, parse,
};

/// xorshift64*, enough to vary the generated macros.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<T: Clone>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())].clone()
    }
}

/// Characters that stress the quoting, the spacing and the comment rules.
const CHARS: &[char] = &[
    'a', 'Z', '0', ' ', ' ', '"', '\'', '`', '#', '+', '\\', 'é', '中', '🙂', '.', '-', '\t',
];

fn text(rng: &mut Rng, max: usize) -> String {
    let most = if rng.below(10) == 0 { max } else { 12 };
    let n = 1 + rng.below(most);
    (0..n).map(|_| rng.pick(CHARS)).collect()
}

fn key(rng: &mut Rng) -> Key {
    if rng.below(3) == 0 {
        Key::Char(
            Character::new(rng.pick(&['a', 'Q', '#', '+', '3', '"', 'é', 'I', 'P', '[', ']', '_']))
                .unwrap(),
        )
    } else {
        rng.pick(&Key::NAMED).1
    }
}

fn step(rng: &mut Rng) -> Step {
    loop {
        let s = match rng.below(11) {
            0 => {
                let mods = Mods {
                    ctrl: rng.below(2) == 0,
                    alt: rng.below(2) == 0,
                    shift: rng.below(3) == 0,
                };
                let count = Count::new(1 + rng.below(COUNT_MAX as usize) as u8).unwrap();
                KeyStep::new(
                    KeyPress {
                        mods,
                        key: key(rng),
                    },
                    count,
                )
                .ok()
                .map(Step::Key)
            }
            1 => Text::new(&text(rng, TYPE_MAX)).map(Step::Type),
            2 => Label::new(&text(rng, LABEL_MAX)).map(Step::Click),
            3 => Label::new(&text(rng, LABEL_MAX)).map(Step::ClickMouse),
            4 => Label::new(&text(rng, LABEL_MAX)).map(|l| {
                Step::Expect(if rng.below(2) == 0 {
                    Expectation::Shows(l)
                } else {
                    Expectation::ShowsNot(l)
                })
            }),
            5 => Node::new(rng.pick(&["fonts", "looks.look", "a-b.c_d.0"]))
                .map(|n| Step::Expect(Expectation::At(n))),
            6 => {
                let status = rng.below(256) as u8;
                Some(Step::Expect(rng.pick(&[
                    Expectation::Saved,
                    Expectation::Unsaved,
                    Expectation::Exited(status),
                ])))
            }
            7 => Pause::from_millis(1 + rng.below(60_000) as u32).map(Step::Sleep),
            8 => Name::new(rng.pick(&["a", "after-save", "v1.2_b"])).map(Step::Screenshot),
            9 => Comment::new(&text(rng, 200)).map(Step::Comment),
            _ => Some(Step::Blank),
        };
        if let Some(s) = s {
            return s;
        }
    }
}

fn generated(rng: &mut Rng) -> Macro {
    let mut names = Vec::new();
    let mut steps: Vec<Step> = (0..rng.below(30))
        .map(|_| step(rng))
        .filter(|s| match s {
            // A macro uses each screenshot's name once.
            Step::Screenshot(n) if names.contains(n) => false,
            Step::Screenshot(n) => {
                names.push(n.clone());
                true
            }
            _ => true,
        })
        .collect();
    if rng.below(4) == 0 {
        let at = steps.iter().position(Step::acts).unwrap_or(steps.len());
        steps.insert(rng.below(at + 1), Step::SetLive);
    }
    Macro::new(steps).unwrap()
}

#[test]
fn every_generated_macro_parses_back_from_its_text() {
    for seed in 1..=3000u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let m = generated(&mut rng);
        let text = m.to_string();
        let back = parse(&text).unwrap_or_else(|e| panic!("seed {seed}: {e}\n{text}"));
        assert_eq!(back, m, "seed {seed}\n{text}");
    }
}

/// Words and pieces of lines, so the generated texts reach every step and
/// many of the errors.
const PIECES: &[&str] = &[
    "Enter",
    "Esc",
    "Down",
    "Ctrl+",
    "Alt+",
    "Shift+",
    "S",
    "#",
    " ",
    "\t",
    "\n",
    "\r\n",
    "3",
    "100",
    "101",
    "0",
    "Type",
    "Click",
    "Mouse",
    "Expect",
    "Not",
    "At",
    "Saved",
    "Unsaved",
    "Exited",
    "Sleep",
    "500ms",
    "2s",
    "61s",
    "0.5",
    "@500ms",
    "`",
    "I",
    "[",
    "_",
    "P",
    "Screenshot",
    "Set",
    "Live",
    "\"",
    "'",
    "a b",
    "looks.look",
    "+",
    "é",
    "failure",
    "live",
    "step-1",
    "Hide",
];

#[test]
fn every_generated_text_that_parses_writes_text_that_parses_the_same() {
    let mut parsed = 0;
    for seed in 1..=20_000u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let text: String = (0..rng.below(16)).map(|_| rng.pick(PIECES)).collect();
        match parse(&text) {
            Ok(m) => {
                parsed += 1;
                let canonical = m.to_string();
                assert_eq!(
                    parse(&canonical).as_ref(),
                    Ok(&m),
                    "seed {seed}: {text:?} wrote {canonical:?}"
                );
                assert_eq!(
                    parse(&canonical).unwrap().to_string(),
                    canonical,
                    "seed {seed}"
                );
            }
            Err(e) => {
                assert!(e.line() >= 1, "seed {seed}: {text:?}");
                assert!(
                    e.line() <= text.split('\n').count(),
                    "seed {seed}: {text:?}: {e}"
                );
                assert!(!e.to_string().contains('\n'), "seed {seed}: {e}");
            }
        }
    }
    // The generator reaches texts that parse as well as texts that fail.
    assert!(parsed > 1000, "{parsed}");
}
