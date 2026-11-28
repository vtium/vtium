//! The command line's options that need a value, in the one table that the
//! parser, `--help` and vtium-config(1) read.

/// An option that needs a value: its name, the value as the usage writes
/// it, whether that value names a path or a program, and what the option
/// does, as `--help` and vtium-config(1) show it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValueOption {
    pub name: &'static str,
    pub value: &'static str,
    pub path: bool,
    pub does: &'static str,
}

/// Every option that needs a value: the one table the parser reads its
/// options from, the usage and vtium-config(1) list them from, and root's
/// refusal reads the path and program options from ([`crate::cli::root_refusal`]), so
/// an option the parser takes cannot be missed by the refusal or the help. Run as root, vtium-config takes none of the flagged
/// ones, so `sudo vtium-config` reads and writes only the system's
/// directories and runs only the console it was built with.
pub const VALUE_OPTIONS: [ValueOption; 14] = [
    ValueOption {
        name: "--configdir",
        value: "DIR",
        path: true,
        does: "the directory of vtium.conf, instead of /etc/vtium",
    },
    ValueOption {
        name: "--frits",
        value: "DIR",
        path: true,
        does: "the font set directory, instead of the one font-frits names",
    },
    ValueOption {
        name: "--sudo",
        value: "PROGRAM",
        path: true,
        does: "the program that runs apply as root, instead of sudo",
    },
    ValueOption {
        name: "--vtium",
        value: "PROGRAM",
        path: true,
        does: "the vtium binary apply checks files with, instead of /usr/bin/vtium",
    },
    ValueOption {
        name: "--kbd-dir",
        value: "DIR",
        path: true,
        does: "a kbd data directory, instead of the built-in list (repeatable)",
    },
    ValueOption {
        name: "--font-dir",
        value: "DIR",
        path: true,
        does: "an outline font directory, instead of the built-in list (repeatable)",
    },
    ValueOption {
        name: "--vconsole-conf",
        value: "FILE",
        path: true,
        does: "vconsole.conf, instead of /etc/vconsole.conf",
    },
    ValueOption {
        name: "--kernel-cmdline",
        value: "FILE",
        path: true,
        does: "the kernel command line, instead of /proc/cmdline",
    },
    ValueOption {
        name: "--sys",
        value: "DIR",
        path: true,
        does: "sysfs, where displays and input devices are read, instead of /sys",
    },
    ValueOption {
        name: "--udev-data",
        value: "DIR",
        path: true,
        does: "udev's database, instead of /run/udev/data",
    },
    ValueOption {
        name: "--helper",
        value: "PROGRAM",
        path: true,
        does: "the font helper to start, instead of this program's serve",
    },
    ValueOption {
        name: "--root",
        value: "DIR",
        path: true,
        does: "the root directory report reads the machine under, instead of /",
    },
    ValueOption {
        name: "--locale",
        value: "LOCALE",
        path: false,
        does: "the console's locale, which serve picks the built-in font by, instead of C",
    },
    ValueOption {
        name: "--sandbox",
        value: "on|off",
        path: false,
        does: "whether serve installs its seccomp filters and enters its landlock domain, on unless off",
    },
];
