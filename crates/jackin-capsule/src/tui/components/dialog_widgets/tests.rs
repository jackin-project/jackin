#[cfg(test)]
use super::*;
use jackin_tui::components::{ContainerInfoRow, ContainerInfoState};

fn usage_state() -> ContainerInfoState {
    ContainerInfoState::new(
        "Usage",
        vec![
            ContainerInfoRow::new("Header", "OpenAI / Codex"),
            ContainerInfoRow::new("Account", "alexey@example.com"),
            ContainerInfoRow::new("Plan", "Pro 20x"),
            ContainerInfoRow::new("Updated", "Updated 2m ago"),
            ContainerInfoRow::new(
                "Session",
                "███████······· 50% left · 50% used · Resets at 15:00 UTC · On pace",
            ),
            ContainerInfoRow::new(
                "Weekly",
                "███████████··· 80% left · 20% used · Resets on Friday 10:00 UTC · 25% in reserve",
            ),
        ],
    )
}

fn line_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
}

#[test]
fn usage_overlay_lines_fit_responsive_widths() {
    let state = usage_state();
    for width in [44, 64, 96] {
        let lines = usage_info_lines_for_width(&state, width);
        for line in &lines {
            let cols = usage_line_width(line);
            assert!(
                cols <= usize::from(width),
                "line exceeds width {width}: {cols} cols: {:?}",
                line_text(line)
            );
        }
    }
}

#[test]
fn usage_overlay_wide_layout_keeps_header_and_full_width_remaining_meters() {
    let state = usage_state();
    let width = 96;
    let lines = usage_info_lines_for_width(&state, width);
    let text = lines.iter().map(line_text).collect::<Vec<_>>();

    assert!(
        text.iter()
            .any(|line| line.contains("OpenAI / Codex") && line.contains("alexey@example.com")),
        "provider/account header missing: {text:?}"
    );
    assert!(
        text.iter()
            .any(|line| line.contains("Updated 2m ago") && line.contains("Pro 20x")),
        "freshness/plan header missing: {text:?}"
    );

    let meter = text
        .iter()
        .find(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && trimmed.chars().all(|ch| matches!(ch, '█' | '░'))
        })
        .expect("full-width quota meter");
    assert_eq!(
        jackin_tui::display_cols(meter.trim()),
        usage_content_width(usize::from(width)),
        "meter should span the padded dialog body width: {meter:?}"
    );
}

#[test]
fn usage_overlay_narrow_layout_keeps_compact_bucket_details() {
    let state = usage_state();
    let lines = usage_info_lines_for_width(&state, 44);
    let text = lines.iter().map(line_text).collect::<Vec<_>>();

    assert!(
        text.iter()
            .any(|line| line.contains("Session") && line.contains("50% left")),
        "narrow layout should keep session remaining: {text:?}"
    );
    assert!(
        text.iter()
            .any(|line| line.contains("Session") && line.contains("Resets at 15:00 UTC")),
        "narrow layout should keep reset detail: {text:?}"
    );
    assert!(
        !text.iter().any(|line| line.contains("On pace")),
        "narrow layout should drop pace detail to fit: {text:?}"
    );
}
}
