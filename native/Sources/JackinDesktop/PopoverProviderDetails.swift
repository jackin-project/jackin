// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBridge
import SwiftUI

private struct ProviderScrollReset: Equatable {
    let presentationSequence: UInt64
    let accountLabel: String
}

@MainActor
struct PopoverProviderDetails: View {
    @ObservedObject var store: PresentationStore
    let provider: GlanceProviderRow
    @ObservedObject var presentationState: PopoverPresentationState
    @State private var scrollPosition = ScrollPosition(edge: .top)

    var body: some View {
        let surface = store.surfaces.first { $0.id == provider.surfaceId }
        let rows = surface?.detailPresentation.rows ?? []
        let metadataRows = rows.filter { $0.kind != .bucket }
        let limitRows = rows.filter { $0.kind == .bucket }
        let reset = ProviderScrollReset(
            presentationSequence: presentationState.sequence,
            accountLabel: provider.accountLabel
        )

        Form {
            Section { providerIdentity }
            limitsSection(limitRows, surface: surface)
            detailsSection(metadataRows)
            errorSection(surface)
        }
        .formStyle(.grouped)
        .scrollPosition($scrollPosition)
        .defaultScrollAnchor(.top, for: .initialOffset)
        .task(id: reset) { await resetScroll(ifNeededFor: reset) }
        .accessibilityLabel("\(provider.displayLabel) usage details")
        .accessibilityIdentifier("popover.provider.\(provider.surfaceId)")
    }

    private var providerIdentity: some View {
        HStack(spacing: 10) {
            if let mark = ProviderMarks.swiftUIImage(forIconKey: provider.iconKey) {
                mark
                    .resizable()
                    .scaledToFit()
                    .frame(width: 28, height: 28)
                    .accessibilityHidden(true)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(provider.displayLabel).font(.headline)
                Text(provider.accountLabel)
                    .foregroundStyle(.primary)
                    .accessibilityIdentifier("popover.provider-account")
                Text(provider.activityLabel)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .accessibilityIdentifier("popover.provider-activity")
            }
            Spacer()
            if provider.isRefreshing {
                ProgressView()
                    .controlSize(.small)
                    .accessibilityLabel(provider.activityLabel)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(provider.accessibilityLabel)
        .accessibilityIdentifier("popover.provider-identity")
    }

    @ViewBuilder
    private func limitsSection(
        _ rows: [UsageDetailRow],
        surface: SurfaceRow?
    ) -> some View {
        if !rows.isEmpty {
            Section {
                ForEach(rows) { limitRow($0) }
            } header: {
                sectionHeader("Limits")
            }
        } else if surface?.lastError == nil {
            Section {
                Text("No limit details available").foregroundStyle(.secondary)
            } header: {
                sectionHeader("Limits")
            }
        }
    }

    @ViewBuilder
    private func detailsSection(_ rows: [UsageDetailRow]) -> some View {
        if !rows.isEmpty {
            Section {
                ForEach(rows) { row in
                    LabeledContent {
                        Text(row.displayLabel).foregroundStyle(.primary)
                    } label: {
                        Text(row.label)
                            .foregroundStyle(.primary)
                            .accessibilityIdentifier("popover.detail-label.\(row.rowId)")
                    }
                    .accessibilityLabel("\(row.label), \(row.displayLabel)")
                    .accessibilityIdentifier("popover.detail.\(row.rowId)")
                }
            } header: {
                sectionHeader("Details")
            }
        }
    }

    @ViewBuilder
    private func errorSection(_ surface: SurfaceRow?) -> some View {
        if let error = surface?.lastError ?? provider.lastError {
            Section {
                Label(error, systemImage: "exclamationmark.triangle")
                    .accessibilityIdentifier("popover.provider-error")
                Button("Retry") { store.refresh(surfaceId: provider.surfaceId) }
                    .disabled(store.refreshInProgress)
                    .accessibilityIdentifier("popover.provider-retry")
            } header: {
                sectionHeader("Provider status")
            }
        }
    }

    private func resetScroll(ifNeededFor reset: ProviderScrollReset) async {
        guard presentationState.claimScrollReset(accountLabel: reset.accountLabel) else { return }
        await Task.yield()
        scrollPosition.scrollTo(edge: .top)
    }

    private func limitRow(_ row: UsageDetailRow) -> some View {
        VStack(alignment: .leading, spacing: 6) {
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
                    Text(value).font(.caption).foregroundStyle(.primary)
                }
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityRepresentation {
            Text(row.displayLabel)
                .accessibilityLabel("\(row.label), \(row.displayLabel)")
                .accessibilityIdentifier("popover.limit.\(row.rowId)")
        }
    }

    private func sectionHeader(_ title: String) -> some View {
        Text(title).accessibilityLabel(title)
    }
}
