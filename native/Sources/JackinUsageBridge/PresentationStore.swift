// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation

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
        public var planLabel: String?
        public var buckets: [BucketRow]
        public var updatedLabel: String
        public var lastError: String?
    }

    public struct BucketRow: Identifiable, Sendable, Equatable {
        public var id: String { label }
        public let label: String
        public let usedLabel: String?
        public let remainingPercent: UInt8?
        public let resetLabel: String?
        public let severity: String
        public let status: String
    }

    @Published public private(set) var mergedBarLabel: String = "jackin❯ usage"
    /// Rust-owned short status-item label (e.g. `Cl 63%`); empty when no %.
    @Published public private(set) var compactBarLabel: String = ""
    @Published public private(set) var surfaces: [SurfaceRow] = []
    @Published public private(set) var lastError: String?
    @Published public private(set) var isOpen: Bool = false
    /// Refresh floor in seconds (owned by Rust; mirrored for Settings).
    @Published public private(set) var refreshFloorSecs: UInt64 = 300
    /// Display preference: show monospaced percent next to the template icon.
    @Published public var showPercentInMenuBar: Bool {
        didSet {
            UserDefaults.standard.set(showPercentInMenuBar, forKey: Self.showPercentKey)
        }
    }

    private static let showPercentKey = "jackin.desktop.showPercent"

    private let bridge = UsageMenuBarBridge.create()
    private var eventCursor: UInt64 = 0
    private var pollTask: Task<Void, Never>?

    public init() {
        // Default ON when key is absent.
        if UserDefaults.standard.object(forKey: Self.showPercentKey) == nil {
            self.showPercentInMenuBar = true
        } else {
            self.showPercentInMenuBar = UserDefaults.standard.bool(forKey: Self.showPercentKey)
        }
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

    private func applySnapshots() {
        do {
            mergedBarLabel = try bridge.mergedStatusBarLabel()
            compactBarLabel = try bridge.compactStatusBarLabel()
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
                            planLabel: nil,
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
                            planLabel: view.planLabel,
                            buckets: view.buckets.map { bucket in
                                BucketRow(
                                    label: bucket.label,
                                    usedLabel: bucket.usedLabel,
                                    remainingPercent: bucket.remainingPercent,
                                    resetLabel: bucket.resetLabel,
                                    severity: bucket.severity,
                                    status: bucket.status
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
                            planLabel: nil,
                            buckets: [],
                            updatedLabel: "",
                            lastError: nil
                        )
                    )
                }
            }
            surfaces = rows
            lastError = nil
        } catch {
            lastError = String(describing: error)
        }
    }
}
