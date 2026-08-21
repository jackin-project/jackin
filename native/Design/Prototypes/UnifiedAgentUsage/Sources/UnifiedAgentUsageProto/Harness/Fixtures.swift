import Foundation
import SwiftUI

enum ProtoFixtures {
    static let resetUnavailable = "Reset unavailable"

    static func load(_ name: String) -> ProtoProjection {
        guard let projection = projection(named: name) else {
            fatalError("unknown --tr-scenario \(name)")
        }
        return projection
    }

    static func projection(named name: String) -> ProtoProjection? {
        switch name {
        case "default", "F02": f02()
        case "F00": f00()
        case "F01": f01()
        case "F03": f03(selected: "codex-plus")
        case "F04": f04()
        case "F05": f05()
        case "F06": f06()
        case "F07": f07()
        case "F08": f08()
        case "F09": f09()
        case "F10": f10()
        case "F11": f11()
        case "F12": f12()
        case "F13": f13()
        case "F14": f14()
        case "F15":
            f02(
                scenario: "F15", script: .acceptPercentStyle,
                statusRows: ["claude", "amp", "codex"])
        case "F16":
            f02(
                scenario: "F16", script: .rejectLowFloor,
                statusRows: ["claude", "amp", "codex"])
        case "F17":
            f02(
                scenario: "F17", script: .reorderedFloor,
                statusRows: ["claude", "amp", "codex"])
        case "F18-f02": f02(scenario: "F18-f02")
        case "F18-f11": f11(scenario: "F18-f11")
        case "F19-en-US":
            f19(
                scenario: "F19-en-US", localeID: "en_US", direction: .leftToRight,
                provider: "OpenAI Organization Production Sandbox — Southeast Asia",
                account: "organization-production-sandbox@example.test",
                plan: "Enterprise workspace with centrally managed weekly limits",
                reset: "Resets Tuesday, 18 August 2026 at 23:59 Indochina Time",
                error:
                    "Provider response could not be refreshed; showing the last successful quota snapshot",
                refresh: "Refresh Refresh", openUsage: "Open Usage Open Usage")
        case "F19-ar-SA":
            f19(
                scenario: "F19-ar-SA", localeID: "ar_SA", direction: .rightToLeft,
                provider: "أوبن إيه آي", account: "team-01@example.test", plan: "فريق",
                reset: "تتم إعادة الضبط خلال ٣ أيام",
                error: "تعذّر تحديث الاستخدام؛ تظهر آخر لقطة ناجحة",
                refresh: "تحديث", openUsage: "فتح الاستخدام")
        case "F19-ja-JP":
            f19(
                scenario: "F19-ja-JP", localeID: "ja_JP", direction: .leftToRight,
                provider: "OpenAI", account: "研究チーム@example.test", plan: "エンタープライズ",
                reset: "8月18日火曜日 23:59にリセット",
                error: "使用量を更新できないため、最後に成功した値を表示しています",
                refresh: "更新", openUsage: "使用状況を開く")
        case "F19-de-DE":
            f19(
                scenario: "F19-de-DE", localeID: "de_DE", direction: .leftToRight,
                provider: "OpenAI", account: "forschung@example.test", plan: "Unternehmen",
                reset: "Zurücksetzung am Dienstag, 18. August 2026 um 23:59 Uhr",
                error: "Schlüsselbundzugriff verweigert",
                refresh: "Aktualisieren", openUsage: "Nutzung öffnen")
        case "F20": f02(scenario: "F20")
        case "F21": f03(scenario: "F21", selected: "codex-personal", statusRows: ["codex"])
        case "F22": f22()
        case "F23": f03(scenario: "F23", selected: "codex-personal", statusRows: ["codex"])
        case "F24-f02": f02(scenario: "F24-f02")
        case "F24-f11": f11(scenario: "F24-f11")
        case "F24-f12": f12(scenario: "F24-f12")
        case "F25": f25()
        case "F26": f26()
        case "F27": f27()
        case "F28": f28()
        case "F29": f29()
        default: nil
        }
    }

    /// Every previewable scenario, grouped for the in-prototype Scenario menu.
    /// Launch contract names stay identical — the menu only re-drives them.
    static let scenarioMenu: [(title: String, names: [String])] = [
        ("Everyday", ["F02", "F01", "F03", "F25"]),
        ("Quota pressure", ["F04", "F05", "F22"]),
        ("Degraded", ["F06", "F07", "F08", "F10", "F29"]),
        ("Credential gaps", ["F09", "F26", "F27", "F28"]),
        ("Scale and stress", ["F11", "F12"]),
        ("Global states", ["F00", "F13", "F14"]),
        ("Localization", ["F19-en-US", "F19-de-DE", "F19-ja-JP", "F19-ar-SA"]),
        ("Mutations", ["F15", "F16", "F17"]),
        (
            "Contract pins",
            ["F18-f02", "F18-f11", "F20", "F21", "F23", "F24-f02", "F24-f11", "F24-f12"]
        ),
    ]

    /// One-line human description per scenario — surfaced as the Scenario
    /// menu item subtitle/tooltip so a code like "F24-f11" is never the only
    /// label. Mirrors the `### Fxx —` headings in Fixtures.md.
    static let scenarioDescriptions: [String: String] = [
        "F00": "No providers detected — empty state with add-provider prompt",
        "F01": "Single provider, healthy quota",
        "F02": "Full provider catalog, all healthy (default)",
        "F03": "Multi-account provider with account switcher",
        "F04": "Nearly exhausted quota — warning severity",
        "F05": "Exhausted quota — depleted state",
        "F06": "Stale last-good data after a failed refresh",
        "F07": "Refresh in progress over last-good data",
        "F08": "One provider timed out, the rest healthy",
        "F09": "Permission denied — re-authorization required",
        "F10": "Offline, showing cached data",
        "F11": "Long labels and emails — truncation stress",
        "F12": "Large dataset layout envelope",
        "F13": "Initial loading state",
        "F14": "Global bridge error",
        "F15": "Accepted preference mutation (percent style)",
        "F16": "Rejected preference mutation with retry",
        "F17": "Mutation completions arriving out of order",
        "F18-f02": "Accessibility display settings pinned over F02",
        "F18-f11": "Accessibility display settings pinned over F11",
        "F19-en-US": "Localization — English (US)",
        "F19-de-DE": "Localization — German (Germany)",
        "F19-ja-JP": "Localization — Japanese (Japan)",
        "F19-ar-SA": "Localization — Arabic (Saudi Arabia), right-to-left",
        "F20": "Destructive pending sentinel",
        "F21": "Keyboard and VoiceOver task completion",
        "F22": "Provider-supplied money cap window",
        "F23": "Physical display and window restoration",
        "F24-f02": "Continuous resize and overflow pinned over F02",
        "F24-f11": "Continuous resize and overflow pinned over F11",
        "F24-f12": "Continuous resize and overflow pinned over F12",
        "F25": "Multi-account rich overview grid",
        "F26": "Sign-in required state",
        "F27": "API key required state",
        "F28": "Unsupported credential state",
        "F29": "Rate limited with backoff, last-good rows",
    ]

    // MARK: Core records

    static let codexPersonal = ProtoAccount(
        key: "codex-personal", label: "personal@example.test", plan: "Plus",
        remaining: 57, resetText: "Resets in 3d", state: .current,
        windows: [
            ProtoQuotaWindow(
                stableID: "bucket:weekly", label: "Weekly", category: .longRange, periodTag: "w",
                display: "57% left · Resets in 3d", primaryValue: "57% left",
                resetLabel: "Resets in 3d", meter: 57, state: .warning,
                pace: "Runs out in 2d at current pace"),
            ProtoQuotaWindow(
                stableID: "bucket:review", label: "Code review · Weekly", category: .model,
                display: "91% left · Resets in 3d", primaryValue: "91% left",
                resetLabel: "Resets in 3d", meter: 91, state: .current),
            ProtoQuotaWindow(
                stableID: "bucket:five-hour", label: "Five-hour", category: .session,
                display: "63% left · Resets in 2h", primaryValue: "63% left",
                resetLabel: "Resets in 2h", meter: 63, state: .current,
                pace: "On pace"),
            ProtoQuotaWindow(
                stableID: "bucket:credits", label: "Credits",
                display: "3 manual resets available · Next expires in 3d 4h",
                primaryValue: "3 available", resetLabel: "Next expires in 3d 4h",
                supplementalValue: "Manual limit resets", meter: nil, state: .current),
        ], auth: "OAuth · configured profile")

    static let codexPlus = ProtoAccount(
        key: "codex-plus", label: "team@example.test", plan: "Plus",
        remaining: 0, resetText: "Resets in 3d", state: .depleted,
        windows: [
            ProtoQuotaWindow(
                stableID: "bucket:weekly", label: "Weekly", category: .longRange, periodTag: "w",
                display: "0% left · Resets in 3d", primaryValue: "0% left",
                resetLabel: "Resets in 3d", meter: 0, state: .depleted)
        ])

    static let codexOrganization = ProtoAccount(
        key: "codex-organization", label: "organization-production-sandbox@example.test",
        plan: "Enterprise", remaining: 88, resetText: "Resets in 3d", state: .current,
        windows: [
            ProtoQuotaWindow(
                stableID: "bucket:weekly", label: "Weekly", category: .longRange, periodTag: "w",
                display: "88% left · Resets in 3d", primaryValue: "88% left",
                resetLabel: "Resets in 3d", meter: 88, state: .current,
                pace: "On pace"),
            // Spend-control lane from the same /wham/usage payload
            // (individual_limit) — a quota-bound money cap, not spend tracking.
            ProtoQuotaWindow(
                stableID: "bucket:monthly-credit-pool", label: "Monthly credit pool",
                category: .longRange, periodTag: "mo",
                display: "$312 used of $500 cap · Resets Sep 1",
                primaryValue: "38% left", resetLabel: "Resets Sep 1",
                supplementalValue: "Monthly cap · $312 / $500", meter: 38,
                state: .current),
        ])

    static let claudePersonal = ProtoAccount(
        key: "claude-personal", label: "personal@example.test", plan: "Pro",
        remaining: 12, resetText: "Resets in 1h", state: .danger,
        windows: [
            ProtoQuotaWindow(
                stableID: "bucket:weekly", label: "All models", category: .general,
                display: "12% left · Resets in 1h", primaryValue: "12% left",
                resetLabel: "Resets in 1h", meter: 12, state: .danger),
            ProtoQuotaWindow(
                stableID: "bucket:fable", label: "Fable", category: .model,
                display: "65% left · Resets in 4d", primaryValue: "65% left",
                resetLabel: "Resets in 4d", meter: 65, state: .current,
                pace: "On pace"),
            ProtoQuotaWindow(
                stableID: "bucket:session", label: "Session", category: .session,
                display: "74% left", primaryValue: "74% left",
                meter: 74, state: .current,
                pace: "On pace"),
            ProtoQuotaWindow(
                stableID: "bucket:extra-usage", label: "Extra usage",
                display: "28% used · Monthly cap: $14 / $50", primaryValue: "28% used",
                supplementalValue: "Monthly cap · $14 / $50", meter: 28,
                state: .current),
        ], auth: "OAuth · macOS Keychain")

    static let ampDefault = ProtoAccount(
        key: "amp-default", label: "developer@example.test", plan: "Amp Free",
        remaining: 100, resetText: "Resets in 18h", state: .current,
        windows: [
            ProtoQuotaWindow(
                stableID: "bucket:daily", label: "Amp Free",
                display: "100% left · Resets daily", primaryValue: "100% left",
                resetLabel: "Resets daily", meter: 100, state: .current,
                notStarted: true),
            ProtoQuotaWindow(
                stableID: "bucket:individual", label: "Individual credits",
                display: "$18.40 remaining", primaryValue: "$18.40",
                supplementalValue: "Remaining balance", meter: nil, state: .current),
            ProtoQuotaWindow(
                stableID: "bucket:workspace", label: "Workspace Platform",
                display: "$126.75 remaining", primaryValue: "$126.75",
                supplementalValue: "Remaining balance", meter: nil, state: .current),
        ], auth: "API key · Amp settings")

    static let grokDefault = ProtoAccount(
        key: "grok-default", label: "developer@example.test", plan: "SuperGrok",
        remaining: 72, resetText: "Resets Sep 1", state: .current,
        windows: [
            ProtoQuotaWindow(
                stableID: "bucket:monthly", label: "Monthly", category: .longRange, periodTag: "mo",
                display: "72% left · Resets Sep 1", primaryValue: "72% left",
                resetLabel: "Resets Sep 1", meter: 72, state: .current,
                pace: "On pace"),
            ProtoQuotaWindow(
                stableID: "bucket:prepaid", label: "Extra usage credits",
                display: "$24.80 remaining", primaryValue: "$24.80",
                supplementalValue: "Prepaid balance", meter: nil, state: .current),
            ProtoQuotaWindow(
                stableID: "bucket:on-demand", label: "On-demand usage",
                display: "Budget: $12 / $100", primaryValue: "88% left",
                supplementalValue: "Budget · $12 / $100", meter: 88, state: .current),
        ], auth: "Grok CLI session")

    static let zaiDefault = ProtoAccount(
        key: "zai-default", label: "configured API key", plan: "Coding Pro",
        remaining: 81, resetText: "Resets in 4d", state: .current,
        windows: [
            ProtoQuotaWindow(
                stableID: "bucket:tokens", label: "Tokens", category: .model,
                display: "81% left · Resets in 4d", primaryValue: "81% left",
                resetLabel: "Resets in 4d", meter: 81, state: .current,
                pace: "On pace"),
            ProtoQuotaWindow(
                stableID: "bucket:mcp", label: "MCP", category: .model,
                display: "42 / 100 (58 remaining) · Resets in 4d",
                primaryValue: "58 remaining", secondaryValue: "42 / 100 used",
                resetLabel: "Resets in 4d", meter: 58,
                state: .current),
            ProtoQuotaWindow(
                stableID: "bucket:five-hour", label: "5-hour", category: .session,
                display: "94% left · Resets in 2h", primaryValue: "94% left",
                resetLabel: "Resets in 2h", meter: 94, state: .current),
        ], auth: "API key · env ZAI_API_KEY")

    static let kimiDefault = ProtoAccount(
        key: "kimi-default", label: "local Kimi Code", plan: "Coding",
        remaining: 45, resetText: "Resets in 5d", state: .current,
        windows: [
            ProtoQuotaWindow(
                stableID: "bucket:weekly", label: "Weekly", category: .longRange, periodTag: "w",
                display: "45% left · Resets in 5d", primaryValue: "45% left",
                resetLabel: "Resets in 5d", meter: 45, state: .current,
                pace: "Runs out in 4d at current pace"),
            ProtoQuotaWindow(
                stableID: "bucket:rate", label: "Rate Limit", category: .session,
                display: "76% left · Resets in 38m", primaryValue: "76% left",
                resetLabel: "Resets in 38m", meter: 76, state: .current,
                pace: "On pace"),
        ], auth: "Local token · Kimi Code credentials")

    static let minimaxDefault = ProtoAccount(
        key: "minimax-default", label: "configured API key", plan: "Coding Pro",
        remaining: 33, resetText: "Resets in 2d", state: .warning,
        windows: [
            ProtoQuotaWindow(
                stableID: "bucket:general-weekly", label: "General · Weekly", category: .longRange,
                periodTag: "w",
                display: "33% left · Usage: 6.7K / 10K · Resets in 2d",
                primaryValue: "33% left", secondaryValue: "6.7K / 10K used",
                resetLabel: "Resets in 2d", meter: 33,
                state: .warning),
            ProtoQuotaWindow(
                stableID: "bucket:lightning", label: "Lightning", category: .model,
                display: "84% left · Usage: 160 / 1K · Resets in 3h",
                primaryValue: "84% left", secondaryValue: "160 / 1K used",
                resetLabel: "Resets in 3h", meter: 84,
                state: .current),
            ProtoQuotaWindow(
                stableID: "bucket:general-5h", label: "General · 5h", category: .session,
                display: "68% left · Usage: 320 / 1K · Resets in 3h",
                primaryValue: "68% left", secondaryValue: "320 / 1K used",
                resetLabel: "Resets in 3h", meter: 68,
                state: .current),
        ], auth: "API token · env MINIMAX_API_KEY")

    static func catalogAccount(
        _ provider: String, remaining: Int, reset: String?
    ) -> ProtoAccount {
        ProtoAccount(
            key: "\(provider)-default", label: "default", plan: "Default",
            remaining: remaining, resetText: reset, state: .current,
            windows: [
                ProtoQuotaWindow(
                    stableID: "bucket:weekly", label: "Weekly", category: .longRange,
                    periodTag: "w",
                    display: "\(remaining)% left · \(reset ?? resetUnavailable)",
                    primaryValue: "\(remaining)% left",
                    resetLabel: reset ?? resetUnavailable,
                    meter: remaining, state: .current)
            ])
    }

    static func provider(
        _ key: String, _ name: String, percent: Int?, reset: String?,
        accounts: [ProtoAccount], selected: String? = nil,
        state: ProtoState = .current, updatedAgo: String? = nil,
        activity: String? = nil, error: String? = nil
    ) -> ProtoProvider {
        ProtoProvider(
            key: key, name: name, state: state, summaryPercent: percent,
            summaryReset: reset, accounts: accounts,
            selectedAccountKey: selected ?? accounts.first?.key,
            updatedAgo: updatedAgo, activityText: activity, errorText: error)
    }

    static func codexProvider(
        accounts: [ProtoAccount] = [codexPersonal], selected: String? = "codex-personal",
        state: ProtoState = .current, updatedAgo: String? = nil,
        activity: String? = nil, error: String? = nil
    ) -> ProtoProvider {
        provider(
            "codex", "OpenAI / Codex", percent: 57, reset: "Resets in 3d",
            accounts: accounts, selected: selected, state: state,
            updatedAgo: updatedAgo, activity: activity, error: error)
    }

    static func claudeProvider(
        state: ProtoState = .current, error: String? = nil, usable: Bool = true
    ) -> ProtoProvider {
        provider(
            "claude", "Anthropic / Claude", percent: 12, reset: "Resets in 1h",
            accounts: usable ? [claudePersonal] : [], state: state, error: error)
    }

    /// Seven desktop providers in frozen canonical order; Codex carries two
    /// accounts so the default scenario previews multi-account presentation.
    static func catalog(
        codexState: ProtoState = .current, codexActivity: String? = nil,
        kimiUnavailable: Bool = false
    ) -> [ProtoProvider] {
        [
            codexProvider(
                accounts: [codexPersonal, codexPlus],
                state: codexState, activity: codexActivity),
            claudeProvider(),
            provider(
                "amp", "Amp", percent: 100, reset: "Resets in 18h",
                accounts: [ampDefault]),
            provider(
                "grok", "xAI / Grok", percent: 72, reset: "Resets Sep 1",
                accounts: [grokDefault]),
            provider(
                "zai", "Z.AI / GLM", percent: 81, reset: "Resets in 4d",
                accounts: [zaiDefault]),
            kimiUnavailable
                ? provider(
                    "kimi", "Kimi", percent: 45, reset: nil, accounts: [],
                    state: .unavailable, error: "usage provider probe timed out")
                : provider(
                    "kimi", "Kimi", percent: 45, reset: "Resets in 5d",
                    accounts: [kimiDefault]),
            provider(
                "minimax", "MiniMax", percent: 33, reset: "Resets in 2d",
                accounts: [minimaxDefault], state: .warning),
        ]
    }

    // MARK: Scenarios

    static func projection(
        _ scenario: String, providers: [ProtoProvider], statusRows: [String],
        script: ProtoMutationScript = .standard,
        selectedProvider: String? = nil, selectedAccount: String? = nil,
        chrome: ProtoChrome = ProtoChrome()
    ) -> ProtoProjection {
        ProtoProjection(
            scenario: scenario, providers: providers, statusRows: statusRows,
            isLoading: false, globalError: nil, chrome: chrome, mutationScript: script,
            selectedProviderKey: selectedProvider, selectedAccountKey: selectedAccount)
    }

    static func f00() -> ProtoProjection {
        projection("F00", providers: [], statusRows: [])
    }

    static func f01() -> ProtoProjection {
        projection(
            "F01", providers: [codexProvider()], statusRows: ["codex"],
            selectedProvider: "codex", selectedAccount: "codex-personal")
    }

    static func f02(
        scenario: String = "F02", script: ProtoMutationScript = .standard,
        statusRows: [String] = ["claude", "amp", "codex"]
    ) -> ProtoProjection {
        projection(scenario, providers: catalog(), statusRows: statusRows, script: script)
    }

    static func f03(
        scenario: String = "F03", selected: String,
        statusRows: [String] = []
    ) -> ProtoProjection {
        projection(
            scenario,
            providers: [
                codexProvider(
                    accounts: [codexPersonal, codexPlus, codexOrganization],
                    selected: selected)
            ],
            statusRows: statusRows,
            selectedProvider: "codex", selectedAccount: selected)
    }

    static func f04() -> ProtoProjection {
        projection(
            "F04", providers: [claudeProvider()], statusRows: ["claude"],
            selectedProvider: "claude", selectedAccount: "claude-personal")
    }

    static func f05() -> ProtoProjection {
        projection(
            "F05",
            providers: [codexProvider(accounts: [codexPlus], selected: "codex-plus")],
            statusRows: [], selectedProvider: "codex", selectedAccount: "codex-plus")
    }

    static func f06() -> ProtoProjection {
        projection(
            "F06",
            providers: [
                codexProvider(
                    state: .stale, updatedAgo: "Updated 47m ago",
                    error: "Codex provider usage unavailable; cached quota is stale")
            ],
            statusRows: ["codex"], selectedProvider: "codex",
            selectedAccount: "codex-personal")
    }

    static func f07() -> ProtoProjection {
        projection(
            "F07",
            providers: catalog(codexState: .refreshing, codexActivity: "Updating…"),
            statusRows: ["claude", "amp", "codex"])
    }

    static func f08() -> ProtoProjection {
        projection(
            "F08", providers: catalog(kimiUnavailable: true),
            statusRows: ["claude", "amp", "codex"])
    }

    static func f09() -> ProtoProjection {
        projection(
            "F09",
            providers: [
                claudeProvider(
                    state: .unavailable, error: "Claude Keychain access denied",
                    usable: false)
            ],
            statusRows: [], selectedProvider: "claude")
    }

    static func f10() -> ProtoProjection {
        projection(
            "F10",
            providers: [
                provider(
                    "kimi", "Kimi", percent: 45, reset: "Resets in 5d",
                    accounts: [kimiDefault],
                    state: .stale, updatedAgo: "Updated 1h ago",
                    error: "Kimi billing endpoint unavailable; local presence only")
            ],
            statusRows: ["kimi"], selectedProvider: "kimi",
            selectedAccount: "kimi-default")
    }

    static func f11(scenario: String = "F11") -> ProtoProjection {
        projection(
            scenario,
            providers: [
                provider(
                    "codex",
                    "OpenAI Organization Production Sandbox — Southeast Asia",
                    percent: 57,
                    reset: "Resets Tuesday, 18 August 2026 at 23:59 Indochina Time",
                    accounts: [
                        ProtoAccount(
                            key: "codex-organization",
                            label: "organization-production-sandbox@example.test",
                            plan: "Enterprise workspace with centrally managed weekly limits",
                            remaining: 57,
                            resetText:
                                "Resets Tuesday, 18 August 2026 at 23:59 Indochina Time",
                            state: .current,
                            windows: [
                                ProtoQuotaWindow(
                                    stableID: "bucket:weekly",
                                    label: "Organization-wide weekly accelerated-model allocation",
                                    category: .longRange, periodTag: "w",
                                    display:
                                        "57% left · Resets Tuesday, 18 August 2026 at 23:59 Indochina Time",
                                    primaryValue: "57% left",
                                    resetLabel:
                                        "Resets Tuesday, 18 August 2026 at 23:59 Indochina Time",
                                    meter: 57, state: .current)
                            ])
                    ],
                    state: .stale,
                    error:
                        "Provider response could not be refreshed; showing the last successful quota snapshot"
                )
            ],
            statusRows: ["codex"], selectedProvider: "codex",
            selectedAccount: "codex-organization")
    }

    static func f12(scenario: String = "F12") -> ProtoProjection {
        let surfaces = ["codex", "claude", "amp", "grok", "zai", "kimi", "minimax"]
        let names = [
            "OpenAI / Codex", "Anthropic / Claude", "Amp", "xAI / Grok",
            "Z.AI / GLM", "Kimi", "MiniMax",
        ]
        let plans = ["Free", "Plus", "Pro", "Team", "Enterprise", "Default"]
        let cycle: [Int?] = [88, nil, 28, 0, 12, 57, 100]
        let windowLabels = [
            "Hourly", "Daily", "Daily", "Weekly", "Monthly", "Model",
            "Organization", "Credits",
        ]
        let resets = [
            "Resets in 1h", "Resets in 6h", "Resets in 18h", "Resets in 3d",
            "Resets Sep 1", "Reset unavailable", "Resets Tuesday 23:59",
            "No reset supplied",
        ]
        var globalIndex = 0
        var providers: [ProtoProvider] = []
        for (providerIndex, surface) in surfaces.enumerated() {
            var accounts: [ProtoAccount] = []
            for ordinal in 1...6 {
                let index = globalIndex
                globalIndex += 1
                let key = "\(surface)-load-0\(ordinal)"
                let label =
                    key == "claude-load-03"
                    ? "Research workspace" : "\(surface)-0\(ordinal)@example.test"
                let remaining = cycle[index % 7]
                let windows = (0..<8).map { windowIndex -> ProtoQuotaWindow in
                    let windowRemaining = cycle[(index + windowIndex) % 7]
                    let remainingText =
                        windowRemaining.map { "\($0)% left" } ?? "Remaining unavailable"
                    return ProtoQuotaWindow(
                        stableID: "limit-0\(windowIndex + 1)",
                        label: windowLabels[windowIndex],
                        display: "\(remainingText) · \(resets[windowIndex])",
                        primaryValue: remainingText,
                        resetLabel: resets[windowIndex],
                        meter: windowRemaining,
                        state: stateFor(remaining: windowRemaining))
                }
                accounts.append(
                    ProtoAccount(
                        key: key, label: label, plan: plans[ordinal - 1],
                        remaining: remaining, resetText: resets[index % 8],
                        state: stateFor(remaining: remaining), windows: windows))
            }
            providers.append(
                ProtoProvider(
                    key: surface, name: names[providerIndex],
                    state: .current, summaryPercent: accounts[0].remaining,
                    summaryReset: accounts[0].resetText, accounts: accounts,
                    selectedAccountKey: accounts[0].key, updatedAgo: nil,
                    activityText: nil, errorText: nil))
        }
        return projection(
            scenario, providers: providers, statusRows: ["claude", "codex", "amp"],
            selectedProvider: "claude", selectedAccount: "claude-load-03")
    }

    static func stateFor(remaining: Int?) -> ProtoState {
        guard let remaining else { return .current }
        switch remaining {
        case 0: return .depleted
        case ...15: return .danger
        case ...30: return .warning
        default: return .current
        }
    }

    static func f13() -> ProtoProjection {
        ProtoProjection(
            scenario: "F13", providers: [], statusRows: [], isLoading: true,
            globalError: nil, chrome: ProtoChrome(), mutationScript: .standard,
            selectedProviderKey: nil, selectedAccountKey: nil)
    }

    static func f14() -> ProtoProjection {
        ProtoProjection(
            scenario: "F14", providers: [], statusRows: [], isLoading: false,
            globalError: "Usage presentation is unavailable", chrome: ProtoChrome(),
            mutationScript: .standard, selectedProviderKey: nil,
            selectedAccountKey: nil)
    }

    static func f19(
        scenario: String, localeID: String, direction: LayoutDirection,
        provider: String, account: String, plan: String, reset: String,
        error: String, refresh: String, openUsage: String
    ) -> ProtoProjection {
        var chrome = ProtoChrome()
        chrome.refreshTitle = refresh
        chrome.openUsageTitle = openUsage
        chrome.locale = Locale(identifier: localeID)
        chrome.layoutDirection = direction
        var providers = catalog()
        providers[0] = ProtoProvider(
            key: "codex", name: provider, state: .stale, summaryPercent: 57,
            summaryReset: reset,
            accounts: [
                ProtoAccount(
                    key: "codex-team", label: account, plan: plan, remaining: 57,
                    resetText: reset, state: .current,
                    windows: [
                        ProtoQuotaWindow(
                            stableID: "bucket:weekly", label: "Weekly", category: .longRange,
                            periodTag: "w",
                            display: "57% left · \(reset)", primaryValue: "57% left",
                            resetLabel: reset, meter: 57,
                            state: .current)
                    ])
            ],
            selectedAccountKey: "codex-team", updatedAgo: nil, activityText: nil,
            errorText: error)
        return projection(
            scenario, providers: providers, statusRows: ["claude", "amp", "codex"],
            chrome: chrome)
    }

    static func f22() -> ProtoProjection {
        projection(
            "F22",
            providers: [
                provider(
                    "minimax", "MiniMax", percent: 33, reset: nil,
                    accounts: [
                        ProtoAccount(
                            key: "minimax-default", label: "default", plan: "Pro",
                            remaining: 33, resetText: nil, state: .current,
                            windows: [
                                ProtoQuotaWindow(
                                    stableID: "bucket:monthly-credit-cap",
                                    label: "Monthly credit allowance", category: .longRange,
                                    periodTag: "mo",
                                    display: "$6 available of $20 cap · Resets Sep 1",
                                    primaryValue: "$6 available", resetLabel: "Resets Sep 1",
                                    supplementalValue: "Monthly cap · $6 / $20",
                                    meter: nil, state: .current)
                            ])
                    ])
            ],
            statusRows: ["minimax"], selectedProvider: "minimax",
            selectedAccount: "minimax-default")
    }

    /// Multi-account rich: Codex three accounts, Claude two — the canonical
    /// deduplicated account graph across plan tiers and states.
    static func f25() -> ProtoProjection {
        projection(
            "F25",
            providers: [
                codexProvider(
                    accounts: [codexPersonal, codexPlus, codexOrganization],
                    selected: "codex-personal"),
                provider(
                    "claude", "Anthropic / Claude", percent: 12, reset: "Resets in 1h",
                    accounts: [
                        claudePersonal,
                        ProtoAccount(
                            key: "claude-work", label: "work@example.test", plan: "Team",
                            remaining: 91, resetText: "Resets in 4d", state: .current,
                            windows: [
                                ProtoQuotaWindow(
                                    stableID: "bucket:weekly", label: "Weekly",
                                    category: .longRange, periodTag: "w",
                                    display: "91% left · Resets in 4d",
                                    primaryValue: "91% left", resetLabel: "Resets in 4d",
                                    meter: 91,
                                    state: .current, pace: "On pace"),
                                ProtoQuotaWindow(
                                    stableID: "bucket:session", label: "Session",
                                    category: .session,
                                    display: "Not started · Resets in 5h",
                                    primaryValue: "100% left", resetLabel: "Resets in 5h",
                                    meter: 100, state: .current, notStarted: true),
                            ]),
                    ],
                    selected: "claude-personal"),
            ],
            statusRows: ["claude", "codex"], selectedProvider: "codex",
            selectedAccount: "codex-personal")
    }

    /// needs_login: credential present but expired/revoked — re-auth required.
    static func f26() -> ProtoProjection {
        projection(
            "F26",
            providers: [
                provider(
                    "claude", "Anthropic / Claude", percent: nil, reset: nil,
                    accounts: [], state: .needsLogin,
                    updatedAgo: "Updated 2h ago",
                    error: "Claude sign-in expired — sign in again to resume quota updates")
            ],
            statusRows: ["claude"], selectedProvider: "claude")
    }

    /// needs_secret: no API key discovered anywhere for a key-only provider.
    static func f27() -> ProtoProjection {
        projection(
            "F27",
            providers: [
                provider(
                    "zai", "Z.AI / GLM", percent: nil, reset: nil,
                    accounts: [], state: .needsSecret,
                    error: "No Z.AI API key found — set ZAI_API_KEY to enable quota tracking")
            ],
            statusRows: ["zai"], selectedProvider: "zai")
    }

    /// unsupported: credential exists but exposes no quota surface
    /// (presence-only, e.g. an OpenAI API key without a ChatGPT subscription).
    static func f28() -> ProtoProjection {
        projection(
            "F28",
            providers: [
                provider(
                    "codex", "OpenAI / Codex", percent: nil, reset: nil,
                    accounts: [], state: .unsupported,
                    error: "OpenAI API-key subscription quota is unavailable")
            ],
            statusRows: ["codex"], selectedProvider: "codex")
    }

    /// Rate limited: provider 429 with a Retry-After deadline; last-good rows
    /// stay visible under the backoff marker.
    static func f29() -> ProtoProjection {
        projection(
            "F29",
            providers: [
                provider(
                    "grok", "xAI / Grok", percent: 72, reset: nil,
                    accounts: [catalogAccount("grok", remaining: 72, reset: nil)],
                    state: .rateLimited, updatedAgo: "Updated 6m ago",
                    error: "Grok billing endpoint rate limited · Retry in 12m")
            ],
            statusRows: ["grok"], selectedProvider: "grok",
            selectedAccount: "grok-default")
    }
}
