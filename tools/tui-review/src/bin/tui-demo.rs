//! Opens vtium-config's TUI in the caller's own terminal on the fixtures
//! its screen goldens draw, so the maintainer can move through the screens
//! the goldens show:
//!
//! ```text
//! tui-demo draft ROOT VTIUM-CONFIG
//! tui-demo figure ROOT
//! ```
//!
//! `draft` runs the real binary VTIUM-CONFIG on the draft of the goldens
//! named `draft-*`: their installed vtium.conf and the keyboard `kbd` at
//! `pci-0:3.3`, which it lays out under ROOT as a sysfs tree and a udev
//! database, with their stand-in console as the check. vtium.conf and the
//! frits live under ROOT, so Install runs `vtium-config apply` as the user
//! into ROOT and never asks for sudo, and the sudo vtium-config is given
//! refuses to run.
//!
//! `figure` runs the TUI in this process on the flow of ADR-1403 D7's
//! figure, which the goldens named `flow`, `card` and `route` draw. That
//! flow is not read from a draft, so the view changes nothing, and Install
//! says that the demo installs nothing.
//!
//! Both refuse to start unless `HOME` and every XDG directory lie under
//! ROOT, so nothing the TUI writes reaches the caller's own home, and both
//! refuse to run as root. `make tui-demo` makes ROOT, sets the variables
//! and removes ROOT afterwards.

use std::ffi::OsString;
use std::io;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use vtium_config::specimen::{Picture, Summary};
use vtium_config::tui::desk::Done;
use vtium_config::tui::fixture;
use vtium_config::tui::model::View;
use vtium_config::tui::port::{Port, Step, Tools, Tty, Wake, drive};

/// The variables that name where the TUI keeps and reads the user's files.
const HOMES: [&str; 5] = [
    "HOME",
    "XDG_CONFIG_HOME",
    "XDG_STATE_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
];

/// Why the demo will not start with `root` as its throwaway directory, if
/// it will not.
fn refusal(root: &Path) -> Option<String> {
    if rustix::process::geteuid().is_root() {
        return Some("run the demo as yourself, never as root".into());
    }
    if !root.is_absolute() {
        return Some(format!("{}: ROOT must be an absolute path", root.display()));
    }
    for var in HOMES {
        let under = std::env::var_os(var).is_some_and(|v| Path::new(&v).starts_with(root));
        if !under {
            return Some(format!(
                "{var} must name a directory under {}, so the demo never touches your own files",
                root.display()
            ));
        }
    }
    None
}

fn write(path: &Path, text: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, text)
}

fn script(path: &Path, text: &str) -> io::Result<()> {
    write(path, text.as_bytes())?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

fn link(path: &Path, target: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    symlink(target, path)
}

/// The goldens' keyboard under `root`: an input device named `kbd` in
/// sysfs, as the kernel lists one, and udev's database entry that gives it
/// the class keyboard and the path `pci-0:3.3`, with udev's control socket
/// standing for a running udev daemon.
fn keyboard(root: &Path) -> io::Result<()> {
    let sys = root.join("sys");
    let input = "devices/platform/demo/input/input3";
    let dir = sys.join(input);
    link(&dir.join("subsystem"), "../../../../../class/input")?;
    for (name, value) in [
        ("name", "kbd"),
        ("capabilities/ev", "3"),
        ("capabilities/key", "fffffffe"),
    ] {
        write(&dir.join(name), format!("{value}\n").as_bytes())?;
    }
    let node = dir.join("event2");
    write(&node.join("dev"), b"13:66\n")?;
    write(
        &node.join("uevent"),
        b"MAJOR=13\nMINOR=66\nDEVNAME=input/event2\n",
    )?;
    link(&node.join("device"), "..")?;
    link(&node.join("subsystem"), "../../../../../../class/input")?;
    link(
        &sys.join("class/input/event2"),
        &format!("../../{input}/event2"),
    )?;
    link(&sys.join("class/input/input3"), &format!("../../{input}"))?;
    std::fs::create_dir_all(sys.join("class/drm"))?;
    write(
        &root.join("udev/data/c13:66"),
        b"I:1000\nE:ID_INPUT=1\nE:ID_INPUT_KEYBOARD=1\nE:ID_PATH=pci-0:3.3\nG:seat\n",
    )?;
    write(&root.join("udev/control"), b"")
}

/// Lays the draft goldens' machine out under `root` and starts the TUI of
/// `program` on it, as the draft `bench` of its installed vtium.conf.
fn draft(root: &Path, program: &Path) -> Result<(), String> {
    let at = |e: io::Error| format!("{}: {e}", root.display());
    let etc = root.join("etc");
    write(&etc.join("vtium.conf"), fixture::DRAFT.as_bytes()).map_err(at)?;
    std::fs::create_dir_all(etc.join("fonts")).map_err(at)?;
    script(&root.join("vtium"), fixture::CHECKER).map_err(at)?;
    script(
        &root.join("sudo"),
        "#!/bin/sh\necho 'the demo never runs sudo' >&2\nexit 1\n",
    )
    .map_err(at)?;
    keyboard(root).map_err(at)?;
    let paths = vtium_config::tui::Paths {
        configdir: etc.clone(),
        frits: Ok(etc.join("fonts")),
        drafts: vtium_config::tui::default_drafts(),
        sudo: root.join("sudo").into(),
        vtium: root.join("vtium").into(),
        helper: vtium_config::specimen::Helper {
            program: program.to_owned(),
            options: Vec::new(),
        },
        sys: root.join("sys"),
    };
    let store = vtium_config::store::Store::locate(
        rustix::process::geteuid().as_raw(),
        |name| std::env::var_os(name),
        |_| None,
    )?;
    vtium_config::drafts::Session {
        store,
        paths: &paths,
    }
    .make("bench", vtium_config::drafts::From::Installed)
    .map_err(|f| f.text().to_string())?;
    let args: [(&str, &Path); 6] = [
        ("--configdir", &etc),
        ("--frits", &etc.join("fonts")),
        ("--vtium", &root.join("vtium")),
        ("--sudo", &root.join("sudo")),
        ("--sys", &root.join("sys")),
        ("--udev-data", &root.join("udev/data")),
    ];
    let mut command = std::process::Command::new(program);
    command.arg("--no-banner");
    for (option, path) in args {
        command.arg(option).arg(path);
    }
    // only returns when the binary did not start
    Err(format!(
        "{}: {}",
        program.display(),
        command.arg("tui").exec()
    ))
}

/// The real terminal, but for installing: the figure's flow comes from no
/// draft, so the demo has nothing to install and says so.
struct Figure(Tty);

impl Port for Figure {
    fn size(&mut self) -> (usize, usize) {
        self.0.size()
    }

    fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.0.write(bytes)
    }

    fn arm(&mut self, after: Duration) {
        self.0.arm(after);
    }

    fn wait(&mut self) -> io::Result<Vec<Wake>> {
        self.0.wait()
    }

    fn privileged(&mut self, _step: Step<'_>) -> Result<Done, String> {
        Err(
            "the demo of the figure's flow installs nothing; make tui-demo opens a draft that does"
                .into(),
        )
    }

    fn on_panic(&mut self, bytes: Vec<u8>) {
        self.0.on_panic(bytes);
    }

    fn cell_px(&mut self) -> Option<(u32, u32)> {
        self.0.cell_px()
    }

    fn picture(&mut self, font: &str, height: u32, max: (u32, u32)) -> Result<Picture, String> {
        self.0.picture(font, height, max)
    }

    fn summary(&mut self, font: &str, height: u32) -> Result<Summary, String> {
        self.0.summary(font, height)
    }

    fn sources(&mut self, fonts: Vec<String>) {
        self.0.sources(fonts);
    }
}

/// Runs the TUI on the figure's flow in this terminal.
fn figure() -> Result<u8, String> {
    let none = "the demo draws no specimens";
    let tools = Tools {
        painter: Box::new(move |_, _, _| Err(none.to_owned())),
        summarizer: Box::new(move |_, _| Err(none.to_owned())),
        finder: None,
    };
    let tty = Tty::open(tools).map_err(|e| format!("the terminal: {e}"))?;
    let mut inputs = fixture::inputs();
    inputs.view = View::Flow;
    drive(&mut Figure(tty), inputs).map_err(|e| format!("the terminal: {e}"))
}

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let usage = || {
        eprintln!("usage: tui-demo draft ROOT VTIUM-CONFIG\n       tui-demo figure ROOT");
        ExitCode::from(2)
    };
    let (mode, root, program) = match args.as_slice() {
        [m, r, p] if m == "draft" => ("draft", PathBuf::from(r), Some(PathBuf::from(p))),
        [m, r] if m == "figure" => ("figure", PathBuf::from(r), None),
        _ => return usage(),
    };
    if let Some(why) = refusal(&root) {
        eprintln!("tui-demo: {why}");
        return ExitCode::FAILURE;
    }
    let result = match (mode, program) {
        ("draft", Some(program)) => draft(&root, &program).map(|()| 0),
        _ => figure(),
    };
    match result {
        Ok(status) => ExitCode::from(status),
        Err(why) => {
            eprintln!("tui-demo: {why}");
            ExitCode::FAILURE
        }
    }
}
