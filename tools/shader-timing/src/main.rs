//! shader-timing: time spatial shaders off-screen on a render node.
//!
//! ```text
//! shader-timing list
//! shader-timing run [--device surfaceless|software|/dev/dri/renderDN]
//!                   [--sizes 1920x1080,3840x2160] [--modes plain,smooth,ghostty,gl-linear]
//!                   [--shaders NAME,...] [--frames 31] [--warmup 3] [--refresh 60]
//!                   [--tsv FILE]
//! ```
//!
//! `run` prints one row a plan, size and mode, as tab-separated values
//! after a header that names the device, and writes the same rows to
//! `--tsv` when given. A name in `--shaders` is a shader's, or
//! `crt+bloom` for the two chains. It exits 0 when every plan ran, 1 when
//! one failed to compile or draw, and 2 on a usage error or a device it
//! cannot open.

use std::io::Write;
use std::process::ExitCode;

use vtium_shader_timing::device::{Choice, Device};
use vtium_shader_timing::source::{self, Mode};
use vtium_shader_timing::timing::Bench;
use vtium_shader_timing::{Plan, Row, plans, time_plan};

const USAGE: &str = "usage: shader-timing list\n       shader-timing run [--device surfaceless|software|/dev/dri/renderDN] \
[--sizes WxH,...] [--modes plain,smooth,ghostty,gl-linear] [--shaders NAME,...] \
[--frames N] [--warmup N] [--refresh HZ] [--tsv FILE]";

fn usage(problem: &str) -> ExitCode {
    eprintln!("shader-timing: {problem}\n{USAGE}");
    ExitCode::from(2)
}

struct Options {
    device: Choice,
    sizes: Vec<(u32, u32)>,
    modes: Vec<Mode>,
    shaders: Option<Vec<String>>,
    frames: u32,
    warmup: u32,
    refresh: f32,
    tsv: Option<String>,
}

fn parse_size(text: &str) -> Option<(u32, u32)> {
    let (w, h) = text.split_once('x')?;
    let (w, h) = (w.parse().ok()?, h.parse().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

fn options(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        device: Choice::Surfaceless,
        sizes: vec![(1920, 1080), (3840, 2160)],
        modes: Mode::ALL.to_vec(),
        shaders: None,
        frames: 31,
        warmup: 3,
        refresh: 60.0,
        tsv: None,
    };
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--device" => o.device = Choice::parse(&value()?),
            "--sizes" => {
                o.sizes = value()?
                    .split(',')
                    .map(|s| {
                        parse_size(s).ok_or_else(|| format!("{s} is not a size such as 1920x1080"))
                    })
                    .collect::<Result<_, _>>()?
            }
            "--modes" => {
                o.modes = value()?
                    .split(',')
                    .map(|m| Mode::parse(m).ok_or_else(|| format!("{m} is not a read mode")))
                    .collect::<Result<_, _>>()?
            }
            "--shaders" => o.shaders = Some(value()?.split(',').map(str::to_owned).collect()),
            "--frames" | "--warmup" => {
                let v = value()?;
                let n: u32 = v
                    .parse()
                    .map_err(|_| format!("{flag} takes a whole number, not {v}"))?;
                if flag == "--frames" {
                    if n == 0 {
                        return Err("--frames must be at least 1".to_owned());
                    }
                    o.frames = n;
                } else {
                    o.warmup = n;
                }
            }
            "--refresh" => {
                let v = value()?;
                o.refresh = v
                    .parse()
                    .ok()
                    .filter(|r: &f32| *r >= 1.0)
                    .ok_or_else(|| format!("--refresh takes a rate of at least 1, not {v}"))?;
            }
            "--tsv" => o.tsv = Some(value()?),
            other => return Err(format!("unknown option {other}")),
        }
    }
    Ok(o)
}

fn selected(all: Vec<Plan>, names: &Option<Vec<String>>) -> Result<Vec<Plan>, String> {
    let Some(names) = names else {
        return Ok(all);
    };
    let known = |p: &Plan| match p {
        Plan::Single(s) => s.name.to_owned(),
        Plan::OnePass(a, b) | Plan::TwoPasses(a, b) => format!("{}+{}", a.name, b.name),
    };
    for n in names {
        if !all.iter().any(|p| known(p) == *n) {
            return Err(format!("no shader is named {n}"));
        }
    }
    Ok(all
        .into_iter()
        .filter(|p| names.contains(&known(p)))
        .collect())
}

fn run(o: Options) -> ExitCode {
    let shaders = match source::catalog() {
        Ok(s) => s,
        Err(e) => return usage(&e),
    };
    let plans = match selected(plans(&shaders), &o.shaders) {
        Ok(p) => p,
        Err(e) => return usage(&e),
    };
    let device = match Device::open(&o.device) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("shader-timing: the device cannot be opened: {e}");
            return ExitCode::from(2);
        }
    };
    let mut lines = vec![
        format!("# device\t{}", device.node),
        format!("# renderer\t{}", device.renderer),
        format!("# version\t{}", device.version),
    ];
    let mut failed = false;
    let mut out = std::io::stdout().lock();
    for &(w, h) in &o.sizes {
        let bench = match Bench::new(&device, w, h) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("shader-timing: {w}x{h}: {e}");
                return ExitCode::from(2);
            }
        };
        if lines.len() == 3 {
            let clock = if bench.timer_queries() {
                "GL_EXT_disjoint_timer_query, and glFinish around each frame"
            } else {
                "glFinish around each frame; the driver has no timer queries"
            };
            lines.push(format!("# clock\t{clock}"));
            lines.push(Row::HEADER.to_owned());
            for l in &lines {
                let _ = writeln!(out, "{l}");
            }
        }
        for plan in &plans {
            for &mode in &o.modes {
                match time_plan(&bench, plan, mode, o.warmup, o.frames, o.refresh) {
                    Ok(row) => {
                        let line = row.tsv();
                        let _ = writeln!(out, "{line}");
                        let _ = out.flush();
                        lines.push(line);
                    }
                    Err(e) => {
                        failed = true;
                        eprintln!(
                            "shader-timing: {} ({}) at {w}x{h}: {e}",
                            plan.label(),
                            mode.name()
                        );
                    }
                }
            }
        }
    }
    if let Some(path) = &o.tsv {
        let text = lines.join("\n") + "\n";
        if let Err(e) = std::fs::write(path, text) {
            eprintln!("shader-timing: {path} cannot be written: {e}");
            return ExitCode::from(2);
        }
    }
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("list") if args.len() == 1 => match source::catalog() {
            Ok(shaders) => {
                for p in plans(&shaders) {
                    println!("{}\t{}\t{}", p.label(), p.reads(), p.work());
                }
                for s in &shaders {
                    println!("# {}: {}", s.name, s.origin);
                }
                ExitCode::SUCCESS
            }
            Err(e) => usage(&e),
        },
        Some("run") => match options(&args[1..]) {
            Ok(o) => run(o),
            Err(e) => usage(&e),
        },
        _ => usage("a command is needed"),
    }
}
