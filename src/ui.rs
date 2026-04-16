//! nanite-ui — in-tree component library for Nanite Clip.
//!
//! Phase 1 ships a theme/token layer and a set of primitive widgets that
//! style iced's built-ins through our design tokens. Later phases will layer
//! overlays, composite layouts, data widgets, and pickers on top.
//!
//! `dead_code` is silenced module-wide while primitives land ahead of their
//! first call sites; remove once the migration has consumed each one.

#![allow(dead_code)]

pub mod app;
pub mod data;
pub mod layout;
pub mod overlay;
pub mod pickers;
pub mod primitives;
pub mod theme;
