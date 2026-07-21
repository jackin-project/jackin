// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBridge
import SwiftUI

struct SurfaceCard: View {
    let surface: PresentationStore.SurfaceRow

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text(surface.label)
                    .font(.subheadline.weight(.semibold))
                if let symbol = statusBadgeSymbol(surface.status) {
                    Image(systemName: symbol)
                        .font(.caption)
                        .foregroundStyle(badgeTint(surface.status))
                        .help(surface.status)
                        .accessibilityLabel(surface.status)
                }
                Spacer(minLength: 4)
            }
            if !surface.accountLabel.isEmpty || surface.planLabel != nil {
                HStack(spacing: 4) {
                    if !surface.accountLabel.isEmpty {
                        Text(surface.accountLabel)
                    }
                    if let plan = surface.planLabel {
                        if !surface.accountLabel.isEmpty {
                            Text("·")
                        }
                        Text(plan)
                    }
                }
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            // Last-good buckets stay visible even when lastError is set.
            ForEach(surface.buckets) { bucket in
                BucketGaugeRow(bucket: bucket)
            }
            if let err = surface.lastError {
                Text(err)
                    .font(.caption2)
                    .foregroundStyle(.orange)
            }
        }
        .padding(10)
        .opacity(surface.status == "stale" || surface.status == "unavailable" ? 0.85 : 1.0)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(cardAccessibilityLabel)
    }

    private var cardAccessibilityLabel: String {
        var parts = [surface.label, surface.statusBarLabel]
        if let err = surface.lastError {
            parts.append(err)
        }
        return parts.filter { !$0.isEmpty }.joined(separator: " ")
    }

    private func badgeTint(_ status: String) -> Color {
        switch status {
        case "error", "needs_login", "needs_secret", "unavailable":
            return .red
        case "stale":
            return .orange
        default:
            return .secondary
        }
    }
}

struct BucketGaugeRow: View {
    let bucket: PresentationStore.BucketRow

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack {
                Text(bucket.label)
                    .font(.caption2)
                Spacer()
                HStack(spacing: 6) {
                    if let used = bucket.usedLabel {
                        Text(used)
                            .font(.caption2)
                            .monospacedDigit()
                    } else if bucket.status == "unavailable" || bucket.status == "refreshing" {
                        Text(bucket.status)
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                    if let reset = bucket.resetLabel {
                        Text(reset)
                            .font(.caption2)
                            .monospacedDigit()
                            .foregroundStyle(.secondary)
                    }
                }
            }
            // Never invent % — only draw when Rust provided remaining_percent.
            if let remaining = bucket.remainingPercent {
                let used = Double(100 - Int(remaining))
                Gauge(value: used, in: 0...100) {
                    EmptyView()
                }
                .gaugeStyle(.accessoryLinearCapacity)
                .tint(severityTint(bucket.severity))
                .accessibilityLabel("\(bucket.label) \(Int(used)) percent used")
            }
        }
    }
}
