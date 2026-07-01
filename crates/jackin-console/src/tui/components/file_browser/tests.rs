use super::*;

#[test]
fn page_rows_for_modal_uses_listing_viewport_height() {
    let tmp = tempfile::tempdir().unwrap();
    let state = FileBrowserState::from_listing(crate::services::file_browser::listing_at(
        tmp.path().to_path_buf(),
        tmp.path().to_path_buf(),
    ));
    assert!(page_rows_for_modal(ratatui::layout::Rect::new(0, 0, 80, 24), &state) > 0);
}
