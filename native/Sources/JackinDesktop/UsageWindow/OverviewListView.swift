// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBridge
import SwiftUI

/// Full-width overview rows (S2/S3 content for the Overview selection).
struct OverviewListView: View {
    @ObservedObject var store: PresentationStore
    var onSelect: (String) -> Void

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 8) {
                if store.overviewRows.isEmpty {
                    Text("No enabled surfaces")
                        .foregroundStyle(.secondary)
                        .padding()
                }
                ForEach(store.overviewRows) { row in
                    Button {
                        onSelect(row.surfaceId)
                    } label: {
                        HStack(alignment: .firstTextBaseline, spacing: 10) {
                            Circle()
                                .fill(severityTint(row.severity))
                                .frame(width: 8, height: 8)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(row.displayLabel)
                                    .font(.headline)
                                if !row.headline.isEmpty {
                                    Text(row.headline)
                                        .font(.subheadline)
                                        .monospacedDigit()
                                } else {
                                    Text(row.statusWord)
                                        .font(.subheadline)
                                        .foregroundStyle(.secondary)
                                }
                            }
                            Spacer(minLength: 8)
                            VStack(alignment: .trailing, spacing: 2) {
                                if let reset = row.resetLabel {
                                    Text(reset)
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                        .monospacedDigit()
                                }
                                if let exact = row.exactReset {
                                    Text(exact)
                                        .font(.caption2)
                                        .foregroundStyle(.tertiary)
                                }
                            }
                        }
                        .padding(12)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background {
                            RoundedRectangle(cornerRadius: 10)
                                .fill(.background.secondary)
                        }
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(
                        "\(row.displayLabel) \(row.headline.isEmpty ? row.statusWord : row.headline)"
                    )
                }
            }
            .padding(16)
        }
        .safeAreaInset(edge: .bottom) {
            HStack {
                Text(store.nextRefreshLabel)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                Spacer()
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            .background {
                GlassFallbacks.footerBarBackground()
            }
        }
    }
}
