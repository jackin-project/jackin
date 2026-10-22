// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import CoreGraphics
import Foundation
import JackinUsageBindings

@MainActor
extension PresentationStore {
    func applySnapshots() async {
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
        apply(projection, request: request)
    }

    private func apply(_ projection: DesktopProjectionDto, request: UInt64) {
        guard request >= lastAppliedRequest,
            projection.generation >= lastAppliedGeneration
        else { return }
        lastAppliedRequest = request
        lastAppliedGeneration = projection.generation
        nextRefreshLabel = projection.nextRefreshLabel
        refreshInProgress = projection.refreshInProgress
        let diagnosticBySurface = Dictionary(
            projection.diagnostics.compactMap { diagnostic in
                diagnostic.surfaceId.map { ($0, diagnostic.displayLabel) }
            },
            uniquingKeysWith: { first, _ in first }
        )
        discoveryDiagnostics = projection.diagnostics.map { diagnostic in
            DiscoveryDiagnostic(
                surfaceId: diagnostic.surfaceId,
                scopeLabel: diagnostic.scopeLabel,
                issue: diagnostic.issue,
                message: diagnostic.message,
                displayLabel: diagnostic.displayLabel
            )
        }
        surfaces = mapSurfaces(projection, diagnostics: diagnosticBySurface)
        providerGroups = projection.providers.map(Self.mapProviderGroup)
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

    private func mapSurfaces(
        _ projection: DesktopProjectionDto,
        diagnostics: [String: String]
    ) -> [SurfaceRow] {
        let providerBySurface = Dictionary(
            uniqueKeysWithValues: projection.providers.map { ($0.group.surfaceId, $0) }
        )
        return projection.surfaces.map { surface in
            guard let provider = providerBySurface[surface.id] else {
                return Self.emptySurface(surface, diagnostic: diagnostics[surface.id])
            }
            return Self.mapSurface(
                surface,
                view: provider.selectedUsage,
                diagnostic: diagnostics[surface.id]
            )
        }
    }

    private static func mapProviderGroup(
        _ provider: DesktopProviderProjectionDto
    ) -> ProviderGroupRow {
        ProviderGroupRow(
            surfaceId: provider.group.surfaceId,
            displayLabel: provider.group.displayLabel,
            iconKey: provider.group.iconKey,
            fallbackGlyph: provider.group.fallbackGlyph,
            usageURL: provider.group.usageUrl,
            accountColumnLabel: provider.group.accountColumnLabel,
            planOrStatusLabel: provider.group.planOrStatusLabel,
            remainingLabel: provider.group.remainingLabel,
            resetDisplayLabel: provider.group.resetDisplayLabel,
            accounts: provider.group.accounts.map(Self.mapAccountDto),
            accessibilityLabel: provider.group.accessibilityLabel,
            lastError: provider.group.emptyState?.lastError,
            selectedAccountRoute: SelectedAccountRoute(dto: provider.selectedAccountRoute)
        )
    }

    func retainLastGoodAfterProjectionFailure(_ error: Error, request: UInt64) {
        guard request >= lastAppliedRequest else { return }
        report(error, userMessage: "Usage could not be updated. Try again.")
    }

    func applyProjectionFailureForTesting(_ error: Error, request: UInt64 = .max) {
        retainLastGoodAfterProjectionFailure(error, request: request)
    }

    func fixtureKey(surfaceId: String, accountKey: String) -> String {
        "\(surfaceId)#\(accountKey)"
    }

    func applyFixtureProjection(_ projection: QIFixtureProjection) {
        providerGlanceRows = projection.glanceRows
        projectedStatusBarRows = projection.statusBarGlanceRows
        surfaces = projection.surfaces
        accounts = projection.accounts
        providerGroups = projection.providerGroups
        refreshVisibleStatusRows()
    }

    func runFixtureRefresh() {
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

    static func emptySurface(
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

    static func mapSurface(
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

    static func mapBucketDto(_ bucket: QuotaBucketDto) -> BucketRow {
        BucketRow(
            label: bucket.label,
            usedLabel: bucket.usedLabel,
            limitLabel: bucket.limitLabel,
            usedPercent: bucket.usedPercent,
            usedPercentOrigin: bucket.usedPercentOrigin,
            remainingPercent: bucket.remainingPercent,
            remainingPercentOrigin: bucket.remainingPercentOrigin,
            countQuota: bucket.countQuota.map(CountQuotaModel.init(dto:)),
            providerQuotaState: bucket.providerQuotaState,
            resetsAt: bucket.resetsAt,
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
            meterPercent: bucket.meterPercent,
            meterSide: bucket.meterSide
        )
    }

    static func mapAccountDto(_ row: AccountDescriptorDto) -> AccountRow {
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
            usedPercent: row.usedPercent,
            usedPercentOrigin: row.usedPercentOrigin,
            remainingPercent: row.remainingPercent,
            remainingPercentOrigin: row.remainingPercentOrigin,
            countQuota: row.countQuota.map(CountQuotaModel.init(dto:)),
            providerQuotaState: row.providerQuotaState,
            resetsAt: row.resetsAt,
            meterPercent: row.meterPercent,
            meterSide: row.meterSide,
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

    static func mapGlanceDto(_ row: ProviderGlanceRowDto) -> GlanceProviderRow {
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
            dimmed: row.dimmed,
            glanceUsedPercent: row.glanceUsedPercent,
            glanceUsedPercentOrigin: row.glanceUsedPercentOrigin,
            glanceRemainingPercentOrigin: row.glanceRemainingPercentOrigin,
            glanceCountQuota: row.glanceCountQuota.map(CountQuotaModel.init(dto:)),
            glanceProviderQuotaState: row.glanceProviderQuotaState,
            glanceResetsAt: row.glanceResetsAt,
            glanceMeterPercent: row.glanceMeterPercent,
            glanceMeterSide: row.glanceMeterSide
        )
    }

    func reconcileSelections() {
        if let usageSelection, !isNavigableSurface(usageSelection) {
            self.usageSelection = nil
            usageAccountSelection = nil
        } else if let usageSelection,
            let usageAccountSelection,
            !accounts.contains(where: {
                $0.surfaceId == usageSelection && $0.accountKey == usageAccountSelection
            })
        {
            let route =
                providerGroups.first(where: { $0.surfaceId == usageSelection })?
                .selectedAccountRoute
            if let route, case .unavailable = route {
                self.usageAccountSelection = nil
            } else {
                self.usageSelection = nil
                self.usageAccountSelection = nil
            }
        }
        if let popoverSelection,
            !providerGlanceRows.contains(where: { $0.surfaceId == popoverSelection })
        {
            self.popoverSelection = providerGlanceRows.first?.surfaceId
        }
    }

    func isNavigableSurface(_ surfaceId: String) -> Bool {
        providerGlanceRows.contains(where: { $0.surfaceId == surfaceId })
            && surfaces.contains(where: { $0.id == surfaceId && $0.enabled })
    }

    func refreshVisibleStatusRows() {
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

    func report(_ error: Error, userMessage: String) {
        NSLog("jackin desktop bridge error: %@", String(describing: error))
        lastError = userMessage
    }
}
