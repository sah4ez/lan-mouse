# DTLS Handshake "Broken Pipe" Error Troubleshooting

## Problem

You're seeing this error when trying to connect:

```
[ERROR] DTLS handshake failed with 15.1.30.54:4242: io error: Broken pipe (os error 32)
```

## Root Cause

The "Broken pipe (os error 32)" error during DTLS handshake means the **remote daemon is actively rejecting your connection** because your certificate fingerprint is **not in the remote daemon's `authorized_fingerprints` list**.

Even though:
- ✓ The IP address is correct and reachable
- ✓ Port 4242 is accessible
- ✓ The daemon is running on both machines

The connection is still rejected because of **certificate authorization failure**.

## How lan-mouse Certificate Authorization Works

lan-mouse uses DTLS (TLS over UDP) for encrypted communication. Each daemon generates its own certificate on first run. For two daemons to connect:

1. **Machine A** must have **Machine B's** certificate fingerprint in its `authorized_fingerprints` list
2. **Machine B** must have **Machine A's** certificate fingerprint in its `authorized_fingerprints` list

This is a **mutual authorization** requirement - both sides must authorize each other.

## Solution: Exchange Certificate Fingerprints

### Step 1: Extract Certificate Fingerprint from Machine A

On the machine that's trying to connect (the one showing the error), run:

```bash
./scripts/extract-fingerprint.sh
```

You'll see output like:
```
Certificate fingerprint (lowercase, for config):
aa:bb:cc:dd:ee:ff:00:11:22:33:44:55:66:77:88:99:aa:bb:cc:dd:ee:ff:00:11:22:33:44:55:66:77:88:99
```

**Copy this fingerprint** - you'll need it for the remote machine's config.

### Step 2: Extract Certificate Fingerprint from Machine B (Remote)

On the remote machine (15.1.30.54 in your case), run:

```bash
./scripts/extract-fingerprint.sh
```

**Copy this fingerprint** as well.

### Step 3: Add Machine A's Fingerprint to Machine B's Config

On the **remote machine** (15.1.30.54), edit the config file:

**Linux/macOS:** `~/.config/lan-mouse/config.toml`

Add the fingerprint from Machine A:

```toml
[authorized_fingerprints]
"aa:bb:cc:dd:ee:ff:00:11:22:33:44:55:66:77:88:99:aa:bb:cc:dd:ee:ff:00:11:22:33:44:55:66:77:88:99" = "machine-a-name"
```

### Step 4: Add Machine B's Fingerprint to Machine A's Config

On **your local machine**, edit the config file:

**Linux/macOS:** `~/.config/lan-mouse/config.toml`

Add the fingerprint from Machine B:

```toml
[authorized_fingerprints]
"xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx" = "machine-b-name"
```

### Step 5: Restart Both Daemons

**Important:** Configuration changes only take effect after restarting the daemon.

**On Machine A:**
```bash
# Stop the daemon (Ctrl+C if running)
# Then restart:
lan-mouse daemon
```

**On Machine B (15.1.30.54):**
```bash
# Stop the daemon (Ctrl+C if running)
# Then restart:
lan-mouse daemon
```

### Step 6: Verify Connection

After restarting both daemons, you should see successful connection logs:

**On Machine A:**
```
[INFO] connecting to 15.1.30.54:4242 ...
[INFO] Successfully established DTLS connection with 15.1.30.54:4242
[INFO] client (1) connected @ 15.1.30.54:4242
```

**On Machine B:**
```
[INFO] dtls client connected, ip: <your-ip>
```

## Complete Configuration Example

**Machine A config (`~/.config/lan-mouse/config.toml`):**
```toml
port = 4242
release_bind = ["KeyA", "KeyS", "KeyD", "KeyF"]

[authorized_fingerprints]
"xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx" = "machine-b"

[[clients]]
hostname = "machine-b"
ips = ["15.1.30.54"]
position = "right"
activate_on_startup = true
```

**Machine B config (`~/.config/lan-mouse/config.toml`):**
```toml
port = 4242
release_bind = ["KeyA", "KeyS", "KeyD", "KeyF"]

[authorized_fingerprints]
"aa:bb:cc:dd:ee:ff:00:11:22:33:44:55:66:77:88:99:aa:bb:cc:dd:ee:ff:00:11:22:33:44:55:66:77:88:99" = "machine-a"

[[clients]]
hostname = "machine-a"
ips = ["<your-ip>"]
position = "left"
activate_on_startup = true
```

## Troubleshooting If It Still Doesn't Work

### 1. Verify Fingerprints Are Correct

Run the extraction script on both machines again and compare with what's in the config files:

```bash
# On both machines
./scripts/extract-fingerprint.sh
```

Make sure:
- The fingerprints in the config files match exactly
- No extra spaces or quotes
- All lowercase letters
- Colons between each pair of characters

### 2. Check Config File Syntax

Make sure the config file is valid TOML:

```bash
# Check for syntax errors
cat ~/.config/lan-mouse/config.toml
```

Common mistakes:
- Missing quotes around the fingerprint
- Extra spaces after commas
- Invalid TOML syntax

### 3. Verify Daemon Restarted

Make sure you actually restarted the daemons after editing the config:

```bash
# Check if daemon is running
pgrep -f "lan-mouse.*daemon"

# If not, start it
lan-mouse daemon
```

### 4. Check Logs with Debug Output

Run the daemon with debug logging to see detailed connection attempts:

```bash
RUST_LOG=debug lan-mouse daemon
```

Look for:
- Certificate verification errors
- Fingerprint mismatch messages
- Connection rejection reasons

### 5. Run Diagnostic Script

Use the diagnostic script to verify network connectivity:

```bash
./scripts/diagnose-connection.sh 15.1.30.54
```

This will check:
- Local daemon status
- Network connectivity
- Port accessibility
- Firewall status

## Common Mistakes

### ❌ Only Adding One Fingerprint

**Wrong:** Only adding the remote machine's fingerprint to your local config

**Correct:** Add BOTH fingerprints to BOTH config files (mutual authorization)

### ❌ Not Restarting Daemon

**Wrong:** Editing config but not restarting the daemon

**Correct:** Always restart the daemon after editing `config.toml`

### ❌ Incorrect Fingerprint Format

**Wrong:**
```toml
[authorized_fingerprints]
"AA:BB:CC:DD:EE:FF" = "remote"  # Wrong - uppercase
"aabbccddeeff" = "remote"        # Wrong - missing colons
"a b b c c d d" = "remote"       # Wrong - spaces
```

**Correct:**
```toml
[authorized_fingerprints]
"aa:bb:cc:dd:ee:ff" = "remote"  # Correct - lowercase with colons
```

### ❌ Using Wrong Config File

**Wrong:** Editing `/etc/lan-mouse/config.toml` or another location

**Correct:** Edit `~/.config/lan-mouse/config.toml` (user-specific config)

## Technical Details

### Why "Broken Pipe"?

The "Broken pipe (os error 32)" error occurs because:

1. Your daemon initiates DTLS handshake with the remote daemon
2. The remote daemon receives the handshake request
3. The remote daemon checks your certificate fingerprint against its `authorized_fingerprints` list
4. The fingerprint is NOT found in the authorized list
5. The remote daemon **rejects the connection** by closing the socket
6. Your daemon receives a "Broken pipe" error because the connection was closed unexpectedly

### Certificate Verification Process

The server-side verification (in `src/listen.rs`):

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

### Security Benefits

This mutual authorization provides:
- **Authentication:** Both parties verify each other's identity
- **Man-in-the-middle protection:** Only authorized certificates can connect
- **Access control:** You control exactly which machines can connect

## Additional Resources

- [Main README](README.md) - General setup and configuration
- [DTLS Handshake Fix](DTLS_HANDSHAKE_FIX.md) - Similar issue with "Alert is Fatal" error
- [Connection Troubleshooting](docs/CONNECTION_TROUBLESHOOTING.md) - General connection issues
- [Diagnostic Script](scripts/diagnose-connection.sh) - Network diagnostic tool

## Quick Reference

```bash
# Extract fingerprint
./scripts/extract-fingerprint.sh

# Edit config
nano ~/.config/lan-mouse/config.toml

# Restart daemon
pkill -f "lan-mouse.*daemon"
lan-mouse daemon

# Run diagnostics
./scripts/diagnose-connection.sh <remote-ip>

# Check logs with debug
RUST_LOG=debug lan-mouse daemon
```
