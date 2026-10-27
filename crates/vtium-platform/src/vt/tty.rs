//! A VT's tty, through the kernel's VT and keyboard ioctls.

use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::path::{Path, PathBuf};

use rustix::fs::{Mode, OFlags};
use rustix::termios::{OptionalActions, QueueSelector, Termios};

use super::{Console, DisplayMode, KbMode, Reldisp, Switching};
use crate::ioctl;

/// `TTY_MAJOR`, `linux/major.h`: the VTs' character devices.
const TTY_MAJOR: u32 = 4;

/// A VT's tty, opened read-write and never as the controlling terminal.
#[derive(Debug)]
pub struct Tty {
    fd: OwnedFd,
}

impl Tty {
    /// Opens the VT at `path` and returns it with its number, the device's
    /// minor (vt_real.c:197-225).
    pub fn open(path: &Path) -> io::Result<(Tty, u16)> {
        log::info!(target: "vt", "using tty {}", path.display());
        let fd = rustix::fs::open(
            path,
            OFlags::RDWR | OFlags::NOCTTY | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .inspect_err(|e| log::error!(target: "vt", "cannot open tty {}: {e}", path.display()))?;
        let stat = rustix::fs::fstat(&fd)?;
        let num = u16::try_from(rustix::fs::minor(stat.st_rdev))
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
        log::debug!(target: "vt", "the tty is VT {num}");
        Ok((Tty { fd }, num))
    }

    /// The VT to use when none is configured (vt_real.c:627-675): the one
    /// stderr is, when it is a VT, or else the first free VT. `None` on a
    /// system without VTs, which gets a [`super::FakeVt`].
    pub fn find() -> io::Result<Option<PathBuf>> {
        if !Path::new("/dev/tty0").exists() {
            return Ok(None);
        }
        if let Ok(stat) = rustix::fs::fstat(io::stderr().as_fd()) {
            if let Some(num) = vt_number(stat.st_rdev) {
                let path = PathBuf::from(format!("/dev/tty{num}"));
                if path.exists() {
                    return Ok(Some(path));
                }
            }
        }
        let flags = OFlags::NONBLOCK | OFlags::NOCTTY | OFlags::CLOEXEC;
        let fd = rustix::fs::open("/dev/tty0", flags, Mode::empty())
            .or_else(|_| rustix::fs::open("/dev/tty1", flags, Mode::empty()))
            .inspect_err(|e| log::error!(target: "vt", "cannot find a parent tty: {e}"))?;
        let num = ioctl::vt_openqry(fd.as_fd())
            .inspect_err(|e| log::error!(target: "vt", "cannot get an unused tty: {e}"))?;
        if num <= 0 {
            log::error!(target: "vt", "no unused tty");
            return Err(io::Error::from(io::ErrorKind::NotFound));
        }
        Ok(Some(PathBuf::from(format!("/dev/tty{num}"))))
    }
}

/// The VT a device number names: the tty major's minors from 1.
fn vt_number(rdev: u64) -> Option<u32> {
    let minor = rustix::fs::minor(rdev);
    (rustix::fs::major(rdev) == TTY_MAJOR && minor > 0).then_some(minor)
}

impl Console for Tty {
    type Termios = Termios;

    fn active(&self) -> io::Result<u16> {
        ioctl::vt_active(self.fd.as_fd())
    }

    fn set_display_mode(&self, mode: DisplayMode) -> io::Result<()> {
        let mode = match mode {
            DisplayMode::Text => ioctl::KD_TEXT,
            DisplayMode::Graphics => ioctl::KD_GRAPHICS,
        };
        ioctl::kd_set_mode(self.fd.as_fd(), mode)
    }

    fn switching_is_ours(&self) -> io::Result<bool> {
        is_ours(&ioctl::vt_get_mode(self.fd.as_fd())?)
    }

    fn set_switching(&self, mode: Switching) -> io::Result<()> {
        let mode = match mode {
            Switching::Auto => ioctl::VtMode {
                mode: ioctl::VT_AUTO,
                ..ioctl::VtMode::default()
            },
            Switching::Process => process_mode()?,
        };
        ioctl::vt_set_mode(self.fd.as_fd(), mode)
    }

    fn keyboard_mode(&self) -> io::Result<KbMode> {
        let raw = ioctl::kd_get_kb_mode(self.fd.as_fd())?;
        let raw = u32::try_from(raw).map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
        Ok(KbMode::from_raw(raw))
    }

    fn set_keyboard_mode(&self, mode: KbMode) -> io::Result<()> {
        ioctl::kd_set_kb_mode(self.fd.as_fd(), mode.raw())
    }

    fn reldisp(&self, answer: Reldisp) -> io::Result<()> {
        let arg = match answer {
            Reldisp::Refuse => 0,
            Reldisp::Allow => 1,
        };
        ioctl::vt_reldisp(self.fd.as_fd(), arg)
    }

    fn activate(&self, num: u16) -> io::Result<()> {
        ioctl::vt_activate(self.fd.as_fd(), num)
    }

    fn flush_input(&self) -> io::Result<()> {
        rustix::termios::tcflush(&self.fd, QueueSelector::IFlush)?;
        Ok(())
    }

    fn termios(&self) -> io::Result<Termios> {
        Ok(rustix::termios::tcgetattr(&self.fd)?)
    }

    fn set_termios(&self, termios: &Termios) -> io::Result<()> {
        rustix::termios::tcsetattr(&self.fd, OptionalActions::Now, termios)?;
        Ok(())
    }

    fn silent(termios: &Termios) -> Termios {
        let mut raw = termios.clone();
        raw.make_raw();
        raw
    }

    fn bell(&self) -> io::Result<()> {
        if rustix::io::write(&self.fd, b"\x07")? == 1 {
            Ok(())
        } else {
            Err(io::Error::from(io::ErrorKind::WriteZero))
        }
    }

    fn try_clone(&self) -> io::Result<Tty> {
        Ok(Tty {
            fd: self.fd.try_clone()?,
        })
    }

    fn watch(&self) -> Option<io::Result<OwnedFd>> {
        Some(self.fd.try_clone())
    }
}

/// A signal's number, as `struct vt_mode` holds it.
fn signal_number(signal: rustix::process::Signal) -> io::Result<i16> {
    i16::try_from(signal.as_raw()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
}

/// vtium's `VT_PROCESS`: SIGUSR2 to release, SIGUSR1 to acquire, as the
/// C reference set them (vt_real.c:239-240).
fn process_mode() -> io::Result<ioctl::VtMode> {
    Ok(ioctl::VtMode {
        mode: ioctl::VT_PROCESS,
        waitv: 0,
        relsig: signal_number(rustix::process::Signal::USR2)?,
        acqsig: signal_number(rustix::process::Signal::USR1)?,
        frsig: 0,
    })
}

/// Whether `mode` is vtium's `VT_PROCESS`. The kernel keeps `waitv` and
/// ignores `frsig`, so only the mode and the two signals count.
fn is_ours(mode: &ioctl::VtMode) -> io::Result<bool> {
    let ours = process_mode()?;
    Ok(mode.mode == ours.mode && mode.relsig == ours.relsig && mode.acqsig == ours.acqsig)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_vtiums_vt_process_is_ours() -> io::Result<()> {
        let ours = process_mode()?;
        assert!(is_ours(&ours)?);
        assert!(is_ours(&ioctl::VtMode { waitv: 1, ..ours })?);
        // Another process's signals, VT_AUTO, and a mode the headers do not
        // name are not.
        assert!(!is_ours(&ioctl::VtMode { relsig: 10, ..ours })?);
        assert!(!is_ours(&ioctl::VtMode { acqsig: 12, ..ours })?);
        assert!(!is_ours(&ioctl::VtMode {
            mode: ioctl::VT_AUTO,
            ..ours
        })?);
        assert!(!is_ours(&ioctl::VtMode { mode: 2, ..ours })?);
        Ok(())
    }

    #[test]
    fn stderr_names_a_vt_only_on_the_tty_major() {
        let dev = |major, minor| rustix::fs::makedev(major, minor);
        assert_eq!(vt_number(dev(TTY_MAJOR, 3)), Some(3));
        assert_eq!(vt_number(dev(TTY_MAJOR, 0)), None);
        assert_eq!(vt_number(dev(136, 3)), None);
    }
}
