#!/bin/bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

# Setup script for authentication tokens
# This helps users configure the POC to access real API data

echo "🤖 Agent Harbor - Usage Limits POC Setup"
echo "=========================================="
echo ""

# Function to set environment variable
set_env_var() {
  local var_name=$1
  local description=$2
  local current_value=${!var_name}

  echo "$description"
  if [ -n "$current_value" ]; then
    echo "Current value: ${current_value:0:20}..."
  else
    echo "Not currently set"
  fi

  read -p "Enter $var_name (or press Enter to skip): " value
  if [ -n "$value" ]; then
    export $var_name="$value"
    echo "$var_name=$value" >>~/.agent-harbor-env
    echo "✅ Set $var_name"
  else
    echo "⏭️  Skipped $var_name"
  fi
  echo ""
}

# Create env file if it doesn't exist
touch ~/.agent-harbor-env

echo "This script helps you configure authentication tokens for the POC."
echo "Tokens can be obtained from:"
echo "- Cursor: Extract from ~/.config/Cursor/User/globalStorage/state.vscdb"
echo "- Codex: From ChatGPT browser session or config files"
echo "- Claude: Usually handled automatically by the CLI"
echo "- Replit: From Replit web interface"
echo ""

set_env_var "CURSOR_AUTH_TOKEN" "Cursor authentication token (JWT from database)"
set_env_var "CODEX_AUTH_TOKEN" "Codex/ChatGPT authentication token"
set_env_var "REPLIT_AUTH_TOKEN" "Replit authentication token"

echo "Setup complete! Run the following to load your tokens:"
echo "source ~/.agent-harbor-env"
echo ""
echo "Then run: just poc-show-usage-limits"
