//! Static Markdown bodies for the in-app documentation dialog. One
//! `const &str` per section; the parent module's `sections()` joins them with
//! the live shortcut table at render time.
//!
//! Split six ways by topic. It was one 4,261-line file, which is too big to
//! open just to correct a sentence. Every constant is re-exported here, so
//! `sections()` still refers to them by bare name.
//!
//! The topic files declare their constants plain `pub` rather than
//! `pub(super)`: `pub(super)` there would mean "visible inside `content`",
//! which cannot then be re-exported one level wider. `content` is itself a
//! private module, so `pub` inside it leaks nothing beyond this dialog.

mod analysis;
mod basics;
mod columns;
mod integrations;
mod search;
mod views;

pub(super) use analysis::*;
pub(super) use basics::*;
pub(super) use columns::*;
pub(super) use integrations::*;
pub(super) use search::*;
pub(super) use views::*;
