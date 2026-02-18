# X11 Automated Build Pipeline - Quick Reference

## What is this?

An automated build system that ensures X11 code compiles correctly on Linux without requiring manual testing on a Linux system. Works from macOS with automatic pre-commit checks.

## Quick Start

### 1. Setup (one-time)

```bash
./scripts/setup-x11-ci.sh
```

### 2. Daily Development

```bash
# Quick check (fast, no cross-compilation tools needed)
make check-x11

# Or let pre-commit hook do it automatically
git commit -m "your changes"
```

### 3. Build Linux Binaries (optional, requires cross-compilation tools)

```bash
make build-linux-x11
```

## Key Commands

| Command | What it does | Speed | Cross-tools needed |
|---------|--------------|-------|-------------------|
| `make check-x11` | Checks X11 code compiles | ⚡ Fast | ❌ No |
| `make check-all-x11` | Checks all X11 packages | ⚡ Fast | ❌ No |
| `make build-linux-x11` | Builds Linux binaries | 🐢 Slow | ✅ Yes |
| `make test-x11` | Runs X11 tests | ⚡ Fast | ❌ No |

## How It Works

1. **Pre-commit Hook** - Automatically checks X11 code before you commit
2. **Quick Check** - Verifies compilation on your current platform (macOS)
3. **Cross-compilation** - Optional: builds actual Linux binaries for testing

## Why This Matters

- ✅ Catches X11 compilation errors early
- ✅ No need for Linux VM for every change
- ✅ Automatic validation on every commit
- ✅ Fast checks for daily development
- ✅ Optional full builds when needed

## Files Created

- `Makefile` - Easy-to-use build commands
- `scripts/build-linux-x11.sh` - Comprehensive Linux build script
- `scripts/pre-commit-x11-check.sh` - Automatic pre-commit validation
- `scripts/setup-x11-ci.sh` - One-time setup script
- `docs/X11_CI_SETUP.md` - Full documentation

## Troubleshooting

**Quick check fails?**
```bash
make check-x11
```

**Need actual Linux binaries?**
```bash
# Install cross-compilation tools
brew install x86_64-linux-gnu-gcc aarch64-linux-gnu-gcc

# Build binaries
make build-linux-x11
```

**Pre-commit hook not working?**
```bash
./scripts/setup-x11-ci.sh
```

## More Information

See [docs/X11_CI_SETUP.md](docs/X11_CI_SETUP.md) for detailed documentation.

## Summary

- Use `make check-x11` for daily development (fast, no cross-tools needed)
- Pre-commit hook automatically validates X11 code
- Optional: `make build-linux-x11` for actual Linux binaries
- All X11 code is validated before it reaches CI/CD
