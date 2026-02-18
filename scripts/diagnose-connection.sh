#!/bin/bash
# Diagnostic script for lan-mouse connection issues

set -e

echo "=========================================="
echo "lan-mouse Connection Diagnostic Tool"
echo "=========================================="
echo ""

# Function to check if a command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Function to check if a UDP port is listening
check_udp_port() {
    local host=$1
    local port=$2
    local description=$3
    
    echo "Checking $description ($host:$port)..."
    
    if command_exists nc; then
        # For UDP, we try to send a packet and see if we get an ICMP "port unreachable" error
        # If we don't get an error, the port is likely open (or firewall is silently dropping packets)
        local result
        result=$(echo "test" | nc -u -v -w 1 "$host" "$port" 2>&1) || true
        
        # Check if nc reported success or if we didn't get a "port unreachable" error
        if echo "$result" | grep -qi "succeeded\|connected\|Connection to"; then
            echo "✓ UDP port $port is OPEN on $host"
            return 0
        elif echo "$result" | grep -qi "port unreachable\|connection refused"; then
            echo "✗ UDP port $port is CLOSED on $host"
            return 1
        else
            # If we don't get any error, assume port might be open (firewall might be silent)
            echo "✓ UDP port $port appears to be OPEN on $host (no ICMP error received)"
            return 0
        fi
    else
        echo "⚠ nc (netcat) is not available to check UDP port"
        return 2
    fi
}

# Function to check if lan-mouse is running
check_lanmouse_running() {
    local description=$1
    
    echo "Checking if lan-mouse daemon is running ($description)..."
    
    if pgrep -f "lan-mouse.*daemon" > /dev/null; then
        echo "✓ lan-mouse daemon is RUNNING"
        pgrep -f "lan-mouse.*daemon" -l
        return 0
    else
        echo "✗ lan-mouse daemon is NOT running"
        return 1
    fi
}

# Function to check firewall
check_firewall() {
    echo "Checking firewall status..."
    
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        if command_exists ufw; then
            if ufw status | grep -q "Status: active"; then
                echo "⚠ UFW firewall is ACTIVE"
                ufw status | grep 4242 || echo "  No rule found for port 4242"
            else
                echo "✓ UFW firewall is inactive or not configured"
            fi
        elif command_exists firewall-cmd; then
            if firewall-cmd --state >/dev/null 2>&1; then
                echo "⚠ firewalld is ACTIVE"
                firewall-cmd --list-ports | grep 4242 || echo "  No rule found for port 4242"
            else
                echo "✓ firewalld is inactive or not configured"
            fi
        else
            echo "⚠ Unable to determine firewall status"
        fi
    elif [[ "$OSTYPE" == "darwin"* ]]; then
        if /usr/libexec/ApplicationFirewall/socketfilterfw --getglobalstate | grep -q "enabled"; then
            echo "⚠ macOS firewall is ENABLED"
            echo "  Check System Preferences > Security & Privacy > Firewall"
        else
            echo "✓ macOS firewall is disabled"
        fi
    fi
}

# Function to get local IP addresses
get_local_ips() {
    echo "Local IP addresses:"
    if command_exists ip; then
        ip addr show | grep "inet " | grep -v "127.0.0.1" | awk '{print "  " $2}' | cut -d/ -f1
    elif command_exists ifconfig; then
        ifconfig | grep "inet " | grep -v "127.0.0.1" | awk '{print "  " $2}'
    else
        echo "  Unable to determine local IP addresses"
    fi
}

# Function to test network connectivity
test_connectivity() {
    local target=$1
    echo "Testing network connectivity to $target..."
    
    # macOS uses -W for timeout in milliseconds, Linux uses -W for timeout in seconds
    # Detect OS and use appropriate ping command
    if [[ "$OSTYPE" == "darwin"* ]]; then
        # macOS: -W is timeout in milliseconds
        if ping -c 1 -W 2000 "$target" >/dev/null 2>&1; then
            echo "✓ Host $target is REACHABLE"
            return 0
        else
            echo "✗ Host $target is UNREACHABLE"
            return 1
        fi
    else
        # Linux: -W is timeout in seconds
        if ping -c 1 -W 2 "$target" >/dev/null 2>&1; then
            echo "✓ Host $target is REACHABLE"
            return 0
        else
            echo "✗ Host $target is UNREACHABLE"
            return 1
        fi
    fi
}

# Main diagnostic flow
echo "Step 1: Checking local lan-mouse daemon..."
check_lanmouse_running "local"
echo ""

echo "Step 2: Checking local IP addresses..."
get_local_ips
echo ""

echo "Step 3: Checking firewall status..."
check_firewall
echo ""

# Read target IP from argument or prompt
if [ -n "$1" ]; then
    TARGET_IP="$1"
else
    echo "Step 4: Enter the IP address of the remote lan-mouse daemon:"
    read -r TARGET_IP
fi

if [ -z "$TARGET_IP" ]; then
    echo "No target IP provided, skipping remote checks"
    exit 0
fi

echo ""
echo "Step 5: Testing network connectivity to $TARGET_IP..."
test_connectivity "$TARGET_IP"
echo ""

echo "Step 6: Checking if lan-mouse UDP port 4242 is open on $TARGET_IP..."
check_udp_port "$TARGET_IP" 4242 "remote lan-mouse daemon"
echo ""

echo "=========================================="
echo "Diagnostic Summary"
echo "=========================================="
echo ""
echo "If the remote UDP port is CLOSED or UNREACHABLE:"
echo "1. Make sure lan-mouse daemon is running on the remote machine"
echo "2. Check firewall settings on both machines"
echo "3. Verify the IP address is correct"
echo "4. Ensure both machines are on the same network"
echo ""
echo "To start lan-mouse daemon on the remote machine:"
echo "  lan-mouse daemon"
echo ""
echo "To check firewall on Linux (ufw):"
echo "  sudo ufw allow 4242/udp"
echo ""
echo "To check firewall on Linux (firewalld):"
echo "  sudo firewall-cmd --add-port=4242/udp --permanent"
echo "  sudo firewall-cmd --reload"
echo ""
echo "To check firewall on macOS:"
echo "  System Preferences > Security & Privacy > Firewall > Firewall Options"
echo "  Add lan-mouse to allowed applications"
echo ""
