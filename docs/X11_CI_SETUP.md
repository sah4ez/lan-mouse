# Automated X11 CI on macOS

This document describes the automated build pipeline for cross-compiling Linux builds with X11 features on macOS. This ensures that X11 code compiles correctly on Linux without requiring manual testing on a Linux system.

## Overview

The automated X11 CI system provides:

1. **Cross-compilation from macOS to Linux** - Build Linux binaries (AMD64 and ARM64) with X11 features
2. **Pre-commit hooks** - Automatically check X11 compilation before committing changes
3. **Makefile targets** - Easy-to-use commands for building and testing X11 code
4. **Automated validation** - Ensure X11 code compiles correctly on every change

## Quick Start

### 1. Install the CI System

Run the setup script to install all necessary components:

```bash
./scripts/setup-x11-ci.sh
```

This will:
- Install Linux targets for cross-compilation
- Install the pre-commit hook
- Make all scripts executable
- Check for cross-compilation tools

### 2. Quick Compilation Check

For a fast check that verifies X11 code compiles (recommended for daily development):

```bash
make check-x11
```

This checks the X11 code on your current platform without requiring cross-compilation tools.

### 3. Build Linux X11 Binaries

Build all Linux binaries with X11 features (requires cross-compilation tools):

```bash
make build-linux-x11
```

This will create binaries in:
- `target/x86_64-unknown-linux-gnu/release/lan-mouse`
- `target/aarch64-unknown-linux-gnu/release/lan-mouse`

**Note:** Building actual Linux binaries requires cross-compilation toolchains. See [Cross-compilation Setup](#cross-compilation-setup) for details.

## Available Commands

### Makefile Targets

| Command | Description | Requires Cross-compilation Tools |
|---------|-------------|-------------------------------|
| `make check-x11` | Quick X11 compilation check (current platform) | No |
| `make check-all-x11` | Check all X11-related packages compile | No |
| `make build-linux-x11` | Build all Linux X11 binaries (AMD64 + ARM64) | Yes |
| `make build-linux-x11-amd64` | Build Linux AMD64 with X11 features | Yes |
| `make build-linux-x11-arm64` | Build Linux ARM64 with X11 features | Yes |
| `make test-x11` | Run X11-specific tests | No |
| `make clean` | Clean build artifacts | No |
| `make install-linux-targets` | Install Linux targets for cross-compilation | No |
| `make quick-check` | Quick check (used by pre-commit hook) | No |

### Shell Scripts

#### `scripts/build-linux-x11.sh`

Comprehensive build script that:
- Installs Linux targets if needed
- Checks for cross-compilation tools
- Builds for both AMD64 and ARM64
- Verifies binary creation
- Provides detailed output

Usage:
```bash
./scripts/build-linux-x11.sh
```

#### `scripts/pre-commit-x11-check.sh`

Pre-commit hook that:
- Checks if X11-related files were modified
- Runs X11 compilation checks only when needed
- Prevents commits with broken X11 code
- Provides clear error messages

This hook is automatically installed by the setup script.

#### `scripts/setup-x11-ci.sh`

Setup script that:
- Installs pre-commit hook
- Makes scripts executable
- Installs Linux targets
- Checks for cross-compilation tools
- Provides setup summary

Usage:
```bash
./scripts/setup-x11-ci.sh
```

## Pre-commit Hook

The pre-commit hook automatically checks X11 compilation when you commit changes to X11-related files.

### How It Works

1. Detects if X11-related files are staged for commit
2. If X11 files are modified, runs compilation checks
3. Prevents commits if compilation fails
4. Only runs when necessary (fast for non-X11 changes)

### X11-related Files

The hook checks for changes to:
- Files matching `x11.rs` pattern
- Files in `input-capture` directory
- Files in `input-emulation` directory

### Disabling the Hook (Temporarily)

If you need to skip the pre-commit check:

```bash
git commit --no-verify
```

## Cross-compilation Setup

### Required Targets

The following Rust targets are required:

```bash
rustup target add x86_64-unknown-linux-gnu
rustup target add aarch64-unknown-linux-gnu
```

### Cross-compilation Tools (Optional)

For actual binary creation, you may need cross-compilation toolchains:

```bash
# For AMD64
brew install x86_64-linux-gnu-gcc

# For ARM64
brew install aarch64-linux-gnu-gcc
```

**Note:** The `cargo check` command works without these tools, but `cargo build` requires them for actual binary creation.

## CI/CD Integration

### GitHub Actions

The project already includes GitHub Actions workflows that build for Linux:

- `.github/workflows/rust.yml` - Runs on every push/PR with `--all-features`
- `.github/workflows/multiplatform-build.yml` - Builds for multiple platforms including Linux with X11

The local CI system complements these by providing fast, local validation before pushing.

### Workflow

1. **Local Development** - Use `make check-x11` for quick validation
2. **Pre-commit** - Automatic check before committing
3. **Push** - GitHub Actions run full CI/CD pipeline
4. **Pull Request** - All checks must pass before merging

## Troubleshooting

### Build Fails with "target not installed"

Install the required targets:

```bash
make install-linux-targets
```

### Pre-commit Hook Not Running

Check if the hook is installed:

```bash
ls -la .git/hooks/pre-commit
```

If missing, run the setup script:

```bash
./scripts/setup-x11-ci.sh
```

### Cross-compilation Errors

If you encounter cross-compilation errors when building Linux binaries:

1. Ensure cross-compilation tools are installed (see [Cross-compilation Setup](#cross-compilation-setup))
2. Check that the target is installed: `rustup target list --installed`
3. Try a simple check first: `make check-x11` (doesn't require cross-compilation tools)

**Note:** For daily development, use `make check-x11` which doesn't require cross-compilation tools and is much faster.

### Permission Denied on Scripts

Make scripts executable:

```bash
chmod +x scripts/*.sh
```

## Best Practices

1. **Use quick checks for daily development** - `make check-x11` is fast and doesn't require cross-compilation tools
2. **Run checks before pushing** - Use `make check-x11` to catch issues early
3. **Test on actual Linux** - Periodically test binaries on a real Linux system using `make build-linux-x11`
4. **Keep hooks updated** - Re-run setup script after pulling changes
5. **Review CI logs** - Check GitHub Actions logs for any issues
6. **Use pre-commit hooks** - They automatically check X11 code before committing

## Advanced Usage

### Building Specific Features

Build with specific feature combinations:

```bash
# X11 capture only
cargo build --no-default-features --features x11_capture

# X11 emulation only
cargo build --no-default-features --features x11_emulation

# Both X11 features
cargo build --no-default-features --features x11_capture,x11_emulation
```

### Custom Build Script

You can create custom build scripts using the existing scripts as templates:

```bash
#!/bin/bash
# Custom build script
cargo check --package input-capture --features x11
cargo check --package input-emulation --features x11
cargo build --no-default-features --features x11_capture,x11_emulation
```

## Contributing

When contributing to X11-related code:

1. Run `make check-x11` before committing
2. Test on a Linux system if possible
3. Ensure all CI checks pass
4. Update documentation if needed

## Support

For issues or questions:

- Check the main [BUILD.md](../BUILD.md) for general build instructions
- Review GitHub Actions logs for CI failures
- Open an issue on GitHub for persistent problems

## Summary

The automated X11 CI system provides:

✅ Fast local validation of X11 code
✅ Automatic pre-commit checks
✅ Cross-compilation from macOS to Linux
✅ Integration with existing CI/CD
✅ Easy-to-use commands
✅ Comprehensive documentation

This ensures that X11 code compiles correctly on Linux without requiring manual testing on a Linux system for every change.
