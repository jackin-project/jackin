// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBridge
import SwiftUI

/// Full Capsule-parity provider card (S4). All strings are Rust-owned.
struct ProviderCardView: View {
    let surface: PresentationStore.SurfaceRow
    /// Settings percent style (`left` / `used`) — matches menu-bar chips.
    var percentStyle: String = "left"
    /// Multi-account rows for this surface (empty when single host login).
    var accounts: [PresentationStore.AccountRow] = []
    var onSelectAccount: ((String) -> Void)?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                identityBlock
                if accounts.count > 1 {
                    accountSwitcher
                }
                ForEach(surface.buckets) { bucket in
                    metricCard(bucket)
                }
                if surface.buckets.isEmpty {
                    Text("— No data")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .padding(12)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background {
                            RoundedRectangle(cornerRadius: 10)
                                .fill(.background.secondary)
                        }
                }
                if let caption = surface.estimateCaption {
                    Text(caption)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                if let err = surface.lastError {
                    Text(err)
                        .font(.caption)
                        .foregroundStyle(.orange)
                }
            }
            .padding(20)
            .opacity(surface.status == "stale" || surface.status == "unavailable" ? 0.85 : 1.0)
        }
        .accessibilityElement(children: .contain)
    }

    private var identityBlock: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline) {
                Text(surface.label)
                    .font(.title2.weight(.semibold))
                if let symbol = statusBadgeSymbol(surface.status) {
                    Image(systemName: symbol)
                        .foregroundStyle(badgeTint(surface.status))
                        .accessibilityLabel(surface.status)
                }
                Spacer(minLength: 8)
                if !surface.accountLabel.isEmpty {
                    HStack(spacing: 2) {
                        Text(surface.accountLabel)
                        if let user = surface.username, !user.isEmpty {
                            Text("(\(user))")
                                .foregroundStyle(.secondary)
                        }
                    }
                    .font(.subheadline)
                }
            }
            HStack {
                Text(surface.updatedLabel.isEmpty ? "—" : surface.updatedLabel)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
                if let plan = surface.planLabel {
                    Text(plan)
                        .font(.caption.weight(.medium))
                        .padding(.horizontal, 8)
                        .padding(.vertical, 2)
                        .background {
                            GlassFallbacks.statusChipBackground(tint: .secondary)
                        }
                }
            }
            if let origin = surface.credentialOrigin, !origin.isEmpty {
                Text("Auth: \(origin)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }

    /// OpenUsage/CodexBar-style account pills (multi-account per provider).
    private var accountSwitcher: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Accounts")
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(accounts) { account in
                        Button {
                            onSelectAccount?(account.accountKey)
                        } label: {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(account.accountLabel)
                                    .font(.caption.weight(account.selected ? .semibold : .regular))
                                    .lineLimit(1)
                                if let rem = account.remainingPercent {
                                    Text(
                                        statusItemPercentToken(
                                            remainingPercent: rem,
                                            percentStyle: percentStyle
                                        )
                                    )
                                    .font(.caption2.monospacedDigit())
                                    .foregroundStyle(account.selected ? Color.white.opacity(0.9) : .secondary)
                                }
                            }
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

    @ViewBuilder
    private func metricCard(_ bucket: PresentationStore.BucketRow) -> some View {
        // OpenUsage metric anatomy: title, thin full-width bar, value left / reset right.
        VStack(alignment: .leading, spacing: 6) {
            Text(bucket.label)
                .font(.subheadline.weight(.semibold))
            switch bucketRowShape(
                remainingPercent: bucket.remainingPercent,
                usedLabel: bucket.usedLabel
            ) {
            case .gauge:
                if let remaining = bucket.remainingPercent {
                    // OpenUsage: remaining fill grows left-to-right (full bar = healthy).
                    let remainingFrac = Double(remaining) / 100.0
                    GeometryReader { geo in
                        ZStack(alignment: .leading) {
                            Capsule().fill(Color.primary.opacity(0.10))
                            Capsule()
                                .fill(severityTint(bucket.severity))
                                .frame(width: max(3, geo.size.width * remainingFrac))
                        }
                    }
                    .frame(height: 4)
                    .accessibilityLabel(
                        "\(bucket.label) \(metricPrimaryLabel(bucket))"
                    )
                }
                // OpenUsage: primary remaining/used (or depleted reset) · reset right.
                HStack(alignment: .firstTextBaseline) {
                    Text(metricPrimaryLabel(bucket))
                        .font(.caption.weight(.semibold))
                        .monospacedDigit()
                        .foregroundStyle(severityTint(bucket.severity))
                    Spacer(minLength: 8)
                    if let remaining = bucket.remainingPercent, remaining > 0,
                       let reset = bucket.resetLabel, !reset.isEmpty
                    {
                        Text(reset)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .monospacedDigit()
                    }
                }
            case .valueOnly:
                HStack {
                    Text(bucket.usedLabel ?? "—")
                        .font(.caption)
                        .monospacedDigit()
                    Spacer()
                    if let reset = bucket.resetLabel {
                        Text(reset)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            case .empty:
                Text("— No data")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            // CodexBar dual-column pace ("On pace" · "Runs out in …").
            if let pace = bucket.paceLabel, !pace.isEmpty {
                let parts = splitPaceLabel(pace)
                if parts.count >= 2 {
                    HStack(alignment: .firstTextBaseline) {
                        Text(parts[0])
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                            .monospacedDigit()
                        Spacer(minLength: 8)
                        Text(parts[1])
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.trailing)
                    }
                } else {
                    Text(pace)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }
            if let limit = bucket.limitLabel, !limit.isEmpty {
                Text(limit)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background {
            // Content layer — standard material only (HIG: no Liquid Glass here).
            GlassFallbacks.contentCardBackground()
        }
        .accessibilityElement(children: .combine)
    }

    /// Primary metric line: remaining/used % or depleted Rust reset countdown.
    private func metricPrimaryLabel(_ bucket: PresentationStore.BucketRow) -> String {
        bucketMetricPrimaryLabel(
            remainingPercent: bucket.remainingPercent,
            usedLabel: bucket.usedLabel,
            resetLabel: bucket.resetLabel,
            percentStyle: percentStyle
        )
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
