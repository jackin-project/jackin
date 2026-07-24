// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBridge
import SwiftUI

/// Full Capsule-parity provider card (S4). Every field, string, and order comes
/// from the Rust ``UsageDetailPresentation``; this view renders the rows and
/// their leading/trailing lines mechanically and never splits, joins, reorders,
/// or relabels a usage string (plan 008). `meterPercent` drives bar geometry,
/// `severity` drives color — neither becomes visible text.
struct ProviderCardView: View {
    let content: UsageWindowModel.Content
    var onSelectAccount: ((String) -> Void)?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                ForEach(content.detail.rows) { row in
                    detailRow(row)
                }
                if content.accounts.count > 1 {
                    accountSwitcher
                }
            }
            .padding(20)
        }
        .accessibilityElement(children: .contain)
    }

    @ViewBuilder
    private func detailRow(_ row: UsageDetailRow) -> some View {
        switch row.kind {
        case .bucket:
            bucketCard(row)
        default:
            metadataRow(row)
        }
    }

    private func metadataRow(_ row: UsageDetailRow) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(row.label)
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            Spacer(minLength: 8)
            VStack(alignment: .trailing, spacing: 2) {
                ForEach(Array(row.layoutLines.enumerated()), id: \.offset) { _, line in
                    lineView(line, trailingStyle: .primary)
                }
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(row.label) \(row.displayLabel)")
    }

    private func bucketCard(_ row: UsageDetailRow) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(row.label)
                .font(.subheadline.weight(.semibold))
            if let meter = row.meterPercent {
                // Rust geometry only: fill grows left-to-right; color from severity.
                let frac = Double(meter) / 100.0
                GeometryReader { geo in
                    ZStack(alignment: .leading) {
                        Capsule().fill(Color.primary.opacity(0.10))
                        Capsule()
                            .fill(severityTint(row.severity))
                            .frame(width: max(3, geo.size.width * frac))
                    }
                }
                .frame(height: 4)
            }
            ForEach(Array(row.layoutLines.enumerated()), id: \.offset) { _, line in
                lineView(line, trailingStyle: .secondary, leadingTint: severityTint(row.severity))
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background {
            // Content layer — standard material only (HIG: no Liquid Glass here).
            GlassFallbacks.contentCardBackground()
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(row.label) \(row.displayLabel)")
    }

    /// One already-grouped Rust line: leading on the left, trailing right-aligned.
    @ViewBuilder
    private func lineView(
        _ line: UsagePresentationLine,
        trailingStyle: HierarchicalShapeStyle,
        leadingTint: Color? = nil
    ) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            if let leading = line.leading {
                Text(leading)
                    .font(.caption.weight(.semibold))
                    .monospacedDigit()
                    .foregroundStyle(leadingTint ?? .primary)
            }
            if line.leading != nil, line.trailing != nil {
                Spacer(minLength: 8)
            } else if line.trailing != nil {
                Spacer(minLength: 0)
            }
            if let trailing = line.trailing {
                Text(trailing)
                    .font(.caption)
                    .monospacedDigit()
                    .foregroundStyle(trailingStyle)
            }
        }
        .frame(maxWidth: .infinity, alignment: line.leading == nil ? .trailing : .leading)
    }

    /// Multi-account pills: Rust `accountLabel` + selected styling only. No
    /// remaining percentage, no local selection, no heading (plan 008 / N1).
    private var accountSwitcher: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(content.accounts) { account in
                    Button {
                        onSelectAccount?(account.accountKey)
                    } label: {
                        Text(account.accountLabel)
                            .font(.caption.weight(account.selected ? .semibold : .regular))
                            .lineLimit(1)
                            .padding(.horizontal, 10)
                            .padding(.vertical, 7)
                            .background {
                                Capsule(style: .continuous)
                                    .fill(
                                        account.selected
                                            ? Color.accentColor.opacity(0.92)
                                            : Color.primary.opacity(0.06)
                                    )
                            }
                            .foregroundStyle(account.selected ? Color.white : Color.primary)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(
                        "\(account.accountLabel)\(account.selected ? ", selected" : "")"
                    )
                    .accessibilityAddTraits(account.selected ? .isSelected : [])
                }
            }
        }
    }
}
