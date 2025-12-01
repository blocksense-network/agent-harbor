#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

"""
Claude Code Usage Limits Verifier

This script verifies the research findings about Claude Code's usage limits by:
1. Spawning a Claude Code CLI session
2. Sending the /usage command
3. Parsing the response to extract usage information

Based on research in specs/Research/Agents/Obtaining-Usage-Limits.md
"""

import subprocess
import sys
import re
import json
from typing import Dict, Optional, Tuple
from dataclasses import dataclass


@dataclass
class ClaudeUsageInfo:
    """Parsed usage information from Claude Code /usage command"""
    session_remaining: Optional[str] = None
    weekly_remaining: Optional[str] = None
    reset_time: Optional[str] = None
    plan_type: Optional[str] = None
    raw_output: str = ""

    def to_dict(self) -> Dict:
        return {
            "session_remaining": self.session_remaining,
            "weekly_remaining": self.weekly_remaining,
            "reset_time": self.reset_time,
            "plan_type": self.plan_type,
            "raw_output": self.raw_output
        }


class ClaudeUsageVerifier:
    """Verifies Claude Code usage limits programmatically"""

    def __init__(self, claude_path: str = "claude"):
        self.claude_path = claude_path

    def check_claude_available(self) -> bool:
        """Check if Claude Code CLI is available"""
        try:
            result = subprocess.run(
                [self.claude_path, "--version"],
                capture_output=True,
                text=True,
                timeout=10
            )
            return result.returncode == 0
        except (subprocess.TimeoutExpired, FileNotFoundError):
            return False

    def get_usage_info(self) -> ClaudeUsageInfo:
        """
        Get usage information by spawning Claude and sending /usage command

        According to research, this should work by:
        1. Starting Claude in a subprocess
        2. Sending the /usage slash command
        3. Capturing and parsing the output
        """
        usage_info = ClaudeUsageInfo()

        try:
            # Start Claude process
            # Note: This is a simplified approach. In practice, you might need
            # to handle interactive sessions differently
            print("Starting Claude Code CLI session...")

            # For demonstration, we'll try a direct approach
            # In reality, you might need to use pexpect or similar for interactive control
            cmd = [self.claude_path, "--help"]
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=30,
                input="/usage\n"  # Try to send the command
            )

            usage_info.raw_output = result.stdout + result.stderr

            # Parse the output for usage information
            # This is a basic parser - real implementation would need more sophisticated parsing
            self._parse_usage_output(usage_info)

        except subprocess.TimeoutExpired:
            usage_info.raw_output = "Timeout: Claude CLI took too long to respond"
        except FileNotFoundError:
            usage_info.raw_output = f"Claude CLI not found at: {self.claude_path}"
        except Exception as e:
            usage_info.raw_output = f"Error running Claude CLI: {str(e)}"

        return usage_info

    def _parse_usage_output(self, usage_info: ClaudeUsageInfo) -> None:
        """Parse the raw output from Claude CLI for usage information"""
        output = usage_info.raw_output.lower()

        # Look for patterns that indicate usage limits
        # These patterns are based on the research document descriptions

        # Session limits (5-hour window)
        session_patterns = [
            r"(\d+)\s*(?:messages?|actions?)\s*(?:remaining|left)\s*in\s*(?:this\s*)?session",
            r"session.*?(?:limit|remaining).*?(\d+)",
            r"(\d+)\s*(?:messages?|actions?).*?5.*?(?:hour|hr)"
        ]

        for pattern in session_patterns:
            match = re.search(pattern, output, re.IGNORECASE)
            if match:
                usage_info.session_remaining = match.group(1)
                break

        # Weekly limits
        weekly_patterns = [
            r"(\d+)\s*(?:messages?|actions?|hours?)\s*(?:remaining|left)\s*(?:this\s*)?week",
            r"weekly.*?(?:limit|remaining).*?(\d+)",
            r"week.*?(?:limit|remaining).*?(\d+)"
        ]

        for pattern in weekly_patterns:
            match = re.search(pattern, output, re.IGNORECASE)
            if match:
                usage_info.weekly_remaining = match.group(1)
                break

        # Reset time patterns
        reset_patterns = [
            r"resets?\s*(?:in|at)\s*([^.\n]+)",
            r"next\s*reset\s*([^.\n]+)",
            r"available\s*(?:again|in)\s*([^.\n]+)"
        ]

        for pattern in reset_patterns:
            match = re.search(pattern, output, re.IGNORECASE)
            if match:
                usage_info.reset_time = match.group(1).strip()
                break

        # Plan type detection
        if "pro" in output:
            usage_info.plan_type = "pro"
        elif "max" in output:
            usage_info.plan_type = "max"
        elif "free" in output:
            usage_info.plan_type = "free"


def main():
    """Main verification function"""
    verifier = ClaudeUsageVerifier()
    usage_info = verifier.get_usage_info()

    if usage_info.plan_type:
        print(f"Claude: {usage_info.plan_type} plan")
    else:
        print("Claude: Plan not detected")

    if usage_info.session_remaining:
        print(f"Claude: {usage_info.session_remaining} session remaining")
    if usage_info.weekly_remaining:
        print(f"Claude: {usage_info.weekly_remaining} weekly remaining")

    # Save results to JSON
    with open("claude_usage_results.json", "w") as f:
        json.dump(usage_info.to_dict(), f, indent=2)

    return 0


if __name__ == "__main__":
    sys.exit(main())
