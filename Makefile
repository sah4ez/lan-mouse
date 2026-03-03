# Makefile for automated Linux cross-compilation builds on macOS
# This ensures X11 code compiles correctly without manual intervention

.PHONY: help build-linux-x11 build-linux-x11-amd64 build-linux-x11-arm64 clean check-x11 check-all-x11 test-x11

# Default target
help:
	@echo "Available targets:"
	@echo "  make build-linux-x11         - Build Linux binaries with X11 features (AMD64 + ARM64)"
	@echo "  make build-linux-x11-amd64   - Build Linux AMD64 with X11 features"
	@echo "  make build-linux-x11-arm64   - Build Linux ARM64 with X11 features"
	@echo "  make check-x11               - Check X11 code compiles (no binary output)"
	@echo "  make check-all-x11           - Check all X11-related packages compile"
	@echo "  make test-x11                - Run X11-specific tests"
	@echo "  make clean                   - Clean build artifacts"
	@echo ""
	@echo "Quick start:"
	@echo "  make build-linux-x11         - Build all Linux X11 binaries"

# Build all Linux X11 binaries
build-linux-x11: build-linux-x11-amd64 build-linux-x11-arm64
	@echo "✓ All Linux X11 builds completed successfully"

# Build Linux AMD64 with X11 features
build-linux-x11-amd64:
	@echo "Building Linux AMD64 with X11 features..."
	@cargo build --no-default-features --release --target x86_64-unknown-linux-gnu --features x11_capture,x11_emulation
	@echo "✓ Linux AMD64 X11 build completed"
	@ls -lh target/x86_64-unknown-linux-gnu/release/lan-mouse || true

# Build Linux ARM64 with X11 features
build-linux-x11-arm64:
	@echo "Building Linux ARM64 with X11 features..."
	@cargo build --no-default-features --release --target aarch64-unknown-linux-gnu --features x11_capture,x11_emulation
	@echo "✓ Linux ARM64 X11 build completed"
	@ls -lh target/aarch64-unknown-linux-gnu/release/lan-mouse || true

# Check X11 code compiles (faster than full build, uses current platform)
check-x11:
	@echo "Checking X11 code compilation (current platform)..."
	@cargo check --package input-capture --features x11
	@cargo check --package input-emulation --features x11
	@echo "✓ X11 code compilation check passed"

# Check all X11-related packages compile
check-all-x11:
	@echo "Checking all X11-related packages..."
	@cargo check --package input-capture --features x11
	@cargo check --package input-emulation --features x11
	@cargo check --no-default-features --features x11_capture,x11_emulation
	@echo "✓ All X11 packages compilation check passed"

# Run X11-specific tests
test-x11:
	@echo "Running X11-specific tests..."
	@cargo test --package input-capture --features x11 --lib
	@echo "✓ X11 tests completed"

# Clean build artifacts
clean:
	@echo "Cleaning build artifacts..."
	@cargo clean
	@echo "✓ Clean completed"

# Install required targets for cross-compilation
install-linux-targets:
	@echo "Installing Linux targets..."
	@rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu
	@echo "✓ Linux targets installed"

# Quick check (runs on every commit if pre-commit hook is installed)
quick-check:
	@echo "Quick X11 compilation check..."
	@cargo check --package input-capture --features x11
	@echo "✓ Quick check passed"
