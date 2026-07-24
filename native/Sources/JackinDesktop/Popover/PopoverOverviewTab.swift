// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBridge
import SwiftUI

/// Overview tab: exactly one button-row per Rust glance row, in returned order.
/// Displays the Rust `displayLabel`, `headline`, `resetLabel`, `statusWord`,
/// severity color, and `dimmed`; a row click changes the popover selection.
struct PopoverOverviewTab: View {
    let providers: [PresentationStore.GlanceProviderRow]
    @Binding var selection: String?

    var body: some View {
        VStack(spacing: 2) {
            ForEach(providers) { provider in
                Button {
                    selection = provider.surfaceId
                } label: {
                    HStack(spacing: 8) {
                        Circle()
                            .fill(severityTint(provider.severity))
                            .frame(width: 8, height: 8)
                        VStack(alignment: .leading, spacing: 1) {
                            Text(provider.displayLabel).font(.callout)
                            HStack(spacing: 6) {
                                Text(provider.headline)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                if let reset = provider.resetLabel {
                                    Text(reset)
                                        .font(.caption2)
                                        .foregroundStyle(.tertiary)
                                }
                            }
                        }
                        Spacer()
                        Text(provider.statusWord)
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                    .opacity(provider.dimmed ? 0.55 : 1)
                    .contentShape(Rectangle())
                    .padding(.vertical, 4)
                    .padding(.horizontal, 8)
                }
                .buttonStyle(.plain)
            }
        }
    }
}
