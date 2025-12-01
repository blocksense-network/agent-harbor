#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

"""
Extract Cursor Authentication Token

This script extracts the authentication token from Cursor's local database
using the same technique as implemented in the ah-agents crate.
"""

import sqlite3
import os
import sys
from pathlib import Path


def get_cursor_db_path():
    """Get the platform-specific Cursor database path"""
    home = os.path.expanduser("~")

    # Linux path (from the Rust code)
    db_path = os.path.join(home, ".config/Cursor/User/globalStorage/state.vscdb")

    if os.path.exists(db_path):
        return db_path

    print(f"Cursor database not found at: {db_path}")
    return None


def extract_cursor_token():
    """Extract authentication token from Cursor database"""
    db_path = get_cursor_db_path()
    if not db_path:
        return None

    try:
        conn = sqlite3.connect(db_path)
        cursor = conn.cursor()

        # Query all cursorAuth keys (same as in the Rust code)
        cursor.execute("SELECT key, value FROM ItemTable WHERE key LIKE 'cursorAuth/%'")

        tokens = {}
        for key, value in cursor.fetchall():
            tokens[key] = value
            print(f"Found token: {key}")

        conn.close()

        # Try different token types in order of preference
        # 1. API key first
        if 'cursorAuth/apiKey' in tokens:
            token = tokens['cursorAuth/apiKey']
            print("Using API key")
            return token

        # 2. Access token
        if 'cursorAuth/accessToken' in tokens:
            token = tokens['cursorAuth/accessToken']
            print("Using access token")
            return token

        # 3. Refresh token
        if 'cursorAuth/refreshToken' in tokens:
            token = tokens['cursorAuth/refreshToken']
            print("Using refresh token")
            return token

        print("No suitable authentication token found")
        return None

    except Exception as e:
        print(f"Error accessing Cursor database: {e}")
        return None


def main():
    print("Extracting Cursor Authentication Token...")
    print("=" * 50)

    token = extract_cursor_token()

    if token:
        print("\n✅ Successfully extracted Cursor authentication token!")
        print(f"Token: {token[:20]}...{token[-20:] if len(token) > 40 else token}")

        # Set environment variable
        print(f"\nTo use this token, run:")
        print(f"export CURSOR_AUTH_TOKEN='{token}'")

        # Also write to a file for easy copying
        with open("cursor_token.txt", "w") as f:
            f.write(token)
        print("Token also saved to cursor_token.txt")

        return 0
    else:
        print("\n❌ Could not extract Cursor authentication token")
        print("Make sure Cursor is installed and you're logged in")
        return 1


if __name__ == "__main__":
    sys.exit(main())
