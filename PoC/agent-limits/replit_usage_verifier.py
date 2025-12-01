#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

"""
Replit Ghostwriter Usage Limits Verifier

This script verifies the research findings about Replit Ghostwriter's usage limits by:
1. Authenticating with Replit's API
2. Making GraphQL queries to retrieve usage information
3. Parsing AI credit usage data

Based on research in specs/Research/Agents/Obtaining-Usage-Limits.md

Note: This requires valid Replit authentication credentials to work.
The research indicates that Replit exposes usage data through GraphQL APIs.
"""

import urllib.request
import urllib.error
import json as json_module
import sys
import json
import os
from typing import Dict, Optional, Tuple
from dataclasses import dataclass
import time


@dataclass
class ReplitUsageInfo:
    """Parsed usage information from Replit API"""
    plan_type: Optional[str] = None
    monthly_credits_included: Optional[float] = None
    credits_used: Optional[float] = None
    credits_remaining: Optional[float] = None
    usage_percentage: Optional[float] = None
    billing_period_start: Optional[str] = None
    billing_period_end: Optional[str] = None
    raw_response: Dict = None

    def to_dict(self) -> Dict:
        return {
            "plan_type": self.plan_type,
            "monthly_credits_included": self.monthly_credits_included,
            "credits_used": self.credits_used,
            "credits_remaining": self.credits_remaining,
            "usage_percentage": self.usage_percentage,
            "billing_period_start": self.billing_period_start,
            "billing_period_end": self.billing_period_end,
            "raw_response": self.raw_response
        }


class ReplitUsageVerifier:
    """Verifies Replit Ghostwriter usage limits via GraphQL API"""

    def __init__(self, auth_token: Optional[str] = None):
        self.auth_token = auth_token or os.getenv("REPLIT_AUTH_TOKEN") or self._extract_replit_auth_token()
        self.graphql_url = "https://replit.com/graphql"
        self.headers = {
            "Content-Type": "application/json",
            "User-Agent": "Replit-Ghostwriter-Verifier/1.0"
        }

        if self.auth_token:
            # Replit typically uses cookies or authorization headers
            # This may need adjustment based on actual auth method
            self.headers["Authorization"] = f"Bearer {self.auth_token}"

    def _extract_replit_auth_token(self) -> Optional[str]:
        """Extract Replit authentication token from filesystem (like ah-agents)"""
        home = os.path.expanduser("~")

        # Check for Replit config files
        config_paths = [
            os.path.join(home, ".config", "replit", "config.json"),
            os.path.join(home, ".replit", "config.json"),
            os.path.join(home, ".local", "share", "replit", "config.json"),
        ]

        for config_path in config_paths:
            if os.path.exists(config_path):
                try:
                    with open(config_path, 'r') as f:
                        config = json.load(f)
                        for key in ["token", "auth_token", "api_key", "access_token", "session_token"]:
                            if key in config:
                                return config[key]
                except:
                    continue

        # Check for browser storage (Replit web interface)
        browser_paths = [
            os.path.join(home, ".config", "google-chrome", "Default", "Local Storage", "leveldb"),
            os.path.join(home, ".config", "chromium", "Default", "Local Storage", "leveldb"),
            os.path.join(home, "Library", "Application Support", "Google", "Chrome", "Default", "Local Storage", "leveldb"),
        ]

        for browser_path in browser_paths:
            if os.path.exists(browser_path):
                token = self._extract_replit_token_from_browser(browser_path)
                if token:
                    return token

        return None

    def _extract_replit_token_from_browser(self, browser_path: str) -> Optional[str]:
        """Extract Replit authentication token from browser LevelDB storage"""
        try:
            import leveldb
        except ImportError:
            return None

        try:
            db = leveldb.LevelDB(browser_path)
            # Look for Replit related keys
            replit_keys = [
                b'replit-session-token',
                b'replit-access-token',
                b'replit-auth-token',
            ]

            for key in replit_keys:
                try:
                    value = db.Get(key)
                    if value:
                        token = value.decode('utf-8', errors='ignore')
                        if token and len(token) > 20:  # Basic validation
                            return token
                except KeyError:
                    continue

            # Search for keys containing 'replit'
            it = db.RangeIter()
            for key, value in it:
                key_str = key.decode('utf-8', errors='ignore')
                if 'replit' in key_str.lower():
                    value_str = value.decode('utf-8', errors='ignore')
                    if value_str and len(value_str) > 20:
                        return value_str

        except Exception:
            pass

        return None

    def check_authentication(self) -> bool:
        """
        Check if we have valid authentication for Replit API

        Based on research, Replit uses GraphQL APIs that require authentication.
        """
        if not self.auth_token:
            return False

        try:
            # Simple query to check if authenticated
            test_query = """
            query {
                currentUser {
                    id
                    username
                }
            }
            """

            request_data = json_module.dumps({"query": test_query}).encode('utf-8')
            req = urllib.request.Request(
                self.graphql_url,
                data=request_data,
                headers=self.headers,
                method='POST'
            )

            with urllib.request.urlopen(req, timeout=10) as response:
                if response.getcode() == 200:
                    data = json_module.loads(response.read().decode('utf-8'))
                    return "data" in data and "currentUser" in data["data"]

            return False

        except:
            return False

    def get_usage_info(self) -> ReplitUsageInfo:
        """
        Get usage information from Replit's GraphQL API

        According to research, Replit provides usage data through GraphQL queries.
        The dashboard shows detailed billing information for the current period.
        """
        usage_info = ReplitUsageInfo()

        if not self.check_authentication():
            usage_info.raw_response = {"error": "No valid authentication"}
            return usage_info

        try:
            # Based on research, Replit uses GraphQL for usage queries
            # This is a hypothetical query structure based on the research
            usage_query = """
            query GetUserUsage {
                currentUser {
                    username
                    subscription {
                        plan
                        credits {
                            included
                            used
                            remaining
                        }
                        billingPeriod {
                            start
                            end
                        }
                    }
                }
            }
            """

            # Prepare the request data
            request_data = json_module.dumps({"query": usage_query}).encode('utf-8')
            req = urllib.request.Request(
                self.graphql_url,
                data=request_data,
                headers=self.headers,
                method='POST'
            )

            try:
                with urllib.request.urlopen(req, timeout=30) as response:
                    if response.getcode() == 200:
                        data = json_module.loads(response.read().decode('utf-8'))
                        usage_info.raw_response = data
                        self._parse_usage_response(usage_info, data)
                    else:
                        usage_info.raw_response = {
                            "error": f"GraphQL request failed: {response.getcode()}",
                            "response_text": response.read().decode('utf-8')[:500]  # Truncate for safety
                        }
            except urllib.error.HTTPError as e:
                usage_info.raw_response = {
                    "error": f"HTTP error: {e.code}",
                    "response_text": e.read().decode('utf-8')[:500]
                }
            except urllib.error.URLError as e:
                usage_info.raw_response = {
                    "error": f"URL error: {str(e)}"
                }

        except Exception as e:
            usage_info.raw_response = {"error": f"Unexpected error: {str(e)}"}

        return usage_info

    def _parse_usage_response(self, usage_info: ReplitUsageInfo, data: Dict) -> None:
        """Parse the GraphQL response for usage information"""
        try:
            # Navigate the response structure (based on research patterns)
            user_data = data.get("data", {}).get("currentUser", {})

            # Plan information
            subscription = user_data.get("subscription", {})
            if isinstance(subscription, dict):
                usage_info.plan_type = subscription.get("plan")

                # Credits information
                credits = subscription.get("credits", {})
                if isinstance(credits, dict):
                    included = credits.get("included")
                    used = credits.get("used")
                    remaining = credits.get("remaining")

                    if included is not None:
                        usage_info.monthly_credits_included = float(included)
                    if used is not None:
                        usage_info.credits_used = float(used)
                    if remaining is not None:
                        usage_info.credits_remaining = float(remaining)

                    # Calculate usage percentage
                    if included and included > 0:
                        usage_info.usage_percentage = (used / included) * 100 if used else 0

                # Billing period
                billing_period = subscription.get("billingPeriod", {})
                if isinstance(billing_period, dict):
                    usage_info.billing_period_start = billing_period.get("start")
                    usage_info.billing_period_end = billing_period.get("end")

        except Exception as e:
            # If parsing fails, at least keep the raw response
            pass

    def demonstrate_api_patterns(self) -> None:
        """Demonstrate the expected API patterns based on research"""
        print("Expected Replit GraphQL API Patterns (based on research):")
        print("-" * 55)

        print("1. Authentication:")
        print("   - Uses Bearer tokens or session cookies")
        print("   - May require CSRF tokens for some operations")

        print("\n2. GraphQL Endpoint:")
        print("   - URL: https://replit.com/graphql")
        print("   - Method: POST with JSON payload")

        print("\n3. Usage Query Structure:")
        sample_query = """
        query GetUserUsage {
            currentUser {
                username
                subscription {
                    plan
                    credits {
                        included
                        used
                        remaining
                    }
                    billingPeriod {
                        start
                        end
                    }
                }
            }
        }
        """
        print(sample_query)

        print("\n4. Expected Response Structure:")
        sample_response = {
            "data": {
                "currentUser": {
                    "username": "testuser",
                    "subscription": {
                        "plan": "Core",
                        "credits": {
                            "included": 25.00,
                            "used": 18.50,
                            "remaining": 6.50
                        },
                        "billingPeriod": {
                            "start": "2025-01-01T00:00:00Z",
                            "end": "2025-01-31T23:59:59Z"
                        }
                    }
                }
            }
        }
        print(json.dumps(sample_response, indent=2))

    def simulate_usage_calculation(self) -> None:
        """Simulate usage calculations with sample data"""
        print("\nSimulating usage calculations:")

        # Sample data based on research (Core plan = ~$25/month credits)
        sample_data = {
            "plan": "Core",
            "included": 25.00,
            "used": 18.50,
            "remaining": 6.50
        }

        plan = sample_data["plan"]
        included = sample_data["included"]
        used = sample_data["used"]
        remaining = sample_data["remaining"]
        percentage = (used / included) * 100

        print(f"Plan: {plan}")
        print(f"Monthly credits included: ${included}")
        print(f"Credits used: ${used}")
        print(f"Credits remaining: ${remaining}")
        print(f"Usage percentage: {percentage:.1f}%")

        # Replit uses effort-based pricing, so this is credit-based
        print("\nNote: Replit uses effort-based pricing where different actions")
        print("(edits, checkpoints, etc.) consume different amounts of credits")


def main():
    """Main verification function"""
    verifier = ReplitUsageVerifier()
    usage_info = verifier.get_usage_info()

    if not verifier.auth_token:
        print("Replit: No auth token available")
        return 0

    if usage_info.plan_type:
        print(f"Replit: {usage_info.plan_type} plan")
    else:
        print("Replit: Plan not detected")

    if usage_info.credits_remaining is not None:
        print(f"Replit: ${usage_info.credits_remaining:.2f} credits remaining")
    elif usage_info.usage_percentage is not None:
        print(f"Replit: {usage_info.usage_percentage:.1f}% used")
    else:
        print("Replit: Rate limit data not available")

    # Save results to JSON
    with open("replit_usage_results.json", "w") as f:
        json.dump(usage_info.to_dict(), f, indent=2)

    return 0


if __name__ == "__main__":
    sys.exit(main())
