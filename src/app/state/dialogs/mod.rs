//! Per-dialog and per-feature auxiliary state types, split out of the state
//! module. Everything here is small session/dialog/pipeline state; the two core
//! structs ([`OctaApp`](super::OctaApp) / [`TabState`](super::TabState)) stay in
//! `mod.rs`. All types are re-exported from the parent, so call sites keep using
//! `crate::app::state::<Name>`.
//!
//! Split six ways by topic; every type is re-exported here so `state/mod.rs`
//! and the dialog renderers keep their existing paths.

use std::sync::{Arc, Mutex};

use octa::data::{self, DataTable, ViewMode};
use octa::ui;
use octa::ui::settings::DialogSize;

mod analysis;
mod load;
mod rows;
mod save;
mod tabs;
mod transform;

pub(crate) use analysis::*;
pub(crate) use load::*;
pub(crate) use rows::*;
pub(crate) use save::*;
pub(crate) use tabs::*;
pub(crate) use transform::*;
