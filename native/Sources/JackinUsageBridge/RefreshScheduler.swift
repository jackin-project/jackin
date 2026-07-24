// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation

/// Serializes every `UsageMenuBarBridge` access off the main actor.
///
/// The UniFFI bridge is synchronous and shares one Rust runtime mutex, so a
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
public final class RefreshScheduler: @unchecked Sendable {
    private let bridge: UsageMenuBarBridge
    private let queue = DispatchQueue(label: "com.jackin-project.desktop.bridge")
    private let stateLock = NSLock()
    private var invalidated = false

    public init(bridge: UsageMenuBarBridge) {
        self.bridge = bridge
    }

    public convenience init() {
        self.init(bridge: UsageMenuBarBridge.create())
    }

    public enum SchedulerError: Error {
        case invalidated
    }

    /// Run one bridge operation on the serial queue and await its result on the
    /// calling actor. Throws `SchedulerError.invalidated` once shut down.
    public func run<T: Sendable>(
        _ operation: @escaping @Sendable (UsageMenuBarBridge) throws -> T
    ) async throws -> T {
        if isInvalidated() {
            throw SchedulerError.invalidated
        }
        return try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<T, Error>) in
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

    /// Mark the scheduler invalid and shut the bridge down on the serial queue
    /// behind any in-flight operation. Never blocks the caller (no `@MainActor`
    /// wait on the Rust mutex during termination); later `run` calls are rejected.
    public func invalidateAndShutdown() {
        stateLock.lock()
        invalidated = true
        stateLock.unlock()
        queue.async {
            try? self.bridge.shutdown()
        }
    }

    private func isInvalidated() -> Bool {
        stateLock.lock()
        defer { stateLock.unlock() }
        return invalidated
    }
}
