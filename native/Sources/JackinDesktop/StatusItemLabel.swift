// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBridge
import SwiftUI

/// Menu-bar status item — **OpenUsage-style multi-provider strip** with
/// Liquid Glass chip capsules (macOS 26+) / ultraThinMaterial fallback.
///
/// One chip per enabled provider: **provider icon + remaining %**
/// (stacked session/weekly when Rust supplies dual buckets). Remainings from
/// Rust only — never invents percentages.
struct StatusItemLabel: View {
    @ObservedObject var store: PresentationStore

    var body: some View {
        HStack(spacing: 5) {
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
                // OpenUsage: [icon 99%/63%] [icon 100%/81%] … for each provider.
                ForEach(store.statusItemChips) { chip in
                    StatusItemChipView(chip: chip)
                        .opacity(itemOpacity)
                }
            }
        }
        .frame(minWidth: 18, minHeight: 22)
        .padding(.horizontal, 1)
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
        if !store.statusItemChips.isEmpty {
            return statusItemAccessibilityLabel(chips: store.statusItemChips)
        }
        if !store.statusItemText.isEmpty {
            return "jackin Desktop \(store.statusItemText)"
        }
        return "jackin Desktop"
    }

    static let chipFont = Font.system(size: 11, weight: .semibold, design: .rounded)
    static let dualFont = Font.system(size: 10, weight: .semibold, design: .rounded)

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

/// One provider chip: glass capsule + icon + available-token % (OpenUsage density).
private struct StatusItemChipView: View {
    let chip: StatusItemChip

    var body: some View {
        HStack(spacing: 3) {
            providerMark

            if chip.percentLines.isEmpty {
                Text(chip.compactLabel)
                    .font(StatusItemLabel.chipFont)
                    .monospacedDigit()
                    .foregroundStyle(severityTint(chip.severity))
                    .lineLimit(1)
            } else if chip.percentLines.count == 1 {
                Text(chip.percentLines[0])
                    .font(StatusItemLabel.chipFont)
                    .monospacedDigit()
                    .foregroundStyle(severityTint(chip.severity))
                    .lineLimit(1)
            } else {
                VStack(alignment: .trailing, spacing: 0) {
                    ForEach(Array(chip.percentLines.enumerated()), id: \.offset) { index, line in
                        let lineSeverity =
                            index < chip.severityPerLine.count
                            ? chip.severityPerLine[index]
                            : chip.severity
                        Text(line)
                            .font(StatusItemLabel.dualFont)
                            .monospacedDigit()
                            .foregroundStyle(severityTint(lineSeverity))
                            .lineLimit(1)
                    }
                }
            }
        }
        .padding(.horizontal, 5)
        .padding(.vertical, 2)
        .background {
            GlassFallbacks.statusItemChipBackground(severity: severityTint(chip.severity))
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(
            chip.percentLines.isEmpty
                ? chip.compactLabel
                : "\(chip.glyph) \(chip.percentLines.joined(separator: " "))"
        )
    }

    @ViewBuilder
    private var providerMark: some View {
        ZStack {
            Circle()
                .fill(severityTint(chip.severity).opacity(0.28))
                .frame(width: 15, height: 15)
            if let systemImage = chip.systemImage {
                Image(systemName: systemImage)
                    .font(.system(size: 8, weight: .bold))
                    .foregroundStyle(severityTint(chip.severity))
            } else {
                Text(chip.glyph)
                    .font(.system(size: 8, weight: .bold, design: .rounded))
                    .foregroundStyle(.primary)
            }
        }
    }
}
