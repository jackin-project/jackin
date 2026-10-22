// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation
import JackinUsageBridge

struct BucketFixture {
    let id: String
    let label: String
    let remaining: String
    let meter: Double
    let severity: String
    let supporting: [String]
}

extension VisualQAFixtures {
    static func detailPresentation(for provider: Provider) -> UsageDetailPresentation {
        switch provider {
        case .codex: openAIDetail()
        case .claude: anthropicDetail()
        default: .empty
        }
    }

    static func openAIDetail() -> UsageDetailPresentation {
        UsageDetailPresentation(rows: [
            metadata(id: "auth", label: "Auth", value: "OAuth · ~/.codex/auth.json"),
            bucket(
                BucketFixture(
                    id: "bucket:0",
                    label: "Session",
                    remaining: "63% left",
                    meter: 63,
                    severity: "normal",
                    supporting: ["On pace", "Resets in 2h 14m"]
                )),
            bucket(
                BucketFixture(
                    id: "bucket:1",
                    label: "Weekly",
                    remaining: "57% left",
                    meter: 57,
                    severity: "warn",
                    supporting: ["13% in deficit · Runs out in 2d 17h", "Resets in 3d"]
                )),
            bucket(
                BucketFixture(
                    id: "bucket:2",
                    label: "Codex Spark 5-hour",
                    remaining: "88% left",
                    meter: 88,
                    severity: "normal",
                    supporting: ["On pace", "Resets in 4h 02m"]
                )),
            bucket(
                BucketFixture(
                    id: "bucket:3",
                    label: "Codex Spark Weekly",
                    remaining: "100% left",
                    meter: 100,
                    severity: "normal",
                    supporting: ["Resets in 7d"]
                )),
            detail(
                id: "bucket:4",
                label: "Limit Reset Credits",
                lines: ["3 manual resets available", "Next expires in 3d 4h"]
            ),
        ])
    }

    static func exhaustedOpenAIDetail() -> UsageDetailPresentation {
        var rows = openAIDetail().rows
        rows[1] = bucket(
            BucketFixture(
                id: "bucket:0",
                label: "Session",
                remaining: "0% left",
                meter: 0,
                severity: "danger",
                supporting: ["Resets in 42m"]
            ))
        return UsageDetailPresentation(rows: rows)
    }

    static func anthropicDetail() -> UsageDetailPresentation {
        UsageDetailPresentation(
            rows: anthropicQuotaBuckets() + anthropicModelBuckets() + [
                detail(
                    id: "bucket:6",
                    label: "Extra usage",
                    lines: ["Spend bound", "Quota-bound money / spend slot (limits only)"]
                )
            ])
    }

    static func anthropicQuotaBuckets() -> [UsageDetailRow] {
        [
            bucket(
                BucketFixture(
                    id: "bucket:0",
                    label: "Session",
                    remaining: "74% left",
                    meter: 74,
                    severity: "normal",
                    supporting: ["12% in deficit", "Resets in 4h 19m"]
                ))
        ]
    }

    static func anthropicModelBuckets() -> [UsageDetailRow] {
        [
            bucket(
                BucketFixture(
                    id: "bucket:1",
                    label: "Weekly",
                    remaining: "12% left",
                    meter: 12,
                    severity: "danger",
                    supporting: ["52% in reserve", "Resets in 1h"]
                )),
            bucket(
                BucketFixture(
                    id: "bucket:2",
                    label: "All models",
                    remaining: "28% left",
                    meter: 28,
                    severity: "warn",
                    supporting: ["Weekly all-models window", "Resets with weekly"]
                )),
            bucket(
                BucketFixture(
                    id: "bucket:3",
                    label: "Sonnet",
                    remaining: "35% left",
                    meter: 35,
                    severity: "warn",
                    supporting: ["Model-scoped · paced", "Resets in 6d 12h"]
                )),
            bucket(
                BucketFixture(
                    id: "bucket:4",
                    label: "Fable only",
                    remaining: "28% left",
                    meter: 28,
                    severity: "warn",
                    supporting: ["Resets in 12h 19m"]
                )),
            bucket(
                BucketFixture(
                    id: "bucket:5",
                    label: "Daily Routines",
                    remaining: "100% left",
                    meter: 100,
                    severity: "normal",
                    supporting: ["No reset timestamp from provider"]
                )),
        ]
    }

    static func metadata(id: String, label: String, value: String) -> UsageDetailRow {
        UsageDetailRow(
            rowId: id,
            kind: .metadata,
            label: label,
            layoutLines: [UsagePresentationLine(leading: value, trailing: nil)],
            displayLabel: value,
            meterPercent: nil,
            severity: "normal"
        )
    }

    static func detail(id: String, label: String, lines: [String]) -> UsageDetailRow {
        UsageDetailRow(
            rowId: id,
            kind: .detail,
            label: label,
            layoutLines: lines.map { UsagePresentationLine(leading: $0, trailing: nil) },
            displayLabel: lines.joined(separator: " · "),
            meterPercent: nil,
            severity: "normal"
        )
    }

    static func bucket(_ fixture: BucketFixture) -> UsageDetailRow {
        UsageDetailRow(
            rowId: fixture.id,
            kind: .bucket,
            label: fixture.label,
            layoutLines: ([fixture.remaining] + fixture.supporting).map {
                UsagePresentationLine(leading: $0, trailing: nil)
            },
            displayLabel: ([fixture.remaining] + fixture.supporting).joined(separator: " · "),
            meterPercent: fixture.meter,
            severity: fixture.severity,
            meterSide: "remaining"
        )
    }
}
