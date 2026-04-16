//! Primitive widgets — the small reusable building blocks that every other
//! component composes out of. Phase-1 coverage only; overlays, layout, data,
//! and pickers live in their own modules and will be added in later phases.
//!
//! Individual primitives live in their own submodules; import them directly
//! (`crate::ui::primitives::button::button`). Top-level re-exports are added
//! lazily as call sites start using each primitive, so an unused wrapper
//! doesn't bleed into every diagnostic run.

pub mod avatar;
pub mod badge;
pub mod button;
pub mod checkbox;
pub mod input;
pub mod kbd;
pub mod label;
pub mod pick_list;
pub mod progress;
pub mod radio;
pub mod separator;
pub mod skeleton;
pub mod slider;
pub mod spinner;
pub mod switch;
pub mod tag;
pub mod textarea;
pub mod tooltip;
