use super::*;

#[test]
fn width_oracle_covers_profile_clusters() {
    let profile = VirtualTerminalProfile::default();
    let cases = [
        ("a", 1),
        ("e\u{301}", 1),
        ("\u{301}", 0),
        ("\u{4f60}", 2),
        ("\u{2601}", 1),
        ("\u{2601}\u{fe0f}", 2),
        ("\u{1f600}", 2),
        ("\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}", 2),
        ("\u{ff76}", 1),
        ("\u{ff76}\u{ff9e}", 2),
        ("\u{ff8a}\u{ff9f}", 2),
        ("\u{00a1}", 1),
    ];

    for (cluster, expected) in cases {
        assert_eq!(
            profile.cluster_width(cluster),
            expected,
            "width mismatch for {cluster:?}"
        );
    }
}
