// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::{Names, source::Inventory};
use anyhow::{Result, ensure};

// This independently specified contract survives deleted/renamed source tests.
// Additive source tests are allowed and must also execute. Removing a required
// identity requires an explicit change to this gate contract.
pub(super) const XCTEST: &[&str] = &[
    "ArchitectureTests.testApplicationDelegateOpensRuntimeOnLaunch",
    "ArchitectureTests.testDesktopSourcesContainNoForbiddenUsagePresentation",
    "ArchitectureTests.testDesktopSourcesDoNotComposePercentOrResetLiterals",
    "ArchitectureTests.testDesktopSourcesHaveNoHardcodedProviderDisplayNames",
    "ArchitectureTests.testFinalCaptureMatrixBuildsCleanBranchHeadApp",
    "ArchitectureTests.testFixtureUsageWindowSurvivesForeignFullScreenHosts",
    "ArchitectureTests.testFixtureWindowVisibilityLeaseEndsOnlyWithWindowLifecycle",
    "ArchitectureTests.testJackinPhosphorTokensMatchBrandGuide",
    "ArchitectureTests.testLatestOnlySourcesHaveNoMacOSCompatibilityBranches",
    "ArchitectureTests.testNoSwiftPercentArithmeticOnDisplayStrings",
    "ArchitectureTests.testOverviewHasNoOrphanOverviewLevelProgress",
    "ArchitectureTests.testOverviewPrimaryValuesUseSystemForeground",
    "ArchitectureTests.testPackageSwiftUsesBinaryTargetNotHostDylib",
    "ArchitectureTests.testPopoverHasNoGaugeAndSurfaceCardGone",
    "ArchitectureTests.testProductIdentityUsesNativeNoninteractivePlacements",
    "ArchitectureTests.testProductionHasNoCustomGlassEffects",
    "ArchitectureTests.testProductionHasNoHandPaintedSystemMaterial",
    "ArchitectureTests.testScreenShareProbeLivesOnlyInPresentationStore",
    "ArchitectureTests.testSelectStatusBarGlanceRowsHidesZeroAndCapsThree",
    "ArchitectureTests.testSettingsHydrationDoesNotPersistClampedFloor",
    "ArchitectureTests.testSeverityTintUsesBrandAndSystemWarningColors",
    "ArchitectureTests.testStatusBarIsTemplateMonoWithoutSeverityTint",
    "ArchitectureTests.testStatusBarOrderRequiresRebuildOnRankChange",
    "ArchitectureTests.testSwiftSourcesHaveNoProviderProbeImports",
    "ArchitectureTests.testUsageWindowRendersSharedDetailModel",
    "ArchitectureTests.testVisualQAStateRestoresLiquidGlassAfterDarkMode",
    "BridgeBoundaryTests.testBindingsTargetContainsOnlyGeneratedSwift",
    "BridgeBoundaryTests.testBridgeHandleNamedOnlyByFacade",
    "BridgeBoundaryTests.testGeneratedCModuleImportedOnlyByGeneratedSwift",
    "CountQuotaPresentationTests.testGeneratedCountCodecPreservesIndependentFieldPresenceAndOrder",
    "CountQuotaPresentationTests.testGeneratedCountCodecPreservesUnsignedBoundaryAndUnknownValues",
    "CountQuotaPresentationTests.testNativeAccountSummaryPreservesCountsAndIndependentResetEpoch",
    "CountQuotaPresentationTests.testNativeBucketCopiesTypedCountWithoutParsingDisplayLabels",
    "CountQuotaPresentationTests.testUnknownZeroAndAbsentQuotaRemainDistinct",
    "LintPolicyTests.testApplicationSourcesNeverSeeForceOperationRelief",
    "LintPolicyTests.testNestedSizeDebtConfigsCarryOwnerAndDeletionCondition",
    "LintPolicyTests.testRootConfigKeepsForceOperationsAtError",
    "LintPolicyTests.testRootDisablesOnlyFormatterConflicts",
    "LintPolicyTests.testTestTreesRelaxOnlyForceOperationsAndNamedSizeDebt",
    "OverviewInventoryTests.testProviderWithoutAccountsIsLeaf",
    "OverviewInventoryTests.testTreeCopiesFinishedDisplayStringsVerbatim",
    "OverviewInventoryTests.testTreePreservesRustGroupAndAccountOrder",
    "PlatformLaneTests.testComponentMapPost26SymbolsAreGuarded",
    "PlatformLaneTests.testManifestAndReadmeRecordBothLanes",
    "PlatformLaneTests.testNoUIDesignRequiresCompatibilityAnywhere",
    "PopoverPresentationTests.testExactAccountSelectionUpdatesIdentityAndUsageHandoff",
    "PopoverPresentationTests.testPopoverActivatesBeforePresentingFromSecondaryDisplayStatusItem",
    "PopoverPresentationTests.testPopoverContentOrderAndFooterPlacement",
    "PopoverPresentationTests.testPopoverResetsScrollOnlyAfterExplicitNativePresentation",
    "PopoverPresentationTests.testScrollResetClaimSurvivesViewRemounts",
    "PopoverPresentationTests.testSingleAccountIdentityNeedsNoPicker",
    "PresentationSelectionTests.testCountOverviewRendersLiteralUnsignedRequestsAndUnknownReset",
    "PresentationSelectionTests.testExplicitOverviewAccountChoiceExplainsReplacementDisappearingWithSibling",
    "PresentationSelectionTests.testLastAccountRemovalRendersNoticeAndRetryInNativeWindow",
    "PresentationSelectionTests.testLastRemovedAccountLeavesActionableProviderRowAndNotice",
    "PresentationSelectionTests.testLatestAccountIntentWinsWhenEarlierProjectionCompletesFirst",
    "PresentationSelectionTests.testMatchingPopoverAccountChoiceClearsRemovedDestinationNoticeWithoutNavigation",
    "PresentationSelectionTests.testMissingPopoverSelectionRendersUnavailableInsteadOfFirstSibling",
    "PresentationSelectionTests.testNavigationFencesPendingSetterAndRejectsMismatchedPublishedIdentity",
    "PresentationSelectionTests.testPopoverRecoveryDoesNotClearNoticeWhenRequestedAccountDisappears",
    "PresentationSelectionTests.testProviderOnlyDestinationUsesExactRustSelectionAfterReordering",
    "PresentationSelectionTests.testReappearingPersistedAccountDoesNotNavigateWithoutUserAction",
    "PresentationSelectionTests.testRemovedAccountKeepsOverviewNoticeAndRequiresExplicitSiblingSelection",
    "PresentationSelectionTests.testUnrelatedPopoverAccountChangeDoesNotCancelPendingUsageAccountIntent",
    "PresentationSelectionTests.testUnrelatedPopoverAccountChangeKeepsRemovedDestinationNotice",
    "PresentationSelectionTests.testUnrelatedUnavailableSelectionPreservesActiveExplicitAccount",
    "PresentationStoreTests.testDiscoveryDiagnosticKeepsRustOwnedSanitizedCopy",
    "PresentationStoreTests.testProductionLaunchDoesNotRequireSwiftOwnedHostPaths",
    "PresentationStoreTests.testProjectionFailureRetainsExactLastGoodStateAndSelection",
    "StatusPopoverFocusTests.testEmptySurfaceIdOpensOverview",
    "StatusPopoverFocusTests.testFallbackItemOpensOverview",
    "StatusPopoverFocusTests.testProviderClickSelectsSurface",
    "StatusPopoverFocusTests.testSurfaceIdMapLookup",
    "UsageSidebarToggleAuthorityTests.testStandardMenuKeyEquivalents",
    "UsageSidebarToggleAuthorityTests.testViewMenuDispatchesOnlyThroughNativeSplitViewResponder",
    "UsageSidebarToggleAuthorityTests.testWindowToolbarUsesOnlyStandardSplitViewItems",
    "UsageWindowModelTests.testDetailRowAndLineOrderFlattenedExactlyOnce",
    "UsageWindowModelTests.testDisabledIncomingSelectionFallsBackToOverview",
    "UsageWindowModelTests.testDuplicateBucketLabelsKeepDistinctIds",
    "UsageWindowModelTests.testEmptyEnabledSet",
    "UsageWindowModelTests.testIncomingProviderSelectionResolvesToDetail",
    "UsageWindowModelTests.testMultiAccountActionAndSelectedStyling",
    "UsageWindowModelTests.testRemovedAccountSelectionReturnsToOverviewWithoutSiblingFallback",
    "UsageWindowModelTests.testSentinelRowsTransmittedUnchanged",
    "UsageWindowModelTests.testSidebarOrderAndOverviewSelection",
    "UsageWindowModelTests.testStaleDetailAndLastGoodBucketCoexist",
    "VendorProvenanceTests.testVendorTreeIsAbsentOrFullyProvenanced",
    "VisualQAFixturesTests.testCatalogContainsEveryStableFixtureExactlyOnce",
    "VisualQAFixturesTests.testCatalogProviderOrderAndLayoutEnvelope",
    "VisualQAFixturesTests.testFixtureAccountSelectionNeverCallsBridge",
    "VisualQAFixturesTests.testFixtureIdentitiesAreSynthetic",
    "VisualQAFixturesTests.testFixtureModeRequiresExplicitSelector",
    "VisualQAFixturesTests.testFixturePreferenceChangesNeverCallBridge",
    "VisualQAFixturesTests.testFixtureRefreshTransitionsToUpdatingAndBack",
    "VisualQAFixturesTests.testFixtureSnapshotNormalizesRemovedProviderAtStateOwner",
    "VisualQAFixturesTests.testProductionSourcesExposeNoDestructiveAction",
    "VisualQAFixturesTests.testProductionSourcesExposeNoInvisibleShortcutControl",
    "VisualQAFixturesTests.testRefreshingStatusUsesAccessibleNonfocusedProgress",
    "VisualQAFixturesTests.testRetainedUsageWindowPreservesValidDestinationUntilExplicitlyChanged",
];

pub(super) const SWIFT_TESTING: &[&str] = &[
    "ProjectBaselineTests.providerStatusFocus",
    "ProjectBaselineTests.statusItemContextMenuOrder",
];

pub(super) const UI: &[&str] = &[
    "JackinDesktopUITests.testEmptyUsageStateIsDistinct",
    "JackinDesktopUITests.testFocusedPopoverPassesAccessibilityAudit",
    "JackinDesktopUITests.testFocusedPopoverUsesRealHost",
    "JackinDesktopUITests.testGlobalErrorUsageStateIsDistinct",
    "JackinDesktopUITests.testLoadingUsageStateIsDistinct",
    "JackinDesktopUITests.testMaximumContentRemainsScrollableAtMinimumSize",
    "JackinDesktopUITests.testMaximumPopoverContentRemainsScrollable",
    "JackinDesktopUITests.testMultiAccountProviderUsesNativePicker",
    "JackinDesktopUITests.testNativeSidebarOwnsLeadingRegionAndToggleKeepsItsCoordinate",
    "JackinDesktopUITests.testOverviewAndProviderNavigationAtMinimumSize",
    "JackinDesktopUITests.testOverviewPassesAccessibilityAudit",
    "JackinDesktopUITests.testPartialFailureOverviewRemainsCoherentWhenRepresented",
    "JackinDesktopUITests.testPopoverRoutesProviderContextIntoUsage",
    "JackinDesktopUITests.testProviderDetailPassesAccessibilityAudit",
    "JackinDesktopUITests.testRefreshActivityTransitionReachesNativeChrome",
    "JackinDesktopUITests.testRefreshingUsageExposesNativeBusyState",
    "JackinDesktopUITests.testRetainedUsageWindowPreservesContextAcrossCloseAndReopen",
    "JackinDesktopUITests.testSidebarShortcutPreservesDetailKeyboardFocus",
    "JackinDesktopUITests.testStandardCommandsAndMenusShareNativeState",
];

pub(super) fn verify_source(unit: &Inventory, ui: &Inventory) -> Result<()> {
    require_baseline(&unit.xctest, XCTEST, "XCTest")?;
    require_baseline(&unit.testing, SWIFT_TESTING, "Swift Testing")?;
    require_baseline(&ui.xctest, UI, "UI XCTest")
}

pub(super) fn require_baseline(current: &Names, mandatory: &[&str], label: &str) -> Result<()> {
    let expected: Names = mandatory.iter().map(|name| (*name).to_owned()).collect();
    ensure!(
        expected.len() == mandatory.len() && !expected.is_empty(),
        "{label} mandatory gate contract is empty or duplicated"
    );
    let missing: Vec<_> = expected.difference(current).collect();
    ensure!(
        missing.is_empty(),
        "{label} source inventory removed mandatory gate tests: {missing:?}"
    );
    Ok(())
}
