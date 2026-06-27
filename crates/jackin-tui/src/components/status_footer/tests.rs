//! Tests for `status_footer`.
use super::*;
use ratatui::{Terminal, backend::TestBackend};

fn dump(left: &str, right: &str, width: u16) -> String {
    let backend = TestBackend::new(width, 1);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(StatusFooter::new(left).right(right), frame.area());
        })
        .unwrap();
    (0..width)
        .map(|x| terminal.backend().buffer()[(x, 0)].symbol().to_owned())
        .collect()
}

fn dump_group(left: &str, right: StatusRightGroup<'_>, width: u16) -> String {
    let backend = TestBackend::new(width, 1);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(StatusFooter::new(left).right_group(right), frame.area());
        })
        .unwrap();
    (0..width)
        .map(|x| terminal.backend().buffer()[(x, 0)].symbol().to_owned())
        .collect()
}

#[test]
fn renders_activity_on_the_left_and_chip_on_the_right() {
    let row = dump("Building Docker image", "k7p9m2xq", 60);
    assert!(
        row.contains("Building Docker image"),
        "activity missing: {row:?}"
    );
    assert!(row.contains("k7p9m2xq"), "chip missing: {row:?}");
    assert!(
        row.find("Building").unwrap() < row.find("k7p9m2xq").unwrap(),
        "activity must be left of the chip: {row:?}"
    );
}

#[test]
fn bar_fills_white_background_across_the_row() {
    let backend = TestBackend::new(30, 1);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(StatusFooter::new("x").right("y"), frame.area());
        })
        .unwrap();
    for x in 0..30 {
        assert_eq!(
            terminal.backend().buffer()[(x, 0)].bg,
            WHITE,
            "cell {x} should have white bg"
        );
    }
}

#[test]
fn debug_chip_renders_in_amber_to_the_right_of_the_instance_chip() {
    let backend = TestBackend::new(60, 1);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                StatusFooter::new("building")
                    .right("s9994y2n")
                    .right_debug(Some("jk-run-3d7e23")),
                frame.area(),
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let row: String = (0..60)
        .map(|x| buffer[(x, 0)].symbol().to_owned())
        .collect();
    assert!(row.contains("s9994y2n"), "instance chip missing: {row:?}");
    assert!(
        row.contains("jk-run-3d7e23"),
        "debug run-id chip missing: {row:?}"
    );
    assert!(
        row.find("s9994y2n").unwrap() < row.find("jk-run-3d7e23").unwrap(),
        "run id must be right of the instance id: {row:?}"
    );
    assert!(
        (0..60).any(|x| buffer[(x, 0)].bg == DANGER_RED),
        "run-id chip must use DANGER_RED background"
    );
}

#[test]
fn right_group_renders_usage_container_then_run_id() {
    let row = dump_group(
        "building",
        StatusRightGroup {
            spend: None,
            usage: Some("Session 37%"),
            container: "s9994y2n",
            run_id: Some("jk-run-3d7e23"),
        },
        80,
    );

    assert!(row.contains("Session 37%"), "usage missing: {row:?}");
    assert!(row.contains("s9994y2n"), "container missing: {row:?}");
    assert!(
        row.contains("jk-run-3d7e23"),
        "debug run id missing: {row:?}"
    );
    assert!(
        row.find("Session 37%").unwrap() < row.find("s9994y2n").unwrap(),
        "usage must render left of container: {row:?}"
    );
    assert!(
        row.find("s9994y2n").unwrap() < row.find("jk-run-3d7e23").unwrap(),
        "container must render left of run id: {row:?}"
    );
}

#[test]
fn right_group_layout_orders_usage_container_run_id() {
    let layout = status_right_group_layout(
        100,
        StatusRightGroup {
            spend: None,
            usage: Some("Session 37%"),
            container: "s9994y2n",
            run_id: Some("jk-run-3d7e23"),
        },
    );

    let usage = layout.usage.expect("usage chunk");
    let container = layout.container.expect("container chunk");
    let run_id = layout.run_id.expect("run id chunk");

    assert!(usage.start < container.start);
    assert!(container.start < run_id.start);
    assert_eq!(run_id.end, 101);
}

#[test]
fn right_group_layout_compacts_usage_before_dropping_it() {
    let layout = status_right_group_layout(
        44,
        StatusRightGroup {
            spend: None,
            usage: Some("Session 37% · Weekly 10%"),
            container: "jk-test-container",
            run_id: None,
        },
    );

    let usage = layout.usage.expect("usage chunk");
    assert!(usage.text.contains("Session 37%"), "{usage:?}");
    assert!(!usage.text.contains("Weekly 10%"), "{usage:?}");
}

#[test]
fn right_group_render_uses_compacted_layout() {
    let row = dump_group(
        "building",
        StatusRightGroup {
            spend: None,
            usage: Some("Session 37% · Weekly 10%"),
            container: "jk-test-container",
            run_id: None,
        },
        44,
    );

    assert!(row.contains("Session 37%"), "{row:?}");
    assert!(!row.contains("Weekly 10%"), "{row:?}");
    assert!(row.contains("jk-test-container"), "{row:?}");
}

#[test]
fn compact_usage_status_label_keeps_quota_and_lifecycle_state() {
    assert_eq!(
        compact_usage_status_label("Codex · Session 37% · Weekly 10% · account login"),
        "Session 37% · login"
    );
    assert_eq!(
        compact_usage_status_label("Amp · account unavailable login"),
        "login"
    );
}

#[test]
fn right_chip_rect_targets_instance_chip_before_debug_chip() {
    let area = Rect::new(0, 23, 80, 1);
    let rect = status_footer_right_chip_rect(area, "s9994y2n", Some("jk-run-3d7e23"))
        .expect("instance chip rect");
    assert_eq!(rect.y, 23);
    assert_eq!(rect.width, 10);
    assert!(
        rect.x < 80 - 10,
        "instance chip must sit left of the debug chip"
    );
}
