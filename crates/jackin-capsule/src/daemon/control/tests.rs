use super::coalesce_client_frames;
use jackin_protocol::attach::ClientFrame;

fn resize(rows: u16, cols: u16) -> ClientFrame {
    ClientFrame::Resize { rows, cols }
}

#[test]
fn non_resize_first_frame_passes_through_alone() {
    let (frames, coalesced) = coalesce_client_frames(ClientFrame::Detach, || None);
    assert!(matches!(frames.as_slice(), [ClientFrame::Detach]));
    assert_eq!(coalesced, 0);
}

#[test]
fn consecutive_resizes_coalesce_to_latest() {
    let mut queue = vec![resize(30, 100), resize(40, 120)].into_iter();
    let (frames, coalesced) = coalesce_client_frames(resize(20, 80), || queue.next());
    assert!(matches!(
        frames.as_slice(),
        [ClientFrame::Resize {
            rows: 40,
            cols: 120
        }]
    ));
    assert_eq!(coalesced, 2);
}

#[test]
fn stray_frame_behind_resize_is_preserved_not_dropped() {
    let mut queue = vec![ClientFrame::Detach].into_iter();
    let (frames, coalesced) = coalesce_client_frames(resize(20, 80), || queue.next());
    assert!(matches!(
        frames.as_slice(),
        [
            ClientFrame::Resize { rows: 20, cols: 80 },
            ClientFrame::Detach
        ]
    ));
    assert_eq!(coalesced, 0);
}

#[test]
fn stray_frame_after_several_resizes_is_preserved() {
    let mut queue = vec![resize(25, 90), ClientFrame::Input(vec![0x61])].into_iter();
    let (frames, coalesced) = coalesce_client_frames(resize(20, 80), || queue.next());
    assert!(matches!(
        frames.as_slice(),
        [
            ClientFrame::Resize { rows: 25, cols: 90 },
            ClientFrame::Input(bytes)
        ] if bytes == &[0x61]
    ));
    assert_eq!(coalesced, 1);
}
