// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation
import XCTest

@testable import JackinUsageBindings
@testable import JackinUsageBridge

@MainActor
final class CountQuotaPresentationTests: XCTestCase {
    func testGeneratedCountCodecPreservesUnsignedBoundaryAndUnknownValues() {
        let values: [UInt64?] = [nil, 0, 1, 9_007_199_254_740_993, UInt64.max]
        for value in values {
            for period in ["utc_daily", "unknown"] {
                let quota = count(value, value, value, period: period)
                var writer = WireWriter()
                quota.encode(to: &writer)
                var reader = WireReader(data: writer.data)
                XCTAssertEqual(CountQuotaDto.decode(from: &reader), quota)
                XCTAssertEqual(reader.position, writer.data.count)
                if value == UInt64.max {
                    XCTAssertEqual(
                        Array(writer.data.prefix(9)), [1] + Array(repeating: 255, count: 8))
                } else if value == nil {
                    XCTAssertEqual(Array(writer.data.prefix(3)), [0, 0, 0])
                }
            }
        }
    }

    func testGeneratedCountCodecPreservesIndependentFieldPresenceAndOrder() {
        let quota = count(nil, UInt64.max, 9_007_199_254_740_993, period: "unknown")
        var writer = WireWriter()
        quota.encode(to: &writer)
        var reader = WireReader(data: writer.data)
        let decoded = CountQuotaDto.decode(from: &reader)
        XCTAssertNil(decoded.used)
        XCTAssertEqual(decoded.limit, UInt64.max)
        XCTAssertEqual(decoded.remaining, 9_007_199_254_740_993)
        XCTAssertEqual(decoded, quota)
        XCTAssertEqual(Array(writer.data.prefix(10)), [0, 1] + Array(repeating: 255, count: 8))
    }

    func testGeneratedBucketAndAccountCodecsPreserveTypedCountsAndZeroReset() {
        let quota = count(nil, UInt64.max, 9_007_199_254_740_993, period: "unknown")
        let originalBucket = bucket(quota, reset: 0)
        var bucketWriter = WireWriter()
        originalBucket.encode(to: &bucketWriter)
        var bucketReader = WireReader(data: bucketWriter.data)
        XCTAssertEqual(QuotaBucketDto.decode(from: &bucketReader), originalBucket)
        let originalAccount = account(quota, reset: 0)
        var accountWriter = WireWriter()
        originalAccount.encode(to: &accountWriter)
        var accountReader = WireReader(data: accountWriter.data)
        XCTAssertEqual(AccountDescriptorDto.decode(from: &accountReader), originalAccount)
    }

    func testNativeBucketCopiesTypedCountWithoutParsingDisplayLabels() {
        let quota = count(9_007_199_254_740_993, UInt64.max, 2, period: "unknown")
        let row = PresentationStore.mapBucketDto(bucket(quota, reset: nil))
        XCTAssertEqual(row.countQuota?.used, 9_007_199_254_740_993)
        XCTAssertEqual(row.countQuota?.limit, UInt64.max)
        XCTAssertEqual(row.countQuota?.remaining, 2)
        XCTAssertEqual(row.countQuota?.unit, "requests")
        XCTAssertEqual(row.countQuota?.period, "unknown")
        XCTAssertEqual(row.countQuota?.provenance, "provider_reported")
        XCTAssertEqual(row.usedLabel, "$1.00")
        XCTAssertEqual(row.limitLabel, "9000 tokens")
        XCTAssertEqual(row.displaySegments, ["9007199254740993 requests used", "2 requests left"])
        XCTAssertNil(row.usedMoney)
        XCTAssertNil(row.limitMoney)
        XCTAssertNil(row.resetsAt)
    }

    func testUnknownZeroAndAbsentQuotaRemainDistinct() {
        let unknown = PresentationStore.mapBucketDto(
            bucket(count(nil, nil, nil, period: "unknown"), reset: nil))
        let zero = PresentationStore.mapBucketDto(
            bucket(count(0, 0, 0, period: "utc_daily"), reset: 0))
        let absent = PresentationStore.mapBucketDto(bucket(nil, reset: nil))
        XCTAssertNotNil(unknown.countQuota)
        XCTAssertNil(unknown.countQuota?.used)
        XCTAssertNil(unknown.countQuota?.remaining)
        XCTAssertNil(unknown.resetsAt)
        XCTAssertEqual(zero.countQuota?.used, 0)
        XCTAssertEqual(zero.countQuota?.limit, 0)
        XCTAssertEqual(zero.countQuota?.remaining, 0)
        XCTAssertEqual(zero.resetsAt, 0)
        XCTAssertNil(absent.countQuota)
        XCTAssertNotEqual(unknown.countQuota, zero.countQuota)
    }

    func testNativeAccountSummaryPreservesCountsAndIndependentResetEpoch() {
        let quota = count(nil, UInt64.max, 9_007_199_254_740_993, period: "unknown")
        let row = PresentationStore.mapAccountDto(account(quota, reset: 0))
        XCTAssertNil(row.countQuota?.used)
        XCTAssertEqual(row.countQuota?.limit, UInt64.max)
        XCTAssertEqual(row.countQuota?.remaining, 9_007_199_254_740_993)
        XCTAssertEqual(row.countQuota?.period, "unknown")
        XCTAssertEqual(row.resetsAt, 0)
        XCTAssertEqual(row.remainingLabel, "9007199254740993 requests left")
        XCTAssertNil(PresentationStore.mapAccountDto(account(nil, reset: nil)).countQuota)
        XCTAssertNil(PresentationStore.mapAccountDto(account(quota, reset: nil)).resetsAt)
    }

    private func account(_ quota: CountQuotaDto?, reset: Int64?) -> AccountDescriptorDto {
        AccountDescriptorDto(
            surfaceId: "openrouter",
            providerColumnLabel: "",
            accountKey: "count-account",
            accountLabel: "count-account",
            planLabel: nil,
            selected: true,
            lifecycle: "current",
            lifecycleLabel: "Current",
            provenance: [],
            provenanceLabel: "Provider reported",
            planOrStatusLabel: "Ready",
            remainingPercent: nil,
            remainingLabel: "9007199254740993 requests left",
            headline: "9007199254740993 requests left",
            resetLabel: nil,
            resetDisplayLabel: "—",
            exactReset: nil,
            statusWord: "fresh",
            statusLabel: "Ready",
            severity: "normal",
            updatedLabel: "Updated now",
            lastError: nil,
            dimmed: false,
            accessibilityLabel: "count-account",
            countQuota: quota,
            resetsAt: reset,
            usedMoney: nil,
            limitMoney: nil,
            remainingMoney: nil)
    }

    private func count(
        _ used: UInt64?, _ limit: UInt64?, _ remaining: UInt64?, period: String
    ) -> CountQuotaDto {
        CountQuotaDto(
            used: used,
            limit: limit,
            remaining: remaining,
            unit: "requests",
            period: period,
            provenance: "provider_reported")
    }

    private func bucket(_ quota: CountQuotaDto?, reset: Int64?) -> QuotaBucketDto {
        QuotaBucketDto(
            label: "Credits",
            usedLabel: "$1.00",
            limitLabel: "9000 tokens",
            remainingPercent: nil,
            resetLabel: nil,
            resetsAt: reset,
            statusSlot: nil,
            paceLabel: nil,
            status: "fresh",
            usedMoney: nil,
            limitMoney: nil,
            remainingMoney: nil,
            countQuota: quota,
            severity: "normal",
            remainingLabel: nil,
            displaySegments: ["9007199254740993 requests used", "2 requests left"],
            displayLabel: "9007199254740993 requests used · 2 requests left",
            meterPercent: nil)
    }
}
