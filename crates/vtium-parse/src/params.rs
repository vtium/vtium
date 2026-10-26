//! CSI and DCS parameters, with the private marker that may open them, and
//! the intermediates of ESC, CSI and DCS sequences.

use core::fmt;

use crate::bounds::{INTERMEDIATES_MAX, PARAM_VALUE_CAP, PARAMS_MAX, SUBPARAMS_MAX};

/// The parameters of a CSI sequence or a DCS string.
///
/// A parameter is `None` when it was empty or past those the sequence
/// carried, wherever it falls (ECMA-48 5.4.2 e and g): `CSI ;5H` has `None`
/// and `Some(5)`, and `CSI 5;H` has `Some(5)` and `None`. So is an empty
/// sub-parameter.
///
/// A private marker, one of `<`, `=`, `>` and `?` as the first byte of
/// the parameter string (ECMA-48 5.4.2 b), is kept apart: see
/// [`marker`](Self::marker).
#[derive(Clone)]
pub struct Params {
    marker: Option<u8>,
    /// The `;` separators taken, at most [`PARAMS_MAX`] (`csi_argc`).
    separators: usize,
    /// Each value, or [`OMITTED`].
    values: [u32; PARAMS_MAX],
    sub_len: [u8; PARAMS_MAX],
    subs: [[u32; SUBPARAMS_MAX]; PARAMS_MAX],
    /// Past [`SUBPARAMS_MAX`] colons, digits are dropped until the next
    /// `;` (`csi_subarg_drop`).
    sub_drop: bool,
}

/// The stored value of an empty parameter. No value reaches it: digits
/// stop once a value passes [`PARAM_VALUE_CAP`].
const OMITTED: u32 = u32::MAX;

impl Params {
    pub(crate) const fn new() -> Self {
        Params {
            marker: None,
            separators: 0,
            values: [OMITTED; PARAMS_MAX],
            sub_len: [0; PARAMS_MAX],
            subs: [[OMITTED; SUBPARAMS_MAX]; PARAMS_MAX],
            sub_drop: false,
        }
    }

    /// `do_clear`'s parameter half (tsm-vte.c:1220-1225).
    pub(crate) fn clear(&mut self) {
        *self = Self::new();
    }

    /// `do_param` (tsm-vte.c:1277-1326), for a byte from `0` to `;`.
    pub(crate) fn put(&mut self, byte: u8) {
        if byte == b';' {
            // An empty parameter stays omitted before a ';' too (ECMA-48
            // 5.4.2 e and g), where the C reference made it 0
            // (tsm-vte.c:1284-1286).
            self.separators = (self.separators + 1).min(PARAMS_MAX);
            self.sub_drop = false;
            return;
        }

        let i = self.separators;
        if i >= PARAMS_MAX {
            return;
        }

        let n = usize::from(self.sub_len[i]);
        if byte == b':' {
            if n < SUBPARAMS_MAX {
                self.sub_len[i] += 1;
            } else {
                self.sub_drop = true;
            }
            return;
        }

        if self.sub_drop {
            return;
        }
        let val = match n {
            0 => &mut self.values[i],
            _ => &mut self.subs[i][n - 1],
        };
        // Only digits reach here: the state machine sends `0` to `;`, and
        // `<` to `?` are markers.
        let digit = u32::from(byte - b'0');
        *val = match *val {
            OMITTED => digit,
            v if v > PARAM_VALUE_CAP => v,
            v => v * 10 + digit,
        };
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
        (self.separators + 1).min(PARAMS_MAX)
    }

    /// Parameter `i`, `None` when it was empty. Any `i` may be asked for:
    /// past the sequence's parameters it is `None`.
    pub fn get(&self, i: usize) -> Option<u32> {
        self.values.get(i).copied().filter(|&v| v != OMITTED)
    }

    /// The sub-parameters of parameter `i`, each `None` when empty:
    /// `CSI 38:2::255:0:0m` gives parameter 0 the sub-parameters
    /// `2, None, 255, 0, 0`.
    pub fn subparams(&self, i: usize) -> impl ExactSizeIterator<Item = Option<u32>> + '_ {
        let n = self.sub_len.get(i).map_or(0, |&n| usize::from(n));
        let subs = self.subs.get(i).map_or(&[][..], |subs| &subs[..n]);
        subs.iter().map(|&v| (v != OMITTED).then_some(v))
    }

    /// Whether any parameter carries sub-parameters, `csi_has_subargs`
    /// (tsm-vte.c:2457-2465).
    pub fn has_subparams(&self) -> bool {
        self.sub_len[..self.count()].iter().any(|&n| n > 0)
    }
}

impl fmt::Debug for Params {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct Subs<'a>(&'a Params, usize);
        impl fmt::Debug for Subs<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_list().entries(self.0.subparams(self.1)).finish()
            }
        }
        let mut list = f.debug_list();
        for i in 0..self.count() {
            if self.subparams(i).len() == 0 {
                list.entry(&self.get(i));
            } else {
                list.entry(&(self.get(i), Subs(self, i)));
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
