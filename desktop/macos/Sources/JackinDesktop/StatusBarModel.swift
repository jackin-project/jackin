import Foundation

@MainActor
final class StatusBarModel: ObservableObject {
    @Published private(set) var daemon: DaemonHealth = .checking
    @Published private(set) var workspaces: [DesktopWorkspace] = []
    @Published private(set) var sessions: [DesktopSession] = []
    @Published private(set) var pullRequests: [GitHubPullRequest] = []
    @Published private(set) var pullRequestError: String?
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
            workspaces = try await workspaceList.workspaces
            sessions = try await sessionList.sessions
            eventSubscription = try await eventRoutes
            daemon = .connected(hello)
        } catch {
            workspaces = []
            sessions = []
            pullRequests = []
            pullRequestError = nil
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

    func openPullRequest(_ pullRequest: GitHubPullRequest) async {
        do {
            _ = try await client.openBrowser(pullRequest.url)
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
}

enum DaemonHealth: Equatable {
    case checking
    case connected(DaemonHello)
    case disconnected(String)
}
