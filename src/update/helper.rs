use std::path::PathBuf;
use std::process::Command;

use super::download;
use super::helper_shared::{ApplyKind, ApplyPlan};
use super::types::{InstallChannel, PreparedUpdate, UpdateAssetKind};

pub fn spawn_apply_helper(prepared: &PreparedUpdate) -> Result<(), String> {
    let current_exe = std::env::current_exe()
        .map_err(|error| format!("failed to locate the running executable: {error}"))?;
    let helper_path = helper_binary_path(&current_exe, prepared.install_channel)?;
    let install_dir = current_exe
        .parent()
        .ok_or_else(|| "failed to locate the current install directory".to_string())?
        .to_path_buf();
    let plan = ApplyPlan {
        plan_version: ApplyPlan::VERSION,
        wait_pid: std::process::id(),
        kind: apply_kind(prepared.install_channel, prepared.asset_kind)?,
        staged_asset: prepared.asset_path.clone(),
        target_executable: current_exe,
        install_dir,
        relaunch_args: Vec::new(),
    };
    let plan_path = write_apply_plan(&plan)?;

    Command::new(&helper_path)
        .arg("--apply-plan")
        .arg(&plan_path)
        .spawn()
        .map(|_| ())
        .map_err(|error| {
            format!(
                "failed to launch the updater helper {}: {error}",
                helper_path.display()
            )
        })
}

fn apply_kind(channel: InstallChannel, asset_kind: UpdateAssetKind) -> Result<ApplyKind, String> {
    match (channel, asset_kind) {
        (InstallChannel::WindowsMsi, UpdateAssetKind::Msi) => Ok(ApplyKind::WindowsMsi),
        (InstallChannel::WindowsPortable, UpdateAssetKind::Exe) => {
            Ok(ApplyKind::WindowsPortableExe)
        }
        (InstallChannel::LinuxPortable, UpdateAssetKind::TarGz) => {
            Ok(ApplyKind::LinuxPortableTarGz)
        }
        _ => Err(format!(
            "install channel {} cannot apply asset kind {} automatically",
            channel.label(),
            asset_kind.label()
        )),
    }
}

fn helper_binary_path(
    current_exe: &std::path::Path,
    install_channel: InstallChannel,
) -> Result<PathBuf, String> {
    let file_name = if cfg!(target_os = "windows") {
        "nanite-clip-updater.exe"
    } else {
        "nanite-clip-updater"
    };
    let helper_path = current_exe
        .parent()
        .ok_or_else(|| "failed to find the executable directory".to_string())?
        .join(file_name);
    if helper_path.exists() {
        Ok(helper_path)
    } else if matches!(install_channel, InstallChannel::LinuxPortable) {
        Ok(current_exe.to_path_buf())
    } else if matches!(install_channel, InstallChannel::WindowsPortable) {
        let temp_helper_dir = download::staging_root().join("apply");
        std::fs::create_dir_all(&temp_helper_dir)
            .map_err(|error| format!("failed to create the helper staging directory: {error}"))?;
        let temp_helper_path = temp_helper_dir.join(file_name);
        std::fs::copy(current_exe, &temp_helper_path).map_err(|error| {
            format!(
                "failed to stage a temporary updater helper at {}: {error}",
                temp_helper_path.display()
            )
        })?;
        Ok(temp_helper_path)
    } else {
        Err(format!(
            "the updater helper is missing from this installation: {}",
            helper_path.display()
        ))
    }
}

fn write_apply_plan(plan: &ApplyPlan) -> Result<PathBuf, String> {
    let plan_dir = download::staging_root().join("apply");
    std::fs::create_dir_all(&plan_dir)
        .map_err(|error| format!("failed to create the updater plan directory: {error}"))?;
    let plan_path = plan_dir.join("apply-plan.json");
    let bytes = serde_json::to_vec_pretty(plan)
        .map_err(|error| format!("failed to serialize the updater apply plan: {error}"))?;
    std::fs::write(&plan_path, bytes)
        .map_err(|error| format!("failed to write the updater apply plan: {error}"))?;
    Ok(plan_path)
}
