//! CSI and DCS parameters, with the private marker that may open them, and
//! the intermediates of ESC, CSI and DCS sequences.

use core::fmt;

use crate::bounds::{INTERMEDIATES_MAX, PARAM_VALUE_CAP, PARAMS_MAX, SUBPARAMS_MAX};

/// The parameters of a CSI sequence or a DCS string, as the C reference
/// keeps them in `csi_argv`, `csi_subargc` and `csi_subargv`.
///
/// A parameter is -1 when it was empty or past those the sequence carried,
/// wherever it falls (ECMA-48 5.4.2 e and g): `CSI ;5H` has -1 and 5, and
/// `CSI 5;H` has 5 and -1. An omitted sub-parameter is -1.
///
/// A private marker, one of `<`, `=`, `>` and `?` as the first byte of
/// the parameter string (ECMA-48 5.4.2 b), is kept apart: see
/// [`marker`](Self::marker).
#[derive(Clone)]
pub struct Params {
    marker: Option<u8>,
    /// `csi_argc`: the `;` separators taken, at most [`PARAMS_MAX`].
    argc: usize,
    values: [i32; PARAMS_MAX],
    sub_len: [u8; PARAMS_MAX],
    subs: [[i32; SUBPARAMS_MAX]; PARAMS_MAX],
    /// Past [`SUBPARAMS_MAX`] colons, digits are dropped until the next
    /// `;` (`csi_subarg_drop`).
    sub_drop: bool,
}

impl Params {
    pub(crate) const fn new() -> Self {
        Params {
            marker: None,
            argc: 0,
            values: [-1; PARAMS_MAX],
            sub_len: [0; PARAMS_MAX],
            subs: [[-1; SUBPARAMS_MAX]; PARAMS_MAX],
            sub_drop: false,
        }
    }

    /// `do_clear`'s parameter half (tsm-vte.c:1220-1225).
    pub(crate) fn clear(&mut self) {
        self.marker = None;
        self.argc = 0;
        self.values = [-1; PARAMS_MAX];
        self.sub_len = [0; PARAMS_MAX];
        self.sub_drop = false;
    }

    /// `do_param` (tsm-vte.c:1277-1326), for a byte from `0` to `;`.
    pub(crate) fn put(&mut self, data: u32) {
        if data == u32::from(b';') {
            // An empty parameter stays -1 before a ';' too (ECMA-48 5.4.2 e
            // and g), where the C reference made it 0 (tsm-vte.c:1284-1286).
            if self.argc < PARAMS_MAX {
                self.argc += 1;
            }
            self.sub_drop = false;
            return;
        }

        if self.argc >= PARAMS_MAX {
            return;
        }

        let n = usize::from(self.sub_len[self.argc]);
        if data == u32::from(b':') {
            if n < SUBPARAMS_MAX {
                self.subs[self.argc][n] = -1;
                self.sub_len[self.argc] += 1;
            } else {
                self.sub_drop = true;
            }
            return;
        }

        if self.sub_drop {
            return;
        }
        let val = if n > 0 {
            &mut self.subs[self.argc][n - 1]
        } else {
            &mut self.values[self.argc]
        };
        if *val > PARAM_VALUE_CAP {
            return;
        }
        // Only digits reach here: the state machine sends `0` to `;`, and
        // `<` to `?` are collected as markers.
        if let Some(digit) = char::from_u32(data).and_then(|c| c.to_digit(10)) {
            // `digit` is below 10 and `*val` at most PARAM_VALUE_CAP.
            let digit = digit as i32;
            *val = if *val <= 0 { digit } else { *val * 10 + digit };
        }
    }

    /// A private marker opens the parameter string.
    pub(crate) fn set_marker(&mut self, marker: u8) {
        self.marker = Some(marker);
    }

    /// The private marker that opened the parameter string, one of `<`,
    /// `=`, `>` and `?`: `CSI ? 25 h` has `Some(b'?')`.
    pub fn marker(&self) -> Option<u8> {
        self.marker
    }

    /// The parameter count as `do_csi` counts it before dispatch:
    /// one more than the `;` separators, at most [`PARAMS_MAX`]
    /// (tsm-vte.c:2542-2543). It is never 0: `CSI m` has one parameter,
    /// omitted.
    pub fn count(&self) -> usize {
        (self.argc + 1).min(PARAMS_MAX)
    }

    /// Parameter `i`, or -1 when omitted, as `csi_argv[i]` reads. Any `i`
    /// may be asked for: past the sequence's parameters it is -1.
    pub fn get(&self, i: usize) -> i32 {
        self.values.get(i).copied().unwrap_or(-1)
    }

    /// The sub-parameters of parameter `i`, each -1 when omitted:
    /// `CSI 38:2::255:0:0m` gives parameter 0 the sub-parameters
    /// `[2, -1, 255, 0, 0]`.
    pub fn subparams(&self, i: usize) -> &[i32] {
        match (self.subs.get(i), self.sub_len.get(i)) {
            (Some(subs), Some(&n)) => &subs[..usize::from(n)],
            _ => &[],
        }
    }

    /// Whether any parameter carries sub-parameters, `csi_has_subargs`
    /// (tsm-vte.c:2457-2465).
    pub fn has_subparams(&self) -> bool {
        self.sub_len[..self.count()].iter().any(|&n| n > 0)
    }
}

impl fmt::Debug for Params {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut list = f.debug_list();
        for i in 0..self.count() {
            let subs = self.subparams(i);
            if subs.is_empty() {
                list.entry(&self.get(i));
            } else {
                list.entry(&(self.get(i), subs));
            }
        }
        list.finish()
    }
}

/// The intermediate bytes (0x20 to 0x2F) of an ESC, CSI or DCS sequence, in
/// the order they came: `ESC $ ( B` has `$(`.
///
/// With the final byte they identify the control function (ECMA-48 5.4),
/// so each keeps its own byte, its place and its count. A sequence with
/// more than [`INTERMEDIATES_MAX`] dispatches nothing (vt100.net), so a
/// consumer never sees one. The C reference collapsed them to a set of
/// flags, a departure listed in the crate's README.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Intermediates {
    bytes: [u8; INTERMEDIATES_MAX],
    len: u8,
}

impl Intermediates {
    /// No intermediates.
    pub const NONE: Self = Intermediates {
        bytes: [0; INTERMEDIATES_MAX],
        len: 0,
    };

    /// Adds `byte`, or returns false when the sequence already has
    /// [`INTERMEDIATES_MAX`].
    pub(crate) fn push(&mut self, byte: u8) -> bool {
        let Some(slot) = self.bytes.get_mut(usize::from(self.len)) else {
            return false;
        };
        *slot = byte;
        self.len += 1;
        true
    }

    /// The intermediates, in order.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl fmt::Debug for Intermediates {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.as_bytes().escape_ascii().to_string())
    }
}
