//! The type a tab's preview is, which the shared state names. This crate
//! sits below the crate that defines the previews, so [`crate::tab::Tab`]
//! cannot name their type itself: the shared state implements
//! [`Previews`] once, and every tab's [`crate::tab::Tab::preview`] returns
//! that type, borrowing from the shared state it reads.
//!
//! References:
//! - ADR-1206 D9: the trait names the shared state's types through its
//!   type parameters, and its methods have defaults.
//! - ADR-1201 D4: the previews of the Fonts, Looks and Effects tabs.

/// The shared state's type for what a tab previews.
pub trait Previews {
    /// What a tab previews and where, borrowing from the shared state for
    /// `'a`, such as the effect the Effects tab highlights.
    type Preview<'a>
    where
        Self: 'a;
}
