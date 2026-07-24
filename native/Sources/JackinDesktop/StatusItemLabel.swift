// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBridge

/// AppKit rendering for one per-provider `NSStatusItem`: a template provider
/// icon and the Rust `barLabel` verbatim with monospaced digits.
///
/// No severity tint, mini meter, dual stack, percent calculation, fallback data
/// label, or provider reordering — Rust owns every usage value; the status item
/// only displays it.
@MainActor
enum StatusItemRendering {
    /// Template icon for a provider icon key, using the shared
    /// `desktopProviderSystemImage` seam and falling back to the bundled
    /// JackinMark for keys outside the seven-provider domain (e.g. `opencode`).
    static func icon(forIconKey iconKey: String) -> NSImage {
        if let symbol = desktopProviderSystemImage(iconKey: iconKey),
           let image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)
        {
            image.isTemplate = true
            return image
        }
        return fallbackIcon()
    }

    /// The static jackin❯ logomark used by the empty-set fallback status item
    /// and for any non-provider icon key.
    static func fallbackIcon() -> NSImage {
        if let url = Bundle.main.url(forResource: "JackinMark", withExtension: "pdf"),
           let image = NSImage(contentsOf: url)
        {
            image.isTemplate = true
            return image
        }
        let image =
            NSImage(systemSymbolName: "chevron.right", accessibilityDescription: nil) ?? NSImage()
        image.isTemplate = true
        return image
    }

    /// Attributed title carrying the Rust `barLabel` verbatim (monospaced digits).
    static func title(_ barLabel: String) -> NSAttributedString {
        let font = NSFont.monospacedDigitSystemFont(
            ofSize: NSFont.systemFontSize,
            weight: .regular
        )
        return NSAttributedString(string: barLabel, attributes: [.font: font])
    }
}
