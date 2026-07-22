// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBridge
import SwiftUI

/// Menu-bar status item — OpenUsage-inspired strip (glyph + stacked short percents).
///
/// Clean-room look-and-feel only. Percent tokens are derived from Rust
/// `remainingPercent` fields; full compact labels stay on accessibility.
struct StatusItemLabel: View {
    @ObservedObject var store: PresentationStore

    var body: some View {
        HStack(spacing: 8) {
            if store.statusItemChips.isEmpty {
                statusIcon
                    .opacity(itemOpacity)
                if !store.statusItemText.isEmpty {
                    Text(store.statusItemText)
                        .font(Self.chipFont)
                        .monospacedDigit()
                        .opacity(itemOpacity)
                }
            } else {
                // OpenUsage multi-metric strip: no single global logo crowding chips.
                ForEach(store.statusItemChips) { chip in
                    StatusItemChipView(chip: chip)
                        .opacity(itemOpacity)
                }
            }
        }
        .frame(minWidth: 18, minHeight: 20)
        .padding(.horizontal, 2)
        .accessibilityLabel(accessibilityText)
        .onAppear {
            if !store.isOpen {
                store.openDefault()
            }
        }
    }

    private var itemOpacity: Double {
        store.allEnabledSurfacesDegraded ? 0.45 : 1.0
    }

    @ViewBuilder
    private var statusIcon: some View {
        if let mark = Self.loadLogomark() {
            Image(nsImage: mark)
                .renderingMode(.template)
                .resizable()
                .interpolation(.high)
                .frame(width: 14, height: 14)
        } else {
            Image(systemName: "gauge.with.needle")
                .symbolRenderingMode(.monochrome)
                .imageScale(.small)
                .frame(width: 14, height: 14)
        }
    }

    private var accessibilityText: String {
        if !store.statusItemText.isEmpty {
            return "jackin Desktop \(store.statusItemText)"
        }
        return "jackin Desktop"
    }

    static let chipFont = Font.system(size: 10, weight: .semibold, design: .rounded)

    private static func loadLogomark() -> NSImage? {
        let bundle = Bundle.module
        let url =
            bundle.url(forResource: "JackinMark", withExtension: "pdf")
            ?? bundle.url(forResource: "JackinMark", withExtension: "PDF")
        guard let url else { return nil }
        guard let image = NSImage(contentsOf: url) else { return nil }
        image.isTemplate = true
        image.size = NSSize(width: 14, height: 14)
        return image
    }
}

/// OpenUsage menu-bar chip: small glyph + stacked short percents (session/weekly).
private struct StatusItemChipView: View {
    let chip: StatusItemChip

    var body: some View {
        HStack(spacing: 3) {
            Text(chip.glyph)
                .font(.system(size: 9, weight: .bold, design: .rounded))
                .foregroundStyle(.primary)
                .frame(width: 14, height: 14)
                .background {
                    Circle().fill(severityTint(chip.severity).opacity(0.22))
                }

            if chip.percentLines.isEmpty {
                Text(chip.compactLabel)
                    .font(StatusItemLabel.chipFont)
                    .monospacedDigit()
                    .lineLimit(1)
            } else {
                VStack(alignment: .trailing, spacing: 0) {
                    ForEach(Array(chip.percentLines.enumerated()), id: \.offset) { _, line in
                        Text(line)
                            .font(StatusItemLabel.chipFont)
                            .monospacedDigit()
                            .lineLimit(1)
                    }
                }
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(chip.compactLabel)
    }
}
