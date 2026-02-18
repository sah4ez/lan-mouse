# Connection Troubleshooting Guide

This guide helps you diagnose and fix connection issues between lan-mouse daemons running on different machines.

## Common Connection Issues

When lan-mouse fails to connect to a remote daemon, you may see errors like:

```
[WARN] lan_mouse::connect] failed to connect to 15.1.30.94:4242: `Connection timed out`
[WARN] lan_mouse::capture] releasing capture: not connected
```

> **Important:** lan-mouse uses **UDP** protocol for communication, not TCP. When testing connectivity, make sure to use UDP-specific commands.

These errors typically indicate one of the following issues:

1. **Remote daemon not running** - The lan-mouse daemon is not started on the remote machine
2. **Port not accessible** - The remote daemon is not listening on port 4242
3. **Firewall blocking** - A firewall is blocking UDP traffic on port 4242
4. **Network unreachable** - The IP address is incorrect or the machines are on different networks
5. **IP address changed** - The remote machine's IP address has changed

## Using the Diagnostic Script

A diagnostic script is provided to help troubleshoot connection issues:

```bash
./scripts/diagnose-connection.sh [remote-ip]
```

If you don't provide a remote IP, the script will prompt you for one.

### What the Script Checks

1. **Local daemon status** - Verifies if lan-mouse is running locally
2. **Local IP addresses** - Shows your machine's IP addresses
3. **Firewall status** - Checks if a firewall is active
4. **Network connectivity** - Tests if the remote host is reachable via ping
5. **Port accessibility** - Checks if port 4242 is open on the remote machine

### Example Output

```
==========================================
lan-mouse Connection Diagnostic Tool
==========================================

Step 1: Checking local lan-mouse daemon...
✓ lan-mouse daemon is RUNNING
12345 pts/0 00:00:01 lan-mouse daemon

Step 2: Checking local IP addresses...
Local IP addresses:
  192.168.1.100
  10.0.0.5

Step 3: Checking firewall status...
✓ UFW firewall is inactive or not configured

Step 4: Enter the IP address of the remote lan-mouse daemon:
15.1.30.94

Step 5: Testing network connectivity to 15.1.30.94...
✓ Host 15.1.30.94 is REACHABLE

Step 6: Checking if lan-mouse port 4242 is open on 15.1.30.94...
✗ Port 4242 is CLOSED or unreachable on 15.1.30.94

==========================================
Diagnostic Summary
==========================================

If the remote port is CLOSED or UNREACHABLE:
1. Make sure lan-mouse daemon is running on the remote machine
2. Check firewall settings on both machines
3. Verify the IP address is correct
4. Ensure both machines are on the same network
```

## Manual Troubleshooting Steps

### 1. Verify Remote Daemon is Running

On the remote machine, check if lan-mouse is running:

```bash
# Linux/macOS
pgrep -f "lan-mouse.*daemon"

# If not running, start it:
lan-mouse daemon
```

### 2. Check Network Connectivity

Test if you can reach the remote machine:

```bash
ping 15.1.30.94
```

If ping fails, check:
- Both machines are on the same network
- The IP address is correct
- No network issues (cables, Wi-Fi, etc.)

### 3. Check Port Accessibility

Test if UDP port 4242 is accessible:

```bash
# Using nc (netcat) - UDP
nc -u -v -w 3 15.1.30.94 4242

# Alternative: send a UDP packet
echo "test" | nc -u 15.1.30.94 4242
```

> **Note:** lan-mouse uses UDP protocol. TCP connections will be refused (`Connection refused`). This is expected behavior.

If the UDP port is closed, the remote daemon may not be running or a firewall is blocking it.

### 4. Check Firewall Settings

#### Linux (UFW)

```bash
# Check firewall status
sudo ufw status

# Allow UDP port 4242
sudo ufw allow 4242/udp

# Reload firewall
sudo ufw reload
```

#### Linux (firewalld)

```bash
# Check firewall status
sudo firewall-cmd --state

# Allow UDP port 4242
sudo firewall-cmd --add-port=4242/udp --permanent
sudo firewall-cmd --reload
```

#### macOS

1. Go to **System Preferences > Security & Privacy > Firewall**
2. Click **Firewall Options**
3. Add `lan-mouse` to the list of allowed applications
4. Ensure "Block all incoming connections" is unchecked

### 5. Verify Configuration

Check your lan-mouse configuration file (usually `~/.config/lan-mouse/config.toml`):

```toml
# Ensure the client IP is correct
[[clients]]
position = "right"
hostname = "remote-hostname"
ips = ["15.1.30.94"]  # Verify this IP is correct
port = 4242
```

### 6. Check Logs

Run lan-mouse with debug logging to see detailed connection attempts:

```bash
RUST_LOG=debug lan-mouse daemon
```

Look for messages like:
- `connecting to 15.1.30.94:4242 ...`
- `Connection timed out`
- `DTLS handshake failed`

## Improved Error Messages

The lan-mouse daemon now provides more detailed error messages to help diagnose connection issues:

### Connection Timeout

```
[ERROR] Connection to 15.1.30.94:4242 timed out after 5s
This typically means:
  1. The remote lan-mouse daemon is not running
  2. The remote daemon is not listening on port 4242
  3. A firewall is blocking the connection
  4. The IP address 15.1.30.94 is incorrect or unreachable

Troubleshooting steps:
  1. Run the diagnostic script: ./scripts/diagnose-connection.sh 15.1.30.94
  2. Check if lan-mouse is running on the remote machine: ssh 15.1.30.94 'pgrep -f lan-mouse'
  3. Test network connectivity: ping -c 3 15.1.30.94
  4. Check if the UDP port is open: nc -u -v -w 3 15.1.30.94 4242
  5. Verify firewall settings on both machines (ensure UDP port 4242 is allowed)
```

### DTLS Handshake Failure

```
[ERROR] DTLS handshake failed with 15.1.30.94:4242: ...
This may be due to:
  - Network connectivity issues (firewall, NAT)
  - DTLS version or cipher suite mismatch
  - Certificate validation issues
  - The remote device may not be running or may have a different version
```

## Cross-Platform Considerations

### Linux to macOS

- Ensure macOS firewall allows incoming connections
- Check that both machines are on the same network
- Verify the macOS daemon is running with proper permissions

### macOS to Linux

- Check Linux firewall (UFW, firewalld, iptables)
- Ensure the Linux daemon is running
- Verify network connectivity

### Linux to Linux

- Check firewall on both machines
- Ensure both daemons are running
- Verify network connectivity

## Getting Help

If you've tried all the above steps and still can't connect:

1. Run the diagnostic script on both machines
2. Collect the logs from both daemons (run with `RUST_LOG=debug`)
3. Check the GitHub issues for similar problems
4. Create a new issue with:
   - Operating systems of both machines
   - Output from the diagnostic script
   - Relevant log excerpts
   - Your configuration file (with sensitive information redacted)

## Additional Resources

- [Main README](../README.md)
- [Configuration Guide](../README.md#configuration)
- [GitHub Issues](https://github.com/feschber/lan-mouse/issues)
