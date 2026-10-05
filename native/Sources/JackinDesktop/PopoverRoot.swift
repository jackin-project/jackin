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
    private var onRetryLastOperation: (() -> Void)?
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
        onRetryLastOperation: (() -> Void)? = nil,
        onOpenUsage: ((UsageNavigationContext?) -> Void)? = nil
    ) {
        self.store = store
        self.presentationState = presentationState
        self.onRetryLastOperation = onRetryLastOperation
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
        if store.isOpening, store.providerGroups.isEmpty {
            ProgressView("Loading usage")
                .controlSize(.large)
                .accessibilityIdentifier("popover.loading")
        } else if let error = store.lastError, store.providerGroups.isEmpty {
            ContentUnavailableView {
                Label("Usage unavailable", systemImage: "exclamationmark.triangle")
            } description: {
                Text(error)
            } actions: {
                Button("Retry", action: retryLastOperation)
                    .disabled(store.isOpening || store.refreshInProgress)
                    .accessibilityIdentifier("popover.retry")
            }
            .accessibilityIdentifier("popover.global-error")
        } else if let provider = selectedProvider {
            providerForm(provider)
        } else if store.popoverSelection == nil {
            overviewContent
        } else if let group = store.providerGroups.first(where: {
            $0.surfaceId == store.popoverSelection
        }) {
            ContentUnavailableView {
                Label(group.displayLabel, systemImage: "exclamationmark.triangle")
            } description: {
                Text(group.lastError ?? group.accessibilityLabel)
                if let error = store.lastError, error != group.lastError {
                    Text(error)
                        .accessibilityIdentifier("popover.refresh-error")
                }
            } actions: {
                if store.lastError != nil {
                    Button("Retry refresh", action: retryLastOperation)
                        .disabled(store.isOpening || store.refreshInProgress)
                        .accessibilityIdentifier("popover.refresh-retry")
                }
                Button("Retry") { store.refresh(surfaceId: group.surfaceId) }
                    .disabled(store.isOpening || store.refreshInProgress)
            }
            .accessibilityIdentifier("popover.provider-unavailable")
        } else {
            ContentUnavailableView(
                "Provider unavailable",
                systemImage: "exclamationmark.triangle",
                description: Text(store.popoverSelection ?? "")
            )
            .accessibilityIdentifier("popover.provider-unavailable")
        }
    }

    @ViewBuilder
    private var overviewContent: some View {
        if store.providerGroups.isEmpty {
            ContentUnavailableView(
                "No providers detected",
                systemImage: "chevron.right",
                description: Text(UsageWindowModel.emptyHint)
            )
            .accessibilityIdentifier("popover.empty")
        } else {
            Form {
                Section("Overview") {
                    ForEach(store.providerGroups) { group in
                        let account = store.accounts.first {
                            $0.surfaceId == group.surfaceId && $0.selected
                        }
                        let glance = store.providerGlanceRows.first {
                            $0.surfaceId == group.surfaceId
                        }
                        Button {
                            store.popoverSelection = group.surfaceId
                        } label: {
                            HStack {
                                VStack(alignment: .leading, spacing: 3) {
                                    Text(group.displayLabel)
                                        .foregroundStyle(.primary)
                                    Text(
                                        account?.accountLabel
                                            ?? glance?.accountLabel
                                            ?? group.accessibilityLabel
                                    )
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                    if let activity = glance?.activityLabel {
                                        Text(activity)
                                            .font(.caption)
                                            .foregroundStyle(.secondary)
                                    }
                                }
                                Spacer(minLength: 8)
                                Image(systemName: "chevron.right")
                                    .font(.caption.weight(.semibold))
                                    .foregroundStyle(.tertiary)
                            }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("popover.overview.provider.\(group.surfaceId)")
                    }
                }
            }
            .accessibilityIdentifier("popover.overview")
        }
    }

    private var selectedProvider: StatusPopoverFocus.Content? {
        StatusPopoverFocus.content(
            providerGroups: store.providerGroups,
            surfaces: store.surfaces,
            accounts: store.accounts,
            glanceRows: store.providerGlanceRows,
            selection: store.popoverSelection,
            refreshError: store.lastError
        )
    }

    private func providerForm(_ provider: StatusPopoverFocus.Content) -> some View {
        let surface = provider.surface
        let metadataRows = surface.detailPresentation.rows.filter { $0.kind != .bucket }
        let limitRows = surface.detailPresentation.rows.filter { $0.kind == .bucket }
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
            } else if provider.lastError == nil {
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

            if let error = provider.refreshError {
                Section {
                    if error != provider.lastError {
                        Label(error, systemImage: "exclamationmark.triangle")
                            .accessibilityIdentifier("popover.refresh-error")
                    }
                    Button("Retry", action: retryLastOperation)
                        .disabled(store.isOpening || store.refreshInProgress)
                        .accessibilityIdentifier("popover.refresh-retry")
                } header: {
                    sectionHeader("Refresh status")
                }
            }

            if let error = provider.lastError {
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

    private func providerIdentity(_ provider: StatusPopoverFocus.Content) -> some View {
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
        provider: StatusPopoverFocus.Content
    ) -> Binding<String> {
        Binding(
            get: { accounts.first(where: \.selected)?.accountKey ?? "" },
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

    private func retryLastOperation() {
        if let onRetryLastOperation {
            onRetryLastOperation()
        } else {
            store.retryLastOperation()
        }
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
                let reselectionNotice = store.accountSelectionReselectionNotice(
                    surfaceId: provider.surfaceId
                )
                if accounts.count > 1 || reselectionNotice != nil {
                    Picker(
                        "Account",
                        selection: accountSelection(accounts, provider: provider)
                    ) {
                        if !accounts.contains(where: \.selected) {
                            Text(
                                reselectionNotice
                                    ?? store.surfaces.first(where: { $0.id == provider.surfaceId })?
                                        .lastError
                                    ?? provider.lastError ?? provider.activityLabel
                            )
                            .tag("")
                            .disabled(true)
                        }
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
