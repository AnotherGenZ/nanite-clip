mod pipeline;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::command_runner;
use crate::config::{
    AudioSourceConfig, PostProcessAudioCodec, PostProcessingConfig, PremixDurationMode,
    PremixNormalization, PremixPlacement,
};
pub(crate) use pipeline::*;

pub use pipeline::{
    delete_saved_metadata, probe_audio_streams_blocking, probe_ffmpeg_capabilities,
    read_saved_metadata, run_blocking, write_saved_metadata,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrimSpec {
    pub tail_secs: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbedAudioStream {
    pub index: usize,
    pub codec: String,
    pub channels: u32,
    pub sample_rate: u32,
    pub title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PostProcessRequest {
    pub input: PathBuf,
    pub output: PathBuf,
    pub trim: Option<TrimSpec>,
    pub audio_layout: Vec<AudioSourceConfig>,
    pub post_processing: PostProcessingConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PostProcessResult {
    Unchanged {
        tracks: Vec<OutputAudioTrack>,
    },
    Rewritten {
        output: PathBuf,
        plan: PostProcessPlan,
        tracks: Vec<OutputAudioTrack>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PostProcessPlan {
    pub trimmed: bool,
    pub premix_stream_index: Option<usize>,
    pub preserved_stream_count: usize,
    pub codec_used: PostProcessAudioCodec,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutputAudioTrack {
    pub stream_index: i32,
    pub role: String,
    pub label: String,
    pub gain_db: f32,
    pub muted: bool,
    pub source_kind: String,
    pub source_value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FfmpegCapabilities {
    pub present: bool,
    pub version: Option<Version>,
    pub meets_floor: bool,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPostProcessMetadata {
    pub trim: Option<TrimSpec>,
    pub audio_layout: Vec<AudioSourceConfig>,
    pub post_processing: PostProcessingConfig,
}

#[derive(Debug, thiserror::Error)]
pub enum PostProcessError {
    #[error("ffmpeg 4.2 or newer is required for audio post-processing")]
    FfmpegMissing,
    #[error("ffprobe failed: {0}")]
    FfprobeFailed(String),
    #[error(
        "layout mismatch between config and probe: expected {expected} audio streams, found {actual}"
    )]
    #[allow(dead_code)]
    LayoutMismatch { expected: usize, actual: usize },
    #[error("failed to build ffmpeg filter graph: {0}")]
    FilterGraphBuild(String),
    #[error("ffmpeg exited with {status}: {stderr}")]
    FfmpegExit { status: ExitStatus, stderr: String },
    #[error("verification failed: {0}")]
    VerificationFailed(String),
    #[error("I/O error: {0}")]
    Io(String),
}

pub async fn run(
    request: PostProcessRequest,
    ffmpeg: &FfmpegCapabilities,
) -> Result<PostProcessResult, PostProcessError> {
    if !ffmpeg.present || !ffmpeg.meets_floor {
        return Err(PostProcessError::FfmpegMissing);
    }

    let request_clone = request.clone();
    let ffmpeg = ffmpeg.clone();
    tokio::task::spawn_blocking(move || run_blocking(request_clone, &ffmpeg))
        .await
        .map_err(|error| PostProcessError::Io(error.to_string()))?
}
