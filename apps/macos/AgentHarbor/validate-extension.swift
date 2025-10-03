#!/usr/bin/env swift
//
//  validate-extension.swift
//  Extension Validation Tool
//
//  Validates that a built AgentHarbor app contains a properly configured extension
//  Usage: swift validate-extension.swift /path/to/AgentHarbor.app
//

import Foundation

func validateExtension(at appPath: String) -> Int {
    print("AgentHarbor Extension Validator")
    print("===================================")

    let extensionIdentifier = "com.AgentHarbor.AgentFSKitExtension"
    var exitCode = 0

    // 1. Check if extension bundle exists
    let extensionPath = "\(appPath)/Contents/PlugIns/AgentFSKitExtension.appex"
    guard FileManager.default.fileExists(atPath: extensionPath) else {
        print("❌ Extension bundle not found at: \(extensionPath)")
        return 1
    }

    print("✅ Extension bundle found at: \(extensionPath)")

    // 2. Validate extension bundle structure
    let bundleURL = URL(fileURLWithPath: extensionPath)
    guard let extensionBundle = Bundle(url: bundleURL) else {
        print("❌ Cannot load extension bundle")
        return 1
    }

    print("✅ Extension bundle loads successfully")

    // 3. Check Info.plist contents
    guard let infoDict = extensionBundle.infoDictionary else {
        print("❌ Extension Info.plist cannot be read")
        return 1
    }

    print("✅ Extension Info.plist loaded")

    // 4. Validate required Info.plist keys for system extensions
    let requiredKeys = [
        "CFBundleIdentifier",
        "CFBundleName",
        "CFBundleVersion",
        "CFBundleShortVersionString",
        "NSExtension"
    ]

    for key in requiredKeys {
        guard infoDict[key] != nil else {
            print("❌ Required Info.plist key missing: \(key)")
            exitCode = 1
            continue
        }
        print("✅ Info.plist key present: \(key)")
    }

    // 5. Validate NSExtension configuration (modern system extensions)
    if let nsExtension = infoDict["NSExtension"] as? [String: Any] {
        if let pointIdentifier = nsExtension["NSExtensionPointIdentifier"] as? String {
            print("✅ Extension point: \(pointIdentifier)")
            if pointIdentifier != "com.apple.filesystems" {
                print("⚠️  Warning: Expected filesystem extension point")
            }
        } else {
            print("❌ NSExtensionPointIdentifier missing")
            exitCode = 1
        }
    } else {
        print("❌ NSExtension configuration missing")
        exitCode = 1
    }

    // 6. Check executable
    if let executableURL = extensionBundle.executableURL {
        print("✅ Extension executable URL: \(executableURL.path)")

        // 7. Check if executable is readable and has proper size
        do {
            let attributes = try FileManager.default.attributesOfItem(atPath: executableURL.path)
            if let fileSize = attributes[.size] as? NSNumber {
                print("✅ Extension executable size: \(fileSize.intValue) bytes")
                if fileSize.intValue < 1000 {
                    print("⚠️  Warning: Extension executable seems unusually small")
                }
            }
        } catch {
            print("❌ Cannot read extension executable attributes: \(error)")
            exitCode = 1
        }
    } else {
        print("❌ Extension executable URL not found")
        exitCode = 1
    }

    // 8. Check bundle identifier matches expected
    if let bundleId = infoDict["CFBundleIdentifier"] as? String {
        print("✅ Extension bundle ID: \(bundleId)")
        if bundleId != extensionIdentifier {
            print("⚠️  Warning: Bundle ID mismatch - expected: \(extensionIdentifier), got: \(bundleId)")
        }
    }

    // 9. Basic app validation
    let appURL = URL(fileURLWithPath: appPath)
    print("✅ App bundle URL: \(appURL.path)")

    // Check if app bundle is valid
    guard let appBundle = Bundle(url: appURL) else {
        print("❌ Cannot load app bundle")
        return 1
    }

    if let version = appBundle.infoDictionary?["CFBundleShortVersionString"] as? String {
        print("✅ App version: \(version)")
    }

    print("\nValidation complete. Exit code: \(exitCode)")
    return exitCode
}

// Main execution
if CommandLine.arguments.count != 2 {
    print("Usage: \(CommandLine.arguments[0]) <path-to-AgentHarbor.app>")
    print("Example: \(CommandLine.arguments[0]) /path/to/build/AgentHarbor.app")
    exit(1)
}

let appPath = CommandLine.arguments[1]
let result = validateExtension(at: appPath)
exit(Int32(result))
