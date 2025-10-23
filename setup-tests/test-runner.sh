#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

# Docker-based integration test runner for agent-harbor setup
# Tests sourcing codex-setup with NIX=1 in a clean Alpine container

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}🚀 Starting Docker-based codex-setup integration test${NC}"
echo -e "${YELLOW}📋 This test runs in an Ubuntu Linux container to verify Nix installation via codex-setup sourcing${NC}"

# Check if Docker is available
if ! command -v docker >/dev/null 2>&1; then
  echo -e "${RED}❌ Docker is not available. Please install Docker first.${NC}"
  exit 1
fi

echo -e "${GREEN}✅ Docker is available${NC}"

# Get the directory where this script is located
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo -e "${YELLOW}📁 Test directory: $SCRIPT_DIR${NC}"
echo -e "${YELLOW}📁 Repository root: $REPO_ROOT${NC}"

# Build the Docker image
echo -e "${YELLOW}📦 Building Docker image...${NC}"
cd "$SCRIPT_DIR"

# Build the Docker image from the repository root (where the agent-harbor files are)
docker build -f Dockerfile -t agent-harbor-test "$REPO_ROOT"

echo -e "${YELLOW}🐳 Running test in Docker container...${NC}"

# Run the test script inside the container
# Use --rm to clean up the container after the test
# Mount the container test script
docker run --rm \
  --name agent-harbor-integration-test \
  -v "$SCRIPT_DIR/container-test.sh:/container-test.sh" \
  agent-harbor-test \
  sudo /bin/bash /container-test.sh

# Check the exit code
TEST_EXIT_CODE=$?

# Clean up
echo -e "${YELLOW}🧹 Cleaning up Docker image...${NC}"
docker rmi agent-harbor-test >/dev/null 2>&1 || true

if [ $TEST_EXIT_CODE -eq 0 ]; then
  echo -e "${GREEN}🎊 Docker-based codex-setup integration test completed successfully!${NC}"
  echo -e "${GREEN}✅ Nix installation via codex-setup sourcing works correctly in Ubuntu Linux container${NC}"
else
  echo -e "${RED}💥 Docker-based codex-setup integration test failed!${NC}"
  exit $TEST_EXIT_CODE
fi
