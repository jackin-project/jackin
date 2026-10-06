// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit

/// AppKit rendering for one per-provider `NSStatusItem`.
///
/// **Liquid Glass / product law:** menu-bar items stay
/// **template mono** — no glass chips, no severity tint on the bar. Dual stack
/// shows compact reset countdown (top) + Rust `barLabel` glance % (bottom).
/// Every usage value is Rust-owned; Swift only layouts and templates icons.
@MainActor
public enum StatusItemRendering {
    /// Template icon for a provider icon key.
    ///
    /// Prefers **official** bundled PDF logomarks from `ProviderMarks`.
    /// Falls back to the jackin❯ monogram for unknown keys.
    public static func icon(forIconKey iconKey: String) -> NSImage {
        if let official = ProviderMarks.templateImage(forIconKey: iconKey) {
            return official
        }
        return fallbackIcon()
    }

    /// The static jackin❯ logomark used by the empty-set fallback status item
    /// and for any non-provider icon key.
    public static func fallbackIcon() -> NSImage {
        if let image = JackinBrandIdentity.templateMonogram() {
            return image
        }
        let image =
            NSImage(systemSymbolName: "chevron.right", accessibilityDescription: nil) ?? NSImage()
        image.isTemplate = true
        return image
    }

    /// Dual-stack title: compact reset (top) + `barLabel` (bottom).
    ///
    /// When no reset is available, falls back to a single-line `barLabel`.
    public static func title(
        barLabel: String,
        compactResetLabel: String?
    ) -> NSAttributedString {
        let paragraph = NSMutableParagraphStyle()
        paragraph.alignment = .left
        paragraph.maximumLineHeight = 10
        paragraph.minimumLineHeight = 9

        let bottomFont = NSFont.monospacedDigitSystemFont(
            ofSize: NSFont.smallSystemFontSize,
            weight: .semibold
        )
        let topFont = NSFont.monospacedDigitSystemFont(
            ofSize: NSFont.smallSystemFontSize - 1,
            weight: .medium
        )

        guard let compactResetLabel, !compactResetLabel.isEmpty else {
            return NSAttributedString(
                string: barLabel,
                attributes: [
                    .font: bottomFont,
                    .foregroundColor: NSColor.labelColor,
                    .paragraphStyle: paragraph,
                ]
            )
        }

        let result = NSMutableAttributedString()
        result.append(
            NSAttributedString(
                string: compactResetLabel + "\n",
                attributes: [
                    .font: topFont,
                    .foregroundColor: NSColor.secondaryLabelColor,
                    .paragraphStyle: paragraph,
                ]
            )
        )
        result.append(
            NSAttributedString(
                string: barLabel,
                attributes: [
                    .font: bottomFont,
                    .foregroundColor: NSColor.labelColor,
                    .paragraphStyle: paragraph,
                ]
            )
        )
        return result
    }
}
