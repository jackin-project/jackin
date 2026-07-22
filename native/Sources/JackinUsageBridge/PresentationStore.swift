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
    @Published public private(set) var overviewRows: [OverviewRow] = []
    /// Known accounts across surfaces (multi-account host logins / shared snapshots).
    @Published public private(set) var accounts: [AccountRow] = []
    /// Sidebar / detail selection: `nil` = Overview, else surface id.
    @Published public var usageSelection: String?
    @Published public private(set) var lastError: String?
    @Published public private(set) var isOpen: Bool = false
    /// Refresh floor in seconds (owned by Rust; mirrored for Settings).
    @Published public private(set) var refreshFloorSecs: UInt64 = 300

    @Published public var displayMode: StatusItemDisplayMode {
        didSet {
            UserDefaults.standard.set(displayMode.rawValue, forKey: Self.displayModeKey)
            applyStatusItemText()
        }
    }

    @Published public var pinnedSurfaceId: String {
        didSet {
            UserDefaults.standard.set(pinnedSurfaceId, forKey: Self.pinnedSurfaceKey)
            applyStatusItemText()
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
            applyStatusItemText()
        }
    }

    /// Rust `percent_style`: `left` | `used`.
    @Published public var percentStyle: String {
        didSet {
            UserDefaults.standard.set(percentStyle, forKey: Self.percentStyleKey)
            pushFormatPrefs()
            if isOpen {
                applySnapshots()
            }
        }
    }

    /// Rust `reset_style`: `countdown` | `exact_clock`.
    @Published public var resetStyle: String {
        didSet {
            UserDefaults.standard.set(resetStyle, forKey: Self.resetStyleKey)
            pushFormatPrefs()
            if isOpen {
                applySnapshots()
            }
        }
    }

    @Published public var hideWhileScreenSharing: Bool {
        didSet {
            UserDefaults.standard.set(hideWhileScreenSharing, forKey: Self.hideScreenShareKey)
            applyStatusItemText()
        }
    }

    private static let displayModeKey = "jackin.desktop.displayMode"
    private static let pinnedSurfaceKey = "jackin.desktop.pinnedSurfaceId"
    private static let stripMaxKey = "jackin.desktop.stripMax"
    private static let percentStyleKey = "jackin.desktop.percentStyle"
    private static let resetStyleKey = "jackin.desktop.resetStyle"
    private static let hideScreenShareKey = "jackin.desktop.hideWhileScreenSharing"

    private let bridge = UsageMenuBarBridge.create()
    private var eventCursor: UInt64 = 0
    private var pollTask: Task<Void, Never>?
    private var screenShareActive: Bool = false

    public init() {
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

    public func openDefault() {
        let home = FileManager.default.homeDirectoryForCurrentUser
        let dataDir = home.appendingPathComponent(".jackin/data").path
        open(dataDir: dataDir, refreshFloorSecs: 300, enabled: [])
    }

    public func open(dataDir: String, refreshFloorSecs: UInt64, enabled: [String]) {
        do {
            try bridge.openRuntime(
                config: OpenConfig(
                    dataDir: dataDir,
                    refreshFloorSecs: refreshFloorSecs,
                    enabledSurfaceIds: enabled
                )
            )
            isOpen = true
            lastError = nil
            self.refreshFloorSecs = try bridge.refreshFloorSecs()
            pushFormatPrefs()
            // First load forces network so the bar is not stuck on "refreshing".
            refreshAll(force: true)
            startPolling()
        } catch {
            lastError = String(describing: error)
            isOpen = false
        }
    }

    public func shutdown() {
        pollTask?.cancel()
        pollTask = nil
        do {
            try bridge.shutdown()
        } catch {
            lastError = String(describing: error)
        }
        isOpen = false
    }

    public func setEnabled(surfaceId: String, enabled: Bool) {
        do {
            try bridge.setEnabled(surfaceId: surfaceId, enabled: enabled)
            refreshAll(force: true)
        } catch {
            lastError = String(describing: error)
        }
    }

    /// Select multi-account identity for a surface (Rust-persisted).
    public func setSelectedAccount(surfaceId: String, accountKey: String) {
        do {
            try bridge.setSelectedAccount(surfaceId: surfaceId, accountKey: accountKey)
            applySnapshots()
        } catch {
            lastError = String(describing: error)
        }
    }

    /// Accounts for one surface (empty when none known).
    public func accountsForSurface(_ surfaceId: String) -> [AccountRow] {
        accounts.filter { $0.surfaceId == surfaceId }
    }

    public func setRefreshFloorSecs(_ secs: UInt64) {
        do {
            try bridge.setRefreshFloorSecs(secs: secs)
            refreshFloorSecs = try bridge.refreshFloorSecs()
        } catch {
            lastError = String(describing: error)
        }
    }

    /// Manual Refresh button — bypasses floor.
    public func refreshAll() {
        refreshAll(force: true)
    }

    public func refreshAll(force: Bool) {
        do {
            try bridge.refresh(surfaceId: nil, force: force)
            applySnapshots()
        } catch {
            lastError = String(describing: error)
            applySnapshots()
        }
    }

    public func refresh(surfaceId: String) {
        do {
            try bridge.refresh(surfaceId: surfaceId, force: true)
            applySnapshots()
        } catch {
            lastError = String(describing: error)
        }
    }

    private func pushFormatPrefs() {
        guard isOpen else { return }
        do {
            try bridge.setFormatPrefs(
                prefs: UsageFormatPrefsDto(
                    percentStyle: percentStyle,
                    resetStyle: resetStyle
                )
            )
        } catch {
            lastError = String(describing: error)
        }
    }

    private func startPolling() {
        pollTask?.cancel()
        pollTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: 5_000_000_000)
                self?.pollOnce()
            }
        }
    }

    private func pollOnce() {
        guard isOpen else { return }
        if hideWhileScreenSharing {
            screenShareActive = Self.isScreenCurrentlyShared()
        } else {
            screenShareActive = false
        }
        // Always-on: ask Rust to refresh when the floor allows (force: false).
        // Rust no-ops inside the floor so this is poll-safe every 5s.
        do {
            if try bridge.refreshDue() {
                try bridge.refresh(surfaceId: nil, force: false)
            }
        } catch {
            lastError = String(describing: error)
        }
        do {
            let batch = try bridge.nextEvents(cursor: eventCursor, max: 64)
            if batch.resyncRequired {
                // Event cursor behind retained log — reset and re-project snapshots.
                eventCursor = batch.nextCursor
                applySnapshots()
                return
            }
            eventCursor = batch.nextCursor
            if !batch.events.isEmpty {
                applySnapshots()
            } else {
                // Still refresh bar labels (relative "updated" text) cheaply.
                applySnapshots()
            }
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

    private func applySnapshots() {
        do {
            mergedBarLabel = try bridge.mergedStatusBarLabel()
            compactBarLabel = try bridge.compactStatusBarLabel()
            nextRefreshLabel = try bridge.nextRefreshLabel()
            let listed = try bridge.listSurfaces()
            var rows: [SurfaceRow] = []
            for surface in listed {
                guard surface.enabled else {
                    rows.append(
                        SurfaceRow(
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
                    )
                    continue
                }
                if let view = try? bridge.snapshot(surfaceId: surface.id) {
                    rows.append(
                        SurfaceRow(
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
                                    limitMoney: bucket.limitMoney
                                )
                            },
                            updatedLabel: view.updatedLabel,
                            lastError: view.lastError
                        )
                    )
                } else {
                    rows.append(
                        SurfaceRow(
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
                    )
                }
            }
            surfaces = rows
            overviewRows = try bridge.overviewRows().map { row in
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
            accounts = (try? bridge.listAccounts(surfaceId: nil))?.map { row in
                AccountRow(
                    surfaceId: row.surfaceId,
                    accountKey: row.accountKey,
                    accountLabel: row.accountLabel,
                    planLabel: row.planLabel,
                    selected: row.selected,
                    remainingPercent: row.remainingPercent,
                    statusWord: row.statusWord
                )
            } ?? []
            applyStatusItemText()
            lastError = nil
        } catch {
            lastError = String(describing: error)
        }
    }

    /// Open the Usage window on Overview or a specific surface.
    public func selectUsageSurface(_ surfaceId: String?) {
        usageSelection = surfaceId
    }

    private func applyStatusItemText() {
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
                statusItemText = try bridge.compactStatusBarLabel()
                statusItemChips = try chipsForProviderPreview(maxCount: 1, preferWorstFirst: true)
            case .pinned(let surfaceId):
                statusItemText = try bridge.compactStatusBarLabelFor(surfaceId: surfaceId) ?? ""
                statusItemChips = try chipsForPinned(surfaceId: surfaceId)
            case .strip(let max):
                // CodexBar-style: one chip per provider (catalog order).
                statusItemText = try bridge.compactStatusBarStrip(max: max)
                statusItemChips = try chipsForProviderPreview(
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

    private func chipsForPinned(surfaceId: String) throws -> [StatusItemChip] {
        guard let label = try bridge.compactStatusBarLabelFor(surfaceId: surfaceId),
              !label.isEmpty
        else {
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
    /// with numeric remaining / preview data, worst-first.
    private func chipsForProviderPreview(maxCount: Int, preferWorstFirst: Bool) throws
        -> [StatusItemChip]
    {
        let snaps = try surfaceSnapshotsForStatusItem()
        return buildStatusItemChips(
            surfaces: snaps,
            maxCount: maxCount,
            preferWorstFirst: preferWorstFirst,
            percentStyle: percentStyle,
            // Catalog strip: show every enabled provider icon; focus: data only.
            includeAllEnabled: !preferWorstFirst
        )
    }

    private func surfaceSnapshotsForStatusItem() throws -> [StatusItemSurfaceSnapshot] {
        var snaps: [StatusItemSurfaceSnapshot] = []
        for row in surfaces {
            let compact =
                (try? bridge.compactStatusBarLabelFor(surfaceId: row.id))
                ?? ""
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
