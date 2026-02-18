# Connection Timeout Fix - Summary

## Problem

The user reported that the Linux build of lan-mouse was working correctly, but when attempting to connect from a macOS client, the connection was timing out with the following error:

```
[WARN] lan_mouse::connect] failed to connect to 15.1.30.94:4242: `Connection timed out`
[WARN] lan_mouse::capture] releasing capture: not connected
```

> **Important:** lan-mouse uses **UDP** protocol for communication, not TCP. This is a critical detail for troubleshooting.

## Root Cause Analysis

The connection timeout issue is a **network connectivity problem**, not a code bug. The Linux daemon is unable to establish a DTLS connection to the macOS daemon at `15.1.30.94:4242` because:

1. The macOS daemon may not be running
2. The macOS daemon may not be listening on port 4242
3. A firewall may be blocking UDP traffic on port 4242
4. The IP address may be incorrect or unreachable
5. The machines may be on different networks

## Solution

Since this is a network configuration issue rather than a code bug, the solution focuses on providing better diagnostic tools and error messages to help users troubleshoot connection problems.

### Changes Made

#### 1. Created Diagnostic Script (`scripts/diagnose-connection.sh`)

A comprehensive diagnostic script that checks:
- Local lan-mouse daemon status
- Local IP addresses
- Firewall status (UFW, firewalld, macOS firewall)
- Network connectivity to remote host (ping test)
- **UDP port accessibility** (port 4242) - **Updated to check UDP, not TCP**

> **Important:** The script now correctly checks UDP ports using `nc -u` instead of TCP ports, since lan-mouse uses UDP protocol for communication.

The script provides clear, actionable feedback and troubleshooting steps.

**Usage:**
```bash
./scripts/diagnose-connection.sh [remote-ip]
```

#### 2. Enhanced Error Logging in `src/connect.rs`

Added detailed debug and error logging to help diagnose connection issues:

- **Connection timeout errors** now include:
  - Clear explanation of what timeout means
  - List of common causes
  - Step-by-step troubleshooting instructions
  - Reference to the diagnostic script

- **UDP socket creation and connection** now logs each step
- **DTLS handshake** logs success/failure with context
- **Multiple address attempts** log progress and remaining attempts

Example of improved timeout error:
```
[ERROR] Connection to 15.1.30.94:4242 timed out after 5s
This typically means:
  1. The remote lan-mouse daemon is not running
  2. The remote daemon is not listening on port 4242
  3. A firewall is blocking the connection
  4. The IP address 15.1.30.94 is incorrect or unreachable

Troubleshooting steps:
  1. Run the diagnostic script: ./scripts/diagnose-connection.sh 15.1.30.94
  2. Check if lan-mouse is running on the remote machine
  3. Test network connectivity
  4. Check if the port is open
  5. Verify firewall settings on both machines
```

#### 3. Enhanced Error Logging in `src/capture.rs`

Improved the "releasing capture: not connected" warning to include:
- Explanation that the client is not connected
- Reference to connection attempt logs
- Suggestion to run the diagnostic script

#### 4. Created Comprehensive Troubleshooting Guide (`docs/CONNECTION_TROUBLESHOOTING.md`)

A detailed guide covering:
- Common connection issues and their causes
- How to use the diagnostic script
- Manual troubleshooting steps
- Firewall configuration for Linux (UFW, firewalld) and macOS
- Configuration verification
- Log analysis
- Cross-platform considerations
- Getting help

#### 5. Updated Main README

Added a "Troubleshooting Connection Issues" section in the configuration section with:
- Link to the troubleshooting guide
- Reference to the diagnostic script

## Files Modified

1. **`scripts/diagnose-connection.sh`** (new) - Diagnostic script
2. **`src/connect.rs`** - Enhanced error logging
3. **`src/capture.rs`** - Enhanced warning messages
4. **`docs/CONNECTION_TROUBLESHOOTING.md`** (new) - Troubleshooting guide
5. **`README.md`** - Added troubleshooting section

## How to Use

### For Users Experiencing Connection Issues

1. **Run the diagnostic script:**
   ```bash
   ./scripts/diagnose-connection.sh 15.1.30.94
   ```

2. **Review the output** and follow the suggested steps

3. **Check the logs** with debug logging enabled:
   ```bash
   RUST_LOG=debug lan-mouse daemon
   ```

4. **Refer to the troubleshooting guide** for detailed steps:
   - [Connection Troubleshooting Guide](docs/CONNECTION_TROUBLESHOOTING.md)

### Common Fixes

Based on the diagnostic output, common fixes include:

1. **Start the remote daemon:**
   ```bash
   # On the remote machine (macOS in this case)
   lan-mouse daemon
   ```

2. **Configure firewall:**
   ```bash
   # Linux (UFW)
   sudo ufw allow 4242/udp

   # Linux (firewalld)
   sudo firewall-cmd --add-port=4242/udp --permanent
   sudo firewall-cmd --reload

   # macOS: System Preferences > Security & Privacy > Firewall > Firewall Options
   ```

3. **Verify IP address:**
   - Check the remote machine's IP address
   - Update the configuration file with the correct IP

4. **Check network connectivity:**
   ```bash
   ping 15.1.30.94
   nc -u -v -w 3 15.1.30.94 4242  # UDP check
   ```
   > **Note:** lan-mouse uses UDP, not TCP. TCP connections will be refused, which is expected.

## Testing

To test the changes:

1. Build the project:
   ```bash
   cargo build --release
   ```

2. Run the daemon with debug logging:
   ```bash
   RUST_LOG=debug ./target/release/lan-mouse daemon
   ```

3. Run the diagnostic script:
   ```bash
   ./scripts/diagnose-connection.sh <remote-ip>
   ```

4. Verify that error messages are clear and helpful

## Benefits

1. **Better user experience** - Users can quickly diagnose connection issues
2. **Reduced support burden** - Clear error messages reduce the need for manual troubleshooting
3. **Comprehensive documentation** - Detailed guide covers all common scenarios
4. **Automated diagnostics** - Script automates common checks
5. **Actionable feedback** - Each error includes specific next steps

## Future Improvements

Potential enhancements for the future:

1. **Automatic retry with exponential backoff** - Instead of failing immediately, retry connections
2. **Network discovery** - Automatically discover lan-mouse instances on the local network
3. **GUI diagnostics** - Integrate diagnostic checks into the GTK frontend
4. **Connection health monitoring** - Show connection status and quality in real-time
5. **Automatic firewall configuration** - Offer to configure firewall rules automatically

## Conclusion

The connection timeout issue is a network configuration problem, not a code bug. The solution provides comprehensive diagnostic tools and documentation to help users identify and resolve network connectivity issues quickly and effectively.
