// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation
import XCTest

@testable import JackinUsageBindings
@testable import JackinUsageBridge

@MainActor
final class MoneyQuotaPresentationTests: XCTestCase {
    func testGeneratedMoneyCodecPreservesSignedBoundsCurrencyAndExponent() {
        for amount in [Int64.min, -250, 0, 9_007_199_254_740_993, Int64.max] {
            for exponent: UInt8 in [0, 2, 3, 255] {
                let money = MoneyDto(amountMinor: amount, currency: "SGD", exponent: exponent)
                var writer = WireWriter()
                money.encode(to: &writer)
                var reader = WireReader(data: writer.data)
                XCTAssertEqual(MoneyDto.decode(from: &reader), money)
                XCTAssertEqual(reader.position, writer.data.count)
                if amount == Int64.min {
                    XCTAssertEqual(
                        Array(writer.data.prefix(8)), Array(repeating: 0, count: 7) + [128])
                } else if amount == Int64.max {
                    XCTAssertEqual(
                        Array(writer.data.prefix(8)), Array(repeating: 255, count: 7) + [127])
                }
            }
        }
    }

    func testOptionalMoneyWireDistinguishesUnknownAndPresentZero() {
        let zero = MoneyDto(amountMinor: 0, currency: "JPY", exponent: 0)
        for money in [nil, zero] {
            var writer = WireWriter()
            writer.writeOptional(money) { writer, value in value.encode(to: &writer) }
            var reader = WireReader(data: writer.data)
            let decoded = reader.readOptional { MoneyDto.decode(from: &$0) }
            XCTAssertEqual(decoded, money)
            XCTAssertEqual(writer.data.first, money == nil ? 0 : 1)
        }
    }

    func testGeneratedBucketAndAccountCodecsPreserveIndependentMoneyFields() {
        let used = MoneyDto(amountMinor: 9_007_199_254_740_993, currency: "USD", exponent: 2)
        let remaining = MoneyDto(amountMinor: -250, currency: "USD", exponent: 2)
        let originalBucket = bucket(used: used, limit: nil, remaining: remaining)
        var bucketWriter = WireWriter()
        originalBucket.encode(to: &bucketWriter)
        var bucketReader = WireReader(data: bucketWriter.data)
        XCTAssertEqual(QuotaBucketDto.decode(from: &bucketReader), originalBucket)
        let originalAccount = account(used: nil, limit: used, remaining: remaining)
        var accountWriter = WireWriter()
        originalAccount.encode(to: &accountWriter)
        var accountReader = WireReader(data: accountWriter.data)
        XCTAssertEqual(AccountDescriptorDto.decode(from: &accountReader), originalAccount)
    }

    func testNativeBucketPreservesMoneyWithoutParsingLabelsOrNarrowingAmounts() {
        let used = MoneyDto(amountMinor: 9_007_199_254_740_993, currency: "USD", exponent: 2)
        let limit = MoneyDto(amountMinor: Int64.max, currency: "USD", exponent: 2)
        let remaining = MoneyDto(amountMinor: -250, currency: "USD", exponent: 2)
        let row = PresentationStore.mapBucketDto(
            bucket(used: used, limit: limit, remaining: remaining))
        XCTAssertEqual(row.usedMoney, used)
        XCTAssertEqual(row.limitMoney, limit)
        XCTAssertEqual(row.remainingMoney, remaining)
        XCTAssertNil(row.countQuota)
        XCTAssertEqual(row.usedLabel, "9000 tokens")
        XCTAssertEqual(row.limitLabel, "JPY 7")
        XCTAssertEqual(row.displayLabel, "$90071992547409.93 used · $-2.50 remaining")
    }

    func testNativeAccountPreservesIndependentUnknownMoneyAndSignedRemaining() {
        let limit = MoneyDto(amountMinor: Int64.max, currency: "SGD", exponent: 255)
        let remaining = MoneyDto(amountMinor: Int64.min, currency: "SGD", exponent: 255)
        let row = PresentationStore.mapAccountDto(
            account(used: nil, limit: limit, remaining: remaining))
        XCTAssertNil(row.usedMoney)
        XCTAssertEqual(row.limitMoney, limit)
        XCTAssertEqual(row.remainingMoney, remaining)
        XCTAssertNil(row.countQuota)
        XCTAssertNil(row.resetsAt)
        XCTAssertEqual(row.remainingLabel, "Rust-owned remaining label")
    }

    func testNativePresentZeroMoneyRemainsDifferentFromUnknown() {
        let zero = MoneyDto(amountMinor: 0, currency: "USD", exponent: 2)
        let bucket = PresentationStore.mapBucketDto(
            bucket(used: zero, limit: zero, remaining: zero))
        XCTAssertEqual(bucket.usedMoney, zero)
        XCTAssertEqual(bucket.limitMoney, zero)
        XCTAssertEqual(bucket.remainingMoney, zero)
        XCTAssertNil(bucket.meterPercent)
        let row = PresentationStore.mapAccountDto(account(used: zero, limit: zero, remaining: zero))
        XCTAssertEqual(row.usedMoney, zero)
        XCTAssertEqual(row.limitMoney, zero)
        XCTAssertEqual(row.remainingMoney, zero)
        let unknown = PresentationStore.mapAccountDto(
            account(used: nil, limit: nil, remaining: nil))
        XCTAssertNil(unknown.usedMoney)
        XCTAssertNil(unknown.limitMoney)
        XCTAssertNil(unknown.remainingMoney)
    }

    private func bucket(used: MoneyDto?, limit: MoneyDto?, remaining: MoneyDto?) -> QuotaBucketDto {
        QuotaBucketDto(
            label: "Spend",
            usedLabel: "9000 tokens",
            limitLabel: "JPY 7",
            remainingPercent: nil,
            resetLabel: nil,
            resetsAt: nil,
            statusSlot: "spend",
            paceLabel: nil,
            status: "fresh",
            usedMoney: used,
            limitMoney: limit,
            remainingMoney: remaining,
            countQuota: nil,
            severity: "normal",
            remainingLabel: nil,
            displaySegments: ["$90071992547409.93 used", "$-2.50 remaining"],
            displayLabel: "$90071992547409.93 used · $-2.50 remaining",
            meterPercent: nil)
    }

    private func account(
        used: MoneyDto?, limit: MoneyDto?, remaining: MoneyDto?
    ) -> AccountDescriptorDto {
        AccountDescriptorDto(
            surfaceId: "openrouter",
            providerColumnLabel: "",
            accountKey: "money-account",
            accountLabel: "money-account",
            planLabel: nil,
            selected: true,
            lifecycle: "current",
            lifecycleLabel: "Current",
            provenance: [],
            provenanceLabel: "Provider reported",
            planOrStatusLabel: "Ready",
            remainingPercent: nil,
            remainingLabel: "Rust-owned remaining label",
            headline: "Rust-owned headline",
            resetLabel: nil,
            resetDisplayLabel: "—",
            exactReset: nil,
            statusWord: "fresh",
            statusLabel: "Ready",
            severity: "normal",
            updatedLabel: "Updated now",
            lastError: nil,
            dimmed: false,
            accessibilityLabel: "money-account",
            countQuota: nil,
            resetsAt: nil,
            usedMoney: used,
            limitMoney: limit,
            remainingMoney: remaining)
    }
}
