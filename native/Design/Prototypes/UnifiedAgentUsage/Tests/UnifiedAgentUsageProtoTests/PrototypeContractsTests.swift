import XCTest

@testable import UnifiedAgentUsageProto

final class PrototypeContractsTests: XCTestCase {
    func testWindowContractAcceptsMinimumAndRejectsInvalidInputs() throws {
        let minimum = try ProtoConfig.validated([
            "prototype", "--tr-window", "800x520", "--tr-appearance", "dark",
        ])
        XCTAssertEqual(minimum.window, CGSize(width: 800, height: 520))
        XCTAssertThrowsError(try ProtoConfig.validated(["prototype", "--tr-window", "800"]))
        XCTAssertThrowsError(try ProtoConfig.validated(["prototype", "--tr-window", "799x520"]))
        XCTAssertThrowsError(
            try ProtoConfig.validated(["prototype", "--tr-appearance", "light"]))
    }

    func testReductionFlagsParseTogether() throws {
        let config = try ProtoConfig.validated([
            "prototype", "--tr-reduce", "transparency,motion",
        ])
        XCTAssertTrue(config.reduceTransparency)
        XCTAssertTrue(config.reduceMotion)
        XCTAssertThrowsError(
            try ProtoConfig.validated(["prototype", "--tr-reduce", "unknown"]))
    }

    func testDefaultAndF02ResolveToSameFixture() throws {
        let defaultFixture = try XCTUnwrap(ProtoFixtures.projection(named: "default"))
        let f02 = try XCTUnwrap(ProtoFixtures.projection(named: "F02"))
        XCTAssertEqual(defaultFixture.providers.map(\.key), f02.providers.map(\.key))
        XCTAssertEqual(
            defaultFixture.providers.flatMap(\.accounts).flatMap(\.windows).map(\.stableID),
            f02.providers.flatMap(\.accounts).flatMap(\.windows).map(\.stableID))
        XCTAssertNil(ProtoFixtures.projection(named: "unknown"))
    }

    @MainActor
    func testMultiAccountHeadersAreNotDestinationsAndSingleProvidersAre() throws {
        let store = ProtoStore(projection: try XCTUnwrap(ProtoFixtures.projection(named: "F25")))
        let multi = try XCTUnwrap(store.projection.providers.first { $0.accounts.count > 1 })
        XCTAssertFalse(store.sidebarDestinations.contains(.provider(multi.key)))
        for account in multi.accounts {
            XCTAssertTrue(
                store.sidebarDestinations.contains(
                    .account(provider: multi.key, account: account.key)))
        }
        let catalog = ProtoStore(
            projection: try XCTUnwrap(ProtoFixtures.projection(named: "F02")))
        let single = try XCTUnwrap(catalog.projection.providers.first { $0.accounts.count == 1 })
        XCTAssertTrue(catalog.sidebarDestinations.contains(.provider(single.key)))
    }

    @MainActor
    func testProviderDestinationNormalizesToAccountAndKeyboardOrderIsStable() throws {
        let store = ProtoStore(projection: try XCTUnwrap(ProtoFixtures.projection(named: "F25")))
        let multi = try XCTUnwrap(store.projection.providers.first { $0.accounts.count > 1 })
        store.navigate(to: .provider(multi.key))
        guard case .account(let provider, let account) = store.resolvedSidebar else {
            return XCTFail("multi-account provider did not normalize to an account")
        }
        XCTAssertEqual(provider, multi.key)
        XCTAssertEqual(account, multi.accounts[0].key)
        XCTAssertEqual(store.sidebarDestinations.first, .overview)
        XCTAssertEqual(
            Array(store.sidebarDestinations.dropFirst().prefix(multi.accounts.count)),
            multi.accounts.map { .account(provider: multi.key, account: $0.key) })
    }

    @MainActor
    func testValidAccountSelectionSurvivesFixtureReload() throws {
        let store = ProtoStore(projection: try XCTUnwrap(ProtoFixtures.projection(named: "F25")))
        let multi = try XCTUnwrap(store.projection.providers.first { $0.accounts.count > 1 })
        let selected = try XCTUnwrap(multi.accounts.dropFirst().first)
        store.navigate(to: .account(provider: multi.key, account: selected.key))
        store.loadScenario("F25")
        XCTAssertEqual(store.resolvedSidebar, .account(provider: multi.key, account: selected.key))
    }

    func testUnavailableAndStaleSummaryTruth() throws {
        let unavailable = try XCTUnwrap(
            ProtoFixtures.projection(named: "F09")?.providers.first)
        XCTAssertFalse(unavailable.state.exposesQuotaSummary)
        XCTAssertEqual(unavailable.activityLabel, "Unavailable")

        let stale = try XCTUnwrap(ProtoFixtures.projection(named: "F06")?.providers.first)
        XCTAssertTrue(stale.state.exposesQuotaSummary)
        XCTAssertTrue(stale.activityLabel.contains("Stale"))
        XCTAssertNotNil(stale.summaryPercent)
    }

    func testSemanticQuotaOrderDoesNotDependOnLabels() {
        func window(_ id: String, _ category: ProtoQuotaCategory) -> ProtoQuotaWindow {
            ProtoQuotaWindow(
                stableID: id, label: "identical", category: category, display: id,
                primaryValue: id, meter: 50, state: .current)
        }
        let source = [
            window("session-a", .session), window("model-a", .model),
            window("long-a", .longRange), window("model-b", .model),
            window("general-a", .general), window("other-a", .other),
        ]
        XCTAssertEqual(
            ProtoQuotaOrdering.ordered(source).map(\.stableID),
            ["long-a", "model-a", "model-b", "general-a", "session-a", "other-a"])
    }
}
