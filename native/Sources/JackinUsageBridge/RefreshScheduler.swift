// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation
import JackinUsageBindings

public final class RefreshScheduler: @unchecked Sendable {
    private let bridge: UsageMenuBarBridge
    private let queue = DispatchQueue(label: "com.jackin-project.desktop.bridge")
    private let stateLock = NSLock()
    private var invalidated = false

    public init(bridge: UsageMenuBarBridge) {
        self.bridge = bridge
    }

    public convenience init() {
        self.init(bridge: UsageMenuBarBridge())
    }

    public enum SchedulerError: Error {
        case invalidated
    }

    public func openRuntime(config: OpenConfig) async throws -> UInt64 {
        try await run { handle in
            try handle.openRuntime(config: config)
            return try handle.refreshFloorSecs()
        }
    }

    public func setEnabled(surfaceId: String, enabled: Bool) async throws {
        try await run { try $0.setEnabled(surfaceId: surfaceId, enabled: enabled) }
    }

    public func setSelectedAccount(surfaceId: String, accountKey: String) async throws {
        try await run { try $0.setSelectedAccount(surfaceId: surfaceId, accountKey: accountKey) }
    }

    public func setRefreshFloorSecs(_ secs: UInt64) async throws -> UInt64 {
        try await run { handle in
            try handle.setRefreshFloorSecs(secs: secs)
            return try handle.refreshFloorSecs()
        }
    }

    public func refresh(surfaceId: String?, force: Bool) async throws {
        try await run { try $0.refresh(surfaceId: surfaceId, force: force) }
    }

    public func setFormatPrefs(_ prefs: UsageFormatPrefsDto) async throws {
        try await run { try $0.setFormatPrefs(prefs: prefs) }
    }

    public func pollOnce(cursor: UInt64) async throws -> UInt64 {
        try await run { handle in
            if try handle.refreshDue() {
                try handle.refresh(surfaceId: nil, force: false)
            }
            return try handle.nextEvents(cursor: cursor, max: 64).nextCursor
        }
    }

    public func desktopProjection(statusBarMax: UInt32) async throws -> DesktopProjectionDto {
        try await run { try $0.desktopProjection(statusBarMax: statusBarMax) }
    }

    public func invalidateAndShutdown() {
        stateLock.lock()
        invalidated = true
        stateLock.unlock()
        queue.async {
            try? self.bridge.shutdown()
        }
    }

    private func run<T: Sendable>(
        _ operation: @escaping @Sendable (UsageMenuBarBridge) throws -> T
    ) async throws -> T {
        if isInvalidated() {
            throw SchedulerError.invalidated
        }
        return try await withCheckedThrowingContinuation {
            (continuation: CheckedContinuation<T, Error>) in
            queue.async {
                if self.isInvalidated() {
                    continuation.resume(throwing: SchedulerError.invalidated)
                    return
                }
                do {
                    continuation.resume(returning: try operation(self.bridge))
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    private func isInvalidated() -> Bool {
        stateLock.lock()
        defer { stateLock.unlock() }
        return invalidated
    }
}
