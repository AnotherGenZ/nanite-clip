use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyKind {
    WindowsMsi,
    WindowsPortableExe,
    LinuxPortableTarGz,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyPlan {
    pub plan_version: u32,
    pub wait_pid: u32,
    pub kind: ApplyKind,
    pub staged_asset: PathBuf,
    pub target_executable: PathBuf,
    pub install_dir: PathBuf,
    #[serde(default)]
    pub relaunch_args: Vec<String>,
}

impl ApplyPlan {
    pub const VERSION: u32 = 1;
}
