# Quick Fix Steps - DTLS Handshake Error

## Current Situation

- ✅ macOS daemon is running
- ✅ Linux daemon is running
- ✅ Network connectivity works (ping succeeds)
- ✅ UDP port 4242 is open
- ❌ DTLS handshake fails: "Alert is Fatal or Close Notify"

**Root Cause:** macOS daemon is rejecting Linux daemon's connection because Linux daemon's certificate fingerprint is NOT in macOS config's `authorized_fingerprints` list.

## What I've Already Done

✅ Added macOS daemon's fingerprint to macOS config: `a9:04:19:bb:5a:d3:33:3e:02:1d:59:47:d3:a6:b0:7d:64:91:05:04:e5:7f:b5:56:19:4d:8e:5d:1f:7c:27:74`

## What You Need to Do

### Step 1: Get Linux Daemon's Certificate Fingerprint

**On the Linux machine** (where the Linux daemon is running), run:

```bash
cd /path/to/lan-mouse
./scripts/extract-fingerprint.sh
```

This will output something like:
```
Certificate fingerprint (lowercase, for config):
xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx
```

**Copy this fingerprint** - you'll need it in the next step.

### Step 2: Add Linux Fingerprint to macOS Config

**On the macOS machine**, edit the config file:

```bash
nano ~/.config/lan-mouse/config.toml
```

Add the Linux daemon's fingerprint to the `authorized_fingerprints` section:

```toml
[authorized_fingerprints]
"a9:04:19:bb:5a:d3:33:3e:02:1d:59:47:d3:a6:b0:7d:64:91:05:04:e5:7f:b5:56:19:4d:8e:5d:1f:7c:27:74" = "macos-pc007-2"
"xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx:xx" = "linux-pc007"
```

Replace `xx:xx:xx:...` with the actual fingerprint you got from Step 1.

Save and exit (Ctrl+O, Enter, Ctrl+X).

### Step 3: Restart macOS Daemon

**On the macOS machine**, stop and restart the daemon:

```bash
# Stop the daemon (Ctrl+C in the terminal where it's running)
# Then restart it:
lan-mouse daemon
```

### Step 4: Verify Connection

**On the Linux machine**, check the logs. You should see:

```
[INFO] connecting to 15.1.30.94:4242 ...
[INFO] Successfully established DTLS connection with 15.1.30.94:4242
[INFO] client (1) connected @ 15.1.30.94:4242
```

**On the macOS machine**, you should see:

```
[INFO] dtls client connected, ip: 15.1.30.93
```

## Troubleshooting

### Still Getting DTLS Error?

1. **Verify fingerprint is correct:**
   - Make sure you copied the entire fingerprint (all 32 hex bytes)
   - Check there are no typos or missing characters
   - Ensure it's in lowercase

2. **Check config syntax:**
   - Make sure fingerprint is in quotes: `"xx:xx:..."`
   - Ensure there's a description after the fingerprint
   - Verify the `[authorized_fingerprints]` section exists

3. **Restart both daemons:**
   - Config changes are only loaded on restart
   - Stop with Ctrl+C and restart both daemons

4. **Check for other fingerprints:**
   - If you have multiple clients, each needs its own fingerprint
   - All fingerprints should be in the same `authorized_fingerprints` section

## Example Final Config

**macOS config (`~/.config/lan-mouse/config.toml`):**
```toml
port = 4242
release_bind = ["KeyA", "KeyS", "KeyD", "KeyF"]

[authorized_fingerprints]
"a9:04:19:bb:5a:d3:33:3e:02:1d:59:47:d3:a6:b0:7d:64:91:05:04:e5:7f:b5:56:19:4d:8e:5d:1f:7c:27:74" = "macos-pc007-2"
"12:34:56:78:9a:bc:de:f0:12:34:56:78:9a:bc:de:f0:12:34:56:78:9a:bc:de:f0" = "linux-pc007"

[[clients]]
hostname = "linux-pc007"
ips = ["15.1.30.93"]
position = "right"
activate_on_startup = true
```

## Need Help?

- **Diagnostic script:** `./scripts/diagnose-connection.sh 15.1.30.94`
- **Detailed guide:** [`DTLS_HANDSHAKE_FIX.md`](DTLS_HANDSHAKE_FIX.md)
- **Connection troubleshooting:** [`docs/CONNECTION_TROUBLESHOOTING.md`](docs/CONNECTION_TROUBLESHOOTING.md)
