use std::path::PathBuf;

use chrono::{DateTime, Utc};
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
    pub target_version: String,
    pub staged_asset: PathBuf,
    pub target_executable: PathBuf,
    pub install_dir: PathBuf,
    pub result_path: PathBuf,
    pub log_path: PathBuf,
    #[serde(default)]
    pub relaunch_args: Vec<String>,
}

impl ApplyPlan {
    pub const VERSION: u32 = 2;
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApplyResultStatus {
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyResult {
    pub plan_version: u32,
    pub target_version: String,
    pub status: ApplyResultStatus,
    #[serde(default)]
    pub detail: Option<String>,
    pub log_path: PathBuf,
    pub finished_at: DateTime<Utc>,
}
