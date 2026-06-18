//! Tests for `view`.
use super::*;

#[test]
fn workspace_frame_areas_match_header_body_footer_contract() {
    let areas = workspace_frame_areas(Rect::new(0, 0, 80, 24));

    assert_eq!(areas.header, Rect::new(0, 0, 80, 2));
    assert_eq!(areas.body, Rect::new(0, 2, 80, 20));
    assert_eq!(areas.footer, Rect::new(0, 22, 80, 2));
}

#[test]
fn modal_content_area_reserves_footer_height() {
    let area = Rect::new(3, 4, 80, 24);

    assert_eq!(modal_content_area(area, 3), Rect::new(3, 4, 80, 21));
}

#[test]
fn modal_content_area_saturates_when_footer_exceeds_height() {
    let area = Rect::new(3, 4, 80, 2);

    assert_eq!(modal_content_area(area, 3), Rect::new(3, 4, 80, 0));
}

#[test]
fn reserved_footer_height_prefers_screen_specific_heights() {
    assert_eq!(
        reserved_footer_height_for_facts(ReservedFooterHeightFacts {
            editor_footer_height: Some(4),
            settings_footer_height: Some(6),
            workspace_footer_height: 2,
        }),
        4
    );
    assert_eq!(
        reserved_footer_height_for_facts(ReservedFooterHeightFacts {
            editor_footer_height: None,
            settings_footer_height: Some(6),
            workspace_footer_height: 2,
        }),
        6
    );
    assert_eq!(
        reserved_footer_height_for_facts(ReservedFooterHeightFacts {
            editor_footer_height: None,
            settings_footer_height: None,
            workspace_footer_height: 2,
        }),
        2
    );
}

#[test]
fn workspace_header_title_is_view_owned() {
    assert_eq!(workspace_header_title(), "workspaces");
}

#[test]
fn modal_areas_stable_preferred_size() {
    // On a wide terminal (300 cols) each dialog holds its preferred width
    // (pct_w% of the 160-col reference), not a fraction of the terminal.
    let wide = Rect::new(0, 0, 300, 40);
    assert_eq!(delete_confirm_area(wide).width, 96); // 60% of 160 = 96
    assert_eq!(delete_confirm_area(wide).height, 7);
    assert_eq!(purge_confirm_area(wide).width, 112); // 70% of 160 = 112
    assert_eq!(purge_confirm_area(wide).height, 9);
    assert_eq!(status_overlay_area(wide).width, 80); // 50% of 160 = 80
    assert_eq!(status_overlay_area(wide).height, 7);

    // On a narrow terminal (50 cols), dialogs shrink to terminal_width - 4 margin.
    let narrow = Rect::new(0, 0, 50, 40);
    assert_eq!(delete_confirm_area(narrow).width, 46); // min(96, 50-4) = 46
    assert_eq!(status_overlay_area(narrow).width, 46); // min(80, 46) = 46
}

#[test]
fn modal_overlay_visible_tracks_any_modal_fact() {
    assert!(!modal_overlay_visible(ModalOverlayState::default()));
    assert!(modal_overlay_visible(ModalOverlayState {
        status_overlay: true,
        ..ModalOverlayState::default()
    }));
    assert!(modal_overlay_visible(ModalOverlayState {
        settings_auth_modal: true,
        ..ModalOverlayState::default()
    }));
    assert!(modal_overlay_visible(ModalOverlayState {
        destructive_confirm: true,
        ..ModalOverlayState::default()
    }));
}
