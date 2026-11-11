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
    BusError, Error, classify, sd_bus_call_method, sd_bus_error_free, sd_bus_message_unref,
    sd_bus_set_method_call_timeout,
};

/// systemd's manager on the system bus.
const SYSTEMD1: &CStr = c"org.freedesktop.systemd1";
const SYSTEMD1_PATH: &CStr = c"/org/freedesktop/systemd1";
const MANAGER: &CStr = c"org.freedesktop.systemd1.Manager";

/// The directory that exists only while systemd is PID 1, which
/// sd_booted(3) checks (systemd at 78ab4b10, man/sd_booted.xml:55).
const BOOTED: &str = "/run/systemd/system";

/// The system bus's address where `DBUS_SYSTEM_BUS_ADDRESS` names none, as
/// sd-bus's `sd_bus_open_system` takes it (systemd at 78ab4b10,
/// src/libsystemd/sd-bus/bus-internal.h:17 and sd-bus.c:1321-1335).
const SYSTEM_BUS: &str = "unix:path=/run/dbus/system_bus_socket";

/// How long the whole sequence may take, from the connection to the bus to
/// the cancel of a job that did not finish.
const SCOPE_TIMEOUT: Duration = Duration::from_millis(crate::bounds::SCOPE_JOB_MS as u64);

/// The end of that time kept for the cancel: the connection, the calls and
/// the wait for the job all end this long before the deadline.
const CANCEL_RESERVE: Duration = Duration::from_millis(500);

/// One property of the transient unit, as `StartTransientUnit` takes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Property {
    /// A property whose value is a string.
    Text(&'static str, String),
    /// A property whose value is an array of 32-bit numbers, `PIDs`.
    Numbers(&'static str, Vec<u32>),
}

/// The calls to systemd's manager that [`start_with`] makes, so the tests
/// can stand in for the bus. Each one that waits ends by `until`, with
/// [`ScopeError::NoAnswer`] when nothing came in time.
pub(crate) trait Bus {
    /// Starts listening for the manager's `JobRemoved` signal. It comes
    /// before the call, so that a job that finishes at once is not missed.
    fn watch_jobs(&mut self, until: Instant) -> Result<(), ScopeError>;
    /// `StartTransientUnit(name, mode, properties, no auxiliary units)`,
    /// which answers the path of the job it queued.
    fn start_transient_unit(
        &mut self,
        name: &str,
        mode: &str,
        properties: &[Property],
        until: Instant,
    ) -> Result<String, ScopeError>;
    /// The next job the manager reported finished, as its path and its
    /// result, or `None` once `until` has passed, even with signals queued.
    fn next_removed_job(&mut self, until: Instant) -> Result<Option<(String, String)>, ScopeError>;
    /// `Cancel` on the job at `job`, which systemd refuses for a job that
    /// has already finished.
    fn cancel_job(&mut self, job: &str, until: Instant) -> Result<(), ScopeError>;
    /// Whether the unit `unit` holds the process `pid`, by the process's
    /// control group.
    fn holds(&mut self, unit: &str, pid: u32) -> bool;
}

/// Why a login child got no scope of its own.
#[derive(Debug)]
pub enum ScopeError {
    /// systemd is not PID 1, so there is no manager to ask.
    NoSystemd,
    /// The system bus cannot be reached, or the manager refused.
    Bus(Error),
    /// The system bus or the manager did not answer in time.
    NoAnswer,
    /// The scope's job did not finish within `SCOPE_JOB_MS` (the crate's
    /// bounds). vtium then cancelled it, or, when systemd refused the
    /// cancel, here is why.
    TimedOut { not_cancelled: Option<String> },
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
            ScopeError::NoAnswer => write!(f, "the system bus did not answer in time"),
            ScopeError::TimedOut {
                not_cancelled: None,
            } => write!(
                f,
                "systemd did not start the scope within {} seconds, so vtium cancelled its job",
                SCOPE_TIMEOUT.as_secs()
            ),
            ScopeError::TimedOut {
                not_cancelled: Some(why),
            } => write!(
                f,
                "systemd did not start the scope within {} seconds, and vtium could not cancel its job ({why}), so the session may still move into the scope later",
                SCOPE_TIMEOUT.as_secs()
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
/// systemd has done it, or at most `SCOPE_JOB_MS` (the crate's bounds) after
/// it was called. Where systemd is not PID 1 it asks nothing.
pub fn start(unit: &str, pid: u32) -> Result<(), ScopeError> {
    if !Path::new(BOOTED).is_dir() {
        return Err(ScopeError::NoSystemd);
    }
    #[expect(
        clippy::disallowed_methods,
        reason = "the scope's sequence is bounded in time"
    )]
    let now = Instant::now();
    let address = std::env::var_os("DBUS_SYSTEM_BUS_ADDRESS")
        .filter(|a| !a.is_empty())
        .map_or_else(|| SYSTEM_BUS.into(), |a| a.as_encoded_bytes().to_vec());
    let address = CString::new(address).map_err(|_| ScopeError::Bus(errno(-libc::EINVAL)))?;
    start_at(
        &address,
        unit,
        pid,
        now.checked_add(SCOPE_TIMEOUT).unwrap_or(now),
    )
}

/// [`start`] on the bus at `address`, done by `deadline`.
pub(crate) fn start_at(
    address: &CStr,
    unit: &str,
    pid: u32,
    deadline: Instant,
) -> Result<(), ScopeError> {
    let mut bus = SystemBus::open(address, waits_until(deadline))?;
    start_with(&mut bus, unit, pid, deadline)
}

/// When the connection, the calls and the wait for the job end, so that
/// the cancel still has [`CANCEL_RESERVE`] before `deadline`.
fn waits_until(deadline: Instant) -> Instant {
    deadline.checked_sub(CANCEL_RESERVE).unwrap_or(deadline)
}

/// [`start`] on any bus, done by `deadline`.
pub(crate) fn start_with(
    bus: &mut impl Bus,
    unit: &str,
    pid: u32,
    deadline: Instant,
) -> Result<(), ScopeError> {
    let waits = waits_until(deadline);
    bus.watch_jobs(waits)?;
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
    let job = match bus.start_transient_unit(unit, "fail", &properties, waits) {
        Ok(job) => job,
        // the request went out, and systemd may yet queue its job
        Err(ScopeError::NoAnswer) => return too_late(bus, unit, pid, None, deadline),
        Err(e) => return Err(e),
    };
    loop {
        match bus.next_removed_job(waits)? {
            None => return too_late(bus, unit, pid, Some(&job), deadline),
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

/// The end of a job that did not finish in time: vtium cancels it before it
/// lets the child go, so that no late job moves `login` out of vtium's
/// session after logind has refused it a session of its own. When systemd
/// refuses the cancel because the job finished meanwhile, the child's
/// control group tells whether the scope holds it. With no job, because the
/// request for it got no answer, there is nothing to cancel.
fn too_late(
    bus: &mut impl Bus,
    unit: &str,
    pid: u32,
    job: Option<&str>,
    deadline: Instant,
) -> Result<(), ScopeError> {
    let cancelled = match job {
        Some(job) => bus.cancel_job(job, deadline).map_err(|e| e.to_string()),
        None => Err("systemd did not answer the request for it in time".to_owned()),
    };
    if bus.holds(unit, pid) {
        return Ok(());
    }
    Err(ScopeError::TimedOut {
        not_cancelled: cancelled.err(),
    })
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
    fn sd_bus_new(bus: *mut *mut c_void) -> c_int;
    fn sd_bus_set_address(bus: *mut c_void, address: *const c_char) -> c_int;
    fn sd_bus_set_bus_client(bus: *mut c_void, b: c_int) -> c_int;
    fn sd_bus_start(bus: *mut c_void) -> c_int;
    fn sd_bus_is_ready(bus: *mut c_void) -> c_int;
    fn sd_bus_close(bus: *mut c_void);
    fn sd_bus_unref(bus: *mut c_void) -> *mut c_void;
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
    /// Connects to the bus at `address` by `until`. The connection is made
    /// as sd-bus's `sd_bus_open_system` makes it, from `sd_bus_new`,
    /// `sd_bus_set_address`, `sd_bus_set_bus_client` and `sd_bus_start`
    /// (sd_bus_new(3); systemd at 78ab4b10, sd-bus.c:1337-1371), except that
    /// the call timeout is set before the start, so that it bounds the bus's
    /// `Hello`, and that vtium waits for the connection itself, until it is
    /// ready (sd_bus_is_ready(3)) or `until` has passed. sd-bus's own wait
    /// for authentication is 90 s, which no call sets (bus-socket.c:793).
    fn open(address: &CStr, until: Instant) -> Result<SystemBus, ScopeError> {
        let mut raw: *mut c_void = ptr::null_mut();
        #[expect(unsafe_code, reason = "sd-bus's C interface")]
        // SAFETY: `raw` is a valid out pointer; sd-bus sets it only on
        // success.
        let made = unsafe { sd_bus_new(&mut raw) };
        if made < 0 {
            return Err(ScopeError::Bus(errno(made)));
        }
        let mut bus = SystemBus {
            bus: raw,
            slot: ptr::null_mut(),
            removed: Box::default(),
        };
        let check = |r: c_int| {
            if r < 0 {
                Err(ScopeError::Bus(errno(r)))
            } else {
                Ok(())
            }
        };
        #[expect(unsafe_code, reason = "sd-bus's C interface")]
        // SAFETY: `bus.bus` is the bus just made, not yet started; the
        // address is NUL-ended.
        unsafe {
            check(sd_bus_set_address(bus.bus, address.as_ptr()))?;
            check(sd_bus_set_bus_client(bus.bus, 1))?;
        }
        bus.bound(until)?;
        #[expect(unsafe_code, reason = "sd-bus's C interface")]
        // SAFETY: `bus.bus` is the bus just configured.
        check(unsafe { sd_bus_start(bus.bus) })?;
        loop {
            #[expect(unsafe_code, reason = "sd-bus's C interface")]
            // SAFETY: `bus.bus` is the bus being started.
            let ready = unsafe { sd_bus_is_ready(bus.bus) };
            check(ready)?;
            if ready > 0 {
                return Ok(bus);
            }
            #[expect(unsafe_code, reason = "sd-bus's C interface")]
            // SAFETY: as above; a null message pointer asks for none back.
            let processed = unsafe { sd_bus_process(bus.bus, ptr::null_mut()) };
            check(processed)?;
            if processed == 0 {
                bus.wait(until)?;
            }
        }
    }

    /// Sets the bus's call timeout to what is left before `until`, which
    /// bounds the calls that take the bus's own timeout.
    fn bound(&mut self, until: Instant) -> Result<(), ScopeError> {
        let usec = left(until)?;
        #[expect(unsafe_code, reason = "sd-bus's C interface")]
        // SAFETY: `self.bus` is the bus this owns.
        let timed = unsafe { sd_bus_set_method_call_timeout(self.bus, usec) };
        if timed < 0 {
            return Err(ScopeError::Bus(errno(timed)));
        }
        Ok(())
    }

    /// Waits for the bus to have something to process, at most until
    /// `until`, which is [`ScopeError::NoAnswer`] once it has passed.
    fn wait(&mut self, until: Instant) -> Result<(), ScopeError> {
        let usec = left(until)?;
        #[expect(unsafe_code, reason = "sd-bus's C interface")]
        // SAFETY: `self.bus` is the bus this owns.
        let waited = unsafe { sd_bus_wait(self.bus, usec) };
        if waited < 0 && waited != -libc::EINTR {
            return Err(ScopeError::Bus(errno(waited)));
        }
        Ok(())
    }
}

/// The microseconds left before `until`, or [`ScopeError::NoAnswer`] when
/// none are.
fn left(until: Instant) -> Result<u64, ScopeError> {
    #[expect(
        clippy::disallowed_methods,
        reason = "the scope's sequence is bounded in time"
    )]
    let now = Instant::now();
    let left = until
        .checked_duration_since(now)
        .filter(|d| !d.is_zero())
        .ok_or(ScopeError::NoAnswer)?;
    Ok(u64::try_from(left.as_micros()).unwrap_or(u64::MAX).max(1))
}

/// A call sd-bus failed with `code` and `error`: no answer in time, or what
/// the bus or systemd answered.
fn failed(code: c_int, error: &BusError) -> ScopeError {
    if code == -libc::ETIMEDOUT {
        return ScopeError::NoAnswer;
    }
    ScopeError::Bus(classify(text(error.name), text(error.message), code))
}

impl Drop for SystemBus {
    fn drop(&mut self) {
        #[expect(unsafe_code, reason = "sd-bus's C interface")]
        // SAFETY: each is what sd-bus gave, freed once: the slot first,
        // which stops the handler before its queue goes (null-tolerant),
        // then the connection. It is closed without a flush, which would
        // wait without a bound for a connection still starting
        // (sd-bus.c:3449-3468); every call here waited for its answer, so
        // nothing is left to send.
        unsafe {
            sd_bus_slot_unref(self.slot);
            sd_bus_close(self.bus);
            sd_bus_unref(self.bus);
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
    fn watch_jobs(&mut self, until: Instant) -> Result<(), ScopeError> {
        // the match is added with a call that takes the bus's timeout
        self.bound(until)?;
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
        if r == -libc::ETIMEDOUT {
            Err(ScopeError::NoAnswer)
        } else if r < 0 {
            Err(ScopeError::Bus(errno(r)))
        } else {
            Ok(())
        }
    }

    fn start_transient_unit(
        &mut self,
        name: &str,
        mode: &str,
        properties: &[Property],
        until: Instant,
    ) -> Result<String, ScopeError> {
        let usec = left(until)?;
        let c = |s: &str| CString::new(s).map_err(|_| ScopeError::Bus(errno(-libc::EINVAL)));
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
            return Err(ScopeError::Bus(errno(r)));
        }
        let built = append(m, &name, &mode, properties).map_err(ScopeError::Bus);
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
            // `usec` is what is left of the sequence's time.
            let called = unsafe { sd_bus_call(self.bus, m, usec, &mut error, &mut reply) };
            if called < 0 {
                return Err(failed(called, &error));
            }
            let mut job: *const c_char = ptr::null();
            #[expect(unsafe_code, reason = "sd-bus's C interface")]
            // SAFETY: `reply` is the answer just received; "o" reads one
            // object path into the pointer, which lives as long as the
            // reply.
            let read = unsafe { sd_bus_message_read(reply, c"o".as_ptr(), &mut job) };
            if read < 0 {
                return Err(ScopeError::Bus(errno(read)));
            }
            text(job).ok_or_else(|| ScopeError::Bus(errno(-libc::EBADMSG)))
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

    fn next_removed_job(&mut self, until: Instant) -> Result<Option<(String, String)>, ScopeError> {
        loop {
            // the time first: a job that finished past it counts as late
            if left(until).is_err() {
                return Ok(None);
            }
            if let Some(job) = self.removed.pop_front() {
                return Ok(Some(job));
            }
            #[expect(unsafe_code, reason = "sd-bus's C interface")]
            // SAFETY: `self.bus` is the open connection; a null message
            // pointer asks for none back.
            let processed = unsafe { sd_bus_process(self.bus, ptr::null_mut()) };
            if processed < 0 {
                return Err(ScopeError::Bus(errno(processed)));
            }
            if processed == 0 {
                match self.wait(until) {
                    Err(ScopeError::NoAnswer) => return Ok(None),
                    waited => waited?,
                }
            }
        }
    }

    fn cancel_job(&mut self, job: &str, until: Instant) -> Result<(), ScopeError> {
        let job = CString::new(job).map_err(|_| ScopeError::Bus(errno(-libc::EINVAL)))?;
        // the call takes the bus's timeout
        self.bound(until)?;
        let mut error = BusError {
            name: ptr::null(),
            message: ptr::null(),
            need_free: 0,
        };
        let mut reply: *mut c_void = ptr::null_mut();
        #[expect(unsafe_code, reason = "sd-bus's C interface")]
        // SAFETY: `self.bus` is the open connection; every string is
        // NUL-ended; `error` starts as SD_BUS_ERROR_NULL and `reply` as
        // null; the empty signature takes no arguments.
        let called = unsafe {
            sd_bus_call_method(
                self.bus,
                SYSTEMD1.as_ptr(),
                job.as_ptr(),
                c"org.freedesktop.systemd1.Job".as_ptr(),
                c"Cancel".as_ptr(),
                &mut error,
                &mut reply,
                c"".as_ptr(),
            )
        };
        let result = if called < 0 {
            Err(failed(called, &error))
        } else {
            Ok(())
        };
        #[expect(unsafe_code, reason = "sd-bus's C interface")]
        // SAFETY: each is what sd-bus gave above, freed once.
        unsafe {
            sd_bus_error_free(&mut error);
            sd_bus_message_unref(reply);
        }
        result
    }

    /// The child's control group, `0::PATH` on the unified hierarchy, ends
    /// in the unit's name while the unit holds it.
    fn holds(&mut self, unit: &str, pid: u32) -> bool {
        let tail = format!("/{unit}");
        std::fs::read_to_string(format!("/proc/{pid}/cgroup"))
            .is_ok_and(|groups| groups.lines().any(|l| l.ends_with(&tail)))
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
        /// The jobs the client cancelled, in order.
        cancelled: Vec<String>,
        /// The job finishes, and its scope takes the child, just as the
        /// client asks to cancel it, which systemd then refuses.
        finishes_at_cancel: bool,
        /// The unit that holds the child, once its job finished.
        holder: Option<String>,
        /// The last job queued and its unit.
        queued: Option<(String, String)>,
        /// Nothing answers: the bus or the manager is wedged.
        silent: bool,
    }

    impl Bus for StandInManager {
        fn watch_jobs(&mut self, _: Instant) -> Result<(), ScopeError> {
            if self.silent {
                return Err(ScopeError::NoAnswer);
            }
            self.watching = true;
            Ok(())
        }

        fn start_transient_unit(
            &mut self,
            name: &str,
            mode: &str,
            properties: &[Property],
            _: Instant,
        ) -> Result<String, ScopeError> {
            self.calls
                .push((name.to_owned(), mode.to_owned(), properties.to_vec()));
            if self.units.iter().any(|u| u == name) {
                return Err(ScopeError::Bus(Error::Refused {
                    name: "org.freedesktop.systemd1.UnitExists".to_owned(),
                    message: format!("Unit {name} was already loaded or has a fragment file."),
                }));
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
                    if result == "done" {
                        self.holder = Some(name.to_owned());
                    }
                }
            }
            self.queued = Some((job.clone(), name.to_owned()));
            Ok(job)
        }

        fn next_removed_job(&mut self, _: Instant) -> Result<Option<(String, String)>, ScopeError> {
            Ok(self.signals.pop_front())
        }

        /// A job still queued is cancelled; any other, systemd refuses.
        fn cancel_job(&mut self, job: &str, _: Instant) -> Result<(), ScopeError> {
            self.cancelled.push(job.to_owned());
            let queued = self.queued.take();
            match queued {
                Some((j, _)) if j == job && !self.finishes_at_cancel => Ok(()),
                Some((j, unit)) if j == job => {
                    self.holder = Some(unit);
                    Err(refused_no_such_job(job))
                }
                _ => Err(refused_no_such_job(job)),
            }
        }

        fn holds(&mut self, unit: &str, _: u32) -> bool {
            self.holder.as_deref() == Some(unit)
        }
    }

    fn refused_no_such_job(job: &str) -> ScopeError {
        ScopeError::Bus(Error::Refused {
            name: "org.freedesktop.DBus.Error.UnknownObject".to_owned(),
            message: format!("Unknown object '{job}'."),
        })
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
    }

    // A job that never ends is cancelled before start_with returns, and so
    // before vtium lets the child go: no late job can move `login` out of
    // vtium's session once logind has refused it a session of its own.
    #[test]
    fn a_job_that_never_ends_is_cancelled_before_the_child_goes() {
        let mut systemd = StandInManager::default();
        let e = start_with(&mut systemd, "vtium-login-tty2-1.scope", 1, deadline());
        assert_eq!(systemd.cancelled, ["/org/freedesktop/systemd1/job/1"]);
        assert!(!systemd.holds("vtium-login-tty2-1.scope", 1));
        assert_eq!(
            e.map_err(|e| e.to_string()),
            Err(
                "systemd did not start the scope within 2 seconds, so vtium cancelled its job"
                    .to_owned()
            )
        );
    }

    // A job that finishes as vtium cancels it: systemd refuses the cancel,
    // and the scope holds the child, so the child has its scope after all.
    #[test]
    fn a_job_that_finishes_as_it_is_cancelled_counts_as_done() {
        let mut systemd = StandInManager {
            finishes_at_cancel: true,
            ..StandInManager::default()
        };
        let e = start_with(&mut systemd, "vtium-login-tty2-1.scope", 1, deadline());
        assert!(e.is_ok(), "{e:?}");
        assert_eq!(systemd.cancelled, ["/org/freedesktop/systemd1/job/1"]);
    }

    // A bus that never answers asks for no scope and cancels nothing: no
    // request went out.
    #[test]
    fn a_silent_bus_ends_before_any_request() {
        let mut systemd = StandInManager {
            silent: true,
            ..StandInManager::default()
        };
        let e = start_with(&mut systemd, "vtium-login-tty2-1.scope", 1, deadline());
        assert!(matches!(e, Err(ScopeError::NoAnswer)), "{e:?}");
        assert!(systemd.calls.is_empty() && systemd.cancelled.is_empty());
    }

    // The real connection against a socket that accepts and never answers,
    // as a wedged bus daemon does: the whole sequence ends by its deadline,
    // here 1 s away, of which the connection may take all but the 500 ms
    // kept for a cancel. sd-bus's own wait for the bus's authentication is
    // 90 s, so a wait that ignored the deadline would take that long. The
    // test waits for the answer on another thread, at most 20 s, only so
    // that it fails instead of hanging.
    #[test]
    fn a_bus_that_never_answers_ends_by_the_deadline() {
        let dir = std::env::temp_dir().join(format!(
            "vtium-scope-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
        let path = dir.join("bus");
        let listener =
            std::os::unix::net::UnixListener::bind(&path).unwrap_or_else(|e| panic!("{e}"));
        let address =
            CString::new(format!("unix:path={}", path.display())).unwrap_or_else(|e| panic!("{e}"));
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            #[expect(clippy::disallowed_methods, reason = "the sequence's deadline")]
            let now = Instant::now();
            let deadline = now + Duration::from_millis(1000);
            let got = start_at(&address, "vtium-login-tty2-1.scope", 1, deadline);
            let _ = tx.send(got.map_err(|e| e.to_string()));
        });
        let got = rx.recv_timeout(Duration::from_secs(20));
        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            got,
            Ok(Err("the system bus did not answer in time".to_owned()))
        );
    }

    #[test]
    fn a_cancel_systemd_refuses_says_the_session_may_still_move() {
        let e = ScopeError::TimedOut {
            not_cancelled: Some(
                "systemd answered org.freedesktop.DBus.Error.AccessDenied: no".to_owned(),
            ),
        };
        assert_eq!(
            e.to_string(),
            "systemd did not start the scope within 2 seconds, and vtium could not cancel its job (systemd answered org.freedesktop.DBus.Error.AccessDenied: no), so the session may still move into the scope later"
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
