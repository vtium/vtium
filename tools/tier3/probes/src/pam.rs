//! A session opened through PAM, as a display manager opens its greeter's
//! session and as the broker would open the drawer's (ADR-1400 D4, open
//! question 2): the root process opens the session, so pam_systemd
//! registers it with logind with the root process as its leader, and the
//! process it then starts as the account runs inside it.

use std::ffi::{c_char, c_int, c_void};

use crate::sys::{Library, cstring, text};

const PAM_TTY: c_int = 3;
const PAM_ESTABLISH_CRED: c_int = 0x2;
const PAM_DELETE_CRED: c_int = 0x4;
const PAM_CONV_ERR: c_int = 19;

#[repr(C)]
struct PamConv {
    conv: unsafe extern "C" fn(c_int, *mut *const c_void, *mut *mut c_void, *mut c_void) -> c_int,
    appdata: *mut c_void,
}

/// No module of the stacks the probes open asks a question, so any
/// question fails.
unsafe extern "C" fn no_conversation(
    _: c_int,
    _: *mut *const c_void,
    _: *mut *mut c_void,
    _: *mut c_void,
) -> c_int {
    PAM_CONV_ERR
}

type Start =
    unsafe extern "C" fn(*const c_char, *const c_char, *const PamConv, *mut *mut c_void) -> c_int;
type SetItem = unsafe extern "C" fn(*mut c_void, c_int, *const c_void) -> c_int;
type PutEnv = unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int;
type GetEnvList = unsafe extern "C" fn(*mut c_void) -> *mut *mut c_char;
type Flags = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
type End = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
type StrError = unsafe extern "C" fn(*mut c_void, c_int) -> *const c_char;

pub struct Session {
    _lib: Library,
    handle: *mut c_void,
    setcred: Flags,
    close_session: Flags,
    end: End,
    strerror: StrError,
    getenvlist: GetEnvList,
    _conv: Box<PamConv>,
}

/// The steps of opening a session, each with its answer.
pub struct Opened {
    pub session: Option<Session>,
    pub steps: Vec<(String, String)>,
}

/// Opens a session of `user` through the PAM service `service` on VT `vt`,
/// with each of `env` (NAME=VALUE) put into PAM's environment first, as a
/// display manager puts XDG_SESSION_CLASS and XDG_SESSION_TYPE there.
pub fn open(service: &str, user: &str, vt: u32, env: &[String]) -> Opened {
    let mut steps = Vec::new();
    let lib = match Library::open("libpam.so.0") {
        Ok(l) => l,
        Err(e) => {
            steps.push(("libpam".into(), e));
            return Opened {
                session: None,
                steps,
            };
        }
    };
    // SAFETY: each type matches the function's declaration in
    // security/pam_appl.h.
    let syms = unsafe {
        (|| -> Result<_, String> {
            Ok((
                lib.get::<Start>("pam_start")?,
                lib.get::<SetItem>("pam_set_item")?,
                lib.get::<PutEnv>("pam_putenv")?,
                lib.get::<GetEnvList>("pam_getenvlist")?,
                lib.get::<Flags>("pam_acct_mgmt")?,
                lib.get::<Flags>("pam_setcred")?,
                lib.get::<Flags>("pam_open_session")?,
                lib.get::<Flags>("pam_close_session")?,
                lib.get::<End>("pam_end")?,
                lib.get::<StrError>("pam_strerror")?,
            ))
        })()
    };
    let (
        start,
        set_item,
        putenv,
        getenvlist,
        acct_mgmt,
        setcred,
        open_session,
        close_session,
        end,
        strerror,
    ) = match syms {
        Ok(s) => s,
        Err(e) => {
            steps.push(("libpam".into(), e));
            return Opened {
                session: None,
                steps,
            };
        }
    };
    let conv = Box::new(PamConv {
        conv: no_conversation,
        appdata: std::ptr::null_mut(),
    });
    let mut handle: *mut c_void = std::ptr::null_mut();
    let cservice = cstring(service);
    let cuser = cstring(user);
    // SAFETY: valid strings, a conversation that outlives the handle, and a
    // handle pointer to fill.
    let rc = unsafe { start(cservice.as_ptr(), cuser.as_ptr(), &*conv, &mut handle) };
    let say = |rc: c_int| {
        if rc == 0 {
            "ok".to_string()
        } else {
            // SAFETY: pam_strerror returns a static string.
            format!("error {rc}: {}", text(unsafe { strerror(handle, rc) }))
        }
    };
    steps.push(("pam_start".into(), say(rc)));
    if rc != 0 {
        return Opened {
            session: None,
            steps,
        };
    }
    let tty = cstring(&format!("tty{vt}"));
    // SAFETY: the handle is open and the item is a C string.
    steps.push((
        "PAM_TTY".into(),
        say(unsafe { set_item(handle, PAM_TTY, tty.as_ptr().cast()) }),
    ));
    for e in env {
        let ce = cstring(e);
        // SAFETY: the handle is open and the variable is a C string.
        steps.push((
            format!("pam_putenv {e}"),
            say(unsafe { putenv(handle, ce.as_ptr()) }),
        ));
    }
    // SAFETY: the handle is open.
    steps.push(("pam_acct_mgmt".into(), say(unsafe { acct_mgmt(handle, 0) })));
    // SAFETY: the handle is open.
    steps.push((
        "pam_setcred".into(),
        say(unsafe { setcred(handle, PAM_ESTABLISH_CRED) }),
    ));
    // SAFETY: the handle is open.
    let rc = unsafe { open_session(handle, 0) };
    steps.push(("pam_open_session".into(), say(rc)));
    let session = Session {
        _lib: lib,
        handle,
        setcred,
        close_session,
        end,
        strerror,
        getenvlist,
        _conv: conv,
    };
    if rc != 0 {
        session.end(rc);
        return Opened {
            session: None,
            steps,
        };
    }
    Opened {
        session: Some(session),
        steps,
    }
}

impl Session {
    /// PAM's environment after the session opened, which carries
    /// XDG_SESSION_ID and XDG_RUNTIME_DIR from pam_systemd.
    pub fn env(&self) -> Vec<String> {
        let mut out = Vec::new();
        // SAFETY: pam_getenvlist returns a malloc'd, null-terminated array
        // of malloc'd strings, which the caller frees.
        unsafe {
            let list = (self.getenvlist)(self.handle);
            if list.is_null() {
                return out;
            }
            let mut p = list;
            while !(*p).is_null() {
                out.push(text(*p));
                libc::free((*p).cast());
                p = p.add(1);
            }
            libc::free(list.cast());
        }
        out
    }

    /// Closes the session and ends PAM; the answer of pam_close_session.
    pub fn close(self) -> String {
        // SAFETY: the handle is open.
        let rc = unsafe { (self.close_session)(self.handle, 0) };
        let said = if rc == 0 {
            "ok".to_string()
        } else {
            // SAFETY: pam_strerror returns a static string.
            format!(
                "error {rc}: {}",
                text(unsafe { (self.strerror)(self.handle, rc) })
            )
        };
        // SAFETY: the handle is open.
        unsafe { (self.setcred)(self.handle, PAM_DELETE_CRED) };
        self.end(rc);
        said
    }

    fn end(&self, rc: c_int) {
        // SAFETY: ends the handle once; nothing uses it after.
        unsafe { (self.end)(self.handle, rc) };
    }
}
