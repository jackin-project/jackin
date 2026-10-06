// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBridge
import SwiftUI

/// Native provider detail.
///
/// Content remains on standard list surfaces.
public struct ProviderDetailView: View {
    public let content: UsageWindowModel.Content
    public let providerError: String?
    public var onSelectAccount: (String, String) -> Void
    public var onRetry: () -> Void

    public init(
        content: UsageWindowModel.Content,
        providerError: String? = nil,
        onSelectAccount: @escaping (String, String) -> Void = { _, _ in },
        onRetry: @escaping () -> Void = {}
    ) {
        self.content = content
        self.providerError = providerError
        self.onSelectAccount = onSelectAccount
        self.onRetry = onRetry
    }

    public var body: some View {
        List {
            Section {
                providerIdentity
            }

            if content.accounts.count > 1 {
                Section {
                    Picker("Account", selection: accountSelection) {
                        ForEach(content.accounts) { account in
                            Text(account.accountLabel)
                                .tag(account.accountKey)
                        }
                    }
                    .pickerStyle(.menu)
                    .accessibilityLabel("Account")
                    .accessibilityIdentifier("usage.account-picker")
                } header: {
                    sectionHeader("Account")
                }
            }

            let metadataRows = content.detail.rows.filter { $0.kind != .bucket }
            if !metadataRows.isEmpty {
                Section {
                    ForEach(metadataRows) { row in
                        LabeledContent {
                            Text(row.displayLabel)
                                .foregroundStyle(.primary)
                        } label: {
                            Text(row.label)
                                .foregroundStyle(.primary)
                                .accessibilityIdentifier("usage.detail-label.\(row.rowId)")
                        }
                        .accessibilityLabel("\(row.label), \(row.displayLabel)")
                    }
                } header: {
                    sectionHeader("Details")
                }
            }

            let limitRows = content.detail.rows.filter { $0.kind == .bucket }
            if !limitRows.isEmpty {
                Section {
                    ForEach(limitRows) { row in
                        limitRow(row)
                    }
                } header: {
                    sectionHeader("Limits")
                }
            } else if providerError == nil {
                Section {
                    Text("No limit details available")
                        .foregroundStyle(.secondary)
                } header: {
                    sectionHeader("Limits")
                }
            }

            if let providerError {
                Section {
                    Label(providerError, systemImage: "exclamationmark.triangle")
                        .accessibilityIdentifier("usage.provider-error")
                    Button("Retry", action: onRetry)
                        .accessibilityIdentifier("usage.provider-retry")
                } header: {
                    sectionHeader("Provider status")
                }
            }

            if let rawURL = content.usageURL, let url = URL(string: rawURL) {
                Section {
                    Link(destination: url) {
                        Label("Open provider usage page", systemImage: "arrow.up.right")
                    }
                    .accessibilityIdentifier("usage.open-provider-page")
                }
            }
        }
        .listStyle(.inset)
        .accessibilityLabel("\(content.displayLabel) usage details")
        .accessibilityIdentifier("usage.provider.\(content.surfaceId)")
    }

    private var providerIdentity: some View {
        HStack(spacing: 12) {
            if let iconKey = content.iconKey,
                let mark = ProviderMarks.swiftUIImage(forIconKey: iconKey)
            {
                mark
                    .resizable()
                    .scaledToFit()
                    .frame(width: 32, height: 32)
                    .accessibilityHidden(true)
            } else if let fallbackGlyph = content.fallbackGlyph {
                Text(fallbackGlyph)
                    .font(.headline)
                    .accessibilityHidden(true)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(content.identity.providerTitle)
                    .font(.title2)
                Text(content.identity.accountLabel)
                    .foregroundStyle(.primary)
                    .accessibilityIdentifier("usage.provider-account")
                Text(content.identity.activityLabel)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .accessibilityIdentifier("usage.provider-activity")
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(content.identity.accessibilityLabel)
        .accessibilityIdentifier("usage.provider-identity")
    }

    private var accountSelection: Binding<String> {
        Binding(
            get: {
                content.accounts.first(where: \.selected)?.accountKey
                    ?? content.accounts.first?.accountKey
                    ?? ""
            },
            set: { onSelectAccount(content.surfaceId, $0) }
        )
    }

    private func limitRow(_ row: UsageDetailRow) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            LabeledContent(row.label) {
                Text(row.layoutLines.first?.leading ?? row.displayLabel)
                    .monospacedDigit()
                    .foregroundStyle(.primary)
            }
            if let percent = row.meterPercent {
                ProgressView(value: Double(percent), total: 100)
                    .tint(severityTint(row.severity))
                    .accessibilityHidden(true)
            }
            ForEach(Array(row.layoutLines.dropFirst().enumerated()), id: \.offset) { _, line in
                if let value = line.leading ?? line.trailing {
                    Text(value)
                        .font(.callout)
                        .foregroundStyle(.primary)
                }
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityRepresentation {
            Text(row.displayLabel)
                .accessibilityLabel("\(row.label), \(row.displayLabel)")
                .accessibilityIdentifier("usage.limit.\(row.rowId)")
        }
    }

    private func sectionHeader(_ title: String) -> some View {
        Text(title)
            .foregroundStyle(.primary)
            .accessibilityLabel(title)
            .accessibilityIdentifier(
                "usage.section.\(title.lowercased().replacingOccurrences(of: " ", with: "-"))"
            )
    }
}
