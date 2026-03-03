# Broken Pipe Error Fix Summary

## Problem

Users were encountering the following error when trying to connect lan-mouse daemons:

```
[ERROR] DTLS handshake failed with 15.1.30.54:4242: io error: Broken pipe (os error 32)
```

Even though:
- The IP address was correct and reachable
- Port 4242 was accessible
- The daemon was running on both machines

## Root Cause

The "Broken pipe (os error 32)" error occurs when the **remote daemon rejects the connection** during the DTLS handshake because the client's certificate fingerprint is **not in the remote daemon's `authorized_fingerprints` list**.

This is a certificate authorization issue, not a network connectivity issue.

## Solution

### 1. Created Comprehensive Troubleshooting Guide

**File:** [`BROKEN_PIPE_TROUBLESHOOTING.md`](BROKEN_PIPE_TROUBLESHOOTING.md)

A detailed guide that explains:
- What the "Broken pipe" error means
- How lan-mouse certificate authorization works
- Step-by-step instructions to exchange certificate fingerprints
- Complete configuration examples
- Common mistakes to avoid
- Technical details about the error

### 2. Enhanced Error Messages in Code

**Modified Files:**
- [`src/connect.rs`](src/connect.rs:98-111) - Client-side error handling
- [`src/listen.rs`](src/listen.rs:147-151) - Server-side error logging

#### Client-Side Improvements ([`src/connect.rs`](src/connect.rs:98-111))

When a "Broken pipe" error is detected, the daemon now provides specific guidance:

```
[ERROR] DTLS handshake failed with 15.1.30.54:4242: io error: Broken pipe (os error 32)
[ERROR] ==============================================
[ERROR] BROKEN PIPE ERROR - Certificate Authorization Issue
[ERROR] ==============================================
[ERROR] 
[ERROR] The remote daemon (15.1.30.54:4242) is rejecting your connection
[ERROR] because your certificate fingerprint is NOT in its
[ERROR] authorized_fingerprints list.
[ERROR] 
[ERROR] SOLUTION: Exchange certificate fingerprints between machines
[ERROR] 
[ERROR] Step 1: Extract YOUR certificate fingerprint:
[ERROR]   ./scripts/extract-fingerprint.sh
[ERROR] 
[ERROR] Step 2: Add YOUR fingerprint to REMOTE machine's config:
[ERROR]   On 15.1.30.54: Edit ~/.config/lan-mouse/config.toml
[ERROR]   Add to [authorized_fingerprints] section
[ERROR] 
[ERROR] Step 3: Extract REMOTE certificate fingerprint:
[ERROR]   ssh 15.1.30.54 './scripts/extract-fingerprint.sh'
[ERROR] 
[ERROR] Step 4: Add REMOTE fingerprint to YOUR machine's config:
[ERROR]   Edit ~/.config/lan-mouse/config.toml
[ERROR]   Add to [authorized_fingerprints] section
[ERROR] 
[ERROR] Step 5: Restart BOTH daemons:
[ERROR]   pkill -f 'lan-mouse.*daemon' && lan-mouse daemon
[ERROR] 
[ERROR] For detailed instructions, see: BROKEN_PIPE_TROUBLESHOOTING.md
[ERROR] ==============================================
```

#### Server-Side Improvements ([`src/listen.rs`](src/listen.rs:147-151))

When a connection is rejected due to unauthorized certificate, the server now logs:

```
[ERROR] ==============================================
[ERROR] CONNECTION REJECTED - Certificate Not Authorized
[ERROR] ==============================================
[ERROR] 
[ERROR] A connection attempt was REJECTED because the
[ERROR] certificate fingerprint is not in authorized_fingerprints:
[ERROR] 
[ERROR]   Fingerprint: aa:bb:cc:dd:ee:ff:00:11:22:33:44:55:66:77:88:99:aa:bb:cc:dd:ee:ff:00:11:22:33:44:55:66:77:88:99
[ERROR] 
[ERROR] To authorize this connection, add this fingerprint to
[ERROR] your config file (~/.config/lan-mouse/config.toml):
[ERROR] 
[ERROR]   [authorized_fingerprints]
[ERROR]   "aa:bb:cc:dd:ee:ff:00:11:22:33:44:55:66:77:88:99:aa:bb:cc:dd:ee:ff:00:11:22:33:44:55:66:77:88:99" = "client-name"
[ERROR] 
[ERROR] Then restart the daemon to apply the changes.
[ERROR] ==============================================
```

### 3. Updated Documentation

**Modified File:** [`README.md`](README.md:393-418)

Added references to the new troubleshooting guide in the main README:

- Updated the "Troubleshooting Connection Issues" section
- Added specific guidance for the "Broken pipe" error
- Listed all available troubleshooting guides

## How to Use These Improvements

### For Users Experiencing the Error

1. **Read the error message** - The daemon now provides clear guidance
2. **Follow the troubleshooting guide** - See [`BROKEN_PIPE_TROUBLESHOOTING.md`](BROKEN_PIPE_TROUBLESHOOTING.md)
3. **Exchange certificate fingerprints** - Add each machine's fingerprint to the other's config
4. **Restart both daemons** - Config changes only take effect after restart

### Quick Fix Steps

```bash
# Step 1: Extract your certificate fingerprint
./scripts/extract-fingerprint.sh

# Step 2: Add your fingerprint to remote machine's config
# On remote machine: Edit ~/.config/lan-mouse/config.toml
# Add to [authorized_fingerprints] section

# Step 3: Extract remote machine's certificate fingerprint
ssh <remote-ip> './scripts/extract-fingerprint.sh'

# Step 4: Add remote fingerprint to your machine's config
# Edit ~/.config/lan-mouse/config.toml
# Add to [authorized_fingerprints] section

# Step 5: Restart both daemons
pkill -f 'lan-mouse.*daemon' && lan-mouse daemon
```

## Benefits

1. **Clear Error Messages** - Users immediately understand what's wrong
2. **Actionable Guidance** - Step-by-step instructions in the error message
3. **Comprehensive Documentation** - Detailed guide with examples
4. **Server-Side Visibility** - Both client and server show helpful messages
5. **Reduced Support Burden** - Users can self-diagnose and fix the issue

## Technical Details

### Certificate Verification Process

The server-side verification (in [`src/listen.rs`](src/listen.rs:78-103)):

```rust
fn verify_peer_certificate(certs: &[Vec<u8>], _chains: &[CertificateDer]) -> Result<(), webrtc_dtls::Error> {
    // Generate fingerprint from client certificate
    let fingerprint = crypto::generate_fingerprint(&certs[0]);
    
    // Check if fingerprint is in authorized list
    if authorized_keys.read().contains_key(&fingerprint) {
        Ok(())  // Accept connection
    } else {
        Err(webrtc_dtls::Error::ErrVerifyDataMismatch)  // Reject connection
    }
}
```

### Why "Broken Pipe"?

The "Broken pipe (os error 32)" error occurs because:

1. Client initiates DTLS handshake with the remote daemon
2. Server receives the handshake request
3. Server checks the certificate fingerprint against `authorized_fingerprints`
4. Fingerprint is NOT found in the authorized list
5. Server **rejects the connection** by closing the socket
6. Client receives "Broken pipe" error because connection was closed unexpectedly

### Security Benefits

This mutual authorization provides:
- **Authentication:** Both parties verify each other's identity
- **Man-in-the-middle protection:** Only authorized certificates can connect
- **Access control:** You control exactly which machines can connect

## Related Files

- [`BROKEN_PIPE_TROUBLESHOOTING.md`](BROKEN_PIPE_TROUBLESHOOTING.md) - Comprehensive troubleshooting guide
- [`src/connect.rs`](src/connect.rs) - Client-side connection logic with improved error messages
- [`src/listen.rs`](src/listen.rs) - Server-side listener with improved rejection logging
- [`README.md`](README.md) - Updated main documentation
- [`docs/CONNECTION_TROUBLESHOOTING.md`](docs/CONNECTION_TROUBLESHOOTING.md) - General connection troubleshooting
- [`DTLS_HANDSHAKE_FIX.md`](DTLS_HANDSHAKE_FIX.md) - Linux to macOS certificate authorization
- [`scripts/extract-fingerprint.sh`](scripts/extract-fingerprint.sh) - Certificate fingerprint extraction tool
- [`scripts/diagnose-connection.sh`](scripts/diagnose-connection.sh) - Network diagnostic tool

## Testing

To test the improvements:

1. **Test the error message:**
   - Start a daemon on Machine A
   - Start a daemon on Machine B (without adding fingerprints)
   - Try to connect from A to B
   - Verify you see the enhanced error message

2. **Test the fix:**
   - Extract fingerprints from both machines
   - Add each fingerprint to the other's config
   - Restart both daemons
   - Verify connection succeeds

3. **Test the documentation:**
   - Follow the guide step-by-step
   - Verify all commands work
   - Check configuration examples

## Summary

These improvements transform a confusing error message into a clear, actionable guide that helps users quickly diagnose and fix certificate authorization issues. The enhanced error messages and comprehensive documentation significantly improve the user experience when setting up lan-mouse connections.
