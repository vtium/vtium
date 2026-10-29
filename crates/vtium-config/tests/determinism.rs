//! Placement follows each device (docs/architecture.md, "Determinism";
//! ADR-1401 D11): generated devices walk generated rulesets in every
//! report order and at every hotplug point, and each lands the same.
//!
//! The workspace has no property-test crate, so the sets are small and the
//! orders exhaustive; the generator is seeded, and a failure prints its
//! seed.

use vtium_config::{Class, Config, Edid, Identity, Place, Placed, UsbSerial, parse};

/// A small seeded generator (xorshift64*).
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

    fn chance(&mut self) -> bool {
        self.next() & 1 == 1
    }
}

const PATHS: [&str; 3] = [
    "pci-0000:15:00.0-usb-0:3.3",
    "pci-0000:15:00.0-usb-0:3.4.1",
    "pci-0000:15:00.0-usb-0:3.2",
];
const CARDS: [&str; 2] = ["pci-0000:03:00.0", "pci-0000:00:02.0"];
const CONNECTORS: [&str; 3] = ["DP-8", "HDMI-A-2", "DP-1"];
const CLASSES: [Class; 6] = [
    Class::Keyboard,
    Class::Key,
    Class::Mouse,
    Class::Joystick,
    Class::Tablet,
    Class::Switch,
];

fn display(rng: &mut Rng) -> Identity {
    let mut d = Identity::display();
    let card = rng.below(CARDS.len());
    d.card = Some(CARDS[card].to_owned());
    d.on_boot_card = card == 0;
    d.connector = Some(CONNECTORS[rng.below(CONNECTORS.len())].to_owned());
    d.order = rng.below(4) as u32;
    d.lit_at_start = rng.below(3) == 0;
    if rng.chance() {
        d.edid = Some(Edid {
            maker: ["AAA", "BBB"][rng.below(2)].to_owned(),
            product: rng.below(3) as u16,
            serial: format!("SER:{}", rng.below(3)),
        });
    }
    d
}

fn input(rng: &mut Rng) -> Identity {
    let mut d = Identity::input();
    for class in CLASSES {
        if rng.below(3) == 0 {
            d = d.with_class(class);
        }
    }
    if rng.chance() {
        d.path = Some(format!(
            "{}:1.{}",
            PATHS[rng.below(PATHS.len())],
            rng.below(3)
        ));
    }
    if rng.chance() {
        d.serial = Some(UsbSerial {
            vendor: 0xaaaa,
            product: rng.below(3) as u16,
            serial: "NUMPAD0001".to_owned(),
        });
    }
    if rng.below(4) == 0 {
        let role = ["token", "numpad"][rng.below(2)];
        d.properties
            .insert("VTIUM_ROLE".to_owned(), role.to_owned());
    }
    d.name = Some(["Keyboard A", "Numpad", "Mouse"][rng.below(3)].to_owned());
    d
}

/// A generated ruleset over the same facts the devices draw from, so its
/// rules match some of them, with the guards on or off.
fn ruleset(rng: &mut Rng) -> Config {
    let mut text = String::from("version = 1\n[head desk]\n[head panel]\n[rules]\n");
    let heads = ["desk", "panel"];
    for _ in 0..rng.below(6) {
        let line = match rng.below(9) {
            0 => format!(
                "display edid:AAA:0x{:04x}:SER:{} -> {}",
                rng.below(3),
                rng.below(3),
                heads[rng.below(2)]
            ),
            1 => format!(
                "display connector:{}/{} -> {}",
                CARDS[rng.below(2)],
                ["DP-*", "HDMI-A-?", "*"][rng.below(3)],
                heads[rng.below(2)]
            ),
            2 => format!("display card:{} -> dark", CARDS[rng.below(2)]),
            3 => format!(
                "input path:{} -> {}",
                PATHS[rng.below(3)],
                heads[rng.below(2)]
            ),
            4 => format!(
                "input path:{} serial:aaaa:{:04x}:NUMPAD0001 -> {}",
                PATHS[rng.below(3)],
                rng.below(3),
                heads[rng.below(2)]
            ),
            5 => format!(
                "input class:{} -> {}",
                CLASSES[rng.below(6)].name(),
                ["focus", "ignore", "desk"][rng.below(3)]
            ),
            6 => "input name:*pad -> panel".to_owned(),
            7 => "input * -> ignore".to_owned(),
            _ => format!("display * -> {}", ["desk", "dark"][rng.below(2)]),
        };
        text.push_str(&line);
        text.push('\n');
    }
    text.push_str("[guards]\n");
    for guard in ["anti-lockout", "rescue-key", "never-dark"] {
        let on = if rng.below(4) == 0 { "off" } else { "on" };
        text.push_str(&format!("{guard} = {on}\n"));
    }
    match parse(&text) {
        Ok(p) => p.config,
        Err(e) => panic!("the generated ruleset parses: {e:?}\n{text}"),
    }
}

/// Every order of `0..n`, by Heap's algorithm.
fn orders(n: usize) -> Vec<Vec<usize>> {
    let mut a: Vec<usize> = (0..n).collect();
    let mut c = vec![0; n];
    let mut out = vec![a.clone()];
    let mut i = 0;
    while i < n {
        if c[i] < i {
            if i % 2 == 0 {
                a.swap(0, i);
            } else {
                a.swap(c[i], i);
            }
            out.push(a.clone());
            c[i] += 1;
            i = 0;
        } else {
            c[i] = 0;
            i += 1;
        }
    }
    out
}

/// Where each device of `chosen` lands, by its index in `devices`, and
/// where the focus lands, when they are reported in the order `chosen`
/// gives.
fn landing(
    config: &Config,
    devices: &[Identity],
    chosen: &[usize],
) -> (Vec<(usize, Placed)>, Option<Place>) {
    let set: Vec<Identity> = chosen.iter().map(|&i| devices[i].clone()).collect();
    let placement = config.place(&set, None);
    let mut out: Vec<(usize, Placed)> = chosen.iter().copied().zip(placement.placed).collect();
    out.sort_by_key(|(i, _)| *i);
    (out, placement.focus)
}

#[test]
fn each_device_lands_the_same_in_every_report_order() {
    let mut cases = 0usize;
    for seed in 1..=60u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15));
        let config = ruleset(&mut rng);
        let displays = 1 + rng.below(3);
        let inputs = 1 + rng.below(3);
        let mut devices: Vec<Identity> = (0..displays).map(|_| display(&mut rng)).collect();
        devices.extend((0..inputs).map(|_| input(&mut rng)));
        // a seed that draws no display lit at start and none on the boot
        // card still has one on the boot card, so the display guard picks
        devices[0].on_boot_card = true;
        let all: Vec<usize> = (0..devices.len()).collect();
        let want = landing(&config, &devices, &all);
        for order in orders(devices.len()) {
            assert_eq!(
                landing(&config, &devices, &order),
                want,
                "seed {seed}: the order {order:?} lands a device elsewhere"
            );
            // each hotplug point: the devices plugged so far land as that
            // set reported at once in any order
            for k in 1..order.len() {
                let mut plugged = order[..k].to_vec();
                let got = landing(&config, &devices, &plugged);
                plugged.sort_unstable();
                assert_eq!(
                    got,
                    landing(&config, &devices, &plugged),
                    "seed {seed}: {k} devices plugged in the order {order:?} land elsewhere"
                );
            }
            cases += 1;
        }
    }
    assert!(cases > 1000, "the test walked {cases} orders");
}
