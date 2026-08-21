// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import CoreGraphics
import Foundation
import JackinUsageBindings

/// Status-item display mode (Settings-selectable; Rust supplies every string).
public enum StatusItemDisplayMode: String, CaseIterable, Sendable {
    case iconOnly
    case focusPercent
    case pinnedSurface
    case strip
}

/// Thin presentation store: polls Rust boltffi snapshots; no provider probes.
@MainActor
public final class PresentationStore: ObservableObject {
    public struct IdentityRow: Sendable, Equatable {
        public let providerTitle: String
        public let accountLabel: String
        public let activityLabel: String
        public let activityKind: String
        public let accessibilityLabel: String

        public init(dto: UsageIdentityPresentationDto) {
            providerTitle = dto.providerTitle
            accountLabel = dto.accountLabel
            activityLabel = dto.activityLabel
            activityKind = dto.activityKind
            accessibilityLabel = dto.accessibilityLabel
        }
    }
    public struct SurfaceRow: Identifiable, Sendable, Equatable {
        public let id: String
        public let label: String
        public var enabled: Bool
        public var statusBarLabel: String
        public var status: String
        public var accountLabel: String
        public var username: String?
        public var planLabel: String?
        public var credentialOrigin: String?
        public var estimateCaption: String?
        public var buckets: [BucketRow]
        public var updatedLabel: String
        public var lastError: String?
        public var identity: IdentityRow?
        /// Rust-owned Capsule-parity provider detail.
        ///
        /// The Usage window
        /// renders these rows verbatim; other surfaces ignore it.
        public var detailPresentation: UsageDetailPresentation

        public init(
            id: String,
            label: String,
            enabled: Bool,
            statusBarLabel: String,
            status: String,
            accountLabel: String,
            username: String?,
            planLabel: String?,
            credentialOrigin: String?,
            estimateCaption: String?,
            buckets: [BucketRow],
            updatedLabel: String,
            lastError: String?,
            detailPresentation: UsageDetailPresentation,
            identity: IdentityRow? = nil
        ) {
            self.id = id
            self.label = label
            self.enabled = enabled
            self.statusBarLabel = statusBarLabel
            self.status = status
            self.accountLabel = accountLabel
            self.username = username
            self.planLabel = planLabel
            self.credentialOrigin = credentialOrigin
            self.estimateCaption = estimateCaption
            self.buckets = buckets
            self.updatedLabel = updatedLabel
            self.lastError = lastError
            self.detailPresentation = detailPresentation
            self.identity = identity
        }
    }

    public struct BucketRow: Identifiable, Sendable, Equatable {
        public var id: String { label }
        public let label: String
        public let usedLabel: String?
        public let limitLabel: String?
        public let remainingPercent: UInt8?
        public let resetLabel: String?
        public let paceLabel: String?
        public let statusSlot: String?
        public let severity: String
        public let status: String
        /// Rust money fields (display-only; formatted in the shell).
        public let usedMoney: MoneyDto?
        public let limitMoney: MoneyDto?
        /// Rust-owned limits-only presentation (rendered verbatim; never recomputed).
        public let remainingLabel: String?
        public let displaySegments: [String]
        public let displayLabel: String
        public let meterPercent: UInt8?
    }

    /// One Rust-owned provider glance row projected verbatim (no computed usage
    /// values in Swift). `id == surfaceId`.
    public struct GlanceProviderRow: Identifiable, Sendable, Equatable {
        public var id: String { surfaceId }
        public let surfaceId: String
        public let iconKey: String
        public let fallbackGlyph: String
        public let usageURL: String?
        public let displayLabel: String
        public let accountLabel: String
        public let planLabel: String?
        public let glanceRemainingPercent: UInt8?
        public let barLabel: String
        public let headline: String
        public let resetLabel: String?
        public let compactResetLabel: String?
        public let exactReset: String?
        public let statusWord: String
        public let isRefreshing: Bool
        public let statusLabel: String
        public let severity: String
        public let updatedLabel: String
        public let activityLabel: String
        public let activityKind: String
        public let accessibilityLabel: String
        public let lastError: String?
        public let dimmed: Bool

        public init(
            surfaceId: String,
            iconKey: String,
            fallbackGlyph: String,
            usageURL: String?,
            displayLabel: String,
            accountLabel: String,
            planLabel: String?,
            glanceRemainingPercent: UInt8?,
            barLabel: String,
            headline: String,
            resetLabel: String?,
            compactResetLabel: String?,
            exactReset: String?,
            statusWord: String,
            isRefreshing: Bool,
            statusLabel: String,
            severity: String,
            updatedLabel: String,
            activityLabel: String,
            activityKind: String,
            accessibilityLabel: String,
            lastError: String?,
            dimmed: Bool
        ) {
            self.surfaceId = surfaceId
            self.iconKey = iconKey
            self.fallbackGlyph = fallbackGlyph
            self.usageURL = usageURL
            self.displayLabel = displayLabel
            self.accountLabel = accountLabel
            self.planLabel = planLabel
            self.glanceRemainingPercent = glanceRemainingPercent
            self.barLabel = barLabel
            self.headline = headline
            self.resetLabel = resetLabel
            self.compactResetLabel = compactResetLabel
            self.exactReset = exactReset
            self.statusWord = statusWord
            self.isRefreshing = isRefreshing
            self.statusLabel = statusLabel
            self.severity = severity
            self.updatedLabel = updatedLabel
            self.activityLabel = activityLabel
            self.activityKind = activityKind
            self.accessibilityLabel = accessibilityLabel
            self.lastError = lastError
            self.dimmed = dimmed
        }
    }

    public struct ProviderGroupRow: Identifiable, Sendable, Equatable {
        public var id: String { surfaceId }
        public let surfaceId: String
        public let displayLabel: String
        public let iconKey: String
        public let fallbackGlyph: String
        public let usageURL: String?
        public let accountColumnLabel: String
        public let planOrStatusLabel: String
        public let remainingLabel: String
        public let resetDisplayLabel: String
        public let accounts: [AccountRow]
        public let accessibilityLabel: String
        public let lastError: String?

        public init(
            surfaceId: String,
            displayLabel: String,
            iconKey: String,
            fallbackGlyph: String,
            usageURL: String?,
            accountColumnLabel: String,
            planOrStatusLabel: String,
            remainingLabel: String,
            resetDisplayLabel: String,
            accounts: [AccountRow],
            accessibilityLabel: String,
            lastError: String?
        ) {
            self.surfaceId = surfaceId
            self.displayLabel = displayLabel
            self.iconKey = iconKey
            self.fallbackGlyph = fallbackGlyph
            self.usageURL = usageURL
            self.accountColumnLabel = accountColumnLabel
            self.planOrStatusLabel = planOrStatusLabel
            self.remainingLabel = remainingLabel
            self.resetDisplayLabel = resetDisplayLabel
            self.accounts = accounts
            self.accessibilityLabel = accessibilityLabel
            self.lastError = lastError
        }
    }

    /// Rust-owned, sanitized discovery failure.
    ///
    /// No credential location or secret.
    public struct DiscoveryDiagnostic: Identifiable, Sendable, Equatable {
        public var id: String { "\(surfaceId ?? "global")#\(scopeLabel)#\(issue)" }
        public let surfaceId: String?
        public let scopeLabel: String
        public let issue: String
        public let message: String
        public let displayLabel: String
    }

    /// Multi-account row for a host surface (Rust-owned keys/labels).
    public struct AccountRow: Identifiable, Sendable, Equatable {
        public var id: String { "\(surfaceId)#\(accountKey)" }
        public let surfaceId: String
        public let providerColumnLabel: String
        public let accountKey: String
        public let accountLabel: String
        public let planLabel: String?
        public let selected: Bool
        public let lifecycle: String
        public let lifecycleLabel: String
        public let provenanceLabel: String
        public let planOrStatusLabel: String
        public let remainingPercent: UInt8?
        public let remainingLabel: String
        public let headline: String
        public let resetDisplayLabel: String
        public let statusWord: String
        public let statusLabel: String
        public let severity: String
        public let updatedLabel: String
        public let lastError: String?
        public let dimmed: Bool
        public let accessibilityLabel: String

        public init(
            surfaceId: String,
            providerColumnLabel: String,
            accountKey: String,
            accountLabel: String,
            planLabel: String?,
            selected: Bool,
            lifecycle: String,
            lifecycleLabel: String,
            provenanceLabel: String,
            planOrStatusLabel: String,
            remainingPercent: UInt8?,
            remainingLabel: String,
            headline: String,
            resetDisplayLabel: String,
            statusWord: String,
            statusLabel: String,
            severity: String,
            updatedLabel: String,
            lastError: String?,
            dimmed: Bool,
            accessibilityLabel: String
        ) {
            self.surfaceId = surfaceId
            self.providerColumnLabel = providerColumnLabel
            self.accountKey = accountKey
            self.accountLabel = accountLabel
            self.planLabel = planLabel
            self.selected = selected
            self.lifecycle = lifecycle
            self.lifecycleLabel = lifecycleLabel
            self.provenanceLabel = provenanceLabel
            self.planOrStatusLabel = planOrStatusLabel
            self.remainingPercent = remainingPercent
            self.remainingLabel = remainingLabel
            self.headline = headline
            self.resetDisplayLabel = resetDisplayLabel
            self.statusWord = statusWord
            self.statusLabel = statusLabel
            self.severity = severity
            self.updatedLabel = updatedLabel
            self.lastError = lastError
            self.dimmed = dimmed
            self.accessibilityLabel = accessibilityLabel
        }
    }

    /// Complete frozen state used only by explicit `--fixture` QA launches.
    public struct QIFixtureProjection: Sendable, Equatable {
        public let glanceRows: [GlanceProviderRow]
        public let statusBarGlanceRows: [GlanceProviderRow]
        public let surfaces: [SurfaceRow]
        public let accounts: [AccountRow]
        public let providerGroups: [ProviderGroupRow]

        public init(
            glanceRows: [GlanceProviderRow],
            statusBarGlanceRows: [GlanceProviderRow],
            surfaces: [SurfaceRow],
            accounts: [AccountRow],
            providerGroups: [ProviderGroupRow]
        ) {
            self.glanceRows = glanceRows
            self.statusBarGlanceRows = statusBarGlanceRows
            self.surfaces = surfaces
            self.accounts = accounts
            self.providerGroups = providerGroups
        }
    }

    /// Footer / window next-refresh string from Rust.
    @Published public private(set) var nextRefreshLabel: String = ""
    @Published public private(set) var surfaces: [SurfaceRow] = []
    /// Rust-owned seven-provider glance rows (auto-detected, catalog order).
    ///
    /// Full inventory for popover / Usage — **includes** 0% (OV-7).
    @Published public private(set) var providerGlanceRows: [GlanceProviderRow] = []
    /// Burn-first **status bar** chips only (SB-3/14/17/19): hide 0%, soonest-
    /// then-remaining, hard-cap ≤3.
    ///
    /// Popover never uses this list.
    @Published public private(set) var statusBarGlanceRows: [GlanceProviderRow] = []
    /// Presentation-only privacy flag: `false` hides the Rust status-bar values
    /// during screen sharing (it may hide a Rust label, never replace it).
    @Published public private(set) var statusBarShowsValues = true
    /// Canonical Rust-ordered provider groups with account children.
    @Published public private(set) var providerGroups: [ProviderGroupRow] = []
    @Published public var overviewExpandedProviderIDs: Set<String> = []
    @Published public var overviewSelectionID: String?
    /// Known accounts across surfaces (multi-account host logins / shared snapshots).
    @Published public private(set) var accounts: [AccountRow] = []
    @Published public private(set) var discoveryDiagnostics: [DiscoveryDiagnostic] = []
    /// Sidebar / detail selection: `nil` = Overview, else surface id.
    @Published public private(set) var usageSelection: String?
    /// Exact account context carried by navigation into the Usage window.
    @Published public private(set) var usageAccountSelection: String?
    /// Focused popover provider; nil lets the host select the first available provider.
    @Published public var popoverSelection: String?
    /// True only while an enqueued refresh request runs its bridge operation —
    /// drives the popover/footer spinner.
    ///
    /// Never clears glance rows or surfaces.
    @Published public private(set) var refreshInProgress = false
    @Published public private(set) var lastError: String?
    @Published public private(set) var isOpen: Bool = false
    /// True from the moment a cold open is submitted until it succeeds/fails, so
    /// a second `open`/`openDefault` (e.g. `applicationDidBecomeActive` firing
    /// while the async open is still in flight) is a no-op rather than a
    /// duplicate runtime open.
    @Published public private(set) var isOpening: Bool = false
    /// Refresh floor in seconds (owned by Rust; mirrored for Settings).
    @Published public private(set) var refreshFloorSecs: UInt64 = 300

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
            // SB-3: never more than three burn-first chips.
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

    /// Rust `percent_style`: `left` | `used`.
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

    /// Rust `reset_style`: `countdown` | `exact_clock`.
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

    /// All bridge access is serialized off the main actor through this scheduler
    /// so a Keychain consent sheet can never freeze the UI. `PresentationStore`
    /// itself holds no bridge reference and makes no direct `bridge.` calls.
    private let scheduler: RefreshScheduler
    private var projectedStatusBarRows: [GlanceProviderRow] = []
    private var nextApplyRequest: UInt64 = 0
    private var lastAppliedRequest: UInt64 = 0
    private var lastAppliedGeneration: UInt64 = 0
    private var knownOverviewProviderIDs: Set<String> = []
    private var eventCursor: UInt64 = 0
    private var pollTask: Task<Void, Never>?
    private var screenShareActive: Bool = false
    private var fixtureMode = false
    private var fixtureTerminalProjection: QIFixtureProjection?
    private var fixtureRefreshingProjection: QIFixtureProjection?
    private var fixtureAccountProjections: [String: QIFixtureProjection] = [:]
    private var fixtureRefreshTask: Task<Void, Never>?
    private var launchConfiguration: LaunchConfiguration = .production

    public var usesFixture: Bool { fixtureMode }

    public convenience init() {
        self.init(scheduler: RefreshScheduler())
    }

    /// Designated initializer.
    ///
    /// Tests inject a scheduler wrapping a fake bridge.
    public init(scheduler: RefreshScheduler) {
        self.scheduler = scheduler
        let defaults = UserDefaults.standard
        if let raw = defaults.string(forKey: Self.displayModeKey),
            let mode = StatusItemDisplayMode(rawValue: raw)
        {
            self.displayMode = mode
        } else if defaults.object(forKey: "jackin.desktop.showPercent") != nil {
            // Pre-release migration: old boolean → mode (no long-term shim).
            self.displayMode =
                defaults.bool(forKey: "jackin.desktop.showPercent")
                ? .focusPercent
                : .iconOnly
            defaults.removeObject(forKey: "jackin.desktop.showPercent")
        } else {
            // Burn-first multi-chip strip by default (SB-3 ≤3).
            self.displayMode = .strip
        }
        self.pinnedSurfaceId = defaults.string(forKey: Self.pinnedSurfaceKey) ?? ""
        // SB-3 hard-caps at 3; migrate older UserDefaults >3 down.
        let strip = defaults.object(forKey: Self.stripMaxKey) as? Int ?? statusBarMaxChips
        self.stripMax = max(1, min(statusBarMaxChips, strip))
        let percent = defaults.string(forKey: Self.percentStyleKey) ?? "left"
        self.percentStyle = (percent == "used") ? "used" : "left"
        let reset = defaults.string(forKey: Self.resetStyleKey) ?? "countdown"
        self.resetStyle = (reset == "exact_clock") ? "exact_clock" : "countdown"
        self.hideWhileScreenSharing = defaults.bool(forKey: Self.hideScreenShareKey)
    }

    /// How this launch should open the runtime.
    ///
    /// Smoke mode is defense-in-depth
    /// for the isolated launch test: a non-home data root and no live probes.
    public enum LaunchConfiguration: Sendable, Equatable {
        case production
        case ephemeralSmoke(dataDir: String)

        /// Resolve from the environment: an absolute, non-home
        /// `JACKIN_DESKTOP_SMOKE_DATA_DIR` selects ephemeral smoke; else production.
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

    /// Retry the failed cold open, or refresh when the runtime is already open.
    public func retryLastOperation() {
        guard !fixtureMode else { return }
        if isOpen {
            refreshAll()
        } else {
            openForLaunch(launchConfiguration)
        }
    }

    /// Ephemeral smoke open: isolated path, live probes disabled, exactly one
    /// snapshot application, and no initial/manual/periodic refresh or polling.
    private func openSmoke(dataDir: String) {
        guard !isOpen, !isOpening else { return }
        isOpening = true
        let config = OpenConfig(
            dataDirOverride: dataDir,
            configRootOverride: URL(fileURLWithPath: dataDir)
                .appendingPathComponent("config").path,
            refreshFloorSecs: 300,
            enabledSurfaceIds: [],
            allowLiveProbes: false
        )
        Task { [weak self] in
            guard let self else { return }
            do {
                _ = try await self.scheduler.openRuntime(config: config)
                self.isOpen = true
                self.isOpening = false
                self.lastError = nil
                await self.applySnapshots()
            } catch {
                self.report(error, userMessage: "Usage could not start. Try again.")
                self.isOpen = false
                self.isOpening = false
            }
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

    private func open(
        dataDirOverride: String?,
        configRootOverride: String?,
        refreshFloorSecs: UInt64,
        enabled: [String],
        allowLiveProbes: Bool
    ) {
        // Coalesce duplicate cold-opens: a second open while one is in flight
        // (or already open) is a no-op, so `applicationDidBecomeActive` firing
        // during the async open cannot start a second runtime.
        guard !isOpen, !isOpening else { return }
        isOpening = true
        let config = OpenConfig(
            dataDirOverride: dataDirOverride,
            configRootOverride: configRootOverride,
            refreshFloorSecs: refreshFloorSecs,
            enabledSurfaceIds: enabled,
            allowLiveProbes: allowLiveProbes
        )
        Task { [weak self] in
            guard let self else { return }
            do {
                let floor = try await self.scheduler.openRuntime(config: config)
                self.isOpen = true
                self.isOpening = false
                self.lastError = nil
                self.refreshFloorSecs = floor
                await self.pushFormatPrefs()
                // First load forces network so the bar is not stuck on "refreshing".
                await self.refreshAll(force: true)
                self.startPolling()
            } catch {
                self.report(error, userMessage: "Usage could not start. Try again.")
                self.isOpen = false
                self.isOpening = false
            }
        }
    }

    public func shutdown() {
        pollTask?.cancel()
        pollTask = nil
        fixtureRefreshTask?.cancel()
        fixtureRefreshTask = nil
        // Non-blocking: shutdown runs on the serial queue behind any in-flight
        // bridge op; the main actor never waits on the Rust mutex.
        scheduler.invalidateAndShutdown()
        isOpen = false
        isOpening = false
    }

    public func setEnabled(surfaceId: String, enabled: Bool) {
        guard !fixtureMode else { return }
        Task { [weak self] in
            guard let self else { return }
            do {
                try await self.scheduler.setEnabled(surfaceId: surfaceId, enabled: enabled)
                await self.refreshAll(force: true)
            } catch {
                self.report(error, userMessage: "Provider setting could not be saved.")
            }
        }
    }

    /// Select multi-account identity for a surface (Rust-persisted).
    public func setSelectedAccount(surfaceId: String, accountKey: String) {
        if fixtureMode {
            guard
                let projection = fixtureAccountProjections[
                    fixtureKey(
                        surfaceId: surfaceId,
                        accountKey: accountKey
                    )]
            else {
                return
            }
            applyFixtureProjection(projection)
            fixtureTerminalProjection = projection
            usageAccountSelection = accountKey
            return
        }
        Task { [weak self] in
            guard let self else { return }
            do {
                try await self.scheduler.setSelectedAccount(
                    surfaceId: surfaceId, accountKey: accountKey)
                await self.applySnapshots()
            } catch {
                self.report(error, userMessage: "Account selection could not be saved.")
            }
        }
    }

    /// Accounts for one surface (empty when none known).
    public func accountsForSurface(_ surfaceId: String) -> [AccountRow] {
        accounts.filter { $0.surfaceId == surfaceId }
    }

    /// Inject frozen DATA_CONTRACT / QI presentation without a live bridge poll.
    ///
    /// Used by explicit visual-QA launches so UI automation drives the same
    /// ``PresentationStore`` + SwiftUI surfaces as production.
    /// Does not invent strings — caller supplies Rust-shaped fixtures.
    public func applyQIFixture(
        glanceRows: [GlanceProviderRow],
        statusBarGlanceRows: [GlanceProviderRow]? = nil,
        surfaces: [SurfaceRow],
        accounts: [AccountRow],
        providerGroups: [ProviderGroupRow],
        refreshingProjection: QIFixtureProjection? = nil,
        accountProjections: [String: QIFixtureProjection] = [:],
        popoverSelection: String?,
        usageSelection: String?,
        nextRefreshLabel: String = "next update 4m",
        isLoading: Bool = false,
        isRefreshing: Bool = false,
        lastError: String? = nil
    ) {
        fixtureMode = true
        let projection = QIFixtureProjection(
            glanceRows: glanceRows,
            statusBarGlanceRows: statusBarGlanceRows
                ?? selectStatusBarGlanceRows(from: glanceRows, maxCount: min(3, stripMax)),
            surfaces: surfaces,
            accounts: accounts,
            providerGroups: providerGroups
        )
        fixtureTerminalProjection = projection
        fixtureRefreshingProjection = refreshingProjection
        fixtureAccountProjections = accountProjections
        applyFixtureProjection(projection)
        let providerIDs = Set(providerGroups.map(\.surfaceId))
        knownOverviewProviderIDs = providerIDs
        overviewExpandedProviderIDs = providerIDs
        self.popoverSelection = popoverSelection
        self.usageSelection = usageSelection
        usageAccountSelection =
            accounts.first(where: {
                $0.surfaceId == usageSelection && $0.selected
            })?.accountKey
        self.nextRefreshLabel = nextRefreshLabel
        self.refreshInProgress = isRefreshing
        self.isOpen = true
        self.isOpening = isLoading
        self.lastError = lastError
        reconcileSelections()
        refreshVisibleStatusRows()
    }

    public func setRefreshFloorSecs(_ secs: UInt64) {
        guard !fixtureMode else { return }
        Task { [weak self] in
            guard let self else { return }
            do {
                let floor = try await self.scheduler.setRefreshFloorSecs(secs)
                self.refreshFloorSecs = floor
            } catch {
                self.report(error, userMessage: "Refresh interval could not be saved.")
            }
        }
    }

    /// Manual Refresh button — bypasses floor.
    public func refreshAll() {
        if fixtureMode {
            runFixtureRefresh()
            return
        }
        Task { [weak self] in await self?.refreshAll(force: true) }
    }

    /// Submit refresh intent once.
    ///
    /// Rust broker generations own coalescing.
    private func refreshAll(force: Bool) async {
        await performRefresh(surfaceId: nil, force: force)
    }

    private func performRefresh(surfaceId: String?, force: Bool) async {
        var refreshError: Error?
        do {
            try await scheduler.refresh(surfaceId: surfaceId, force: force)
        } catch {
            refreshError = error
        }
        await applySnapshots()
        if let refreshError, lastError == nil {
            report(refreshError, userMessage: "Usage could not be refreshed. Try again.")
        }
    }

    public func refresh(surfaceId: String) {
        if fixtureMode {
            runFixtureRefresh()
            return
        }
        Task { [weak self] in
            guard let self else { return }
            await self.performRefresh(surfaceId: surfaceId, force: true)
        }
    }

    private func pushFormatPrefs() async {
        guard !fixtureMode, isOpen else { return }
        let prefs = UsageFormatPrefsDto(percentStyle: percentStyle, resetStyle: resetStyle)
        do {
            try await scheduler.setFormatPrefs(prefs)
        } catch {
            report(error, userMessage: "Display setting could not be saved.")
        }
    }

    private func startPolling() {
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
        // Always-on: ask Rust to refresh when the floor allows (force: false).
        // Rust no-ops inside the floor so this is poll-safe every 5s. The whole
        // due-check + refresh + event-drain runs as one serialized bridge op off
        // the main actor, so a consent sheet cannot freeze the UI or queue polls.
        let cursor = eventCursor
        do {
            let nextCursor = try await scheduler.pollOnce(cursor: cursor)
            eventCursor = nextCursor
            await applySnapshots()
        } catch {
            report(error, userMessage: "Usage could not be updated. Try again.")
        }
    }

    /// Poll CGSession for active screen share (privacy collapse).
    ///
    /// AppKit-free.
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

    private func applySnapshots() async {
        guard !fixtureMode else { return }
        nextApplyRequest &+= 1
        let request = nextApplyRequest
        let barMax = UInt32(max(1, min(statusBarMaxChips, stripMax)))
        let projection: DesktopProjectionDto
        do {
            projection = try await scheduler.desktopProjection(statusBarMax: barMax)
        } catch {
            retainLastGoodAfterProjectionFailure(error, request: request)
            return
        }
        guard request >= lastAppliedRequest,
            projection.generation >= lastAppliedGeneration
        else { return }
        lastAppliedRequest = request
        lastAppliedGeneration = projection.generation
        nextRefreshLabel = projection.nextRefreshLabel
        refreshInProgress = projection.refreshInProgress
        discoveryDiagnostics = projection.diagnostics.map { diagnostic in
            DiscoveryDiagnostic(
                surfaceId: diagnostic.surfaceId,
                scopeLabel: diagnostic.scopeLabel,
                issue: diagnostic.issue,
                message: diagnostic.message,
                displayLabel: diagnostic.displayLabel
            )
        }
        let diagnosticBySurface = Dictionary(
            projection.diagnostics.compactMap { diagnostic in
                diagnostic.surfaceId.map { ($0, diagnostic.displayLabel) }
            },
            uniquingKeysWith: { first, _ in first }
        )
        let providerBySurface = Dictionary(
            uniqueKeysWithValues: projection.providers.map { ($0.group.surfaceId, $0) }
        )
        surfaces = projection.surfaces.map { surface in
            guard let provider = providerBySurface[surface.id] else {
                return Self.emptySurface(
                    surface,
                    diagnostic: diagnosticBySurface[surface.id]
                )
            }
            return Self.mapSurface(
                surface,
                view: provider.selectedUsage,
                diagnostic: diagnosticBySurface[surface.id]
            )
        }
        providerGroups = projection.providers.map { provider in
            let accountRows = provider.group.accounts.map(Self.mapAccountDto)
            return ProviderGroupRow(
                surfaceId: provider.group.surfaceId,
                displayLabel: provider.group.displayLabel,
                iconKey: provider.group.iconKey,
                fallbackGlyph: provider.group.fallbackGlyph,
                usageURL: provider.group.usageUrl,
                accountColumnLabel: provider.group.accountColumnLabel,
                planOrStatusLabel: provider.group.planOrStatusLabel,
                remainingLabel: provider.group.remainingLabel,
                resetDisplayLabel: provider.group.resetDisplayLabel,
                accounts: accountRows,
                accessibilityLabel: provider.group.accessibilityLabel,
                lastError: provider.group.emptyState?.lastError
            )
        }
        let providerIDs = Set(providerGroups.map(\.surfaceId))
        overviewExpandedProviderIDs.formUnion(providerIDs.subtracting(knownOverviewProviderIDs))
        overviewExpandedProviderIDs.formIntersection(providerIDs)
        knownOverviewProviderIDs = providerIDs
        accounts = providerGroups.flatMap(\.accounts)
        providerGlanceRows = projection.glanceRows.map(Self.mapGlanceDto)
        projectedStatusBarRows = projection.statusBarGlanceRows.map(Self.mapGlanceDto)
        refreshVisibleStatusRows()
        reconcileSelections()
        lastError = projection.errorMessage
    }

    private func retainLastGoodAfterProjectionFailure(_ error: Error, request: UInt64) {
        guard request >= lastAppliedRequest else { return }
        report(error, userMessage: "Usage could not be updated. Try again.")
    }

    /// Test seam for the same transient-failure path used by `applySnapshots`.
    func applyProjectionFailureForTesting(_ error: Error, request: UInt64 = .max) {
        retainLastGoodAfterProjectionFailure(error, request: request)
    }

    private func fixtureKey(surfaceId: String, accountKey: String) -> String {
        "\(surfaceId)#\(accountKey)"
    }

    private func applyFixtureProjection(_ projection: QIFixtureProjection) {
        providerGlanceRows = projection.glanceRows
        projectedStatusBarRows = projection.statusBarGlanceRows
        surfaces = projection.surfaces
        accounts = projection.accounts
        providerGroups = projection.providerGroups
        refreshVisibleStatusRows()
    }

    private func runFixtureRefresh() {
        guard let refreshing = fixtureRefreshingProjection,
            let terminal = fixtureTerminalProjection
        else { return }
        fixtureRefreshTask?.cancel()
        applyFixtureProjection(refreshing)
        refreshInProgress = true
        fixtureRefreshTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(2))
            guard !Task.isCancelled, let self else { return }
            self.applyFixtureProjection(terminal)
            self.refreshInProgress = false
        }
    }

    private static func emptySurface(
        _ surface: SurfaceDescriptorDto,
        diagnostic: String?
    ) -> SurfaceRow {
        SurfaceRow(
            id: surface.id,
            label: surface.label,
            enabled: surface.enabled,
            statusBarLabel: "",
            status: "",
            accountLabel: "",
            username: nil,
            planLabel: nil,
            credentialOrigin: nil,
            estimateCaption: nil,
            buckets: [],
            updatedLabel: "",
            lastError: diagnostic,
            detailPresentation: .empty
        )
    }

    private static func mapSurface(
        _ surface: SurfaceDescriptorDto,
        view: UsageViewDto,
        diagnostic: String?
    ) -> SurfaceRow {
        SurfaceRow(
            id: surface.id,
            label: view.identity.providerTitle,
            enabled: surface.enabled,
            statusBarLabel: view.statusBarLabel,
            status: view.status,
            accountLabel: view.accountLabel,
            username: view.username,
            planLabel: view.planLabel,
            credentialOrigin: view.credentialOrigin,
            estimateCaption: view.estimateCaption,
            buckets: view.buckets.map(Self.mapBucketDto),
            updatedLabel: view.updatedLabel,
            lastError: view.lastError ?? diagnostic,
            detailPresentation: UsageDetailPresentation(dto: view.detailPresentation),
            identity: IdentityRow(dto: view.identity)
        )
    }

    private static func mapBucketDto(_ bucket: QuotaBucketDto) -> BucketRow {
        BucketRow(
            label: bucket.label,
            usedLabel: bucket.usedLabel,
            limitLabel: bucket.limitLabel,
            remainingPercent: bucket.remainingPercent,
            resetLabel: bucket.resetLabel,
            paceLabel: bucket.paceLabel,
            statusSlot: bucket.statusSlot,
            severity: bucket.severity,
            status: bucket.status,
            usedMoney: bucket.usedMoney,
            limitMoney: bucket.limitMoney,
            remainingLabel: bucket.remainingLabel,
            displaySegments: bucket.displaySegments,
            displayLabel: bucket.displayLabel,
            meterPercent: bucket.meterPercent
        )
    }

    private static func mapAccountDto(_ row: AccountDescriptorDto) -> AccountRow {
        AccountRow(
            surfaceId: row.surfaceId,
            providerColumnLabel: row.providerColumnLabel,
            accountKey: row.accountKey,
            accountLabel: row.accountLabel,
            planLabel: row.planLabel,
            selected: row.selected,
            lifecycle: row.lifecycle,
            lifecycleLabel: row.lifecycleLabel,
            provenanceLabel: row.provenanceLabel,
            planOrStatusLabel: row.planOrStatusLabel,
            remainingPercent: row.remainingPercent,
            remainingLabel: row.remainingLabel,
            headline: row.headline,
            resetDisplayLabel: row.resetDisplayLabel,
            statusWord: row.statusWord,
            statusLabel: row.statusLabel,
            severity: row.severity,
            updatedLabel: row.updatedLabel,
            lastError: row.lastError,
            dimmed: row.dimmed,
            accessibilityLabel: row.accessibilityLabel
        )
    }

    private static func mapGlanceDto(_ row: ProviderGlanceRowDto) -> GlanceProviderRow {
        GlanceProviderRow(
            surfaceId: row.surfaceId,
            iconKey: row.iconKey,
            fallbackGlyph: row.fallbackGlyph,
            usageURL: row.usageUrl,
            displayLabel: row.displayLabel,
            accountLabel: row.accountLabel,
            planLabel: row.planLabel,
            glanceRemainingPercent: row.glanceRemainingPercent,
            barLabel: row.barLabel,
            headline: row.headline,
            resetLabel: row.resetLabel,
            compactResetLabel: row.compactResetLabel,
            exactReset: row.exactReset,
            statusWord: row.statusWord,
            isRefreshing: row.isRefreshing,
            statusLabel: row.statusLabel,
            severity: row.severity,
            updatedLabel: row.updatedLabel,
            activityLabel: row.activityLabel,
            activityKind: row.activityKind,
            accessibilityLabel: row.accessibilityLabel,
            lastError: row.lastError,
            dimmed: row.dimmed
        )
    }

    /// Open the Usage window on Overview or a specific surface.
    public func selectUsageSurface(_ surfaceId: String?) {
        selectUsageContext(surfaceId: surfaceId, accountKey: nil)
    }

    /// Open Usage on one exact canonical provider/account context.
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

    private func reconcileSelections() {
        if let usageSelection, !isNavigableSurface(usageSelection) {
            self.usageSelection = nil
            usageAccountSelection = nil
        } else if let usageSelection,
            let usageAccountSelection,
            !accounts.contains(where: {
                $0.surfaceId == usageSelection && $0.accountKey == usageAccountSelection
            })
        {
            self.usageAccountSelection =
                accounts.first(where: {
                    $0.surfaceId == usageSelection && $0.selected
                })?.accountKey
        }
        if let popoverSelection,
            !providerGlanceRows.contains(where: { $0.surfaceId == popoverSelection })
        {
            self.popoverSelection = providerGlanceRows.first?.surfaceId
        }
    }

    private func isNavigableSurface(_ surfaceId: String) -> Bool {
        providerGlanceRows.contains(where: { $0.surfaceId == surfaceId })
            && surfaces.contains(where: { $0.id == surfaceId && $0.enabled })
    }

    private func refreshVisibleStatusRows() {
        switch displayMode {
        case .iconOnly:
            statusBarGlanceRows = []
        case .focusPercent:
            statusBarGlanceRows = Array(projectedStatusBarRows.prefix(1))
        case .pinnedSurface:
            statusBarGlanceRows = providerGlanceRows.filter { $0.surfaceId == pinnedSurfaceId }
        case .strip:
            statusBarGlanceRows = Array(projectedStatusBarRows.prefix(stripMax))
        }
    }

    private func report(_ error: Error, userMessage: String) {
        NSLog("jackin desktop bridge error: %@", String(describing: error))
        lastError = userMessage
    }
}
