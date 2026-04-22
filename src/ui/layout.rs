//! Layout composites — higher-level structural widgets that compose out of
//! primitives to give the app page-level building blocks.
//!
//! Phase 4 coverage:
//!
//! * [`card`] — header/body/footer card
//! * [`panel`] — card with a title+description header
//! * [`section`] — titled content group inside a panel
//! * [`tabs`] — horizontal tab bar with active indicator
//! * [`sidebar`] — vertical nav column with active highlighting
//! * [`toolbar`] — horizontal action bar
//! * [`page_header`] — page-level title + subtitle + actions row
//! * [`empty_state`] — centered placeholder for empty lists
//! * [`stat`] — KPI card (label + value + optional delta)

pub mod card;
pub mod collapsible_header;
pub mod empty_state;
pub mod page_header;
pub mod panel;
pub mod section;
pub mod sidebar;
pub mod stat;
pub mod tabs;
pub mod toolbar;
