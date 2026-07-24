// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBridge
import SwiftUI

/// Full-width overview rows (S2/S3 content for the Overview selection).
///
/// Renders the Rust-owned glance rows verbatim — identity, plan, headline,
/// reset, and status strings all come from Rust; this view synthesizes no usage
/// text (plan 008). Empty shows the fixed hint.
struct OverviewListView: View {
    let model: UsageWindowModel
    var onSelect: (String) -> Void

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 8) {
                if model.isEmpty {
                    Text(UsageWindowModel.emptyHint)
                        .foregroundStyle(.secondary)
                        .padding()
                }
                ForEach(model.sidebar) { row in
                    Button {
                        onSelect(row.surfaceId)
                    } label: {
                        overviewCard(row)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("\(row.displayLabel) \(row.headline)")
                }
            }
            .padding(16)
        }
    }

    private func overviewCard(_ row: PresentationStore.GlanceProviderRow) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Circle()
                .fill(severityTint(row.severity))
                .frame(width: 8, height: 8)
                .padding(.top, 6)

            VStack(alignment: .leading, spacing: 6) {
                HStack(alignment: .firstTextBaseline) {
                    Text(row.displayLabel)
                        .font(.headline)
                    Spacer(minLength: 8)
                    if let plan = row.planLabel, !plan.isEmpty {
                        Text(plan)
                            .font(.caption.weight(.medium))
                            .foregroundStyle(.secondary)
                    }
                }

                if !row.headline.isEmpty {
                    Text(row.headline)
                        .font(.subheadline)
                        .monospacedDigit()
                } else if !row.statusWord.isEmpty {
                    Text(row.statusWord)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }

                if let reset = row.resetLabel, !reset.isEmpty {
                    Text(reset)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .monospacedDigit()
                }
                if let exact = row.exactReset, !exact.isEmpty {
                    Text(exact)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
                if let error = row.lastError, !error.isEmpty {
                    Text(error)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background {
            // Content layer — standard material only (HIG: no Liquid Glass here).
            GlassFallbacks.contentCardBackground()
        }
    }
}
