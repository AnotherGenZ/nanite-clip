use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::config::{
    AudioSourceConfig, PostProcessAudioCodec, PostProcessingConfig, PremixDurationMode,
    PremixNormalization, PremixPlacement,
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

pub fn sidecar_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("clip");
    path.with_file_name(format!(".{file_name}.post-process.json"))
}

pub fn write_saved_metadata(
    path: &Path,
    metadata: &SavedPostProcessMetadata,
) -> Result<(), PostProcessError> {
    let sidecar = sidecar_path(path);
    let contents = serde_json::to_vec_pretty(metadata)
        .map_err(|error| PostProcessError::Io(error.to_string()))?;
    std::fs::write(sidecar, contents).map_err(|error| PostProcessError::Io(error.to_string()))
}

pub fn read_saved_metadata(path: &Path) -> Result<SavedPostProcessMetadata, PostProcessError> {
    let contents = std::fs::read(sidecar_path(path))
        .map_err(|error| PostProcessError::Io(error.to_string()))?;
    serde_json::from_slice(&contents).map_err(|error| PostProcessError::Io(error.to_string()))
}

pub fn delete_saved_metadata(path: &Path) -> Result<(), PostProcessError> {
    match std::fs::remove_file(sidecar_path(path)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(PostProcessError::Io(error.to_string())),
    }
}

pub fn run_blocking(
    request: PostProcessRequest,
    ffmpeg: &FfmpegCapabilities,
) -> Result<PostProcessResult, PostProcessError> {
    if !ffmpeg.present || !ffmpeg.meets_floor {
        return Err(PostProcessError::FfmpegMissing);
    }

    let probed = probe_audio_streams_blocking(&request.input)?;
    if !needs_post_process(&request, &probed) {
        return Ok(PostProcessResult::Unchanged {
            tracks: derive_output_tracks(&request, &probed, None, false),
        });
    }

    let build = build_execution_plan(&request, &probed)?;
    let temp_path = temporary_post_process_path(&request.output);

    if let Some(first_pass) = &build.loudnorm_first_pass {
        let first = run_ffmpeg(first_pass, None)?;
        let (target_lufs, tp_db, lra) = match request.post_processing.premix.normalization {
            PremixNormalization::LoudnessTarget {
                target_lufs,
                tp_db,
                lra,
            } => (target_lufs, tp_db, lra),
            _ => (-14.0, -1.0, 11.0),
        };
        let measured = parse_loudnorm_json(&first, target_lufs, tp_db, lra)?;
        let second = build
            .second_pass_with_measurement(measured, &temp_path)
            .map_err(PostProcessError::FilterGraphBuild)?;
        run_ffmpeg(&second, Some(&temp_path))?
    } else {
        let args = build
            .final_pass(&temp_path)
            .map_err(PostProcessError::FilterGraphBuild)?;
        run_ffmpeg(&args, Some(&temp_path))?
    };

    verify_playable(&temp_path)?;
    std::fs::rename(&temp_path, &request.output).map_err(|error| {
        let _ = std::fs::remove_file(&temp_path);
        PostProcessError::Io(error.to_string())
    })?;

    Ok(PostProcessResult::Rewritten {
        output: request.output,
        plan: build.plan,
        tracks: build.output_tracks,
    })
}

pub fn needs_post_process(req: &PostProcessRequest, probed: &[ProbedAudioStream]) -> bool {
    req.trim.is_some()
        || (probed.len() >= 2 && req.post_processing.premix.enabled)
        || (!probed.is_empty() && req.post_processing.rewrite_track_titles)
}

#[allow(dead_code)]
pub async fn probe_audio_streams(
    path: PathBuf,
) -> Result<Vec<ProbedAudioStream>, PostProcessError> {
    tokio::task::spawn_blocking(move || probe_audio_streams_blocking(&path))
        .await
        .map_err(|error| PostProcessError::Io(error.to_string()))?
}

pub fn probe_audio_streams_blocking(
    path: &Path,
) -> Result<Vec<ProbedAudioStream>, PostProcessError> {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("a")
        .arg("-show_entries")
        .arg("stream=index,codec_name,channels,sample_rate:stream_tags=title")
        .arg("-of")
        .arg("json")
        .arg(path)
        .output()
        .map_err(|error| PostProcessError::FfprobeFailed(error.to_string()))?;

    if !output.status.success() {
        return Err(PostProcessError::FfprobeFailed(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    #[derive(Deserialize)]
    struct ProbeJson {
        #[serde(default)]
        streams: Vec<ProbeStream>,
    }

    #[derive(Deserialize)]
    struct ProbeStream {
        index: usize,
        #[serde(default)]
        codec_name: String,
        #[serde(default)]
        channels: u32,
        #[serde(default)]
        sample_rate: String,
        #[serde(default)]
        tags: ProbeTags,
    }

    #[derive(Default, Deserialize)]
    struct ProbeTags {
        title: Option<String>,
    }

    let json: ProbeJson = serde_json::from_slice(&output.stdout)
        .map_err(|error| PostProcessError::FfprobeFailed(error.to_string()))?;
    Ok(json
        .streams
        .into_iter()
        .map(|stream| ProbedAudioStream {
            index: stream.index,
            codec: stream.codec_name,
            channels: stream.channels,
            sample_rate: stream.sample_rate.parse().unwrap_or(0),
            title: stream.tags.title,
        })
        .collect())
}

pub fn probe_ffmpeg_capabilities() -> FfmpegCapabilities {
    let output = match Command::new("ffmpeg").arg("-version").output() {
        Ok(output) => output,
        Err(_) => {
            return FfmpegCapabilities {
                present: false,
                version: None,
                meets_floor: false,
                warning: None,
            };
        }
    };

    if !output.status.success() {
        return FfmpegCapabilities {
            present: false,
            version: None,
            meets_floor: false,
            warning: None,
        };
    }

    let first_line = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    let parsed = parse_ffmpeg_version(first_line.as_str());
    FfmpegCapabilities {
        present: true,
        version: parsed.clone(),
        meets_floor: parsed
            .as_ref()
            .is_none_or(|version| *version >= Version::new(4, 2, 0)),
        warning: if parsed.is_none() {
            Some(
                "Unknown ffmpeg version string. Assuming audio post-processing is supported."
                    .into(),
            )
        } else {
            None
        },
    }
}

pub fn parse_ffmpeg_version(line: &str) -> Option<Version> {
    let token = line.split_whitespace().nth(2)?.trim_start_matches('n');
    let cleaned = token
        .trim_start_matches('N')
        .split('-')
        .next()
        .unwrap_or(token);
    let numeric = cleaned
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '.')
        .collect::<String>();
    if numeric.is_empty() {
        return None;
    }
    let parts = numeric.split('.').collect::<Vec<_>>();
    let version = match parts.as_slice() {
        [major, minor] => format!("{major}.{minor}.0"),
        [major] => format!("{major}.0.0"),
        _ => numeric,
    };
    Version::parse(&version).ok()
}

#[derive(Debug, Clone)]
struct ExecutionPlan {
    base_args: Vec<OsString>,
    filter_graph: Option<String>,
    loudnorm_first_pass: Option<Vec<OsString>>,
    output_tracks: Vec<OutputAudioTrack>,
    plan: PostProcessPlan,
}

impl ExecutionPlan {
    fn final_pass(&self, output: &Path) -> Result<Vec<OsString>, String> {
        let mut args = self.base_args.clone();
        if let Some(graph) = &self.filter_graph {
            args.push("-filter_complex".into());
            args.push(graph.clone().into());
        }
        args.push(output.as_os_str().to_os_string());
        Ok(args)
    }

    fn second_pass_with_measurement(
        &self,
        measurement: LoudnormMeasurement,
        output: &Path,
    ) -> Result<Vec<OsString>, String> {
        let mut args = self.base_args.clone();
        if let Some(graph) = &self.filter_graph {
            let measured = graph.replace(
                "__LOUDNORM__",
                &format!(
                    "loudnorm=I={}:TP={}:LRA={}:measured_I={}:measured_TP={}:measured_LRA={}:measured_thresh={}:offset={}:linear=true:print_format=summary",
                    measurement.target_lufs,
                    measurement.tp_db,
                    measurement.lra,
                    measurement.input_i,
                    measurement.input_tp,
                    measurement.input_lra,
                    measurement.input_thresh,
                    measurement.target_offset,
                ),
            );
            args.push("-filter_complex".into());
            args.push(measured.into());
        }
        args.push(output.as_os_str().to_os_string());
        Ok(args)
    }
}

#[derive(Debug, Clone)]
struct LoudnormMeasurement {
    target_lufs: f32,
    tp_db: f32,
    lra: f32,
    input_i: f32,
    input_tp: f32,
    input_lra: f32,
    input_thresh: f32,
    target_offset: f32,
}

#[derive(Debug, Deserialize)]
struct LoudnormProbeJson {
    input_i: String,
    input_tp: String,
    input_lra: String,
    input_thresh: String,
    target_offset: String,
}

fn build_execution_plan(
    request: &PostProcessRequest,
    probed: &[ProbedAudioStream],
) -> Result<ExecutionPlan, PostProcessError> {
    let matched = match_layout(&request.audio_layout, probed);
    let can_premix = request.post_processing.premix.enabled
        && matched
            .iter()
            .any(|entry| !entry.config.muted_in_premix && entry.config.included_in_premix);

    let mut base_args = vec!["-y".into()];
    if let Some(trim) = &request.trim {
        base_args.push("-sseof".into());
        base_args.push(format!("-{}", trim.tail_secs).into());
    }
    base_args.push("-i".into());
    base_args.push(request.input.as_os_str().to_os_string());

    let mut output_tracks = Vec::new();
    let mut preserved_count = 0usize;
    let mut premix_index = None;

    let filter_graph = if can_premix && matched.len() > 1 {
        let unmuted = matched
            .iter()
            .filter(|entry| !entry.config.muted_in_premix && entry.config.included_in_premix)
            .collect::<Vec<_>>();
        let mut legs = Vec::new();
        for (leg_index, entry) in unmuted.iter().enumerate() {
            legs.push(format!(
                "[0:a:{}]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,volume={}dB[a{}]",
                entry.audio_stream_index, entry.config.gain_db, leg_index
            ));
        }
        let inputs = (0..unmuted.len())
            .map(|index| format!("[a{index}]"))
            .collect::<String>();
        let duration = match request.post_processing.premix.duration_mode {
            PremixDurationMode::Longest => "longest",
            PremixDurationMode::First => "first",
            PremixDurationMode::Shortest => "shortest",
        };
        let normalize = match request.post_processing.premix.normalization {
            PremixNormalization::AmixDivide => "1",
            PremixNormalization::SumThenLimit | PremixNormalization::LoudnessTarget { .. } => "0",
        };
        legs.push(format!(
            "{inputs}amix=inputs={}:duration={duration}:normalize={normalize}[mix_raw]",
            unmuted.len()
        ));
        let final_stage = match request.post_processing.premix.normalization {
            PremixNormalization::AmixDivide => "[mix_raw]anull[mixed]".to_string(),
            PremixNormalization::SumThenLimit => format!(
                "[mix_raw]alimiter=limit={}:attack={}:release={}[mixed]",
                request.post_processing.limiter.limit,
                request.post_processing.limiter.attack_ms,
                request.post_processing.limiter.release_ms,
            ),
            PremixNormalization::LoudnessTarget { .. } => "[mix_raw]__LOUDNORM__[mixed]".into(),
        };
        legs.push(final_stage);
        Some(legs.join(";"))
    } else {
        None
    };

    base_args.push("-map".into());
    base_args.push("0:v".into());

    let mix_enabled = filter_graph.is_some();
    if mix_enabled && request.post_processing.premix.placement == PremixPlacement::First {
        base_args.push("-map".into());
        base_args.push("[mixed]".into());
        premix_index = Some(0);
        output_tracks.push(OutputAudioTrack {
            stream_index: 0,
            role: "mixed".into(),
            label: request.post_processing.premix.track_title.clone(),
            gain_db: 0.0,
            muted: false,
            source_kind: "mixed".into(),
            source_value: request.post_processing.premix.track_title.clone(),
        });
    }

    if request.post_processing.preserve_originals && !probed.is_empty() {
        base_args.push("-map".into());
        base_args.push("0:a".into());
        for (index, entry) in matched.iter().enumerate() {
            let stream_index = output_tracks.len() as i32;
            output_tracks.push(OutputAudioTrack {
                stream_index,
                role: "source".into(),
                label: if request.post_processing.rewrite_track_titles {
                    entry.config.label.clone()
                } else {
                    entry
                        .probed
                        .title
                        .clone()
                        .unwrap_or_else(|| entry.config.label.clone())
                },
                gain_db: entry.config.gain_db,
                muted: entry.config.muted_in_premix,
                source_kind: audio_source_kind_name(entry.config),
                source_value: entry.config.kind.config_display_value(),
            });
            preserved_count = index + 1;
        }
    }

    if mix_enabled && request.post_processing.premix.placement == PremixPlacement::Last {
        let stream_index = output_tracks.len() as i32;
        base_args.push("-map".into());
        base_args.push("[mixed]".into());
        premix_index = Some(stream_index as usize);
        output_tracks.push(OutputAudioTrack {
            stream_index,
            role: "mixed".into(),
            label: request.post_processing.premix.track_title.clone(),
            gain_db: 0.0,
            muted: false,
            source_kind: "mixed".into(),
            source_value: request.post_processing.premix.track_title.clone(),
        });
    }

    base_args.push("-c:v".into());
    base_args.push("copy".into());
    if !output_tracks.is_empty() {
        base_args.push("-c:a".into());
        base_args.push("copy".into());
    }
    if let Some(mix_index) = premix_index {
        base_args.push(format!("-c:a:{mix_index}").into());
        base_args.push(codec_name(request.post_processing.codec.clone()).into());
        base_args.push(format!("-b:a:{mix_index}").into());
        base_args.push(format!("{}k", request.post_processing.bitrate_kbps).into());
    }
    for track in &output_tracks {
        base_args.push(format!("-metadata:s:a:{}", track.stream_index).into());
        base_args.push(format!("title={}", track.label).into());
    }
    if output_extension(&request.output) == Some("mp4") {
        base_args.push("-movflags".into());
        base_args.push("+faststart".into());
    }

    let loudnorm_first_pass = match request.post_processing.premix.normalization {
        PremixNormalization::LoudnessTarget { .. } if filter_graph.is_some() => {
            let mut first = vec!["-y".into()];
            if let Some(trim) = &request.trim {
                first.push("-sseof".into());
                first.push(format!("-{}", trim.tail_secs).into());
            }
            first.push("-i".into());
            first.push(request.input.as_os_str().to_os_string());
            first.push("-filter_complex".into());
            first.push(filter_graph.clone().unwrap().into());
            first.push("-map".into());
            first.push("[mixed]".into());
            first.push("-f".into());
            first.push("null".into());
            first.push("-".into());
            Some(first)
        }
        _ => None,
    };

    Ok(ExecutionPlan {
        base_args,
        filter_graph,
        loudnorm_first_pass,
        output_tracks,
        plan: PostProcessPlan {
            trimmed: request.trim.is_some(),
            premix_stream_index: premix_index,
            preserved_stream_count: preserved_count,
            codec_used: request.post_processing.codec.clone(),
        },
    })
}

fn run_ffmpeg(args: &[OsString], output_path: Option<&Path>) -> Result<String, PostProcessError> {
    let output = Command::new("ffmpeg")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| PostProcessError::Io(error.to_string()))?;

    if !output.status.success() {
        if let Some(path) = output_path {
            let _ = std::fs::remove_file(path);
        }
        return Err(PostProcessError::FfmpegExit {
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    Ok(String::from_utf8_lossy(&output.stderr).to_string())
}

fn parse_loudnorm_json(
    stderr: &str,
    target_lufs: f32,
    tp_db: f32,
    lra: f32,
) -> Result<LoudnormMeasurement, PostProcessError> {
    let start = stderr
        .rfind('{')
        .ok_or_else(|| PostProcessError::FilterGraphBuild("missing loudnorm JSON".into()))?;
    let end = stderr[start..]
        .find('}')
        .map(|offset| start + offset + 1)
        .ok_or_else(|| PostProcessError::FilterGraphBuild("unterminated loudnorm JSON".into()))?;
    let json: LoudnormProbeJson = serde_json::from_str(&stderr[start..end])
        .map_err(|error| PostProcessError::FilterGraphBuild(error.to_string()))?;
    Ok(LoudnormMeasurement {
        target_lufs,
        tp_db,
        lra,
        input_i: json.input_i.parse().unwrap_or(target_lufs),
        input_tp: json.input_tp.parse().unwrap_or(tp_db),
        input_lra: json.input_lra.parse().unwrap_or(lra),
        input_thresh: json.input_thresh.parse().unwrap_or(-24.0),
        target_offset: json.target_offset.parse().unwrap_or(0.0),
    })
}

fn verify_playable(path: &Path) -> Result<(), PostProcessError> {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-show_entries")
        .arg("format=duration")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .arg(path)
        .output()
        .map_err(|error| PostProcessError::VerificationFailed(error.to_string()))?;
    if !output.status.success() {
        return Err(PostProcessError::VerificationFailed(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(())
}

fn temporary_post_process_path(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("clip");
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("mkv");
    path.with_file_name(format!(".{stem}.post.{ext}"))
}

fn codec_name(codec: PostProcessAudioCodec) -> &'static str {
    match codec {
        PostProcessAudioCodec::Aac => "aac",
        PostProcessAudioCodec::Opus => "libopus",
    }
}

fn output_extension(path: &Path) -> Option<&str> {
    path.extension().and_then(|value| value.to_str())
}

fn audio_source_kind_name(config: &AudioSourceConfig) -> String {
    match &config.kind {
        crate::config::AudioSourceKind::DefaultOutput => "default_output".into(),
        crate::config::AudioSourceKind::DefaultInput => "default_input".into(),
        crate::config::AudioSourceKind::Device { .. } => "device".into(),
        crate::config::AudioSourceKind::Application { .. } => "application".into(),
        crate::config::AudioSourceKind::ApplicationInverse { .. } => "application_inverse".into(),
        crate::config::AudioSourceKind::Merged { .. } => "merged".into(),
        crate::config::AudioSourceKind::Raw { .. } => "raw".into(),
    }
}

struct MatchedLayout<'a> {
    audio_stream_index: usize,
    config: &'a AudioSourceConfig,
    probed: &'a ProbedAudioStream,
}

fn match_layout<'a>(
    layout: &'a [AudioSourceConfig],
    probed: &'a [ProbedAudioStream],
) -> Vec<MatchedLayout<'a>> {
    if layout.len() == probed.len() {
        layout
            .iter()
            .zip(probed.iter().enumerate())
            .map(|(config, (audio_stream_index, probed))| MatchedLayout {
                audio_stream_index,
                config,
                probed,
            })
            .collect()
    } else {
        probed
            .iter()
            .enumerate()
            .filter_map(|(index, probed)| {
                layout.get(index).map(|config| MatchedLayout {
                    audio_stream_index: index,
                    config,
                    probed,
                })
            })
            .collect()
    }
}

fn derive_output_tracks(
    request: &PostProcessRequest,
    probed: &[ProbedAudioStream],
    premix_index: Option<usize>,
    preserve_originals: bool,
) -> Vec<OutputAudioTrack> {
    let matched = match_layout(&request.audio_layout, probed);
    let mut tracks = Vec::new();
    if let Some(index) = premix_index {
        tracks.push(OutputAudioTrack {
            stream_index: index as i32,
            role: "mixed".into(),
            label: request.post_processing.premix.track_title.clone(),
            gain_db: 0.0,
            muted: false,
            source_kind: "mixed".into(),
            source_value: request.post_processing.premix.track_title.clone(),
        });
    }
    if preserve_originals {
        for entry in matched {
            tracks.push(OutputAudioTrack {
                stream_index: tracks.len() as i32,
                role: "source".into(),
                label: entry.config.label.clone(),
                gain_db: entry.config.gain_db,
                muted: entry.config.muted_in_premix,
                source_kind: audio_source_kind_name(entry.config),
                source_value: entry.config.kind.config_display_value(),
            });
        }
    }
    tracks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AudioSourceKind, LimiterConfig, PremixConfig};

    fn sample_request() -> PostProcessRequest {
        PostProcessRequest {
            input: PathBuf::from("/tmp/in.mkv"),
            output: PathBuf::from("/tmp/out.mkv"),
            trim: Some(TrimSpec { tail_secs: 30 }),
            audio_layout: vec![
                AudioSourceConfig {
                    label: "Game".into(),
                    kind: AudioSourceKind::Application {
                        name: "PlanetSide2".into(),
                    },
                    gain_db: 0.0,
                    muted_in_premix: false,
                    included_in_premix: true,
                },
                AudioSourceConfig {
                    label: "Voice".into(),
                    kind: AudioSourceKind::Application {
                        name: "TeamSpeak".into(),
                    },
                    gain_db: 3.0,
                    muted_in_premix: false,
                    included_in_premix: true,
                },
            ],
            post_processing: PostProcessingConfig {
                premix: PremixConfig::default(),
                preserve_originals: true,
                rewrite_track_titles: true,
                codec: PostProcessAudioCodec::Aac,
                bitrate_kbps: 192,
                limiter: LimiterConfig::default(),
            },
        }
    }

    #[test]
    fn ffmpeg_version_probe_parses_supported_versions() {
        assert_eq!(
            parse_ffmpeg_version("ffmpeg version 6.1.1 Copyright"),
            Some(Version::new(6, 1, 1))
        );
        assert_eq!(
            parse_ffmpeg_version("ffmpeg version 4.2 Copyright"),
            Some(Version::new(4, 2, 0))
        );
    }

    #[test]
    fn needs_post_process_truth_table() {
        let request = sample_request();
        let empty = Vec::<ProbedAudioStream>::new();
        assert!(needs_post_process(&request, &empty));

        let no_trim = PostProcessRequest {
            trim: None,
            ..request.clone()
        };
        assert!(needs_post_process(
            &no_trim,
            &[ProbedAudioStream {
                index: 0,
                codec: "aac".into(),
                channels: 2,
                sample_rate: 48_000,
                title: Some("Game".into()),
            }]
        ));

        let no_titles = PostProcessRequest {
            post_processing: PostProcessingConfig {
                rewrite_track_titles: false,
                premix: PremixConfig {
                    enabled: false,
                    ..PostProcessingConfig::default().premix
                },
                ..PostProcessingConfig::default()
            },
            trim: None,
            audio_layout: vec![],
            ..sample_request()
        };
        assert!(!needs_post_process(
            &no_titles,
            &[ProbedAudioStream {
                index: 0,
                codec: "aac".into(),
                channels: 2,
                sample_rate: 48_000,
                title: Some("Game".into()),
            }]
        ));
    }

    #[test]
    fn build_plan_creates_premix_track_first_by_default() {
        let request = sample_request();
        let probed = vec![
            ProbedAudioStream {
                index: 0,
                codec: "aac".into(),
                channels: 2,
                sample_rate: 48_000,
                title: Some("app:PlanetSide2".into()),
            },
            ProbedAudioStream {
                index: 1,
                codec: "aac".into(),
                channels: 2,
                sample_rate: 48_000,
                title: Some("app:TeamSpeak".into()),
            },
        ];
        let plan = build_execution_plan(&request, &probed).unwrap();
        assert_eq!(plan.plan.premix_stream_index, Some(0));
        assert_eq!(plan.output_tracks[0].role, "mixed");
    }

    #[test]
    fn build_plan_uses_audio_stream_positions_not_absolute_stream_indexes() {
        let request = sample_request();
        let probed = vec![
            ProbedAudioStream {
                index: 1,
                codec: "opus".into(),
                channels: 2,
                sample_rate: 48_000,
                title: Some("app:PlanetSide 2".into()),
            },
            ProbedAudioStream {
                index: 2,
                codec: "opus".into(),
                channels: 2,
                sample_rate: 48_000,
                title: Some("app:TeamSpeak".into()),
            },
        ];

        let plan = build_execution_plan(&request, &probed).unwrap();
        let filter_graph = plan.filter_graph.expect("expected premix filter graph");

        assert!(filter_graph.contains("[0:a:0]"));
        assert!(filter_graph.contains("[0:a:1]"));
        assert!(!filter_graph.contains("[0:a:2]"));
    }
}
