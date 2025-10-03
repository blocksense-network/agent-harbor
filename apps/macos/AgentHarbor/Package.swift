// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "AgentHarbor",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(
            name: "AgentHarbor",
            targets: ["AgentHarbor"]
        )
    ],
    dependencies: [],
    targets: [
        .executableTarget(
            name: "AgentHarbor",
            dependencies: [],
            path: "AgentHarbor",
            exclude: ["Info.plist"],
            resources: [
                .copy("../../../adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension.appex")
            ]
        )
    ]
)
