import Darwin
import Foundation

struct DaemonClient {
    var socketPath: String
    var protocolVersion: Int

    init(socketPath: String? = nil, protocolVersion: Int = 2) {
        self.socketPath = socketPath ?? "\(NSHomeDirectory())/.jackin/run/jackin-daemon.sock"
        self.protocolVersion = protocolVersion
    }

    func hello() async throws -> DaemonHello {
        try await request(EmptyDaemonRequest(type: "daemon/hello", protocolVersion: protocolVersion), as: DaemonHello.self)
    }

    func workspaces() async throws -> WorkspaceList {
        try await request(EmptyDaemonRequest(type: "workspace/list", protocolVersion: protocolVersion), as: WorkspaceList.self)
    }

    func sessions() async throws -> SessionList {
        try await request(EmptyDaemonRequest(type: "session/list", protocolVersion: protocolVersion), as: SessionList.self)
    }

    func myOpenPullRequests(limit: Int = 10) async throws -> GitHubPullRequestList {
        try await request(
            GitHubPullRequestRequest(type: "github/my_open_prs", protocolVersion: protocolVersion, limit: limit),
            as: GitHubPullRequestList.self
        )
    }

    func openBrowser(_ url: String) async throws -> DesktopActionResponse {
        try await request(
            DesktopOpenRequest(type: "desktop/open", protocolVersion: protocolVersion, kind: "browser", target: url),
            as: DesktopActionResponse.self
        )
    }

    func launchWorkspace(_ workspace: DesktopWorkspace) async throws -> DesktopActionResponse {
        try await request(
            WorkspaceLaunchRequest(
                type: "workspace/launch",
                protocolVersion: protocolVersion,
                workspace: workspace.name,
                role: workspace.lastRole ?? workspace.defaultRole
            ),
            as: DesktopActionResponse.self
        )
    }

    func openGhosttyHardline(target: String) async throws -> DesktopActionResponse {
        try await request(
            DesktopOpenRequest(type: "desktop/open", protocolVersion: protocolVersion, kind: "ghostty_hardline", target: target),
            as: DesktopActionResponse.self
        )
    }

    private func request<Request: Encodable & Sendable, Response: Decodable>(
        _ request: Request,
        as responseType: Response.Type
    ) async throws -> Response {
        let socketPath = socketPath
        try await Task.detached {
            let data = try JSONEncoder.daemon.encode(request)
            let line = String(decoding: data, as: UTF8.self) + "\n"
            let response = try sendJSONLine(line, to: socketPath)
            return try JSONDecoder.daemon.decode(responseType, from: Data(response.utf8))
        }.value
    }
}

private struct EmptyDaemonRequest: Encodable, Sendable {
    let type: String
    let protocolVersion: Int

    enum CodingKeys: String, CodingKey {
        case type
        case protocolVersion = "protocol"
    }
}

private struct GitHubPullRequestRequest: Encodable, Sendable {
    let type: String
    let protocolVersion: Int
    let limit: Int

    enum CodingKeys: String, CodingKey {
        case type
        case protocolVersion = "protocol"
        case limit
    }
}

private struct DesktopOpenRequest: Encodable, Sendable {
    let type: String
    let protocolVersion: Int
    let kind: String
    let target: String

    enum CodingKeys: String, CodingKey {
        case type
        case protocolVersion = "protocol"
        case kind
        case target
    }
}

private struct WorkspaceLaunchRequest: Encodable, Sendable {
    let type: String
    let protocolVersion: Int
    let workspace: String
    let role: String?

    enum CodingKeys: String, CodingKey {
        case type
        case protocolVersion = "protocol"
        case workspace
        case role
    }
}

struct DaemonHello: Decodable, Equatable {
    let type: String
    let version: String
    let protocolVersion: Int
    let minProtocol: Int
    let maxProtocol: Int
    let capabilities: [DaemonCapability]

    enum CodingKeys: String, CodingKey {
        case type
        case version
        case protocolVersion = "protocol"
        case minProtocol = "min_protocol"
        case maxProtocol = "max_protocol"
        case capabilities
    }
}

struct DaemonCapability: Decodable, Equatable {
    let method: String
    let sinceProtocol: Int

    enum CodingKeys: String, CodingKey {
        case method
        case sinceProtocol = "since_protocol"
    }
}

struct WorkspaceList: Decodable, Equatable {
    let type: String
    let workspaces: [DesktopWorkspace]
}

struct DesktopWorkspace: Decodable, Identifiable, Equatable {
    var id: String { name }

    let name: String
    let workdir: String
    let mountCount: Int
    let defaultRole: String?
    let lastRole: String?
    let defaultAgent: String?
    let keepAwake: Bool

    enum CodingKeys: String, CodingKey {
        case name
        case workdir
        case mountCount = "mount_count"
        case defaultRole = "default_role"
        case lastRole = "last_role"
        case defaultAgent = "default_agent"
        case keepAwake = "keep_awake"
    }
}

struct SessionList: Decodable, Equatable {
    let type: String
    let sessions: [DesktopSession]
}

struct DesktopSession: Decodable, Identifiable, Equatable {
    var id: String { containerName }

    let containerName: String
    let status: String
    let displayName: String
    let workspace: String?
    let role: String?
    let agent: String?

    enum CodingKeys: String, CodingKey {
        case containerName = "container_name"
        case status
        case displayName = "display_name"
        case workspace
        case role
        case agent
    }
}

struct GitHubPullRequestList: Decodable, Equatable {
    let type: String
    let pullRequests: [GitHubPullRequest]

    enum CodingKeys: String, CodingKey {
        case type
        case pullRequests = "pull_requests"
    }
}

struct GitHubPullRequest: Decodable, Identifiable, Equatable {
    var id: String { url }

    let number: Int
    let title: String
    let url: String
    let author: String
    let repository: String
    let createdAt: String
    let updatedAt: String
    let isDraft: Bool

    enum CodingKeys: String, CodingKey {
        case number
        case title
        case url
        case author
        case repository
        case createdAt = "created_at"
        case updatedAt = "updated_at"
        case isDraft = "is_draft"
    }
}

struct DesktopActionResponse: Decodable, Equatable {
    let action: String
    let command: [String]
}

enum DaemonClientError: LocalizedError {
    case socketPathTooLong(String)
    case connectFailed(String)
    case emptyResponse

    var errorDescription: String? {
        switch self {
        case .socketPathTooLong(let path):
            "Daemon socket path is too long: \(path)"
        case .connectFailed(let path):
            "Could not connect to daemon socket at \(path)"
        case .emptyResponse:
            "Daemon returned an empty response"
        }
    }
}

private extension JSONDecoder {
    static var daemon: JSONDecoder {
        JSONDecoder()
    }
}

private extension JSONEncoder {
    static var daemon: JSONEncoder {
        JSONEncoder()
    }
}

private func sendJSONLine(_ line: String, to socketPath: String) throws -> String {
    let fd = socket(AF_UNIX, SOCK_STREAM, 0)
    guard fd >= 0 else {
        throw DaemonClientError.connectFailed(socketPath)
    }
    defer {
        close(fd)
    }

    var address = sockaddr_un()
    address.sun_family = sa_family_t(AF_UNIX)

    let encodedPath = Array(socketPath.utf8CString)
    let capacity = MemoryLayout.size(ofValue: address.sun_path)
    guard encodedPath.count <= capacity else {
        throw DaemonClientError.socketPathTooLong(socketPath)
    }

    withUnsafeMutableBytes(of: &address.sun_path) { buffer in
        buffer.copyBytes(from: encodedPath)
    }

    let length = socklen_t(MemoryLayout<sa_family_t>.size + encodedPath.count)
    let connected = withUnsafePointer(to: &address) { pointer in
        pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { socketAddress in
            Darwin.connect(fd, socketAddress, length)
        }
    }
    guard connected == 0 else {
        throw DaemonClientError.connectFailed(socketPath)
    }

    let payload = Data(line.utf8)
    try payload.withUnsafeBytes { buffer in
        guard let base = buffer.baseAddress else { return }
        var sent = 0
        while sent < payload.count {
            let count = Darwin.write(fd, base.advanced(by: sent), payload.count - sent)
            guard count > 0 else {
                throw DaemonClientError.connectFailed(socketPath)
            }
            sent += count
        }
    }

    var bytes = [UInt8]()
    var byte = UInt8(0)
    while Darwin.read(fd, &byte, 1) == 1 {
        if byte == 10 {
            break
        }
        bytes.append(byte)
    }

    guard !bytes.isEmpty else {
        throw DaemonClientError.emptyResponse
    }
    return String(decoding: bytes, as: UTF8.self)
}
