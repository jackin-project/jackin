// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import CoreGraphics
import Foundation

/// Status-item display mode (Settings-selectable; Rust supplies every string).
public enum StatusItemDisplayMode: String, CaseIterable, Sendable {
    case iconOnly
    case focusPercent
    case pinnedSurface
    case strip
}

/// Pure mode → which Rust accessor to call (unit-testable; no bridge).
public enum StatusItemTextSelection: Equatable, Sendable {
    case empty
    case focus
    case pinned(surfaceId: String)
    case strip(max: UInt32)
}

/// Select the status-item text source from prefs. Empty when icon-only or
/// screen-share collapse is active; pinned without an id falls back to empty.
public func statusItemTextSelection(
    mode: StatusItemDisplayMode,
    pinnedSurfaceId: String?,
    stripMax: Int,
    hideForScreenShare: Bool
) -> StatusItemTextSelection {
    if hideForScreenShare {
        return .empty
    }
    switch mode {
    case .iconOnly:
        return .empty
    case .focusPercent:
        return .focus
    case .pinnedSurface:
        guard let id = pinnedSurfaceId, !id.isEmpty else {
            return .empty
        }
        return .pinned(surfaceId: id)
    case .strip:
        let cap = UInt32(max(1, min(8, stripMax)))
        return .strip(max: cap)
    }
}

/// Thin presentation store: polls Rust UniFFI snapshots; no provider probes.
@MainActor
public final class PresentationStore: ObservableObject {
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
        public let displayLabel: String
        public let accountLabel: String
        public let planLabel: String?
        public let glanceRemainingPercent: UInt8?
        public let barLabel: String
        public let headline: String
        public let resetLabel: String?
        public let exactReset: String?
        public let statusWord: String
        public let isRefreshing: Bool
        public let statusLabel: String
        public let severity: String
        public let updatedLabel: String
        public let lastError: String?
        public let dimmed: Bool
    }

    public struct OverviewRow: Identifiable, Sendable, Equatable {
        public var id: String { surfaceId }
        public let surfaceId: String
        public let displayLabel: String
        public let headline: String
        public let resetLabel: String?
        public let exactReset: String?
        public let statusWord: String
        public let severity: String
    }

    /// Multi-account row for a host surface (Rust-owned keys/labels).
    public struct AccountRow: Identifiable, Sendable, Equatable {
        public var id: String { "\(surfaceId)#\(accountKey)" }
        public let surfaceId: String
        public let accountKey: String
        public let accountLabel: String
        public let planLabel: String?
        public let selected: Bool
        public let remainingPercent: UInt8?
        public let statusWord: String
    }

    @Published public private(set) var mergedBarLabel: String = "jackin❯ usage"
    /// Rust-owned short status-item label for focus mode (e.g. `Cl 37%` remaining).
    @Published public private(set) var compactBarLabel: String = ""
    /// Mode-selected status-item text (empty = icon only). Accessibility + fallback.
    @Published public private(set) var statusItemText: String = ""
    /// OpenUsage-style menu-bar chips (Rust compact labels + remaining for mini bars).
    @Published public private(set) var statusItemChips: [StatusItemChip] = []
    /// Footer / window next-refresh string from Rust.
    @Published public private(set) var nextRefreshLabel: String = ""
    @Published public private(set) var surfaces: [SurfaceRow] = []
    /// Rust-owned seven-provider glance rows (auto-detected, canonical order).
    @Published public private(set) var providerGlanceRows: [GlanceProviderRow] = []
    /// Presentation-only privacy flag: `false` hides the Rust status-bar values
    /// during screen sharing (it may hide a Rust label, never replace it).
    @Published public private(set) var statusBarShowsValues = true
    @Published public private(set) var overviewRows: [OverviewRow] = []
    /// Known accounts across surfaces (multi-account host logins / shared snapshots).
    @Published public private(set) var accounts: [AccountRow] = []
    /// Sidebar / detail selection: `nil` = Overview, else surface id.
    @Published public var usageSelection: String?
    /// Popover tab selection: `nil` = Overview, else provider surface id.
    @Published public var popoverSelection: String?
    /// True only while an enqueued refresh request runs its bridge operation —
    /// drives the popover/footer spinner. Never clears glance rows or surfaces.
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
            Task { [weak self] in await self?.applyStatusItemText() }
        }
    }

    @Published public var pinnedSurfaceId: String {
        didSet {
            UserDefaults.standard.set(pinnedSurfaceId, forKey: Self.pinnedSurfaceKey)
            Task { [weak self] in await self?.applyStatusItemText() }
        }
    }

    @Published public var stripMax: Int {
        didSet {
            let clamped = max(1, min(8, stripMax))
            if clamped != stripMax {
                stripMax = clamped
                return
            }
            UserDefaults.standard.set(stripMax, forKey: Self.stripMaxKey)
            Task { [weak self] in await self?.applyStatusItemText() }
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
            Task { [weak self] in await self?.applyStatusItemText() }
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
    /// Per-surface compact status-bar label captured during the last projection,
    /// so status-item chip building needs no further bridge round-trips on main.
    private var compactLabelBySurface: [String: String] = [:]
    private var eventCursor: UInt64 = 0
    private var pollTask: Task<Void, Never>?
    private var refreshTask: Task<Void, Never>?
    private var screenShareActive: Bool = false

    public convenience init() {
        self.init(scheduler: RefreshScheduler())
    }

    /// Designated initializer. Tests inject a scheduler wrapping a fake bridge.
    public init(scheduler: RefreshScheduler) {
        self.scheduler = scheduler
        let defaults = UserDefaults.standard
        if let raw = defaults.string(forKey: Self.displayModeKey),
           let mode = StatusItemDisplayMode(rawValue: raw)
        {
            self.displayMode = mode
        } else if defaults.object(forKey: "jackin.desktop.showPercent") != nil {
            // Pre-release migration: old boolean → mode (no long-term shim).
            self.displayMode = defaults.bool(forKey: "jackin.desktop.showPercent")
                ? .focusPercent
                : .iconOnly
            defaults.removeObject(forKey: "jackin.desktop.showPercent")
        } else {
            // OpenUsage-like multi-metric strip by default (worst-first, cap stripMax).
            self.displayMode = .strip
        }
        self.pinnedSurfaceId = defaults.string(forKey: Self.pinnedSurfaceKey) ?? ""
        // Default cap 8 = full frozen host catalog (OpenUsage: every enabled provider).
        let strip = defaults.object(forKey: Self.stripMaxKey) as? Int ?? 8
        self.stripMax = max(1, min(8, strip))
        let percent = defaults.string(forKey: Self.percentStyleKey) ?? "left"
        self.percentStyle = (percent == "used") ? "used" : "left"
        let reset = defaults.string(forKey: Self.resetStyleKey) ?? "countdown"
        self.resetStyle = (reset == "exact_clock") ? "exact_clock" : "countdown"
        self.hideWhileScreenSharing = defaults.bool(forKey: Self.hideScreenShareKey)
    }

    /// True when every enabled surface is stale/unavailable/error (dims status item).
    public var allEnabledSurfacesDegraded: Bool {
        let enabled = surfaces.filter(\.enabled)
        guard !enabled.isEmpty else { return true }
        return enabled.allSatisfy { row in
            switch row.status {
            case "fresh", "refreshing":
                return false
            default:
                return true
            }
        }
    }

    /// How this launch should open the runtime. Smoke mode is defense-in-depth
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
        switch configuration {
        case .production:
            openDefault()
        case .ephemeralSmoke(let dataDir):
            openSmoke(dataDir: dataDir)
        }
    }

    public func openDefault() {
        let home = FileManager.default.homeDirectoryForCurrentUser
        let dataDir = home.appendingPathComponent(".jackin/data").path
        open(dataDir: dataDir, refreshFloorSecs: 300, enabled: [])
    }

    /// Ephemeral smoke open: isolated path, live probes disabled, exactly one
    /// snapshot application, and no initial/manual/periodic refresh or polling.
    private func openSmoke(dataDir: String) {
        guard !isOpen, !isOpening else { return }
        isOpening = true
        let config = OpenConfig(
            dataDir: dataDir,
            refreshFloorSecs: 300,
            enabledSurfaceIds: [],
            allowLiveProbes: false
        )
        Task { [weak self] in
            guard let self else { return }
            do {
                _ = try await self.scheduler.run { handle -> UInt64 in
                    try handle.openRuntime(config: config)
                    return try handle.refreshFloorSecs()
                }
                self.isOpen = true
                self.isOpening = false
                self.lastError = nil
                await self.applySnapshots()
            } catch {
                self.lastError = String(describing: error)
                self.isOpen = false
                self.isOpening = false
            }
        }
    }

    public func open(dataDir: String, refreshFloorSecs: UInt64, enabled: [String]) {
        // Coalesce duplicate cold-opens: a second open while one is in flight
        // (or already open) is a no-op, so `applicationDidBecomeActive` firing
        // during the async open cannot start a second runtime.
        guard !isOpen, !isOpening else { return }
        isOpening = true
        let config = OpenConfig(
            dataDir: dataDir,
            refreshFloorSecs: refreshFloorSecs,
            enabledSurfaceIds: enabled,
            allowLiveProbes: true
        )
        Task { [weak self] in
            guard let self else { return }
            do {
                let floor = try await self.scheduler.run { handle -> UInt64 in
                    try handle.openRuntime(config: config)
                    return try handle.refreshFloorSecs()
                }
                self.isOpen = true
                self.isOpening = false
                self.lastError = nil
                self.refreshFloorSecs = floor
                await self.pushFormatPrefs()
                // First load forces network so the bar is not stuck on "refreshing".
                await self.refreshAll(force: true)
                self.startPolling()
            } catch {
                self.lastError = String(describing: error)
                self.isOpen = false
                self.isOpening = false
            }
        }
    }

    public func shutdown() {
        pollTask?.cancel()
        pollTask = nil
        refreshTask?.cancel()
        refreshTask = nil
        // Non-blocking: shutdown runs on the serial queue behind any in-flight
        // bridge op; the main actor never waits on the Rust mutex.
        scheduler.invalidateAndShutdown()
        isOpen = false
        isOpening = false
    }

    public func setEnabled(surfaceId: String, enabled: Bool) {
        Task { [weak self] in
            guard let self else { return }
            do {
                try await self.scheduler.run { try $0.setEnabled(surfaceId: surfaceId, enabled: enabled) }
                await self.refreshAll(force: true)
            } catch {
                self.lastError = String(describing: error)
            }
        }
    }

    /// Select multi-account identity for a surface (Rust-persisted).
    public func setSelectedAccount(surfaceId: String, accountKey: String) {
        Task { [weak self] in
            guard let self else { return }
            do {
                try await self.scheduler.run {
                    try $0.setSelectedAccount(surfaceId: surfaceId, accountKey: accountKey)
                }
                await self.applySnapshots()
            } catch {
                self.lastError = String(describing: error)
            }
        }
    }

    /// Accounts for one surface (empty when none known).
    public func accountsForSurface(_ surfaceId: String) -> [AccountRow] {
        accounts.filter { $0.surfaceId == surfaceId }
    }

    public func setRefreshFloorSecs(_ secs: UInt64) {
        Task { [weak self] in
            guard let self else { return }
            do {
                let floor = try await self.scheduler.run { handle -> UInt64 in
                    try handle.setRefreshFloorSecs(secs: secs)
                    return try handle.refreshFloorSecs()
                }
                self.refreshFloorSecs = floor
            } catch {
                self.lastError = String(describing: error)
            }
        }
    }

    /// Manual Refresh button — bypasses floor.
    public func refreshAll() {
        Task { [weak self] in await self?.refreshAll(force: true) }
    }

    /// Coalesce overlapping refresh requests into one in-flight task so a
    /// consent sheet cannot build a prompt storm.
    private func refreshAll(force: Bool) async {
        refreshTask?.cancel()
        let task = Task { [weak self] in
            guard let self else { return }
            // Refresh-request activity drives the spinner; other bridge commands
            // (open/poll/settings/account/shutdown) never set it.
            self.refreshInProgress = true
            do {
                try await self.scheduler.run { try $0.refresh(surfaceId: nil, force: force) }
            } catch {
                self.lastError = String(describing: error)
            }
            await self.applySnapshots()
            self.refreshInProgress = false
        }
        refreshTask = task
        await task.value
    }

    public func refresh(surfaceId: String) {
        Task { [weak self] in
            guard let self else { return }
            self.refreshInProgress = true
            do {
                try await self.scheduler.run { try $0.refresh(surfaceId: surfaceId, force: true) }
                await self.applySnapshots()
            } catch {
                self.lastError = String(describing: error)
            }
            self.refreshInProgress = false
        }
    }

    private func pushFormatPrefs() async {
        guard isOpen else { return }
        let prefs = UsageFormatPrefsDto(percentStyle: percentStyle, resetStyle: resetStyle)
        do {
            try await scheduler.run { try $0.setFormatPrefs(prefs: prefs) }
        } catch {
            lastError = String(describing: error)
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
        guard isOpen else { return }
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
            let nextCursor = try await scheduler.run { handle -> UInt64 in
                if try handle.refreshDue() {
                    try handle.refresh(surfaceId: nil, force: false)
                }
                return try handle.nextEvents(cursor: cursor, max: 64).nextCursor
            }
            eventCursor = nextCursor
            await applySnapshots()
        } catch {
            lastError = String(describing: error)
        }
    }

    /// Poll CGSession for active screen share (privacy collapse). AppKit-free.
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
        let projection: BridgeProjection
        do {
            projection = try await scheduler.run { handle -> BridgeProjection in
                let merged = try handle.mergedStatusBarLabel()
                let compact = try handle.compactStatusBarLabel()
                let nextRefresh = try handle.nextRefreshLabel()
                let listed = try handle.listSurfaces()
                var surfaces: [SurfaceProjection] = []
                for surface in listed {
                    let view = surface.enabled ? try? handle.snapshot(surfaceId: surface.id) : nil
                    let compactFor =
                        surface.enabled
                        ? ((try? handle.compactStatusBarLabelFor(surfaceId: surface.id)) ?? "")
                        : ""
                    surfaces.append(
                        SurfaceProjection(info: surface, view: view, compactLabel: compactFor)
                    )
                }
                let overview = try handle.overviewRows()
                let accounts = (try? handle.listAccounts(surfaceId: nil)) ?? []
                let glanceRows = (try? handle.providerGlanceRows()) ?? []
                return BridgeProjection(
                    mergedBarLabel: merged,
                    compactBarLabel: compact,
                    nextRefreshLabel: nextRefresh,
                    surfaces: surfaces,
                    overviewRows: overview,
                    accounts: accounts,
                    glanceRows: glanceRows
                )
            }
        } catch {
            lastError = String(describing: error)
            return
        }

        mergedBarLabel = projection.mergedBarLabel
        compactBarLabel = projection.compactBarLabel
        nextRefreshLabel = projection.nextRefreshLabel
        var labelBySurface: [String: String] = [:]
        surfaces = projection.surfaces.map { entry in
            let surface = entry.info
            labelBySurface[surface.id] = entry.compactLabel
            guard surface.enabled else {
                return SurfaceRow(
                    id: surface.id,
                    label: surface.label,
                    enabled: false,
                    statusBarLabel: "",
                    status: "disabled",
                    accountLabel: "",
                    username: nil,
                    planLabel: nil,
                    credentialOrigin: nil,
                    estimateCaption: nil,
                    buckets: [],
                    updatedLabel: "",
                    lastError: nil
                )
            }
            guard let view = entry.view else {
                return SurfaceRow(
                    id: surface.id,
                    label: surface.label,
                    enabled: true,
                    statusBarLabel: "unavailable",
                    status: "unavailable",
                    accountLabel: "",
                    username: nil,
                    planLabel: nil,
                    credentialOrigin: nil,
                    estimateCaption: nil,
                    buckets: [],
                    updatedLabel: "",
                    lastError: nil
                )
            }
            return SurfaceRow(
                id: surface.id,
                label: surface.label,
                enabled: true,
                statusBarLabel: view.statusBarLabel,
                status: view.status,
                accountLabel: view.accountLabel,
                username: view.username,
                planLabel: view.planLabel,
                credentialOrigin: view.credentialOrigin,
                estimateCaption: view.estimateCaption,
                buckets: view.buckets.map { bucket in
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
                },
                updatedLabel: view.updatedLabel,
                lastError: view.lastError
            )
        }
        compactLabelBySurface = labelBySurface
        overviewRows = projection.overviewRows.map { row in
            OverviewRow(
                surfaceId: row.surfaceId,
                displayLabel: row.displayLabel,
                headline: row.headline,
                resetLabel: row.resetLabel,
                exactReset: row.exactReset,
                statusWord: row.statusWord,
                severity: row.severity
            )
        }
        accounts = projection.accounts.map { row in
            AccountRow(
                surfaceId: row.surfaceId,
                accountKey: row.accountKey,
                accountLabel: row.accountLabel,
                planLabel: row.planLabel,
                selected: row.selected,
                remainingPercent: row.remainingPercent,
                statusWord: row.statusWord
            )
        }
        // Rust owns detection, ordering, and every string — project verbatim.
        providerGlanceRows = projection.glanceRows.map { row in
            GlanceProviderRow(
                surfaceId: row.surfaceId,
                iconKey: row.iconKey,
                displayLabel: row.displayLabel,
                accountLabel: row.accountLabel,
                planLabel: row.planLabel,
                glanceRemainingPercent: row.glanceRemainingPercent,
                barLabel: row.barLabel,
                headline: row.headline,
                resetLabel: row.resetLabel,
                exactReset: row.exactReset,
                statusWord: row.statusWord,
                isRefreshing: row.isRefreshing,
                statusLabel: row.statusLabel,
                severity: row.severity,
                updatedLabel: row.updatedLabel,
                lastError: row.lastError,
                dimmed: row.dimmed
            )
        }
        lastError = nil
        await applyStatusItemText()
    }

    /// Open the Usage window on Overview or a specific surface.
    public func selectUsageSurface(_ surfaceId: String?) {
        usageSelection = surfaceId
    }

    private func applyStatusItemText() async {
        let selection = statusItemTextSelection(
            mode: displayMode,
            pinnedSurfaceId: pinnedSurfaceId.isEmpty ? nil : pinnedSurfaceId,
            stripMax: stripMax,
            hideForScreenShare: hideWhileScreenSharing && screenShareActive
        )
        guard isOpen else {
            statusItemText = ""
            statusItemChips = []
            return
        }
        do {
            switch selection {
            case .empty:
                statusItemText = ""
                statusItemChips = []
            case .focus:
                // Single worst provider preview (still a per-provider chip).
                statusItemText = try await scheduler.run { try $0.compactStatusBarLabel() }
                statusItemChips = chipsForProviderPreview(maxCount: 1, preferWorstFirst: true)
            case .pinned(let surfaceId):
                if let cached = compactLabelBySurface[surfaceId] {
                    statusItemText = cached
                } else {
                    statusItemText =
                        (try await scheduler.run {
                            try $0.compactStatusBarLabelFor(surfaceId: surfaceId)
                        }) ?? ""
                }
                statusItemChips = chipsForPinned(surfaceId: surfaceId)
            case .strip(let max):
                // CodexBar-style: one chip per provider (catalog order).
                statusItemText = try await scheduler.run { try $0.compactStatusBarStrip(max: max) }
                statusItemChips = chipsForProviderPreview(
                    maxCount: Int(max),
                    preferWorstFirst: false
                )
            }
        } catch {
            lastError = String(describing: error)
            statusItemText = ""
            statusItemChips = []
        }
    }

    private func chipsForPinned(surfaceId: String) -> [StatusItemChip] {
        let label = compactLabelBySurface[surfaceId] ?? ""
        guard !label.isEmpty else {
            return []
        }
        guard let row = surfaces.first(where: { $0.id == surfaceId && $0.enabled }) else {
            return [
                StatusItemChip(
                    surfaceId: surfaceId,
                    glyph: statusItemGlyph(compactLabel: label, surfaceId: surfaceId),
                    systemImage: statusItemSystemImage(surfaceId: surfaceId),
                    percentLines: [],
                    compactLabel: label,
                    remainingPercent: nil,
                    remainingPerLine: [],
                    severity: "ok"
                ),
            ]
        }
        return [makeChip(row: row, compactLabel: label)]
    }

    /// One status-item chip per enabled provider (OpenUsage strip: icon + remaining %).
    ///
    /// Strip mode includes all enabled hosts (cap `maxCount`); focus mode only those
    /// with numeric remaining / preview data, worst-first. Uses the per-surface
    /// compact labels captured during the last projection — no bridge round-trip.
    private func chipsForProviderPreview(maxCount: Int, preferWorstFirst: Bool)
        -> [StatusItemChip]
    {
        let snaps = surfaceSnapshotsForStatusItem()
        return buildStatusItemChips(
            surfaces: snaps,
            maxCount: maxCount,
            preferWorstFirst: preferWorstFirst,
            percentStyle: percentStyle,
            // Catalog strip: show every enabled provider icon; focus: data only.
            includeAllEnabled: !preferWorstFirst
        )
    }

    private func surfaceSnapshotsForStatusItem() -> [StatusItemSurfaceSnapshot] {
        var snaps: [StatusItemSurfaceSnapshot] = []
        for row in surfaces {
            let compact = compactLabelBySurface[row.id] ?? ""
            let pairs: [(UInt8, String)] = row.buckets.compactMap { bucket in
                guard let rem = bucket.remainingPercent else { return nil }
                return (rem, bucket.severity)
            }
            snaps.append(
                StatusItemSurfaceSnapshot(
                    surfaceId: row.id,
                    label: row.label,
                    enabled: row.enabled,
                    statusBarLabel: row.statusBarLabel,
                    status: row.status,
                    compactLabel: compact,
                    remainings: pairs.map(\.0),
                    severities: pairs.map(\.1)
                )
            )
        }
        return snaps
    }

    private func makeChip(row: SurfaceRow, compactLabel: String) -> StatusItemChip {
        let pairs: [(UInt8, String)] = row.buckets.compactMap { bucket in
            guard let rem = bucket.remainingPercent else { return nil }
            return (rem, bucket.severity)
        }
        let snap = StatusItemSurfaceSnapshot(
            surfaceId: row.id,
            label: row.label,
            enabled: row.enabled,
            statusBarLabel: row.statusBarLabel,
            status: row.status,
            compactLabel: compactLabel,
            remainings: pairs.map(\.0),
            severities: pairs.map(\.1)
        )
        return buildStatusItemChips(
            surfaces: [snap],
            maxCount: 1,
            preferWorstFirst: false,
            percentStyle: percentStyle
        ).first
            ?? StatusItemChip(
                surfaceId: row.id,
                glyph: statusItemGlyph(compactLabel: compactLabel, surfaceId: row.id),
                systemImage: statusItemSystemImage(surfaceId: row.id),
                percentLines: [],
                compactLabel: compactLabel,
                remainingPercent: nil,
                remainingPerLine: [],
                severity: "ok"
            )
    }

    private func drivingBucket(for row: SurfaceRow) -> BucketRow? {
        let numeric = row.buckets.compactMap { bucket -> (UInt8, BucketRow)? in
            guard let rem = bucket.remainingPercent else { return nil }
            return (rem, bucket)
        }
        guard let best = drivingBucketForStatusItem(
            remainingAndSeverity: numeric.map { (remaining: $0.0, severity: $0.1.severity) }
        ) else {
            return nil
        }
        return numeric.first(where: { $0.0 == best.remaining })?.1
    }
}

/// One surface's raw bridge projection: descriptor, snapshot (nil when
/// disabled/unavailable), and its compact status-bar label — all captured in a
/// single serialized off-main bridge batch.
private struct SurfaceProjection: Sendable {
    let info: SurfaceDescriptorDto
    let view: UsageViewDto?
    let compactLabel: String
}

/// The full set of raw bridge outputs `applySnapshots` needs, collected in one
/// serialized off-main batch so the `@MainActor` mapping does zero bridge work.
private struct BridgeProjection: Sendable {
    let mergedBarLabel: String
    let compactBarLabel: String
    let nextRefreshLabel: String
    let surfaces: [SurfaceProjection]
    let overviewRows: [OverviewRowDto]
    let accounts: [AccountDescriptorDto]
    let glanceRows: [ProviderGlanceRowDto]
}
