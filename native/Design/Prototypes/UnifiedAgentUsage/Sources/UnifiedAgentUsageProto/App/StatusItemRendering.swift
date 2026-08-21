import AppKit

/// AppKit rendering for one per-provider `NSStatusItem`.
enum StatusItemRendering {
    static func title(
        barLabel: String, compactResetLabel: String?,
        percentTint: NSColor
    ) -> NSAttributedString {
        let paragraph = NSMutableParagraphStyle()
        paragraph.alignment = .left
        paragraph.maximumLineHeight = 9
        paragraph.minimumLineHeight = 9
        paragraph.lineSpacing = 0

        let bottomFont = NSFont.monospacedDigitSystemFont(ofSize: 10.5, weight: .semibold)
        let topFont = NSFont.monospacedDigitSystemFont(ofSize: 9.5, weight: .semibold)

        guard let compactResetLabel, !compactResetLabel.isEmpty else {
            return NSAttributedString(
                string: barLabel,
                attributes: [
                    .font: bottomFont,
                    .foregroundColor: percentTint,
                    .paragraphStyle: paragraph,
                ])
        }

        // Measured against the rendered button: the multi-line title block
        // draws top-aligned, ~2.5pt above the icon's center. A −2.5pt global
        // shift with a 3pt spread (top −0.5, bottom −3.5) lands the text ink
        // mid exactly on the icon ink mid (raster-verified, both lines).
        let result = NSMutableAttributedString()
        result.append(
            NSAttributedString(
                string: compactResetLabel + "\n",
                attributes: [
                    .font: topFont,
                    .foregroundColor: JackinBrand.mutedNSColor,
                    .paragraphStyle: paragraph,
                    .baselineOffset: -0.5,
                ]))
        result.append(
            NSAttributedString(
                string: barLabel,
                attributes: [
                    .font: bottomFont,
                    .foregroundColor: percentTint,
                    .paragraphStyle: paragraph,
                    .baselineOffset: -3.5,
                ]))
        return result
    }
}
