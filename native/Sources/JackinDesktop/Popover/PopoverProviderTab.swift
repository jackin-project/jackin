// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBridge
import SwiftUI

/// Provider tab: header (Rust `displayLabel`/`accountLabel`/`updatedLabel`/
/// `planLabel`/`lastError`/`dimmed`), an account chip row when more than one
/// account is known, and the generic bucket list (every `displaySegments`
/// element once, in source order, with meter geometry). The header action only
/// emits `onOpenUsageWindow` (plan 007 binds it).
struct PopoverProviderTab: View {
    let provider: PresentationStore.GlanceProviderRow
    let surface: PresentationStore.SurfaceRow?
    let accounts: [PresentationStore.AccountRow]
    let refreshInProgress: Bool
    let onSelectAccount: (String, String) -> Void
    let onOpenUsageWindow: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Button {
                onOpenUsageWindow(provider.surfaceId)
            } label: {
                HStack {
                    VStack(alignment: .leading, spacing: 1) {
                        Text(provider.displayLabel).font(.headline)
                        Text(provider.accountLabel)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    if refreshInProgress || provider.isRefreshing {
                        ProgressView().controlSize(.small)
                    }
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .opacity(provider.dimmed ? 0.55 : 1)

            if let plan = provider.planLabel {
                Text(plan).font(.caption2).foregroundStyle(.secondary)
            }
            Text(provider.updatedLabel).font(.caption2).foregroundStyle(.tertiary)
            if let error = provider.lastError {
                Text(error).font(.caption2).foregroundStyle(.red)
            }

            if accounts.count > 1 {
                HStack(spacing: 6) {
                    ForEach(accounts) { account in
                        Button {
                            onSelectAccount(provider.surfaceId, account.accountKey)
                        } label: {
                            Text(account.accountLabel)
                                .font(.caption2)
                                .padding(.horizontal, 6)
                                .padding(.vertical, 2)
                                .background(
                                    Capsule().fill(
                                        account.selected
                                            ? Color.accentColor.opacity(0.2)
                                            : Color.secondary.opacity(0.12)
                                    )
                                )
                        }
                        .buttonStyle(.plain)
                    }
                }
            }

            if let surface {
                ForEach(Array(surface.buckets.enumerated()), id: \.offset) { _, bucket in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(bucket.label).font(.caption).foregroundStyle(.secondary)
                        bucketMeter(bucket.meterPercent)
                        ForEach(Array(bucket.displaySegments.enumerated()), id: \.offset) { _, segment in
                            Text(segment).font(.caption2)
                        }
                    }
                }
            }
        }
        .padding(8)
    }

    /// Meter geometry only from Rust `meterPercent` (never inverted/rounded/labeled).
    @ViewBuilder
    private func bucketMeter(_ meterPercent: UInt8?) -> some View {
        GeometryReader { geometry in
            ZStack(alignment: .leading) {
                Capsule().fill(Color.secondary.opacity(0.25))
                if let meterPercent {
                    Capsule()
                        .fill(Color.accentColor)
                        .frame(width: geometry.size.width * CGFloat(meterPercent) / 100.0)
                }
            }
        }
        .frame(height: 4)
    }
}
