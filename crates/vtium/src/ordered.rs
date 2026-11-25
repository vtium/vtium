//! Ordered sets and maps built by inserting one item at a time.
//!
//! `collect()` into a `BTreeSet` or a `BTreeMap` first gathers the items
//! into a vector and sorts the vector with the standard library's stable
//! sort. Rust compiles a separate copy of that sort for each item type, and
//! each copy adds about 3 to 5 KB to the minspec binary. Inserting the items
//! one at a time reuses the tree's insert, which the binary needs anyway.
//! A map comes out the same, because both ways keep the last value given
//! for a key. A set keeps the first of the items that compare equal, where
//! `collect()` keeps the last, so `set` suits only a type whose equal items
//! are identical, such as a number or a `String`, but not a `PathBuf`. The
//! vtium crate builds its sets and maps with these two functions instead of
//! `collect()`, which keeps the minspec binary within the size budget that
//! tier 3 holds it to.
//!
//! References:
//! - ADR-1200 D19: the minspec binary's size budget.

use std::collections::{BTreeMap, BTreeSet};

/// The set of `items`.
pub(crate) fn set<T: Ord>(items: impl IntoIterator<Item = T>) -> BTreeSet<T> {
    let mut set = BTreeSet::new();
    set.extend(items);
    set
}

/// The map of `pairs`. When a key comes more than once, its last value
/// stays, as with `collect()`.
pub(crate) fn map<K: Ord, V>(pairs: impl IntoIterator<Item = (K, V)>) -> BTreeMap<K, V> {
    let mut map = BTreeMap::new();
    map.extend(pairs);
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_set_holds_what_collect_holds() {
        let items = [5_u16, 1, 3, 1, 2, 3];
        let collected: BTreeSet<u16> = items.into_iter().collect();
        assert_eq!(set(items), collected);
        assert_eq!(set(items).into_iter().collect::<Vec<_>>(), [1, 2, 3, 5]);
    }

    #[test]
    fn a_map_keeps_the_last_value_of_a_key_as_collect_does() {
        let pairs = [("b", 1), ("a", 2), ("b", 3), ("c", 4), ("a", 5)];
        let collected: BTreeMap<&str, i32> = pairs.into_iter().collect();
        assert_eq!(map(pairs), collected);
        assert_eq!(map(pairs).get("a"), Some(&5));
        assert_eq!(map(pairs).get("b"), Some(&3));
    }
}
