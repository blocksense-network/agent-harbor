"""
Test suite for the mock coding agent.

Tests verify that:
1. The agent can run scenarios and create expected files
2. The agent produces expected terminal output
3. The agent writes Codex-compatible rollout files
4. The CLI commands work correctly
"""

import os
import json
import tempfile
import shutil
import subprocess
import pytest
import pexpect
from pathlib import Path


class TestMockAgent:
    """Test suite for the mock coding agent functionality."""

    @pytest.fixture
    def temp_workspace(self):
        """Create a temporary workspace directory for testing."""
        workspace = tempfile.mkdtemp(prefix="mock_agent_test_")
        yield workspace
        shutil.rmtree(workspace)

    @pytest.fixture
    def temp_codex_home(self):
        """Create a temporary codex home directory for testing."""
        codex_home = tempfile.mkdtemp(prefix="mock_agent_codex_")
        yield codex_home
        shutil.rmtree(codex_home)

    @pytest.fixture
    def project_root(self):
        """Get the project root directory."""
        return Path(__file__).parent.parent

    def test_hello_scenario_file_creation(self, temp_workspace, temp_codex_home, project_root):
        """Test that running the hello scenario creates the expected file."""
        scenario_path = project_root / "examples" / "hello_scenario.yaml"
        
        # Run the agent
        result = subprocess.run([
            "python", "-m", "src.cli", "run",
            "--scenario", str(scenario_path),
            "--workspace", temp_workspace,
            "--codex-home", temp_codex_home
        ], cwd=project_root, capture_output=True, text=True)
        
        # Verify the command succeeded
        assert result.returncode == 0, f"Command failed: {result.stderr}"
        
        # Verify hello.py was created
        hello_file = Path(temp_workspace) / "hello.py"
        assert hello_file.exists(), "hello.py was not created"
        
        # Verify the content is correct
        content = hello_file.read_text()
        assert "print('Hello, World!')" in content, f"Unexpected content: {content}"

    def test_hello_scenario_terminal_output(self, temp_workspace, temp_codex_home, project_root):
        """Test that the agent produces expected terminal output."""
        scenario_path = project_root / "examples" / "hello_scenario.yaml"
        
        # Use pexpect to capture live output
        proc = pexpect.spawn(
            "python", ["-m", "src.cli", "run",
                      "--scenario", str(scenario_path),
                      "--workspace", temp_workspace,
                      "--codex-home", temp_codex_home],
            cwd=str(project_root),
            timeout=30
        )
        
        try:
            # Expect user input trace
            proc.expect(r"\[user\] Please create hello\.py that prints Hello, World!")
            
            # Expect thinking trace  
            proc.expect(r"\[thinking\] I'll create hello\.py with a print statement\.")
            
            # Expect tool call trace
            proc.expect(r"\[tool_call\] write_file")
            
            # Expect tool result trace
            proc.expect(r"\[tool_result\] File written successfully")
            
            # Expect assistant response
            proc.expect(r"\[assistant\] Created hello\.py\. Run: python hello\.py")
            
            # Wait for completion
            proc.expect(pexpect.EOF)
            
        finally:
            proc.close()
        
        assert proc.exitstatus == 0, f"Process failed with exit code {proc.exitstatus}"

    def test_demo_scenario(self, temp_workspace, temp_codex_home, project_root):
        """Test the built-in demo scenario."""
        result = subprocess.run([
            "python", "-m", "src.cli", "demo",
            "--workspace", temp_workspace,
            "--codex-home", temp_codex_home
        ], cwd=project_root, capture_output=True, text=True)
        
        # Verify the command succeeded
        assert result.returncode == 0, f"Demo command failed: {result.stderr}"
        
        # Verify the demo scenario file was created
        demo_scenario = Path(temp_workspace) / "_demo_scenario.json"
        assert demo_scenario.exists(), "Demo scenario file was not created"
        
        # Verify it's valid JSON
        with open(demo_scenario) as f:
            scenario_data = json.load(f)
        
        assert "meta" in scenario_data, "Demo scenario missing meta section"
        assert "turns" in scenario_data, "Demo scenario missing turns section"

    def test_rollout_file_creation(self, temp_workspace, temp_codex_home, project_root):
        """Test that rollout files are created in the correct location."""
        scenario_path = project_root / "examples" / "hello_scenario.yaml"
        
        result = subprocess.run([
            "python", "-m", "src.cli", "run",
            "--scenario", str(scenario_path),
            "--workspace", temp_workspace,
            "--codex-home", temp_codex_home
        ], cwd=project_root, capture_output=True, text=True)
        
        assert result.returncode == 0, f"Command failed: {result.stderr}"
        
        # Check that rollout files were created
        sessions_dir = Path(temp_codex_home) / "sessions"
        assert sessions_dir.exists(), "Sessions directory was not created"
        
        # Find rollout files (they have date-based subdirectories)
        rollout_files = list(sessions_dir.rglob("rollout-*.jsonl"))
        assert len(rollout_files) > 0, "No rollout files were created"
        
        # Verify the rollout file contains valid JSONL
        rollout_file = rollout_files[0]
        with open(rollout_file) as f:
            lines = f.readlines()
        
        assert len(lines) > 0, "Rollout file is empty"
        
        # Verify each line is valid JSON
        for line in lines:
            line = line.strip()
            if line:
                json.loads(line)  # This will raise if invalid JSON

    def test_session_log_creation(self, temp_workspace, temp_codex_home, project_root):
        """Test that session log files are created."""
        scenario_path = project_root / "examples" / "hello_scenario.yaml"
        
        result = subprocess.run([
            "python", "-m", "src.cli", "run",
            "--scenario", str(scenario_path),
            "--workspace", temp_workspace,
            "--codex-home", temp_codex_home
        ], cwd=project_root, capture_output=True, text=True)
        
        assert result.returncode == 0, f"Command failed: {result.stderr}"
        
        # Check that session log files were created
        logs_dir = Path(temp_codex_home) / "logs"
        assert logs_dir.exists(), "Logs directory was not created"
        
        # Find session log files
        log_files = list(logs_dir.glob("session-*.jsonl"))
        assert len(log_files) > 0, "No session log files were created"
        
        # Verify the log file contains valid JSONL
        log_file = log_files[0]
        with open(log_file) as f:
            lines = f.readlines()
        
        assert len(lines) > 0, "Session log file is empty"
        
        # Verify each line is valid JSON
        for line in lines:
            line = line.strip()
            if line:
                json.loads(line)  # This will raise if invalid JSON

    def test_file_operations(self, temp_workspace, temp_codex_home, project_root):
        """Test various file operations in scenarios."""
        # Create a custom scenario that tests multiple file operations
        custom_scenario = {
            "meta": {
                "instructions": "Test file operations"
            },
            "turns": [
                {"user": "Create and modify files for testing"},
                {"tool": {"name": "write_file", "args": {"path": "test.txt", "text": "Initial content\n"}}},
                {"tool": {"name": "read_file", "args": {"path": "test.txt"}}},
                {"tool": {"name": "append_file", "args": {"path": "test.txt", "text": "Appended content\n"}}},
                {"tool": {"name": "read_file", "args": {"path": "test.txt"}}},
                {"assistant": "Files created and modified successfully."}
            ]
        }
        
        # Write the scenario to a temporary file
        scenario_file = Path(temp_workspace) / "test_scenario.json"
        with open(scenario_file, "w") as f:
            json.dump(custom_scenario, f)
        
        # Run the scenario
        result = subprocess.run([
            "python", "-m", "src.cli", "run",
            "--scenario", str(scenario_file),
            "--workspace", temp_workspace,
            "--codex-home", temp_codex_home
        ], cwd=project_root, capture_output=True, text=True)
        
        assert result.returncode == 0, f"Command failed: {result.stderr}"
        
        # Verify the file was created and has the expected content
        test_file = Path(temp_workspace) / "test.txt"
        assert test_file.exists(), "test.txt was not created"
        
        content = test_file.read_text()
        assert "Initial content" in content, "Initial content not found"
        assert "Appended content" in content, "Appended content not found"

    def test_cli_help(self, project_root):
        """Test that CLI help commands work."""
        result = subprocess.run([
            "python", "-m", "src.cli", "--help"
        ], cwd=project_root, capture_output=True, text=True)
        
        assert result.returncode == 0, f"Help command failed: {result.stderr}"
        assert "Mock Coding Agent" in result.stdout
        assert "run" in result.stdout
        assert "demo" in result.stdout
        assert "server" in result.stdout

    def test_invalid_scenario(self, temp_workspace, temp_codex_home, project_root):
        """Test handling of invalid scenario files."""
        # Create an invalid scenario file
        invalid_scenario = Path(temp_workspace) / "invalid.json"
        invalid_scenario.write_text("{ invalid json")
        
        result = subprocess.run([
            "python", "-m", "src.cli", "run",
            "--scenario", str(invalid_scenario),
            "--workspace", temp_workspace,
            "--codex-home", temp_codex_home
        ], cwd=project_root, capture_output=True, text=True)
        
        # Should fail with non-zero exit code
        assert result.returncode != 0, "Should have failed with invalid JSON"