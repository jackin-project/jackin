// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "UnifiedAgentUsageProto",
    platforms: [.macOS(.v26)],
    targets: [
        .executableTarget(
            name: "UnifiedAgentUsageProto",
            resources: [.process("Resources")]
        ),
        .testTarget(
            name: "UnifiedAgentUsageProtoTests",
            dependencies: ["UnifiedAgentUsageProto"]
        ),
    ]
)
