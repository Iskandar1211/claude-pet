// swift-tools-version: 6.0
import PackageDescription

// Собирается без Xcode, на Command Line Tools. `.app` собирает scripts/build_app.sh.
let package = Package(
    name: "ClaudePet",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(name: "ClaudePet")
    ]
)
