import Foundation

@MainActor
final class StatusBarModel: ObservableObject {
    @Published private(set) var daemon: DaemonHealth = .checking
    @Published private(set) var workspaces: [DesktopWorkspace] = []
    @Published private(set) var sessions: [DesktopSession] = []
    @Published private(set) var pullRequests: [GitHubPullRequest] = []
    @Published private(set) var accountProviders: [AccountProviderStatus] = []
    @Published private(set) var accountFetchedAtEpochSeconds: UInt64?
    @Published private(set) var accountStatusCacheHit = false
    @Published private(set) var pullRequestError: String?
    @Published private(set) var accountStatusError: String?
    @Published private(set) var openError: String?
    @Published private(set) var eventSubscription: EventSubscriptionResponse?
    @Published private(set) var isRefreshing = false

    private let client: DaemonClient

    init(client: DaemonClient = DaemonClient()) {
        self.client = client
    }

    var symbolName: String {
        switch daemon {
        case .checking:
            "circle.dotted"
        case .connected:
            sessions.isEmpty ? "bolt.horizontal.circle" : "bolt.horizontal.circle.fill"
        case .disconnected:
            "exclamationmark.circle"
        }
    }

    func refresh() async {
        isRefreshing = true
        defer { isRefreshing = false }

        do {
            let hello = try await client.hello()
            async let workspaceList = client.workspaces()
            async let sessionList = client.sessions()
            async let eventRoutes = client.eventSubscription()
            async let accountStatus = client.accountStatus()
            workspaces = try await workspaceList.workspaces
            sessions = try await sessionList.sessions
            eventSubscription = try await eventRoutes
            do {
                let status = try await accountStatus
                accountProviders = status.providers
                accountFetchedAtEpochSeconds = status.fetchedAtEpochSeconds
                accountStatusCacheHit = status.cacheHit
                accountStatusError = nil
            } catch {
                accountProviders = []
                accountFetchedAtEpochSeconds = nil
                accountStatusCacheHit = false
                accountStatusError = error.localizedDescription
            }
            daemon = .connected(hello)
        } catch {
            workspaces = []
            sessions = []
            pullRequests = []
            accountProviders = []
            accountFetchedAtEpochSeconds = nil
            accountStatusCacheHit = false
            pullRequestError = nil
            accountStatusError = nil
            self.eventSubscription = nil
            daemon = .disconnected(error.localizedDescription)
            return
        }

        do {
            pullRequests = try await client.myOpenPullRequests(limit: 10).pullRequests
            pullRequestError = nil
        } catch {
            pullRequests = []
            pullRequestError = error.localizedDescription
        }
    }

    func runningCount(for workspace: DesktopWorkspace) -> Int {
        sessions.filter { $0.workspace == workspace.name }.count
    }

    var readyForReviewPullRequests: [GitHubPullRequest] {
        pullRequests.filter { !$0.isDraft }
    }

    var projectSummaries: [DesktopProjectSummary] {
        let repositories = Set(pullRequests.map(\.repository) + sessions.compactMap(\.repository))
        return repositories.sorted().map { repository in
            let projectPullRequests = pullRequests.filter { $0.repository == repository }
            let projectSessions = sessions.filter { $0.repository == repository }
            let workspaceNames = Set(projectSessions.compactMap(\.workspace)).sorted()
            return DesktopProjectSummary(
                repository: repository,
                pullRequests: projectPullRequests,
                runningSessions: projectSessions,
                workspaceNames: workspaceNames
            )
        }
    }

    func openPullRequest(_ pullRequest: GitHubPullRequest) async {
        await openURL(pullRequest.url)
    }

    func openPullRequestInDiffsHub(_ pullRequest: GitHubPullRequest) async {
        await openURL(pullRequest.diffshubUrl)
    }

    func openRepository(_ repository: String) async {
        await openURL("https://github.com/\(repository)")
    }

    func openRepositoryPullRequests(_ repository: String) async {
        await openURL("https://github.com/\(repository)/pulls")
    }

    private func openURL(_ url: String) async {
        do {
            _ = try await client.openBrowser(url)
            openError = nil
        } catch {
            openError = error.localizedDescription
        }
    }

    func launchWorkspace(_ workspace: DesktopWorkspace) async {
        do {
            _ = try await client.launchWorkspace(workspace)
            openError = nil
        } catch {
            openError = error.localizedDescription
        }
    }

    func openSession(_ session: DesktopSession) async {
        do {
            _ = try await client.openGhosttyHardline(target: session.containerName)
            openError = nil
        } catch {
            openError = error.localizedDescription
        }
    }

    func refreshAccountStatus() async {
        do {
            let status = try await client.accountStatus(refresh: true)
            accountProviders = status.providers
            accountFetchedAtEpochSeconds = status.fetchedAtEpochSeconds
            accountStatusCacheHit = status.cacheHit
            accountStatusError = nil
        } catch {
            accountProviders = []
            accountFetchedAtEpochSeconds = nil
            accountStatusCacheHit = false
            accountStatusError = error.localizedDescription
        }
    }
}

enum DaemonHealth: Equatable {
    case checking
    case connected(DaemonHello)
    case disconnected(String)
}

struct DesktopProjectSummary: Identifiable, Equatable {
    var id: String { repository }

    let repository: String
    let pullRequests: [GitHubPullRequest]
    let runningSessions: [DesktopSession]
    let workspaceNames: [String]

    var activePullRequests: [GitHubPullRequest] {
        pullRequests.filter { !$0.isDraft }
    }
}
