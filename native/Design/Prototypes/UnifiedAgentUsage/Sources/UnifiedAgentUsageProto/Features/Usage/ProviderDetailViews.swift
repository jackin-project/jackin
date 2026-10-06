import SwiftUI

struct ProviderDetailView: View {
    let store: ProtoStore
    let provider: ProtoProvider

    var body: some View {
        ScrollView {
            ProviderDetailSections(
                store: store, provider: provider, identifierPrefix: "usage"
            )
            .frame(maxWidth: 760)
            .padding(.horizontal, 28)
            .padding(.vertical, 28)
            .frame(maxWidth: .infinity)
        }
        .background(JackinStageBackground())
        .accessibilityLabel("\(provider.name) usage details")
        .accessibilityIdentifier("usage.provider.\(provider.key)")
    }
}

/// Shared content projection for the Usage detail and transient popover.
///
/// Each host supplies its system-owned List/Form material; this view owns the
/// content once so labels, ordering, states, and actions cannot drift.
struct ProviderDetailSections: View {
    let store: ProtoStore
    let provider: ProtoProvider
    let identifierPrefix: String
    var compact = false

    var body: some View {
        let account = store.account(for: provider)

        VStack(alignment: .leading, spacing: compact ? 16 : 28) {
            if compact {
                compactProviderIdentity(account)
                compactLimits(account)
            } else {
                providerIdentity(account)
                if let account {
                    currentPressure(account, providerState: provider.state)
                }
                fullLimits(account)
            }

            if !compact {
                TechnicalPanel(title: "Account", detail: "Credential source") {
                    if let username = account?.username {
                        DetailFactItem(
                            icon: "person.text.rectangle", label: "Username", value: username)
                        Divider()
                    }
                    if let plan = account?.plan {
                        DetailFactItem(
                            icon: "checkmark.seal", label: "Plan", value: plan)
                        if account?.auth != nil { Divider() }
                    }
                    if let auth = account?.auth {
                        DetailFactItem(
                            icon: "key", label: "Authentication", value: auth)
                    }
                }
            }

            if let error = provider.errorText {
                TechnicalPanel(title: "Provider status", detail: "Attention required") {
                    VStack(alignment: .leading, spacing: JackinSpace.sm) {
                        Label(error, systemImage: "exclamationmark.triangle")
                            .foregroundStyle(JackinBrand.warning)
                            .accessibilityIdentifier("\(identifierPrefix).provider-error")
                        if let ago = provider.updatedAgo {
                            Text(ago)
                                .font(JackinType.metadata)
                                .foregroundStyle(JackinBrand.muted)
                        }
                        Button(store.chrome.retryTitle) { store.refresh() }
                            .disabled(store.refreshInProgress)
                            .accessibilityIdentifier("\(identifierPrefix).provider-retry")
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func fullLimits(_ account: ProtoAccount?) -> some View {
        VStack(alignment: .leading, spacing: JackinSpace.sm) {
            technicalSectionHeader("Limits", detail: "\(account?.windows.count ?? 0) limits")
            if let account, !account.windows.isEmpty {
                LazyVGrid(
                    columns: [GridItem(.adaptive(minimum: 260), spacing: JackinSpace.sm)],
                    spacing: JackinSpace.sm
                ) {
                    ForEach(ProtoQuotaOrdering.ordered(account.windows)) { window in
                        LimitModule(
                            window: window,
                            identifierPrefix: "\(identifierPrefix).limit")
                    }
                }
            } else if provider.errorText == nil {
                Text("No limit details available")
                    .foregroundStyle(JackinBrand.muted)
            }
        }
    }

    @ViewBuilder
    private func compactLimits(_ account: ProtoAccount?) -> some View {
        if let account, !account.windows.isEmpty {
            let ranked = ProtoQuotaOrdering.ordered(account.windows)
            VStack(alignment: .leading, spacing: JackinSpace.sm) {
                technicalSectionHeader("Limits", detail: "\(account.windows.count) limits")
                LimitModule(
                    window: ranked[0],
                    identifierPrefix: "\(identifierPrefix).limit")
                ForEach(ranked.dropFirst().prefix(2)) { window in
                    CompactLimitRow(
                        window: window,
                        identifierPrefix: "\(identifierPrefix).limit")
                }
                if account.windows.count > 3 {
                    Text("+\(account.windows.count - 3) more limits in Usage")
                        .font(JackinType.metadata)
                        .foregroundStyle(JackinBrand.muted)
                        .frame(maxWidth: .infinity, alignment: .center)
                        .padding(.top, JackinSpace.xxs)
                }
            }
        } else if provider.errorText == nil {
            Text("No limit details available")
                .foregroundStyle(JackinBrand.muted)
        }
    }

    private func technicalSectionHeader(_ title: String, detail: String) -> some View {
        HStack(alignment: .firstTextBaseline) {
            Text(title.uppercased())
                .font(JackinType.sectionTitle)
                .tracking(0.45)
            Spacer()
            Text(detail)
                .font(JackinType.tertiary)
                .foregroundStyle(JackinBrand.quiet)
        }
    }

    private func providerIdentity(_ account: ProtoAccount?) -> some View {
        ViewThatFits(in: .horizontal) {
            HStack(alignment: .center, spacing: JackinSpace.md) {
                providerIdentityMark
                providerIdentityCopy(account)
                Spacer(minLength: JackinSpace.md)
                providerActivity
            }
            .fixedSize(horizontal: true, vertical: false)

            VStack(alignment: .leading, spacing: JackinSpace.sm) {
                HStack(alignment: .top, spacing: JackinSpace.md) {
                    providerIdentityMark
                    providerIdentityCopy(account)
                }
                providerActivity
                    .padding(.leading, 60)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(
            "\(provider.name), \(account?.label ?? ""), \(provider.activityLabel)"
        )
        .accessibilityIdentifier("\(identifierPrefix).provider-identity")
    }

    private var providerIdentityMark: some View {
        BrandMarkChip(
            iconKey: provider.iconKey, fallbackGlyph: provider.fallbackGlyph,
            markSize: 28, chipSize: 44)
    }

    private func providerIdentityCopy(_ account: ProtoAccount?) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            Text("PROVIDER")
                .font(JackinType.technicalLabel)
                .tracking(0.45)
                .foregroundStyle(JackinBrand.quiet)
            Text(provider.name)
                .font(.title2.weight(.semibold))
            if let account {
                Text([account.label, account.plan].joined(separator: "  ·  "))
                    .font(.callout)
                    .foregroundStyle(JackinBrand.muted)
                    .accessibilityIdentifier("\(identifierPrefix).provider-account")
            }
        }
    }

    @ViewBuilder
    private var providerActivity: some View {
        if provider.isRefreshing || store.refreshInProgress {
            ProgressView()
                .controlSize(.small)
                .accessibilityLabel(provider.activityLabel)
        } else {
            Label(provider.activityLabel, systemImage: provider.state.symbol)
                .font(JackinType.metadata)
                .foregroundStyle(JackinBrand.muted)
                .labelStyle(.titleAndIcon)
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("\(identifierPrefix).provider-activity")
        }
    }

    private func compactProviderIdentity(_ account: ProtoAccount?) -> some View {
        VStack(alignment: .leading, spacing: JackinSpace.sm) {
            HStack(spacing: JackinSpace.sm) {
                BrandMarkChip(
                    iconKey: provider.iconKey, fallbackGlyph: provider.fallbackGlyph,
                    markSize: 24, chipSize: 40)
                VStack(alignment: .leading, spacing: JackinSpace.xxs) {
                    Text(provider.name)
                        .font(.headline)
                    if let account {
                        Text(account.label)
                            .font(.callout)
                            .foregroundStyle(JackinBrand.muted)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                }
            }
            Label(provider.activityLabel, systemImage: provider.state.symbol)
                .font(JackinType.metadata)
                .foregroundStyle(JackinBrand.muted)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(
            "\(provider.name), \(account?.label ?? ""), \(provider.activityLabel)")
    }

    private func currentPressure(
        _ account: ProtoAccount, providerState: ProtoState
    ) -> some View {
        let effectiveState = providerState == .current ? account.state : providerState
        return HStack(spacing: 0) {
            pressureFact(
                "CURRENT PRESSURE",
                account.remaining.map { "\($0)% left" } ?? "Unavailable",
                tint: pressureTint(effectiveState))
            Divider().frame(height: 38)
            pressureFact("NEXT RESET", account.resetText ?? "Unavailable")
            Divider().frame(height: 38)
            pressureFact("STATE", effectiveState.label ?? "Available")
        }
        .padding(.vertical, 14)
        .background(
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .fill(JackinBrand.inset)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .strokeBorder(JackinBrand.separator, lineWidth: 1)
        )
    }

    private func pressureFact(_ label: String, _ value: String, tint: Color = .primary) -> some View
    {
        VStack(alignment: .leading, spacing: 5) {
            Text(label)
                .font(JackinType.technicalLabel)
                .tracking(0.45)
                .foregroundStyle(JackinBrand.quiet)
            Text(value)
                .font(.callout.weight(.semibold))
                .monospacedDigit()
                .foregroundStyle(tint)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(.horizontal, 16)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func pressureTint(_ state: ProtoState) -> Color {
        switch state {
        case .warning: JackinBrand.warning
        case .danger, .depleted: JackinBrand.danger
        default: .primary
        }
    }
}

/// Raised technical dossier panel. The native window/popover remains the
/// structural glass host; authored quota content stays opaque and precise.
private struct TechnicalPanel<Content: View>: View {
    let title: String
    let detail: String
    @ViewBuilder let content: Content

    init(
        title: String, detail: String,
        @ViewBuilder content: () -> Content
    ) {
        self.title = title
        self.detail = detail
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .firstTextBaseline) {
                Text(title.uppercased())
                    .font(JackinType.sectionTitle)
                    .tracking(0.45)
                    .foregroundStyle(.primary)
                Spacer()
                Text(detail)
                    .font(JackinType.tertiary)
                    .foregroundStyle(JackinBrand.quiet)
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 14)

            Divider()

            VStack(alignment: .leading, spacing: 0) {
                content
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 16)
        }
        .background(
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .fill(JackinBrand.card)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .strokeBorder(JackinBrand.separator, lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
    }
}

private struct LimitModule: View {
    let window: ProtoQuotaWindow
    let identifierPrefix: String

    var body: some View {
        LimitRowView(window: window, identifierPrefix: identifierPrefix)
            .padding(.horizontal, 16)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .background(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .fill(JackinBrand.card)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .strokeBorder(JackinBrand.separator, lineWidth: 1)
            )
    }
}

private struct CompactLimitRow: View {
    let window: ProtoQuotaWindow
    let identifierPrefix: String

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline, spacing: JackinSpace.xs) {
                Text(window.label)
                    .font(.callout.weight(.medium))
                    .lineLimit(1)
                Spacer()
                Text(window.primaryValue)
                    .font(.callout.weight(.semibold))
                    .monospacedDigit()
            }
            if let reset = window.resetLabel {
                Text(reset)
                    .font(JackinType.metadata)
                    .monospacedDigit()
                    .foregroundStyle(JackinBrand.muted)
            }
        }
        .padding(12)
        .background(
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .fill(JackinBrand.inset)
        )
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(window.accessibilitySummary)
        .accessibilityIdentifier("\(identifierPrefix).\(window.stableID)")
    }
}

/// One limit-window row — shared by the Usage-window detail and the popover
/// so both surfaces render the same Rust-owned fields identically (DRY).
struct LimitRowView: View {
    let window: ProtoQuotaWindow
    /// Accessibility identifier prefix (`usage.limit` / `popover.limit`).
    var identifierPrefix = "usage.limit"

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .firstTextBaseline, spacing: JackinSpace.sm) {
                Text(window.label.uppercased())
                    .font(JackinType.technicalLabel)
                    .tracking(0.45)
                    .foregroundStyle(JackinBrand.quiet)
                Spacer()
                if let reset = window.resetLabel {
                    Text(reset)
                        .font(JackinType.metadata)
                        .monospacedDigit()
                        .foregroundStyle(JackinBrand.muted)
                }
            }
            HStack(alignment: .firstTextBaseline, spacing: JackinSpace.sm) {
                Text(window.primaryValue)
                    .font(JackinType.detailMetric)
                    .monospacedDigit()
                    .foregroundStyle(window.notStarted ? .secondary : .primary)
                Spacer()
                if let secondary = window.secondaryValue {
                    Text(secondary)
                        .font(JackinType.metadata)
                        .monospacedDigit()
                        .foregroundStyle(JackinBrand.muted)
                }
            }
            if let meter = window.meter {
                QuotaMeter(percent: meter, tint: meterTint(window.state))
                    .accessibilityHidden(true)
            }
            HStack(alignment: .firstTextBaseline, spacing: JackinSpace.sm) {
                if let supplemental = window.supplementalValue {
                    Text(supplemental)
                }
                Spacer(minLength: 0)
                if let pace = window.pace {
                    Text(pace)
                }
            }
            .font(JackinType.metadata)
            .foregroundStyle(JackinBrand.quiet)
        }
        .padding(.vertical, 16)
        .accessibilityElement(children: .ignore)
        .accessibilityRepresentation {
            Text(window.display)
                .accessibilityLabel(window.accessibilitySummary)
                .accessibilityIdentifier("\(identifierPrefix).\(window.stableID)")
        }
    }
}

/// A readable account fact, deliberately stacked instead of compressed into a
/// small two-column table. Long provider values receive the full content width.
private struct DetailFactItem: View {
    let icon: String
    let label: String
    let value: String

    var body: some View {
        HStack(alignment: .top, spacing: JackinSpace.sm) {
            Image(systemName: icon)
                .foregroundStyle(JackinBrand.muted)
                .frame(width: 18)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: JackinSpace.xxs) {
                Text(label.uppercased())
                    .font(JackinType.technicalLabel)
                    .tracking(0.45)
                    .foregroundStyle(JackinBrand.quiet)
                Text(value)
                    .foregroundStyle(.primary)
                    .textSelection(.enabled)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(label), \(value)")
        .padding(.vertical, 12)
    }
}

struct DetailRootView: View {
    let store: ProtoStore
    let onOpenSettings: () -> Void
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        ZStack(alignment: .topLeading) {
            JackinBrand.stage
                .ignoresSafeArea()
            Group {
                switch store.resolvedSidebar {
                case .overview:
                    OverviewContentView(store: store, onOpenSettings: onOpenSettings)
                case .provider(let key):
                    if let provider = store.provider(key) {
                        ProviderDetailView(store: store, provider: provider)
                    } else {
                        OverviewContentView(store: store, onOpenSettings: onOpenSettings)
                    }
                case .account(let providerKey, _):
                    if let provider = store.provider(providerKey) {
                        ProviderDetailView(store: store, provider: provider)
                    } else {
                        OverviewContentView(store: store, onOpenSettings: onOpenSettings)
                    }
                }
            }
            .id(transitionKey)
            .transition(
                reduceMotion
                    ? .identity
                    : .asymmetric(
                        insertion: .opacity.combined(with: .offset(y: 5)),
                        removal: .opacity))
        }
        .animation(
            reduceMotion ? nil : .easeOut(duration: 0.15),
            value: transitionKey)
    }

    private var transitionKey: String {
        switch store.resolvedSidebar {
        case .overview: "overview"
        case .provider(let key): "provider:\(key)"
        case .account(let provider, _): "provider:\(provider)"
        }
    }
}
