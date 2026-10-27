//! The crate's bounds: one source for the constants the code uses and for
//! the table in this crate's README.
//!
//! Each bound is a [`Bound`] row holding its value, what happens past it,
//! and where it comes from; the constant takes its value from the row. The
//! README table is generated from [`BOUNDS`], and
//! `readme_table_matches_bounds` fails when the committed copy differs.
//! `VTIUM_PLATFORM_BLESS=1 cargo test -p vtium-platform readme_table`
//! rewrites it.

/// One bound and its account, a row of the README table.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct Bound {
    name: &'static str,
    value: usize,
    what: &'static str,
    past: &'static str,
    cite: &'static str,
}

const READ: Bound = Bound {
    name: "READ_MAX",
    value: 16 * 1024,
    what: "Bytes one read takes from the pty",
    past: "The rest stays in the kernel for the next read",
    cite: "`KMSCON_NREAD`, pty.c:50",
};

const READS: Bound = Bound {
    name: "READS_PER_TURN",
    value: 50,
    what: "Reads one readiness event makes before the loop serves its other sources",
    past: "The pty's registration is renewed, so the next turn reads on",
    cite: "pty.c:627-651",
};

const HIGH: Bound = Bound {
    name: "REPLY_HIGH",
    value: 64 * 1024,
    what: "Replies queued for the child, past which its output is no longer read",
    past: "The consumer stops after the reply that reached it, and nothing more is read or fed until the replies drain below `REPLY_LOW`, so a child that queries without reading stalls itself",
    cite: "`PTY_REPLY_HIGH`, pty.c:68; aaronsb/kmscon#245",
};

const LOW: Bound = Bound {
    name: "REPLY_LOW",
    value: 16 * 1024,
    what: "Replies queued for the child below which reading resumes after a stall",
    past: "Reading and feeding resume",
    cite: "`PTY_REPLY_LOW`, pty.c:69",
};

const CEILING: Bound = Bound {
    name: "REPLY_MAX",
    value: 256 * 1024,
    what: "Replies queued for the child, a ceiling only a consumer that does not stop at `REPLY_HIGH` reaches",
    past: "The reply that would pass it and every later one are dropped until the replies drain below `REPLY_LOW`",
    cite: "`PTY_REPLY_MAX`, pty.c:70, 479-484",
};

const MOTION: Bound = Bound {
    name: "MOTION_COALESCE_AT",
    value: 4 * 1024,
    what: "Bytes queued for the child from which pointer motion reports are coalesced",
    past: "A motion report replaces the unsent motion reports at the queue's end, so the child reads the latest position; keys, pastes and button reports are queued whole and never dropped. The C reference queued every motion report (aaronsb/kmscon#247)",
    cite: "vtium/vtium#20",
};

const EXIT_FEED: Bound = Bound {
    name: "EXIT_FEED_MAX",
    value: 64 * 1024,
    what: "Bytes fed after the child exits before the exit is told, unless a read finds nothing more first",
    past: "The exit is told and the host closes the pty, dropping what a process the child started writes on. A pty holds at most about 20 KiB for its master (4 KiB in the line discipline and 16 KiB of flip buffers, measured on Linux 7.2), all of it ahead of anything written after the exit, so the child's own output is fed first",
    cite: "vtium/vtium#20; the C reference told the exit at the child's reaping and dropped what was unread (pty.c:737-768, 830-848)",
};

const FLIP_WAIT: Bound = Bound {
    name: "FLIP_WAIT_MS",
    value: 1000,
    what: "Milliseconds a modeset or a sleep waits for a page flip in flight to complete",
    past: "The operation goes ahead and the flip is no longer waited for. The loop runs on while it waits: the operation starts from the flip's completion event or from this timer",
    cite: "drm_shared.c:834-859, where the C reference blocked its loop for the wait; the operator's decision on vtium/vtium#18",
};

const WAKE_RETRY: Bound = Bound {
    name: "WAKE_RETRY_MS",
    value: 50,
    what: "Milliseconds between tries to wake a DRM card that failed to wake, as while another process holds DRM master",
    past: "The next try",
    cite: "drm_shared.c:1212-1224",
};

const RETRIES: Bound = Bound {
    name: "WAKE_RETRIES",
    value: 20,
    what: "Tries to wake a DRM card after a wake-up or a hotplug scan failed",
    past: "The tries stop with `Event::WakeFailed`, until the next wake-up or hotplug",
    cite: "`MAX_VT_TIMEOUT_RETRIES`, drm_shared.c:51",
};

const DAMAGE_CLIPS: Bound = Bound {
    name: "DAMAGE_CLIPS_MAX",
    value: 128,
    what: "Damage rectangles one swap sends, its frame's and those of earlier frames whose swaps failed",
    past: "The rectangles become their bounding box. The C reference sent each frame's rectangles and freed them on a failed swap",
    cite: "vtium/vtium#18",
};

/// Every bound, in the README table's order.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const BOUNDS: [Bound; 11] = [
    READ,
    READS,
    HIGH,
    LOW,
    CEILING,
    MOTION,
    EXIT_FEED,
    FLIP_WAIT,
    WAKE_RETRY,
    RETRIES,
    DAMAGE_CLIPS,
];

// The stall ends below where it starts, and starts below the ceiling.
const _: () = assert!(LOW.value < HIGH.value && HIGH.value < CEILING.value);

/// Bytes one read takes from the pty.
pub const READ_MAX: usize = READ.value;

/// Reads one readiness event makes before the loop serves its other
/// sources.
pub const READS_PER_TURN: usize = READS.value;

/// Replies queued for the child, past which its output is no longer read.
pub const REPLY_HIGH: usize = HIGH.value;

/// Replies queued for the child below which reading resumes after a stall.
pub const REPLY_LOW: usize = LOW.value;

/// Replies queued for the child past which replies are dropped.
pub const REPLY_MAX: usize = CEILING.value;

/// Bytes queued for the child from which pointer motion reports are
/// coalesced.
pub const MOTION_COALESCE_AT: usize = MOTION.value;

/// Bytes fed after the child exits before the exit is told.
pub const EXIT_FEED_MAX: usize = EXIT_FEED.value;

/// Milliseconds a modeset or a sleep waits for a page flip in flight.
pub const FLIP_WAIT_MS: usize = FLIP_WAIT.value;

/// Milliseconds between tries to wake a DRM card.
pub const WAKE_RETRY_MS: usize = WAKE_RETRY.value;

/// Tries to wake a DRM card before giving up until the next wake-up.
pub const WAKE_RETRIES: usize = RETRIES.value;

/// Damage rectangles one swap sends before they become their bounding
/// box.
pub const DAMAGE_CLIPS_MAX: usize = DAMAGE_CLIPS.value;

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    const BEGIN: &str = "<!-- bounds table: generated from src/bounds.rs -->\n";
    const END: &str = "<!-- end of bounds table -->\n";

    /// The README table, generated from [`BOUNDS`].
    fn table() -> String {
        let mut t = String::from(
            "| Bound | Value | What it bounds | Past it | Source |\n|---|---|---|---|---|\n",
        );
        for b in &BOUNDS {
            t += &format!(
                "| `{}` | {} | {} | {} | {} |\n",
                b.name, b.value, b.what, b.past, b.cite
            );
        }
        t
    }

    #[test]
    fn readme_table_matches_bounds() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/README.md");
        let readme = std::fs::read_to_string(path).expect("README.md");
        let start = readme
            .find(BEGIN)
            .expect("README.md lacks the begin marker")
            + BEGIN.len();
        let end = readme.find(END).expect("README.md lacks the end marker");
        let want = table();
        if readme[start..end] == want {
            return;
        }
        if std::env::var_os("VTIUM_PLATFORM_BLESS").is_some() {
            let fresh = format!("{}{want}{}", &readme[..start], &readme[end..]);
            std::fs::write(path, fresh).expect("rewrite README.md");
            return;
        }
        panic!(
            "README.md's bounds table differs from src/bounds.rs; \
             rerun with VTIUM_PLATFORM_BLESS=1 to rewrite it. Generated:\n{want}"
        );
    }

    // pty.c:50, 68-70; drm_shared.c:51, 838, 1220
    #[test]
    fn values_are_the_c_references() {
        assert_eq!(READ_MAX, 16384);
        assert_eq!(READS_PER_TURN, 50);
        assert_eq!(REPLY_HIGH, 64 * 1024);
        assert_eq!(REPLY_LOW, 16 * 1024);
        assert_eq!(REPLY_MAX, 4 * REPLY_HIGH);
        assert_eq!(FLIP_WAIT_MS, 1000);
        assert_eq!(WAKE_RETRY_MS, 50);
        assert_eq!(WAKE_RETRIES, 20);
    }
}
