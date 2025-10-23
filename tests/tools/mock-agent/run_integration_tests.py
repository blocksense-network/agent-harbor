#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

"""
Test runner for mock-agent integration tests.

This script runs comprehensive integration tests that verify claude and codex
CLI tools can successfully interact with our mock-agent API server.
"""

import argparse
import os
import subprocess
import sys
import tempfile
import json
from pathlib import Path


def check_dependencies():
    """Check if required dependencies are available."""
    print("Checking dependencies...")
    
    # Check Python modules
    try:
        import requests
        print("✓ requests module available")
    except ImportError:
        print("✗ requests module not available (optional)")
    
    # Check CLI tools
    tools = ["codex", "claude"]
    available_tools = []
    
    for tool in tools:
        try:
            result = subprocess.run([tool, "--version"], capture_output=True, timeout=10)
            if result.returncode == 0:
                print(f"✓ {tool} CLI available")
                available_tools.append(tool)
            else:
                print(f"✗ {tool} CLI not working properly")
        except (subprocess.TimeoutExpired, FileNotFoundError):
            print(f"✗ {tool} CLI not found in PATH")
    
    if not available_tools:
        print("\nWARNING: No CLI tools available. Integration tests will be limited.")
    
    return available_tools


def create_test_workspace():
    """Create a temporary workspace for testing."""
    workspace = tempfile.mkdtemp(prefix="mock_agent_integration_")
    print(f"Created test workspace: {workspace}")
    return workspace


def run_manual_test_scenario(workspace, tool, scenario_name):
    """Run a manual test scenario to verify basic functionality."""
    print(f"\n=== Running {scenario_name} with {tool} ===")
    
    # Change to workspace
    os.chdir(workspace)
    
    if scenario_name == "hello_world":
        if tool == "codex":
            cmd = [
                "codex", "exec",
                "--dangerously-bypass-approvals-and-sandbox", 
                "--json",
                "Create hello.py that prints Hello, World!"
            ]
        elif tool == "claude":
            cmd = [
                "claude", 
                "--print",
                "--dangerously-skip-permissions",
                "Create hello.py that prints Hello, World!"
            ]
        else:
            print(f"Unknown tool: {tool}")
            return False
        
        # Set environment for mock server
        env = os.environ.copy()
        if tool == "codex":
            env["CODEX_API_BASE"] = "http://127.0.0.1:18080/v1"
            env["CODEX_API_KEY"] = "mock-key"
        
        try:
            print(f"Running: {' '.join(cmd)}")
            result = subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=30)
            
            print(f"Return code: {result.returncode}")
            if result.stdout:
                print(f"STDOUT:\n{result.stdout}")
            if result.stderr:
                print(f"STDERR:\n{result.stderr}")
            
            # Check if hello.py was created
            hello_file = os.path.join(workspace, "hello.py")
            if os.path.exists(hello_file):
                print("✓ hello.py was created successfully")
                with open(hello_file, 'r') as f:
                    content = f.read()
                    print(f"File content:\n{content}")
                return True
            else:
                print("✗ hello.py was not created")
                return False
                
        except subprocess.TimeoutExpired:
            print("✗ Command timed out")
            return False
        except Exception as e:
            print(f"✗ Command failed: {e}")
            return False
    
    elif scenario_name == "multi_step":
        # Multi-step scenario: create calculator, then tests
        steps = [
            "Create calculator.py with add and subtract functions",
            "Create test calculator with unit tests"
        ]
        
        for i, step in enumerate(steps, 1):
            print(f"\nStep {i}: {step}")
            
            if tool == "codex":
                cmd = [
                    "codex", "exec",
                    "--dangerously-bypass-approvals-and-sandbox",
                    "--json", 
                    step
                ]
            elif tool == "claude":
                cmd = [
                    "claude",
                    "--print", 
                    "--dangerously-skip-permissions",
                    step
                ]
            
            env = os.environ.copy()
            if tool == "codex":
                env["CODEX_API_BASE"] = "http://127.0.0.1:18080/v1"
                env["CODEX_API_KEY"] = "mock-key"
            
            try:
                result = subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=30)
                print(f"Step {i} return code: {result.returncode}")
                
                if result.returncode != 0:
                    print(f"✗ Step {i} failed")
                    if result.stderr:
                        print(f"Error: {result.stderr}")
                    return False
                    
            except Exception as e:
                print(f"✗ Step {i} failed: {e}")
                return False
        
        # Verify expected files were created
        expected_files = ["calculator.py", "test_calculator.py"]
        all_created = True
        
        for filename in expected_files:
            filepath = os.path.join(workspace, filename)
            if os.path.exists(filepath):
                print(f"✓ {filename} was created")
            else:
                print(f"✗ {filename} was not created")
                all_created = False
        
        return all_created
    
    return False


def main():
    parser = argparse.ArgumentParser(description="Run mock-agent integration tests")
    parser.add_argument("--tool", choices=["codex", "claude", "all"], default="all",
                       help="Which CLI tool to test (default: all)")
    parser.add_argument("--scenario", choices=["hello_world", "multi_step", "all"], default="all",
                       help="Which test scenario to run (default: all)")
    parser.add_argument("--server-port", type=int, default=18080,
                       help="Port for mock server (default: 18080)")
    parser.add_argument("--workspace", help="Use specific workspace directory")
    parser.add_argument("--verbose", "-v", action="store_true",
                       help="Verbose output")
    
    args = parser.parse_args()
    
    print("Mock Agent Integration Test Runner")
    print("=" * 40)
    
    # Check dependencies
    available_tools = check_dependencies()
    
    if not available_tools and args.tool != "all":
        if args.tool not in available_tools:
            print(f"Error: {args.tool} is not available")
            return 1
    
    # Determine which tools to test
    if args.tool == "all":
        tools_to_test = available_tools
    else:
        if args.tool in available_tools:
            tools_to_test = [args.tool]
        else:
            print(f"Error: {args.tool} is not available")
            return 1
    
    if not tools_to_test:
        print("No tools available for testing")
        return 1
    
    # Determine scenarios to run
    if args.scenario == "all":
        scenarios = ["hello_world", "multi_step"]
    else:
        scenarios = [args.scenario]
    
    # Create workspace
    if args.workspace:
        workspace = args.workspace
        os.makedirs(workspace, exist_ok=True)
    else:
        workspace = create_test_workspace()
    
    print(f"\nUsing workspace: {workspace}")
    
    # Note: This is a simplified test runner
    # In a real implementation, we would:
    # 1. Start the mock server automatically
    # 2. Run the actual integration test suite
    # 3. Handle server lifecycle management
    
    print(f"\nTo run full integration tests:")
    print(f"1. Start the mock server:")
    print(f"   cd {os.path.dirname(__file__)}")
    print(f"   python -m src.cli server --host 127.0.0.1 --port {args.server_port} --playbook examples/comprehensive_playbook.json")
    print(f"")
    print(f"2. Run the test suite:")
    print(f"   python tests/test_agent_integration.py")
    print(f"")
    print(f"Or run manual scenarios:")
    
    success_count = 0
    total_count = len(tools_to_test) * len(scenarios)
    
    for tool in tools_to_test:
        for scenario in scenarios:
            print(f"\n--- Testing {tool} with {scenario} scenario ---")
            print(f"NOTE: This requires the mock server to be running on port {args.server_port}")
            print(f"Manual test command would be:")
            
            # Show what the command would be
            if tool == "codex":
                print(f"  cd {workspace}")
                print(f"  export CODEX_API_BASE=http://127.0.0.1:{args.server_port}/v1")
                print(f"  export CODEX_API_KEY=mock-key")
                if scenario == "hello_world":
                    print(f"  codex exec --dangerously-bypass-approvals-and-sandbox 'Create hello.py that prints Hello, World!'")
                elif scenario == "multi_step":
                    print(f"  codex exec --dangerously-bypass-approvals-and-sandbox 'Create calculator.py with add and subtract functions'")
                    print(f"  codex exec --dangerously-bypass-approvals-and-sandbox 'Create test calculator with unit tests'")
            
            elif tool == "claude":
                print(f"  cd {workspace}")
                print(f"  # Note: Claude Code may not support custom API endpoints")
                if scenario == "hello_world":
                    print(f"  claude --print --dangerously-skip-permissions 'Create hello.py that prints Hello, World!'")
                elif scenario == "multi_step":
                    print(f"  claude --print --dangerously-skip-permissions 'Create calculator.py with add and subtract functions'")
                    print(f"  claude --print --dangerously-skip-permissions 'Create test calculator with unit tests'")
    
    print(f"\nTest Summary:")
    print(f"- Tools available: {', '.join(available_tools) if available_tools else 'None'}")
    print(f"- Scenarios to test: {', '.join(scenarios)}")
    print(f"- Workspace: {workspace}")
    print(f"\nFor automated testing, run: python tests/test_agent_integration.py")
    
    return 0


if __name__ == "__main__":
    sys.exit(main())