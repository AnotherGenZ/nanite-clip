use std::path::Path;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt as _, Window};
use x11rb::rust_connection::RustConnection;

use super::{
    BackendHints, CaptureSourcePlan, CaptureTarget, CaptureTargetError, DisplayServer,
    GameProcessWatcher, detect_display_server,
};

#[derive(Debug, Default)]
pub struct LinuxProcfsWatcher;

impl LinuxProcfsWatcher {
    pub fn new() -> Self {
        Self
    }
}

impl GameProcessWatcher for LinuxProcfsWatcher {
    fn find_running_pid(&self) -> Option<u32> {
        find_ps2_pid()
    }

    fn is_running(&self, pid: u32) -> bool {
        is_process_running(pid)
    }

    fn resolve_capture_target(
        &self,
        pid: u32,
        configured_source: &str,
    ) -> Result<CaptureSourcePlan, CaptureTargetError> {
        resolve_capture_source(configured_source, pid)
    }
}

/// Resolve the capture source that should be passed to the active backend.
///
/// The legacy default was `screen`, which records the whole monitor and will
/// capture whatever happens to be visible. Treat that and the explicit
/// `planetside2`/`auto` aliases as "bind to the PS2 window", using a specific
/// X11 window ID on X11 and desktop portal capture on Wayland.
pub fn resolve_capture_source(
    configured_source: &str,
    ps2_pid: u32,
) -> Result<CaptureSourcePlan, CaptureTargetError> {
    let configured_source = configured_source.trim();
    if !uses_ps2_window_capture(configured_source) {
        return Ok(CaptureSourcePlan {
            target: if configured_source.eq_ignore_ascii_case("portal") {
                CaptureTarget::WaylandPortal
            } else {
                CaptureTarget::Monitor(configured_source.to_string())
            },
            backend_hints: BackendHints {
                display_server: Some(detect_display_server()),
                restore_portal_session: configured_source.eq_ignore_ascii_case("portal"),
            },
        });
    }

    match detect_display_server() {
        DisplayServer::Wayland => Ok(CaptureSourcePlan {
            target: CaptureTarget::WaylandPortal,
            backend_hints: BackendHints {
                display_server: Some(DisplayServer::Wayland),
                restore_portal_session: true,
            },
        }),
        DisplayServer::X11 | DisplayServer::Unknown => {
            let window_id = find_ps2_window_id(ps2_pid)?;
            Ok(CaptureSourcePlan {
                target: CaptureTarget::X11Window(window_id),
                backend_hints: BackendHints {
                    display_server: Some(DisplayServer::X11),
                    restore_portal_session: false,
                },
            })
        }
    }
}

/// Scan /proc for a Planetside 2 process.
///
/// PS2 runs under Wine/Proton, so `/proc/PID/exe` points at wine-preloader and
/// `/proc/PID/comm` is truncated to 15 chars ("PlanetSide2_x64"). The cmdline is
/// the only reliable signal: it contains the Windows-style path to the game exe.
/// We match any `PlanetSide2_x64*.exe` to cover variants like the bare exe,
/// the BattlEye wrapper used by Steam (`PlanetSide2_x64_BE.exe`), or
/// build-suffixed names that have appeared across releases.
pub fn find_ps2_pid() -> Option<u32> {
    let proc_dir = std::fs::read_dir("/proc").ok()?;
    for entry in proc_dir.flatten() {
        let name = entry.file_name();
        let Ok(pid) = name.to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(cmdline) = std::fs::read_to_string(entry.path().join("cmdline")) else {
            continue;
        };
        if cmdline_matches_ps2(&cmdline) {
            return Some(pid);
        }
    }
    None
}

#[allow(dead_code)]
pub fn is_process_running(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

fn cmdline_matches_ps2(cmdline: &str) -> bool {
    cmdline.split('\0').any(|arg| {
        let file = arg
            .rsplit(|c| ['/', '\\'].contains(&c))
            .next()
            .unwrap_or(arg)
            .to_ascii_lowercase();
        file.starts_with("planetside2_x64") && file.ends_with(".exe")
    })
}

fn uses_ps2_window_capture(source: &str) -> bool {
    let source = source.trim();
    source.is_empty()
        || source.eq_ignore_ascii_case("screen")
        || source.eq_ignore_ascii_case("planetside2")
        || source.eq_ignore_ascii_case("ps2")
        || source.eq_ignore_ascii_case("auto")
}

fn find_ps2_window_id(ps2_pid: u32) -> Result<u32, CaptureTargetError> {
    let (conn, screen_num) =
        x11rb::connect(None).map_err(|e| CaptureTargetError::X11Connect(e.to_string()))?;
    let root = conn
        .setup()
        .roots
        .get(screen_num)
        .ok_or(CaptureTargetError::NoX11Screen)?
        .root;

    let atoms = Atoms::new(&conn)?;
    let windows = top_level_windows(&conn, root, atoms.net_client_list)?;

    if let Some(window_id) = windows
        .iter()
        .copied()
        .find(|window| window_pid(&conn, *window, atoms.net_wm_pid) == Some(ps2_pid))
    {
        return Ok(window_id);
    }

    if let Some(window_id) = windows.iter().copied().find(|window| {
        window_name(&conn, *window, atoms.net_wm_name, atoms.utf8_string)
            .is_some_and(|name| string_matches_ps2_window(&name))
            || window_name(
                &conn,
                *window,
                AtomEnum::WM_NAME.into(),
                AtomEnum::STRING.into(),
            )
            .is_some_and(|name| string_matches_ps2_window(&name))
            || window_class(&conn, *window).is_some_and(|class| string_matches_ps2_window(&class))
    }) {
        return Ok(window_id);
    }

    Err(CaptureTargetError::WindowNotFound)
}

fn top_level_windows(
    conn: &RustConnection,
    root: Window,
    net_client_list: Atom,
) -> Result<Vec<u32>, CaptureTargetError> {
    if let Ok(reply) = conn
        .get_property(false, root, net_client_list, AtomEnum::WINDOW, 0, u32::MAX)
        .map_err(|e| CaptureTargetError::X11Query(e.to_string()))?
        .reply()
        && let Some(windows) = reply.value32()
    {
        let windows: Vec<u32> = windows.collect();
        if !windows.is_empty() {
            return Ok(windows);
        }
    }

    let reply = conn
        .query_tree(root)
        .map_err(|e| CaptureTargetError::X11Query(e.to_string()))?
        .reply()
        .map_err(|e| CaptureTargetError::X11Query(e.to_string()))?;
    Ok(reply.children)
}

fn window_pid(conn: &RustConnection, window: Window, net_wm_pid: Atom) -> Option<u32> {
    conn.get_property(false, window, net_wm_pid, AtomEnum::CARDINAL, 0, 1)
        .ok()?
        .reply()
        .ok()?
        .value32()?
        .next()
}

fn window_name(
    conn: &RustConnection,
    window: Window,
    property: Atom,
    property_type: Atom,
) -> Option<String> {
    let reply = conn
        .get_property(false, window, property, property_type, 0, u32::MAX)
        .ok()?
        .reply()
        .ok()?;
    if reply.value.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(&reply.value).into_owned())
}

fn window_class(conn: &RustConnection, window: Window) -> Option<String> {
    window_name(
        conn,
        window,
        AtomEnum::WM_CLASS.into(),
        AtomEnum::STRING.into(),
    )
}

fn string_matches_ps2_window(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.contains("planetside 2")
        || value.contains("planetside2")
        || value.contains("planetside2_x64")
}

struct Atoms {
    net_client_list: Atom,
    net_wm_name: Atom,
    net_wm_pid: Atom,
    utf8_string: Atom,
}

impl Atoms {
    fn new(conn: &RustConnection) -> Result<Self, CaptureTargetError> {
        Ok(Self {
            net_client_list: intern_atom(conn, b"_NET_CLIENT_LIST")?,
            net_wm_name: intern_atom(conn, b"_NET_WM_NAME")?,
            net_wm_pid: intern_atom(conn, b"_NET_WM_PID")?,
            utf8_string: intern_atom(conn, b"UTF8_STRING")?,
        })
    }
}

fn intern_atom(conn: &RustConnection, name: &[u8]) -> Result<Atom, CaptureTargetError> {
    conn.intern_atom(false, name)
        .map_err(|e| CaptureTargetError::X11Query(e.to_string()))?
        .reply()
        .map(|reply| reply.atom)
        .map_err(|e| CaptureTargetError::X11Query(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{
        CaptureTarget, cmdline_matches_ps2, resolve_capture_source, string_matches_ps2_window,
        uses_ps2_window_capture,
    };

    #[test]
    fn cmdline_match_accepts_known_ps2_variants() {
        assert!(cmdline_matches_ps2(
            "/tmp/proton\0Z:\\Games\\PlanetSide2_x64.exe\0"
        ));
        assert!(cmdline_matches_ps2(
            "/tmp/proton\0Z:\\Games\\PlanetSide2_x64_BE.exe\0"
        ));
    }

    #[test]
    fn cmdline_match_rejects_other_executables() {
        assert!(!cmdline_matches_ps2(
            "/tmp/proton\0Z:\\Games\\notepad.exe\0"
        ));
    }

    #[test]
    fn ps2_window_capture_aliases_include_legacy_screen_default() {
        assert!(uses_ps2_window_capture(""));
        assert!(uses_ps2_window_capture("screen"));
        assert!(uses_ps2_window_capture("planetside2"));
        assert!(uses_ps2_window_capture("auto"));
        assert!(!uses_ps2_window_capture("DP-1"));
        assert!(!uses_ps2_window_capture("portal"));
    }

    #[test]
    fn ps2_window_match_recognizes_expected_titles() {
        assert!(string_matches_ps2_window("PlanetSide 2"));
        assert!(string_matches_ps2_window("PlanetSide2_x64.exe"));
        assert!(!string_matches_ps2_window("Firefox"));
    }

    #[test]
    fn portal_source_sets_portal_target_and_restore_hint() {
        let plan = resolve_capture_source("portal", 42).unwrap();

        assert_eq!(plan.target, CaptureTarget::WaylandPortal);
        assert!(plan.backend_hints.restore_portal_session);
    }

    #[test]
    fn explicit_monitor_source_maps_to_monitor_target() {
        let plan = resolve_capture_source("DP-1", 42).unwrap();

        assert_eq!(plan.target, CaptureTarget::Monitor("DP-1".into()));
        assert!(!plan.backend_hints.restore_portal_session);
    }
}
