#!/usr/bin/env bash
set -euo pipefail

echo "Starting WebUI with mock server for manual testing (DEV MODE)..."
echo "Mock server will cycle through 5 scenarios: bug_fix, code_refactoring, documentation, feature_implementation, testing_workflow"
echo "Create new tasks to see different scenarios in action!"
echo ""
echo "WebUI: http://localhost:3000"
echo "Mock API: http://localhost:3001"
echo ""
echo "Note: Using development servers (not production build) for hot reload and better debugging"
echo ""

# Start mock server in background
cd webui/mock-server
npm run dev &
MOCK_PID=$!

# Start SSR dev server in background
cd ../app
npm run dev &
SSR_PID=$!

# Wait a moment for servers to start
sleep 3

echo "Servers started successfully!"
echo "Press Ctrl+C to stop both servers"

# Function to cleanup on exit
cleanup() {
  echo ""
  echo "Stopping servers..."
  kill $MOCK_PID 2>/dev/null
  kill $SSR_PID 2>/dev/null
  exit 0
}

# Set trap for cleanup
trap cleanup INT TERM

# Wait for user to stop
wait
