use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::db::SessionSummary;

pub fn export_session_summary_markdown(
    summary: &SessionSummary,
    save_dir: &Path,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(save_dir)
        .map_err(|error| format!("failed to prepare summary export directory: {error}"))?;
    let output_path = save_dir.join(format!(
        "session-summary-{}.md",
        sanitize_filename(summary.session_id.as_str())
    ));
    std::fs::write(&output_path, render_session_summary_markdown(summary))
        .map_err(|error| format!("failed to write session summary markdown: {error}"))?;
    Ok(output_path)
}

pub fn render_session_summary_markdown(summary: &SessionSummary) -> String {
    let mut output = String::new();
    output.push_str("# Session Summary\n\n");
    output.push_str(format!("Generated: {}\n\n", Utc::now().to_rfc3339()).as_str());
    output.push_str(format!("- Session ID: `{}`\n", summary.session_id).as_str());
    output.push_str(format!("- Clips: {}\n", summary.total_clips).as_str());
    output.push_str(
        format!(
            "- Total Duration: {} seconds\n",
            summary.total_duration_secs
        )
        .as_str(),
    );
    output.push_str(format!("- Bases Played: {}\n\n", summary.unique_bases).as_str());

    output.push_str("## Top Clip\n\n");
    if let Some(top_clip) = &summary.top_clip {
        output.push_str(
            format!(
                "- Clip #{}: {} points via `{}` at {}\n\n",
                top_clip.clip_id,
                top_clip.score,
                top_clip.rule_id,
                top_clip.trigger_event_at.to_rfc3339()
            )
            .as_str(),
        );
    } else {
        output.push_str("No clips were saved in this session.\n\n");
    }

    output.push_str("## Rule Breakdown\n\n");
    if summary.rule_breakdown.is_empty() {
        output.push_str("No rule-triggered clips were recorded.\n\n");
    } else {
        for item in &summary.rule_breakdown {
            output.push_str(format!("- {}: {}\n", item.label, item.count).as_str());
        }
        output.push('\n');
    }

    output.push_str("## Base Breakdown\n\n");
    if summary.base_breakdown.is_empty() {
        output.push_str("No facility-linked clips were recorded.\n");
    } else {
        for item in &summary.base_breakdown {
            output.push_str(format!("- {}: {}\n", item.label, item.count).as_str());
        }
    }

    output
}

fn sanitize_filename(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => ch,
            _ => '_',
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{BaseCount, ClipSummaryItem, CountByLabel, SessionSummary};
    use chrono::{TimeZone, Utc};

    #[test]
    fn renders_markdown_summary_sections() {
        let summary = SessionSummary {
            session_id: "42-1700000000000".into(),
            total_clips: 3,
            total_duration_secs: 90,
            unique_bases: 2,
            top_clip: Some(ClipSummaryItem {
                clip_id: 5,
                rule_id: "rule_infantry".into(),
                score: 12,
                trigger_event_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
                clip_duration_secs: 30,
            }),
            rule_breakdown: vec![CountByLabel {
                label: "rule_infantry".into(),
                count: 2,
            }],
            base_breakdown: vec![BaseCount {
                facility_id: Some(1234),
                label: "The Crown".into(),
                count: 2,
            }],
        };

        let markdown = render_session_summary_markdown(&summary);
        assert!(markdown.contains("# Session Summary"));
        assert!(markdown.contains("## Top Clip"));
        assert!(markdown.contains("rule_infantry"));
        assert!(markdown.contains("The Crown"));
    }
}
