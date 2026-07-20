// swift-tools-version: 6.0
import PackageDescription

let packageDir = Context.packageDirectory
let cargoReleaseLibDir = packageDir + "/../target/release"

let package = Package(
    name: "JackinUsageMenuBar",
    platforms: [
        .macOS(.v14),
    ],
    products: [
        .library(name: "JackinUsageBridge", targets: ["JackinUsageBridge"]),
        .executable(name: "JackinUsageMenuBar", targets: ["JackinUsageMenuBar"]),
    ],
    targets: [
        .systemLibrary(
            name: "jackin_usage_ffiFFI",
            path: "Generated"
        ),
        .target(
            name: "JackinUsageBridge",
            dependencies: ["jackin_usage_ffiFFI"],
            path: "Sources/JackinUsageBridge",
            linkerSettings: [
                .linkedLibrary("jackin_usage_ffi"),
                .unsafeFlags([
                    "-L\(cargoReleaseLibDir)",
                ]),
            ]
        ),
        .executableTarget(
            name: "JackinUsageMenuBar",
            dependencies: ["JackinUsageBridge"],
            path: "Sources/JackinUsageMenuBar"
        ),
        .testTarget(
            name: "JackinUsageBridgeTests",
            dependencies: ["JackinUsageBridge"],
            path: "Tests/JackinUsageBridgeTests"
        ),
    ]
)
