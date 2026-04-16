use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaceholderSpec {
    pub token: &'static str,
    pub example: &'static str,
    pub description: &'static str,
}

pub const SUPPORTED_PLACEHOLDERS: &[PlaceholderSpec] = &[
    PlaceholderSpec {
        token: "{timestamp}",
        example: "2024-03-09_11-20-00",
        description: "Clip trigger time in local time.",
    },
    PlaceholderSpec {
        token: "{source}",
        example: "manual",
        description: "Clip origin such as manual or rule.",
    },
    PlaceholderSpec {
        token: "{profile}",
        example: "Default",
        description: "Active rule profile ID.",
    },
    PlaceholderSpec {
        token: "{rule}",
        example: "manual_clip",
        description: "Rule ID or manual clip marker.",
    },
    PlaceholderSpec {
        token: "{character}",
        example: "Example",
        description: "Tracked character name.",
    },
    PlaceholderSpec {
        token: "{server}",
        example: "Emerald",
        description: "Resolved world name.",
    },
    PlaceholderSpec {
        token: "{continent}",
        example: "Indar",
        description: "Resolved zone or continent name.",
    },
    PlaceholderSpec {
        token: "{base}",
        example: "The Crown",
        description: "Resolved facility or base name.",
    },
    PlaceholderSpec {
        token: "{score}",
        example: "12",
        description: "Trigger score saved with the clip.",
    },
    PlaceholderSpec {
        token: "{duration}",
        example: "30",
        description: "Clip duration in seconds.",
    },
];

#[derive(Debug, Clone)]
pub struct ClipNamingContext {
    pub timestamp: DateTime<Utc>,
    pub source: String,
    pub profile: String,
    pub rule: String,
    pub character: String,
    pub server: String,
    pub continent: String,
    pub base: String,
    pub score: u32,
    pub duration_secs: u32,
}

pub fn validate_template(template: &str) -> Result<(), String> {
    let _ = render_template(template, &sample_context())?;
    Ok(())
}

pub fn preview_template(template: &str) -> Result<String, String> {
    let rendered = render_template(template, &sample_context())?;
    let sanitized = sanitize_component(&rendered);
    if sanitized.is_empty() {
        Ok("clip".into())
    } else {
        Ok(sanitized)
    }
}

pub fn rename_saved_clip(
    template: &str,
    saved_path: &Path,
    context: &ClipNamingContext,
) -> Result<PathBuf, String> {
    let Some(parent) = saved_path.parent() else {
        return Err(format!(
            "cannot rename clip {} because it has no parent directory",
            saved_path.display()
        ));
    };

    let extension = saved_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    let stem = render_template(template, context)?;
    let sanitized_stem = sanitize_component(&stem);
    let final_stem = if sanitized_stem.is_empty() {
        "clip".to_string()
    } else {
        sanitized_stem
    };

    let mut candidate = parent.join(format!("{final_stem}{extension}"));
    if candidate == saved_path {
        return Ok(candidate);
    }

    let mut collision_index = 2_u32;
    while candidate.exists() {
        candidate = parent.join(format!("{final_stem}-{collision_index}{extension}"));
        collision_index += 1;
    }

    std::fs::rename(saved_path, &candidate).map_err(|error| {
        format!(
            "failed to rename clip from {} to {}: {error}",
            saved_path.display(),
            candidate.display()
        )
    })?;

    Ok(candidate)
}

fn render_template(template: &str, context: &ClipNamingContext) -> Result<String, String> {
    let mut output = String::new();
    let mut chars = template.chars().peekable();

    while let Some(character) = chars.next() {
        if character != '{' {
            output.push(character);
            continue;
        }

        let mut placeholder = String::new();
        while let Some(&next) = chars.peek() {
            chars.next();
            if next == '}' {
                break;
            }
            placeholder.push(next);
        }

        if placeholder.is_empty() {
            return Err("clip naming template contains an empty placeholder".into());
        }

        let value = match placeholder.as_str() {
            "timestamp" => context
                .timestamp
                .with_timezone(&Local)
                .format("%Y-%m-%d_%H-%M-%S")
                .to_string(),
            "source" => context.source.clone(),
            "profile" => context.profile.clone(),
            "rule" => context.rule.clone(),
            "character" => context.character.clone(),
            "server" => context.server.clone(),
            "continent" => context.continent.clone(),
            "base" => context.base.clone(),
            "score" => context.score.to_string(),
            "duration" => context.duration_secs.to_string(),
            other => {
                return Err(format!(
                    "clip naming template uses an unknown placeholder: {{{other}}}"
                ));
            }
        };

        output.push_str(value.as_str());
    }

    Ok(output)
}

fn sanitize_component(value: &str) -> String {
    let mut output = String::new();

    for character in value.chars() {
        let replacement = match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            _ => character,
        };
        output.push(replacement);
    }

    output
        .split_whitespace()
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("_")
        .trim_matches('_')
        .to_string()
}

fn sample_context() -> ClipNamingContext {
    ClipNamingContext {
        timestamp: DateTime::<Utc>::from_timestamp(1_710_000_000, 0).unwrap(),
        source: "manual".into(),
        profile: "Default".into(),
        rule: "Manual Clip".into(),
        character: "Example".into(),
        server: "Emerald".into(),
        continent: "Indar".into(),
        base: "The Crown".into(),
        score: 12,
        duration_secs: 30,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_known_placeholders() {
        assert!(validate_template("{timestamp}_{character}_{rule}").is_ok());
        assert!(validate_template("{unknown}").is_err());
    }

    #[test]
    fn preview_matches_final_filename_sanitization_rules() {
        let preview = preview_template("{character}:{rule}?").unwrap();
        assert_eq!(preview, "Example_Manual_Clip");
    }

    #[test]
    fn sanitizes_invalid_filename_characters() {
        let sanitized = sanitize_component("The Crown: Final/Push?");
        assert_eq!(sanitized, "The_Crown__Final_Push");
    }

    #[test]
    fn renaming_avoids_collisions() {
        let temp_dir =
            std::env::temp_dir().join(format!("nanite-clip-naming-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let original = temp_dir.join("raw.mkv");
        let existing = temp_dir.join("Example_manual.mkv");
        std::fs::write(&original, b"clip").unwrap();
        std::fs::write(&existing, b"existing").unwrap();

        let context = ClipNamingContext {
            character: "Example".into(),
            source: "manual".into(),
            ..sample_context()
        };

        let renamed = rename_saved_clip("{character}_{source}", &original, &context).unwrap();
        assert_eq!(
            renamed.file_name().and_then(|name| name.to_str()),
            Some("Example_manual-2.mkv")
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
