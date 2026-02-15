# Multi-Platform Build Guide

This document provides comprehensive instructions for building the lan-mouse project on multiple platforms.

## Supported Platforms

- **macOS Apple Silicon** (`aarch64-apple-darwin`)
- **macOS Intel** (`x86_64-apple-darwin`)
- **Linux AMD64** (`x86_64-unknown-linux-gnu`)
- **Linux ARM64** (`aarch64-unknown-linux-gnu`)

## Quick Start

### Local Build (Current Platform)

```bash
# Build without default features (recommended for cross-platform compatibility)
cargo build --no-default-features --release

# Build with specific features
cargo build --release --features x11_capture,x11_emulation
```

### Cross-Platform Build

```bash
# Run the multi-platform validation script
./scripts/validate-build.sh
```

## Build Features

The project supports multiple features that can be enabled/disabled:

| Feature | Description | Platform |
|---------|-------------|----------|
| `gtk` | GTK GUI interface | Linux, macOS |
| `layer_shell_capture` | Wayland layer shell capture | Linux |
| `x11_capture` | X11 input capture | Linux |
| `libei_capture` | libei input capture | Linux |
| `wlroots_emulation` | wlroots input emulation | Linux |
| `libei_emulation` | libei input emulation | Linux |
| `rdp_emulation` | Remote Desktop Portal emulation | Linux |
| `x11_emulation` | X11 input emulation | Linux |

## Platform-Specific Instructions

### macOS

#### Prerequisites

```bash
# Install Homebrew (if not already installed)
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Install dependencies
brew install gtk4 libadwaita imagemagick
```

#### Building for Apple Silicon

```bash
# Install target
rustup target add aarch64-apple-darwin

# Build
cargo build --no-default-features --release --target aarch64-apple-darwin
```

#### Building for Intel

```bash
# Install target
rustup target add x86_64-apple-darwin

# Build
cargo build --no-default-features --release --target x86_64-apple-darwin
```

#### Universal Binary

```bash
# Build both architectures
cargo build --no-default-features --release --target aarch64-apple-darwin
cargo build --no-default-features --release --target x86_64-apple-darwin

# Create universal binary
lipo -create -output target/release/lan-mouse-universal \
  target/aarch64-apple-darwin/release/lan-mouse \
  target/x86_64-apple-darwin/release/lan-mouse
```

### Linux

#### Prerequisites (Debian/Ubuntu)

```bash
sudo apt-get update
sudo apt-get install -y \
  libx11-dev \
  libxtst-dev \
  libadwaita-1-dev \
  libgtk-4-dev \
  pkg-config
```

#### Prerequisites (Fedora)

```bash
sudo dnf install -y \
  libX11-devel \
  libXtst-devel \
  libadwaita-devel \
  gtk4-devel \
  pkg-config
```

#### Building for AMD64

```bash
# Install target
rustup target add x86_64-unknown-linux-gnu

# Build
cargo build --no-default-features --release --target x86_64-unknown-linux-gnu
```

#### Building for ARM64

```bash
# Install target
rustup target add aarch64-unknown-linux-gnu

# Install cross-compilation tools
sudo apt-get install gcc-aarch64-linux-gnu

# Build
cargo build --no-default-features --release --target aarch64-unknown-linux-gnu
```

### Cross-Compilation from macOS to Linux

Cross-compilation from macOS to Linux requires additional setup:

```bash
# Install cross-compilation tools
brew install x86_64-linux-gnu-gcc aarch64-linux-gnu-gcc

# Set linker environment variables
export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
export CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++
export AR_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc-ar
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc

# Build
cargo build --no-default-features --release --target aarch64-unknown-linux-gnu
```

## Build Validation

### Automated Validation Script

The project includes a comprehensive validation script that:

1. Validates Rust file syntax
2. Runs unit tests
3. Builds for the current platform
4. Builds for all supported platforms (when possible)
5. Validates build artifacts

```bash
./scripts/validate-build.sh
```

### Manual Validation

#### Check Syntax

```bash
# Check all targets without default features
cargo check --no-default-features --all-targets

# Check with specific features
cargo check --all-targets --features x11_capture,x11_emulation
```

#### Run Tests

```bash
# Run tests without default features
cargo test --no-default-features --lib

# Run tests with specific features
cargo test --lib --features x11_capture,x11_emulation
```

#### Lint with Clippy

```bash
# Lint without default features
cargo clippy --no-default-features --all-targets -- -D warnings

# Lint with specific features
cargo clippy --all-targets --features x11_capture,x11_emulation -- -D warnings
```

## Continuous Integration

The project uses GitHub Actions for automated builds:

### Workflow: Multi-platform Build

Location: `.github/workflows/multiplatform-build.yml`

This workflow:

1. Validates syntax and runs tests
2. Builds for Linux AMD64
3. Builds for Linux ARM64
4. Builds for macOS Apple Silicon
5. Builds for macOS Intel
6. Uploads build artifacts

### Workflow: Rust CI

Location: `.github/workflows/rust.yml`

This workflow runs:

- Formatting checks (`cargo fmt --check`)
- Build checks
- Linting with Clippy
- Tests

## Troubleshooting

### Common Issues

#### Issue: `libadwaita-1` not found on macOS

**Solution:**
```bash
brew install libadwaita
```

#### Issue: Architecture mismatch on macOS

**Error:**
```
ld: warning: ignoring file '/usr/local/Cellar/libgit2/1.9.2_1/lib/libgit2.dylib': 
found architecture 'x86_64', required architecture 'arm64'
```

**Solution:** Reinstall libgit2 for the correct architecture:
```bash
brew reinstall libgit2
```

#### Issue: Cross-compilation linker errors

**Solution:** Install the appropriate cross-compilation toolchain:
```bash
# For ARM64 on Linux
sudo apt-get install gcc-aarch64-linux-gnu

# For AMD64 on ARM64 Linux
sudo apt-get install gcc-x86-64-linux-gnu
```

#### Issue: X11 development headers not found

**Solution:**
```bash
# Debian/Ubuntu
sudo apt-get install libx11-dev libxtst-dev

# Fedora
sudo dnf install libX11-devel libXtst-devel
```

## Build Artifacts

After building, binaries are located in:

```
target/<target-triple>/release/lan-mouse
```

Example paths:
- `target/x86_64-unknown-linux-gnu/release/lan-mouse`
- `target/aarch64-unknown-linux-gnu/release/lan-mouse`
- `target/x86_64-apple-darwin/release/lan-mouse`
- `target/aarch64-apple-darwin/release/lan-mouse`

## Release Builds

For release builds, the following profile settings are used:

```toml
[profile.release]
codegen-units = 1
lto = "fat"
strip = true
panic = "abort"
```

This produces optimized, stripped binaries with minimal size.

## Development Builds

For development builds with debug information:

```bash
cargo build --no-default-features
```

Debug builds are located in:
```
target/<target-triple>/debug/lan-mouse
```

## Additional Resources

- [Rust Cross-Compilation Guide](https://rust-lang.github.io/rustup/cross-compilation.html)
- [Cargo Book - Building for Multiple Targets](https://doc.rust-lang.org/cargo/guide/building-for-more-platforms.html)
- [Project README](README.md)
- [Project Documentation](DOC.md)

## Contributing

When contributing to the project:

1. Always run the validation script before submitting PRs
2. Ensure code passes `cargo fmt --check`
3. Ensure code passes `cargo clippy` with `-D warnings`
4. Run tests on all platforms you have access to
5. Document any platform-specific changes

## Support

For build-related issues:

1. Check this documentation
2. Check existing GitHub Issues
3. Create a new issue with:
   - Platform and architecture
   - Rust version (`rustc --version`)
   - Cargo version (`cargo --version`)
   - Full error output
   - Steps to reproduce
