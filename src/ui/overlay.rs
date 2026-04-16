//! Overlay layer — widgets that sit above normal layout flow.
//!
//! Phase 2 ships three building blocks:
//!
//! * [`toast`] — transient floating notifications, managed via a
//!   [`toast::ToastStack`] state object the app owns and a view helper that
//!   renders it into a floating column.
//! * [`banner`] — inline persistent notices (not an overlay, but lives here
//!   because it shares the tone vocabulary with toasts).
//! * [`modal`] — dimmed-backdrop dialog helper that wraps a base element and
//!   a content card in an [`iced::widget::Stack`].

pub mod banner;
pub mod menu;
pub mod modal;
pub mod popover;
pub mod toast;
