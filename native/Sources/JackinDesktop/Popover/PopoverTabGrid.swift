// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBridge
import SwiftUI

/// Horizontal tab strip: a static Overview tab then one tab per Rust glance row
/// in canonical order (no sort/filter in Swift). Each provider tab shows the
/// Rust `displayLabel`, the icon selected from `iconKey`, and thin geometry from
/// `glanceRemainingPercent`; the Rust `barLabel` is the accessibility value.
struct PopoverTabGrid: View {
    let providers: [PresentationStore.GlanceProviderRow]
    @Binding var selection: String?

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                tab(
                    id: nil,
                    label: "Overview",
                    iconKey: nil,
                    barLabel: "Overview",
                    remaining: nil,
                    dimmed: false
                )
                ForEach(providers) { provider in
                    tab(
                        id: provider.surfaceId,
                        label: provider.displayLabel,
                        iconKey: provider.iconKey,
                        barLabel: provider.barLabel,
                        remaining: provider.glanceRemainingPercent,
                        dimmed: provider.dimmed
                    )
                }
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
        }
    }

    @ViewBuilder
    private func tab(
        id: String?,
        label: String,
        iconKey: String?,
        barLabel: String,
        remaining: UInt8?,
        dimmed: Bool
    ) -> some View {
        Button {
            selection = id
        } label: {
            VStack(spacing: 3) {
                icon(iconKey)
                    .frame(width: 16, height: 16)
                Text(label)
                    .font(.caption2)
                    .lineLimit(1)
                meter(remaining)
            }
            .frame(minWidth: 52)
            .opacity(dimmed ? 0.55 : 1)
            .padding(.vertical, 4)
            .padding(.horizontal, 6)
        }
        .buttonStyle(.plain)
        .background(
            RoundedRectangle(cornerRadius: 6)
                .fill(selection == id ? Color.accentColor.opacity(0.18) : Color.clear)
        )
        .accessibilityLabel(label)
        .accessibilityValue(barLabel)
    }

    @ViewBuilder
    private func icon(_ iconKey: String?) -> some View {
        if let iconKey, let symbol = desktopProviderSystemImage(iconKey: iconKey) {
            Image(systemName: symbol)
        } else {
            Image(systemName: "square.grid.2x2")
        }
    }

    /// Meter geometry only — `nil` remaining shows the empty track (Rust dash /
    /// segments remain the visible truth elsewhere).
    @ViewBuilder
    private func meter(_ remaining: UInt8?) -> some View {
        GeometryReader { geometry in
            ZStack(alignment: .leading) {
                Capsule().fill(Color.secondary.opacity(0.25))
                if let remaining {
                    Capsule()
                        .fill(Color.accentColor)
                        .frame(width: geometry.size.width * CGFloat(remaining) / 100.0)
                }
            }
        }
        .frame(width: 40, height: 3)
    }
}
