//
//  main.swift
//  AgentHarbor
//
//  Main entry point for the AgentHarbor macOS application.
//

import Cocoa

let delegate = AppDelegate()
NSApplication.shared.delegate = delegate
_ = NSApplicationMain(CommandLine.argc, CommandLine.unsafeArgv)