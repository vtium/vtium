//! The `platform` rows: the display, the devices, the configuration and the
//! heads. None of them names a sequence.

use super::{Entry, Other};

pub(super) const ENTRIES: &[Entry] = &[];

pub(super) const OTHER: &[Other] = &[
    Other {
        id: "platform.compositor",
        what: "how vtium composes the screen it draws",
    },
    Other {
        id: "platform.drm2d-fbdev",
        what: "the kernel interfaces vtium draws through",
    },
    Other {
        id: "platform.native-mode",
        what: "the display mode vtium chooses",
    },
    Other {
        id: "platform.vt-switch",
        what: "VT switching and keyboard input",
    },
    Other {
        id: "platform.pty",
        what: "the pty between vtium and its session",
    },
    Other {
        id: "platform.config",
        what: "vtium.conf and the command line",
    },
    Other {
        id: "platform.terminfo",
        what: "the terminfo entry vtium installs",
    },
    Other {
        id: "platform.gpu",
        what: "the GPU renderer",
    },
    Other {
        id: "platform.libseat",
        what: "running under a seat manager",
    },
    Other {
        id: "platform.system-layout",
        what: "the keyboard layout vtium reads from the system",
    },
    Other {
        id: "platform.system-layout-follow",
        what: "following a change to the system's keyboard layout",
    },
    Other {
        id: "platform.multi-monitor",
        what: "one grid across several displays",
    },
    Other {
        id: "platform.hand-back",
        what: "giving the display to a graphical program and taking it back",
    },
    Other {
        id: "platform.heads",
        what: "heads, set in vtium.conf",
    },
    Other {
        id: "platform.heads.rules",
        what: "the rules of vtium.conf",
    },
    Other {
        id: "platform.heads.focus",
        what: "the active head and the roaming chord",
    },
    Other {
        id: "platform.heads.guards",
        what: "the guards against lockout",
    },
    Other {
        id: "platform.heads.rescue-key",
        what: "the rescue key",
    },
    Other {
        id: "platform.heads.on-loss",
        what: "a configuration key, when-display-lost",
    },
    Other {
        id: "platform.heads.isolated",
        what: "a configuration key, isolated",
    },
    Other {
        id: "platform.heads.policy",
        what: "a configuration key, policy",
    },
];
