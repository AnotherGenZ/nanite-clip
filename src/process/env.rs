use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayServer {
    X11,
    Wayland,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopEnvironment {
    KdePlasma,
    Unknown,
}

pub fn detect_display_server() -> DisplayServer {
    display_server_from_env(
        env::var_os("WAYLAND_DISPLAY")
            .as_deref()
            .and_then(|value| value.to_str()),
        env::var("XDG_SESSION_TYPE").ok().as_deref(),
    )
}

pub fn detect_desktop_environment() -> DesktopEnvironment {
    desktop_environment_from_env(
        env::var("XDG_CURRENT_DESKTOP").ok().as_deref(),
        env::var("DESKTOP_SESSION").ok().as_deref(),
        env::var_os("KDE_FULL_SESSION")
            .as_deref()
            .and_then(|value| value.to_str()),
    )
}

pub(crate) fn display_server_from_env(
    wayland_display: Option<&str>,
    xdg_session_type: Option<&str>,
) -> DisplayServer {
    if wayland_display.is_some_and(|value| !value.trim().is_empty()) {
        return DisplayServer::Wayland;
    }

    if xdg_session_type.is_some_and(|value| value.eq_ignore_ascii_case("wayland")) {
        return DisplayServer::Wayland;
    }

    if xdg_session_type.is_some_and(|value| value.eq_ignore_ascii_case("x11")) {
        return DisplayServer::X11;
    }

    DisplayServer::Unknown
}

pub(crate) fn desktop_environment_from_env(
    current_desktop: Option<&str>,
    desktop_session: Option<&str>,
    kde_full_session: Option<&str>,
) -> DesktopEnvironment {
    let plasma_like = current_desktop
        .into_iter()
        .chain(desktop_session)
        .any(|value| {
            value.split(':').any(|segment| {
                segment.eq_ignore_ascii_case("kde") || segment.eq_ignore_ascii_case("plasma")
            })
        })
        || kde_full_session.is_some_and(|value| {
            value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes")
        });

    if plasma_like {
        DesktopEnvironment::KdePlasma
    } else {
        DesktopEnvironment::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DesktopEnvironment, DisplayServer, desktop_environment_from_env, display_server_from_env,
    };

    #[test]
    fn detects_wayland_from_environment() {
        assert_eq!(
            display_server_from_env(Some("wayland-0"), Some("x11")),
            DisplayServer::Wayland
        );
        assert_eq!(
            display_server_from_env(None, Some("wayland")),
            DisplayServer::Wayland
        );
    }

    #[test]
    fn detects_x11_from_environment() {
        assert_eq!(
            display_server_from_env(None, Some("x11")),
            DisplayServer::X11
        );
    }

    #[test]
    fn detects_kde_plasma_desktop_environment() {
        assert_eq!(
            desktop_environment_from_env(Some("KDE"), None, None),
            DesktopEnvironment::KdePlasma
        );
        assert_eq!(
            desktop_environment_from_env(None, Some("plasma"), None),
            DesktopEnvironment::KdePlasma
        );
        assert_eq!(
            desktop_environment_from_env(None, None, Some("true")),
            DesktopEnvironment::KdePlasma
        );
    }
}
