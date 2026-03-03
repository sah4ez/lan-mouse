#!/bin/bash
# Test lan-mouse connection with timeout
# This script runs lan-mouse daemon for a specified duration (default 15 seconds)
# to capture connection attempts and debug information

set -e

# Default timeout in seconds
DEFAULT_TIMEOUT=15

# Parse arguments
TIMEOUT=${1:-$DEFAULT_TIMEOUT}
LOG_LEVEL=${2:-debug}

echo "=========================================="
echo "lan-mouse Connection Test Script"
echo "=========================================="
echo ""
echo "Timeout: ${TIMEOUT} seconds"
echo "Log level: ${LOG_LEVEL}"
echo ""
echo "This will:"
echo "  1. Kill any existing lan-mouse processes"
echo "  2. Start lan-mouse daemon with ${LOG_LEVEL} logging"
echo "  3. Wait ${TIMEOUT} seconds for connection attempts"
echo "  4. Automatically stop the daemon"
echo ""
echo "Move your mouse to the screen edge during this time"
echo "to test connection establishment."
echo ""
echo "Press Ctrl+C to stop early"
echo "=========================================="
echo ""

# Kill any existing lan-mouse processes
echo "[1/4] Stopping any existing lan-mouse processes..."
pkill -f 'lan-mouse.*daemon' 2>/dev/null || true
sleep 1
echo "✓ Existing processes stopped"
echo ""

# Start lan-mouse daemon in background with logging
echo "[2/4] Starting lan-mouse daemon with ${LOG_LEVEL} logging..."
RUST_LOG=${LOG_LEVEL} ./target/release/lan-mouse daemon > /tmp/lan-mouse-test.log 2>&1 &
DAEMON_PID=$!
echo "✓ Daemon started (PID: ${DAEMON_PID})"
echo "  Log file: /tmp/lan-mouse-test.log"
echo ""

# Wait for daemon to initialize
echo "[3/4] Waiting for daemon to initialize..."
sleep 2

# Check if daemon is still running
if ! ps -p ${DAEMON_PID} > /dev/null 2>&1; then
    echo "✗ Daemon failed to start!"
    echo ""
    echo "Log output:"
    cat /tmp/lan-mouse-test.log
    exit 1
fi

echo "✓ Daemon initialized successfully"
echo ""
echo "=========================================="
echo "TESTING PHASE (${TIMEOUT} seconds)"
echo "=========================================="
echo ""
echo "Now move your mouse to the screen edge"
echo "to test connection to remote machine."
echo ""
echo "Monitoring logs..."
echo ""

# Monitor logs in real-time
tail -f /tmp/lan-mouse-test.log &
TAIL_PID=$!

# Wait for specified timeout
sleep ${TIMEOUT}

# Stop tail
kill ${TAIL_PID} 2>/dev/null || true

# Stop daemon
echo ""
echo "[4/4] Stopping daemon..."
kill ${DAEMON_PID} 2>/dev/null || true
wait ${DAEMON_PID} 2>/dev/null || true
echo "✓ Daemon stopped"
echo ""

# Display summary
echo "=========================================="
echo "TEST SUMMARY"
echo "=========================================="
echo ""

# Check for connection attempts
if grep -q "connecting to" /tmp/lan-mouse-test.log; then
    echo "✓ Connection attempts detected:"
    grep "connecting to" /tmp/lan-mouse-test.log | sed 's/^/  /'
    echo ""
fi

# Check for successful connections
if grep -q "Successfully established DTLS connection" /tmp/lan-mouse-test.log; then
    echo "✓ Connections established:"
    grep "Successfully established DTLS connection" /tmp/lan-mouse-test.log | sed 's/^/  /'
    echo ""
fi

# Check for errors
if grep -i "error\|failed" /tmp/lan-mouse-test.log | grep -v "could not resolve" > /dev/null; then
    echo "✗ Errors detected:"
    grep -i "error\|failed" /tmp/lan-mouse-test.log | grep -v "could not resolve" | sed 's/^/  /'
    echo ""
fi

# Check for capture backend status
if grep -q "Successfully created capture backend" /tmp/lan-mouse-test.log; then
    echo "✓ Input capture backend: WORKING"
    echo "  Backend: $(grep "Successfully created capture backend" /tmp/lan-mouse-test.log | awk '{print $NF}')"
    echo ""
elif grep -q "No input capture backend available" /tmp/lan-mouse-test.log; then
    echo "✗ Input capture backend: FAILED"
    echo "  See MACOS_ACCESSIBILITY_TROUBLESHOOTING.md for macOS issues"
    echo ""
fi

echo "=========================================="
echo "Full log saved to: /tmp/lan-mouse-test.log"
echo "=========================================="
echo ""
echo "To view full log:"
echo "  cat /tmp/lan-mouse-test.log"
echo ""
echo "To view connection-related logs:"
echo "  grep -i 'connecting\|connected\|dtls' /tmp/lan-mouse-test.log"
echo ""
