//! CSI and DCS parameters, and the intermediates and markers collected
//! with them.
//!
//! Both keep the C reference's representation, so that dispatch code ported
//! from tsm-vte.c reads the same values: parameters as `int` with -1 for
//! omitted, and intermediates as a set of flags.

use core::fmt;
use core::ops::{BitOr, BitOrAssign};

use crate::bounds::{PARAM_VALUE_CAP, PARAMS_MAX, SUBPARAMS_MAX};

/// The parameters of a CSI sequence or a DCS string, as the C reference
/// keeps them in `csi_argv`, `csi_subargc` and `csi_subargv`.
///
/// A parameter is -1 when it was omitted and is the last one, or past
/// those the sequence carried. An omitted parameter before a `;` is 0
/// (tsm-vte.c:1284-1286): `CSI ;5H` has 0 and 5, `CSI 5;H` has 5 and -1.
/// An omitted sub-parameter is -1.
#[derive(Clone)]
pub struct Params {
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
            argc: 0,
            values: [-1; PARAMS_MAX],
            sub_len: [0; PARAMS_MAX],
            subs: [[-1; SUBPARAMS_MAX]; PARAMS_MAX],
            sub_drop: false,
        }
    }

    /// `do_clear`'s parameter half (tsm-vte.c:1220-1225).
    pub(crate) fn clear(&mut self) {
        self.argc = 0;
        self.values = [-1; PARAMS_MAX];
        self.sub_len = [0; PARAMS_MAX];
        self.sub_drop = false;
    }

    /// `do_param` (tsm-vte.c:1277-1326), for a byte from `0` to `;`.
    pub(crate) fn put(&mut self, data: u32) {
        if data == u32::from(b';') {
            if self.argc < PARAMS_MAX {
                if self.values[self.argc] == -1 {
                    self.values[self.argc] = 0;
                }
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

/// The intermediates and private markers a sequence collected, as the C
/// reference's `csi_flags` (tsm-vte.c:108-121, 1232-1275).
///
/// Each byte sets a flag, so the set keeps neither order nor count:
/// `CSI ! ! p` and `CSI ! p` collect the same set. A byte with no flag of
/// its own sets [`OTHER`](Self::OTHER), so that a sequence carrying it is
/// never mistaken for the plain one. The bit values are the C reference's.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Intermediates(u16);

impl Intermediates {
    /// No intermediate or marker.
    pub const NONE: Self = Intermediates(0);
    /// `!`, `CSI_BANG`.
    pub const BANG: Self = Intermediates(0x0001);
    /// `$`, `CSI_CASH`.
    pub const DOLLAR: Self = Intermediates(0x0002);
    /// The `?` marker, `CSI_WHAT`.
    pub const QUESTION: Self = Intermediates(0x0004);
    /// The `>` marker, `CSI_GT`.
    pub const GREATER: Self = Intermediates(0x0008);
    /// Space, `CSI_SPACE`.
    pub const SPACE: Self = Intermediates(0x0010);
    /// `'`, `CSI_SQUOTE`.
    pub const QUOTE: Self = Intermediates(0x0020);
    /// `"`, `CSI_DQUOTE`.
    pub const DQUOTE: Self = Intermediates(0x0040);
    /// `*`, `CSI_MULT`.
    pub const STAR: Self = Intermediates(0x0080);
    /// `+`, `CSI_PLUS`.
    pub const PLUS: Self = Intermediates(0x0100);
    /// `(`, `CSI_POPEN`.
    pub const LPAREN: Self = Intermediates(0x0200);
    /// `)`, `CSI_PCLOSE`.
    pub const RPAREN: Self = Intermediates(0x0400);
    /// `#`, `CSI_HASH`.
    pub const HASH: Self = Intermediates(0x0800);
    /// Any other intermediate or marker, `CSI_OTHER`: `%`, `&`, `,`, `-`,
    /// `.`, `/`, `<`, `=`, and in an ESC sequence a character from U+00A0
    /// up.
    pub const OTHER: Self = Intermediates(0x1000);

    /// The flag `do_collect` sets for `data` (tsm-vte.c:1232-1275).
    pub(crate) const fn of(data: u32) -> Self {
        match data {
            0x21 => Self::BANG,
            0x24 => Self::DOLLAR,
            0x3f => Self::QUESTION,
            0x3e => Self::GREATER,
            0x20 => Self::SPACE,
            0x27 => Self::QUOTE,
            0x22 => Self::DQUOTE,
            0x2a => Self::STAR,
            0x2b => Self::PLUS,
            0x28 => Self::LPAREN,
            0x29 => Self::RPAREN,
            0x23 => Self::HASH,
            _ => Self::OTHER,
        }
    }

    /// The flags as the C reference's bits.
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// Whether no flag is set.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether every flag of `other` is set.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// The flags set here and not in `allowed`: dispatch code tests
    /// `csi_flags & ~allowed` this way (tsm-vte.c:2475).
    pub const fn difference(self, allowed: Self) -> Self {
        Intermediates(self.0 & !allowed.0)
    }
}

impl BitOr for Intermediates {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Intermediates(self.0 | rhs.0)
    }
}

impl BitOrAssign for Intermediates {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl fmt::Debug for Intermediates {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const NAMES: [(Intermediates, &str); 13] = [
            (Intermediates::BANG, "!"),
            (Intermediates::DOLLAR, "$"),
            (Intermediates::QUESTION, "?"),
            (Intermediates::GREATER, ">"),
            (Intermediates::SPACE, "SP"),
            (Intermediates::QUOTE, "'"),
            (Intermediates::DQUOTE, "\""),
            (Intermediates::STAR, "*"),
            (Intermediates::PLUS, "+"),
            (Intermediates::LPAREN, "("),
            (Intermediates::RPAREN, ")"),
            (Intermediates::HASH, "#"),
            (Intermediates::OTHER, "OTHER"),
        ];
        let mut set = f.debug_set();
        for (flag, name) in NAMES {
            if self.contains(flag) {
                set.entry(&format_args!("{name}"));
            }
        }
        set.finish()
    }
}
