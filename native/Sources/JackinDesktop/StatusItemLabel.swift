// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBridge
import SwiftUI

/// Menu-bar status item — **OpenUsage-style multi-provider usage strip**.
///
/// One chip per enabled provider (up to strip cap): **provider icon + remaining %**
/// (stacked session/weekly when Rust supplies dual buckets). Clean-room layout;
/// remainings from Rust only — never invents percentages.
struct StatusItemLabel: View {
    @ObservedObject var store: PresentationStore

    var body: some View {
        HStack(spacing: 6) {
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
        if !store.statusItemChips.isEmpty {
            return statusItemAccessibilityLabel(chips: store.statusItemChips)
        }
        if !store.statusItemText.isEmpty {
            return "jackin Desktop \(store.statusItemText)"
        }
        return "jackin Desktop"
    }

    /// Slightly larger than 10pt so stacked remaining % stays readable in the menu bar.
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

/// One provider chip: **icon + available-token %** (OpenUsage density).
///
/// Dual-bucket remainings stack as two short percent lines; depleted driving
/// windows may show a Rust reset countdown instead of bare `0%`.
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
                // Single window: larger remaining % next to the icon.
                Text(chip.percentLines[0])
                    .font(StatusItemLabel.chipFont)
                    .monospacedDigit()
                    .foregroundStyle(severityTint(chip.severity))
                    .lineLimit(1)
            } else {
                // Dual stack (session / weekly remaining) — OpenUsage menubar.
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
                .frame(width: 16, height: 16)
            if let systemImage = chip.systemImage {
                Image(systemName: systemImage)
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(severityTint(chip.severity))
            } else {
                Text(chip.glyph)
                    .font(.system(size: 8, weight: .bold, design: .rounded))
                    .foregroundStyle(.primary)
            }
        }
    }
}
