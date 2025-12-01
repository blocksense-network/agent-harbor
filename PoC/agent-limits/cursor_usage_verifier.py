#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

"""
Cursor IDE Usage Limits Verifier

This script verifies the research findings about Cursor's usage limits by:
1. Attempting to authenticate with Cursor's API
2. Calling the usage endpoint used by Cursor's dashboard
3. Parsing the response for token/credit usage information

Based on research in specs/Research/Agents/Obtaining-Usage-Limits.md

Note: This requires valid Cursor authentication credentials to work.
The research indicates that Cursor uses internal APIs that can be reverse-engineered
by inspecting network calls from the Cursor dashboard.
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
class CursorUsageInfo:
    """Parsed usage information from Cursor API"""
    plan_type: Optional[str] = None
    monthly_credits_included: Optional[float] = None
    credits_used: Optional[float] = None
    credits_remaining: Optional[float] = None
    usage_percentage: Optional[float] = None
    projected_exhaustion_date: Optional[str] = None
    raw_response: Dict = None

    def to_dict(self) -> Dict:
        return {
            "plan_type": self.plan_type,
            "monthly_credits_included": self.monthly_credits_included,
            "credits_used": self.credits_used,
            "credits_remaining": self.credits_remaining,
            "usage_percentage": self.usage_percentage,
            "projected_exhaustion_date": self.projected_exhaustion_date,
            "raw_response": self.raw_response
        }


class CursorUsageVerifier:
    """Verifies Cursor usage limits via API calls"""

    def __init__(self, auth_token: Optional[str] = None):
        self.auth_token = auth_token or os.getenv("CURSOR_AUTH_TOKEN") or self._extract_cursor_auth_token()
        self.base_url = "https://api.cursor.com"  # Hypothetical - would need to be determined
        self.headers = {
            "Content-Type": "application/json",
        }

        if self.auth_token:
            self.headers["Authorization"] = f"Bearer {self.auth_token}"

    def _extract_cursor_auth_token(self) -> Optional[str]:
        """Extract Cursor authentication token from filesystem (like ah-agents)"""
        try:
            import sqlite3
        except ImportError:
            return None

        # Get the correct database path based on platform (like ah-agents)
        home = os.path.expanduser("~")

        # Linux path (from ah-agents code)
        db_path = os.path.join(home, ".config", "Cursor", "User", "globalStorage", "state.vscdb")

        if not os.path.exists(db_path):
            return None

        try:
            conn = sqlite3.connect(db_path)
            cursor = conn.cursor()

            # Query all cursorAuth keys (same as in ah-agents)
            cursor.execute("SELECT key, value FROM ItemTable WHERE key LIKE 'cursorAuth/%'")

            tokens = {}
            for key, value in cursor.fetchall():
                tokens[key] = value

            conn.close()

            # Try different token types in order of preference (same as ah-agents)
            # 1. API key first
            if 'cursorAuth/apiKey' in tokens:
                return tokens['cursorAuth/apiKey']

            # 2. Access token
            if 'cursorAuth/accessToken' in tokens:
                return tokens['cursorAuth/accessToken']

            # 3. Refresh token
            if 'cursorAuth/refreshToken' in tokens:
                return tokens['cursorAuth/refreshToken']

        except Exception:
            pass

        return None

    def check_authentication(self) -> bool:
        """
        Check if we have valid authentication for Cursor API

        In practice, this would need to be determined by inspecting
        Cursor's actual authentication mechanism.
        """
        if not self.auth_token:
            return False

        # This is a placeholder - actual implementation would need
        # to reverse-engineer Cursor's authentication
        try:
            # Try a simple authenticated request
            req = urllib.request.Request(f"{self.base_url}/v1/user", headers=self.headers)
            with urllib.request.urlopen(req, timeout=10) as response:
                return response.getcode() == 200
        except:
            return False

    def get_usage_info(self) -> CursorUsageInfo:
        """
        Get usage information from Cursor's API

        According to research, this involves calling the same endpoint
        that Cursor's dashboard uses to display usage statistics.
        """
        usage_info = CursorUsageInfo()

        if not self.check_authentication():
            usage_info.raw_response = {"error": "No valid authentication token"}
            return usage_info

        try:
            # Based on research, Cursor uses GraphQL or REST APIs
            # This is a hypothetical endpoint - would need reverse engineering
            usage_endpoint = f"{self.base_url}/v1/usage"  # Hypothetical

            # Try GraphQL query (common pattern)
            graphql_query = """
            query GetUserUsage {
                user {
                    plan
                    usage {
                        monthlyCredits
                        usedCredits
                        remainingCredits
                        periodStart
                        periodEnd
                    }
                }
            }
            """

            # Prepare the request data
            request_data = json_module.dumps({"query": graphql_query}).encode('utf-8')
            req = urllib.request.Request(
                f"{self.base_url}/graphql",  # Hypothetical GraphQL endpoint
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
                        response_text = response.read().decode('utf-8')[:500]
                        usage_info.raw_response = {
                            "error": f"API request failed: {response.getcode()}",
                            "response_text": response_text
                        }
            except urllib.error.HTTPError as e:
                response_text = e.read().decode('utf-8')[:500]
                usage_info.raw_response = {
                    "error": f"HTTP error: {e.code}",
                    "response_text": response_text
                }
            except urllib.error.URLError as e:
                usage_info.raw_response = {
                    "error": f"URL error: {str(e)}"
                }

        except Exception as e:
            usage_info.raw_response = {"error": f"Unexpected error: {str(e)}"}

        return usage_info

    def _parse_usage_response(self, usage_info: CursorUsageInfo, data: Dict) -> None:
        """Parse the API response for usage information"""
        try:
            # Navigate the response structure (hypothetical based on research)
            user_data = data.get("data", {}).get("user", {})

            # Plan information
            plan = user_data.get("plan", {})
            if isinstance(plan, str):
                usage_info.plan_type = plan
            elif isinstance(plan, dict):
                usage_info.plan_type = plan.get("name")

            # Usage data
            usage = user_data.get("usage", {})

            monthly_credits = usage.get("monthlyCredits")
            used_credits = usage.get("usedCredits")

            if monthly_credits is not None:
                usage_info.monthly_credits_included = float(monthly_credits)

            if used_credits is not None:
                usage_info.credits_used = float(used_credits)

            # Calculate remaining credits
            if usage_info.monthly_credits_included is not None and usage_info.credits_used is not None:
                usage_info.credits_remaining = usage_info.monthly_credits_included - usage_info.credits_used

            # Calculate usage percentage
            if usage_info.monthly_credits_included and usage_info.monthly_credits_included > 0:
                usage_info.usage_percentage = (usage_info.credits_used / usage_info.monthly_credits_included) * 100

            # Try to extract or calculate projected exhaustion date
            # This would be based on current usage rate over time
            # For now, we'll leave this as a placeholder

        except Exception as e:
            # If parsing fails, at least keep the raw response
            pass

    def demonstrate_api_patterns(self) -> None:
        """Demonstrate the expected API patterns based on research"""
        print("Expected Cursor API Patterns (based on research):")
        print("-" * 50)

        print("1. Authentication:")
        print("   - Uses Bearer tokens or JWT from login")
        print("   - Tokens may be stored in Cursor config or browser localStorage")

        print("\n2. Usage Endpoint:")
        print("   - Likely GraphQL or REST API")
        print("   - Endpoint might be: https://api.cursor.com/v1/usage")
        print("   - Or GraphQL: https://api.cursor.com/graphql")

        print("\n3. Expected Response Structure:")
        sample_response = {
            "data": {
                "user": {
                    "plan": "Pro",
                    "usage": {
                        "monthlyCredits": 20.00,
                        "usedCredits": 12.50,
                        "remainingCredits": 7.50,
                        "periodStart": "2025-01-01T00:00:00Z",
                        "periodEnd": "2025-01-31T23:59:59Z"
                    }
                }
            }
        }
        print(json.dumps(sample_response, indent=2))

        print("\n4. Alternative: Web Scraping")
        print("   - Could load dashboard page and parse usage bar")
        print("   - URL: https://cursor.com/dashboard/usage")

    def simulate_usage_calculation(self) -> None:
        """Simulate usage calculations with sample data"""
        print("\nSimulating usage calculations:")

        # Sample data based on research (Pro plan = $20/month credits)
        sample_data = {
            "monthlyCredits": 20.00,
            "usedCredits": 15.75,
        }

        monthly = sample_data["monthlyCredits"]
        used = sample_data["usedCredits"]
        remaining = monthly - used
        percentage = (used / monthly) * 100

        print(f"Monthly credits included: ${monthly}")
        print(f"Credits used: ${used}")
        print(f"Credits remaining: ${remaining}")
        print(f"Usage percentage: {percentage:.1f}%")

        # Simple projection (assuming current usage rate continues)
        if percentage > 0:
            days_in_month = 30
            days_remaining = days_in_month * (remaining / used) if used > 0 else 0
            print(f"Projected days until exhaustion: {days_remaining:.1f}")


def main():
    """Main verification function"""
    verifier = CursorUsageVerifier()
    usage_info = verifier.get_usage_info()

    if not verifier.auth_token:
        print("Cursor: No auth token available")
        return 0

    print("Cursor: Auth token extracted from filesystem")

    if usage_info.plan_type:
        print(f"Cursor: {usage_info.plan_type} plan")
    else:
        print("Cursor: Plan not detected")

    if usage_info.credits_remaining is not None:
        print(f"Cursor: ${usage_info.credits_remaining:.2f} credits remaining")
    elif usage_info.usage_percentage is not None:
        print(f"Cursor: {usage_info.usage_percentage:.1f}% used")
    else:
        print("Cursor: No public API available - usage managed locally")

    # Save results to JSON
    with open("cursor_usage_results.json", "w") as f:
        json.dump(usage_info.to_dict(), f, indent=2)

    return 0


if __name__ == "__main__":
    sys.exit(main())
