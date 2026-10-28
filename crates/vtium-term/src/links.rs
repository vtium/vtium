//! The link store: each distinct OSC 8 link once, by its id parameter and
//! URI, which a line's link table names by [`LinkId`] (ADR-1002, "Link
//! storage"), and the rule for what a link may hold ("What a link may
//! hold").
//!
//! The store charges each entry its id and URI bytes and
//! [`ENTRY_OVERHEAD`], and holds at most [`LINKS_BYTES_MAX`] of charges.
//! It lets a sweep run at most once per quarter of the bound of links
//! opened, so a stream of opens the full screens refuse pays for a sweep
//! only as often as one that fills the store.
//! It does not know which lines name which links: the screen, which holds
//! the lines, sweeps it (`screen/links.rs`), and a sweep keeps the entries
//! the screen marks live, renumbered densely in the order they were added.
//!
//! Entries are kept in a list by `LinkId` and found by a second list of
//! their numbers, sorted by id and URI, so a lookup is a binary search and
//! the store needs no hash (docs/architecture.md, "Determinism").

use crate::bounds::LINKS_BYTES_MAX;
use crate::line::LinkId;

/// What each entry is charged beyond its id and URI bytes: its slot in
/// the entries, its place in the order, and its allocation.
pub(crate) const ENTRY_OVERHEAD: usize = 64;

/// A URI's longest, VTE's and iTerm2's limit.
const URI_MAX: usize = 2083;

/// An id's longest, VTE's limit.
const ID_MAX: usize = 250;

/// One link: its id parameter, which may be empty, then its URI, as one
/// allocation of printable ASCII.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    text: Box<str>,
    id_len: u8,
}

impl Entry {
    fn id(&self) -> &str {
        self.text
            .get(..usize::from(self.id_len))
            .unwrap_or_default()
    }

    fn uri(&self) -> &str {
        self.text
            .get(usize::from(self.id_len)..)
            .unwrap_or_default()
    }

    fn key(&self) -> (&str, &str) {
        (self.id(), self.uri())
    }
}

/// The links a terminal holds, shared by its main and alternate screens
/// and its scrollback.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LinkStore {
    /// By `LinkId`.
    entries: Vec<Entry>,
    /// The entries' numbers, sorted by id and URI.
    order: Vec<u32>,
    /// What the entries are charged.
    charged: usize,
    /// The bytes of links still to be opened, added or refused, before the
    /// next sweep may run: a quarter of the bound after each sweep, so a
    /// sweep, linear in the lines held, runs at most once per quarter of
    /// the bound of new links, whatever it freed.
    owed: usize,
    /// The sweeps run, for the test of their cost.
    #[cfg(test)]
    pub(crate) sweeps: usize,
}

impl LinkStore {
    /// What a link of `id` and `uri` is charged.
    pub(crate) fn cost(id: &str, uri: &str) -> usize {
        id.len()
            .saturating_add(uri.len())
            .saturating_add(ENTRY_OVERHEAD)
    }

    /// The entries' charges.
    pub(crate) fn charged(&self) -> usize {
        self.charged
    }

    /// Counts the opening of a link charged `cost`, added or refused.
    pub(crate) fn opened(&mut self, cost: usize) {
        self.owed = self.owed.saturating_sub(cost);
    }

    /// Whether a sweep may run: a quarter of the bound of links has been
    /// opened since the last.
    pub(crate) fn may_sweep(&self) -> bool {
        self.owed == 0
    }

    /// Whether `cost` more stays within [`LINKS_BYTES_MAX`].
    pub(crate) fn fits(&self, cost: usize) -> bool {
        self.charged.saturating_add(cost) <= LINKS_BYTES_MAX
    }

    /// How many links the store holds; each `LinkId` is below it.
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// The id parameter and URI of `link`; an id is `None` when the link
    /// has none.
    pub(crate) fn get(&self, link: LinkId) -> Option<(Option<&str>, &str)> {
        let entry = self.entries.get(usize::try_from(link.get()).ok()?)?;
        let id = Some(entry.id()).filter(|id| !id.is_empty());
        Some((id, entry.uri()))
    }

    /// What `link` is charged, 0 for a link the store does not hold.
    pub(crate) fn cost_of(&self, link: LinkId) -> usize {
        self.get(link)
            .map_or(0, |(id, uri)| Self::cost(id.unwrap_or_default(), uri))
    }

    /// Where the entry of `id` and `uri` is, or would go, in the order.
    fn search(&self, id: &str, uri: &str) -> Result<usize, usize> {
        self.order.binary_search_by(|&n| {
            let entry = usize::try_from(n).ok().and_then(|n| self.entries.get(n));
            entry.map_or(("", ""), Entry::key).cmp(&(id, uri))
        })
    }

    /// The link of `id` and `uri`, when the store holds it.
    pub(crate) fn find(&self, id: &str, uri: &str) -> Option<LinkId> {
        let at = self.search(id, uri).ok()?;
        self.order.get(at).map(|&n| LinkId::new(n))
    }

    /// Adds the link of `id` and `uri`, which the caller has checked is
    /// absent and [`fits`](Self::fits).
    pub(crate) fn insert(&mut self, id: &str, uri: &str) -> Option<LinkId> {
        let (Err(at), Ok(id_len), Ok(n)) = (
            self.search(id, uri),
            u8::try_from(id.len()),
            u32::try_from(self.entries.len()),
        ) else {
            return None;
        };
        let text = [id, uri].concat().into_boxed_str();
        // A list's first allocation holds four; the first entry takes room
        // for itself alone, so one link's heap stays under its charge.
        if self.entries.capacity() == 0 {
            self.entries.reserve_exact(1);
            self.order.reserve_exact(1);
        }
        self.entries.push(Entry { text, id_len });
        self.order.insert(at, n);
        self.charged = self.charged.saturating_add(Self::cost(id, uri));
        Some(LinkId::new(n))
    }

    /// Keeps the entries `live` marks, renumbered in their order, and
    /// returns the new `LinkId` of each old one, `None` for those freed.
    /// The lists are allocated to fit, so the heap follows the charge down.
    pub(crate) fn retain(&mut self, live: &[bool]) -> Vec<Option<LinkId>> {
        #[cfg(test)]
        {
            self.sweeps = self.sweeps.saturating_add(1);
        }
        self.owed = LINKS_BYTES_MAX / 4;
        let mut map = Vec::with_capacity(self.entries.len());
        let kept = live.iter().filter(|&&l| l).count();
        let mut entries = Vec::with_capacity(kept);
        self.charged = 0;
        for (i, entry) in core::mem::take(&mut self.entries).into_iter().enumerate() {
            if !live.get(i).copied().unwrap_or(false) {
                map.push(None);
                continue;
            }
            let n = u32::try_from(entries.len()).unwrap_or(u32::MAX);
            map.push(Some(LinkId::new(n)));
            self.charged = self
                .charged
                .saturating_add(Self::cost(entry.id(), entry.uri()));
            entries.push(entry);
        }
        self.entries = entries;
        let mut order = Vec::with_capacity(kept);
        for &old in &self.order {
            let new = usize::try_from(old)
                .ok()
                .and_then(|o| map.get(o).copied().flatten());
            if let Some(new) = new {
                order.push(new.get());
            }
        }
        self.order = order;
        map
    }

    /// The bytes the store holds on the heap, by capacity.
    pub(crate) fn heap_bytes(&self) -> usize {
        let text = self.entries.iter().map(|e| e.text.len()).sum::<usize>();
        [
            self.entries.capacity().saturating_mul(size_of::<Entry>()),
            self.order.capacity().saturating_mul(size_of::<u32>()),
            text,
        ]
        .into_iter()
        .fold(0, usize::saturating_add)
    }
}

/// What an OSC 8 asks: a link opened, by its id parameter, which may be
/// empty, and its URI, or the open link closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Osc8<'a> {
    Open { id: &'a str, uri: &'a str },
    Close,
}

/// Reads `params ; URI`, the string after `8;`. The URI is 1 to 2,083
/// bytes, each 0x21 to 0x7E, and starts with a scheme (RFC 3986 §3.1); a
/// URI that fails any of these, an empty one included, closes the link.
/// Of the parameters, `key=value` pairs separated by `:`, only the first
/// `id` is read: 1 to 250 bytes, each 0x21 to 0x7E, and an id that fails
/// that is ignored.
pub(crate) fn parse(args: &[u8]) -> Osc8<'_> {
    let Some(split) = args.iter().position(|&b| b == b';') else {
        return Osc8::Close;
    };
    let (params, uri) = (
        args.get(..split).unwrap_or_default(),
        args.get(split.saturating_add(1)..).unwrap_or_default(),
    );
    if uri.len() > URI_MAX || !printable(uri) || !has_scheme(uri) {
        return Osc8::Close;
    }
    let id = params
        .split(|&b| b == b':')
        .find_map(|p| p.strip_prefix(b"id="))
        .filter(|id| id.len() <= ID_MAX && printable(id))
        .unwrap_or_default();
    match (core::str::from_utf8(id), core::str::from_utf8(uri)) {
        (Ok(id), Ok(uri)) => Osc8::Open { id, uri },
        _ => Osc8::Close,
    }
}

/// Non-empty, and every byte from 0x21 to 0x7E: no space, no control and
/// nothing outside ASCII, so what an overlay shows is what a copy carries.
fn printable(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().all(|b| (0x21..=0x7e).contains(b))
}

/// RFC 3986 §3.1: a letter, then letters, digits, `+`, `-` and `.`, then
/// `:`.
fn has_scheme(uri: &[u8]) -> bool {
    let Some(colon) = uri.iter().position(|&b| b == b':') else {
        return false;
    };
    let scheme = uri.get(..colon).unwrap_or_default();
    scheme.first().is_some_and(u8::is_ascii_alphabetic)
        && scheme
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    #[test]
    fn the_store_finds_each_link_by_id_and_uri() {
        let mut store = LinkStore::default();
        let a = store.insert("", "https://a/").unwrap();
        let b = store.insert("1", "https://a/").unwrap();
        let c = store.insert("", "https://0/").unwrap();
        assert_eq!(store.find("", "https://a/"), Some(a));
        assert_eq!(store.find("1", "https://a/"), Some(b));
        assert_eq!(store.find("", "https://0/"), Some(c));
        assert_eq!(store.find("2", "https://a/"), None);
        assert_eq!(store.get(b), Some((Some("1"), "https://a/")));
        assert_eq!(store.get(a), Some((None, "https://a/")));
        assert_eq!(store.charged(), 10 + 11 + 10 + 3 * ENTRY_OVERHEAD);
    }

    #[test]
    fn one_links_heap_stays_under_its_charge() {
        let mut store = LinkStore::default();
        store.insert("", "x:");
        assert!(
            store.heap_bytes() <= store.charged(),
            "{}",
            store.heap_bytes()
        );
        store.insert("", "y:");
        store.retain(&[false, true]);
        assert!(
            store.heap_bytes() <= store.charged(),
            "{}",
            store.heap_bytes()
        );
    }

    #[test]
    fn retain_renumbers_the_kept_entries_in_their_order() {
        let mut store = LinkStore::default();
        for uri in ["x:3", "x:1", "x:2", "x:0"] {
            store.insert("", uri);
        }
        let map = store.retain(&[true, false, true, false]);
        assert_eq!(
            map,
            [Some(LinkId::new(0)), None, Some(LinkId::new(1)), None]
        );
        assert_eq!(store.order, [1, 0], "x:2, then x:3");
        assert_eq!(store.get(LinkId::new(1)), Some((None, "x:2")));
        assert_eq!(store.find("", "x:3"), Some(LinkId::new(0)));
        assert_eq!(store.find("", "x:1"), None);
        assert_eq!(store.charged(), 2 * (3 + ENTRY_OVERHEAD));
        assert_eq!(store.insert("", "x:1"), Some(LinkId::new(2)));
        assert_eq!(store.order, [2, 1, 0]);
    }

    #[test]
    fn parse_reads_the_id_and_checks_the_uri() {
        assert_eq!(
            parse(b"id=a:b=c;https://x/;y"),
            Osc8::Open {
                id: "a",
                uri: "https://x/;y"
            }
        );
        assert_eq!(parse(b";"), Osc8::Close);
        assert_eq!(parse(b""), Osc8::Close);
        assert_eq!(parse(b"id=a"), Osc8::Close);
        assert_eq!(parse(b"id=a b;x:"), Osc8::Open { id: "", uri: "x:" });
    }
}
