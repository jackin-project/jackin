import SwiftUI

// View layer mirrors the incumbent implementation
// (native/Sources/JackinDesktop/UsageWindow/*) over fixture view models and
// lifts verbatim into the real app.

struct SidebarView: View {
    let store: ProtoStore
    @Environment(\.jackinIncreaseContrast) private var processContrast
    @State private var hoveredSelection: SidebarSelection?
    @FocusState private var focusedSelection: SidebarSelection?

    var body: some View {
        VStack(spacing: 0) {
            List {
                sidebarButton(target: .overview) {
                    HStack(spacing: JackinSpace.xs) {
                        Image(systemName: "rectangle.grid.2x2")
                            .foregroundStyle(
                                store.sidebar == .overview
                                    ? JackinBrand.selectionText : Color.primary)
                        Text("Overview")
                    }
                    .font(.callout.weight(.medium))
                }
                .accessibilityIdentifier("usage.sidebar.overview")

                Section {
                    ForEach(store.projection.providers) { provider in
                        if provider.accounts.count > 1 {
                            providerGroupHeader(provider)
                                .accessibilityIdentifier(
                                    "usage.sidebar.provider-group.\(provider.key)")
                            ForEach(provider.accounts) { account in
                                sidebarButton(
                                    target: .account(
                                        provider: provider.key, account: account.key),
                                    indent: 8
                                ) {
                                    accountRow(account, provider: provider)
                                }
                                .accessibilityIdentifier(
                                    "usage.sidebar.account.\(provider.key).\(account.key)")
                            }
                        } else {
                            sidebarButton(target: .provider(provider.key)) {
                                providerRow(provider)
                            }
                            .accessibilityIdentifier(
                                "usage.sidebar.provider.\(provider.key)")
                        }
                    }
                } header: {
                    Text("Providers")
                        .foregroundStyle(processContrast ? Color.primary : JackinBrand.muted)
                        .accessibilityLabel("Providers")
                }
            }
            .listStyle(.sidebar)
            .environment(\.defaultMinListRowHeight, 26)
            .onMoveCommand { direction in
                store.moveSidebarSelection(direction)
                focusedSelection = store.resolvedSidebar
            }
            .accessibilityLabel("Usage providers sidebar")
            .accessibilityIdentifier("usage.sidebar")
        }
        .frame(minWidth: 220, idealWidth: 232, maxWidth: 280)
    }

    private func sidebarButton<Content: View>(
        target: SidebarSelection,
        indent: CGFloat = 0,
        @ViewBuilder content: () -> Content
    ) -> some View {
        let selected = store.sidebar == target
        return Button {
            store.navigate(to: target)
        } label: {
            content()
                .foregroundStyle(selected ? JackinBrand.selectionText : Color.primary)
                .foregroundColor(selected ? JackinBrand.selectionText : Color.primary)
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .fill(
                            selected
                                ? JackinBrand.selectionWell
                                : hoveredSelection == target ? JackinBrand.hover : Color.clear)
                )
                .overlay {
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .stroke(
                            focusedSelection == target
                                ? JackinBrand.focusRing
                                : selected
                                    ? JackinBrand.phosphor.opacity(0.55)
                                    : hoveredSelection == target
                                        ? JackinBrand.phosphor.opacity(0.44) : Color.clear,
                            lineWidth: focusedSelection == target ? 2 : 1)
                }
                .padding(.leading, indent)
        }
        .buttonStyle(.plain)
        .focused($focusedSelection, equals: target)
        .accessibilityAddTraits(selected ? .isSelected : [])
        .onHover { hovering in
            hoveredSelection = hovering ? target : nil
        }
        .listRowInsets(EdgeInsets(top: 1, leading: 0, bottom: 1, trailing: 0))
    }

    @ViewBuilder
    private func providerRow(_ provider: ProtoProvider) -> some View {
        Label {
            VStack(alignment: .leading, spacing: JackinSpace.xxs) {
                HStack {
                    Text(provider.name)
                    Spacer()
                    if provider.state.exposesQuotaSummary, let percent = provider.summaryPercent {
                        Text("\(percent)%")
                            .font(.caption)
                            .monospacedDigit()
                            .foregroundStyle(
                                store.sidebar == .provider(provider.key)
                                    ? JackinBrand.selectionText
                                    : sidebarMetricTint(provider.state))
                    }
                }
                if !provider.state.exposesQuotaSummary {
                    Label(provider.state.label ?? "Unavailable", systemImage: provider.state.symbol)
                        .font(.caption2.weight(.medium))
                        .foregroundStyle(JackinBrand.warning)
                } else if provider.accounts.count <= 1, let percent = provider.summaryPercent {
                    sidebarMeter(
                        percent: percent, state: provider.state)
                }
            }
            .font(.callout)
        } icon: {
            providerMark(provider, selected: store.sidebar == .provider(provider.key))
        }
    }

    private func providerGroupHeader(_ provider: ProtoProvider) -> some View {
        Label {
            Text(provider.name)
                .font(.subheadline.weight(.semibold))
                .lineLimit(1)
        } icon: {
            providerMark(provider, selected: false)
        }
        .padding(.horizontal, 7)
        .padding(.vertical, 3)
        .frame(maxWidth: .infinity, alignment: .leading)
        .listRowInsets(EdgeInsets(top: 3, leading: 0, bottom: 1, trailing: 0))
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(provider.name) accounts")
    }

    private func accountRow(_ account: ProtoAccount, provider: ProtoProvider) -> some View {
        let selected =
            store.sidebar == .account(provider: provider.key, account: account.key)
        return VStack(alignment: .leading, spacing: JackinSpace.xxs) {
            HStack(spacing: JackinSpace.xs) {
                Text(account.label)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer()
                if let remaining = account.remaining {
                    Text("\(remaining)%")
                        .font(.caption)
                        .monospacedDigit()
                        .foregroundStyle(
                            selected
                                ? JackinBrand.selectionText
                                : sidebarMetricTint(account.state))
                }
            }
            if let remaining = account.remaining {
                sidebarMeter(percent: remaining, state: account.state)
            }
        }
        .font(.subheadline)
    }

    private func sidebarMeter(percent: Int, state: ProtoState) -> some View {
        GeometryReader { proxy in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(JackinBrand.meterTrack)
                Capsule()
                    .fill(meterTint(state))
                    .frame(
                        width: proxy.size.width
                            * CGFloat(max(0, min(percent, 100))) / 100)
            }
        }
        .frame(height: 2.5)
        .accessibilityHidden(true)
    }

    private func sidebarMetricTint(_ state: ProtoState) -> Color {
        switch state {
        case .warning: JackinBrand.warning
        case .danger, .depleted: JackinBrand.danger
        default: JackinBrand.muted
        }
    }

    @ViewBuilder
    private func providerMark(_ provider: ProtoProvider, selected: Bool) -> some View {
        if let mark = ProviderMarks.swiftUIImage(forIconKey: provider.iconKey) {
            mark
                .renderingMode(.template)
                .resizable()
                .scaledToFit()
                .foregroundStyle(selected ? JackinBrand.selectionText : Color.primary)
        } else {
            Text(provider.fallbackGlyph)
                .font(.caption2)
                .foregroundStyle(selected ? JackinBrand.selectionText : Color.primary)
        }
    }
}
