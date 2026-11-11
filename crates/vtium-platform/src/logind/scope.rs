//! A systemd scope of its own for each login child, in the libseat build
//! under logind (ADR-1400 D3, its note of 2026-11-10 on each login's own
//! session).
//!
//! vtium holds the seat through a logind session of its own, which its unit
//! opens with `PAMName=vtium`. A child vtium forks starts in that session's
//! scope, and `pam_systemd` in `login` then creates no session for the
//! user, because logind takes a process whose cgroup is a session scope as
//! already in a session. So before the child runs `login`, vtium asks
//! systemd's manager to move it into a transient scope of its own, through
//! `StartTransientUnit` on `org.freedesktop.systemd1.Manager`
//! (org.freedesktop.systemd1(5), systemd 262; external, not re-verified),
//! and waits for that job. tmux puts each pane's process in a scope the same
//! way (tmux's `compat/systemd.c`, read online at commit a550207d).
//!
//! The scope is named `vtium-login-<where>-<pid>.scope`: `<where>` is the
//! VT's name, such as `tty2`, or the seat's name on a seat without VTs, and
//! `<pid>` is the child's process id. A counter would start again at each
//! start of vtium, while a scope that an earlier vtium made can still hold
//! a login that survived it (ADR-1401 D13); a process id names one living
//! process. The scope holds the child only until `pam_systemd` moves `login`
//! into its session's scope; with `CollectMode=inactive-or-failed` systemd
//! then forgets the empty scope.

use std::collections::VecDeque;
use std::ffi::{CStr, CString, c_char, c_int, c_uint, c_void};
use std::fmt;
use std::io;
use std::path::Path;
use std::ptr;
use std::time::{Duration, Instant};

use super::{
    BusError, CALL_TIMEOUT_US, Error, classify, sd_bus_error_free, sd_bus_flush_close_unref,
    sd_bus_message_unref, sd_bus_open_system, sd_bus_set_method_call_timeout,
};

/// systemd's manager on the system bus.
const SYSTEMD1: &CStr = c"org.freedesktop.systemd1";
const SYSTEMD1_PATH: &CStr = c"/org/freedesktop/systemd1";
const MANAGER: &CStr = c"org.freedesktop.systemd1.Manager";

/// The directory that exists only while systemd is PID 1, which
/// sd_booted(3) checks (systemd at 78ab4b10, man/sd_booted.xml:55).
const BOOTED: &str = "/run/systemd/system";

/// How long vtium waits for systemd to finish the scope's job, after the
/// call itself, which [`CALL_TIMEOUT_US`] bounds.
const JOB_TIMEOUT: Duration = Duration::from_millis(crate::bounds::SCOPE_JOB_MS as u64);

/// One property of the transient unit, as `StartTransientUnit` takes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Property {
    /// A property whose value is a string.
    Text(&'static str, String),
    /// A property whose value is an array of 32-bit numbers, `PIDs`.
    Numbers(&'static str, Vec<u32>),
}

/// The calls to systemd's manager that [`start_with`] makes, so the tests
/// can stand in for the bus.
pub(crate) trait Bus {
    /// Starts listening for the manager's `JobRemoved` signal. It comes
    /// before the call, so that a job that finishes at once is not missed.
    fn watch_jobs(&mut self) -> Result<(), Error>;
    /// `StartTransientUnit(name, mode, properties, no auxiliary units)`,
    /// which answers the path of the job it queued.
    fn start_transient_unit(
        &mut self,
        name: &str,
        mode: &str,
        properties: &[Property],
    ) -> Result<String, Error>;
    /// The next job the manager reported finished, as its path and its
    /// result, or `None` once `deadline` has passed.
    fn next_removed_job(&mut self, deadline: Instant) -> Result<Option<(String, String)>, Error>;
}

/// Why a login child got no scope of its own.
#[derive(Debug)]
pub enum ScopeError {
    /// systemd is not PID 1, so there is no manager to ask.
    NoSystemd,
    /// The system bus cannot be reached, or the manager refused.
    Bus(Error),
    /// The scope's job did not finish within `SCOPE_JOB_MS` (the crate's
    /// bounds).
    TimedOut,
    /// The scope's job finished with a result other than `done`.
    Failed(String),
}

impl fmt::Display for ScopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScopeError::NoSystemd => write!(f, "systemd is not the init process"),
            ScopeError::Bus(Error::NoBus(e)) => write!(f, "there is no system bus: {e}"),
            ScopeError::Bus(Error::NotRunning(name)) => write!(f, "systemd answered {name}"),
            ScopeError::Bus(Error::Refused { name, message }) => {
                write!(f, "systemd answered {name}: {message}")
            }
            ScopeError::TimedOut => write!(
                f,
                "systemd did not start the scope within {} seconds",
                JOB_TIMEOUT.as_secs()
            ),
            ScopeError::Failed(result) => write!(f, "systemd's job for the scope ended {result}"),
        }
    }
}

/// The scope's unit name for the child `pid` started on `place`: the VT's
/// name, such as `tty2`, or the seat's.
pub fn unit_name(place: &str, pid: u32) -> String {
    format!("vtium-login-{place}-{pid}.scope")
}

/// Moves the child `pid` into a new scope named `unit`, and returns once
/// systemd has done it. Where systemd is not PID 1 it asks nothing.
pub fn start(unit: &str, pid: u32) -> Result<(), ScopeError> {
    if !Path::new(BOOTED).is_dir() {
        return Err(ScopeError::NoSystemd);
    }
    let mut bus = SystemBus::open().map_err(ScopeError::Bus)?;
    #[expect(
        clippy::disallowed_methods,
        reason = "the job's wait is bounded in time"
    )]
    let now = Instant::now();
    start_with(
        &mut bus,
        unit,
        pid,
        now.checked_add(JOB_TIMEOUT).unwrap_or(now),
    )
}

/// [`start`] on any bus, waiting for the job until `deadline`.
pub(crate) fn start_with(
    bus: &mut impl Bus,
    unit: &str,
    pid: u32,
    deadline: Instant,
) -> Result<(), ScopeError> {
    bus.watch_jobs().map_err(ScopeError::Bus)?;
    let properties = [
        Property::Text(
            "Description",
            format!("The login vtium started as process {pid}"),
        ),
        Property::Numbers("PIDs", vec![pid]),
        Property::Text("CollectMode", "inactive-or-failed".to_owned()),
    ];
    // "fail": systemd refuses the job rather than replace one queued for a
    // unit of the same name.
    let job = bus
        .start_transient_unit(unit, "fail", &properties)
        .map_err(ScopeError::Bus)?;
    loop {
        match bus.next_removed_job(deadline).map_err(ScopeError::Bus)? {
            None => return Err(ScopeError::TimedOut),
            Some((path, result)) if path == job => {
                return if result == "done" {
                    Ok(())
                } else {
                    Err(ScopeError::Failed(result))
                };
            }
            // another client's job
            Some(_) => {}
        }
    }
}

#[expect(unsafe_code, reason = "sd-bus's C interface")]
unsafe extern "C" {
    fn sd_bus_message_new_method_call(
        bus: *mut c_void,
        m: *mut *mut c_void,
        destination: *const c_char,
        path: *const c_char,
        interface: *const c_char,
        member: *const c_char,
    ) -> c_int;
    fn sd_bus_message_append(m: *mut c_void, types: *const c_char, ...) -> c_int;
    fn sd_bus_message_open_container(
        m: *mut c_void,
        kind: c_char,
        contents: *const c_char,
    ) -> c_int;
    fn sd_bus_message_close_container(m: *mut c_void) -> c_int;
    fn sd_bus_message_read(m: *mut c_void, types: *const c_char, ...) -> c_int;
    fn sd_bus_call(
        bus: *mut c_void,
        m: *mut c_void,
        usec: u64,
        ret_error: *mut BusError,
        reply: *mut *mut c_void,
    ) -> c_int;
    fn sd_bus_match_signal(
        bus: *mut c_void,
        slot: *mut *mut c_void,
        sender: *const c_char,
        path: *const c_char,
        interface: *const c_char,
        member: *const c_char,
        callback: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut BusError) -> c_int,
        userdata: *mut c_void,
    ) -> c_int;
    fn sd_bus_process(bus: *mut c_void, r: *mut *mut c_void) -> c_int;
    fn sd_bus_wait(bus: *mut c_void, timeout_usec: u64) -> c_int;
    fn sd_bus_slot_unref(slot: *mut c_void) -> *mut c_void;
}

/// An errno sd-bus returned, negated, as an error of the bus.
fn errno(code: c_int) -> Error {
    Error::NoBus(io::Error::from_raw_os_error(code.saturating_neg()))
}

/// The finished jobs the `JobRemoved` handler has read and the caller has
/// not yet taken, each as its path and its result.
type Removed = VecDeque<(String, String)>;

/// The system bus, through sd-bus, with the slot of the `JobRemoved` match
/// once [`Bus::watch_jobs`] has added it.
struct SystemBus {
    bus: *mut c_void,
    slot: *mut c_void,
    /// Where the handler puts what it reads; boxed so that its address,
    /// which sd-bus holds, stays the same.
    removed: Box<Removed>,
}

impl SystemBus {
    fn open() -> Result<SystemBus, Error> {
        let mut bus: *mut c_void = ptr::null_mut();
        #[expect(unsafe_code, reason = "sd-bus's C interface")]
        // SAFETY: `bus` is a valid out pointer; sd-bus sets it only on
        // success.
        let opened = unsafe { sd_bus_open_system(&mut bus) };
        if opened < 0 {
            return Err(errno(opened));
        }
        let bus = SystemBus {
            bus,
            slot: ptr::null_mut(),
            removed: Box::default(),
        };
        #[expect(unsafe_code, reason = "sd-bus's C interface")]
        // SAFETY: `bus.bus` is the connection just opened.
        let timed = unsafe { sd_bus_set_method_call_timeout(bus.bus, CALL_TIMEOUT_US) };
        if timed < 0 {
            log::warn!(target: "login", "cannot bound systemd's answer: {}",
                io::Error::from_raw_os_error(timed.saturating_neg()));
        }
        Ok(bus)
    }
}

impl Drop for SystemBus {
    fn drop(&mut self) {
        #[expect(unsafe_code, reason = "sd-bus's C interface")]
        // SAFETY: each is what sd-bus gave, freed once: the slot first,
        // which stops the handler before its queue goes (null-tolerant),
        // then the connection.
        unsafe {
            sd_bus_slot_unref(self.slot);
            sd_bus_flush_close_unref(self.bus);
        }
    }
}

/// A string sd-bus owns as a Rust string; empty when it is null.
fn text(p: *const c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    #[expect(unsafe_code, reason = "a string sd-bus owns")]
    // SAFETY: sd-bus gives NUL-ended strings that live as long as the error
    // or message they belong to, which the callers keep until after this.
    let s = unsafe { CStr::from_ptr(p) };
    Some(s.to_string_lossy().into_owned())
}

/// The `JobRemoved(u id, o job, s unit, s result)` signal's handler: puts
/// the job's path and result in the queue that `userdata` points to.
#[expect(unsafe_code, reason = "sd-bus calls it with its message")]
unsafe extern "C" fn job_removed(m: *mut c_void, userdata: *mut c_void, _: *mut BusError) -> c_int {
    let mut id: u32 = 0;
    let mut job: *const c_char = ptr::null();
    let mut unit: *const c_char = ptr::null();
    let mut result: *const c_char = ptr::null();
    // SAFETY: `m` is the signal sd-bus is dispatching; each out pointer is
    // a live local of the type the signature "uoss" reads into.
    let read = unsafe {
        sd_bus_message_read(
            m,
            c"uoss".as_ptr(),
            &mut id as *mut u32,
            &mut job as *mut *const c_char,
            &mut unit as *mut *const c_char,
            &mut result as *mut *const c_char,
        )
    };
    if read < 0 {
        return 0;
    }
    if let (Some(job), Some(result)) = (text(job), text(result)) {
        // SAFETY: `userdata` is the `removed` box of the `SystemBus` whose
        // slot this handler is on; the slot goes before the box does, and
        // sd-bus calls the handler only from that bus's own process call,
        // on this thread.
        let removed = unsafe { &mut *userdata.cast::<Removed>() };
        removed.push_back((job, result));
    }
    0
}

impl Bus for SystemBus {
    fn watch_jobs(&mut self) -> Result<(), Error> {
        let userdata: *mut Removed = &mut *self.removed;
        #[expect(unsafe_code, reason = "sd-bus's C interface")]
        // SAFETY: `self.bus` is the open connection; every string is a
        // NUL-ended literal; `userdata` is the boxed queue, which outlives
        // the slot (see Drop).
        let r = unsafe {
            sd_bus_match_signal(
                self.bus,
                &mut self.slot,
                SYSTEMD1.as_ptr(),
                SYSTEMD1_PATH.as_ptr(),
                MANAGER.as_ptr(),
                c"JobRemoved".as_ptr(),
                job_removed,
                userdata.cast(),
            )
        };
        if r < 0 { Err(errno(r)) } else { Ok(()) }
    }

    fn start_transient_unit(
        &mut self,
        name: &str,
        mode: &str,
        properties: &[Property],
    ) -> Result<String, Error> {
        let c = |s: &str| CString::new(s).map_err(|_| errno(-libc::EINVAL));
        let (name, mode) = (c(name)?, c(mode)?);
        let mut m: *mut c_void = ptr::null_mut();
        #[expect(unsafe_code, reason = "sd-bus's C interface")]
        // SAFETY: `self.bus` is the open connection, the strings are
        // NUL-ended, and `m` is a valid out pointer.
        let r = unsafe {
            sd_bus_message_new_method_call(
                self.bus,
                &mut m,
                SYSTEMD1.as_ptr(),
                SYSTEMD1_PATH.as_ptr(),
                MANAGER.as_ptr(),
                c"StartTransientUnit".as_ptr(),
            )
        };
        if r < 0 {
            return Err(errno(r));
        }
        let built = append(m, &name, &mode, properties);
        let mut error = BusError {
            name: ptr::null(),
            message: ptr::null(),
            need_free: 0,
        };
        let mut reply: *mut c_void = ptr::null_mut();
        let result = built.and_then(|()| {
            #[expect(unsafe_code, reason = "sd-bus's C interface")]
            // SAFETY: `m` is the complete call; `error` starts as
            // SD_BUS_ERROR_NULL and `reply` as null, as sd-bus requires;
            // 0 takes the bus's own call timeout, set at open.
            let called = unsafe { sd_bus_call(self.bus, m, 0, &mut error, &mut reply) };
            if called < 0 {
                return Err(classify(text(error.name), text(error.message), called));
            }
            let mut job: *const c_char = ptr::null();
            #[expect(unsafe_code, reason = "sd-bus's C interface")]
            // SAFETY: `reply` is the answer just received; "o" reads one
            // object path into the pointer, which lives as long as the
            // reply.
            let read = unsafe { sd_bus_message_read(reply, c"o".as_ptr(), &mut job) };
            if read < 0 {
                return Err(errno(read));
            }
            text(job).ok_or_else(|| errno(-libc::EBADMSG))
        });
        #[expect(unsafe_code, reason = "sd-bus's C interface")]
        // SAFETY: each is what sd-bus gave above, freed once: the error (a
        // no-op while it is null), the reply and the call (null-tolerant).
        unsafe {
            sd_bus_error_free(&mut error);
            sd_bus_message_unref(reply);
            sd_bus_message_unref(m);
        }
        result
    }

    fn next_removed_job(&mut self, deadline: Instant) -> Result<Option<(String, String)>, Error> {
        loop {
            if let Some(job) = self.removed.pop_front() {
                return Ok(Some(job));
            }
            #[expect(unsafe_code, reason = "sd-bus's C interface")]
            // SAFETY: `self.bus` is the open connection; a null message
            // pointer asks for none back.
            let processed = unsafe { sd_bus_process(self.bus, ptr::null_mut()) };
            if processed < 0 {
                return Err(errno(processed));
            }
            if processed > 0 {
                continue;
            }
            #[expect(
                clippy::disallowed_methods,
                reason = "the job's wait is bounded in time"
            )]
            let now = Instant::now();
            let Some(left) = deadline
                .checked_duration_since(now)
                .filter(|d| !d.is_zero())
            else {
                return Ok(None);
            };
            let usec = u64::try_from(left.as_micros()).unwrap_or(u64::MAX);
            #[expect(unsafe_code, reason = "sd-bus's C interface")]
            // SAFETY: `self.bus` is the open connection.
            let waited = unsafe { sd_bus_wait(self.bus, usec) };
            if waited < 0 && waited != -libc::EINTR {
                return Err(errno(waited));
            }
        }
    }
}

/// Appends `StartTransientUnit`'s arguments to the call `m`: the name, the
/// mode, the properties, and an empty array of auxiliary units.
fn append(m: *mut c_void, name: &CStr, mode: &CStr, properties: &[Property]) -> Result<(), Error> {
    let check = |r: c_int| if r < 0 { Err(errno(r)) } else { Ok(()) };
    #[expect(unsafe_code, reason = "sd-bus's C interface")]
    // SAFETY: `m` is a call being built; "ss" takes two NUL-ended strings.
    check(unsafe { sd_bus_message_append(m, c"ss".as_ptr(), name.as_ptr(), mode.as_ptr()) })?;
    #[expect(unsafe_code, reason = "sd-bus's C interface")]
    // SAFETY: as above; the container is closed below.
    check(unsafe {
        sd_bus_message_open_container(m, c_char::from_ne_bytes(*b"a"), c"(sv)".as_ptr())
    })?;
    for property in properties {
        match property {
            Property::Text(key, value) => {
                let key = CString::new(*key).map_err(|_| errno(-libc::EINVAL))?;
                let value = CString::new(value.as_str()).map_err(|_| errno(-libc::EINVAL))?;
                #[expect(unsafe_code, reason = "sd-bus's C interface")]
                // SAFETY: "(sv)" takes the key, then the variant's
                // signature "s" and its string, all NUL-ended.
                check(unsafe {
                    sd_bus_message_append(
                        m,
                        c"(sv)".as_ptr(),
                        key.as_ptr(),
                        c"s".as_ptr(),
                        value.as_ptr(),
                    )
                })?;
            }
            Property::Numbers(key, numbers) => {
                let key = CString::new(*key).map_err(|_| errno(-libc::EINVAL))?;
                #[expect(unsafe_code, reason = "sd-bus's C interface")]
                // SAFETY: "(sv)" takes the key, then the variant's signature
                // "au"; inside the variant the array is opened by hand and
                // takes each number as a uint32_t.
                unsafe {
                    check(sd_bus_message_open_container(
                        m,
                        c_char::from_ne_bytes(*b"r"),
                        c"sv".as_ptr(),
                    ))?;
                    check(sd_bus_message_append(m, c"s".as_ptr(), key.as_ptr()))?;
                    check(sd_bus_message_open_container(
                        m,
                        c_char::from_ne_bytes(*b"v"),
                        c"au".as_ptr(),
                    ))?;
                    check(sd_bus_message_open_container(
                        m,
                        c_char::from_ne_bytes(*b"a"),
                        c"u".as_ptr(),
                    ))?;
                    for &n in numbers {
                        check(sd_bus_message_append(m, c"u".as_ptr(), n as c_uint))?;
                    }
                    check(sd_bus_message_close_container(m))?;
                    check(sd_bus_message_close_container(m))?;
                    check(sd_bus_message_close_container(m))?;
                }
            }
        }
    }
    #[expect(unsafe_code, reason = "sd-bus's C interface")]
    // SAFETY: closes the "a(sv)" opened above; "a(sa(sv))" with a count of
    // 0 appends an empty array, which takes no element arguments.
    unsafe {
        check(sd_bus_message_close_container(m))?;
        check(sd_bus_message_append(m, c"a(sa(sv))".as_ptr(), 0 as c_uint))
    }
}

#[cfg(test)]
mod tests {
    #![expect(clippy::panic, reason = "a test fails by panicking")]

    use super::*;

    /// systemd's manager, answering as it does: a unit whose name exists
    /// already is `UnitExists`; a call it takes queues a job, whose
    /// `JobRemoved` comes, among other clients' jobs, only to a client that
    /// listened for it before the call.
    #[derive(Default)]
    struct StandInManager {
        /// The units that exist.
        units: Vec<String>,
        /// The result the next job ends with; `None`, a job that never ends.
        result: Option<&'static str>,
        /// Whether the client listens for `JobRemoved`.
        watching: bool,
        /// The calls made, as their unit name, mode and properties.
        calls: Vec<(String, String, Vec<Property>)>,
        /// The signals for the client to read.
        signals: VecDeque<(String, String)>,
        /// The jobs queued.
        jobs: u32,
    }

    impl Bus for StandInManager {
        fn watch_jobs(&mut self) -> Result<(), Error> {
            self.watching = true;
            Ok(())
        }

        fn start_transient_unit(
            &mut self,
            name: &str,
            mode: &str,
            properties: &[Property],
        ) -> Result<String, Error> {
            self.calls
                .push((name.to_owned(), mode.to_owned(), properties.to_vec()));
            if self.units.iter().any(|u| u == name) {
                return Err(Error::Refused {
                    name: "org.freedesktop.systemd1.UnitExists".to_owned(),
                    message: format!("Unit {name} was already loaded or has a fragment file."),
                });
            }
            self.units.push(name.to_owned());
            self.jobs = self.jobs.saturating_add(1);
            let job = format!("/org/freedesktop/systemd1/job/{}", self.jobs);
            if self.watching {
                // another client's job finishes first
                self.signals.push_back((
                    "/org/freedesktop/systemd1/job/999".to_owned(),
                    "failed".to_owned(),
                ));
                if let Some(result) = self.result {
                    self.signals.push_back((job.clone(), result.to_owned()));
                }
            }
            Ok(job)
        }

        fn next_removed_job(&mut self, _: Instant) -> Result<Option<(String, String)>, Error> {
            Ok(self.signals.pop_front())
        }
    }

    fn deadline() -> Instant {
        #[expect(clippy::disallowed_methods, reason = "the stand-in never waits")]
        Instant::now()
    }

    #[test]
    fn the_scope_holds_the_child_and_is_collected_once_empty() {
        let mut systemd = StandInManager {
            result: Some("done"),
            ..StandInManager::default()
        };
        let unit = unit_name("tty2", 4242);
        assert_eq!(unit, "vtium-login-tty2-4242.scope");
        start_with(&mut systemd, &unit, 4242, deadline())
            .unwrap_or_else(|e| panic!("the scope starts: {e}"));
        assert_eq!(
            systemd.calls,
            [(
                "vtium-login-tty2-4242.scope".to_owned(),
                "fail".to_owned(),
                vec![
                    Property::Text(
                        "Description",
                        "The login vtium started as process 4242".to_owned()
                    ),
                    Property::Numbers("PIDs", vec![4242]),
                    Property::Text("CollectMode", "inactive-or-failed".to_owned()),
                ],
            )]
        );
    }

    #[test]
    fn a_job_that_fails_or_never_ends_is_an_error() {
        let mut systemd = StandInManager {
            result: Some("failed"),
            ..StandInManager::default()
        };
        assert!(matches!(
            start_with(&mut systemd, "vtium-login-tty2-1.scope", 1, deadline()),
            Err(ScopeError::Failed(r)) if r == "failed"
        ));
        let mut systemd = StandInManager::default();
        let e = start_with(&mut systemd, "vtium-login-tty2-1.scope", 1, deadline());
        assert!(matches!(e, Err(ScopeError::TimedOut)), "{e:?}");
        assert_eq!(
            e.map_err(|e| e.to_string()),
            Err("systemd did not start the scope within 2 seconds".to_owned())
        );
    }

    #[test]
    fn a_unit_that_exists_is_refused_by_systemd() {
        let mut systemd = StandInManager {
            units: vec!["vtium-login-tty2-7.scope".to_owned()],
            result: Some("done"),
            ..StandInManager::default()
        };
        let e = start_with(&mut systemd, "vtium-login-tty2-7.scope", 7, deadline());
        assert_eq!(
            e.map_err(|e| e.to_string()),
            Err("systemd answered org.freedesktop.systemd1.UnitExists: Unit vtium-login-tty2-7.scope was already loaded or has a fragment file.".to_owned())
        );
    }

    #[test]
    fn the_reasons_read_as_sentences() {
        assert_eq!(
            ScopeError::NoSystemd.to_string(),
            "systemd is not the init process"
        );
        assert_eq!(
            ScopeError::Failed("canceled".to_owned()).to_string(),
            "systemd's job for the scope ended canceled"
        );
    }
}
