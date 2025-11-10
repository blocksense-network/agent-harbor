#!/usr/bin/env bash
set -euo pipefail
LOGS_DIR="webui/e2e-tests/test-results/logs"
if [ ! -d "$LOGS_DIR" ]; then
    echo "❌ No test logs directory found at $LOGS_DIR"
    exit 1
fi

echo "📊 WebUI Test Report"
echo "==================="

# Count total tests
TOTAL_TESTS=$(find "$LOGS_DIR" -name "*.log" | wc -l)
echo "Total test runs: $TOTAL_TESTS"

# Check for failures
FAILED_TESTS=$(grep -l "failed\|error" "$LOGS_DIR"/*.log 2>/dev/null | wc -l)
PASSED_TESTS=$((TOTAL_TESTS - FAILED_TESTS))

echo "Passed: $PASSED_TESTS"
echo "Failed: $FAILED_TESTS"

if [ "$FAILED_TESTS" -gt 0 ]; then
    echo ""
    echo "❌ Failed tests:"
    grep -l "failed\|error" "$LOGS_DIR"/*.log 2>/dev/null | while read -r log_file; do
        basename "$log_file" .log
    done
    echo ""
    echo "📋 Check individual log files in $LOGS_DIR for details"
    exit 1
else
    echo ""
    echo "✅ All tests passed!"
fi
