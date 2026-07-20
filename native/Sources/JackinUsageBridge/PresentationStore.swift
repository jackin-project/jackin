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
    @Published public private(set) var surfaces: [SurfaceRow] = []
    @Published public private(set) var lastError: String?
    @Published public private(set) var isOpen: Bool = false

    private let bridge = UsageMenuBarBridge.create()
    private var eventCursor: UInt64 = 0
    private var pollTask: Task<Void, Never>?

    public init() {}

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
            refreshAll()
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
            refreshAll()
        } catch {
            lastError = String(describing: error)
        }
    }

    public func refreshAll() {
        do {
            try bridge.refresh(surfaceId: nil)
            applySnapshots()
        } catch {
            lastError = String(describing: error)
            // Still project cached/honest states.
            applySnapshots()
        }
    }

    public func refresh(surfaceId: String) {
        do {
            try bridge.refresh(surfaceId: surfaceId)
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
        do {
            let batch = try bridge.nextEvents(cursor: eventCursor, max: 64)
            if batch.resyncRequired {
                eventCursor = 0
            } else {
                eventCursor = batch.nextCursor
            }
            if !batch.events.isEmpty {
                applySnapshots()
            }
            // Periodic soft refresh of due targets is owned by Rust when refresh() is called.
        } catch {
            lastError = String(describing: error)
        }
    }

    private func applySnapshots() {
        do {
            mergedBarLabel = try bridge.mergedStatusBarLabel()
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
