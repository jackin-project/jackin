// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation
import JackinUsageBindings

@MainActor
extension PresentationStore {
    func openSmoke(dataDir: String) {
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

    func open(
        dataDirOverride: String?,
        configRootOverride: String?,
        refreshFloorSecs: UInt64,
        enabled: [String],
        allowLiveProbes: Bool
    ) {
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
                await self.refreshAll(force: true)
                self.startPolling()
            } catch {
                self.report(error, userMessage: "Usage could not start. Try again.")
                self.isOpen = false
                self.isOpening = false
            }
        }
    }

    public func applyQIFixture(_ input: QIFixtureInput) {
        fixtureMode = true
        let projection = input.projection
        fixtureTerminalProjection = projection
        fixtureRefreshingProjection = input.transitions.refreshing
        fixtureAccountProjections = input.transitions.accountSelections
        applyFixtureProjection(projection)
        let providerIDs = Set(projection.providerGroups.map(\.surfaceId))
        knownOverviewProviderIDs = providerIDs
        overviewExpandedProviderIDs = providerIDs
        popoverSelection = input.selection.popover
        usageSelection = input.selection.usage
        self.usageAccountSelection =
            input.selection.account
            ?? projection.accounts.first(where: {
                $0.surfaceId == input.selection.usage && $0.selected
            })?.accountKey
        nextRefreshLabel = input.status.nextRefreshLabel
        refreshInProgress = input.status.isRefreshing
        self.isOpen = true
        isOpening = input.status.isLoading
        lastError = input.status.lastError
        reconcileSelections()
        refreshVisibleStatusRows()
    }
}
