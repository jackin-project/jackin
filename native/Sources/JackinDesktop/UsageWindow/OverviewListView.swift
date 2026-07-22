// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBridge
import SwiftUI

/// Full-width overview rows (S2/S3 content for the Overview selection).
///
/// OpenUsage-style density: provider identity + stacked remaining buckets
/// (session/weekly) when Rust supplies remainings; driving headline as fallback.
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
                        overviewCard(row)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(accessibilityLabel(for: row))
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

    private func overviewCard(_ row: PresentationStore.OverviewRow) -> some View {
        let surface = store.surfaces.first(where: { $0.id == row.surfaceId })
        // Session/Weekly + model-scoped windows (Fable, …) up to overviewNumericBucketCap.
        let numericBuckets: [PresentationStore.BucketRow] =
            surface?.buckets.filter { $0.remainingPercent != nil }
            .prefix(overviewNumericBucketCap).map { $0 } ?? []

        return HStack(alignment: .top, spacing: 10) {
            Circle()
                .fill(severityTint(row.severity))
                .frame(width: 8, height: 8)
                .padding(.top, 6)

            VStack(alignment: .leading, spacing: 6) {
                HStack(alignment: .firstTextBaseline) {
                    Text(row.displayLabel)
                        .font(.headline)
                    Spacer(minLength: 8)
                    if let plan = surface?.planLabel, !plan.isEmpty {
                        Text(plan)
                            .font(.caption.weight(.medium))
                            .foregroundStyle(.secondary)
                    }
                }

                if !numericBuckets.isEmpty {
                    // Dual-bucket stack (OpenUsage Session/Weekly remainings).
                    VStack(alignment: .leading, spacing: 6) {
                        ForEach(numericBuckets) { bucket in
                            bucketMiniRow(bucket)
                        }
                    }
                } else if !row.headline.isEmpty {
                    Text(row.headline)
                        .font(.subheadline)
                        .monospacedDigit()
                } else {
                    Text(row.statusWord)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }

                if let reset = row.resetLabel, numericBuckets.isEmpty {
                    Text(reset)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .monospacedDigit()
                }
                if let exact = row.exactReset, numericBuckets.isEmpty {
                    Text(exact)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
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

    @ViewBuilder
    private func bucketMiniRow(_ bucket: PresentationStore.BucketRow) -> some View {
        if let remaining = bucket.remainingPercent {
            VStack(alignment: .leading, spacing: 3) {
                HStack(alignment: .firstTextBaseline) {
                    Text(bucket.label.isEmpty ? "Quota" : bucket.label)
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(.secondary)
                    Spacer(minLength: 6)
                    // Depleted: prefer Rust reset countdown over bare zero percent.
                    Text(miniPrimaryLabel(bucket, remaining: remaining))
                        .font(.caption.weight(.semibold))
                        .monospacedDigit()
                        .foregroundStyle(severityTint(bucket.severity))
                    if remaining > 0, let reset = bucket.resetLabel, !reset.isEmpty {
                        Text(reset)
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                            .monospacedDigit()
                            .lineLimit(1)
                            .minimumScaleFactor(0.8)
                    }
                }
                GeometryReader { geo in
                    let frac = Double(remaining) / 100.0
                    ZStack(alignment: .leading) {
                        Capsule().fill(Color.primary.opacity(0.10))
                        Capsule()
                            .fill(severityTint(bucket.severity))
                            .frame(width: max(2, geo.size.width * frac))
                    }
                }
                .frame(height: 3)
                // CodexBar/OpenUsage: pace projection under the quota window.
                if let pace = bucket.paceLabel, !pace.isEmpty {
                    let parts = splitPaceLabel(pace)
                    if parts.count >= 2 {
                        HStack(alignment: .firstTextBaseline) {
                            Text(parts[0])
                                .font(.caption2)
                                .foregroundStyle(.tertiary)
                                .monospacedDigit()
                            Spacer(minLength: 6)
                            Text(parts[1])
                                .font(.caption2)
                                .foregroundStyle(.tertiary)
                                .multilineTextAlignment(.trailing)
                        }
                    } else {
                        Text(pace)
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
            }
        }
    }

    /// OpenUsage primary line: remaining/used % or depleted reset countdown.
    private func miniPrimaryLabel(
        _ bucket: PresentationStore.BucketRow,
        remaining: UInt8
    ) -> String {
        bucketMetricPrimaryLabel(
            remainingPercent: remaining,
            usedLabel: bucket.usedLabel,
            resetLabel: bucket.resetLabel,
            percentStyle: store.percentStyle
        )
    }

    private func accessibilityLabel(for row: PresentationStore.OverviewRow) -> String {
        let surface = store.surfaces.first(where: { $0.id == row.surfaceId })
        let remainings = surface?.buckets.compactMap(\.remainingPercent) ?? []
        if !remainings.isEmpty {
            let lines = statusItemPercentLines(
                remainings: remainings,
                maxLines: 2,
                percentStyle: store.percentStyle
            )
            return "\(row.displayLabel) \(lines.joined(separator: " and "))"
        }
        return "\(row.displayLabel) \(row.headline.isEmpty ? row.statusWord : row.headline)"
    }
}
