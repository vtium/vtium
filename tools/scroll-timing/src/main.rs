//! scroll-timing: time three ways to scroll the CPU path's display on a
//! KMS device, beside the redraw vtium does today.
//!
//! ```text
//! scroll-timing memory [--sizes 1920x1080,1366x768] [--cells 8x16,16x32]
//!                      [--buffers 1,2,3] [--spare 0,screen]
//! scroll-timing run [--device /dev/dri/card0] [--sizes preferred|WxH,...]
//!                   [--cells 8x16,16x32] [--speeds 1,4,16|none]
//!                   [--pixel-speeds 1,2,4,8|none] [--buffers 1,2,3]
//!                   [--ways redraw,copy,move,pan,pan-ahead] [--spare 0,screen]
//!                   [--look random|text]
//!                   [--frames 120] [--warmup 10] [--tsv FILE]
//! ```
//!
//! `--speeds` gives speeds in whole text rows a frame, and `--pixel-speeds`
//! in pixel lines a frame, as a smooth scroll moves. The move runs only at
//! whole-row speeds.
//!
//! `memory` prints what each way needs and opens no device. `run` takes
//! DRM master of the card, so it runs only where no other program shows
//! anything on that card: tier 3's guest, or a text console of a machine
//! whose display server is on another VT. It prints a header that names
//! the machine, the kernel, the driver and the command, then one row a
//! way, size, cell, speed, buffer count and spare rows, as tab-separated
//! values, and writes the same to `--tsv`. It exits 0 when every
//! measurement ran, 1 when one failed, and 2 on a usage error.

mod kms;
mod run;

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use vtium_scroll_timing::memory;
use vtium_scroll_timing::pattern::Look;
use vtium_scroll_timing::ways::Way;
use vtium_scroll_timing::{Geometry, Unit};

use run::{Facts, Measured, Plan};

const USAGE: &str = "usage: scroll-timing memory [--sizes WxH,...] [--cells WxH,...] [--buffers 1,2,3] [--spare 0,screen]\n       \
scroll-timing run [--device /dev/dri/cardN] [--sizes preferred|WxH,...] [--cells WxH,...] [--speeds N,...] \
[--pixel-speeds N,...] [--buffers 1,2,3] [--ways redraw,copy,move,pan,pan-ahead] [--spare N|screen,...] [--look random|text] [--frames N] [--warmup N] [--tsv FILE]";

fn usage(problem: &str) -> ExitCode {
    eprintln!("scroll-timing: {problem}\n{USAGE}");
    ExitCode::from(2)
}

/// drm2d's log lines at warning level and above, on standard error.
struct Stderr;

impl log::Log for Stderr {
    fn enabled(&self, m: &log::Metadata<'_>) -> bool {
        m.level() <= log::Level::Warn
    }
    fn log(&self, r: &log::Record<'_>) {
        if self.enabled(r.metadata()) {
            eprintln!("scroll-timing: {}: {}", r.level(), r.args());
        }
    }
    fn flush(&self) {}
}

struct Options {
    device: PathBuf,
    /// `None` for each display's preferred mode.
    sizes: Vec<Option<(u32, u32)>>,
    plan: Plan,
    tsv: Option<String>,
}

fn parse_size(text: &str) -> Option<(u32, u32)> {
    let (w, h) = text.split_once('x')?;
    let (w, h) = (w.parse().ok()?, h.parse().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

fn list<T>(text: &str, parse: impl Fn(&str) -> Option<T>) -> Option<Vec<T>> {
    text.split(',').map(parse).collect()
}

/// A list of speeds, each above 0, or `none` for no speeds of that unit.
fn speeds(text: &str) -> Option<Vec<u32>> {
    if text == "none" {
        return Some(Vec::new());
    }
    list(text, |s| s.parse().ok().filter(|&n| n > 0))
}

fn options(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        device: PathBuf::from("/dev/dri/card0"),
        sizes: vec![None],
        plan: Plan {
            cells: vec![(8, 16), (16, 32)],
            speeds: vec![1, 4, 16],
            pixel_speeds: vec![1, 2, 4, 8],
            look: Look::Random,
            buffers: vec![1, 2, 3],
            ways: Way::ALL.to_vec(),
            spares: vec![Some(0), None],
            frames: 120,
            warmup: 10,
        },
        tsv: None,
    };
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let value = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
        let bad = || format!("cannot read {flag} {value}");
        match flag.as_str() {
            "--device" => o.device = PathBuf::from(value),
            "--sizes" => {
                o.sizes = list(value, |s| match s {
                    "preferred" => Some(None),
                    _ => parse_size(s).map(Some),
                })
                .ok_or_else(bad)?
            }
            "--cells" => {
                o.plan.cells =
                    list(value, |s| parse_size(s).filter(|c| c.0 <= 64)).ok_or_else(bad)?
            }
            "--speeds" => o.plan.speeds = speeds(value).ok_or_else(bad)?,
            "--pixel-speeds" => o.plan.pixel_speeds = speeds(value).ok_or_else(bad)?,
            "--buffers" => {
                o.plan.buffers = list(value, |s| s.parse().ok().filter(|n| (1..=3).contains(n)))
                    .ok_or_else(bad)?
            }
            "--ways" => o.plan.ways = list(value, Way::parse).ok_or_else(bad)?,
            "--look" => o.plan.look = Look::parse(value).ok_or_else(bad)?,
            "--spare" => {
                o.plan.spares = list(value, |s| match s {
                    "screen" => Some(None),
                    _ => s.parse().ok().map(Some),
                })
                .ok_or_else(bad)?
            }
            "--frames" => o.plan.frames = value.parse().ok().filter(|&n| n > 0).ok_or_else(bad)?,
            "--warmup" => o.plan.warmup = value.parse().map_err(|_| bad())?,
            "--tsv" => o.tsv = Some(value.clone()),
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    Ok(o)
}

const MIB: f64 = 1024.0 * 1024.0;

/// The memory table, for sizes and cells given, with lines as wide as the
/// screen, which a dumb buffer's pitch may round up.
fn memory_table(o: &Options) -> Vec<String> {
    let mut rows = vec![
        "way\tsize\tcell\tbuffers\tspare\tdisplay_bytes\tsystem_bytes\tadded_bytes\tadded_mib\tspeed_unit"
            .to_owned(),
    ];
    for size in &o.sizes {
        let (w, h) = size.unwrap_or((1920, 1080));
        for &(cw, ch) in &o.plan.cells {
            let g = Geometry {
                width: w,
                height: h,
                cell_width: cw,
                cell_height: ch,
            };
            for way in &o.plan.ways {
                let spares: Vec<Option<u32>> = if way.pans() {
                    o.plan.spares.clone()
                } else {
                    vec![Some(0)]
                };
                // Only the pan's memory differs at pixel steps, so the
                // other ways give one row for both units.
                let units: &[(Unit, &str)] = match way {
                    Way::Pan | Way::PanAhead => &[(Unit::Rows, "rows"), (Unit::Pixels, "pixels")],
                    Way::Move => &[(Unit::Rows, "rows")],
                    _ => &[(Unit::Rows, "rows,pixels")],
                };
                for spare in spares {
                    let spare = spare.unwrap_or(g.rows());
                    for &(unit, unit_text) in units {
                        for &b in &o.plan.buffers {
                            let m = memory::needs(g, *way, b, u64::from(w) * 4, spare, unit);
                            let spare_text = if way.pans() {
                                spare.to_string()
                            } else {
                                "-".to_owned()
                            };
                            rows.push(format!(
                                "{}\t{w}x{h}\t{cw}x{ch}\t{b}\t{spare_text}\t{}\t{}\t{}\t{:.1}\t{unit_text}",
                                way.name(),
                                m.display,
                                m.system,
                                m.added,
                                m.added as f64 / MIB
                            ));
                        }
                    }
                }
            }
        }
    }
    rows
}

const HEADER: &str = "way\tsize\tcell\trows_a_frame\tbuffers\tspare\tframes\tcpu_median_ms\tcpu_p95_ms\tcpu_max_ms\tdraw_median_ms\tfps\trefresh_hz\twritten_bytes\tread_bytes\tsystem_bytes\trows_drawn\tdisplay_memory\tsystem_memory\tadded_memory\tcommits\tbusy\trefused\tspeed_unit";

fn row(m: &Measured) -> String {
    let spare = m.spare.map_or("-".to_owned(), |s| s.to_string());
    format!(
        "{}\t{}x{}\t{}x{}\t{}\t{}\t{spare}\t{}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.1}\t{:.3}\t{:.0}\t{:.0}\t{:.0}\t{:.1}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        m.way,
        m.size.0,
        m.size.1,
        m.cell.0,
        m.cell.1,
        m.speed,
        m.buffers,
        m.frames,
        m.cpu.median,
        m.cpu.p95,
        m.cpu.max,
        m.draw.median,
        m.fps,
        m.refresh,
        m.counts[0],
        m.counts[1],
        m.counts[2],
        m.counts[3],
        m.memory.display,
        m.memory.system,
        m.memory.added,
        m.commits,
        m.busy,
        m.refused,
        m.unit.name()
    )
}

/// The machine's processor and its count, from /proc/cpuinfo.
fn machine() -> String {
    let info = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let model = info
        .lines()
        .find_map(|l| {
            l.strip_prefix("model name")
                .and_then(|r| r.split_once(':'))
                .map(|(_, v)| v.trim().to_owned())
        })
        .unwrap_or_else(|| "unknown".to_owned());
    let count = info.lines().filter(|l| l.starts_with("processor")).count();
    format!("{model}, {count} CPUs online")
}

fn emit(lines: &[String], tsv: Option<&str>) -> std::io::Result<()> {
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    std::io::stdout().write_all(text.as_bytes())?;
    if let Some(path) = tsv {
        std::fs::write(path, text)?;
    }
    Ok(())
}

fn main() -> ExitCode {
    let _ = log::set_logger(&Stderr).map(|()| log::set_max_level(log::LevelFilter::Warn));
    let args: Vec<String> = std::env::args().collect();
    let Some(command) = args.get(1) else {
        return usage("no command");
    };
    let o = match options(&args[2..]) {
        Ok(o) => o,
        Err(e) => return usage(&e),
    };
    match command.as_str() {
        "memory" => match emit(&memory_table(&o), o.tsv.as_deref()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("scroll-timing: {e}");
                ExitCode::FAILURE
            }
        },
        "run" => {
            let mut facts = Facts::default();
            let mut results = Vec::new();
            let mut failed = false;
            for &size in &o.sizes {
                for step in [run::drm2d_ways, run::pan_way] {
                    match step(&o.device, size, &o.plan, &mut facts) {
                        Ok(rows) => results.extend(rows),
                        Err(e) => {
                            eprintln!("scroll-timing: {e}");
                            facts.lines.push(("failed".to_owned(), e.to_string()));
                            failed = true;
                        }
                    }
                }
            }
            let uname = rustix::system::uname();
            let mut lines = vec![
                format!("# machine\t{}", machine()),
                format!("# kernel\t{}", uname.release().to_string_lossy()),
                format!("# device\t{}", o.device.display()),
                format!("# command\t{}", args.join(" ")),
            ];
            lines.extend(facts.lines.iter().map(|(k, v)| format!("# {k}\t{v}")));
            lines.push(HEADER.to_owned());
            lines.extend(results.iter().map(row));
            if let Err(e) = emit(&lines, o.tsv.as_deref()) {
                eprintln!("scroll-timing: {e}");
                return ExitCode::FAILURE;
            }
            if failed {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        other => usage(&format!("unknown command {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vtium_scroll_timing::stats::Summary;

    /// A row of results at a pixel speed, with every column its own value.
    fn measured(unit: Unit) -> Measured {
        Measured {
            way: "pan",
            size: (1920, 1080),
            cell: (8, 16),
            speed: 2,
            unit,
            buffers: 2,
            spare: Some(0),
            frames: 120,
            cpu: Summary::default(),
            draw: Summary::default(),
            fps: 75.0,
            refresh: 75.0,
            counts: [0.0; 4],
            memory: memory::Memory {
                display: 0,
                system: 0,
                added: 0,
            },
            commits: 130,
            busy: 0,
            refused: 0,
        }
    }

    /// The unit is the header's last column, so a results file written
    /// before the column existed still reads by its own header, and each
    /// row gives its speed's unit there.
    #[test]
    fn each_row_ends_with_its_speeds_unit() {
        let header: Vec<&str> = HEADER.split('\t').collect();
        assert_eq!(header.last(), Some(&"speed_unit"));
        assert_eq!(header[3], "rows_a_frame");
        for (unit, name) in [(Unit::Rows, "rows"), (Unit::Pixels, "pixels")] {
            let line = row(&measured(unit));
            let fields: Vec<&str> = line.split('\t').collect();
            assert_eq!(fields.len(), header.len(), "{line}");
            assert_eq!(fields.last(), Some(&name));
            assert_eq!(fields[3], "2");
        }
    }

    #[test]
    fn the_pixel_speeds_are_read_and_none_leaves_a_unit_out() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let o = options(&args(&["--pixel-speeds", "1,3", "--speeds", "none"])).unwrap();
        assert_eq!(o.plan.steps(), [(Unit::Pixels, 1), (Unit::Pixels, 3)]);
        let o = options(&[]).unwrap();
        assert_eq!(o.plan.pixel_speeds, [1, 2, 4, 8]);
        assert!(options(&args(&["--pixel-speeds", "0"])).is_err());
    }
}
