// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{Attrs, Color, UnderlineStyle};
use ratatui_core::buffer::CellWidth;

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

#[test]
fn profile_width_stays_aligned_with_ratatui_cell_width() {
    let profile = VirtualTerminalProfile::default();
    let clusters = [
        "a",
        "e\u{301}",
        "\u{301}",
        "\u{4f60}",
        "\u{2601}",
        "\u{2601}\u{fe0f}",
        "\u{1f600}",
        "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}",
        "\u{ff76}",
        "\u{ff76}\u{ff9e}",
        "\u{ff8a}\u{ff9f}",
        "\u{00a1}",
    ];

    for cluster in clusters {
        assert_eq!(
            profile.cluster_width(cluster),
            cluster.cell_width().min(2),
            "Ratatui width drift for {cluster:?}"
        );
    }
}

#[test]
fn profile_owns_agent_visible_terminal_contract() {
    let profile = VirtualTerminalProfile::default();

    assert_eq!(profile.decrqm_status(2027), 0);
    assert_eq!(profile.default_reported_color(10), Some((0xe6, 0xe6, 0xe6)));
    assert_eq!(profile.default_reported_color(11), Some((0, 0, 0)));
    assert_eq!(profile.agent_term, "xterm-256color");
    assert_eq!(profile.agent_colorterm, "truecolor");
    assert_eq!(profile.osc8_policy, Osc8Policy::ModelMetadata);

    let attrs = Attrs {
        foreground: Color::Rgb(1, 2, 3),
        background: Color::Idx(42),
        underline_color: Color::Rgb(4, 5, 6),
        underline_style: UnderlineStyle::Curly,
        bold: true,
        italic: true,
        inverse: true,
        dim: true,
        strikethrough: true,
        slow_blink: true,
        rapid_blink: true,
        conceal: true,
        overline: true,
    };
    assert!(profile.attrs_supported(&attrs));
}

#[test]
fn attrs_supported_rejects_when_profile_lacks_capability() {
    // The all-true default accepts everything; the contract that earns this
    // method is the false branch — a profile missing a capability must reject
    // an attr that needs it.
    let mut profile = VirtualTerminalProfile::default();
    profile.supported_sgr.flags &= !(1 << 2);
    let italic = Attrs {
        italic: true,
        ..Attrs::default()
    };
    assert!(
        !profile.attrs_supported(&italic),
        "italic attr must be rejected when the profile lacks italic"
    );

    // Non-default colors require a color capability; with both off and a
    // truecolor foreground, the color gate must reject.
    let mut mono = VirtualTerminalProfile::default();
    mono.supported_sgr.flags &= !((1 << 11) | (1 << 12));
    let colored = Attrs {
        foreground: Color::Rgb(1, 2, 3),
        ..Attrs::default()
    };
    assert!(
        !mono.attrs_supported(&colored),
        "truecolor fg must be rejected when the profile supports no color"
    );
    // …but a default-colored, unstyled attr still passes the same mono profile.
    assert!(mono.attrs_supported(&Attrs::default()));
}
