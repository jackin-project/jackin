import SwiftUI

enum PercentStyle: String, Sendable {
    case left, used
}

enum SidebarSelection: Hashable, Sendable {
    case overview
    case provider(String)
    case account(provider: String, account: String)
}

/// Stands in for the Rust-owned PresentationStore: every value change here is
/// an accepted/rejected mutation round-trip, never optimistic local state.
@MainActor
@Observable
final class ProtoStore {
    /// Swappable at runtime by the Scenario menu — one projection swap redraws
    /// every surface and rebuilds the status items (shell observes this).
    private(set) var projection: ProtoProjection
    private(set) var percentStyle: PercentStyle = .left
    private(set) var refreshFloorMinutes = 5
    private(set) var floorError: String?
    var sidebar: SidebarSelection
    var accountSelection: [String: String]
    private(set) var refreshGeneration = 0
    private(set) var refreshInProgress = false
    private var acceptedFloorRevision = 0
    private var issuedFloorRevision = 40

    // Settings surface state (fixture-backed mock; no persistence).
    var displayMode: DisplayMode = .strip
    var pinnedSurfaceKey = ""
    var stripMax = 3
    var resetStyle: ResetStyle = .countdown
    var hideWhileScreenSharing = false
    var launchAtLogin = false
    var surfaceEnabled: [String: Bool]

    enum DisplayMode: String, Sendable { case strip, focusPercent, pinnedSurface, iconOnly }
    enum ResetStyle: String, Sendable { case countdown, exactClock }

    init(projection: ProtoProjection) {
        self.projection = projection
        sidebar = Self.initialSidebarSelection(for: projection)
        accountSelection = Dictionary(
            uniqueKeysWithValues: projection.providers.compactMap { provider in
                provider.selectedAccountKey.map { (provider.key, $0) }
            })
        surfaceEnabled = Dictionary(
            uniqueKeysWithValues: projection.providers.map { ($0.key, true) })
        if let providerKey = projection.selectedProviderKey,
            let accountKey = projection.selectedAccountKey
        {
            accountSelection[providerKey] = accountKey
        }
    }

    var chrome: ProtoChrome { projection.chrome }

    /// Scenario-menu drive: swap the whole fixture projection and reset the
    /// selection surface state the old projection owned.
    func loadScenario(_ name: String) {
        let previousSidebar = resolvedSidebar
        let previousAccounts = accountSelection
        let next = ProtoFixtures.load(name)
        projection = next
        accountSelection = Dictionary(
            uniqueKeysWithValues: next.providers.compactMap { provider in
                let preserved = previousAccounts[provider.key].flatMap { key in
                    provider.accounts.contains(where: { $0.key == key }) ? key : nil
                }
                return (preserved ?? provider.selectedAccountKey).map { (provider.key, $0) }
            })
        sidebar = previousSidebar
        let normalized = resolvedSidebar
        sidebar =
            normalized == .overview && previousSidebar != .overview
            ? Self.initialSidebarSelection(for: next) : normalized
        surfaceEnabled = Dictionary(
            uniqueKeysWithValues: next.providers.map { ($0.key, true) })
        floorError = nil
        refreshInProgress = false
    }

    static func initialSidebarSelection(
        for projection: ProtoProjection
    ) -> SidebarSelection {
        guard let providerKey = projection.selectedProviderKey else { return .overview }
        guard
            let provider = projection.providers.first(where: { $0.key == providerKey }),
            provider.accounts.count > 1,
            let accountKey = projection.selectedAccountKey
        else { return .provider(providerKey) }
        return .account(provider: providerKey, account: accountKey)
    }

    /// A removed/disabled provider or account normalizes here, not in a view.
    var resolvedSidebar: SidebarSelection {
        switch sidebar {
        case .provider(let key):
            guard let provider = projection.providers.first(where: { $0.key == key })
            else { return .overview }
            if provider.accounts.count > 1, let account = provider.accounts.first {
                let selected = accountSelection[key] ?? provider.selectedAccountKey ?? account.key
                return .account(provider: key, account: selected)
            }
            return sidebar
        case .account(let providerKey, let accountKey):
            guard let provider = projection.providers.first(where: { $0.key == providerKey })
            else { return .overview }
            if provider.accounts.contains(where: { $0.key == accountKey }) {
                return sidebar
            }
            if provider.accounts.count > 1, let firstAccount = provider.accounts.first {
                return .account(provider: providerKey, account: firstAccount.key)
            }
            return .provider(providerKey)
        case .overview:
            return .overview
        }
    }

    func provider(_ key: String) -> ProtoProvider? {
        projection.providers.first(where: { $0.key == key })
    }

    func account(for provider: ProtoProvider) -> ProtoAccount? {
        let key = accountSelection[provider.key] ?? provider.selectedAccountKey
        return provider.accounts.first(where: { $0.key == key })
            ?? provider.accounts.first
    }

    func selectAccount(_ key: String, for provider: ProtoProvider) {
        accountSelection[provider.key] = key
    }

    /// Sidebar navigation: account rows carry the account selection with
    /// them so detail, popover, and status item follow one source of truth.
    func navigate(to selection: SidebarSelection) {
        sidebar = selection
        sidebar = resolvedSidebar
        if case .account(let providerKey, let accountKey) = sidebar {
            accountSelection[providerKey] = accountKey
        }
    }

    func moveSidebarSelection(_ direction: MoveCommandDirection) {
        guard direction == .up || direction == .down else { return }
        let destinations = sidebarDestinations
        guard !destinations.isEmpty else { return }
        let current = destinations.firstIndex(of: resolvedSidebar) ?? 0
        let delta = direction == .down ? 1 : -1
        navigate(to: destinations[max(0, min(destinations.count - 1, current + delta))])
    }

    var sidebarDestinations: [SidebarSelection] {
        [SidebarSelection.overview]
            + projection.providers.flatMap { provider in
                provider.accounts.count > 1
                    ? provider.accounts.map {
                        SidebarSelection.account(provider: provider.key, account: $0.key)
                    }
                    : [SidebarSelection.provider(provider.key)]
            }
    }

    func summaryRemaining(_ provider: ProtoProvider) -> String? {
        switch percentStyle {
        case .left: return provider.summaryRemainingLeft
        case .used: return provider.summaryRemainingUsed
        }
    }

    func statusPercent(_ provider: ProtoProvider) -> String? {
        guard let percent = provider.summaryPercent else { return nil }
        // Bottom line is the bare percent; the top line's countdown already
        // carries the period context. The summary itself stays driven by
        // long-range windows only (see ProtoProvider.summaryWindow).
        switch percentStyle {
        case .left: return "\(percent)%"
        case .used: return "\(100 - percent)%"
        }
    }

    /// Status-item percent tint — same severity law as the window meters:
    /// danger/depleted red, warning orange, otherwise phosphor.
    func statusTint(_ provider: ProtoProvider) -> NSColor {
        switch provider.summaryWindow?.state ?? provider.state {
        case .danger, .depleted: JackinBrand.dangerNSColor
        case .warning: JackinBrand.warningNSColor
        default: JackinBrand.phosphorNSColor
        }
    }

    func setPercentStyle(_ style: PercentStyle) {
        // F15: Rust accepts and returns the next projection with used strings.
        percentStyle = style
    }

    func requestRefreshFloor(_ minutes: Int) {
        switch projection.mutationScript {
        case .rejectLowFloor where minutes < 5:
            // F16: typed recoverable rejection; accepted value never moved.
            floorError =
                "Refresh floor of \(minutes) minute(s) rejected: minimum is 5 minutes"
        case .reorderedFloor:
            // F17: later intent (15 min) completes before earlier intent
            // (10 min); an older revision can never overwrite a newer one.
            issuedFloorRevision += 1
            let revision = issuedFloorRevision
            floorError = nil
            let delay: Duration = minutes == 10 ? .milliseconds(800) : .milliseconds(400)
            Task { @MainActor [weak self] in
                try? await Task.sleep(for: delay)
                guard let self, revision > self.acceptedFloorRevision else { return }
                self.acceptedFloorRevision = revision
                self.refreshFloorMinutes = minutes
            }
        default:
            issuedFloorRevision += 1
            acceptedFloorRevision = issuedFloorRevision
            refreshFloorMinutes = minutes
            floorError = nil
        }
    }

    /// F16: the exact failed mutation is retried verbatim beside the settings.
    func retryRefreshFloor() {
        floorError = nil
        requestRefreshFloor(1)
    }

    /// Refresh intent joins existing broker work; repeated calls never fork.
    ///
    /// The busy window is a fixed 900 ms fixture so the spinner swap is
    /// visible live without real probe latency.
    func refresh() {
        refreshGeneration += 1
        guard !refreshInProgress else { return }
        refreshInProgress = true
        Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(900))
            self?.refreshInProgress = false
        }
    }
}
