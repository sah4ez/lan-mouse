# DTLS Handshake Error - Certificate Authorization Issue

## Problem

When trying to connect from Linux to macOS, you're getting:

```
[ERROR] DTLS handshake failed with 15.1.30.94:4242: Alert is Fatal or Close Notify
```

This error means the **macOS daemon is rejecting the connection** because the Linux daemon's certificate fingerprint is not in the macOS daemon's `authorized_fingerprints` list.

## Root Cause

lan-mouse uses DTLS (TLS over UDP) for encrypted communication. Each daemon has its own certificate, and connections are only accepted if the remote daemon's certificate fingerprint is in the local daemon's `authorized_fingerprints` configuration.

## Solution

You need to exchange certificate fingerprints between the two machines:

### Step 1: Extract Linux Daemon's Certificate Fingerprint

On the **Linux machine** (where the daemon is running), run:

```bash
./scripts/extract-fingerprint.sh
```

This will output something like:
```
Certificate fingerprint (lowercase, for config):
xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx
```

### Step 2: Add Linux Fingerprint to macOS Config

On the **macOS machine**, edit `~/.config/lan-mouse/config.toml` and add the Linux daemon's fingerprint:

```toml
[authorized_fingerprints]
"xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx" = "linux-pc007"
```

### Step 3: Extract macOS Daemon's Certificate Fingerprint

On the **macOS machine**, run:

```bash
./scripts/extract-fingerprint.sh
```

This will output the macOS daemon's fingerprint.

### Step 4: Add macOS Fingerprint to Linux Config

On the **Linux machine**, edit `~/.config/lan-mouse/config.toml` and add the macOS daemon's fingerprint:

```toml
[authorized_fingerprints]
"yy:yy:yy:yy:yy:yy:yy:yy:yy:yy:yy:yy:yy:yy:yy:yy:yy:yy:yy:yy:yy:yy:yy:yy:yy" = "macos-pc007-2"
```

### Step 5: Restart Both Daemons

Restart both daemons to load the new configuration:

**On Linux:**
```bash
# Stop the daemon (Ctrl+C)
RUST_LOG=debug ./target/release/lan-mouse daemon
```

**On macOS:**
```bash
# Stop the daemon (Ctrl+C)
RUST_LOG=debug lan-mouse daemon
```

## Example Configuration

Here's a complete example with both fingerprints:

**Linux config (`~/.config/lan-mouse/config.toml`):**
```toml
port = 4242
release_bind = ["KeyA", "KeyS", "KeyD", "KeyF"]

[authorized_fingerprints]
"a9:04:19:bb:5a:d3:33:3e:02:1d:59:47:d3:a6:b0:7d:64:91:05:04:e5:7f:b5:56:19:4d:8e:5d:1f:7c:27:74" = "macos-pc007-2"

[[clients]]
hostname = "macos-pc007-2"
ips = ["15.1.30.94"]
position = "left"
activate_on_startup = true
```

**macOS config (`~/.config/lan-mouse/config.toml`):**
```toml
port = 4242
release_bind = ["KeyA", "KeyS", "KeyD", "KeyF"]

[authorized_fingerprints]
"xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx" = "linux-pc007"

[[clients]]
hostname = "linux-pc007"
ips = ["15.1.30.93"]
position = "right"
activate_on_startup = true
```

## Verification

After restarting both daemons, you should see:

**On Linux daemon:**
```
[INFO] connecting to 15.1.30.94:4242 ...
[INFO] Successfully established DTLS connection with 15.1.30.94:4242
[INFO] client (1) connected @ 15.1.30.94:4242
```

**On macOS daemon:**
```
[INFO] dtls client connected, ip: 15.1.30.93
```

## Troubleshooting

If you still get DTLS handshake errors:

1. **Verify fingerprints are correct:**
   - Run `./scripts/extract-fingerprint.sh` on both machines
   - Compare the output with what's in the config files

2. **Check config syntax:**
   - Make sure the fingerprint format is correct (lowercase, colons)
   - Ensure there are no extra spaces or quotes

3. **Restart daemons:**
   - Config changes are only loaded on daemon restart
   - Use `Ctrl+C` to stop and restart the daemon

4. **Check logs:**
   - Run with `RUST_LOG=debug` for detailed error messages
   - Look for specific DTLS error details

## Additional Notes

- **DNS resolution error:** The "could not resolve pc007-2" error is not critical since you have the IP address configured. You can ignore it or remove the `hostname` field and only use `ips`.

- **Certificate regeneration:** If you regenerate certificates, you'll need to update the fingerprints on both machines.

- **Multiple clients:** Each client needs its own fingerprint in the `authorized_fingerprints` section.

## Related Tools

- **Diagnostic script:** `./scripts/diagnose-connection.sh` - Tests network connectivity
- **Fingerprint extraction:** `./scripts/extract-fingerprint.sh` - Extracts certificate fingerprint
- **Troubleshooting guide:** `docs/CONNECTION_TROUBLESHOOTING.md` - Comprehensive guide
