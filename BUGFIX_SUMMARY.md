# Bug Fixes for lan-mouse X11 Backend Issues

## Issues Fixed

### 1. X11 Input Capture Backend Failure (Linux)

**Problem:**
The X11 input capture backend was failing to initialize with error "no backend available".

**Root Cause:**
In [`input-capture/src/x11.rs`](input-capture/src/x11.rs:183), the `XRecordCreateContext` function was being called incorrectly. The third parameter was being cast to `*mut *mut xrecord::XRecordClientInfo` instead of being passed as an array of `XRecordRange` pointers.

**Fix:**
Changed the code to properly create an array of `XRecordRange` pointers and pass it to `XRecordCreateContext`:

```rust
// Before (incorrect):
let context = xrecord::XRecordCreateContext(
    display,
    0,
    &mut record_range as *mut _ as *mut *mut xrecord::XRecordClientInfo,
    1,
);

// After (correct):
let mut ranges: [*mut xrecord::XRecordRange; 1] = [record_range];
let context = xrecord::XRecordCreateContext(
    display,
    0, // XRecordAllClients
    ranges.as_mut_ptr(),
    1,
);
```

### 2. Input Emulation Error Message Typo

**Problem:**
The error message for "no backend available" in input emulation was incorrectly showing as "capture error".

**Root Cause:**
In [`input-emulation/src/error.rs`](input-emulation/src/error.rs:66), the error message was wrong.

**Fix:**
Changed the error message from "capture error" to "no backend available":

```rust
// Before:
#[error("capture error")]
NoAvailableBackend,

// After:
#[error("no backend available")]
NoAvailableBackend,
```

### 3. DTLS Handshake Failure (macOS to Linux)

**Problem:**
DTLS handshake was failing with "Alert is Fatal or Close Notify" when connecting from macOS to Linux.

**Root Cause:**
This was a secondary issue caused by the X11 backends failing to initialize on Linux. When the input emulation backend fails, the server cannot properly accept incoming connections.

**Fix:**
This should be resolved by fixing the X11 capture backend initialization issue above. Additionally, users need to ensure that:

1. The client's certificate fingerprint is added to the server's authorized keys list
2. The server is running and the X11 backends are properly initialized

## Testing Instructions

### For Linux (X11):

1. Rebuild the project:
   ```bash
   cargo build --release
   ```

2. Run the daemon:
   ```bash
   RUST_LOG=debug ./target/release/lan-mouse daemon
   ```

3. Verify that X11 backends are initializing correctly:
   - You should see: "Successfully created capture backend: X11"
   - You should see: "Successfully created emulation backend: X11"

### For macOS to Linux Connection:

1. Get the macOS client's certificate fingerprint:
   ```bash
   ./target/release/lan-mouse fingerprint
   ```

2. Add the fingerprint to the Linux server's authorized keys:
   - Use the GUI or edit the config file to add the fingerprint

3. Restart the Linux daemon:
   ```bash
   ./target/release/lan-mouse daemon
   ```

4. Connect from macOS:
   ```bash
   ./target/release/lan-mouse daemon
   ```

## Technical Details

### XRecord Extension

The XRecord extension is required for input capture on X11. It allows applications to record X11 events (keyboard, mouse, etc.) from other applications. The extension must be available on the X server for the X11 backend to work.

### XTest Extension

The XTest extension is used for input emulation on X11. It allows applications to synthesize input events (keyboard, mouse, etc.) as if they came from a real input device.

### DTLS Handshake

The application uses DTLS (Datagram Transport Layer Security) for secure communication between clients and servers. The handshake requires:
- Both client and server to have valid certificates
- The client's certificate fingerprint to be in the server's authorized keys list
- Compatible DTLS configuration between client and server

## Files Modified

1. [`input-capture/src/x11.rs`](input-capture/src/x11.rs:165-200) - Fixed XRecordCreateContext call
2. [`input-emulation/src/error.rs`](input-emulation/src/error.rs:66) - Fixed error message typo

## Additional Notes

- The X11 backend is only available on Unix systems (not macOS)
- On Wayland, the application will try Wayland-compatible backends first, then fall back to X11 via XWayland
- If XRecord extension is not available, the application will try other backends (libei, layer-shell)
- Certificate fingerprints are SHA-256 hashes of the DER-encoded certificates
