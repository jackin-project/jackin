// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation

@MainActor
extension PresentationStore {
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
            recordUsageAccountSelection(surfaceId: surfaceId, accountKey: accountKey)
            return
        }
        Task { [weak self] in
            guard let self else { return }
            do {
                try await self.scheduler.setSelectedAccount(
                    surfaceId: surfaceId, accountKey: accountKey)
                self.recordUsageAccountSelection(surfaceId: surfaceId, accountKey: accountKey)
                await self.applySnapshots()
            } catch {
                self.report(error, userMessage: "Account selection could not be saved.")
            }
        }
    }

    public func accountsForSurface(_ surfaceId: String) -> [AccountRow] {
        accounts.filter { $0.surfaceId == surfaceId }
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

    public func refreshAll() {
        if fixtureMode {
            runFixtureRefresh()
            return
        }
        Task { [weak self] in await self?.refreshAll(force: true) }
    }

    func refreshAll(force: Bool) async {
        await performRefresh(surfaceId: nil, force: force)
    }

    func performRefresh(surfaceId: String?, force: Bool) async {
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
}
