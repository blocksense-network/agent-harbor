//
//  MainViewController.swift
//  AgentsWorkflow
//

import Cocoa

// Import SystemExtensions only if available (macOS 10.15+)
#if canImport(SystemExtensions)
import SystemExtensions
#endif

class MainViewController: NSViewController {

    private let statusLabel = NSTextField(labelWithString: "Agents Workflow")
    private let extensionStatusLabel = NSTextField(labelWithString: "Extension Status: Checking...")
    private let infoTextView = NSTextView()
    private let approveButton = NSButton(title: "Open Settings to Approve", target: nil, action: nil)
    private let retryButton = NSButton(title: "Retry Activation", target: nil, action: nil)

    override func loadView() {
        let view = NSView(frame: NSRect(x: 0, y: 0, width: 800, height: 600))
        self.view = view
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        setupUI()
        checkExtensionStatus()
        observeStatusNotifications()
    }

    private func setupUI() {
        statusLabel.font = NSFont.systemFont(ofSize: 24, weight: .bold)
        statusLabel.alignment = .center
        statusLabel.translatesAutoresizingMaskIntoConstraints = false

        extensionStatusLabel.font = NSFont.systemFont(ofSize: 14)
        extensionStatusLabel.alignment = .center
        extensionStatusLabel.translatesAutoresizingMaskIntoConstraints = false

        infoTextView.isEditable = false
        infoTextView.backgroundColor = .clear
        infoTextView.string = """
        Agents Workflow macOS Application

        This application hosts system extensions for the Agents Workflow platform,
        including filesystem extensions for AgentFS.
        """
        infoTextView.translatesAutoresizingMaskIntoConstraints = false

        view.addSubview(statusLabel)
        view.addSubview(extensionStatusLabel)
        view.addSubview(infoTextView)
        view.addSubview(approveButton)
        view.addSubview(retryButton)

        NSLayoutConstraint.activate([
            statusLabel.topAnchor.constraint(equalTo: view.topAnchor, constant: 50),
            statusLabel.centerXAnchor.constraint(equalTo: view.centerXAnchor),

            extensionStatusLabel.topAnchor.constraint(equalTo: statusLabel.bottomAnchor, constant: 20),
            extensionStatusLabel.centerXAnchor.constraint(equalTo: view.centerXAnchor),

            infoTextView.topAnchor.constraint(equalTo: extensionStatusLabel.bottomAnchor, constant: 40),
            infoTextView.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 50),
            infoTextView.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -50),
            infoTextView.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -50)
        ])

        approveButton.target = self
        approveButton.action = #selector(openSettingsToFSKitPane)
        approveButton.translatesAutoresizingMaskIntoConstraints = false
        approveButton.isHidden = true

        retryButton.target = self
        retryButton.action = #selector(requestActivation)
        retryButton.translatesAutoresizingMaskIntoConstraints = false
        retryButton.isHidden = true

        NSLayoutConstraint.activate([
            approveButton.topAnchor.constraint(equalTo: extensionStatusLabel.bottomAnchor, constant: 8),
            approveButton.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            retryButton.topAnchor.constraint(equalTo: approveButton.bottomAnchor, constant: 8),
            retryButton.centerXAnchor.constraint(equalTo: view.centerXAnchor)
        ])
    }

    private func checkExtensionStatus() {
        updateExtensionStatus()

        // Set up periodic status checking
        Timer.scheduledTimer(withTimeInterval: 5.0, repeats: true) { [weak self] _ in
            self?.updateExtensionStatus()
        }
    }

    private func updateExtensionStatus() {
        // Check if the extension bundle exists in the app
        if let extensionURL = Bundle.main.builtInPlugInsURL?.appendingPathComponent("AgentFSKitExtension.appex"),
           FileManager.default.fileExists(atPath: extensionURL.path) {

            // Extension bundle exists - it will be automatically registered by the system
            DispatchQueue.main.async {
                self.extensionStatusLabel.stringValue = "Extension Status: Available"
                self.updateInfoText(withError: "Extension bundle found. If disabled, click to approve and enable.")
                self.approveButton.isHidden = false
                self.retryButton.isHidden = false
            }
        } else {
            DispatchQueue.main.async {
                self.extensionStatusLabel.stringValue = "Extension Status: Not Found"
                self.updateInfoText(withError: "Extension bundle not found in app")
                self.approveButton.isHidden = true
                self.retryButton.isHidden = true
            }
        }
    }


    private func updateInfoText(withError error: String) {
        infoTextView.string = """
        Agents Workflow macOS Application

        This application hosts system extensions for the Agents Workflow platform,
        including filesystem extensions for AgentFS.

        Status
        - \(error)
        """
    }

    private func observeStatusNotifications() {
        NotificationCenter.default.addObserver(forName: .awSystemExtensionNeedsUserApproval, object: nil, queue: .main) { [weak self] _ in
            self?.approveButton.isHidden = false
        }
        NotificationCenter.default.addObserver(forName: .awSystemExtensionStatusChanged, object: nil, queue: .main) { [weak self] note in
            if let status = note.userInfo?["status"] as? String {
                self?.extensionStatusLabel.stringValue = "Extension Status: \(status)"
                self?.approveButton.isHidden = (status == "Enabled")
            }
        }
    }

    @objc private func openSettingsToFSKitPane() {
        // macOS 15 File System Extensions pane deep link
        // x-apple.systempreferences:com.apple.ExtensionsPreferences?extensionPointIdentifier=com.apple.fskit.fsmodule
        let urlString = "x-apple.systempreferences:com.apple.ExtensionsPreferences?extensionPointIdentifier=com.apple.fskit.fsmodule"
        if let url = URL(string: urlString) {
            NSWorkspace.shared.open(url)
        } else {
            NSSound.beep()
        }
    }

    @objc private func requestActivation() {
        NotificationCenter.default.post(name: .awRequestSystemExtensionActivation, object: nil)
    }
}