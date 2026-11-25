//! The measurements on a KMS device: the redraw, the copy and the move
//! through vtium-platform's drm2d, and the pan through the stand-in in
//! [`crate::kms`]. Each frame's CPU time is the thread's own clock from
//! the moment the frame's buffer is taken to the moment its commit
//! returns, so it leaves out the wait for the display's flip.

#![expect(
    clippy::disallowed_methods,
    reason = "the tool measures how long frames take, and the clock decides nothing it draws"
)]

use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

use rustix::time::{ClockId, clock_gettime};
use vtium_platform::calloop::EventLoop;
use vtium_platform::display::{
    Buffers, Card, DisplayId, Drm2d, Event, MappedDumb, ModePolicy, Rect, SwapError, Wanted,
};
use vtium_scroll_timing::memory;
use vtium_scroll_timing::pattern::{Look, Pattern};
use vtium_scroll_timing::stats::{Summary, summarize};
use vtium_scroll_timing::ways::{Copy, Move, Pan, Way, redraw};
use vtium_scroll_timing::{Counts, Geometry, Unit};

use crate::kms::Kms;

/// How long a flip or a display may take before the run gives up on it.
const BOUND: Duration = Duration::from_secs(2);

/// The thread's CPU time in milliseconds.
fn cpu_ms() -> f64 {
    let t = clock_gettime(ClockId::ThreadCPUTime);
    t.tv_sec as f64 * 1e3 + t.tv_nsec as f64 / 1e6
}

/// What one run of a way measured.
pub struct Measured {
    pub way: &'static str,
    pub size: (u32, u32),
    pub cell: (u32, u32),
    pub speed: u32,
    /// Whether `speed` counts text rows or pixel lines a frame.
    pub unit: Unit,
    pub buffers: u32,
    /// The pan's spare rows; `None` for the other ways.
    pub spare: Option<u32>,
    pub frames: usize,
    pub cpu: Summary,
    pub draw: Summary,
    pub fps: f64,
    pub refresh: f64,
    /// The mean counts a frame.
    pub counts: [f64; 4],
    pub memory: memory::Memory,
    pub commits: u32,
    /// Swaps drm2d held back because a flip was in flight; it sends each
    /// such frame with the next swap.
    pub busy: u32,
    /// Commits the driver refused.
    pub refused: u32,
}

/// What a run asks for at one mode.
pub struct Plan {
    pub cells: Vec<(u32, u32)>,
    /// The speeds in text rows a frame.
    pub speeds: Vec<u32>,
    /// The speeds in pixel lines a frame, as a smooth scroll moves.
    pub pixel_speeds: Vec<u32>,
    /// What the rows show. The look changes what is drawn, not its cost.
    pub look: Look,
    pub buffers: Vec<u32>,
    pub ways: Vec<Way>,
    /// The pan's spare rows, each `None` for a screen of them.
    pub spares: Vec<Option<u32>>,
    pub frames: usize,
    pub warmup: usize,
}

impl Plan {
    /// Each speed with its unit: the whole-row speeds, then the pixel ones.
    pub fn steps(&self) -> Vec<(Unit, u32)> {
        let rows = self.speeds.iter().map(|&s| (Unit::Rows, s));
        rows.chain(self.pixel_speeds.iter().map(|&s| (Unit::Pixels, s)))
            .collect()
    }

    /// The units the plan's speeds use, each once.
    fn units(&self) -> Vec<Unit> {
        let mut units = Vec::new();
        for (unit, _) in self.steps() {
            if !units.contains(&unit) {
                units.push(unit);
            }
        }
        units
    }
}

/// The facts of a mode that the results file's header gives.
#[derive(Default)]
pub struct Facts {
    pub lines: Vec<(String, String)>,
}

impl Facts {
    /// Adds a fact, once: a fact the run states again is left out.
    fn add(&mut self, key: &str, value: impl Into<String>) {
        let line = (key.to_owned(), value.into());
        if !self.lines.contains(&line) {
            self.lines.push(line);
        }
    }
}

fn mean_counts(all: &[Counts]) -> [f64; 4] {
    let n = all.len().max(1) as f64;
    let sum = |f: fn(&Counts) -> u64| all.iter().map(f).sum::<u64>() as f64 / n;
    [
        sum(|c| c.written),
        sum(|c| c.read),
        sum(|c| c.system_written),
        sum(|c| u64::from(c.rows_drawn)),
    ]
}

struct State {
    drm: Drm2d<Card>,
    ids: Vec<DisplayId>,
}

/// drm2d on the card at `path`, set at `size` or at each display's default
/// mode, with `buffers`, and its first display once it takes a frame.
fn drm2d(
    path: &Path,
    size: Option<(u32, u32)>,
    buffers: Buffers,
) -> io::Result<(EventLoop<'static, State>, State, DisplayId)> {
    let policy = ModePolicy {
        use_original: false,
        wanted: size.map(|(width, height)| {
            Box::new(move |_: &str| {
                Some(Wanted {
                    width,
                    height,
                    rate: None,
                })
            }) as Box<_>
        }),
    };
    let card = Card::open(path)?;
    let drm = Drm2d::new(card, policy, buffers)?;
    let ev: EventLoop<'static, State> = EventLoop::try_new().map_err(io::Error::other)?;
    let mut st = State {
        drm,
        ids: Vec::new(),
    };
    st.drm
        .insert(&ev.handle(), |s: &mut State| Some(&mut s.drm))?;
    st.drm.wake_up()?;
    let mut ev = ev;
    let id = turn_until_ready(&mut ev, &mut st, None)?;
    Ok((ev, st, id))
}

/// Runs the redraw, the copy and the move of `plan` at `size` with each
/// buffer count, and the row's cost in a display buffer and in system
/// memory.
pub fn drm2d_ways(
    path: &Path,
    size: Option<(u32, u32)>,
    plan: &Plan,
    facts: &mut Facts,
) -> io::Result<Vec<Measured>> {
    let mut out = Vec::new();
    let ways: Vec<Way> = plan.ways.iter().copied().filter(|w| !w.pans()).collect();
    if ways.is_empty() {
        return Ok(out);
    }
    for &count in &plan.buffers {
        let buffers =
            Buffers::new(count as u8).ok_or_else(|| io::Error::other("buffers are 1, 2 or 3"))?;
        let (mut ev, mut st, id) = drm2d(path, size, buffers)?;
        let (w, h) = st
            .drm
            .size(id)
            .ok_or_else(|| io::Error::other("the display has no size"))?;
        let refresh = f64::from(st.drm.refresh_mhz(id).unwrap_or(0)) / 1000.0;
        if size.is_some_and(|s| s != (w, h)) {
            return Err(io::Error::other(format!(
                "drm2d set {w}x{h}, not the mode asked for"
            )));
        }
        facts.add(
            &format!("drm2d {w}x{h}, {count} buffers"),
            format!("{} at {refresh:.3} Hz", st.drm.name(id).unwrap_or("?")),
        );
        for &cell in &plan.cells {
            let g = Geometry {
                width: w,
                height: h,
                cell_width: cell.0,
                cell_height: cell.1,
            };
            let pattern = Pattern::with_look(g, plan.look);
            for (unit, speed) in plan.steps() {
                // The move scrolls only by whole rows: padnoir's figures
                // at whole rows already rule it out, since its read-back
                // from write-combined memory takes 13 to 23 ms a frame.
                for &way in ways
                    .iter()
                    .filter(|&&w| unit == Unit::Rows || w != Way::Move)
                {
                    let m = measure_drm2d(
                        &mut ev,
                        &mut st,
                        id,
                        way,
                        g,
                        &pattern,
                        (unit, speed),
                        plan,
                        count,
                        refresh,
                    )?;
                    eprintln!(
                        "scroll-timing: {} {w}x{h} {}x{} {speed} {} a frame, {count} buffers: {:.3} ms",
                        m.way,
                        cell.0,
                        cell.1,
                        unit.name(),
                        m.cpu.median
                    );
                    out.push(m);
                }
            }
        }
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn measure_drm2d(
    ev: &mut EventLoop<'static, State>,
    st: &mut State,
    id: DisplayId,
    way: Way,
    g: Geometry,
    pattern: &Pattern,
    (unit, speed): (Unit, u32),
    plan: &Plan,
    count: u32,
    refresh: f64,
) -> io::Result<Measured> {
    let mut copy = Copy::new(g);
    let mut moves = Move::default();
    // The view's top pixel line, which falls by the step each frame.
    let ch = i64::from(g.cell_height);
    let step = unit.lines(speed, g);
    let mut pos: i64 = (1 << 40) * ch;
    let (mut cpu, mut draw, mut counts) = (Vec::new(), Vec::new(), Vec::new());
    let (mut commits, mut busy, mut refused) = (0, 0, 0);
    let mut pitch = 0u64;
    let mut started = None;
    for frame in 0..plan.warmup + plan.frames {
        turn_until_ready(ev, st, Some(id))?;
        if frame == plan.warmup {
            started = Some(Instant::now());
        }
        let c0 = cpu_ms();
        let mut token = st
            .drm
            .lend(id)
            .ok_or_else(|| io::Error::other("the display lent no buffer"))?;
        let (c, d) = token
            .draw(|canvas| {
                pitch = u64::from(canvas.stride);
                let stride = canvas.stride as usize;
                let d0 = cpu_ms();
                let c = match way {
                    Way::Redraw => redraw(pattern, g, canvas.pixels, stride, pos),
                    Way::Copy => copy.frame(pattern, g, canvas.pixels, stride, pos),
                    Way::Move => {
                        moves.frame(pattern, g, canvas.pixels, stride, canvas.age, pos / ch)
                    }
                    Way::Pan | Way::PanAhead => Counts::default(),
                };
                (c, cpu_ms() - d0)
            })
            .ok_or_else(|| io::Error::other("the token's buffer is gone"))?;
        let damage = [Rect {
            x: 0,
            y: 0,
            width: g.width,
            height: g.height,
        }];
        let swapped = st.drm.swap(token, &damage);
        let mut spent = cpu_ms() - c0;
        commits += 1;
        match swapped {
            Ok(_) => {}
            // With one or three buffers, drm2d lends a buffer while a flip
            // is in flight, and the swap keeps the frame until the flip
            // completes. The owner then sends it with `commit`, as the
            // binary does when the display reports the flip.
            Err(SwapError::Busy) => {
                busy += 1;
                turn_until_flipped(ev, st, id)?;
                let c2 = cpu_ms();
                if let Err(e) = st.drm.commit(id) {
                    log::warn!("the kept frame's commit failed: {e:?}");
                    refused += 1;
                }
                spent += cpu_ms() - c2;
            }
            Err(e) => {
                log::warn!("a swap failed: {e:?}");
                refused += 1;
            }
        }
        if frame >= plan.warmup {
            cpu.push(spent);
            draw.push(d);
            counts.push(c);
        }
        pos -= step;
    }
    turn_until_ready(ev, st, Some(id))?;
    let elapsed = started.map_or(0.0, |s| s.elapsed().as_secs_f64());
    Ok(Measured {
        way: way.name(),
        size: (g.width, g.height),
        cell: (g.cell_width, g.cell_height),
        speed,
        unit,
        buffers: count,
        spare: None,
        frames: plan.frames,
        cpu: summarize(&cpu),
        draw: summarize(&draw),
        fps: plan.frames as f64 / elapsed.max(1e-9),
        refresh,
        counts: mean_counts(&counts),
        memory: memory::needs(g, way, count, pitch, 0, unit),
        commits,
        busy,
        refused,
    })
}

/// Turns the loop until display `id` has no flip in flight.
fn turn_until_flipped(
    ev: &mut EventLoop<'static, State>,
    st: &mut State,
    id: DisplayId,
) -> io::Result<()> {
    let start = Instant::now();
    while st.drm.is_flipping(id) {
        if start.elapsed() > BOUND {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "the flip did not complete",
            ));
        }
        ev.dispatch(Some(Duration::from_millis(20)), st)
            .map_err(io::Error::other)?;
    }
    Ok(())
}

/// Turns the loop until display `id`, or the first display added where
/// `id` is `None`, takes a frame, and returns that display.
fn turn_until_ready(
    ev: &mut EventLoop<'static, State>,
    st: &mut State,
    id: Option<DisplayId>,
) -> io::Result<DisplayId> {
    let start = Instant::now();
    loop {
        for e in st.drm.events() {
            if let Event::Added(d) | Event::Redraw(d) = e
                && !st.ids.contains(&d)
            {
                st.ids.push(d);
            }
        }
        if let Some(d) = id.or(st.ids.first().copied())
            && st.drm.ready_for_frame(d)
        {
            return Ok(d);
        }
        if start.elapsed() > BOUND {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "no display took a frame",
            ));
        }
        ev.dispatch(Some(Duration::from_millis(20)), st)
            .map_err(io::Error::other)?;
    }
}

/// The cost of drawing one text row of `g`, `times` times, into `buf`
/// with lines `stride` apart, at slots in turn: one sample a row.
fn row_costs(
    pattern: &Pattern,
    g: Geometry,
    buf: &mut [u8],
    stride: usize,
    times: usize,
) -> Vec<f64> {
    let slots = (buf.len() / stride / g.cell_height as usize).max(1);
    let mut samples = Vec::with_capacity(times);
    for i in 0..times {
        let at = (i % slots) * g.cell_height as usize * stride;
        let t0 = cpu_ms();
        pattern.draw_row(&mut buf[at..], stride, i as i64);
        samples.push(cpu_ms() - t0);
    }
    samples
}

/// A row of results for the cost of one text row drawn.
fn row_measured(way: &'static str, g: Geometry, samples: &[f64], refresh: f64) -> Measured {
    Measured {
        way,
        size: (g.width, g.height),
        cell: (g.cell_width, g.cell_height),
        speed: 1,
        unit: Unit::Rows,
        buffers: 1,
        spare: None,
        frames: samples.len(),
        cpu: summarize(samples),
        draw: summarize(samples),
        fps: 0.0,
        refresh,
        counts: [g.row_bytes() as f64, 0.0, 0.0, 1.0],
        memory: memory::Memory {
            display: 0,
            system: 0,
            added: 0,
        },
        commits: 0,
        busy: 0,
        refused: 0,
    }
}

/// Runs the pan of `plan` at `size` through the stand-in, with each cell,
/// spare rows and buffer count, after the checks of what the driver
/// allows, and measures what one row drawn costs.
pub fn pan_way(
    path: &Path,
    size: Option<(u32, u32)>,
    plan: &Plan,
    facts: &mut Facts,
) -> io::Result<Vec<Measured>> {
    let mut out = Vec::new();
    let mut kms = Kms::open(path, size)?;
    let (w, h, hz) = kms.mode();
    let refresh = f64::from(hz);
    facts.add("driver", kms.driver());
    facts.add(
        "largest framebuffer",
        format!("{}x{}", kms.max.0, kms.max.1),
    );
    let mut screen = MappedDumb::new(kms.fd(), w, h)?;
    let screen_fb = kms.framebuffer(&screen)?;
    kms.set_mode(screen_fb)?;
    facts.add(&format!("pan {w}x{h}"), format!("mode set at {hz} Hz"));

    // The tallest buffer the driver allows, shown from its last screen.
    let tallest = kms.max.1;
    let verdict = match MappedDumb::new(kms.fd(), w, tallest) {
        Ok(dumb) => match kms.framebuffer(&dumb) {
            Ok(fb) => {
                let v = match kms.test(fb, tallest - h) {
                    Ok(()) => "test commit accepted".to_owned(),
                    Err(e) => format!("test commit refused: {e}"),
                };
                kms.remove_framebuffer(fb);
                v
            }
            Err(e) => format!("framebuffer refused: {e}"),
        },
        Err(e) => format!("dumb buffer refused: {e}"),
    };
    facts.add(
        &format!(
            "pan {w}x{h}, a {w}x{tallest} buffer shown from row {}",
            tallest - h
        ),
        verdict,
    );

    for &cell in &plan.cells {
        let g = Geometry {
            width: w,
            height: h,
            cell_width: cell.0,
            cell_height: cell.1,
        };
        let pattern = Pattern::with_look(g, plan.look);
        let stride = screen.pitch() as usize;
        let samples = row_costs(&pattern, g, screen.as_mut(), stride, 400);
        out.push(row_measured("row-display", g, &samples, refresh));
        let mut system = vec![0u8; g.line_bytes() * g.height as usize];
        let samples = row_costs(&pattern, g, &mut system, g.line_bytes(), 400);
        out.push(row_measured("row-system", g, &samples, refresh));
        for way in plan.ways.iter().copied().filter(|w| w.pans()) {
            for &spare in &plan.spares {
                let spare = spare.unwrap_or(g.rows());
                for &count in &plan.buffers {
                    for unit in plan.units() {
                        out.extend(measure_pan(
                            &mut kms, screen_fb, g, &pattern, way, spare, count, unit, plan,
                            refresh, facts,
                        )?);
                    }
                }
            }
        }
    }
    kms.wait_flip(BOUND)?;
    kms.remove_framebuffer(screen_fb);
    drop(kms);
    drop(screen);
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn measure_pan(
    kms: &mut Kms,
    screen_fb: u32,
    g: Geometry,
    pattern: &Pattern,
    way: Way,
    spare: u32,
    count: u32,
    unit: Unit,
    plan: &Plan,
    refresh: f64,
    facts: &mut Facts,
) -> io::Result<Vec<Measured>> {
    let mut pan = Pan::with_ahead(g, spare, unit, count as usize, way == Way::PanAhead);
    let height = pan.ring.buffer_height(g);
    let mut dumbs = Vec::new();
    for _ in 0..count {
        dumbs.push(MappedDumb::new(kms.fd(), g.width, height)?);
    }
    let fbs: Vec<u32> = dumbs
        .iter()
        .map(|d| kms.framebuffer(d))
        .collect::<io::Result<_>>()?;
    // The window's lowest start: the ring's last slot, and at pixel steps
    // its last line, where the view stands one line above the next row.
    let last = match unit {
        Unit::Rows => (pan.ring.rows - 1) * g.cell_height,
        Unit::Pixels => pan.ring.rows * g.cell_height - 1,
    };
    let test = match kms.test(fbs[0], last) {
        Ok(()) => "accepted".to_owned(),
        Err(e) => format!("refused: {e}"),
    };
    facts.add(
        &format!(
            "pan {}x{} {}x{}, {spare} spare rows, {} steps, a {}x{height} buffer shown from row {last}",
            g.width,
            g.height,
            g.cell_width,
            g.cell_height,
            unit.name(),
            g.width
        ),
        format!("test commit {test}"),
    );
    // The kernel gives a dumb buffer's pages on first touch, so each buffer
    // is written whole before the frames are timed.
    for d in &mut dumbs {
        d.as_mut().fill(0);
    }
    let pitch = dumbs[0].pitch();
    let mut out = Vec::new();
    let speeds = plan.steps();
    for speed in speeds.iter().filter(|s| s.0 == unit).map(|s| s.1) {
        pan.forget();
        let ch = i64::from(g.cell_height);
        let step = unit.lines(speed, g);
        let mut pos: i64 = (1 << 40) * ch;
        let (mut cpu, mut draw, mut counts) = (Vec::new(), Vec::new(), Vec::new());
        let (mut commits, mut busy, mut refused) = (0, 0, 0);
        let mut started = None;
        for frame in 0..plan.warmup + plan.frames {
            let b = frame % count as usize;
            if frame == plan.warmup {
                started = Some(Instant::now());
            }
            let c0 = cpu_ms();
            // With two buffers, a frame is drawn once the flip before it
            // completes, as drm2d draws, and with three while it flips. With
            // one buffer, the frame waits for that flip too: the buffer is
            // the one being scanned out, and until the flip lands the
            // screen may still show the window of the frame before it, so a
            // frame drawn early writes into a window nobody accounts for.
            if count != 3 {
                kms.wait_flip(BOUND)?;
            }
            let d0 = cpu_ms();
            let (c, src_y) = pan.frame(pattern, g, b, dumbs[b].as_mut(), pitch as usize, pos);
            let d = cpu_ms() - d0;
            kms.wait_flip(BOUND)?;
            commits += 1;
            match kms.flip(fbs[b], src_y) {
                Ok(()) => {}
                Err(e) if e.raw_os_error() == Some(rustix::io::Errno::BUSY.raw_os_error()) => {
                    busy += 1
                }
                Err(e) => {
                    log::warn!("a flip to row {src_y} failed: {e}");
                    refused += 1;
                }
            }
            let c1 = cpu_ms();
            if frame >= plan.warmup {
                cpu.push(c1 - c0);
                draw.push(d);
                counts.push(c);
            }
            pos -= step;
        }
        kms.wait_flip(BOUND)?;
        let elapsed = started.map_or(0.0, |s| s.elapsed().as_secs_f64());
        let m = Measured {
            way: way.name(),
            size: (g.width, g.height),
            cell: (g.cell_width, g.cell_height),
            speed,
            unit,
            buffers: count,
            spare: Some(spare),
            frames: plan.frames,
            cpu: summarize(&cpu),
            draw: summarize(&draw),
            fps: plan.frames as f64 / elapsed.max(1e-9),
            refresh,
            counts: mean_counts(&counts),
            memory: memory::needs(g, way, count, u64::from(pitch), spare, unit),
            commits,
            busy,
            refused,
        };
        eprintln!(
            "scroll-timing: {} {}x{} {}x{} {speed} {} a frame, {count} buffers, {spare} spare: {:.3} ms",
            way.name(),
            g.width,
            g.height,
            g.cell_width,
            g.cell_height,
            unit.name(),
            m.cpu.median
        );
        out.push(m);
    }
    // The screen's own buffer goes back on before the tall ones are freed:
    // removing a framebuffer the primary plane shows turns the CRTC off.
    kms.show(screen_fb)?;
    for fb in fbs {
        kms.remove_framebuffer(fb);
    }
    Ok(out)
}
