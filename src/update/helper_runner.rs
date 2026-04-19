use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

use chrono::Utc;

use super::helper_shared::{ApplyKind, ApplyPlan, ApplyResult, ApplyResultStatus};
use crate::command_runner;

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

    let mut logger = ApplyLogger::new(&plan.log_path)?;
    logger.log(format!(
        "starting updater apply for version {} with {:?}",
        plan.target_version, plan.kind
    ));

    let apply_result = (|| -> Result<(), String> {
        logger.log(format!(
            "waiting for process {} to exit before applying",
            plan.wait_pid
        ));
        wait_for_process_exit(plan.wait_pid)?;
        logger.log("app process exited; beginning apply".to_string());

        match plan.kind {
            ApplyKind::WindowsMsi => apply_windows_msi(&plan)?,
            ApplyKind::WindowsPortableExe => apply_windows_portable(&plan)?,
            ApplyKind::LinuxPortableTarGz => apply_linux_portable(&plan)?,
        }

        logger.log("apply step completed successfully".to_string());
        Ok(())
    })();

    let apply_state = match &apply_result {
        Ok(()) => ApplyResultStatus::Succeeded,
        Err(error) => {
            logger.log(format!("apply failed: {error}"));
            ApplyResultStatus::Failed
        }
    };

    let write_result_error = write_apply_result(
        &plan,
        apply_state,
        apply_result.as_ref().err().cloned(),
        &mut logger,
    )
    .err();
    if let Some(error) = &write_result_error {
        logger.log(format!("failed to record apply result: {error}"));
    }

    match relaunch(&plan.target_executable, &plan.relaunch_args) {
        Ok(()) => {
            logger.log(format!(
                "relaunch requested for {}",
                plan.target_executable.display()
            ));
        }
        Err(error) => {
            logger.log(format!("relaunch failed: {error}"));
            return match (apply_result, write_result_error) {
                (Ok(()), Some(write_error)) => Err(format!("{write_error}; {error}")),
                (Ok(()), None) => Err(error),
                (Err(apply_error), Some(write_error)) => {
                    Err(format!("{apply_error}; {write_error}; {error}"))
                }
                (Err(apply_error), None) => Err(format!("{apply_error}; {error}")),
            };
        }
    }

    match (apply_result, write_result_error) {
        (Ok(()), None) => Ok(()),
        (Ok(()), Some(write_error)) => Err(write_error),
        (Err(apply_error), None) => Err(apply_error),
        (Err(apply_error), Some(write_error)) => Err(format!("{apply_error}; {write_error}")),
    }
}

fn relaunch(executable: &Path, args: &[String]) -> Result<(), String> {
    let mut command = Command::new(executable);
    command.args(args);
    command_runner::spawn(&mut command)
        .map(|_| ())
        .map_err(|error| format!("failed to relaunch {}: {error}", executable.display()))
}

#[cfg(target_os = "windows")]
fn apply_windows_msi(plan: &ApplyPlan) -> Result<(), String> {
    let mut command = Command::new("msiexec");
    command
        .args(["/i"])
        .arg(&plan.staged_asset)
        .args(["/passive", "/norestart"]);
    let status = command_runner::status(&mut command)
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

fn write_apply_result(
    plan: &ApplyPlan,
    status: ApplyResultStatus,
    detail: Option<String>,
    logger: &mut ApplyLogger,
) -> Result<(), String> {
    let result = ApplyResult {
        plan_version: plan.plan_version,
        target_version: plan.target_version.clone(),
        status,
        detail,
        log_path: plan.log_path.clone(),
        finished_at: Utc::now(),
    };
    if let Some(parent) = plan.result_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create updater result directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let bytes = serde_json::to_vec_pretty(&result)
        .map_err(|error| format!("failed to serialize updater apply result: {error}"))?;
    fs::write(&plan.result_path, bytes)
        .map_err(|error| format!("failed to write updater apply result: {error}"))?;
    logger.log(format!(
        "apply result recorded at {} with status {:?}",
        plan.result_path.display(),
        result.status
    ));
    Ok(())
}

struct ApplyLogger {
    file: fs::File,
}

impl ApplyLogger {
    fn new(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create updater log directory {}: {error}",
                    parent.display()
                )
            })?;
        }
        let file = fs::File::create(path)
            .map_err(|error| format!("failed to create updater log {}: {error}", path.display()))?;
        Ok(Self { file })
    }

    fn log(&mut self, message: String) {
        let _ = writeln!(self.file, "[{}] {message}", Utc::now().to_rfc3339());
        let _ = self.file.flush();
    }
}
