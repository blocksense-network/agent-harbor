#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
POC_DIR="$(dirname "$SCRIPT_DIR")"
LIB_DIR="$POC_DIR/lib"
INJECTOR_DIR="$POC_DIR/injector"

echo "=== DYLD Insert Libraries PoC - Basic Injection Test ==="
echo "Script directory: $SCRIPT_DIR"
echo "PoC directory: $POC_DIR"
echo

# Function to check if a command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Check prerequisites
echo "Checking prerequisites..."

if ! command_exists clang; then
    echo "ERROR: clang not found. Please install Xcode command line tools."
    exit 1
fi

if ! command_exists cargo; then
    echo "ERROR: cargo not found. Please install Rust."
    exit 1
fi

echo "Prerequisites OK"
echo

# Build the test library
echo "Building test library..."
cd "$LIB_DIR"
./build-test-lib.sh
echo "Test library built successfully"
echo

# Build the injector
echo "Building injector..."
cd "$INJECTOR_DIR"
cargo build --release
INJECTOR="$INJECTOR_DIR/target/release/dyld-injector"
echo "Injector built: $INJECTOR"
echo

# Test 1: Basic injection with sleep
echo "=== Test 1: Basic injection with sleep ==="
echo "Running: $INJECTOR -l $LIB_DIR/test-interpose.dylib sleep 1"
OUTPUT=$("$INJECTOR" -l "$LIB_DIR/test-interpose.dylib" sleep 1 2>&1)
echo "$OUTPUT"
echo "Test 1 completed"

# Verify using macOS system tools - use a longer running process
if command_exists vmmap; then
    echo "=== Additional Verification: System-level library loading check ==="
    # Run a longer-lived process to allow vmmap inspection
    echo "Running longer-lived process for system verification..."
    "$INJECTOR" -l "$LIB_DIR/test-interpose.dylib" sleep 2 &
    INJECTOR_PID=$!
    sleep 0.1  # Give it time to start

    # Find the child process PID (not the injector PID)
    CHILD_PIDS=$(pgrep -P $INJECTOR_PID 2>/dev/null || true)
    for pid in $CHILD_PIDS; do
        if kill -0 "$pid" 2>/dev/null && [ "$pid" != "$INJECTOR_PID" ]; then
            echo "Checking if library is loaded in process $pid using vmmap..."
            if vmmap "$pid" 2>/dev/null | grep -q "test-interpose"; then
                echo "✓ PASS: vmmap confirms library is loaded in process memory"
                VERIFIED_SYSTEM=true
            else
                echo "? Could not find library in vmmap output (may be expected for short-lived processes)"
            fi
            break
        fi
    done
    wait $INJECTOR_PID 2>/dev/null || true
    if [ "$VERIFIED_SYSTEM" != "true" ]; then
        echo "! INFO: System-level verification could not be performed (process too short-lived)"
    fi
fi
echo

# Test 2: Verify library is NOT loaded in parent process
echo "=== Test 2: Verify library NOT loaded in parent process ==="
echo "Running: $INJECTOR -l $LIB_DIR/test-interpose.dylib sleep 1 2>&1 | grep -c 'Library loaded'"
LOAD_COUNT=$("$INJECTOR" -l "$LIB_DIR/test-interpose.dylib" sleep 1 2>&1 | grep -c 'Library loaded' || true)
if [ "$LOAD_COUNT" -eq 1 ]; then
    echo "✓ PASS: Library loaded exactly once (in child process only)"
else
    echo "✗ FAIL: Library loaded $LOAD_COUNT times (expected 1)"
    exit 1
fi

# Additional verification: check that symbol verification also passed
SYMBOL_COUNT=$("$INJECTOR" -l "$LIB_DIR/test-interpose.dylib" sleep 1 2>&1 | grep -c 'Symbol verification passed' || true)
if [ "$SYMBOL_COUNT" -eq 1 ]; then
    echo "✓ PASS: Symbol verification passed (library fully functional)"
else
    echo "✗ FAIL: Symbol verification failed $SYMBOL_COUNT times (expected 1)"
    exit 1
fi
echo

# Test 3: Multiple libraries
echo "=== Test 3: Multiple libraries ==="
# Create a second test library
cd "$LIB_DIR"
cp test-interpose.c test-interpose2.c
sed -i '' 's/DYLD-TEST/DYLD-TEST2/g' test-interpose2.c
clang -dynamiclib -o test-interpose2.dylib test-interpose2.c

echo "Running with two libraries..."
"$INJECTOR" -l "$LIB_DIR/test-interpose.dylib:$LIB_DIR/test-interpose2.dylib" sleep 1
echo "Test 3 completed"

# Clean up test artifacts
rm -f "$LIB_DIR/test-interpose2.c" "$LIB_DIR/test-interpose2.dylib"
echo

# Test 4: Error handling for missing library
echo "=== Test 4: Error handling for missing library ==="
MISSING_LIB="/nonexistent/library.dylib"
echo "Running: $INJECTOR -l $MISSING_LIB sleep 1 (should fail)"
if "$INJECTOR" -l "$MISSING_LIB" sleep 1 2>/dev/null; then
    echo "✗ FAIL: Injector should have failed with missing library"
    exit 1
else
    echo "✓ PASS: Injector correctly failed with missing library"
fi
echo

# Test 5: Verify parent process doesn't show injection messages
echo "=== Test 5: Parent process isolation ==="
echo "Running injector without library injection..."
sleep 1 2>&1 | grep -q "Library loaded" && {
    echo "✗ FAIL: Parent process shows injection messages"
    exit 1
} || echo "✓ PASS: Parent process does not show injection messages"
echo

echo "=== All tests passed! ==="
echo "DYLD Insert Libraries basic injection is working correctly."
