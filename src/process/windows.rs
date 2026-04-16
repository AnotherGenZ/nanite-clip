use std::mem::size_of;

use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};

use super::{
    BackendHints, CaptureSourcePlan, CaptureTarget, CaptureTargetError, GameProcessWatcher,
};

#[derive(Debug, Default)]
pub struct WindowsToolhelpWatcher;

impl WindowsToolhelpWatcher {
    pub fn new() -> Self {
        Self
    }
}

impl GameProcessWatcher for WindowsToolhelpWatcher {
    fn find_running_pid(&self) -> Option<u32> {
        find_ps2_pid()
    }

    fn is_running(&self, pid: u32) -> bool {
        is_process_running(pid)
    }

    fn resolve_capture_target(
        &self,
        _pid: u32,
        configured_source: &str,
    ) -> Result<CaptureSourcePlan, CaptureTargetError> {
        Ok(resolve_capture_source(configured_source))
    }
}

pub fn find_ps2_pid() -> Option<u32> {
    iter_processes()
        .ok()?
        .into_iter()
        .find(|process| exe_name_matches_ps2(process.exe_name.as_str()))
        .map(|process| process.pid)
}

pub fn is_process_running(pid: u32) -> bool {
    iter_processes()
        .map(|processes| processes.into_iter().any(|process| process.pid == pid))
        .unwrap_or(false)
}

pub fn resolve_capture_source(configured_source: &str) -> CaptureSourcePlan {
    let configured_source = configured_source.trim();
    let target = if uses_backend_owned_capture(configured_source) {
        CaptureTarget::BackendOwned
    } else {
        CaptureTarget::Monitor(configured_source.to_string())
    };

    CaptureSourcePlan {
        target,
        backend_hints: BackendHints::default(),
    }
}

#[derive(Debug)]
struct ProcessInfo {
    pid: u32,
    exe_name: String,
}

#[derive(Debug)]
struct Snapshot(windows::Win32::Foundation::HANDLE);

impl Snapshot {
    fn processes() -> Result<Self, String> {
        // SAFETY: Toolhelp32 snapshot creation does not require additional invariants.
        unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
            .map(Self)
            .map_err(|error| format!("failed to snapshot processes: {error}"))
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        // SAFETY: The snapshot handle came from CreateToolhelp32Snapshot and is owned here.
        let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.0) };
    }
}

fn iter_processes() -> Result<Vec<ProcessInfo>, String> {
    let snapshot = Snapshot::processes()?;
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut processes = Vec::new();

    // SAFETY: `entry` points to writable memory with the documented `dwSize`.
    match unsafe { Process32FirstW(snapshot.0, &mut entry) } {
        Ok(()) => loop {
            processes.push(ProcessInfo {
                pid: entry.th32ProcessID,
                exe_name: process_entry_exe_name(&entry),
            });
            // SAFETY: Same invariants as Process32FirstW; `entry` remains valid for reuse.
            if unsafe { Process32NextW(snapshot.0, &mut entry) }.is_err() {
                break;
            }
        },
        Err(error) => {
            return Err(format!("failed to read process snapshot: {error}"));
        }
    }

    Ok(processes)
}

fn process_entry_exe_name(entry: &PROCESSENTRY32W) -> String {
    let length = entry
        .szExeFile
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(entry.szExeFile.len());
    String::from_utf16_lossy(&entry.szExeFile[..length])
}

fn exe_name_matches_ps2(exe_name: &str) -> bool {
    let file = exe_name
        .rsplit(|character| ['/', '\\'].contains(&character))
        .next()
        .unwrap_or(exe_name)
        .to_ascii_lowercase();
    file.starts_with("planetside2_x64") && file.ends_with(".exe")
}

fn uses_backend_owned_capture(configured_source: &str) -> bool {
    configured_source.is_empty()
        || configured_source.eq_ignore_ascii_case("screen")
        || configured_source.eq_ignore_ascii_case("planetside2")
        || configured_source.eq_ignore_ascii_case("ps2")
        || configured_source.eq_ignore_ascii_case("auto")
        || configured_source.eq_ignore_ascii_case("portal")
}

#[cfg(test)]
mod tests {
    use super::{CaptureTarget, exe_name_matches_ps2, resolve_capture_source};

    #[test]
    fn matches_known_ps2_exe_names() {
        assert!(exe_name_matches_ps2("PlanetSide2_x64.exe"));
        assert!(exe_name_matches_ps2("PlanetSide2_x64_BE.exe"));
        assert!(exe_name_matches_ps2("C:\\Games\\PlanetSide2_x64_Steam.exe"));
    }

    #[test]
    fn rejects_other_executables() {
        assert!(!exe_name_matches_ps2("notepad.exe"));
        assert!(!exe_name_matches_ps2("PlanetSide.exe"));
    }

    #[test]
    fn automatic_capture_is_backend_owned() {
        let plan = resolve_capture_source("auto");
        assert_eq!(plan.target, CaptureTarget::BackendOwned);
    }

    #[test]
    fn explicit_source_is_preserved_as_monitor_hint() {
        let plan = resolve_capture_source("Display 1");
        assert_eq!(plan.target, CaptureTarget::Monitor("Display 1".into()));
    }
}
