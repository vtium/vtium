//! The C side: the C reference's parser, which `build.rs` extracts from the
//! pin and compiles with `harness/shim.c`, driven through FFI.
//!
//! The shim sends each dispatch to a callback here, which writes its trace
//! line, and each function a departure acts on to a hook here, which applies
//! the departures the run expects and calls the C reference's own function
//! back. With no departure applied, the trace is the C reference's.
//!
//! The trace is written where vtium's parser hands work to its consumer:
//! each dispatch as it is entered, and every DCS string but XTGETTCAP as
//! hooked, since vtium-parse leaves the choice of DCS handler to its
//! consumer where the C reference made it in `do_dcs_start`.

use std::cell::RefCell;
use std::mem::{offset_of, size_of};

use vtium_parse::InputMode;

use crate::departures::{Departs, Departure};
use crate::script::{Line, Side, Trace, out};

// The C reference's constants (tsm-vte-int.h, libtsm.h, tsm-vte.c), checked
// against the shim's `oracle_constants` by `check`.
const CSI_ARG_MAX: usize = 16;
const CSI_SUBARG_MAX: usize = 8;
const OSC_MAX_LEN: usize = 128;
const XTGETTCAP_MAX_LEN: usize = 1039;
const FLAG_8BIT_MODE: u32 = 0x08;
const FLAG_7BIT_MODE: u32 = 0x10;
const CSI_OTHER: u32 = 0x1000;
const DCS_IGNORED: u32 = 0;
const DCS_XTGETTCAP: u32 = 2;
const ACTION_NONE: i32 = 0;
const ACTION_OSC_START: i32 = 12;
const ACTION_DCS_END: i32 = 11;
const ACTION_OSC_END: i32 = 14;
const ACTION_APC_END: i32 = 17;
const STATE_GROUND: u32 = 1;
const STATE_ESC: u32 = 2;
const STATE_ESC_INT: u32 = 3;
const STATE_CSI_ENTRY: u32 = 4;
const STATE_CSI_PARAM: u32 = 5;
const STATE_CSI_INT: u32 = 6;
const STATE_CSI_IGNORE: u32 = 7;
const STATE_DCS_ENTRY: u32 = 8;
const STATE_DCS_PARAM: u32 = 9;
const STATE_DCS_INT: u32 = 10;
const STATE_DCS_IGNORE: u32 = 12;

const CONSTANTS: [u32; 25] = [
    CSI_ARG_MAX as u32,
    CSI_SUBARG_MAX as u32,
    OSC_MAX_LEN as u32,
    XTGETTCAP_MAX_LEN as u32,
    FLAG_8BIT_MODE,
    FLAG_7BIT_MODE,
    CSI_OTHER,
    DCS_IGNORED,
    DCS_XTGETTCAP,
    ACTION_NONE as u32,
    ACTION_OSC_START as u32,
    ACTION_DCS_END as u32,
    ACTION_OSC_END as u32,
    ACTION_APC_END as u32,
    STATE_GROUND,
    STATE_ESC,
    STATE_ESC_INT,
    STATE_CSI_ENTRY,
    STATE_CSI_PARAM,
    STATE_CSI_INT,
    STATE_CSI_IGNORE,
    STATE_DCS_ENTRY,
    STATE_DCS_PARAM,
    STATE_DCS_INT,
    STATE_DCS_IGNORE,
];

/// `struct tsm_vte` as `harness/shim.c` declares it.
#[repr(C)]
struct Vte {
    mach: *mut core::ffi::c_void,
    state: u32,
    dcs: u32,
    dcs_esc: bool,
    csi_argc: u32,
    csi_argv: [i32; CSI_ARG_MAX],
    csi_subargc: [u32; CSI_ARG_MAX],
    csi_subargv: [[i32; CSI_SUBARG_MAX]; CSI_ARG_MAX],
    csi_subarg_drop: bool,
    csi_flags: u32,
    osc_len: u32,
    osc_arg: [u8; OSC_MAX_LEN],
    tcap_buf: [u8; XTGETTCAP_MAX_LEN],
    tcap_len: usize,
    tcap_over: bool,
    flags: u32,
    sync_frames: u64,
    sync_stop: bool,
    work_left: u64,
}

const LAYOUT: [usize; 20] = [
    size_of::<Vte>(),
    offset_of!(Vte, mach),
    offset_of!(Vte, state),
    offset_of!(Vte, dcs),
    offset_of!(Vte, dcs_esc),
    offset_of!(Vte, csi_argc),
    offset_of!(Vte, csi_argv),
    offset_of!(Vte, csi_subargc),
    offset_of!(Vte, csi_subargv),
    offset_of!(Vte, csi_subarg_drop),
    offset_of!(Vte, csi_flags),
    offset_of!(Vte, osc_len),
    offset_of!(Vte, osc_arg),
    offset_of!(Vte, tcap_buf),
    offset_of!(Vte, tcap_len),
    offset_of!(Vte, tcap_over),
    offset_of!(Vte, flags),
    offset_of!(Vte, sync_frames),
    offset_of!(Vte, sync_stop),
    offset_of!(Vte, work_left),
];

unsafe extern "C" {
    static oracle_constants: [u32; 25];
    static oracle_layout: [usize; 20];
    fn oracle_new() -> *mut Vte;
    fn oracle_free(vte: *mut Vte);
    fn oracle_input(vte: *mut Vte, u8: *const u8, len: usize) -> usize;
    fn oracle_reset(vte: *mut Vte);
    fn oracle_ref_clear(vte: *mut Vte);
    fn oracle_ref_collect(vte: *mut Vte, data: u32);
    fn oracle_ref_param(vte: *mut Vte, data: u32);
    fn oracle_ref_osc_collect(vte: *mut Vte, c: u32);
    fn oracle_ref_dcs_start(vte: *mut Vte, data: u32);
    fn oracle_ref_dcs_collect(vte: *mut Vte, data: u32);
    fn oracle_ref_dcs_end(vte: *mut Vte, data: u32);
    fn oracle_ref_action(vte: *mut Vte, data: u32, action: i32);
    fn oracle_ref_parse(vte: *mut Vte, raw: u32);
    fn oracle_trans(vte: *mut Vte, data: u32, state: i32, action: i32);
    fn tsm_ucs4_to_utf8(g: u32, txt: *mut u8) -> usize;
}

/// Refuses a shim whose constants or `struct tsm_vte` differ from the
/// mirror here.
pub fn check() -> Result<(), String> {
    // SAFETY: plain data the shim defines with these lengths.
    let (constants, layout) = unsafe { (oracle_constants, oracle_layout) };
    if constants != CONSTANTS {
        return Err(format!(
            "the C reference's constants moved: {constants:?}, the C side expects {CONSTANTS:?}"
        ));
    }
    if layout != LAYOUT {
        return Err(format!(
            "struct tsm_vte's layout differs: {layout:?} in C, {LAYOUT:?} in Rust"
        ));
    }
    Ok(())
}

/// A deferred string end (string-end): the exit action an ESC held back,
/// with the OSC string the ESC's own clear then wiped.
struct Pending {
    action: i32,
    osc: [u8; OSC_MAX_LEN],
    osc_len: u32,
    osc_truncated: bool,
}

/// What the C side keeps beside the C reference's state.
struct State {
    departs: Departs,
    trace: Trace,
    /// The sequence's private marker and intermediates, as collected.
    marker: Option<u32>,
    intermediates: Vec<u32>,
    /// A character failed to fit the OSC string.
    osc_truncated: bool,
    /// The DCS string reaches the trace.
    hooked: bool,
    pending: Option<Pending>,
    /// reset-apc-st: the next character is dropped if it is `\`.
    drop_backslash: bool,
}

impl State {
    /// intermediates: a third intermediate makes the sequence do nothing.
    fn null_operation(&self) -> bool {
        self.departs.has(Departure::Intermediates) && self.intermediates.len() > 2
    }

    /// The marker and intermediates: the bytes with intermediates applied,
    /// the C reference's flag set otherwise, which was all it kept.
    fn write_intermediates(&mut self, vte: &Vte) {
        if self.departs.has(Departure::Intermediates) {
            self.trace.intermediates(self.marker, &self.intermediates);
        } else if vte.csi_flags == 0 {
            self.trace.out.push_str(" -");
        } else {
            out!(self.trace.out, " f{:x}", vte.csi_flags);
        }
    }

    /// The parameters, as many as `do_csi` counts (tsm-vte.c:2542-2543).
    fn write_params(&mut self, vte: &Vte) {
        let argc = (vte.csi_argc as usize + 1).min(CSI_ARG_MAX);
        self.trace.params((0..argc).map(|i| {
            let subs = &vte.csi_subargv[i][..vte.csi_subargc[i] as usize];
            (
                i64::from(vte.csi_argv[i]),
                subs.iter().map(|&s| i64::from(s)),
            )
        }));
    }

    /// Ends a line, then gives the C reference the mode a trigger set.
    fn end(&mut self, vte: *mut Vte, line: Line) {
        self.trace.end(line);
        self.sync_mode(vte);
    }

    /// Gives the C reference the trace's mode as its 7-bit and 8-bit flags.
    fn sync_mode(&self, vte: *mut Vte) {
        let flags = match self.trace.mode() {
            InputMode::Utf8 => 0,
            InputMode::EightBit => FLAG_8BIT_MODE,
            InputMode::SevenBit => FLAG_7BIT_MODE,
        };
        // SAFETY: the vte this side owns; C reads its flags only between
        // bytes, and no reference to it is live.
        unsafe { (*vte).flags = (*vte).flags & !(FLAG_7BIT_MODE | FLAG_8BIT_MODE) | flags };
    }
}

thread_local! {
    /// The state of the C side running on this thread, there only while
    /// the C reference runs, so callbacks and hooks can reach it.
    static RUNNING: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Runs `f` on the running state. The borrow ends before any call into C,
/// which may call back.
fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    RUNNING.with(|r| f(r.borrow_mut().as_mut().expect("the C side is running")))
}

/// Runs `f` on the running state and the vte, both read-only to C for the
/// call.
fn with_vte<R>(vte: *mut Vte, f: impl FnOnce(&mut State, &Vte) -> R) -> R {
    // SAFETY: the vte the C side is running; C does not run during `f`.
    with(|s| f(s, unsafe { &*vte }))
}

// The callbacks: one trace line per dispatch.

#[unsafe(no_mangle)]
extern "C" fn oracle_print(vte: *mut Vte, c: u32) {
    with(|s| {
        out!(s.trace.out, "print {c:x}");
        s.end(vte, Line::Counted);
    });
}

#[unsafe(no_mangle)]
extern "C" fn oracle_execute(vte: *mut Vte, c: u32) {
    with(|s| {
        out!(s.trace.out, "exec {c:x}");
        s.end(vte, Line::Counted);
    });
}

#[unsafe(no_mangle)]
extern "C" fn oracle_esc(vte: *mut Vte, final_byte: u32) {
    with_vte(vte, |s, v| {
        if s.null_operation() {
            return;
        }
        out!(s.trace.out, "esc {final_byte:x}");
        s.write_intermediates(v);
        s.end(vte, Line::Dispatch);
    });
}

#[unsafe(no_mangle)]
extern "C" fn oracle_csi(vte: *mut Vte, final_byte: u32) {
    with_vte(vte, |s, v| {
        if s.null_operation() {
            return;
        }
        out!(s.trace.out, "csi {final_byte:x}");
        s.write_intermediates(v);
        s.write_params(v);
        s.end(vte, Line::Dispatch);
    });
}

#[unsafe(no_mangle)]
extern "C" fn oracle_osc_end(vte: *mut Vte, end: u32) {
    with_vte(vte, |s, v| {
        s.trace.out.push_str("osc ");
        s.trace.string_end(end);
        s.trace
            .out
            .push_str(if s.osc_truncated { " t " } else { " " });
        s.trace.hex(&v.osc_arg[..v.osc_len as usize]);
        s.end(vte, Line::Counted);
    });
}

#[unsafe(no_mangle)]
extern "C" fn oracle_xtgettcap(vte: *mut Vte) {
    with_vte(vte, |s, v| {
        if v.tcap_over {
            s.trace.out.push_str("tcap none");
        } else {
            s.trace.out.push_str("tcap ");
            s.trace.hex(&v.tcap_buf[..v.tcap_len]);
        }
        s.end(vte, Line::Counted);
    });
}

#[unsafe(no_mangle)]
extern "C" fn oracle_apc_start(vte: *mut Vte) {
    with(|s| {
        s.trace.out.push_str("apc_start");
        s.end(vte, Line::Counted);
    });
}

#[unsafe(no_mangle)]
extern "C" fn oracle_apc_put(vte: *mut Vte, c: u32) {
    with(|s| {
        out!(s.trace.out, "apc_put {c:x}");
        s.end(vte, Line::Data);
    });
}

#[unsafe(no_mangle)]
extern "C" fn oracle_apc_end(vte: *mut Vte, end: u32) {
    with(|s| {
        s.trace.out.push_str("apc_end ");
        s.trace.string_end(end);
        s.end(vte, Line::Counted);
    });
}

#[unsafe(no_mangle)]
extern "C" fn oracle_paused(_vte: *mut Vte) -> bool {
    with(|s| s.trace.paused())
}

/// The C reference's "unhandled input" warning, which a correct state
/// machine never reaches: a line the Rust side never writes.
#[unsafe(no_mangle)]
extern "C" fn oracle_warning() {
    with(|s| s.trace.out.push_str("warning\n"));
}

// The hooks: the departures, around the C reference's own functions.

#[unsafe(no_mangle)]
extern "C" fn oracle_hook_clear(vte: *mut Vte) {
    // SAFETY (each call into C below): the vte the C side is running, with
    // no borrow of the running state held across the call.
    unsafe { oracle_ref_clear(vte) };
    with(|s| {
        s.marker = None;
        s.intermediates.clear();
    });
}

#[unsafe(no_mangle)]
extern "C" fn oracle_hook_collect(vte: *mut Vte, data: u32) {
    with(|s| match data {
        0x3c..=0x3f => s.marker = Some(data),
        _ => s.intermediates.push(data),
    });
    unsafe { oracle_ref_collect(vte, data) };
}

#[unsafe(no_mangle)]
extern "C" fn oracle_hook_param(vte: *mut Vte, data: u32) {
    let i = unsafe { (*vte).csi_argc } as usize;
    let empty = data == u32::from(b';') && i < CSI_ARG_MAX && unsafe { (*vte).csi_argv[i] } == -1;
    unsafe { oracle_ref_param(vte, data) };
    // empty-params: the C reference made it 0
    if empty && with(|s| s.departs.has(Departure::EmptyParams)) {
        unsafe { (*vte).csi_argv[i] = -1 };
    }
}

#[unsafe(no_mangle)]
extern "C" fn oracle_hook_osc_collect(vte: *mut Vte, c: u32) {
    // osc-prefix: after a character that failed to fit, nothing
    if with(|s| s.departs.has(Departure::OscPrefix) && s.osc_truncated) {
        return;
    }
    let len = unsafe { (*vte).osc_len };
    unsafe { oracle_ref_osc_collect(vte, c) };
    // a noncharacter encodes to nothing, which is no truncation
    let mut buf = [0u8; 4];
    if unsafe { (*vte).osc_len } == len && unsafe { tsm_ucs4_to_utf8(c, buf.as_mut_ptr()) } > 0 {
        with(|s| s.osc_truncated = true);
    }
}

#[unsafe(no_mangle)]
extern "C" fn oracle_hook_dcs_start(vte: *mut Vte, data: u32) {
    let (null, repeated) = with(|s| {
        s.hooked = false;
        let bytes = s.departs.has(Departure::Intermediates);
        (s.null_operation(), bytes && s.intermediates.len() > 1)
    });
    if null {
        unsafe { (*vte).dcs = DCS_IGNORED };
        return;
    }
    // intermediates: XTGETTCAP is one '+', where the flag set took two too
    if repeated {
        unsafe { (*vte).csi_flags |= CSI_OTHER };
    }
    unsafe { oracle_ref_dcs_start(vte, data) };
    if unsafe { (*vte).dcs } == DCS_XTGETTCAP {
        return;
    }
    with_vte(vte, |s, v| {
        s.hooked = true;
        out!(s.trace.out, "dcs_hook {data:x}");
        s.write_intermediates(v);
        s.write_params(v);
        s.end(vte, Line::Counted);
    });
}

#[unsafe(no_mangle)]
extern "C" fn oracle_hook_dcs_collect(vte: *mut Vte, data: u32) {
    with(|s| {
        if s.hooked {
            out!(s.trace.out, "dcs_put {data:x}");
            s.end(vte, Line::Data);
        }
    });
    unsafe { oracle_ref_dcs_collect(vte, data) };
}

#[unsafe(no_mangle)]
extern "C" fn oracle_hook_dcs_end(vte: *mut Vte, data: u32) {
    with(|s| {
        if s.hooked {
            s.trace.out.push_str("dcs_unhook ");
            s.trace.string_end(data);
            s.end(vte, Line::Counted);
        }
        s.hooked = false;
    });
    unsafe { oracle_ref_dcs_end(vte, data) };
}

#[unsafe(no_mangle)]
extern "C" fn oracle_hook_action(vte: *mut Vte, data: u32, action: i32) {
    let deferred = with_vte(vte, |s, v| {
        if action == ACTION_OSC_START {
            s.osc_truncated = false;
        }
        // string-end: an ESC ends no string until the next character
        let end = matches!(action, ACTION_OSC_END | ACTION_DCS_END | ACTION_APC_END);
        if !(s.departs.has(Departure::StringEnd) && data == 0x1b && end) {
            return false;
        }
        s.pending = Some(Pending {
            action,
            osc: v.osc_arg,
            osc_len: v.osc_len,
            osc_truncated: s.osc_truncated,
        });
        true
    });
    if !deferred {
        unsafe { oracle_ref_action(vte, data, action) };
    }
}

/// The states of a sequence's header, where gr-as-gl applies.
fn in_header(state: u32) -> bool {
    matches!(
        state,
        STATE_ESC
            | STATE_ESC_INT
            | STATE_CSI_ENTRY
            | STATE_CSI_PARAM
            | STATE_CSI_INT
            | STATE_CSI_IGNORE
            | STATE_DCS_ENTRY
            | STATE_DCS_PARAM
            | STATE_DCS_INT
    )
}

#[unsafe(no_mangle)]
extern "C" fn oracle_hook_parse(vte: *mut Vte, mut raw: u32) {
    let (flags, state) = unsafe { ((*vte).flags, (*vte).state) };
    let seven = flags & FLAG_7BIT_MODE != 0;
    let eight = !seven && flags & FLAG_8BIT_MODE != 0;
    let utf8 = !seven && !eight;
    let departs = with(|s| s.departs);

    // c1-utf8: dropped before the state machine
    if departs.has(Departure::C1Utf8) && utf8 && (0x80..=0x9f).contains(&raw) {
        return;
    }
    // gr-as-gl: read as the byte 80 below
    if departs.has(Departure::GrAsGl) && eight && (0xa0..=0xff).contains(&raw) && in_header(state) {
        raw -= 0x80;
    }
    // string-end: the character after the ESC ends the string
    if let Some(p) = with(|s| s.pending.take()) {
        if p.action == ACTION_OSC_END {
            unsafe {
                (*vte).osc_arg = p.osc;
                (*vte).osc_len = p.osc_len;
            }
            with(|s| s.osc_truncated = p.osc_truncated);
        }
        let end = if raw == u32::from(b'\\') { 0x9c } else { 0x1b };
        unsafe { oracle_ref_action(vte, end, p.action) };
    }
    // reset-apc-st
    if with(|s| std::mem::take(&mut s.drop_backslash)) && raw == u32::from(b'\\') {
        return;
    }
    // past-ascii: a character from U+00A0 up ends the header
    if departs.has(Departure::PastAscii) && utf8 && raw >= 0xa0 {
        let to = match state {
            STATE_ESC | STATE_ESC_INT => Some(STATE_GROUND),
            STATE_DCS_ENTRY | STATE_DCS_PARAM | STATE_DCS_INT => Some(STATE_DCS_IGNORE),
            _ => None,
        };
        if let Some(to) = to {
            // the character still ends an XTGETTCAP request's wait for `\`,
            // as the C reference's parse_data does for every character
            // (tsm-vte.c:3282-3287)
            unsafe {
                (*vte).dcs_esc = false;
                oracle_trans(vte, raw, to as i32, ACTION_NONE);
            }
            return;
        }
    }
    unsafe { oracle_ref_parse(vte, raw) };
}

/// The C side of one stream.
pub struct CSide {
    vte: *mut Vte,
    state: Option<State>,
}

impl CSide {
    pub fn new(mode: InputMode, departs: Departs) -> Self {
        // SAFETY: a fresh vte, freed in drop.
        let vte = unsafe { oracle_new() };
        assert!(!vte.is_null(), "the C side is out of memory");
        let mut side = CSide {
            vte,
            state: Some(State {
                departs,
                trace: Trace::new(mode),
                marker: None,
                intermediates: Vec::new(),
                osc_truncated: false,
                hooked: false,
                pending: None,
                drop_backslash: false,
            }),
        };
        side.mode_changed();
        side
    }

    fn state(&mut self) -> &mut State {
        self.state
            .as_mut()
            .expect("the state is back after each call")
    }
}

impl Drop for CSide {
    fn drop(&mut self) {
        // SAFETY: the vte oracle_new returned, freed once.
        unsafe { oracle_free(self.vte) };
    }
}

impl Side for CSide {
    fn trace(&mut self) -> &mut Trace {
        &mut self.state().trace
    }

    /// Feeds the data whole, as one call to the C reference's vte_input.
    fn feed(&mut self, data: &[u8]) -> usize {
        RUNNING.with(|r| *r.borrow_mut() = self.state.take());
        // SAFETY: the vte this side owns, and data for the call.
        let n = unsafe { oracle_input(self.vte, data.as_ptr(), data.len()) };
        self.state = RUNNING.with(|r| r.borrow_mut().take());
        n
    }

    fn reset(&mut self) {
        // SAFETY: the vte this side owns; the reset lines call no callback.
        unsafe { oracle_reset(self.vte) };
        let s = self.state();
        s.hooked = false;
        let apc = s.pending.take().is_some_and(|p| p.action == ACTION_APC_END);
        s.drop_backslash = s.departs.has(Departure::ResetApcSt) && apc;
        s.trace.out.push_str("reset\n");
    }

    fn mode_changed(&mut self) {
        let vte = self.vte;
        self.state().sync_mode(vte);
    }
}
