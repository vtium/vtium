//! A rule's match: `*`, or `key:value` tokens every one of which must hold
//! (ADR-1401 D4).

use crate::identity::{Class, Edid, Identity, UsbSerial};

/// One `key:value` token of a rule's match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Token {
    /// `path:ID_PATH`: that `ID_PATH`, or any below it by an interface
    /// (`...:3.3` takes `...:3.3:1.0`).
    Path(String),
    /// `serial:VENDOR:PRODUCT:SERIAL`.
    Serial(UsbSerial),
    /// `bluetooth:ADDRESS`, the device's unique id.
    Bluetooth(String),
    /// `id:VENDOR:PRODUCT`.
    Id(u16, u16),
    /// `name:GLOB`.
    Name(String),
    /// `class:CLASS`.
    Class(Class),
    /// `edid:MAKER:PRODUCT:SERIAL`.
    Edid(Edid),
    /// `connector:CARD/CONNECTOR`, a glob each.
    Connector { card: String, connector: String },
    /// `connector-path:PATH`.
    ConnectorPath(String),
    /// `card:GLOB`.
    Card(String),
    /// `udev:NAME=GLOB`.
    Udev { name: String, value: String },
}

/// Why a token did not parse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenError {
    /// No `:` after the key.
    NoKey,
    /// A key D4 does not name.
    UnknownKey,
    /// A known key with a value it cannot take.
    BadValue,
}

/// A hex number of 16 bits, with or without `0x`.
fn hex16(s: &str) -> Option<u16> {
    let digits = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    if digits.is_empty() || digits.len() > 4 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u16::from_str_radix(digits, 16).ok()
}

impl Token {
    /// Every key D4 names, as the file spells it.
    pub const KEYS: [&'static str; 11] = [
        "path",
        "serial",
        "bluetooth",
        "id",
        "name",
        "class",
        "edid",
        "connector",
        "connector-path",
        "card",
        "udev",
    ];

    /// How each key's token is written, in [`Token::KEYS`]' order, which
    /// an error naming a value its key cannot take shows.
    pub const FORMS: [&'static str; 11] = [
        "path:PATH",
        "serial:VENDOR:PRODUCT:SERIAL",
        "bluetooth:ADDRESS",
        "id:VENDOR:PRODUCT",
        "name:NAME",
        "class:CLASS",
        "edid:MAKER:PRODUCT:SERIAL",
        "connector:CARD/CONNECTOR",
        "connector-path:PATH",
        "card:CARD",
        "udev:NAME=GLOB",
    ];

    pub(crate) fn parse(token: &str) -> Result<Token, TokenError> {
        let (key, value) = token.split_once(':').ok_or(TokenError::NoKey)?;
        if !Token::KEYS.contains(&key) {
            return Err(TokenError::UnknownKey);
        }
        if value.is_empty() {
            return Err(TokenError::BadValue);
        }
        let bad = TokenError::BadValue;
        Ok(match key {
            "path" => Token::Path(value.to_owned()),
            "serial" => {
                // the serial is last and may itself hold a `:`
                let mut parts = value.splitn(3, ':');
                let vendor = parts.next().and_then(hex16).ok_or(bad)?;
                let product = parts.next().and_then(hex16).ok_or(bad)?;
                let serial = parts.next().filter(|s| !s.is_empty()).ok_or(bad)?;
                Token::Serial(UsbSerial {
                    vendor,
                    product,
                    serial: serial.to_owned(),
                })
            }
            "bluetooth" => Token::Bluetooth(value.to_owned()),
            "id" => {
                let (vendor, product) = value.split_once(':').ok_or(bad)?;
                Token::Id(hex16(vendor).ok_or(bad)?, hex16(product).ok_or(bad)?)
            }
            "name" => Token::Name(value.to_owned()),
            "class" => Token::Class(Class::from_name(value).ok_or(bad)?),
            "edid" => {
                let mut parts = value.splitn(3, ':');
                let maker = parts
                    .next()
                    .filter(|m| m.len() == 3 && m.bytes().all(|b| b.is_ascii_uppercase()))
                    .ok_or(bad)?;
                let product = parts.next().and_then(hex16).ok_or(bad)?;
                let serial = parts.next().filter(|s| !s.is_empty()).ok_or(bad)?;
                Token::Edid(Edid {
                    maker: maker.to_owned(),
                    product,
                    serial: serial.to_owned(),
                })
            }
            "connector" => {
                let (card, connector) = value.rsplit_once('/').ok_or(bad)?;
                if card.is_empty() || connector.is_empty() {
                    return Err(bad);
                }
                Token::Connector {
                    card: card.to_owned(),
                    connector: connector.to_owned(),
                }
            }
            "connector-path" => Token::ConnectorPath(value.to_owned()),
            "card" => Token::Card(value.to_owned()),
            _ => {
                let (name, glob) = value.split_once('=').ok_or(bad)?;
                if name.is_empty() {
                    return Err(bad);
                }
                Token::Udev {
                    name: name.to_owned(),
                    value: glob.to_owned(),
                }
            }
        })
    }

    /// The key as the file spells it.
    pub fn key(&self) -> &'static str {
        match self {
            Token::Path(_) => "path",
            Token::Serial(_) => "serial",
            Token::Bluetooth(_) => "bluetooth",
            Token::Id(..) => "id",
            Token::Name(_) => "name",
            Token::Class(_) => "class",
            Token::Edid(_) => "edid",
            Token::Connector { .. } => "connector",
            Token::ConnectorPath(_) => "connector-path",
            Token::Card(_) => "card",
            Token::Udev { .. } => "udev",
        }
    }

    /// Whether the key reads what only a udev daemon records, so it
    /// matches nothing without one (D4).
    pub fn needs_udev(&self) -> bool {
        matches!(
            self,
            Token::Path(_)
                | Token::Serial(_)
                | Token::Card(_)
                | Token::Connector { .. }
                | Token::Udev { .. }
        )
    }

    /// Whether the token holds for `id`.
    pub fn matches(&self, id: &Identity) -> bool {
        match self {
            Token::Path(want) => id.path.as_deref().is_some_and(|path| {
                path.strip_prefix(want.as_str())
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with(':'))
            }),
            Token::Serial(want) => id.serial.as_ref() == Some(want),
            Token::Bluetooth(want) => id.unique.as_deref() == Some(want.as_str()),
            Token::Id(vendor, product) => id.id == Some((*vendor, *product)),
            Token::Name(pattern) => id.name.as_deref().is_some_and(|n| glob(pattern, n)),
            Token::Class(class) => id.classes.has(*class),
            Token::Edid(want) => id.edid.as_ref() == Some(want),
            Token::Connector { card, connector } => {
                id.card.as_deref().is_some_and(|c| glob(card, c))
                    && id.connector.as_deref().is_some_and(|c| glob(connector, c))
            }
            Token::ConnectorPath(want) => id.connector_path.as_deref() == Some(want.as_str()),
            Token::Card(pattern) => id.card.as_deref().is_some_and(|c| glob(pattern, c)),
            Token::Udev { name, value } => id.properties.get(name).is_some_and(|v| glob(value, v)),
        }
    }
}

/// A rule's match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Match {
    /// `*`: every device of the rule's kind.
    Any,
    /// Tokens every one of which must hold.
    All(Vec<Token>),
}

impl Match {
    pub fn matches(&self, id: &Identity) -> bool {
        match self {
            Match::Any => true,
            Match::All(tokens) => tokens.iter().all(|t| t.matches(id)),
        }
    }

    /// Whether every device this match takes, `other` takes too, as far as
    /// the tokens show: `other` is `*`, or each of its tokens is one of
    /// these.
    pub(crate) fn within(&self, other: &Match) -> bool {
        match (self, other) {
            (_, Match::Any) => true,
            (Match::Any, Match::All(_)) => false,
            (Match::All(mine), Match::All(theirs)) => theirs.iter().all(|t| mine.contains(t)),
        }
    }

    pub(crate) fn tokens(&self) -> &[Token] {
        match self {
            Match::Any => &[],
            Match::All(tokens) => tokens,
        }
    }
}

/// Whether `text` matches the glob `pattern`: `*` takes any run of
/// characters, `?` takes one, and every other character takes itself.
///
/// It backtracks only to the last `*`, so it takes at most the product of
/// the two lengths in steps.
pub fn glob(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    // the last `*` seen, and where in the text it is to take one more
    let mut star: Option<(usize, usize)> = None;
    while let Some(&c) = t.get(ti) {
        match p.get(pi) {
            Some('*') => {
                star = Some((pi, ti));
                pi = pi.saturating_add(1);
            }
            Some(&want) if want == '?' || want == c => {
                pi = pi.saturating_add(1);
                ti = ti.saturating_add(1);
            }
            _ => match star {
                Some((at, from)) => {
                    let next = from.saturating_add(1);
                    star = Some((at, next));
                    pi = at.saturating_add(1);
                    ti = next;
                }
                None => return false,
            },
        }
    }
    p.get(pi..)
        .is_some_and(|rest| rest.iter().all(|&c| c == '*'))
}
