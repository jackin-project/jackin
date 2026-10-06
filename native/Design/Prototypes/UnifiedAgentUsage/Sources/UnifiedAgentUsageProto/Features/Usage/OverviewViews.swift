import SwiftUI

struct OverviewContentView: View {
    let store: ProtoStore
    let onOpenSettings: () -> Void

    var body: some View {
        if let error = store.projection.globalError {
            ContentUnavailableView {
                Label("Usage unavailable", systemImage: "exclamationmark.triangle")
            } description: {
                Text(error)
            } actions: {
                Button(store.chrome.retryTitle) { store.refresh() }
                    .disabled(store.refreshInProgress)
                    .accessibilityIdentifier("usage.retry")
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(JackinStageBackground())
            .accessibilityIdentifier("usage.global-error")
        } else if store.projection.isLoading {
            ProgressView("Loading usage")
                .controlSize(.large)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(JackinStageBackground())
                .accessibilityIdentifier("usage.loading")
        } else if store.projection.providers.isEmpty {
            ContentUnavailableView {
                Label("No providers detected", systemImage: "chevron.right")
            } description: {
                Text("Add a provider in Settings to start tracking quota limits.")
            } actions: {
                Button("Open Settings…") { onOpenSettings() }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(JackinStageBackground())
            .accessibilityIdentifier("usage.overview.empty")
        } else {
            ScrollView {
                LazyVGrid(
                    columns: [
                        GridItem(.adaptive(minimum: 300), spacing: 18, alignment: .top)
                    ],
                    spacing: 18
                ) {
                    ForEach(store.projection.providers) { provider in
                        ProviderCardView(store: store, provider: provider)
                    }
                }
                .padding(28)
                .frame(maxWidth: 940)
                .frame(maxWidth: .infinity)
            }
            // Grouped-content stage: the gray under-page ground is what the
            // card white contrasts against, in both appearances.
            .background(JackinStageBackground())
            .accessibilityLabel("Usage overview")
            .accessibilityIdentifier("usage.overview.grid")
        }
    }
}

/// One provider card in the Overview grid.
///
/// Content layer: standard material,
/// no glass. Every canonical account renders as its own block — the overview
/// never collapses multi-account providers to one row. A tap focuses the
/// account in the sidebar detail.
///
/// Visual hierarchy per account block: the remaining percent is the hero
/// (largest type, state-tinted), the identity line is secondary, the meter
/// is a hairline with a visible track, and metadata is a single quiet
/// caption row. Healthy states render no badge — silence means fine.
struct ProviderCardView: View {
    let store: ProtoStore
    let provider: ProtoProvider

    var body: some View {
        VStack(alignment: .leading, spacing: JackinSpace.sm) {
            HStack(spacing: JackinSpace.xs) {
                BrandMarkChip(iconKey: provider.iconKey, fallbackGlyph: provider.fallbackGlyph)
                Text(provider.name)
                    .font(.headline)
                    .lineLimit(1)
                Spacer(minLength: 8)
                if let stateLabel = provider.state.label {
                    Label(stateLabel, systemImage: provider.state.symbol)
                        .font(.caption)
                        .foregroundStyle(badgeTint(provider.state))
                        .labelStyle(.titleAndIcon)
                        .accessibilityHidden(true)
                }
            }

            if provider.accounts.isEmpty {
                emptyAccountsBlock
            } else {
                ForEach(Array(provider.accounts.enumerated()), id: \.element.id) {
                    index, account in
                    if index > 0 {
                        Divider()
                            .padding(.vertical, 2)
                    }
                    accountBlock(account)
                }
            }

            if let error = provider.errorText {
                VStack(alignment: .leading, spacing: JackinSpace.xs) {
                    Label(error, systemImage: "exclamationmark.triangle")
                        .font(.caption)
                        .foregroundStyle(JackinBrand.muted)
                        .lineLimit(3)
                        .fixedSize(horizontal: false, vertical: true)
                    HStack(spacing: JackinSpace.xs) {
                        if let ago = provider.updatedAgo {
                            Text(ago)
                                .font(.caption)
                                .foregroundStyle(JackinBrand.quiet)
                        }
                        Spacer()
                        Button(store.chrome.retryTitle) { store.refresh() }
                            .controlSize(.small)
                            .buttonStyle(.bordered)
                            .disabled(store.refreshInProgress)
                            .accessibilityIdentifier("usage.overview.retry.\(provider.key)")
                    }
                }
                .accessibilityIdentifier("usage.overview.error.\(provider.key)")
            }
        }
        .padding(JackinSpace.lg)
        .frame(maxWidth: .infinity, alignment: .leading)
        .modifier(ProviderCardSurface())
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("usage.overview.provider.\(provider.key)")
    }

    @ViewBuilder
    private var emptyAccountsBlock: some View {
        if provider.errorText != nil {
            EmptyView()
        } else {
            Text("No accounts discovered")
                .font(.callout)
                .foregroundStyle(JackinBrand.muted)
        }
    }

    private func accountBlock(_ account: ProtoAccount) -> some View {
        Button {
            store.navigate(to: .account(provider: provider.key, account: account.key))
        } label: {
            VStack(alignment: .leading, spacing: JackinSpace.xs) {
                HStack(alignment: .firstTextBaseline) {
                    if provider.accounts.count > 1 {
                        Text(account.label)
                            .font(.callout)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    } else {
                        Text(account.plan)
                            .font(.callout)
                            .foregroundStyle(JackinBrand.muted)
                    }
                    Spacer(minLength: 8)
                    if let remaining = account.remaining {
                        HStack(alignment: .firstTextBaseline, spacing: 3) {
                            Text("\(remaining)")
                                .font(JackinType.heroMetric)
                                .monospacedDigit()
                            Text("% left")
                                .font(JackinType.metadata)
                                .foregroundStyle(JackinBrand.muted)
                        }
                        .foregroundStyle(metricTint(account.state))
                    } else {
                        Text("—")
                            .font(.title2)
                            .foregroundStyle(JackinBrand.quiet)
                    }
                }

                if let remaining = account.remaining {
                    QuotaMeter(percent: remaining, tint: meterTint(account.state))
                        .accessibilityHidden(true)
                }

                HStack(spacing: JackinSpace.xs) {
                    if provider.accounts.count > 1 {
                        Text(account.plan)
                    }
                    if let stateLabel = account.state.label {
                        Label(stateLabel, systemImage: account.state.symbol)
                            .labelStyle(.titleAndIcon)
                            .foregroundStyle(badgeTint(account.state))
                    }
                    Spacer()
                    if let reset = account.resetText {
                        Text(reset)
                    }
                }
                .font(JackinType.metadata)
                .monospacedDigit()
                .foregroundStyle(JackinBrand.muted)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(OverviewAccountButtonStyle())
        .accessibilityLabel(
            "\(provider.name), \(account.label), \(account.remaining.map { "\($0) percent left" } ?? "remaining unavailable")"
        )
        .accessibilityIdentifier("usage.overview.account.\(provider.key).\(account.key)")
    }

    private func badgeTint(_ state: ProtoState) -> Color {
        switch state {
        case .warning: JackinBrand.warning
        case .danger, .depleted: JackinBrand.danger
        case .stale, .rateLimited: JackinBrand.warning
        case .needsLogin, .needsSecret, .unsupported, .unavailable: .secondary
        default: .secondary
        }
    }

    private func metricTint(_ state: ProtoState) -> Color {
        switch state {
        case .warning: JackinBrand.warning
        case .danger, .depleted: JackinBrand.danger
        default: .primary
        }
    }
}

private struct OverviewAccountButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        OverviewAccountButtonBody(configuration: configuration)
    }

    private struct OverviewAccountButtonBody: View {
        let configuration: Configuration
        @State private var isHovered = false
        @Environment(\.accessibilityReduceMotion) private var reduceMotion

        var body: some View {
            let isActive = isHovered || configuration.isPressed
            configuration.label
                .padding(.vertical, 10)
                .background(
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .fill(
                            isActive ? JackinBrand.hover : Color.clear)
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .strokeBorder(
                            isHovered ? JackinBrand.separator : Color.clear,
                            lineWidth: 1)
                )
                .opacity(configuration.isPressed ? 0.82 : 1)
                .contentShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
                .onHover { isHovered = $0 }
                .animation(
                    reduceMotion ? nil : .easeOut(duration: 0.12),
                    value: isActive)
        }
    }
}

/// Authored content boundary for the preferred overview cards.
///
/// This is standard opaque content material, never glass. Its compact technical
/// radius and crisp edge separate providers without ornamental depth.
private struct ProviderCardSurface: ViewModifier {
    @Environment(\.colorSchemeContrast) private var contrast
    @Environment(\.jackinIncreaseContrast) private var processContrast

    func body(content: Content) -> some View {
        content
            .background(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .fill(JackinBrand.card)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .strokeBorder(
                        contrast == .increased || processContrast
                            ? JackinBrand.strongSeparator : JackinBrand.separator,
                        lineWidth: contrast == .increased || processContrast ? 1.5 : 1)
            )
    }
}

/// Calibrated quota meter: a low-radius 6pt track, state-tinted fill.
///
/// Content-layer drawing, not chrome — a plain deterministic bar.
struct QuotaMeter: View {
    let percent: Int
    let tint: Color

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .leading) {
                RoundedRectangle(cornerRadius: 2, style: .continuous)
                    .fill(JackinBrand.meterTrack)
                RoundedRectangle(cornerRadius: 2, style: .continuous)
                    .fill(tint)
                    .frame(
                        width: proxy.size.width
                            * CGFloat(min(max(percent, 0), 100)) / 100)
            }
        }
        .frame(height: 6)
        .accessibilityHidden(true)
    }
}
