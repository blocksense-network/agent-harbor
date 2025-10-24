//
//  AppDelegate.swift
//  AgentHarbor
//

import Cocoa

// Import SystemExtensions only if available (macOS 10.15+)
#if canImport(SystemExtensions)
import SystemExtensions
#endif

class AppDelegate: NSObject, NSApplicationDelegate {

    var window: NSWindow?
    private let fsKitExtensionIdentifier = "com.AgentHarbor.AgentFSKitExtension"

    #if canImport(SystemExtensions)
    @available(macOS 10.15, *)
    private var systemExtensionRequest: OSSystemExtensionRequest?
    #endif

    func applicationDidFinishLaunching(_ aNotification: Notification) {
        createMainWindow()
        registerExtensions()
        observeActivationRequests()
        submitSystemExtensionActivationRequest(reason: "app launch")
        print("AgentHarbor application started successfully")
    }

    func applicationWillTerminate(_ aNotification: Notification) {
        print("AgentHarbor application terminating")
    }

    private func createMainWindow() {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 800, height: 600),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )

        window.center()
        window.title = "Agent Harbor"
        window.makeKeyAndOrderFront(nil)

        let viewController = MainViewController()
        window.contentViewController = viewController

        self.window = window
    }

    private func registerExtensions() {
        print("Registering embedded system extensions...")

        // Find the extension bundle
        guard let extensionBundleURL = Bundle.main.builtInPlugInsURL?.appendingPathComponent("AgentFSKitExtension.appex") else {
            print("Error: Could not find AgentFSKitExtension bundle")
            return
        }

        print("Found extension bundle at: \(extensionBundleURL.path)")

        // System extensions are automatically registered when the app launches
        // The system will detect the embedded extension and request user approval if needed
        #if canImport(SystemExtensions)
        if #available(macOS 13.0, *) {
            print("macOS 13.0+ detected - system will handle extension registration automatically")
        } else if #available(macOS 10.15, *) {
            print("macOS 10.15-12.x detected - extension requires manual approval in System Settings")
        }
        #else
        print("SystemExtensions framework not available - extension may require manual approval")
        #endif

        print("Extension registration complete - system will handle approval workflow")
    }

    private func observeActivationRequests() {
        NotificationCenter.default.addObserver(forName: .awRequestSystemExtensionActivation, object: nil, queue: .main) { [weak self] _ in
            self?.submitSystemExtensionActivationRequest(reason: "user request")
        }
    }

    private func submitSystemExtensionActivationRequest(reason: String) {
        #if canImport(SystemExtensions)
        if #available(macOS 10.15, *) {
            let identifier = fsKitExtensionIdentifier
            let queue = DispatchQueue.main
            let request = OSSystemExtensionRequest.activationRequest(forExtensionWithIdentifier: identifier, queue: queue)
            request.delegate = self
            self.systemExtensionRequest = request
            print("Submitting system extension activation request for \(identifier) [reason=\(reason)]")
            OSSystemExtensionManager.shared.submitRequest(request)
        } else {
            print("SystemExtensions not available on this macOS version; cannot submit activation request")
        }
        #else
        print("SystemExtensions framework not available; cannot submit activation request")
        #endif
    }
}

#if canImport(SystemExtensions)
@available(macOS 10.15, *)
extension AppDelegate: OSSystemExtensionRequestDelegate {
    func requestNeedsUserApproval(_ request: OSSystemExtensionRequest) {
        print("System extension request needs user approval")
        NotificationCenter.default.post(name: .awSystemExtensionNeedsUserApproval, object: nil)
        NotificationCenter.default.post(name: .awSystemExtensionStatusChanged, object: nil, userInfo: ["status": "Approval required in System Settings"])
    }

    func request(_ request: OSSystemExtensionRequest, didFinishWithResult result: OSSystemExtensionRequest.Result) {
        switch result {
        case .completed:
            print("System extension activation completed successfully")
            NotificationCenter.default.post(name: .awSystemExtensionStatusChanged, object: nil, userInfo: ["status": "Enabled"])
        case .willCompleteAfterReboot:
            print("System extension activation will complete after reboot")
            NotificationCenter.default.post(name: .awSystemExtensionStatusChanged, object: nil, userInfo: ["status": "Will complete after reboot"])
        @unknown default:
            print("System extension activation finished with unknown result: \(result.rawValue)")
            NotificationCenter.default.post(name: .awSystemExtensionStatusChanged, object: nil, userInfo: ["status": "Unknown result"])
        }
    }

    func request(_ request: OSSystemExtensionRequest, didFailWithError error: Error) {
        print("System extension activation failed: \(error.localizedDescription)")
        NotificationCenter.default.post(name: .awSystemExtensionStatusChanged, object: nil, userInfo: ["status": "Error: \(error.localizedDescription)"])
    }

    func request(_ request: OSSystemExtensionRequest, actionForReplacingExtension existing: OSSystemExtensionProperties, withExtension replacement: OSSystemExtensionProperties) -> OSSystemExtensionRequest.ReplacementAction {
        print("System extension replacement requested: existing=\(existing.bundleIdentifier), replacement=\(replacement.bundleIdentifier)")
        return .replace
    }
}
#endif

extension Notification.Name {
    static let awRequestSystemExtensionActivation = Notification.Name("AWRequestSystemExtensionActivation")
    static let awSystemExtensionNeedsUserApproval = Notification.Name("AWSystemExtensionNeedsUserApproval")
    static let awSystemExtensionStatusChanged = Notification.Name("AWSystemExtensionStatusChanged")
}