use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{DateTime, Utc};

use crate::command_runner;

const DEFAULT_JPEG_QUALITY: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailFormat {
    Jpeg,
    Png,
}

impl ThumbnailFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
        }
    }
}

pub fn thumbnail_path_for_clip(
    clip_path: &Path,
    clip_id: i64,
    format: ThumbnailFormat,
) -> Option<PathBuf> {
    let parent = clip_path.parent()?;
    Some(
        parent
            .join("thumbnails")
            .join(format!("clip-{clip_id}.{}", format.extension())),
    )
}

pub fn trigger_seek_seconds(
    trigger_event_at: DateTime<Utc>,
    clip_start_at: DateTime<Utc>,
    clip_end_at: DateTime<Utc>,
) -> f64 {
    let clip_duration_ms = (clip_end_at - clip_start_at).num_milliseconds().max(0);
    if clip_duration_ms <= 0 {
        return 0.0;
    }

    let trigger_offset_ms = (trigger_event_at - clip_start_at)
        .num_milliseconds()
        .clamp(0, clip_duration_ms);

    (trigger_offset_ms as f64) / 1_000.0
}

pub fn midpoint_seek_seconds(clip_start_at: DateTime<Utc>, clip_end_at: DateTime<Utc>) -> f64 {
    let clip_duration_ms = (clip_end_at - clip_start_at).num_milliseconds().max(0);
    (clip_duration_ms as f64) / 2_000.0
}

pub fn extract_clip_thumbnail(
    clip_path: &Path,
    output_path: &Path,
    seek_seconds: f64,
    format: ThumbnailFormat,
) -> Result<Option<PathBuf>, String> {
    extract_clip_thumbnail_with_program("ffmpeg", clip_path, output_path, seek_seconds, format)
}

pub fn extract_clip_thumbnail_with_program(
    program: &str,
    clip_path: &Path,
    output_path: &Path,
    seek_seconds: f64,
    format: ThumbnailFormat,
) -> Result<Option<PathBuf>, String> {
    if !command_runner::command_available(program) {
        return Ok(None);
    }

    let Some(parent) = output_path.parent() else {
        return Err(format!(
            "thumbnail destination {} has no parent directory",
            output_path.display()
        ));
    };
    std::fs::create_dir_all(parent).map_err(|error| {
        format!(
            "failed to create thumbnail directory {}: {error}",
            parent.display()
        )
    })?;

    let mut command = Command::new(program);
    command
        .arg("-y")
        .arg("-ss")
        .arg(format!("{seek_seconds:.3}"))
        .arg("-i")
        .arg(clip_path)
        .arg("-frames:v")
        .arg("1");

    if matches!(format, ThumbnailFormat::Jpeg) {
        command.arg("-q:v").arg(DEFAULT_JPEG_QUALITY.to_string());
    }

    command.arg(output_path);

    match command_runner::check_output(&mut command) {
        Ok(_) if output_path.exists() => Ok(Some(output_path.to_path_buf())),
        Ok(_) => Ok(None),
        Err(command_runner::CommandError::Failed { .. }) => Ok(None),
        Err(error) => Err(format!("failed to launch thumbnail extraction: {error}")),
    }
}

pub fn generate_clip_thumbnail(
    clip_id: i64,
    clip_path: &Path,
    trigger_event_at: DateTime<Utc>,
    clip_start_at: DateTime<Utc>,
    clip_end_at: DateTime<Utc>,
) -> Result<Option<PathBuf>, String> {
    let Some(output_path) = thumbnail_path_for_clip(clip_path, clip_id, ThumbnailFormat::Jpeg)
    else {
        return Ok(None);
    };

    let seek_seconds = trigger_seek_seconds(trigger_event_at, clip_start_at, clip_end_at);
    let fallback_seek_seconds = midpoint_seek_seconds(clip_start_at, clip_end_at);

    match extract_clip_thumbnail(clip_path, &output_path, seek_seconds, ThumbnailFormat::Jpeg)? {
        Some(path) => Ok(Some(path)),
        None if (seek_seconds - fallback_seek_seconds).abs() > f64::EPSILON => {
            extract_clip_thumbnail(
                clip_path,
                &output_path,
                fallback_seek_seconds,
                ThumbnailFormat::Jpeg,
            )
        }
        None => Ok(None),
    }
}

pub fn extract_discord_thumbnail(clip_path: &Path) -> Result<Option<PathBuf>, String> {
    let output_path = clip_path.with_extension("discord-thumb.png");
    extract_clip_thumbnail(clip_path, &output_path, 1.0, ThumbnailFormat::Png)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_700_000_000 + secs, 0).unwrap()
    }

    #[test]
    fn thumbnail_path_uses_sibling_thumbnails_directory() {
        let clip_path = Path::new("/clips/session/alert-clip.mkv");
        let path = thumbnail_path_for_clip(clip_path, 42, ThumbnailFormat::Jpeg).unwrap();
        assert_eq!(path, PathBuf::from("/clips/session/thumbnails/clip-42.jpg"));
    }

    #[test]
    fn trigger_seek_clamps_to_clip_bounds() {
        let start = ts(0);
        let end = ts(20);

        assert_eq!(trigger_seek_seconds(ts(-5), start, end), 0.0);
        assert_eq!(trigger_seek_seconds(ts(5), start, end), 5.0);
        assert_eq!(trigger_seek_seconds(ts(25), start, end), 20.0);
    }

    #[test]
    fn midpoint_seek_uses_half_the_clip_duration() {
        assert_eq!(midpoint_seek_seconds(ts(0), ts(12)), 6.0);
    }

    #[test]
    fn missing_ffmpeg_returns_none_instead_of_error() {
        let result = extract_clip_thumbnail_with_program(
            "nanite-clip-test-missing-binary",
            Path::new("/tmp/input.mkv"),
            Path::new("/tmp/output.jpg"),
            1.0,
            ThumbnailFormat::Jpeg,
        )
        .unwrap();

        assert_eq!(result, None);
    }
}
