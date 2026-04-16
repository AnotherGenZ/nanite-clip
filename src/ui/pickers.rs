//! Pickers — widgets for selecting values from bounded domains.
//!
//! Phase 6 coverage:
//!
//! * [`date`] — calendar month-grid date picker
//! * [`time`] — hour/minute/second number spinners
//! * [`color`] — preset swatch grid + hex preview
//! * [`font`] — lightweight dropdown of named fonts
//!
//! File picker is deferred: it needs `rfd` plus async integration, so it
//! will ship as a `Task`-based helper rather than a view widget when the
//! app wires up its first upload/export flow.

pub mod color;
pub mod date;
pub mod font;
pub mod time;
