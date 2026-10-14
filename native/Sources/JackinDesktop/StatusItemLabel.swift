// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit







@MainActor
public enum StatusItemRendering {




    public static func icon(forIconKey iconKey: String) -> NSImage {
        if let official = ProviderMarks.templateImage(forIconKey: iconKey) {
            return official
        }
        return fallbackIcon()
    }



    public static func fallbackIcon() -> NSImage {
        if let image = JackinBrandIdentity.templateMonogram() {
            return image
        }
        let image =
            NSImage(systemSymbolName: "chevron.right", accessibilityDescription: nil) ?? NSImage()
        image.isTemplate = true
        return image
    }




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
