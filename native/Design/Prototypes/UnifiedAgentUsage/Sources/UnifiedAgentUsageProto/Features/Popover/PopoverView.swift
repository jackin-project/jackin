import SwiftUI

// Popover mirrors the incumbent PopoverRoot: brand header, grouped Form
// content (identity, Limits, Details, Provider status), fixed controls row.

/// Focused-provider glance hosted by the real system `NSPopover`.
struct PopoverView: View {
    static let contentSize = CGSize(width: 380, height: 520)

    let store: ProtoStore
    let provider: ProtoProvider
    let onOpenUsage: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            popoverBrandHeader

            Divider()

            content
                .frame(width: 380, height: Self.contentSize.height - 94)
                .clipped()

            Divider()

            controls
                .padding(.horizontal, JackinSpace.sm)
                .frame(height: 48)
        }
        .frame(width: Self.contentSize.width, height: Self.contentSize.height)
    }

    private var popoverBrandHeader: some View {
        JackinBrandSignature(width: 92, height: 24)
            .accessibilityHidden(false)
            .accessibilityLabel("jackin❯ desktop")
            .accessibilityAddTraits(.isHeader)
            .padding(.horizontal, 12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .frame(height: 44)
    }

    @ViewBuilder
    private var content: some View {
        if store.projection.isLoading {
            ProgressView("Loading usage")
                .controlSize(.large)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .accessibilityIdentifier("popover.loading")
        } else if let error = store.projection.globalError {
            ContentUnavailableView {
                Label("Usage unavailable", systemImage: "exclamationmark.triangle")
            } description: {
                Text(error)
            } actions: {
                Button(store.chrome.retryTitle) { store.refresh() }
                    .disabled(store.refreshInProgress)
                    .accessibilityIdentifier("popover.retry")
            }
            .accessibilityIdentifier("popover.global-error")
        } else {
            providerForm(provider)
        }
    }

    private func providerForm(_ provider: ProtoProvider) -> some View {
        ScrollView {
            ProviderDetailSections(
                store: store, provider: provider, identifierPrefix: "popover", compact: true
            )
            .padding(12)
        }
        .background(JackinBrand.stage)
        .accessibilityLabel("\(provider.name) usage details")
        .accessibilityIdentifier("popover.provider.\(provider.key)")
    }

    private var accountSelection: Binding<String> {
        Binding(
            get: { store.account(for: provider)?.key ?? "" },
            set: { store.selectAccount($0, for: provider) })
    }

    // Functional controls in a transient surface: real Liquid Glass button
    // styles (macOS 26.0). One prominent action only — Open Usage, the
    // row's primary. Known macOS 26 defect: .glass buttons show no hover
    // state outside a toolbar (fixed in 27); verified live.
    private var controls: some View {
        HStack(spacing: JackinSpace.sm) {
            GlassEffectContainer(spacing: JackinSpace.xs) {
                HStack(spacing: JackinSpace.xs) {
                    Button {
                        store.refresh()
                    } label: {
                        Label(store.chrome.refreshTitle, systemImage: "arrow.clockwise")
                            .labelStyle(.iconOnly)
                    }
                    .buttonStyle(.glass)
                    .keyboardShortcut("r", modifiers: [.command])
                    .disabled(store.refreshInProgress)
                    .accessibilityLabel(store.chrome.refreshTitle)
                    .accessibilityIdentifier("popover.refresh")
                    .help(store.chrome.refreshTitle)

                    Button {
                        onOpenUsage()
                    } label: {
                        Label(store.chrome.openUsageTitle, systemImage: "macwindow")
                            .labelStyle(.iconOnly)
                    }
                    .buttonStyle(.glassProminent)
                    .keyboardShortcut(.defaultAction)
                    .accessibilityLabel(store.chrome.openUsageTitle)
                    .accessibilityIdentifier("popover.open-usage")
                    .help(store.chrome.openUsageTitle)
                }
            }
            Spacer(minLength: 12)

            if provider.accounts.count > 1 {
                Picker("Account", selection: accountSelection) {
                    ForEach(provider.accounts) { entry in
                        Text(entry.label)
                            .tag(entry.key)
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

/// Settings mirrors the incumbent grouped Form over fixture-backed state.
