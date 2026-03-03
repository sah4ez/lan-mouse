# macOS Accessibility Permissions Issue - Event Tap Creation Failed

## Problem

When running lan-mouse on macOS, you may see this error:

```
[ERROR input_capture] No input capture backend available. Tried: [MacOs]
[WARN lan_mouse::capture] input capture exited: error creating input-capture: `no backend available`
```

Or in more detailed logs:

```
[WARN input_capture] Failed to create MacOS input capture backend: error creating macos capture backend: `event tap creation failed`
```

## Root Cause

lan-mouse requires **Accessibility permissions** on macOS to capture mouse and keyboard events. Without these permissions, the macOS input capture backend cannot create an event tap, which is necessary to:

1. Detect when the mouse cursor crosses screen edges
2. Initiate connections to remote machines
3. Capture local input events to send to remote machines

## Symptoms

- lan-mouse daemon starts successfully
- Remote machines can connect TO your macOS machine
- Your macOS machine CANNOT connect TO remote machines
- Mouse cursor crossing screen edges doesn't trigger connections
- Logs show "event tap creation failed" error

## Solution: Grant Accessibility Permissions

### Method 1: Through System Settings (Recommended)

1. Open **System Settings** (or System Preferences on older macOS versions)
2. Go to **Privacy & Security** → **Accessibility**
3. Click the **+** (plus) button
4. Navigate to and select the lan-mouse binary:
   - If installed: `/Applications/LanMouse.app`
   - If running from source: `/Users/your-username/git/lan-mouse/target/release/lan-mouse`
5. Make sure the toggle switch next to lan-mouse is **ON**
6. You may need to enter your administrator password
7. **Restart the lan-mouse daemon** for changes to take effect

### Method 2: Using tccutil (Advanced)

```bash
# Check current accessibility status for lan-mouse
tccutil accessibility com.feschber.LanMouse

# If needed, reset accessibility permissions (you'll need to re-grant them)
tccutil reset Accessibility com.feschber.LanMouse

# Then follow Method 1 to grant permissions again
```

### Method 3: For Development Builds

If you're building from source and running the binary directly:

1. Open **System Settings** → **Privacy & Security** → **Accessibility**
2. Click **+** and select your built binary:
   ```
   /Users/your-username/git/lan-mouse/target/release/lan-mouse
   ```
3. Enable the toggle
4. Restart the daemon

## Verification

After granting permissions and restarting the daemon, check the logs:

### ✅ Success (Permissions Granted)

```bash
RUST_LOG=debug ./target/release/lan-mouse daemon
```

You should see:

```
[INFO input_capture] Attempting to create MacOS input capture backend...
[DEBUG input_capture::macos] Updated displays bounds: Bounds { xmin: 0.0, xmax: 1470.0, ymin: 0.0, ymax: 956.0 }
[INFO input_capture::macos] Enabling CGEvent tap
[INFO input_capture] Successfully created capture backend: macos
```

### ❌ Failure (Permissions Not Granted)

```
[WARN input_capture] Failed to create MacOS input capture backend: error creating macos capture backend: `event tap creation failed`
[ERROR input_capture] No input capture backend available. Tried: [MacOs]
```

## Common Issues

### Issue 1: Permission Granted But Still Fails

**Problem:** You granted permission but still see "event tap creation failed"

**Solutions:**
1. **Restart the daemon** - Permissions only take effect after restart
2. **Check the correct binary** - Make sure you granted permission to the exact binary you're running
3. **Revoke and re-grant** - Sometimes macOS needs a fresh grant:
   - Remove lan-mouse from Accessibility list
   - Add it back
   - Restart daemon

### Issue 2: Binary Path Changed

**Problem:** You rebuilt the binary or moved it, and permission was lost

**Solution:** Grant permission to the new binary path:
```bash
# Check which binary is running
ps aux | grep lan-mouse

# Grant permission to that specific path
# System Settings → Privacy & Security → Accessibility → + → select the binary
```

### Issue 3: Multiple lan-mouse Processes

**Problem:** Multiple instances running with different permission states

**Solution:**
```bash
# Kill all lan-mouse processes
pkill -f lan-mouse

# Start fresh
./target/release/lan-mouse daemon
```

### Issue 4: macOS Security Prompt Not Appearing

**Problem:** No prompt to grant permission appears

**Solution:**
1. Manually add the binary to Accessibility list (Method 1)
2. Or run the binary once, then check System Settings
3. Make sure you're not running in a sandboxed environment

## Troubleshooting Steps

### Step 1: Verify Current Status

```bash
# Check if lan-mouse is running
ps aux | grep lan-mouse

# Check logs for capture backend status
log show --predicate 'process == "lan-mouse"' --last 5m | grep input_capture
```

### Step 2: Check Accessibility Permissions

```bash
# List all apps with accessibility permissions
tccutil accessibility

# Check specific app status
tccutil accessibility com.feschber.LanMouse
```

### Step 3: Test Event Tap Creation

Create a simple test to verify event tap works:

```bash
# Run lan-mouse with debug logging
RUST_LOG=debug ./target/release/lan-mouse daemon

# Look for these lines:
# [INFO input_capture] Successfully created capture backend: macos
# OR
# [WARN input_capture] Failed to create MacOS input capture backend: event tap creation failed
```

### Step 4: Verify Connection Works

After permissions are granted:

1. Start daemon on both machines
2. Move mouse to screen edge towards remote machine
3. Connection should establish automatically
4. Check logs for successful connection:
   ```
   [INFO lan_mouse::connect] connecting to 15.1.30.94:4242 ...
   [INFO lan_mouse::connect] Successfully established DTLS connection with 15.1.30.94:4242
   ```

## Related Issues

### Screen Recording Permissions

Some macOS versions may also require **Screen Recording** permissions:

1. Go to **System Settings** → **Privacy & Security** → **Screen Recording**
2. Add lan-mouse to the list
3. Restart the daemon

### Full Disk Access

Rarely, lan-mouse may need **Full Disk Access** to read configuration:

1. Go to **System Settings** → **Privacy & Security** → **Full Disk Access**
2. Add lan-mouse to the list
3. Restart the daemon

## Automation (For Development)

If you're developing lan-mouse and frequently rebuilding, you can create a script to automate permission granting:

```bash
#!/bin/bash
# setup-macos-permissions.sh

LAN_MOUSE_BIN="/Users/$(whoami)/git/lan-mouse/target/release/lan-mouse"

echo "Please grant Accessibility permission to:"
echo "$LAN_MOUSE_BIN"
echo ""
echo "Opening System Settings..."
open "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"

echo ""
echo "After granting permission, press Enter to restart lan-mouse..."
read

pkill -f lan-mouse
$LAN_MOUSE_BIN daemon
```

## Security Considerations

Granting Accessibility permissions allows an application to:

- Monitor keyboard and mouse events
- Simulate keyboard and mouse input
- Control other applications

**Only grant these permissions to applications you trust.** lan-mouse is open-source, and you can review the code at: https://github.com/feschber/lan-mouse

## Additional Resources

- [Apple Developer Documentation: Accessibility API](https://developer.apple.com/documentation/coregraphics/cgeventtap)
- [lan-mouse GitHub Repository](https://github.com/feschber/lan-mouse)
- [macOS Privacy and Security Guide](https://support.apple.com/guide/mac-help/change-privacy-settings-on-mac-mh41285/mac)

## Quick Reference

```bash
# Check if capture backend is working
RUST_LOG=debug ./target/release/lan-mouse daemon | grep input_capture

# Expected output (success):
# [INFO input_capture] Successfully created capture backend: macos

# Expected output (failure):
# [ERROR input_capture] No input capture backend available. Tried: [MacOs]

# Restart daemon after granting permissions
pkill -f lan-mouse && ./target/release/lan-mouse daemon

# Check connection status
log show --predicate 'process == "lan-mouse"' --last 1m | grep -i "connecting\|connected"
```

## Summary

The "event tap creation failed" error is a macOS permission issue, not a network or certificate problem. The solution is simple:

1. **Grant Accessibility permission** to lan-mouse in System Settings
2. **Restart the daemon** for permissions to take effect
3. **Verify** the capture backend is working in logs

Once permissions are granted, lan-mouse will be able to detect mouse movement across screen edges and establish connections to remote machines.
