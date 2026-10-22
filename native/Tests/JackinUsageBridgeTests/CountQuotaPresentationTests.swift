// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import XCTest

@testable import JackinUsageBindings
@testable import JackinUsageBridge

final class CountQuotaPresentationTests: XCTestCase {
    func testGeneratedCountCodecPreservesIndependentFieldPresenceAndOrder() {
        let count = CountQuotaDto(
            used: 0,
            limit: nil,
            remaining: 12,
            unit: "requests",
            period: "utc_daily",
            provenance: "provider_reported"
        )

        var expected = WireWriter()
        expected.writeOptional(UInt64(0)) { writer, value in writer.writeU64(value) }
        expected.writeOptional(nil as UInt64?) { writer, value in writer.writeU64(value) }
        expected.writeOptional(UInt64(12)) { writer, value in writer.writeU64(value) }
        expected.writeString("requests")
        expected.writeString("utc_daily")
        expected.writeString("provider_reported")

        var encoded = WireWriter()
        count.encode(to: &encoded)
        XCTAssertEqual(encoded.data, expected.data)

        var reader = WireReader(data: encoded.data)
        let decoded = CountQuotaDto.decode(from: &reader)
        XCTAssertEqual(decoded, count)
        XCTAssertEqual(reader.position, encoded.data.count)
    }

    func testGeneratedCountCodecPreservesUnsignedBoundaryAndUnknownValues() {
        let aboveDoubleIntegerPrecision = UInt64(9_007_199_254_740_993)
        let count = CountQuotaDto(
            used: aboveDoubleIntegerPrecision,
            limit: UInt64.max,
            remaining: nil,
            unit: "unknown",
            period: "unknown",
            provenance: "provider_reported"
        )
        var encoded = WireWriter()
        count.encode(to: &encoded)
        var reader = WireReader(data: encoded.data)
        let decoded = CountQuotaDto.decode(from: &reader)

        XCTAssertEqual(decoded.used, aboveDoubleIntegerPrecision)
        XCTAssertEqual(decoded.limit, UInt64.max)
        XCTAssertNil(decoded.remaining)
        XCTAssertEqual(decoded.unit, "unknown")
        XCTAssertEqual(decoded.period, "unknown")
        XCTAssertEqual(reader.position, encoded.data.count)
    }

    @MainActor
    func testGeneratedQuotaBucketCodecPreservesRawRemainingWithCountGeometryOrNone() {
        let complete = bucketDto(
            count: countDto(used: 9, limit: 10, remaining: 1),
            displayLabel: "1 requests left",
            remainingLabel: "1 requests left",
            usedPercent: nil,
            remainingPercent: 57.125,
            meterPercent: 10,
            meterSide: "remaining"
        )
        var completeWriter = WireWriter()
        complete.encode(to: &completeWriter)
        var completeReader = WireReader(data: completeWriter.data)
        let decodedComplete = QuotaBucketDto.decode(from: &completeReader)
        XCTAssertEqual(completeReader.position, completeWriter.data.count)
        let completeRow = PresentationStore.mapBucketDto(decodedComplete)
        XCTAssertEqual(completeRow.remainingPercent, 57.125)
        XCTAssertEqual(completeRow.remainingPercentOrigin, "provider_reported")
        XCTAssertNil(completeRow.usedPercent)
        XCTAssertEqual(completeRow.countQuota?.used, 9)
        XCTAssertEqual(completeRow.countQuota?.limit, 10)
        XCTAssertEqual(completeRow.countQuota?.remaining, 1)
        XCTAssertEqual(completeRow.meterPercent, 10)
        XCTAssertEqual(completeRow.meterSide, "remaining")

        let incomplete = bucketDto(
            count: countDto(used: 9, limit: nil, remaining: 1),
            displayLabel: "1 requests left",
            remainingLabel: "1 requests left",
            usedPercent: nil,
            remainingPercent: 57.125,
            meterPercent: nil,
            meterSide: nil
        )
        var incompleteWriter = WireWriter()
        incomplete.encode(to: &incompleteWriter)
        var incompleteReader = WireReader(data: incompleteWriter.data)
        let decodedIncomplete = QuotaBucketDto.decode(from: &incompleteReader)
        XCTAssertEqual(incompleteReader.position, incompleteWriter.data.count)
        let incompleteRow = PresentationStore.mapBucketDto(decodedIncomplete)
        XCTAssertEqual(incompleteRow.remainingPercent, 57.125)
        XCTAssertEqual(incompleteRow.remainingPercentOrigin, "provider_reported")
        XCTAssertEqual(incompleteRow.countQuota?.used, 9)
        XCTAssertNil(incompleteRow.countQuota?.limit)
        XCTAssertEqual(incompleteRow.countQuota?.remaining, 1)
        XCTAssertNil(incompleteRow.meterPercent)
        XCTAssertNil(incompleteRow.meterSide)
    }

    @MainActor
    // This fixed acceptance ID wraps under the configured Swift formatter.
    // swiftlint:disable:next function_name_whitespace
    func
        testGeneratedProviderQuotaStateCodecAndBucketMappingPreserveUnlimitedRawPercentWithoutMeter()
    {
        let dto = bucketDto(
            count: countDto(used: 0, limit: 10, remaining: 0),
            displayLabel: "Unlimited · Reported remaining: 99%",
            remainingLabel: "Unlimited · Reported remaining: 99%",
            remainingPercent: 99,
            meterPercent: nil,
            meterSide: nil,
            providerQuotaState: "unlimited"
        )
        var writer = WireWriter()
        dto.encode(to: &writer)
        var reader = WireReader(data: writer.data)
        let decoded = QuotaBucketDto.decode(from: &reader)
        XCTAssertEqual(reader.position, writer.data.count)

        let row = PresentationStore.mapBucketDto(decoded)
        XCTAssertEqual(row.providerQuotaState, "unlimited")
        XCTAssertEqual(row.remainingPercent, 99)
        XCTAssertEqual(row.remainingPercentOrigin, "provider_reported")
        XCTAssertEqual(row.countQuota?.remaining, 0)
        XCTAssertNil(row.meterPercent)
        XCTAssertNil(row.meterSide)
    }

    @MainActor
    func testGeneratedProviderQuotaStateCodecAndAccountMappingPreserveExhaustedCountAndRawPercent()
    {
        let dto = accountDto(
            count: countDto(used: 9, limit: 10, remaining: 1),
            resetsAt: nil,
            meterPercent: 0,
            meterSide: "remaining",
            remainingPercent: 57.125,
            providerQuotaState: "exhausted"
        )
        var writer = WireWriter()
        dto.encode(to: &writer)
        var reader = WireReader(data: writer.data)
        let decoded = AccountDescriptorDto.decode(from: &reader)
        XCTAssertEqual(reader.position, writer.data.count)

        let row = PresentationStore.mapAccountDto(decoded)
        XCTAssertEqual(row.providerQuotaState, "exhausted")
        XCTAssertEqual(row.countQuota?.used, 9)
        XCTAssertEqual(row.countQuota?.limit, 10)
        XCTAssertEqual(row.countQuota?.remaining, 1)
        XCTAssertEqual(row.remainingPercent, 57.125)
        XCTAssertEqual(row.remainingPercentOrigin, "provider_reported")
        XCTAssertEqual(row.meterPercent, 0)
        XCTAssertEqual(row.meterSide, "remaining")
    }

    @MainActor
    func testAbsentProviderQuotaStateStaysNilForBucketAndAccount() {
        let bucket = bucketDto(count: nil, displayLabel: "Quota unavailable")
        var bucketWriter = WireWriter()
        bucket.encode(to: &bucketWriter)
        var bucketReader = WireReader(data: bucketWriter.data)
        let decodedBucket = QuotaBucketDto.decode(from: &bucketReader)
        XCTAssertNil(PresentationStore.mapBucketDto(decodedBucket).providerQuotaState)

        let account = accountDto(count: nil, resetsAt: nil, providerQuotaState: nil)
        var accountWriter = WireWriter()
        account.encode(to: &accountWriter)
        var accountReader = WireReader(data: accountWriter.data)
        let decodedAccount = AccountDescriptorDto.decode(from: &accountReader)
        XCTAssertNil(PresentationStore.mapAccountDto(decodedAccount).providerQuotaState)
    }

    @MainActor
    func testGeneratedGlanceProviderQuotaStateCodecAndMappingCopySelectedStateOrAbsence() {
        let unlimited = glanceDto(
            .init(
                providerQuotaState: "unlimited",
                remainingPercent: 99,
                count: countDto(used: 0, limit: 10, remaining: 0),
                meterPercent: nil,
                meterSide: nil,
                headline: "Unlimited · Reported remaining: 99%"
            )
        )
        var unlimitedWriter = WireWriter()
        unlimited.encode(to: &unlimitedWriter)
        var unlimitedReader = WireReader(data: unlimitedWriter.data)
        let decodedUnlimited = ProviderGlanceRowDto.decode(from: &unlimitedReader)
        XCTAssertEqual(unlimitedReader.position, unlimitedWriter.data.count)

        let unlimitedRow = PresentationStore.mapGlanceDto(decodedUnlimited)
        XCTAssertEqual(unlimitedRow.glanceProviderQuotaState, "unlimited")
        XCTAssertEqual(unlimitedRow.glanceRemainingPercent, 99)
        XCTAssertEqual(unlimitedRow.glanceRemainingPercentOrigin, "provider_reported")
        XCTAssertEqual(unlimitedRow.glanceCountQuota?.remaining, 0)
        XCTAssertNil(unlimitedRow.glanceMeterPercent)
        XCTAssertNil(unlimitedRow.glanceMeterSide)

        let absent = glanceDto(
            .init(
                providerQuotaState: nil,
                remainingPercent: nil,
                count: nil,
                meterPercent: nil,
                meterSide: nil,
                headline: "Unlimited"
            )
        )
        var absentWriter = WireWriter()
        absent.encode(to: &absentWriter)
        var absentReader = WireReader(data: absentWriter.data)
        let decodedAbsent = ProviderGlanceRowDto.decode(from: &absentReader)
        XCTAssertEqual(absentReader.position, absentWriter.data.count)

        let absentRow = PresentationStore.mapGlanceDto(decodedAbsent)
        XCTAssertNil(absentRow.glanceProviderQuotaState)
        XCTAssertEqual(absentRow.headline, "Unlimited")
    }

    @MainActor
    func testNativeAccountSummaryPreservesCountsAndIndependentResetEpoch() {
        let count = countDto(used: UInt64.max, limit: 12, remaining: 0)
        let dto = accountDto(
            count: count,
            resetsAt: 1_800_000_123,
            resetLabel: nil,
            resetDisplayLabel: "Reset unavailable"
        )

        let row = PresentationStore.mapAccountDto(dto)

        XCTAssertEqual(row.countQuota?.used, UInt64.max)
        XCTAssertEqual(row.countQuota?.limit, 12)
        XCTAssertEqual(row.countQuota?.remaining, 0)
        XCTAssertEqual(row.countQuota?.unit, "requests")
        XCTAssertEqual(row.usedPercent, 43.375)
        XCTAssertEqual(row.usedPercentOrigin, "derived")
        XCTAssertEqual(row.remainingPercent, 57.125)
        XCTAssertEqual(row.remainingPercentOrigin, "provider_reported")
        XCTAssertEqual(row.meterPercent, 0)
        XCTAssertEqual(row.meterSide, "remaining")
        XCTAssertEqual(row.resetsAt, 1_800_000_123)
        XCTAssertEqual(row.resetDisplayLabel, "Reset unavailable")
    }

    @MainActor
    func testNativeBucketCopiesTypedCountWithoutParsingDisplayLabels() {
        let dto = bucketDto(
            count: countDto(used: 5, limit: 80, remaining: 73),
            displayLabel: "9007199254740993 requests left",
            remainingLabel: "73 requests left",
            meterPercent: 91
        )

        let row = PresentationStore.mapBucketDto(dto)

        XCTAssertEqual(row.countQuota?.used, 5)
        XCTAssertEqual(row.countQuota?.limit, 80)
        XCTAssertEqual(row.countQuota?.remaining, 73)
        XCTAssertEqual(row.usedPercent, 43.375)
        XCTAssertEqual(row.usedPercentOrigin, "derived")
        XCTAssertEqual(row.remainingPercent, 57.125)
        XCTAssertEqual(row.remainingPercentOrigin, "provider_reported")
        XCTAssertEqual(row.meterPercent, 91)
        XCTAssertEqual(row.meterSide, "remaining")
        XCTAssertEqual(row.displayLabel, "9007199254740993 requests left")
        XCTAssertEqual(row.remainingLabel, "73 requests left")
    }

    @MainActor
    func testUnknownZeroAndAbsentQuotaRemainDistinct() {
        let zero = PresentationStore.mapBucketDto(
            bucketDto(
                count: countDto(used: nil, limit: nil, remaining: 0, unit: "unknown"),
                displayLabel: "0 left"
            )
        )
        let absent = PresentationStore.mapBucketDto(
            bucketDto(count: nil, displayLabel: "Quota unavailable")
        )

        XCTAssertNotNil(zero.countQuota)
        XCTAssertEqual(zero.countQuota?.remaining, 0)
        XCTAssertEqual(zero.countQuota?.unit, "unknown")
        XCTAssertNil(absent.countQuota)
    }
}

final class PresentationSelectionTests: XCTestCase {
    @MainActor
    func testCountOverviewRendersLiteralUnsignedRequestsAndUnknownReset() {
        let remaining = "18446744073709551615 requests left"
        let account = PresentationStore.mapAccountDto(
            accountDto(
                count: countDto(used: nil, limit: UInt64.max, remaining: UInt64.max),
                resetsAt: nil,
                resetLabel: nil,
                resetDisplayLabel: "—",
                meterPercent: 100,
                remainingLabel: remaining
            )
        )
        let group = ProviderGroupRow(
            surfaceId: "codex",
            displayLabel: "OpenAI",
            iconKey: "codex",
            fallbackGlyph: "Cx",
            usageURL: nil,
            accountColumnLabel: account.accountLabel,
            planOrStatusLabel: account.planOrStatusLabel,
            remainingLabel: account.remainingLabel,
            resetDisplayLabel: account.resetDisplayLabel,
            accounts: [account],
            accessibilityLabel: account.accessibilityLabel,
            lastError: nil
        )

        let tree = OverviewInventory.tree(groups: [group])

        XCTAssertEqual(tree[0].children?.first?.remainingLabel, remaining)
        XCTAssertEqual(tree[0].children?.first?.resetLabel, "—")
        XCTAssertEqual(tree[0].children?.first?.accountKey, account.accountKey)
    }
}

private func countDto(
    used: UInt64?,
    limit: UInt64?,
    remaining: UInt64?,
    unit: String = "requests"
) -> CountQuotaDto {
    CountQuotaDto(
        used: used,
        limit: limit,
        remaining: remaining,
        unit: unit,
        period: unit == "requests" ? "utc_daily" : "unknown",
        provenance: "provider_reported"
    )
}

private func accountDto(
    count: CountQuotaDto?,
    resetsAt: Int64?,
    resetLabel: String? = "Resets tomorrow",
    resetDisplayLabel: String = "Resets tomorrow",
    meterPercent: Double? = 0,
    meterSide: String? = "remaining",
    remainingPercent: Double = 57.125,
    providerQuotaState: String? = nil,
    remainingLabel: String = "0 requests left"
) -> AccountDescriptorDto {
    AccountDescriptorDto(
        surfaceId: "codex",
        providerColumnLabel: "OpenAI",
        accountKey: "account-1",
        accountLabel: "Test account",
        planLabel: "Pro",
        selected: true,
        lifecycle: "current",
        lifecycleLabel: "Available",
        provenance: ["fixture"],
        provenanceLabel: "Fixture source",
        planOrStatusLabel: "Pro",
        usedPercent: 43.375,
        usedPercentOrigin: "derived",
        remainingPercent: remainingPercent,
        remainingPercentOrigin: "provider_reported",
        countQuota: count,
        providerQuotaState: providerQuotaState,
        resetsAt: resetsAt,
        meterPercent: meterPercent,
        meterSide: meterSide,
        remainingLabel: remainingLabel,
        headline: remainingLabel,
        resetLabel: resetLabel,
        resetDisplayLabel: resetDisplayLabel,
        exactReset: nil,
        statusWord: "fresh",
        statusLabel: "Ready",
        severity: "normal",
        updatedLabel: "Updated now",
        lastError: nil,
        dimmed: false,
        accessibilityLabel: "OpenAI, Test account, \(remainingLabel)"
    )
}

private func bucketDto(
    count: CountQuotaDto?,
    displayLabel: String,
    remainingLabel: String? = nil,
    usedPercent: Double? = 43.375,
    remainingPercent: Double? = 57.125,
    meterPercent: Double? = 57.125,
    meterSide: String? = "remaining",
    providerQuotaState: String? = nil
) -> QuotaBucketDto {
    QuotaBucketDto(
        label: "Requests",
        usedLabel: nil,
        limitLabel: nil,
        usedPercent: usedPercent,
        usedPercentOrigin: usedPercent == nil ? nil : "derived",
        remainingPercent: remainingPercent,
        remainingPercentOrigin: remainingPercent == nil ? nil : "provider_reported",
        countQuota: count,
        providerQuotaState: providerQuotaState,
        resetLabel: nil,
        resetsAt: nil,
        statusSlot: nil,
        paceLabel: nil,
        status: "fresh",
        usedMoney: nil,
        limitMoney: nil,
        severity: "normal",
        remainingLabel: remainingLabel,
        displaySegments: [displayLabel],
        displayLabel: displayLabel,
        meterPercent: meterPercent,
        meterSide: meterSide
    )
}

private struct GlanceQuotaFixture {
    let providerQuotaState: String?
    let remainingPercent: Double?
    let count: CountQuotaDto?
    let meterPercent: Double?
    let meterSide: String?
    let headline: String
}

private func glanceDto(_ fixture: GlanceQuotaFixture) -> ProviderGlanceRowDto {
    ProviderGlanceRowDto(
        surfaceId: "codex",
        iconKey: "codex",
        fallbackGlyph: "Cx",
        usageUrl: "https://example.test/usage",
        displayLabel: "OpenAI",
        accountLabel: "Test account",
        planLabel: "Pro",
        glanceUsedPercent: nil,
        glanceUsedPercentOrigin: nil,
        glanceRemainingPercent: fixture.remainingPercent,
        glanceRemainingPercentOrigin: fixture.remainingPercent == nil ? nil : "provider_reported",
        glanceCountQuota: fixture.count,
        glanceProviderQuotaState: fixture.providerQuotaState,
        glanceResetsAt: nil,
        glanceMeterPercent: fixture.meterPercent,
        glanceMeterSide: fixture.meterSide,
        barLabel: fixture.headline,
        headline: fixture.headline,
        resetLabel: nil,
        compactResetLabel: nil,
        exactReset: nil,
        statusWord: "fresh",
        isRefreshing: false,
        statusLabel: "fresh",
        severity: "normal",
        updatedLabel: "Updated now",
        activityLabel: "Updated now",
        activityKind: "idle",
        accessibilityLabel: "OpenAI, Test account, \(fixture.headline)",
        lastError: nil,
        dimmed: false
    )
}
