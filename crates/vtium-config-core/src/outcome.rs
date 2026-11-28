//! The flow's commands' fixed exit codes (ADR-1403 D8), so a script can
//! tell the outcomes apart. 0 is done; each other code is a [`Fail`].

/// Why a flow command did not finish as asked, each with its exit code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fail {
    /// 1: the check failed where it must pass, at an install; each
    /// problem is printed with its line, and nothing is installed.
    Check(String),
    /// 2: the command could not be used: an unknown option, a rule number,
    /// head or look that does not exist, or a value its key cannot take.
    Usage(String),
    /// 3: the installed vtium.conf changed since the draft was made from
    /// it, so nothing is installed.
    Changed(String),
    /// 4: nothing could be installed without privilege: sudo was refused
    /// or is missing.
    Privilege(String),
    /// 5: a file could not be read or written; the message names its path.
    File(String),
}

impl Fail {
    /// The exit code.
    pub const fn code(&self) -> u8 {
        match self {
            Fail::Check(_) => 1,
            Fail::Usage(_) => 2,
            Fail::Changed(_) => 3,
            Fail::Privilege(_) => 4,
            Fail::File(_) => 5,
        }
    }

    /// What it says.
    pub fn text(&self) -> &str {
        match self {
            Fail::Check(t)
            | Fail::Usage(t)
            | Fail::Changed(t)
            | Fail::Privilege(t)
            | Fail::File(t) => t,
        }
    }
}
