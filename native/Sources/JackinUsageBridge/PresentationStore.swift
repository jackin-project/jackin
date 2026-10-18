// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import CoreGraphics
import Foundation
import JackinUsageBindings

public enum StatusItemDisplayMode: String, CaseIterable, Sendable {
    case iconOnly
    case focusPercent
    case pinnedSurface
    case strip
}

@MainActor
public final class PresentationStore: ObservableObject {
    @Published public internal(set) var nextRefreshLabel: String = ""
    @Published public internal(set) var surfaces: [SurfaceRow] = []
    @Published public internal(set) var providerGlanceRows: [GlanceProviderRow] = []
    @Published public internal(set) var statusBarGlanceRows: [GlanceProviderRow] = []
    @Published public internal(set) var statusBarShowsValues = true
    @Published public internal(set) var providerGroups: [ProviderGroupRow] = []
    @Published public var overviewExpandedProviderIDs: Set<String> = []
    @Published public var overviewSelectionID: String?
    @Published public internal(set) var accounts: [AccountRow] = []
    @Published public internal(set) var discoveryDiagnostics: [DiscoveryDiagnostic] = []
    @Published public internal(set) var usageSelection: String?
    @Published public internal(set) var usageAccountSelection: String?
    @Published public var popoverSelection: String?
    @Published public internal(set) var refreshInProgress = false
    @Published public internal(set) var lastError: String?
    @Published public internal(set) var isOpen: Bool = false
    @Published public internal(set) var isOpening: Bool = false
    @Published public internal(set) var refreshFloorSecs: UInt64 = 300

    @Published public var displayMode: StatusItemDisplayMode {
        didSet {
            UserDefaults.standard.set(displayMode.rawValue, forKey: Self.displayModeKey)
            refreshVisibleStatusRows()
        }
    }

    @Published public var pinnedSurfaceId: String {
        didSet {
            UserDefaults.standard.set(pinnedSurfaceId, forKey: Self.pinnedSurfaceKey)
            refreshVisibleStatusRows()
        }
    }

    @Published public var stripMax: Int {
        didSet {
            let clamped = max(1, min(statusBarMaxChips, stripMax))
            if clamped != stripMax {
                stripMax = clamped
                return
            }
            UserDefaults.standard.set(stripMax, forKey: Self.stripMaxKey)
            if isOpen {
                Task { [weak self] in await self?.applySnapshots() }
            }
        }
    }

    @Published public var percentStyle: String {
        didSet {
            UserDefaults.standard.set(percentStyle, forKey: Self.percentStyleKey)
            Task { [weak self] in
                guard let self else { return }
                await self.pushFormatPrefs()
                if self.isOpen { await self.applySnapshots() }
            }
        }
    }

    @Published public var resetStyle: String {
        didSet {
            UserDefaults.standard.set(resetStyle, forKey: Self.resetStyleKey)
            Task { [weak self] in
                guard let self else { return }
                await self.pushFormatPrefs()
                if self.isOpen { await self.applySnapshots() }
            }
        }
    }

    @Published public var hideWhileScreenSharing: Bool {
        didSet {
            UserDefaults.standard.set(hideWhileScreenSharing, forKey: Self.hideScreenShareKey)
        }
    }

    private static let displayModeKey = "jackin.desktop.displayMode"
    private static let pinnedSurfaceKey = "jackin.desktop.pinnedSurfaceId"
    private static let stripMaxKey = "jackin.desktop.stripMax"
    private static let percentStyleKey = "jackin.desktop.percentStyle"
    private static let resetStyleKey = "jackin.desktop.resetStyle"
    private static let hideScreenShareKey = "jackin.desktop.hideWhileScreenSharing"

    let scheduler: RefreshScheduler
    var projectedStatusBarRows: [GlanceProviderRow] = []
    var nextApplyRequest: UInt64 = 0
    var lastAppliedRequest: UInt64 = 0
    var lastAppliedGeneration: UInt64 = 0
    var knownOverviewProviderIDs: Set<String> = []
    var eventCursor: UInt64 = 0
    var pollTask: Task<Void, Never>?
    var screenShareActive: Bool = false
    var fixtureMode = false
    var fixtureTerminalProjection: QIFixtureProjection?
    var fixtureRefreshingProjection: QIFixtureProjection?
    var fixtureAccountProjections: [String: QIFixtureProjection] = [:]
    var fixtureRefreshTask: Task<Void, Never>?
    var launchConfiguration: LaunchConfiguration = .production

    public var usesFixture: Bool { fixtureMode }

    public convenience init() {
        self.init(scheduler: RefreshScheduler())
    }

    public init(scheduler: RefreshScheduler) {
        self.scheduler = scheduler
        let defaults = UserDefaults.standard
        if let raw = defaults.string(forKey: Self.displayModeKey),
            let mode = StatusItemDisplayMode(rawValue: raw)
        {
            self.displayMode = mode
        } else if defaults.object(forKey: "jackin.desktop.showPercent") != nil {
            self.displayMode =
                defaults.bool(forKey: "jackin.desktop.showPercent")
                ? .focusPercent
                : .iconOnly
            defaults.removeObject(forKey: "jackin.desktop.showPercent")
        } else {
            self.displayMode = .strip
        }
        self.pinnedSurfaceId = defaults.string(forKey: Self.pinnedSurfaceKey) ?? ""
        let strip = defaults.object(forKey: Self.stripMaxKey) as? Int ?? statusBarMaxChips
        self.stripMax = max(1, min(statusBarMaxChips, strip))
        let percent = defaults.string(forKey: Self.percentStyleKey) ?? "left"
        self.percentStyle = (percent == "used") ? "used" : "left"
        let reset = defaults.string(forKey: Self.resetStyleKey) ?? "countdown"
        self.resetStyle = (reset == "exact_clock") ? "exact_clock" : "countdown"
        self.hideWhileScreenSharing = defaults.bool(forKey: Self.hideScreenShareKey)
    }

    public enum LaunchConfiguration: Sendable, Equatable {
        case production
        case ephemeralSmoke(dataDir: String)

        public static func resolve(
            environment: [String: String],
            homeDirectory: String
        ) -> LaunchConfiguration {
            if let dir = environment["JACKIN_DESKTOP_SMOKE_DATA_DIR"],
                dir.hasPrefix("/"),
                !dir.hasPrefix(homeDirectory)
            {
                return .ephemeralSmoke(dataDir: dir)
            }
            return .production
        }
    }

    public func openForLaunch(_ configuration: LaunchConfiguration) {
        launchConfiguration = configuration
        switch configuration {
        case .production:
            openDefault()
        case .ephemeralSmoke(let dataDir):
            openSmoke(dataDir: dataDir)
        }
    }

    public func openDefault() {
        open(
            dataDirOverride: nil,
            configRootOverride: nil,
            refreshFloorSecs: 300,
            enabled: [],
            allowLiveProbes: true
        )
    }

    public func retryLastOperation() {
        guard !fixtureMode else { return }
        if isOpen {
            refreshAll()
        } else {
            openForLaunch(launchConfiguration)
        }
    }

    public func open(dataDir: String, refreshFloorSecs: UInt64, enabled: [String]) {
        open(
            dataDirOverride: dataDir,
            configRootOverride: URL(fileURLWithPath: dataDir)
                .appendingPathComponent("config").path,
            refreshFloorSecs: refreshFloorSecs,
            enabled: enabled,
            allowLiveProbes: true
        )
    }

    public func shutdown() {
        pollTask?.cancel()
        pollTask = nil
        fixtureRefreshTask?.cancel()
        fixtureRefreshTask = nil
        scheduler.invalidateAndShutdown()
        isOpen = false
        isOpening = false
    }

    func pushFormatPrefs() async {
        guard !fixtureMode, isOpen else { return }
        let prefs = UsageFormatPrefsDto(percentStyle: percentStyle, resetStyle: resetStyle)
        do {
            try await scheduler.setFormatPrefs(prefs)
        } catch {
            report(error, userMessage: "Display setting could not be saved.")
        }
    }

    func startPolling() {
        pollTask?.cancel()
        pollTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: 5_000_000_000)
                await self?.pollOnce()
            }
        }
    }

    private func pollOnce() async {
        guard !fixtureMode, isOpen else { return }
        if hideWhileScreenSharing {
            screenShareActive = Self.isScreenCurrentlyShared()
        } else {
            screenShareActive = false
        }
        statusBarShowsValues = !(hideWhileScreenSharing && screenShareActive)
        let cursor = eventCursor
        do {
            let nextCursor = try await scheduler.pollOnce(cursor: cursor)
            eventCursor = nextCursor
            await applySnapshots()
        } catch {
            report(error, userMessage: "Usage could not be updated. Try again.")
        }
    }

    public static func isScreenCurrentlyShared() -> Bool {
        guard let dict = CGSessionCopyCurrentDictionary() as? [String: Any] else {
            return false
        }
        if let shared = dict["CGSSessionScreenIsShared"] as? Bool {
            return shared
        }
        if let shared = dict["CGSSessionScreenIsShared"] as? NSNumber {
            return shared.boolValue
        }
        return false
    }

    public func selectUsageSurface(_ surfaceId: String?) {
        selectUsageContext(surfaceId: surfaceId, accountKey: nil)
    }

    public func selectUsageContext(surfaceId: String?, accountKey: String?) {
        guard let surfaceId else {
            usageSelection = nil
            usageAccountSelection = nil
            return
        }
        guard isNavigableSurface(surfaceId) else {
            usageSelection = nil
            usageAccountSelection = nil
            return
        }
        usageSelection = surfaceId
        usageAccountSelection = accountKey
    }

    func recordUsageAccountSelection(surfaceId: String, accountKey: String) {
        guard usageSelection == surfaceId else { return }
        usageAccountSelection = accountKey
    }
}
