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

const BUDGET: Bound = Bound {
    name: "READ_BUDGET",
    value: 64 * 1024,
    what: "Bytes one readiness event reads from the pty before the loop serves its other sources",
    past: "The pty's registration is renewed, so the next turn reads on, and the loop serves every other head's keys, output and frames in between",
    cite: "ADR-1401 D2, where one head cannot delay another head's frames or any input; the C reference bounded the reads alone (pty.c:627-651)",
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
    what: "Milliseconds each page flip in flight waits for its completion event: a swap's, a cursor commit's or a modeset's, and a modeset or a sleep waiting on it. A commit the card refused as busy waits as long for the event of the display's next vblank",
    past: "The flip is taken as done, logged at warn level for the first lost event until the card delivers one again: the display reports `Event::Flipped`, or nothing for a vblank, and lends its token again, and a modeset or a sleep waiting on it goes ahead. The loop runs on while it waits",
    cite: "drm_shared.c:834-859, where the C reference blocked its loop for the wait and bounded no swap's flip; the operator's decision on vtium/vtium#18",
};

const FENCE_WAIT: Bound = Bound {
    name: "GPU_FENCE_WAIT_MS",
    value: 2000,
    what: "Milliseconds a head's render thread waits for the GPU to finish a frame, on the fence the frame ended with, in a build with `gpu`",
    past: "The fence is destroyed and the frame fails as a lost context, so the head draws on the CPU path until vtium restarts (ADR-1303 D9): a GPU or a virtual GPU's host that stalls would otherwise hold the VT switch, which waits for the frame composing",
    cite: "ADR-1303 D7; the C reference waited in `eglSwapBuffers`, with no bound",
};

const WAKE_RETRY: Bound = Bound {
    name: "WAKE_RETRY_MS",
    value: 50,
    what: "Milliseconds between tries to wake a DRM card that failed to wake, as while another process holds DRM master, and between tries to unblank a framebuffer",
    past: "The next try",
    cite: "drm_shared.c:1212-1224",
};

const RETRIES: Bound = Bound {
    name: "WAKE_RETRIES",
    value: 2 * FLIP_WAIT.value / WAKE_RETRY.value,
    what: "Tries to wake a DRM card after a wake-up, a hotplug scan or an unblank's modeset failed, and to unblank a framebuffer after `FBIOBLANK` failed",
    past: "The tries stop with `Event::WakeFailed`, until the next wake-up, hotplug, key press or pointer action; a framebuffer's, until the next wake-up, unblank, key press or pointer action",
    cite: "Twice `FLIP_WAIT_MS` at `WAKE_RETRY_MS`, so the tries outlast the sleep of a process that held DRM master before, which a flip in flight delays by up to `FLIP_WAIT_MS`; the C reference tried 20 times (`MAX_VT_TIMEOUT_RETRIES`, drm_shared.c:51). vtium/vtium#19",
};

const DAMAGE_CLIPS: Bound = Bound {
    name: "DAMAGE_CLIPS_MAX",
    value: 128,
    what: "Damage rectangles one swap sends, its frame's and those of earlier frames whose swaps failed",
    past: "The rectangles become their bounding box. The C reference sent each frame's rectangles and freed them on a failed swap",
    cite: "vtium/vtium#18",
};

const INPUT_READS: Bound = Bound {
    name: "INPUT_READS_PER_TURN",
    value: 16,
    what: "Reads of 16 events each that one readiness event makes on an input device before the loop serves its other sources",
    past: "The device stays readable, and the next turn reads on",
    cite: "vtium/vtium#19; the C reference read 16 events at a time until a short read, with no cap (input.c:78-113)",
};

const RETAKE_QUIET: Bound = Bound {
    name: "RETAKE_QUIET_MS",
    value: 10_000,
    what: "Milliseconds with no re-take of the VT after which the next re-take is logged as a warning again",
    past: "The next re-take is a warning and its count starts again; until then each is counted at debug level, so a steady stream of input on the tty logs one warning",
    cite: "vtium/vtium#19; the C reference flushed the tty's input and logged nothing (vt_real.c:184-186)",
};

const RESOURCE_READS: Bound = Bound {
    name: "RESOURCE_READS",
    value: 8,
    what: "Reads of a DRM card's CRTC and connector lists, read again while a count is larger than the list read, as a connector plugged in between two reads makes it",
    past: "The read fails, as a failed hotplug scan or wake-up does, and the next one reads again",
    cite: "vtium/vtium#89; drm-ffi's `get_resources` read twice and set each list's length to the count read, past its buffer",
};

const BLOB: Bound = Bound {
    name: "BLOB_MAX",
    value: 4096,
    what: "Bytes of a KMS property blob vtium reads, as a connector's `PATH`",
    past: "The blob is not read: a warning names the connector, whose display is bound without a path and keeps none while the connector stays connected",
    cite: "the kernel builds a DisplayPort multi-stream connector's `PATH` in a 255-byte buffer; drm-ffi's `get_property_blob` read a blob of any length the kernel gave",
};

const MODES: Bound = Bound {
    name: "MODES_MAX",
    value: 1024,
    what: "Modes of a DRM connector vtium reads",
    past: "The connector is not read: a warning names it, the scan leaves it unbound, and the next scan reads it again. The operator's monitor lists 40 modes (measured on Linux 7.2)",
    cite: "vtium/vtium#192; the kernel sets no limit on a connector's modes. The C reference read them through libdrm with no bound (drm_shared.c:734, 1485)",
};

const ENCODERS: Bound = Bound {
    name: "ENCODERS_MAX",
    value: 32,
    what: "Encoders of a DRM connector vtium reads",
    past: "The connector is not read, as past `MODES_MAX`",
    cite: "vtium/vtium#192; the kernel gives a card at most 32 encoders, since it keeps their indices in 32-bit masks (`__drm_encoder_init`)",
};

const OBJECT_PROPERTIES: Bound = Bound {
    name: "OBJECT_PROPERTIES_MAX",
    value: 64,
    what: "Properties of one KMS object (a connector, CRTC or plane) vtium reads",
    past: "The object's properties are not read: a warning names the object. A connector found by the scan is left unbound and read again at the next scan; a plane is passed over, as one whose properties cannot be read",
    cite: "vtium/vtium#192; the kernel attaches at most 64 properties to an object (`DRM_OBJECT_MAX_PROPERTY`). The C reference read them through libdrm with no bound (drm_shared.c:114)",
};

const PLANES: Bound = Bound {
    name: "PLANES_MAX",
    value: 32,
    what: "Planes of a DRM card vtium reads",
    past: "The planes are not listed: a warning says so, and a display bound meanwhile finds no primary plane and is bound again at the next scan",
    cite: "vtium/vtium#192; the kernel gives a card at most 32 planes, since it keeps their indices in 32-bit masks (`__drm_universal_plane_init`). The C reference read them through libdrm with no bound (drm_shared.c:238)",
};

const PROPERTIES: Bound = Bound {
    name: "UDEV_PROPERTIES_MAX",
    value: 128,
    what: "udev properties an input device's identity keeps for a head file's `udev:` match",
    past: "The device's later properties, in udev's order, are left out with a warning, so no rule matches them; the classes and the named keys are read first. The operator's devices carry at most 42 (measured under systemd 262)",
    cite: "ADR-1401 D4; vtium/vtium#47",
};

const PROPERTY: Bound = Bound {
    name: "UDEV_PROPERTY_MAX",
    value: 512,
    what: "Bytes of one udev property, its name and value, or of one sysfs attribute an input device's identity reads (its name, unique id, vendor and product)",
    past: "The property or attribute is left out of the identity, so no rule matches it; it is never cut short, which a glob could match. The operator's longest property is 209 bytes",
    cite: "ADR-1401 D4; vtium/vtium#47",
};

const IMAGE_CRASHES: Bound = Bound {
    name: "IMAGE_CRASHES_MAX",
    value: 3,
    what: "Image helper crashes one console's session takes: a worker killed by a signal, a helper that ends with an exit status other than 0, or a reply that fails a check; a helper that idles out as a request goes out fails that request and counts nothing",
    past: "That console starts no more image helpers until its session ends, logs it once, and answers as with no helper; other consoles keep theirs",
    cite: "ADR-1101 D8, the maintainer (2026-11-01); the C reference restarted its helper after every failure",
};

const IMAGE_BASE: Bound = Bound {
    name: "IMAGE_HELPER_BASE",
    value: 64 << 20,
    what: "Bytes of address space the image helper has beyond three copies of a request's `max_bytes`, for its code and libraries",
    past: "An allocation past the cap fails: the decoders answer `OVER_BUDGET`, and anything else ends the worker",
    cite: "`HELPER_AS_BASE`, image_host.c:48; ADR-1101 D3",
};

/// Every bound, in the README table's order.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const BOUNDS: [Bound; 25] = [
    READ,
    READS,
    BUDGET,
    HIGH,
    LOW,
    CEILING,
    MOTION,
    EXIT_FEED,
    FLIP_WAIT,
    FENCE_WAIT,
    WAKE_RETRY,
    RETRIES,
    DAMAGE_CLIPS,
    INPUT_READS,
    RETAKE_QUIET,
    RESOURCE_READS,
    BLOB,
    MODES,
    ENCODERS,
    OBJECT_PROPERTIES,
    PLANES,
    PROPERTIES,
    PROPERTY,
    IMAGE_CRASHES,
    IMAGE_BASE,
];

// A turn's share of a pty is whole reads at the largest.
const _: () = assert!(BUDGET.value.is_multiple_of(READ.value));

// The stall ends below where it starts, and starts below the ceiling.
const _: () = assert!(LOW.value < HIGH.value && HIGH.value < CEILING.value);

// A VT switched with no handshake leaves the previous holder of DRM master
// up to a flip wait to sleep and drop it; the wake-up tries outlast that
// twice over.
const _: () = assert!(RETRIES.value * WAKE_RETRY.value >= 2 * FLIP_WAIT.value);

/// Bytes one read takes from the pty.
pub const READ_MAX: usize = READ.value;

/// Reads one readiness event makes before the loop serves its other
/// sources.
pub const READS_PER_TURN: usize = READS.value;

/// Bytes one readiness event reads from the pty before the loop serves
/// its other sources.
pub const READ_BUDGET: usize = BUDGET.value;

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

/// Milliseconds a page flip in flight waits for its completion event.
pub const FLIP_WAIT_MS: usize = FLIP_WAIT.value;

/// Milliseconds a render thread waits on a frame's fence; see the README's
/// bounds table.
pub const GPU_FENCE_WAIT_MS: usize = FENCE_WAIT.value;

/// Milliseconds between tries to wake a DRM card.
pub const WAKE_RETRY_MS: usize = WAKE_RETRY.value;

/// Tries to wake a DRM card or unblank a framebuffer before giving up
/// until the next wake-up, hotplug or input.
pub const WAKE_RETRIES: usize = RETRIES.value;

/// Damage rectangles one swap sends before they become their bounding
/// box.
pub const DAMAGE_CLIPS_MAX: usize = DAMAGE_CLIPS.value;

/// Reads one readiness event makes on an input device.
pub const INPUT_READS_PER_TURN: usize = INPUT_READS.value;

/// Milliseconds with no re-take of the VT after which the next is logged
/// as a warning again.
pub const RETAKE_QUIET_MS: usize = RETAKE_QUIET.value;

/// Reads of a DRM card's CRTC and connector lists before one fails.
pub const RESOURCE_READS_MAX: usize = RESOURCE_READS.value;

/// Image helper crashes one console's session takes before it starts no
/// more.
pub const IMAGE_CRASHES_MAX: usize = IMAGE_CRASHES.value;

/// Bytes of address space the image helper has beyond three copies of a
/// request's `max_bytes`.
pub const IMAGE_HELPER_BASE: usize = IMAGE_BASE.value;

/// Bytes of a KMS property blob vtium reads.
pub(crate) const BLOB_MAX: usize = BLOB.value;

/// Modes of a DRM connector vtium reads.
pub(crate) const MODES_MAX: usize = MODES.value;

/// Encoders of a DRM connector vtium reads.
pub(crate) const ENCODERS_MAX: usize = ENCODERS.value;

/// Properties of one KMS object vtium reads.
pub(crate) const OBJECT_PROPERTIES_MAX: usize = OBJECT_PROPERTIES.value;

/// Planes of a DRM card vtium reads.
pub(crate) const PLANES_MAX: usize = PLANES.value;

/// udev properties an input device's identity keeps.
pub(crate) const UDEV_PROPERTIES_MAX: usize = PROPERTIES.value;

/// Bytes of one udev property or sysfs attribute an input device's
/// identity keeps.
pub(crate) const UDEV_PROPERTY_MAX: usize = PROPERTY.value;

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

    // pty.c:50, 68-70; drm_shared.c:838, 1220
    #[test]
    fn values_are_the_c_references() {
        assert_eq!(READ_MAX, 16384);
        assert_eq!(READS_PER_TURN, 50);
        assert_eq!(REPLY_HIGH, 64 * 1024);
        assert_eq!(REPLY_LOW, 16 * 1024);
        assert_eq!(REPLY_MAX, 4 * REPLY_HIGH);
        assert_eq!(FLIP_WAIT_MS, 1000);
        assert_eq!(WAKE_RETRY_MS, 50);
    }
}
