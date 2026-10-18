// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

public struct QIFixtureTransitions: Sendable, Equatable {
    public let refreshing: QIFixtureProjection?
    public let accountSelections: [String: QIFixtureProjection]

    public init(
        refreshing: QIFixtureProjection? = nil,
        accountSelections: [String: QIFixtureProjection] = [:]
    ) {
        self.refreshing = refreshing
        self.accountSelections = accountSelections
    }
}

public struct QIFixtureSelection: Sendable, Equatable {
    public let popover: String?
    public let usage: String?
    public let account: String?

    public init(popover: String?, usage: String?, account: String? = nil) {
        self.popover = popover
        self.usage = usage
        self.account = account
    }
}

public struct QIFixtureStatus: Sendable, Equatable {
    public let nextRefreshLabel: String
    public let isLoading: Bool
    public let isRefreshing: Bool
    public let lastError: String?

    public init(
        nextRefreshLabel: String = "next update 4m",
        isLoading: Bool = false,
        isRefreshing: Bool = false,
        lastError: String? = nil
    ) {
        self.nextRefreshLabel = nextRefreshLabel
        self.isLoading = isLoading
        self.isRefreshing = isRefreshing
        self.lastError = lastError
    }
}

public struct QIFixtureInput: Sendable, Equatable {
    public let projection: QIFixtureProjection
    public let transitions: QIFixtureTransitions
    public let selection: QIFixtureSelection
    public let status: QIFixtureStatus

    public init(
        projection: QIFixtureProjection,
        transitions: QIFixtureTransitions = QIFixtureTransitions(),
        selection: QIFixtureSelection,
        status: QIFixtureStatus = QIFixtureStatus()
    ) {
        self.projection = projection
        self.transitions = transitions
        self.selection = selection
        self.status = status
    }

    public func replacing(
        projection: QIFixtureProjection? = nil,
        transitions: QIFixtureTransitions? = nil,
        selection: QIFixtureSelection? = nil,
        status: QIFixtureStatus? = nil
    ) -> QIFixtureInput {
        QIFixtureInput(
            projection: projection ?? self.projection,
            transitions: transitions ?? self.transitions,
            selection: selection ?? self.selection,
            status: status ?? self.status
        )
    }
}
