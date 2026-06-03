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
fn workspace_header_title_is_view_owned() {
    assert_eq!(workspace_header_title(), "workspaces");
}

#[test]
fn modal_areas_keep_existing_sizes() {
    let area = Rect::new(0, 0, 100, 40);

    assert_eq!(delete_confirm_area(area).width, 60);
    assert_eq!(delete_confirm_area(area).height, 7);
    assert_eq!(purge_confirm_area(area).width, 70);
    assert_eq!(purge_confirm_area(area).height, 9);
    assert_eq!(status_overlay_area(area).width, 50);
    assert_eq!(status_overlay_area(area).height, 7);
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
