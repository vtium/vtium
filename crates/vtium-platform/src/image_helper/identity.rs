//! Whose account the image helper runs as (ADR-1101 D2): the session's
//! user, found again before each request, or the pre-login account, or
//! nobody's at all.
//!
//! The rules read facts about the system through [`Facts`], so each rule
//! is tested against a stand-in; [`System`] reads the real ones.
//!
//! | Situation | File media | Inline PNG and zlib |
//! |---|---|---|
//! | logind running, and the login child in a logind session of its own whose class is a login | read as that session's user, root included | decoded as that session's user |
//! | logind running, and the child in no session of its own | refused | decoded as the pre-login account |
//! | built without `logind`, or no logind running | read as the pty's owner; refused when that owner is root | decoded as the pty's owner; when that owner is root, as the pre-login account |
//!
//! A vtium not running as root starts a helper only as itself, and only
//! when the session's user is its own user; holding group 0 in any form,
//! it refuses every request.

use std::collections::BTreeSet;
use std::ffi::CString;

/// The session classes `pam_systemd(8)` gives a fully logged-in user.
pub const LOGIN_CLASSES: [&str; 4] = ["user", "user-early", "user-light", "user-early-light"];

/// The pre-login account the package creates through its sysusers file,
/// and the one taken when it is missing.
pub const PRELOGIN_ACCOUNTS: [&str; 2] = ["vtium-image", "nobody"];

/// The login child's logind session, as logind describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginSession {
    pub uid: u32,
    pub class: String,
    /// The session is vtium's own, which the child inherited.
    pub shared: bool,
}

/// The account vtium runs as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Me {
    pub uid: u32,
    pub gid: u32,
    /// vtium holds group 0 in any form: real, effective, saved or
    /// supplementary.
    pub group_zero: bool,
}

/// What the rules read from the system.
pub trait Facts {
    /// Whether logind runs: never in a build without `logind`; an error
    /// counts as running, so an unknown state refuses files.
    fn logind_running(&self) -> bool;
    /// The logind session of process `pid`, if it is in one and logind
    /// answers.
    fn logind_session(&self, pid: u32) -> Option<LoginSession>;
    /// The account vtium runs as.
    fn me(&self) -> Me;
    /// The uid and primary gid of the account named `name`.
    fn account(&self, name: &str) -> Option<(u32, u32)>;
    /// The primary gid and the supplementary groups of the account `uid`.
    fn groups(&self, uid: u32) -> Option<(u32, Vec<u32>)>;
}

/// The console's session, as the binary sees it before each request.
#[derive(Debug)]
pub struct Session {
    /// The login child's pid, while it runs.
    pub child: Option<u32>,
    /// The owner of the session's pty, as `login` sets it, or why it could
    /// not be read.
    pub pty_owner: Result<u32, String>,
}

/// The ids a helper takes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub uid: u32,
    pub gid: u32,
    /// The supplementary groups to set, or `None` to keep vtium's, which a
    /// vtium not running as root cannot change.
    pub groups: Option<Vec<u32>>,
}

/// Why a request takes no helper.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// File media before login, or from a pty root owns with no logind.
    BeforeLogin,
    /// No account may read it: no pre-login account, a vtium not running
    /// as root whose user is not the session's, or one holding group 0.
    NoAccount(String),
}

/// The account for one request of file media (`file`) or inline bytes,
/// and whether a disagreement between the pty's owner and logind should
/// be logged, as `(owner, logind)`.
pub fn resolve(
    facts: &dyn Facts,
    prelogin: &mut Prelogin,
    session: &Session,
    file: bool,
) -> (Result<Account, Refusal>, Option<(u32, u32)>) {
    let me = facts.me();
    if me.uid != 0 {
        return (as_me(facts, &me, session), None);
    }
    let logind = session
        .child
        .and_then(|child| facts.logind_session(child))
        .filter(|s| !s.shared && LOGIN_CLASSES.contains(&s.class.as_str()));
    if let Some(s) = logind {
        let disagree = session
            .pty_owner
            .as_ref()
            .ok()
            .filter(|&&owner| owner != s.uid)
            .map(|&owner| (owner, s.uid));
        return (user(facts, s.uid), disagree);
    }
    if facts.logind_running() {
        return (before_login(facts, prelogin, file), None);
    }
    let answer = match &session.pty_owner {
        Err(why) => Err(Refusal::NoAccount(format!(
            "the pty's owner cannot be read: {why}"
        ))),
        // never root by fallback: a root pty is a login prompt's
        Ok(0) => before_login(facts, prelogin, file),
        Ok(owner) => user(facts, *owner),
    };
    (answer, None)
}

/// The session's user `uid`, with its primary group and supplementary
/// groups.
fn user(facts: &dyn Facts, uid: u32) -> Result<Account, Refusal> {
    let (gid, groups) = facts
        .groups(uid)
        .ok_or_else(|| Refusal::NoAccount(format!("no account has uid {uid}")))?;
    Ok(Account {
        uid,
        gid,
        groups: Some(groups),
    })
}

/// Before login: no file, and inline bytes as the pre-login account.
fn before_login(
    facts: &dyn Facts,
    prelogin: &mut Prelogin,
    file: bool,
) -> Result<Account, Refusal> {
    if file {
        return Err(Refusal::BeforeLogin);
    }
    let (uid, gid) = prelogin
        .get(facts)
        .ok_or_else(|| Refusal::NoAccount("neither vtium-image nor nobody exists".to_owned()))?;
    Ok(Account {
        uid,
        gid,
        groups: Some(Vec::new()),
    })
}

/// A vtium not running as root: a helper as itself only, for a session
/// whose user is vtium's own, and none while it holds group 0.
fn as_me(facts: &dyn Facts, me: &Me, session: &Session) -> Result<Account, Refusal> {
    if me.group_zero {
        return Err(Refusal::NoAccount(
            "vtium is not root but holds group 0".to_owned(),
        ));
    }
    let session_user = session
        .child
        .and_then(|child| facts.logind_session(child))
        .filter(|s| !s.shared && LOGIN_CLASSES.contains(&s.class.as_str()))
        .map(|s| s.uid)
        .or_else(|| session.pty_owner.as_ref().ok().copied());
    if session_user != Some(me.uid) {
        return Err(Refusal::NoAccount(format!(
            "vtium runs as uid {} and cannot start a helper as the session's user",
            me.uid
        )));
    }
    Ok(Account {
        uid: me.uid,
        gid: me.gid,
        groups: None,
    })
}

/// The pre-login account, looked up the first time it is needed and
/// remembered, a miss included, so a slow name service cannot stall the
/// login prompt on every picture. An account that resolves to uid 0 or
/// gid 0 is passed over for the next.
#[derive(Clone, Debug, Default)]
pub struct Prelogin(Option<Option<(u32, u32)>>);

impl Prelogin {
    pub fn get(&mut self, facts: &dyn Facts) -> Option<(u32, u32)> {
        *self.0.get_or_insert_with(|| {
            PRELOGIN_ACCOUNTS
                .iter()
                .filter_map(|name| facts.account(name))
                .find(|&(uid, gid)| uid != 0 && gid != 0)
        })
    }
}

/// Whether this process holds group 0 in any form: real, effective,
/// saved or supplementary. One function makes the check, and it reads the
/// group list at the size `getgroups` reports; an error counts as holding
/// it.
pub fn holds_group_zero() -> bool {
    let (mut r, mut e, mut s) = (0, 0, 0);
    #[expect(unsafe_code, reason = "getresgid(2), which rustix does not wrap")]
    // SAFETY: getresgid writes one gid through each pointer, each a live
    // local of the right type.
    let read = unsafe { libc::getresgid(&mut r, &mut e, &mut s) } == 0;
    if !read || r == 0 || e == 0 || s == 0 {
        return true;
    }
    rustix::process::getgroups().map_or(true, |groups| groups.iter().any(|g| g.as_raw() == 0))
}

/// The real system's facts.
#[derive(Clone, Copy, Debug, Default)]
pub struct System;

impl Facts for System {
    fn logind_running(&self) -> bool {
        #[cfg(feature = "logind")]
        return super::login::seats() != Some(0);
        #[cfg(not(feature = "logind"))]
        false
    }

    fn logind_session(&self, pid: u32) -> Option<LoginSession> {
        #[cfg(feature = "logind")]
        return super::login::session(pid);
        #[cfg(not(feature = "logind"))]
        {
            let _ = pid;
            None
        }
    }

    fn me(&self) -> Me {
        Me {
            uid: rustix::process::geteuid().as_raw(),
            gid: rustix::process::getegid().as_raw(),
            group_zero: holds_group_zero(),
        }
    }

    fn account(&self, name: &str) -> Option<(u32, u32)> {
        let name = CString::new(name).ok()?;
        passwd(|pw, buf, res| {
            #[expect(
                unsafe_code,
                reason = "getpwnam_r(3), the name service's account lookup"
            )]
            // SAFETY: every pointer is a live local or the caller's buffer
            // of the length given; the answer points into that buffer.
            unsafe {
                libc::getpwnam_r(name.as_ptr(), pw, buf.as_mut_ptr(), buf.len(), res)
            }
        })
        .map(|(uid, gid, _)| (uid, gid))
    }

    fn groups(&self, uid: u32) -> Option<(u32, Vec<u32>)> {
        let (_, gid, name) = passwd(|pw, buf, res| {
            #[expect(
                unsafe_code,
                reason = "getpwuid_r(3), the name service's account lookup"
            )]
            // SAFETY: as in `account`.
            unsafe {
                libc::getpwuid_r(uid, pw, buf.as_mut_ptr(), buf.len(), res)
            }
        })?;
        let name = CString::new(name).ok()?;
        let mut n: libc::c_int = 32;
        loop {
            let mut list = vec![0 as libc::gid_t; usize::try_from(n).ok()?];
            let mut got = n;
            #[expect(
                unsafe_code,
                reason = "getgrouplist(3), the name service's group lookup"
            )]
            // SAFETY: `list` holds `got` gids, which getgrouplist writes at
            // most, and it writes the count it needs through `got`.
            let r = unsafe { libc::getgrouplist(name.as_ptr(), gid, list.as_mut_ptr(), &mut got) };
            if r >= 0 {
                list.truncate(usize::try_from(got).ok()?);
                let unique: BTreeSet<u32> = list.into_iter().collect();
                return Some((gid, unique.into_iter().collect()));
            }
            if got <= n || got > 65_536 {
                return None;
            }
            n = got;
        }
    }
}

/// One `passwd` lookup through `call`, its buffer grown until the entry
/// fits: the entry's uid, gid and name.
fn passwd(
    call: impl Fn(*mut libc::passwd, &mut Vec<libc::c_char>, *mut *mut libc::passwd) -> libc::c_int,
) -> Option<(u32, u32, Vec<u8>)> {
    let mut size = 16_384usize;
    loop {
        let mut buf: Vec<libc::c_char> = vec![0; size];
        #[expect(unsafe_code, reason = "a zeroed passwd, which getpw*_r fills")]
        // SAFETY: `passwd` is plain old data, for which all zeroes is a
        // valid value of every field.
        let mut pw: libc::passwd = unsafe { std::mem::zeroed() };
        let mut res: *mut libc::passwd = std::ptr::null_mut();
        let r = call(&mut pw, &mut buf, &mut res);
        if r == libc::ERANGE && size < 1 << 22 {
            size = size.saturating_mul(2);
            continue;
        }
        if r != 0 || res.is_null() || pw.pw_name.is_null() {
            return None;
        }
        #[expect(unsafe_code, reason = "the name getpw*_r wrote into the buffer")]
        // SAFETY: on success pw_name points at a NUL-terminated string in
        // `buf`, which lives to the end of this block.
        let name = unsafe { std::ffi::CStr::from_ptr(pw.pw_name) }
            .to_bytes()
            .to_vec();
        return Some((pw.pw_uid, pw.pw_gid, name));
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// A stand-in system: logind's state, the child's session, vtium's
    /// account, and an account table that counts its lookups.
    struct Stand {
        running: bool,
        session: Option<LoginSession>,
        me: Me,
        accounts: Vec<(&'static str, u32, u32)>,
        lookups: Cell<u32>,
    }

    impl Facts for Stand {
        fn logind_running(&self) -> bool {
            self.running
        }
        fn logind_session(&self, _: u32) -> Option<LoginSession> {
            self.session.clone()
        }
        fn me(&self) -> Me {
            self.me.clone()
        }
        fn account(&self, name: &str) -> Option<(u32, u32)> {
            self.lookups.set(self.lookups.get() + 1);
            self.accounts
                .iter()
                .find(|a| a.0 == name)
                .map(|a| (a.1, a.2))
        }
        fn groups(&self, uid: u32) -> Option<(u32, Vec<u32>)> {
            Some((uid + 100, vec![uid + 100, 7]))
        }
    }

    fn root() -> Stand {
        Stand {
            running: true,
            session: None,
            me: Me {
                uid: 0,
                gid: 0,
                group_zero: true,
            },
            accounts: vec![("vtium-image", 980, 980), ("nobody", 65534, 65534)],
            lookups: Cell::new(0),
        }
    }

    fn login(uid: u32, class: &str) -> Option<LoginSession> {
        Some(LoginSession {
            uid,
            class: class.to_owned(),
            shared: false,
        })
    }

    fn session(owner: Result<u32, String>) -> Session {
        Session {
            child: Some(42),
            pty_owner: owner,
        }
    }

    fn account(facts: &Stand, owner: Result<u32, String>, file: bool) -> Result<Account, Refusal> {
        resolve(facts, &mut Prelogin::default(), &session(owner), file).0
    }

    fn user(uid: u32) -> Result<Account, Refusal> {
        Ok(Account {
            uid,
            gid: uid + 100,
            groups: Some(vec![uid + 100, 7]),
        })
    }

    const PRELOGIN: Result<Account, Refusal> = Ok(Account {
        uid: 980,
        gid: 980,
        groups: Some(Vec::new()),
    });

    // covers: image.helper
    #[test]
    fn a_logind_login_is_the_sessions_user_root_included() {
        for class in LOGIN_CLASSES {
            let facts = Stand {
                session: login(1000, class),
                ..root()
            };
            assert_eq!(account(&facts, Ok(1000), true), user(1000), "{class}");
        }
        // a root login logind authenticated runs its helper as root
        let facts = Stand {
            session: login(0, "user-early"),
            ..root()
        };
        assert_eq!(account(&facts, Ok(0), true), user(0));
        // logind's answer wins over the pty's owner, and is reported once
        let facts = Stand {
            session: login(1000, "user"),
            ..root()
        };
        let (got, disagree) = resolve(&facts, &mut Prelogin::default(), &session(Ok(1001)), true);
        assert_eq!((got, disagree), (user(1000), Some((1001, 1000))));
    }

    // covers: image.helper
    #[test]
    fn before_login_files_are_refused_and_inline_bytes_take_the_prelogin_account() {
        // a greeter's class, a session shared with vtium, and none at all
        for s in [
            login(1000, "greeter"),
            Some(LoginSession {
                shared: true,
                ..login(1000, "user").unwrap()
            }),
            None,
        ] {
            let facts = Stand {
                session: s,
                ..root()
            };
            assert_eq!(account(&facts, Ok(1000), true), Err(Refusal::BeforeLogin));
            assert_eq!(account(&facts, Ok(1000), false), PRELOGIN);
        }
    }

    #[test]
    fn without_logind_the_ptys_owner_but_never_root() {
        let facts = Stand {
            running: false,
            ..root()
        };
        assert_eq!(account(&facts, Ok(1000), true), user(1000));
        assert_eq!(account(&facts, Ok(0), true), Err(Refusal::BeforeLogin));
        assert_eq!(account(&facts, Ok(0), false), PRELOGIN);
        assert!(matches!(
            account(&facts, Err("gone".to_owned()), false),
            Err(Refusal::NoAccount(_))
        ));
    }

    #[test]
    fn the_prelogin_account_falls_to_nobody_skips_root_and_is_looked_up_once() {
        let facts = Stand {
            accounts: vec![("vtium-image", 0, 980), ("nobody", 65534, 65534)],
            ..root()
        };
        let mut prelogin = Prelogin::default();
        assert_eq!(prelogin.get(&facts), Some((65534, 65534)));
        assert_eq!(prelogin.get(&facts), Some((65534, 65534)));
        assert_eq!(facts.lookups.get(), 2, "both names, once");
        let none = Stand {
            accounts: Vec::new(),
            ..root()
        };
        let mut prelogin = Prelogin::default();
        assert_eq!(prelogin.get(&none), None);
        assert_eq!(prelogin.get(&none), None);
        assert_eq!(none.lookups.get(), 2, "a miss is remembered too");
        assert!(matches!(
            account(&none, Ok(0), false),
            Err(Refusal::NoAccount(_))
        ));
    }

    #[test]
    fn a_vtium_not_running_as_root_starts_a_helper_only_as_itself() {
        let me = Stand {
            running: false,
            me: Me {
                uid: 1000,
                gid: 1000,
                group_zero: false,
            },
            ..root()
        };
        let mine = Ok(Account {
            uid: 1000,
            gid: 1000,
            groups: None,
        });
        assert_eq!(account(&me, Ok(1000), true), mine);
        assert_eq!(account(&me, Ok(1000), false), mine);
        assert!(matches!(
            account(&me, Ok(1001), false),
            Err(Refusal::NoAccount(_))
        ));
        assert!(matches!(
            account(&me, Ok(0), false),
            Err(Refusal::NoAccount(_))
        ));
        let zero = Stand {
            me: Me {
                group_zero: true,
                ..me.me.clone()
            },
            ..me
        };
        assert!(matches!(
            account(&zero, Ok(1000), false),
            Err(Refusal::NoAccount(_))
        ));
    }

    #[test]
    fn group_zero_is_read_from_this_process_and_accounts_from_the_name_service() {
        let me = System.me();
        assert_eq!(me.uid, rustix::process::geteuid().as_raw());
        if me.uid != 0 {
            // the test runs as an ordinary user, whose own groups decide
            let groups = rustix::process::getgroups().unwrap();
            let zero = groups.iter().any(|g| g.as_raw() == 0)
                || rustix::process::getgid().as_raw() == 0
                || rustix::process::getegid().as_raw() == 0;
            assert_eq!(me.group_zero, zero);
        }
        assert_eq!(System.account("root"), Some((0, 0)));
        let (gid, groups) = System.groups(0).unwrap();
        assert_eq!(gid, 0);
        assert!(groups.contains(&0));
    }
}
