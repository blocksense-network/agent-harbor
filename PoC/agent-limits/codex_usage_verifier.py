#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

"""
OpenAI Codex CLI Usage Limits Verifier

This script verifies the research findings about OpenAI Codex CLI's usage limits by:
1. Attempting to run Codex CLI commands
2. Capturing error messages when limits are hit
3. Parsing usage information from outputs

Based on research in specs/Research/Agents/Obtaining-Usage-Limits.md

Note: This script demonstrates the reverse-engineering approach since OpenAI
doesn't provide an official usage API for Codex-ChatGPT subscriptions.
"""

import subprocess
import sys
import re
import json
import os
from typing import Dict, Optional, Tuple
from dataclasses import dataclass
import time


@dataclass
class CodexUsageInfo:
    """Parsed usage information from Codex CLI"""
    has_hit_limit: bool = False
    limit_type: Optional[str] = None  # "session" or "weekly"
    reset_time: Optional[str] = None
    reset_timestamp: Optional[float] = None
    raw_error_message: str = ""
    raw_output: str = ""
    plan_type: Optional[str] = None
    parsed_data: Optional[Dict] = None

    def to_dict(self) -> Dict:
        return {
            "has_hit_limit": self.has_hit_limit,
            "limit_type": self.limit_type,
            "reset_time": self.reset_time,
            "reset_timestamp": self.reset_timestamp,
            "raw_error_message": self.raw_error_message,
            "raw_output": self.raw_output,
            "plan_type": self.plan_type,
            "parsed_data": self.parsed_data
        }


class CodexUsageVerifier:
    """Verifies Codex CLI usage limits programmatically"""

    def __init__(self, codex_path: str = "codex"):
        self.codex_path = codex_path

    def check_codex_available(self) -> bool:
        """Check if Codex CLI is available"""
        try:
            result = subprocess.run(
                [self.codex_path, "--version"],
                capture_output=True,
                text=True,
                timeout=10
            )
            return result.returncode == 0
        except (subprocess.TimeoutExpired, FileNotFoundError):
            return False

    def check_codex_available(self) -> bool:
        """Check if Codex CLI is available"""
        try:
            result = subprocess.run(
                [self.codex_path, "--version"],
                capture_output=True,
                text=True,
                timeout=10
            )
            return result.returncode == 0
        except (subprocess.TimeoutExpired, FileNotFoundError):
            return False

    def extract_codex_rate_limits(self) -> CodexUsageInfo:
        """
        Extract rate limit information by making API calls to ChatGPT

        Based on the Codex implementation, this makes HTTP requests to
        the /wham/usage endpoint to get real-time rate limit information.
        """
        usage_info = CodexUsageInfo()

        # Get authentication token
        token = self._get_codex_auth_token()
        if not token:
            usage_info.raw_output = "No Codex authentication token found"
            return usage_info

        # Make API request to get rate limits
        try:
            import urllib.request
            import urllib.error

            # Use the same base URL normalization as Codex BackendClient
            url = "https://chatgpt.com/backend-api/wham/usage"
            headers = {
                "Authorization": f"Bearer {token}",
                "Content-Type": "application/json",
                "User-Agent": "Codex/1.0"
            }

            req = urllib.request.Request(url, headers=headers, method='GET')

            with urllib.request.urlopen(req, timeout=30) as response:
                if response.getcode() == 200:
                    response_text = response.read().decode('utf-8')
                    # Check if response is JSON or HTML
                    if response_text.strip().startswith('{'):
                        data = json.loads(response_text)
                        usage_info.raw_output = f"API Response: {data}"
                        self._parse_codex_api_response(usage_info, data)
                    else:
                        # HTML response - not the API endpoint we expected
                        usage_info.raw_output = f"Received HTML response instead of JSON API: {response_text[:200]}..."
                else:
                    response_text = response.read().decode('utf-8')
                    usage_info.raw_output = f"API request failed with status: {response.getcode()} - {response_text}"

        except urllib.error.HTTPError as e:
            response_text = e.read().decode('utf-8')
            usage_info.raw_output = f"HTTP Error: {e.code} - {response_text}"
        except urllib.error.URLError as e:
            usage_info.raw_output = f"URL Error: {str(e)}"
        except Exception as e:
            usage_info.raw_output = f"Error fetching rate limits: {str(e)}"

        return usage_info

    def _get_codex_auth_token(self) -> Optional[str]:
        """Get Codex authentication token from various sources (like ah-agents)"""
        # First try environment variable
        token = os.getenv("CODEX_AUTH_TOKEN")
        if token:
            return token

        # Extract from ChatGPT browser storage (like ah-agents approach)
        home = os.path.expanduser("~")

        # ChatGPT stores session data in browser local storage
        # Try to find Chrome/Chromium data
        browser_paths = [
            os.path.join(home, ".config", "google-chrome", "Default", "Local Storage", "leveldb"),
            os.path.join(home, ".config", "chromium", "Default", "Local Storage", "leveldb"),
            os.path.join(home, "Library", "Application Support", "Google", "Chrome", "Default", "Local Storage", "leveldb"),
            os.path.join(home, "AppData", "Local", "Google", "Chrome", "User Data", "Default", "Local Storage", "leveldb"),
        ]

        for browser_path in browser_paths:
            if os.path.exists(browser_path):
                token = self._extract_chatgpt_token_from_browser(browser_path)
                if token:
                    return token

        # Try to extract from ChatGPT CLI config if it exists
        chatgpt_config_paths = [
            os.path.join(home, ".config", "chatgpt", "config.json"),
            os.path.join(home, ".chatgpt", "config.json"),
        ]

        for config_path in chatgpt_config_paths:
            if os.path.exists(config_path):
                try:
                    with open(config_path, 'r') as f:
                        config = json.load(f)
                        for key in ["token", "auth_token", "api_key", "access_token", "session_token"]:
                            if key in config:
                                return config[key]
                except:
                    continue

        # Check for ~/.codex/auth.json (specific file mentioned by user)
        codex_auth_path = os.path.join(home, ".codex", "auth.json")
        if os.path.exists(codex_auth_path):
            try:
                with open(codex_auth_path, 'r') as f:
                    config = json.load(f)
                    # Check for direct token keys
                    for key in ["token", "auth_token", "api_key", "access_token", "session_token", "authToken"]:
                        if key in config and config[key]:
                            return config[key]
                    # Check for nested tokens.access_token
                    if "tokens" in config and isinstance(config["tokens"], dict):
                        tokens = config["tokens"]
                        for key in ["access_token", "token", "auth_token"]:
                            if key in tokens and tokens[key]:
                                return tokens[key]
            except:
                pass

        return None

    def _extract_chatgpt_token_from_browser(self, browser_path: str) -> Optional[str]:
        """Extract ChatGPT authentication token from browser LevelDB storage"""
        try:
            import leveldb
        except ImportError:
            return None

        try:
            db = leveldb.LevelDB(browser_path)
            # Look for ChatGPT related keys
            chatgpt_keys = [
                b'chatgpt-session-token',
                b'chatgpt-access-token',
                b'accessToken',
                b'sessionToken',
            ]

            for key in chatgpt_keys:
                try:
                    value = db.Get(key)
                    if value:
                        token = value.decode('utf-8', errors='ignore')
                        if token and len(token) > 20:  # Basic validation
                            return token
                except KeyError:
                    continue

            # Also search for keys containing 'chatgpt'
            # This is inefficient but might find tokens
            it = db.RangeIter()
            for key, value in it:
                key_str = key.decode('utf-8', errors='ignore')
                if 'chatgpt' in key_str.lower() or 'session' in key_str.lower():
                    value_str = value.decode('utf-8', errors='ignore')
                    if value_str and len(value_str) > 20:
                        return value_str

        except Exception:
            pass

        return None

    def _parse_codex_api_response(self, usage_info: CodexUsageInfo, data: dict) -> None:
        """Parse the ChatGPT API response for rate limits"""
        try:
            # Store the parsed data for display
            usage_info.parsed_data = data

            # Based on the RateLimitStatusPayload structure from Codex
            if 'plan_type' in data:
                plan_type = data['plan_type']
                usage_info.plan_type = plan_type

            # Parse rate limit information
            if 'rate_limit' in data and data['rate_limit']:
                rate_limit_data = data['rate_limit']

                # Primary window (usually 5h session)
                if 'primary_window' in rate_limit_data and rate_limit_data['primary_window']:
                    primary = rate_limit_data['primary_window']
                    if 'used_percent' in primary:
                        usage_percent = float(primary['used_percent'])
                        if usage_percent >= 100.0:
                            usage_info.has_hit_limit = True
                            usage_info.limit_type = "session"

                # Secondary window (usually weekly)
                if 'secondary_window' in rate_limit_data and rate_limit_data['secondary_window']:
                    secondary = rate_limit_data['secondary_window']
                    if 'used_percent' in secondary and float(secondary['used_percent']) >= 100.0:
                        usage_info.has_hit_limit = True
                        usage_info.limit_type = "weekly"

            # Parse credits information
            if 'credits' in data and data['credits']:
                credits_data = data['credits']
                # Store credits data for display

        except Exception as e:
            usage_info.raw_output += f"\nError parsing response: {str(e)}"

    def _parse_codex_rate_limits(self, usage_info: CodexUsageInfo, rate_data: dict) -> None:
        """Parse rate limit data similar to Codex's internal parsing"""
        try:
            # Based on the Codex rate_limits.rs implementation
            # Look for primary and secondary rate limit windows
            if 'primary' in rate_data:
                primary = rate_data['primary']
                if 'used_percent' in primary:
                    usage_info.has_hit_limit = primary['used_percent'] >= 100.0
                    usage_info.limit_type = "session"  # Primary is typically 5h session limit

            if 'secondary' in rate_data:
                secondary = rate_data['secondary']
                if 'used_percent' in secondary and secondary['used_percent'] >= 100.0:
                    usage_info.has_hit_limit = True
                    usage_info.limit_type = "weekly"  # Secondary is typically weekly limit

            # Extract reset time if available
            if 'primary' in rate_data and 'resets_at' in rate_data['primary']:
                usage_info.reset_time = rate_data['primary']['resets_at']

        except Exception as e:
            usage_info.raw_output += f"\nError parsing rate limits: {e}"

    def test_codex_usage(self, test_command: str = "echo 'test'") -> CodexUsageInfo:
        """
        Test Codex usage by running a command and checking for limit messages

        According to research, Codex CLI will output error messages like:
        "You've hit your usage limit. Upgrade to Pro or try again in 4 days 5 hours 31 minutes."

        This method attempts to trigger such messages by running actual commands.
        """
        usage_info = CodexUsageInfo()

        try:
            print(f"Running Codex CLI command: {test_command}")

            # Run a simple codex command that might trigger usage limits
            # In practice, you'd want to run actual coding tasks
            cmd = [self.codex_path, test_command]

            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=60,  # Give it time to process
                cwd="/tmp"   # Use a safe directory
            )

            usage_info.raw_output = result.stdout
            stderr = result.stderr
            usage_info.raw_output += stderr

            # Check for limit messages in output
            self._parse_limit_messages(usage_info, result.stdout + stderr)

        except subprocess.TimeoutExpired:
            usage_info.raw_output = "Timeout: Codex CLI took too long to respond"
        except FileNotFoundError:
            usage_info.raw_output = f"Codex CLI not found at: {self.codex_path}"
        except Exception as e:
            usage_info.raw_output = f"Error running Codex CLI: {str(e)}"

        return usage_info

    def _parse_limit_messages(self, usage_info: CodexUsageInfo, output: str) -> None:
        """Parse output for usage limit messages"""
        output_lower = output.lower()

        # Look for the characteristic limit message
        limit_patterns = [
            r"you[']?ve hit your usage limit",
            r"usage limit.*reached",
            r"limit.*exceeded",
            r"upgrade to pro",
            r"try again in"
        ]

        for pattern in limit_patterns:
            if re.search(pattern, output_lower):
                usage_info.has_hit_limit = True
                break

        if not usage_info.has_hit_limit:
            return

        # Extract the full error message
        # Look for the complete sentence containing the limit message
        lines = output.split('\n')
        for line in lines:
            if any(phrase in line.lower() for phrase in ["usage limit", "try again in", "upgrade to pro"]):
                usage_info.raw_error_message = line.strip()
                break

        # Parse reset time from messages like:
        # "try again in 4 days 5 hours 31 minutes"
        reset_pattern = r"try again in ([^.]+\w)"
        match = re.search(reset_pattern, output_lower)
        if match:
            reset_text = match.group(1).strip()
            usage_info.reset_time = reset_text

            # Try to parse the time duration
            # This is a simple parser - could be enhanced
            try:
                usage_info.reset_timestamp = self._parse_duration_to_timestamp(reset_text)
            except:
                pass  # Parsing failed, keep reset_time as string

        # Determine limit type based on context
        if "week" in output_lower or "7 day" in output_lower:
            usage_info.limit_type = "weekly"
        elif "session" in output_lower or "5 hour" in output_lower:
            usage_info.limit_type = "session"
        else:
            # Try to infer from the reset time
            if usage_info.reset_time:
                if any(unit in usage_info.reset_time.lower() for unit in ["day", "week"]):
                    usage_info.limit_type = "weekly"
                else:
                    usage_info.limit_type = "session"

    def _parse_duration_to_timestamp(self, duration_str: str) -> float:
        """
        Parse a human-readable duration like "4 days 5 hours 31 minutes"
        into a future timestamp
        """
        # Simple parser for common patterns
        total_seconds = 0

        # Match patterns like "4 days", "5 hours", "31 minutes"
        time_patterns = [
            (r"(\d+)\s*days?", 86400),      # days to seconds
            (r"(\d+)\s*hours?", 3600),      # hours to seconds
            (r"(\d+)\s*minutes?", 60),      # minutes to seconds
            (r"(\d+)\s*seconds?", 1),       # seconds
        ]

        for pattern, multiplier in time_patterns:
            match = re.search(pattern, duration_str, re.IGNORECASE)
            if match:
                value = int(match.group(1))
                total_seconds += value * multiplier

        if total_seconds > 0:
            return time.time() + total_seconds

        return time.time()  # Fallback to current time

    def demonstrate_parsing(self) -> None:
        """Demonstrate parsing with sample error messages"""
        print("Demonstrating usage limit parsing with sample messages:")

        sample_messages = [
            "You've hit your usage limit. Upgrade to Pro or try again in 4 days 5 hours 31 minutes.",
            "Usage limit reached for this session. Try again in 2 hours 15 minutes.",
            "You've exceeded your weekly limit. Upgrade to Pro or wait 3 days 12 hours.",
        ]

        for msg in sample_messages:
            print(f"\nSample message: {msg}")
            test_info = CodexUsageInfo()
            self._parse_limit_messages(test_info, msg)

            print(f"  Has hit limit: {test_info.has_hit_limit}")
            print(f"  Limit type: {test_info.limit_type}")
            print(f"  Reset time: {test_info.reset_time}")
            if test_info.reset_timestamp:
                print(f"  Reset timestamp: {time.ctime(test_info.reset_timestamp)}")


def main():
    """Main verification function"""
    verifier = CodexUsageVerifier()

    # Check if token is available first
    token = verifier._get_codex_auth_token()
    if not token:
        print("Codex: No auth token available")
        return 0

    usage_info = verifier.extract_codex_rate_limits()

    # Check if we got any data from the API
    if usage_info.parsed_data:
        # Display rate limit information in YAML format
        try:
            import yaml
            # Format the data for display
            display_data = {
                'plan': usage_info.plan_type or 'unknown',
                'rate_limits': {},
                'credits': {}
            }

            data = usage_info.parsed_data

            # Add rate limit information
            if 'rate_limit' in data and data['rate_limit']:
                rl_data = data['rate_limit']
                rate_limits = {}

                if 'primary_window' in rl_data and rl_data['primary_window']:
                    primary = rl_data['primary_window']
                    resets_at = primary.get('reset_at')
                    if resets_at:
                        # Convert Unix timestamp to readable date
                        from datetime import datetime
                        resets_at = datetime.fromtimestamp(resets_at).isoformat()
                    rate_limits['primary'] = {
                        'used_percent': primary.get('used_percent', 0),
                        'resets_at': resets_at
                    }

                if 'secondary_window' in rl_data and rl_data['secondary_window']:
                    secondary = rl_data['secondary_window']
                    resets_at = secondary.get('reset_at')
                    if resets_at:
                        # Convert Unix timestamp to readable date
                        from datetime import datetime
                        resets_at = datetime.fromtimestamp(resets_at).isoformat()
                    rate_limits['secondary'] = {
                        'used_percent': secondary.get('used_percent', 0),
                        'resets_at': resets_at
                    }

                display_data['rate_limits'] = rate_limits

            # Add credits information
            if 'credits' in data and data['credits']:
                credits_data = data['credits']
                display_data['credits'] = {
                    'balance': credits_data.get('balance'),
                    'unlimited': credits_data.get('unlimited', False)
                }

            # Print YAML output
            print("Codex:")
            print(yaml.dump(display_data, default_flow_style=False, indent=2).strip())

        except ImportError:
            # Fallback if yaml not available
            print(f"Codex: {usage_info.plan_type or 'unknown'} plan - Rate limits available")
    else:
        print("Codex: Auth token extracted from ~/.codex/auth.json")
        print("Codex: API endpoint needs investigation")

    # Save results to JSON
    with open("codex_usage_results.json", "w") as f:
        json.dump(usage_info.to_dict(), f, indent=2)

    return 0


if __name__ == "__main__":
    sys.exit(main())
