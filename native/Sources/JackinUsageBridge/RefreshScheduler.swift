// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation
import JackinUsageBindings

/// Typed facade serializing every `UsageMenuBarBridge` access off the main actor.
///
/// The boltffi bridge is synchronous and shares one Rust runtime mutex, so a
/// Claude refresh that triggers a macOS Keychain consent sheet would block
/// whatever thread called into Rust. If that thread were the main actor — or if
/// any other bridge call (settings, account, poll, snapshot) ran on the main
/// actor while the sheet was up — the menu-bar UI would freeze. This scheduler
/// owns the bridge and runs *all* calls on one dedicated serial queue, so at
/// most one bridge operation is in flight and `@MainActor` callers only ever
/// `await` a result — they never touch the bridge or block on its mutex.
///
/// The pattern is deadlock-free by construction: a single serial queue, one
/// operation at a time, and no bridge operation re-enters the scheduler.
///
/// `UsageMenuBarBridge` may be named only inside this file. Callers use the
/// typed methods below; the generic `run` is private so no arbitrary closure
/// can capture the bridge handle.
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

    /// Open the Rust runtime and return the persisted refresh floor.
    public func openRuntime(config: OpenConfig) async throws -> UInt64 {
        try await run { handle in
            try handle.openRuntime(config: config)
            return try handle.refreshFloorSecs()
        }
    }

    public func setEnabled(surfaceId: String, enabled: Bool) async throws {
        try await run { try $0.setEnabled(surfaceId: surfaceId, enabled: enabled) }
    }

    /// Select multi-account identity for a surface (Rust-persisted).
    public func setSelectedAccount(surfaceId: String, accountKey: String) async throws {
        try await run { try $0.setSelectedAccount(surfaceId: surfaceId, accountKey: accountKey) }
    }

    /// Persist a new refresh floor and return the value Rust reports back.
    public func setRefreshFloorSecs(_ secs: UInt64) async throws -> UInt64 {
        try await run { handle in
            try handle.setRefreshFloorSecs(secs: secs)
            return try handle.refreshFloorSecs()
        }
    }

    /// Submit refresh intent once; Rust broker generations own coalescing.
    public func refresh(surfaceId: String?, force: Bool) async throws {
        try await run { try $0.refresh(surfaceId: surfaceId, force: force) }
    }

    public func setFormatPrefs(_ prefs: UsageFormatPrefsDto) async throws {
        try await run { try $0.setFormatPrefs(prefs: prefs) }
    }

    /// One serialized poll cycle: due-check plus floor-respecting refresh plus
    /// event drain, returning the advanced event cursor.
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

    /// Mark the scheduler invalid and shut the bridge down on the serial queue
    /// behind any in-flight operation.
    ///
    /// Never blocks the caller (no `@MainActor`
    /// wait on the Rust mutex during termination); later calls are rejected.
    public func invalidateAndShutdown() {
        stateLock.lock()
        invalidated = true
        stateLock.unlock()
        queue.async {
            try? self.bridge.shutdown()
        }
    }

    /// Run one bridge operation on the serial queue and await its result on the
    /// calling actor.
    ///
    /// Throws `SchedulerError.invalidated` once shut down.
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
