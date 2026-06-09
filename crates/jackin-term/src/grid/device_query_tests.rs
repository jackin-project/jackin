use super::DamageGrid;
use crate::PassthroughEvent;

fn replies(g: &mut DamageGrid) -> Vec<Vec<u8>> {
    g.drain_passthrough()
        .into_iter()
        .filter_map(|e| match e {
            PassthroughEvent::Reply(b) => Some(b),
            _ => None,
        })
        .collect()
}

#[test]
fn da1_answers_conservative_vt220() {
    let mut g = DamageGrid::new(4, 20, 100);
    g.process(b"\x1b[c");
    assert_eq!(replies(&mut g), vec![b"\x1b[?62c".to_vec()]);
}

#[test]
fn dsr_cursor_position_uses_grid_cursor_not_host() {
    let mut g = DamageGrid::new(10, 40, 100);
    g.process(b"\x1b[4;6H\x1b[6n"); // home to row4/col6 (1-based), then query
    assert_eq!(replies(&mut g), vec![b"\x1b[4;6R".to_vec()]);
}

#[test]
fn decrqm_declines_grapheme_width_mode_2027() {
    let mut g = DamageGrid::new(4, 20, 100);
    g.process(b"\x1b[?2027$p");
    // 0 = "mode not recognized" -> agent renders with legacy column widths.
    assert_eq!(replies(&mut g), vec![b"\x1b[?2027;0$y".to_vec()]);
}

#[test]
fn kitty_keyboard_query_answers_no_enhancement() {
    let mut g = DamageGrid::new(4, 20, 100);
    g.process(b"\x1b[?u");
    assert_eq!(replies(&mut g), vec![b"\x1b[?0u".to_vec()]);
}

#[test]
fn device_queries_are_not_forwarded_to_host() {
    let mut g = DamageGrid::new(4, 20, 100);
    g.process(b"\x1b[c\x1b[6n\x1b[?2026$p\x1b[?u");
    // Every event is an agent-bound Reply; none is a host-bound UnhandledCsi.
    for e in g.drain_passthrough() {
        assert!(
            matches!(e, PassthroughEvent::Reply(_)),
            "device query leaked to host as {e:?}"
        );
    }
}
