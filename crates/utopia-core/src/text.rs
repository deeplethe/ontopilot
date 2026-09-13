//! Text utilities. Used in two places: parsed document bodies, and memory
//! episodes arriving from chat / MCP. Both paths must strip NUL before
//! the text lands in `chunks.text` (#611 / #630 / #665).
//!
//! Postgres `TEXT` does not accept `0x00` — the insert fails with
//! `invalid byte sequence for encoding "UTF8": 0x00`, the transaction
//! rolls back, and the whole document / episode is lost for a few
//! bytes of it.
//!
//! `Cow` borrow path: most bodies carry no NUL, so we hand back a
//! borrowed slice and avoid copying the whole text.

use std::borrow::Cow;

/// Strip `\0` from text going into a `TEXT` column.
///
/// `Cow::Borrowed` when there is nothing to remove — almost every
/// document, almost every episode.
pub fn without_nul(text: &str) -> Cow<'_, str> {
    if text.contains('\0') {
        Cow::Owned(text.replace('\0', ""))
    } else {
        Cow::Borrowed(text)
    }
}
