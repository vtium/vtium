//! [`RenderTimer`]: one display's frame figures, closed into a
//! [`Report`] once an interval, as the C reference's render timer counted
//! them (`render_timer.c`). It only measures and never schedules a frame;
//! the caller passes every time in, so it holds no clock of its own.

use core::fmt;
use std::time::Duration;

/// Frames whose time an interval keeps for its median and 95th
/// percentile. Frames past it still count in the mean and the maximum.
pub const RENDER_TIMER_SAMPLES: usize = 1024;

/// One interval's figures.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub interval: Duration,
    /// Flips completed: frames presented.
    pub flips: u32,
    /// Frames drawn and queued for a flip.
    pub frames: u32,
    /// Draws that changed nothing and queued no flip.
    pub skipped: u32,
    pub cpu_mean: Duration,
    pub cpu_median: Duration,
    pub cpu_p95: Duration,
    pub cpu_max: Duration,
    /// All skipped draws together.
    pub skipped_cpu: Duration,
    /// Pixels in a queued frame's damage, on average.
    pub damage_px_mean: u64,
    /// The display's pixels when the interval closed.
    pub screen_px: u64,
    /// Redraws a hold of synchronized output owed and did not draw.
    pub held: u32,
    /// Holds of synchronized output that reached their timeout.
    pub expired: u32,
    /// Glyphs drawn from the font, every frame together.
    pub misses: u32,
    /// The most one frame drew.
    pub misses_max: u32,
}

/// One display's counters for the current interval.
#[derive(Clone, Debug, Default)]
pub struct RenderTimer {
    flips: u32,
    frames: u32,
    skipped: u32,
    cpu_sum: Duration,
    cpu_max: Duration,
    skipped_cpu: Duration,
    damage_px_sum: u64,
    held: u32,
    expired: u32,
    misses: u32,
    misses_max: u32,
    samples: Vec<Duration>,
    /// The last interval was reported.
    reported: bool,
}

/// The nearest-rank percentile `p` of the sorted `v`.
fn percentile(v: &[Duration], p: usize) -> Duration {
    let rank = v.len().saturating_mul(p).saturating_add(99) / 100;
    v.get(rank.saturating_sub(1)).copied().unwrap_or_default()
}

impl RenderTimer {
    pub fn new() -> RenderTimer {
        RenderTimer::default()
    }

    /// A draw ended after `cpu`: queued for a flip with `damage_px` pixels
    /// of damage, or skipped when it changed nothing. It drew `misses`
    /// glyphs from the font.
    pub fn frame(&mut self, cpu: Duration, queued: bool, damage_px: u64, misses: u32) {
        self.misses = self.misses.saturating_add(misses);
        self.misses_max = self.misses_max.max(misses);
        if !queued {
            self.skipped = self.skipped.saturating_add(1);
            self.skipped_cpu = self.skipped_cpu.saturating_add(cpu);
            return;
        }
        if self.samples.len() < RENDER_TIMER_SAMPLES {
            self.samples.push(cpu);
        }
        self.frames = self.frames.saturating_add(1);
        self.cpu_sum = self.cpu_sum.saturating_add(cpu);
        self.cpu_max = self.cpu_max.max(cpu);
        self.damage_px_sum = self.damage_px_sum.saturating_add(damage_px);
    }

    /// A flip completed.
    pub fn flip(&mut self) {
        self.flips = self.flips.saturating_add(1);
    }

    /// A redraw was owed to a hold of synchronized output.
    pub fn held(&mut self) {
        self.held = self.held.saturating_add(1);
    }

    /// A hold of synchronized output reached its timeout.
    pub fn expired(&mut self) {
        self.expired = self.expired.saturating_add(1);
    }

    /// Closes the interval, `interval` long on a display of `screen_px`
    /// pixels, and starts the next. A report comes for an interval with a
    /// flip or a queued frame, and for the first quiet one after it, so a
    /// log shows where activity stopped; skipped draws alone are quiet.
    pub fn report(&mut self, interval: Duration, screen_px: u64) -> Option<Report> {
        let active = self.flips > 0 || self.frames > 0;
        let line = active || self.reported;
        let mut t = core::mem::take(self);
        self.reported = active;
        if !line {
            return None;
        }
        t.samples.sort_unstable();
        let frames = t.frames;
        let (mean, damage) = match frames {
            0 => (Duration::ZERO, 0),
            n => (
                t.cpu_sum.checked_div(n).unwrap_or_default(),
                t.damage_px_sum.checked_div(u64::from(n)).unwrap_or(0),
            ),
        };
        Some(Report {
            interval,
            flips: t.flips,
            frames,
            skipped: t.skipped,
            cpu_mean: mean,
            cpu_median: percentile(&t.samples, 50),
            cpu_p95: percentile(&t.samples, 95),
            cpu_max: t.cpu_max,
            skipped_cpu: t.skipped_cpu,
            damage_px_mean: damage,
            screen_px,
            held: t.held,
            expired: t.expired,
            misses: t.misses,
            misses_max: t.misses_max,
        })
    }
}

/// `d` in milliseconds to three places.
struct Ms(Duration);

impl fmt::Display for Ms {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let us = self.0.as_micros();
        write!(f, "{}.{:03}", us / 1000, us % 1000)
    }
}

impl Report {
    /// Flips a second, in hundredths.
    fn rate(&self) -> u128 {
        let ns = self.interval.as_nanos();
        u128::from(self.flips)
            .saturating_mul(100_000_000_000)
            .checked_div(ns)
            .unwrap_or(0)
    }

    /// The damaged share of the screen, in hundredths of a percent.
    fn damage_pct(&self) -> u64 {
        self.damage_px_mean
            .saturating_mul(10_000)
            .checked_div(self.screen_px)
            .unwrap_or(0)
    }

    /// The one line the on-screen display draws: frames presented a second,
    /// the median time a frame, the share of the screen damaged, then the
    /// misses and the holds when there are any.
    pub fn osd(&self) -> String {
        let ns = self.interval.as_nanos();
        let rate = u128::from(self.flips)
            .saturating_mul(10_000_000_000)
            .saturating_add(ns / 2)
            .checked_div(ns)
            .unwrap_or(0);
        let pct = self
            .damage_px_mean
            .saturating_mul(1000)
            .saturating_add(self.screen_px / 2)
            .checked_div(self.screen_px)
            .unwrap_or(0);
        let mut s = format!(
            "{}.{} fps {} ms {}.{}%",
            rate / 10,
            rate % 10,
            Ms(self.cpu_median),
            pct / 10,
            pct % 10
        );
        if self.misses > 0 {
            s.push_str(&format!(" {} misses {} max", self.misses, self.misses_max));
        }
        if self.held > 0 || self.expired > 0 {
            s.push_str(&format!(" {} held {} expired", self.held, self.expired));
        }
        s
    }
}

/// The log line, in the C reference's fields.
impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (rate, pct) = (self.rate(), self.damage_pct());
        write!(
            f,
            "render-timer: interval_ms={} presented_per_s={}.{:02} flips={} frames={} \
             cpu_ms_mean={} cpu_ms_median={} cpu_ms_p95={} cpu_ms_max={} damage_px_mean={} \
             damage_pct={}.{:02} skipped={} skipped_cpu_ms={} held={} expired={} \
             glyph_misses={} glyph_misses_max={}",
            Ms(self.interval),
            rate / 100,
            rate % 100,
            self.flips,
            self.frames,
            Ms(self.cpu_mean),
            Ms(self.cpu_median),
            Ms(self.cpu_p95),
            Ms(self.cpu_max),
            self.damage_px_mean,
            pct / 100,
            pct % 100,
            self.skipped,
            Ms(self.skipped_cpu),
            self.held,
            self.expired,
            self.misses,
            self.misses_max,
        )
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn an_interval_reports_its_frames() {
        let mut t = RenderTimer::new();
        for (i, cpu) in [3, 1, 2, 10].into_iter().enumerate() {
            t.frame(ms(cpu), true, 1000 * (i as u64 + 1), 2);
            t.flip();
        }
        t.frame(ms(5), false, 0, 0);
        t.held();
        let r = t.report(Duration::from_secs(1), 10_000).unwrap();
        assert_eq!((r.flips, r.frames, r.skipped), (4, 4, 1));
        assert_eq!(
            (r.cpu_mean, r.cpu_median, r.cpu_p95, r.cpu_max),
            (ms(4), ms(2), ms(10), ms(10))
        );
        assert_eq!(
            (r.damage_px_mean, r.misses, r.misses_max, r.held),
            (2500, 8, 2, 1)
        );
        assert_eq!(r.skipped_cpu, ms(5));
        let line = r.to_string();
        assert!(line.contains("presented_per_s=4.00 flips=4"), "{line}");
        assert!(
            line.contains("cpu_ms_median=2.000 cpu_ms_p95=10.000"),
            "{line}"
        );
        assert!(line.contains("damage_pct=25.00"), "{line}");
        assert_eq!(
            r.osd(),
            "4.0 fps 2.000 ms 25.0% 8 misses 2 max 1 held 0 expired"
        );
    }

    // render_timer_report: the first quiet interval after activity is
    // reported, later quiet ones are not, and skipped draws are quiet
    #[test]
    fn quiet_intervals_report_once() {
        let mut t = RenderTimer::new();
        t.frame(ms(1), true, 1, 0);
        assert!(t.report(ms(1000), 1).is_some());
        t.frame(ms(1), false, 0, 0);
        let quiet = t.report(ms(1000), 1).unwrap();
        assert_eq!((quiet.frames, quiet.skipped), (0, 1));
        assert!(t.report(ms(1000), 1).is_none());
        t.flip();
        assert_eq!(
            t.report(ms(500), 1).unwrap().to_string().split(' ').nth(2),
            Some("presented_per_s=2.00")
        );
    }
}
