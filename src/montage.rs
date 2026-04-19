use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use chrono::Utc;

use crate::background_jobs::BackgroundJobContext;
use crate::command_runner;
use crate::db::{ClipAudioTrackRecord, PostProcessStatus};

#[derive(Debug, Clone, PartialEq)]
pub struct MontageClip {
    pub clip_id: i64,
    pub path: PathBuf,
    pub post_process_status: PostProcessStatus,
    pub audio_tracks: Vec<ClipAudioTrackRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MontageResult {
    pub output_path: PathBuf,
    pub source_clip_ids: Vec<i64>,
    pub normalized_clip_count: usize,
}

pub async fn create_concat_montage(
    ctx: BackgroundJobContext,
    save_directory: PathBuf,
    clips: Vec<MontageClip>,
) -> Result<MontageResult, String> {
    tokio::task::spawn_blocking(move || {
        create_concat_montage_blocking(ctx, &save_directory, &clips)
    })
    .await
    .map_err(|error| format!("failed to join montage worker: {error}"))?
}

fn create_concat_montage_blocking(
    ctx: BackgroundJobContext,
    save_directory: &Path,
    clips: &[MontageClip],
) -> Result<MontageResult, String> {
    validate_concat_inputs(clips)?;
    ctx.progress(1, 4, "Validated montage inputs.")?;

    std::fs::create_dir_all(save_directory)
        .map_err(|error| format!("failed to prepare {}: {error}", save_directory.display()))?;

    let normalized = prepare_concat_inputs(&ctx, save_directory, clips)?;
    validate_concat_inputs(&normalized.inputs)?;

    let extension = clips[0]
        .path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("mkv");
    let output_path = unique_output_path(
        save_directory,
        &format!("montage-{}", Utc::now().format("%Y%m%d-%H%M%S")),
        extension,
    );
    let list_path = output_path.with_extension(format!("{extension}.concat.txt"));
    let mut file = std::fs::File::create(&list_path).map_err(|error| {
        format!(
            "failed to create concat list {}: {error}",
            list_path.display()
        )
    })?;
    for clip in &normalized.inputs {
        writeln!(file, "file '{}'", escape_concat_path(&clip.path))
            .map_err(|error| format!("failed to write concat list: {error}"))?;
    }
    ctx.progress(3, 4, "Launching ffmpeg concat job.")?;

    let mut command = Command::new("ffmpeg");
    command
        .arg("-y")
        .arg("-f")
        .arg("concat")
        .arg("-safe")
        .arg("0")
        .arg("-i")
        .arg(&list_path)
        .arg("-c")
        .arg("copy")
        .arg(&output_path);
    let status = command_runner::status(&mut command)
        .map_err(|error| format!("failed to start ffmpeg: {error}"))?;

    let _ = std::fs::remove_file(&list_path);
    if !status.success() {
        return Err(format!("ffmpeg concat exited with status {status}"));
    }

    ctx.progress(4, 4, "Montage completed.")?;
    Ok(MontageResult {
        output_path,
        source_clip_ids: clips.iter().map(|clip| clip.clip_id).collect(),
        normalized_clip_count: normalized.normalized_count,
    })
}

pub fn validate_concat_inputs(clips: &[MontageClip]) -> Result<(), String> {
    if clips.len() < 2 {
        return Err("Choose at least two clips for a montage.".into());
    }

    let first_path = &clips[0].path;
    if !first_path.exists() {
        return Err(format!("Clip does not exist: {}", first_path.display()));
    }

    let first_extension = first_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let first_probe = probe_media_signature(first_path)?;

    for clip in &clips[1..] {
        if !clip.path.exists() {
            return Err(format!("Clip does not exist: {}", clip.path.display()));
        }

        let extension = clip
            .path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if extension != first_extension {
            return Err(format!(
                "Clip {} uses .{} but the montage started with .{}.",
                clip.path.display(),
                extension,
                first_extension
            ));
        }

        let probe = probe_media_signature(&clip.path)?;
        if probe != first_probe {
            return Err(format!(
                "Clip {} is not stream-copy compatible with the other montage inputs.",
                clip.path.display()
            ));
        }
    }

    Ok(())
}

struct PreparedConcatInputs {
    inputs: Vec<MontageClip>,
    _temps: TempPathsGuard,
    normalized_count: usize,
}

#[derive(Default)]
struct TempPathsGuard {
    paths: Vec<PathBuf>,
}

impl Drop for TempPathsGuard {
    fn drop(&mut self) {
        for path in self.paths.drain(..) {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AudioStreamSignature {
    index: i32,
    codec: String,
    channels: u32,
}

fn prepare_concat_inputs(
    ctx: &BackgroundJobContext,
    save_directory: &Path,
    clips: &[MontageClip],
) -> Result<PreparedConcatInputs, String> {
    let signatures = clips
        .iter()
        .map(|clip| probe_audio_streams(&clip.path))
        .collect::<Result<Vec<_>, _>>()?;
    let mix_presence = clips
        .iter()
        .map(|clip| {
            clip.audio_tracks
                .iter()
                .find(|track| track.role == "mixed")
                .map(|track| track.stream_index)
        })
        .collect::<Vec<_>>();

    let needs_normalization = signatures.windows(2).any(|pair| pair[0] != pair[1])
        || mix_presence.windows(2).any(|pair| pair[0] != pair[1]);
    if !needs_normalization {
        return Ok(PreparedConcatInputs {
            inputs: clips.to_vec(),
            _temps: TempPathsGuard::default(),
            normalized_count: 0,
        });
    }

    ctx.progress(
        2,
        4,
        format!(
            "Normalizing {} clip(s) to a single mix track before concat.",
            clips.len()
        ),
    )?;

    let selections = clips
        .iter()
        .zip(signatures.iter())
        .map(|(clip, streams)| select_montage_audio_stream(clip, streams))
        .collect::<Result<Vec<_>, _>>()?;
    let dominant_codec = dominant_audio_codec(&selections);

    let mut temps = TempPathsGuard::default();
    let extension = clips[0]
        .path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("mkv");
    let mut normalized_inputs = Vec::with_capacity(clips.len());
    for (clip, selection) in clips.iter().zip(selections.iter()) {
        let temp_path = unique_output_path(
            save_directory,
            &format!("montage-normalized-{}", clip.clip_id),
            extension,
        );
        remux_normalized_clip(
            &clip.path,
            &temp_path,
            selection.stream_index,
            dominant_codec.as_deref(),
            selection.codec.as_str(),
        )?;
        temps.paths.push(temp_path.clone());
        normalized_inputs.push(MontageClip {
            clip_id: clip.clip_id,
            path: temp_path,
            post_process_status: clip.post_process_status,
            audio_tracks: vec![ClipAudioTrackRecord {
                id: 0,
                clip_id: clip.clip_id,
                stream_index: 0,
                role: "mixed".into(),
                label: "Mixed".into(),
                gain_db: 0.0,
                muted: false,
                source_kind: "normalized".into(),
                source_value: selection.codec.clone(),
            }],
        });
    }

    Ok(PreparedConcatInputs {
        inputs: normalized_inputs,
        _temps: temps,
        normalized_count: clips.len(),
    })
}

struct MontageAudioSelection {
    stream_index: i32,
    codec: String,
}

fn select_montage_audio_stream(
    clip: &MontageClip,
    streams: &[AudioStreamSignature],
) -> Result<MontageAudioSelection, String> {
    let preferred_index = if clip.post_process_status == PostProcessStatus::Completed {
        clip.audio_tracks
            .iter()
            .find(|track| track.role == "mixed")
            .map(|track| track.stream_index)
            .unwrap_or(0)
    } else {
        0
    };
    let stream = streams
        .iter()
        .find(|stream| stream.index == preferred_index)
        .or_else(|| streams.iter().find(|stream| stream.index == 0))
        .ok_or_else(|| {
            format!(
                "clip {} does not contain an audio stream for montage.",
                clip.path.display()
            )
        })?;
    Ok(MontageAudioSelection {
        stream_index: stream.index,
        codec: stream.codec.clone(),
    })
}

fn dominant_audio_codec(selections: &[MontageAudioSelection]) -> Option<String> {
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for selection in selections {
        *counts.entry(selection.codec.clone()).or_default() += 1;
    }
    let max_count = counts.values().copied().max()?;
    if counts.get("aac").copied() == Some(max_count) {
        return Some("aac".into());
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count == max_count)
        .map(|(codec, _)| codec)
        .next()
}

fn remux_normalized_clip(
    input: &Path,
    output: &Path,
    audio_stream_index: i32,
    dominant_codec: Option<&str>,
    clip_codec: &str,
) -> Result<(), String> {
    let mut command = Command::new("ffmpeg");
    command
        .arg("-y")
        .arg("-i")
        .arg(input)
        .arg("-map")
        .arg("0:v")
        .arg("-map")
        .arg(format!("0:a:{audio_stream_index}"))
        .arg("-c:v")
        .arg("copy");

    if dominant_codec.is_some_and(|codec| codec != clip_codec) {
        let target_codec = dominant_codec.unwrap_or("aac");
        command.arg("-c:a").arg(target_codec);
        if matches!(target_codec, "aac" | "libopus" | "opus") {
            command.arg("-b:a").arg("192k");
        }
    } else {
        command.arg("-c:a").arg("copy");
    }

    command.arg(output);
    let output_result = command_runner::output(&mut command)
        .map_err(|error| format!("failed to start ffmpeg montage normalization: {error}"))?;
    if !output_result.status.success() {
        return Err(format!(
            "ffmpeg montage normalization failed for {}: {}",
            input.display(),
            String::from_utf8_lossy(&output_result.stderr).trim()
        ));
    }

    Ok(())
}

fn probe_media_signature(path: &Path) -> Result<String, String> {
    let mut command = Command::new("ffprobe");
    command
        .arg("-v")
        .arg("error")
        .arg("-show_entries")
        .arg("format=format_name:stream=index,codec_type,codec_name,codec_tag_string,width,height,pix_fmt,sample_rate,channel_layout")
        .arg("-of")
        .arg("default=noprint_wrappers=1")
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = command_runner::output(&mut command)
        .map_err(|error| format!("failed to run ffprobe for {}: {error}", path.display()))?;

    if !output.status.success() {
        return Err(format!(
            "ffprobe rejected {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn probe_audio_streams(path: &Path) -> Result<Vec<AudioStreamSignature>, String> {
    let mut command = Command::new("ffprobe");
    command
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("a")
        .arg("-show_entries")
        .arg("stream=index,codec_name,channels")
        .arg("-of")
        .arg("csv=p=0")
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = command_runner::output(&mut command)
        .map_err(|error| format!("failed to run ffprobe for {}: {error}", path.display()))?;

    if !output.status.success() {
        return Err(format!(
            "ffprobe rejected {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut parts = line.split(',');
            let index = parts
                .next()
                .ok_or_else(|| format!("missing audio stream index in ffprobe output: {line}"))?
                .parse::<i32>()
                .map_err(|error| format!("invalid audio stream index `{line}`: {error}"))?;
            let codec = parts
                .next()
                .ok_or_else(|| format!("missing audio codec in ffprobe output: {line}"))?
                .to_string();
            let channels = parts.next().unwrap_or("0").parse::<u32>().unwrap_or(0);
            Ok(AudioStreamSignature {
                index,
                codec,
                channels,
            })
        })
        .collect()
}

fn unique_output_path(directory: &Path, stem: &str, extension: &str) -> PathBuf {
    let initial = directory.join(format!("{stem}.{extension}"));
    if !initial.exists() {
        return initial;
    }

    for index in 2..10_000 {
        let candidate = directory.join(format!("{stem}-{index}.{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }

    directory.join(format!(
        "{stem}-{}.{}",
        Utc::now().timestamp_millis(),
        extension
    ))
}

fn escape_concat_path(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "'\\''")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_single_clip_montage() {
        let error = validate_concat_inputs(&[MontageClip {
            clip_id: 1,
            path: PathBuf::from("/tmp/example.mkv"),
            post_process_status: PostProcessStatus::Legacy,
            audio_tracks: Vec::new(),
        }])
        .unwrap_err();
        assert!(error.contains("at least two clips"));
    }

    #[test]
    fn output_path_adds_suffix_on_collision() {
        let temp_dir = std::env::temp_dir().join(format!(
            "nanite-clip-montage-test-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let initial = temp_dir.join("montage.mkv");
        std::fs::write(&initial, b"").unwrap();

        let candidate = unique_output_path(&temp_dir, "montage", "mkv");
        assert_ne!(candidate, initial);

        let _ = std::fs::remove_file(initial);
        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
