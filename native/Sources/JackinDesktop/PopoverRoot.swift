// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBridge
import SwiftUI

@MainActor
final class PopoverPresentationState: ObservableObject {
    @Published private(set) var sequence: UInt64 = 0
    private var lastScrollResetSequence: UInt64?
    private var lastScrollResetAccountLabel: String?

    func beginPresentation() {
        sequence &+= 1
    }

    func claimScrollReset(accountLabel: String) -> Bool {
        guard lastScrollResetSequence != sequence || lastScrollResetAccountLabel != accountLabel
        else { return false }
        lastScrollResetSequence = sequence
        lastScrollResetAccountLabel = accountLabel
        return true
    }
}

private struct ProviderScrollReset: Equatable {
    let presentationSequence: UInt64
    let accountLabel: String
}

private enum PopoverQIFullPlateKey: EnvironmentKey {
    static let defaultValue = false
}

extension EnvironmentValues {
    public var popoverQIFullPlate: Bool {
        get { self[PopoverQIFullPlateKey.self] }
        set { self[PopoverQIFullPlateKey.self] = newValue }
    }
}

/// Focused-provider glance hosted by the real system `NSPopover`.
public struct PopoverRoot: View {
    public static let liveContentSize = CGSize(width: 380, height: 520)

    @ObservedObject public var store: PresentationStore
    @ObservedObject private var presentationState: PopoverPresentationState
    @State private var providerScrollPosition = ScrollPosition(edge: .top)
    public var onOpenUsage: ((UsageNavigationContext?) -> Void)?
    @Environment(\.popoverQIFullPlate) private var qiFullPlate

    public init(
        store: PresentationStore,
        onOpenUsage: ((UsageNavigationContext?) -> Void)? = nil
    ) {
        self.store = store
        self.presentationState = PopoverPresentationState()
        self.onOpenUsage = onOpenUsage
    }

    init(
        store: PresentationStore,
        presentationState: PopoverPresentationState,
        onOpenUsage: ((UsageNavigationContext?) -> Void)? = nil
    ) {
        self.store = store
        self.presentationState = presentationState
        self.onOpenUsage = onOpenUsage
    }

    public var body: some View {
        let height: CGFloat = qiFullPlate ? 1_100 : 520
        VStack(spacing: 0) {
            popoverBrandHeader

            Divider()

            content
                .frame(width: 380, height: height - 94)
                .clipped()

            Divider()

            controls
                .padding(.horizontal, 12)
                .frame(height: 48)
        }
        .frame(width: 380, height: height)
    }

    private var popoverBrandHeader: some View {
        JackinBrandSignature(width: 92, height: 24)
            .accessibilityHidden(false)
            .accessibilityLabel("jackin❯ desktop")
            .accessibilityAddTraits(.isHeader)
            .frame(maxWidth: .infinity)
            .frame(height: 44)
            .overlay(alignment: .trailing) {
                if store.usesFixture {
                    Text("Fixture")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.primary)
                        .padding(.trailing, 12)
                        .accessibilityIdentifier("popover.fixture-badge")
                }
            }
    }

    @ViewBuilder
    private var content: some View {
        if store.isOpening, store.providerGlanceRows.isEmpty {
            ProgressView("Loading usage")
                .controlSize(.large)
                .accessibilityIdentifier("popover.loading")
        } else if let error = store.lastError, store.providerGlanceRows.isEmpty {
            ContentUnavailableView {
                Label("Usage unavailable", systemImage: "exclamationmark.triangle")
            } description: {
                Text(error)
            } actions: {
                Button("Retry") { store.retryLastOperation() }
                    .disabled(store.isOpening || store.refreshInProgress)
                    .accessibilityIdentifier("popover.retry")
            }
            .accessibilityIdentifier("popover.global-error")
        } else if let provider = selectedProvider {
            providerForm(provider)
        } else {
            ContentUnavailableView(
                "No providers detected",
                systemImage: "chevron.right",
                description: Text(UsageWindowModel.emptyHint)
            )
            .accessibilityIdentifier("popover.empty")
        }
    }

    private var selectedProvider: PresentationStore.GlanceProviderRow? {
        if let selection = store.popoverSelection,
            let match = store.providerGlanceRows.first(where: { $0.surfaceId == selection })
        {
            return match
        }
        return store.providerGlanceRows.first
    }

    private func providerForm(_ provider: PresentationStore.GlanceProviderRow) -> some View {
        let surface = store.surfaces.first { $0.id == provider.surfaceId }
        let metadataRows = surface?.detailPresentation.rows.filter { $0.kind != .bucket } ?? []
        let limitRows = surface?.detailPresentation.rows.filter { $0.kind == .bucket } ?? []
        let scrollReset = ProviderScrollReset(
            presentationSequence: presentationState.sequence,
            accountLabel: provider.accountLabel
        )

        return Form {
            Section {
                providerIdentity(provider)
            }

            if !limitRows.isEmpty {
                Section {
                    ForEach(limitRows) { row in
                        limitRow(row)
                    }
                } header: {
                    sectionHeader("Limits")
                }
            } else if surface?.lastError == nil {
                Section {
                    Text("No limit details available")
                        .foregroundStyle(.secondary)
                } header: {
                    sectionHeader("Limits")
                }
            }

            if !metadataRows.isEmpty {
                Section {
                    ForEach(metadataRows) { row in
                        LabeledContent {
                            Text(row.displayLabel)
                                .foregroundStyle(.primary)
                        } label: {
                            Text(row.label)
                                .foregroundStyle(.primary)
                                .accessibilityIdentifier(
                                    "popover.detail-label.\(row.rowId)"
                                )
                        }
                        .accessibilityLabel("\(row.label), \(row.displayLabel)")
                        .accessibilityIdentifier("popover.detail.\(row.rowId)")
                    }
                } header: {
                    sectionHeader("Details")
                }
            }

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
        .formStyle(.grouped)
        .scrollPosition($providerScrollPosition)
        .defaultScrollAnchor(.top, for: .initialOffset)
        .task(id: scrollReset) {
            await resetProviderScrollPosition(ifNeededFor: scrollReset)
        }
        .accessibilityLabel("\(provider.displayLabel) usage details")
        .accessibilityIdentifier("popover.provider.\(provider.surfaceId)")
    }

    private func resetProviderScrollPosition(ifNeededFor reset: ProviderScrollReset) async {
        guard presentationState.claimScrollReset(accountLabel: reset.accountLabel) else { return }
        await Task.yield()
        providerScrollPosition.scrollTo(edge: .top)
    }

    private func providerIdentity(_ provider: PresentationStore.GlanceProviderRow) -> some View {
        HStack(spacing: 10) {
            if let mark = ProviderMarks.swiftUIImage(forIconKey: provider.iconKey) {
                mark
                    .resizable()
                    .scaledToFit()
                    .frame(width: 28, height: 28)
                    .accessibilityHidden(true)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(provider.displayLabel)
                    .font(.headline)
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

    private func accountSelection(
        _ accounts: [PresentationStore.AccountRow],
        provider: PresentationStore.GlanceProviderRow
    ) -> Binding<String> {
        Binding(
            get: { accounts.first(where: \.selected)?.accountKey ?? accounts[0].accountKey },
            set: { store.setSelectedAccount(surfaceId: provider.surfaceId, accountKey: $0) }
        )
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
                    Text(value)
                        .font(.caption)
                        .foregroundStyle(.primary)
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
        Text(title)
            .accessibilityLabel(title)
    }

    private var controls: some View {
        HStack(spacing: 12) {
            HStack(spacing: 4) {
                Button {
                    if let id = selectedProvider?.surfaceId {
                        store.refresh(surfaceId: id)
                    } else {
                        store.refreshAll()
                    }
                } label: {
                    Label("Refresh", systemImage: "arrow.clockwise")
                        .labelStyle(.iconOnly)
                }
                .keyboardShortcut("r", modifiers: [.command])
                .disabled(store.refreshInProgress)
                .accessibilityLabel("Refresh")
                .accessibilityIdentifier("popover.refresh")
                .help("Refresh")

                Button {
                    guard let provider = selectedProvider else {
                        onOpenUsage?(nil)
                        return
                    }
                    let accountKey = store.accountsForSurface(provider.surfaceId)
                        .first(where: \.selected)?.accountKey
                    onOpenUsage?(
                        UsageNavigationContext(
                            surfaceId: provider.surfaceId,
                            accountKey: accountKey
                        )
                    )
                } label: {
                    Label("Open Usage", systemImage: "macwindow")
                        .labelStyle(.iconOnly)
                }
                .keyboardShortcut(.defaultAction)
                .accessibilityLabel("Open Usage")
                .accessibilityIdentifier("popover.open-usage")
                .help("Open Usage")
            }
            Spacer(minLength: 12)

            if let provider = selectedProvider {
                let accounts = store.accountsForSurface(provider.surfaceId)
                if accounts.count > 1 {
                    Picker(
                        "Account",
                        selection: accountSelection(accounts, provider: provider)
                    ) {
                        ForEach(accounts) { account in
                            Text(account.accountLabel)
                                .tag(account.accountKey)
                        }
                    }
                    .pickerStyle(.menu)
                    .labelsHidden()
                    .frame(width: 220, alignment: .trailing)
                    .accessibilityLabel("Account")
                    .accessibilityIdentifier("popover.account-picker")
                    .help("Choose account")
                }
            }
        }
    }
}
