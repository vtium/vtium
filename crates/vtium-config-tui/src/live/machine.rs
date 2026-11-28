//! What the Actions menu's items read and do on the machine, which only the
//! loop calls, since update never reads the machine: the devices connected
//! now, for Read devices again, and the copy of a font file of the
//! person's own into the system's fonts folder, for Install as a system
//! font….
//!
//! [`Machine`] is the seam. [`System`] is this machine: sysfs and udev's
//! database for the devices, and `vtium-config apply` through sudo for the
//! font and for Save ([`crate::save::privileged`]). [`Nowhere`] reads and installs
//! nothing, for a loop that has no machine to act on.
//!
//! References:
//! - ADR-1206 D2: the loop is the only place that reads the machine or
//!   runs the privileged step, and update returns those as effects.
//! - ADR-1205 D5: the install copies one font file into
//!   `/usr/local/share/fonts` through the privileged step, on its own.
//! - ADR-1403 D7: Read devices again reads the connected displays and
//!   input devices again.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use vtium_config_core::apply::{self, Placement, SystemFont};
use vtium_config_core::document::Installed;
use vtium_config_core::draft::Plan;
use vtium_config_core::flowmodel::Present;
use vtium_config_core::places::Paths;
use vtium_platform::listing::Roots;

/// A font file readied for the install as a system font: where it comes
/// from, where it goes, whether a file is there already, and the command
/// that copies it, as the question shows it before it runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ready {
    pub source: PathBuf,
    pub target: PathBuf,
    pub placement: Placement,
    pub command: String,
}

/// What the loop reads and does on the machine for the Actions menu and
/// for Save.
pub trait Machine {
    /// The devices connected now, or `None` where this machine's devices
    /// cannot be read.
    fn devices(&mut self) -> Option<Present>;

    /// Readies the font file `source` for the install: reads it, checks
    /// that it may become a system font, and writes the draft the command
    /// installs. `Err` says why it cannot.
    fn ready_font(&mut self, source: &Path) -> Result<Ready, String>;

    /// Runs the command that installs the font readied last, on the
    /// terminal the loop has handed back, so sudo can ask for the
    /// password. `Err` says why the font was not installed.
    fn install_font(&mut self) -> Result<(), String>;

    /// What is installed now, vtium.conf and the font sets, which Save's
    /// review reads again before it opens. `Err` says why it cannot.
    fn installed(&mut self) -> Result<Installed, String> {
        Err(crate::save::words::NO_MACHINE.to_owned())
    }

    /// Saves the configuration `name` through the privileged step, on the
    /// terminal the loop has handed back, so sudo can ask for the
    /// password. `plan.no_route` is the person's yes to saving a file
    /// with no complete route. `Err` says why nothing was saved.
    fn save(&mut self, _name: &str, _plan: &Plan) -> Result<crate::save::Saved, String> {
        Err(crate::save::words::NO_MACHINE.to_owned())
    }
}

/// A machine with nothing to read or install.
#[derive(Clone, Copy, Debug, Default)]
pub struct Nowhere;

impl Machine for Nowhere {
    fn devices(&mut self) -> Option<Present> {
        None
    }

    fn ready_font(&mut self, _source: &Path) -> Result<Ready, String> {
        Err("vtium-config cannot install fonts here.".to_owned())
    }

    fn install_font(&mut self) -> Result<(), String> {
        Err("No font was readied.".to_owned())
    }
}

/// This machine.
#[derive(Debug)]
pub struct System {
    paths: Paths,
    roots: Roots,
    /// udev's control socket, which says whether its daemon runs.
    control: PathBuf,
    /// The command that installs the font readied last.
    readied: Option<Vec<OsString>>,
}

impl System {
    /// This machine, with vtium-config's `paths`, its devices under
    /// `roots`, and udev's daemon found by its control socket `control`.
    pub fn new(paths: Paths, roots: Roots, control: PathBuf) -> System {
        System {
            paths,
            roots,
            control,
            readied: None,
        }
    }
}

impl Machine for System {
    fn devices(&mut self) -> Option<Present> {
        Some(vtium_config_core::flowmodel::present(
            &self.roots,
            &self.control,
        ))
    }

    fn ready_font(&mut self, source: &Path) -> Result<Ready, String> {
        self.readied = None;
        let targets = self.paths.targets();
        let font = SystemFont::read(source, &targets)?;
        let placement = font.placement();
        let mut ready = Ready {
            source: font.source.clone(),
            target: font.target.clone(),
            placement,
            command: String::new(),
        };
        if placement == Placement::Identical {
            return Ok(ready);
        }
        let draft = font.write_draft(&self.paths.drafts).map_err(|e| {
            format!(
                "vtium-config cannot write the draft under {}: {e}",
                self.paths.drafts.display()
            )
        })?;
        let program = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("vtium-config"));
        let route = apply::route(&targets, &self.paths.sudo);
        let argv = apply::save_argv(&route, &program, &targets, &draft);
        ready.command = vtium_config_core::vts::shown(&argv);
        self.readied = Some(argv);
        Ok(ready)
    }

    fn install_font(&mut self) -> Result<(), String> {
        let Some(argv) = self.readied.take() else {
            return Err("No font was readied.".to_owned());
        };
        let Some((program, args)) = argv.split_first() else {
            return Err("No font was readied.".to_owned());
        };
        let shown = vtium_config_core::vts::shown(&argv);
        let status = std::process::Command::new(program)
            .args(args)
            .status()
            .map_err(|e| format!("{shown}: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("{shown} ended with {status}."))
        }
    }

    fn installed(&mut self) -> Result<Installed, String> {
        crate::save::privileged::installed(&self.paths)
    }

    fn save(&mut self, name: &str, plan: &Plan) -> Result<crate::save::Saved, String> {
        crate::save::privileged::save(&self.paths, name, plan)
    }
}
