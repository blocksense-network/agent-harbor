#!/usr/bin/env bash
set -euo pipefail
LOGS_DIR="webui/e2e-tests/test-results/logs"
if [ ! -d "$LOGS_DIR" ]; then
    echo "❌ No test logs directory found at $LOGS_DIR"
    exit 1
fi
# Find the most recent test run directory
LATEST_RUN=$(ls -td "$LOGS_DIR"/test-run-* | head -1)
if [ -z "$LATEST_RUN" ]; then
    echo "❌ No test runs found in $LOGS_DIR"
    exit 1
fi

echo "🔍 Analyzing test results from $(basename "$LATEST_RUN")..."
echo "=============================================="

# Parse failed tests from individual log files
FAILED_TESTS=$(grep -l "RESULT: Test failed" "$LATEST_RUN"/*.log | wc -l)
TOTAL_TESTS=$(ls "$LATEST_RUN"/*.log | grep -v "failed-tests\|test-summary" | wc -l)

if [ "$FAILED_TESTS" -gt 0 ]; then
    echo "❌ $FAILED_TESTS failed tests out of $TOTAL_TESTS total tests"
    echo ""
    echo "📋 Failed tests:"
    echo "---------------"

    # Extract failed test information from log files
    for log_file in "$LATEST_RUN"/*.log; do
        if grep -q "RESULT: Test failed" "$log_file"; then
            TEST_NAME=$(grep "TEST_START:" "$log_file" | sed 's/.*TEST_START: //')
            TEST_ID=$(grep "TEST_ID:" "$log_file" | sed 's/.*TEST_ID: //')
            TEST_FILE=$(grep "TEST_FILE:" "$log_file" | sed 's/.*TEST_FILE: //' | sed 's|.*/||')
            TEST_LINE=$(grep "TEST_LINE:" "$log_file" | sed 's/.*TEST_LINE: //')
            ERROR_MSG=$(grep "ERROR:" "$log_file" | head -1 | sed 's/.*ERROR: //' | cut -c1-80)

            echo "• $TEST_NAME"
            echo "  📄 $(basename "$log_file")"
            echo "  📍 $TEST_FILE:$TEST_LINE"
            if [ -n "$ERROR_MSG" ]; then
                echo "  💥 ${ERROR_MSG:0:80}..."
            fi
            echo ""
        fi
    done

    echo "💡 Commands to investigate further:"
    echo "   • View Playwright HTML report: just webui-test-report"
    echo "   • View detailed JSON: cat $LATEST_RUN/test-summary.json"
    echo "   • List all log files: ls -la $LATEST_RUN/*.log"
    echo "   • View specific test log: cat $LATEST_RUN/<filename>.log"
else
    echo "✅ All $TOTAL_TESTS tests passed!"
fi
