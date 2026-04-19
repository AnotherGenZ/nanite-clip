use std::fs;
use std::path::Path;
use std::process::Command;

use super::helper_shared::{ApplyKind, ApplyPlan};

pub fn run_apply_plan(plan_path: &Path) -> Result<(), String> {
    let plan_bytes = fs::read(plan_path).map_err(|error| {
        format!(
            "failed to read updater apply plan {}: {error}",
            plan_path.display()
        )
    })?;
    let plan = serde_json::from_slice::<ApplyPlan>(&plan_bytes)
        .map_err(|error| format!("failed to parse updater apply plan: {error}"))?;
    if plan.plan_version != ApplyPlan::VERSION {
        return Err(format!(
            "unsupported updater apply plan version {}",
            plan.plan_version
        ));
    }

    wait_for_process_exit(plan.wait_pid)?;

    match plan.kind {
        ApplyKind::WindowsMsi => apply_windows_msi(&plan)?,
        ApplyKind::WindowsPortableExe => apply_windows_portable(&plan)?,
        ApplyKind::LinuxPortableTarGz => apply_linux_portable(&plan)?,
    }

    relaunch(&plan.target_executable, &plan.relaunch_args)?;
    Ok(())
}

fn relaunch(executable: &Path, args: &[String]) -> Result<(), String> {
    Command::new(executable)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("failed to relaunch {}: {error}", executable.display()))
}

#[cfg(target_os = "windows")]
fn apply_windows_msi(plan: &ApplyPlan) -> Result<(), String> {
    let status = Command::new("msiexec")
        .args(["/i"])
        .arg(&plan.staged_asset)
        .args(["/passive", "/norestart"])
        .status()
        .map_err(|error| format!("failed to launch msiexec: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("msiexec exited with status {status}"))
    }
}

#[cfg(not(target_os = "windows"))]
fn apply_windows_msi(_plan: &ApplyPlan) -> Result<(), String> {
    Err("Windows MSI updates are not supported on this platform.".into())
}

#[cfg(target_os = "windows")]
fn apply_windows_portable(plan: &ApplyPlan) -> Result<(), String> {
    let replacement_path = plan.target_executable.with_extension("exe.new");
    fs::copy(&plan.staged_asset, &replacement_path).map_err(|error| {
        format!("failed to copy the staged portable update into place: {error}")
    })?;
    if plan.target_executable.exists() {
        fs::remove_file(&plan.target_executable).map_err(|error| {
            format!(
                "failed to remove the old executable {}: {error}",
                plan.target_executable.display()
            )
        })?;
    }
    fs::rename(&replacement_path, &plan.target_executable).map_err(|error| {
        format!(
            "failed to activate the updated executable {}: {error}",
            plan.target_executable.display()
        )
    })
}

#[cfg(not(target_os = "windows"))]
fn apply_windows_portable(_plan: &ApplyPlan) -> Result<(), String> {
    Err("Windows portable updates are not supported on this platform.".into())
}

#[cfg(target_os = "linux")]
fn apply_linux_portable(plan: &ApplyPlan) -> Result<(), String> {
    use std::io::BufReader;

    use flate2::read::GzDecoder;
    use tar::Archive;

    let temp_root = plan.install_dir.join(".update-unpack");
    if temp_root.exists() {
        let _ = fs::remove_dir_all(&temp_root);
    }
    fs::create_dir_all(&temp_root)
        .map_err(|error| format!("failed to create the Linux update staging directory: {error}"))?;

    let file = fs::File::open(&plan.staged_asset)
        .map_err(|error| format!("failed to open the staged Linux update: {error}"))?;
    let mut archive = Archive::new(GzDecoder::new(BufReader::new(file)));
    archive
        .unpack(&temp_root)
        .map_err(|error| format!("failed to extract the Linux update archive: {error}"))?;

    let extracted_root = fs::read_dir(&temp_root)
        .map_err(|error| format!("failed to inspect the extracted Linux update: {error}"))?
        .filter_map(Result::ok)
        .find(|entry| entry.path().is_dir())
        .map(|entry| entry.path())
        .ok_or_else(|| "the Linux update archive did not contain a root directory".to_string())?;

    copy_directory_contents(&extracted_root, &plan.install_dir)?;
    let _ = fs::remove_dir_all(&temp_root);
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn apply_linux_portable(_plan: &ApplyPlan) -> Result<(), String> {
    Err("Linux portable updates are not supported on this platform.".into())
}

#[cfg(target_os = "windows")]
fn wait_for_process_exit(pid: u32) -> Result<(), String> {
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };

    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, false, pid)
            .map_err(|error| format!("failed to open the running app process: {error}"))?;
        let result = WaitForSingleObject(handle, 60_000);
        let _ = CloseHandle(handle);
        if result == WAIT_OBJECT_0 {
            Ok(())
        } else if result == WAIT_TIMEOUT {
            Err(format!("timed out waiting for process {pid} to exit"))
        } else {
            Err(format!(
                "waiting for the running app process failed with code {:?}",
                result
            ))
        }
    }
}

#[cfg(target_os = "linux")]
fn wait_for_process_exit(pid: u32) -> Result<(), String> {
    use std::thread;
    use std::time::{Duration, Instant};

    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        let result = unsafe { libc::kill(pid as i32, 0) };
        if result != 0 {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(250));
    }
    Err(format!("timed out waiting for process {pid} to exit"))
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn wait_for_process_exit(_pid: u32) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "linux")]
fn copy_directory_contents(source: &Path, destination: &Path) -> Result<(), String> {
    for entry in fs::read_dir(source)
        .map_err(|error| format!("failed to enumerate {}: {error}", source.display()))?
    {
        let entry = entry.map_err(|error| format!("failed to read extracted entry: {error}"))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            fs::create_dir_all(&destination_path).map_err(|error| {
                format!(
                    "failed to create extracted directory {}: {error}",
                    destination_path.display()
                )
            })?;
            copy_directory_contents(&source_path, &destination_path)?;
        } else {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    format!(
                        "failed to create extracted parent directory {}: {error}",
                        parent.display()
                    )
                })?;
            }
            fs::copy(&source_path, &destination_path).map_err(|error| {
                format!(
                    "failed to copy {} into {}: {error}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
            let permissions = fs::metadata(&source_path)
                .map_err(|error| {
                    format!(
                        "failed to read {} permissions: {error}",
                        source_path.display()
                    )
                })?
                .permissions();
            fs::set_permissions(&destination_path, permissions).map_err(|error| {
                format!(
                    "failed to preserve permissions on {}: {error}",
                    destination_path.display()
                )
            })?;
        }
    }
    Ok(())
}
