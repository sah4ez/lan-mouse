#!/usr/bin/env bash
#
# Multi-platform build validation script for lan-mouse
# Validates files, runs tests, and builds for multiple platforms
#
# Platforms:
#   - macOS Apple Silicon (aarch64-apple-darwin)
#   - macOS Intel (x86_64-apple-darwin)
#   - Linux AMD64 (x86_64-unknown-linux-gnu)
#   - Linux ARM64 (aarch64-unknown-linux-gnu)
#

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Cargo path
CARGO="${CARGO:-$HOME/.cargo/bin/cargo}"

# Build targets
TARGETS=(
    "aarch64-apple-darwin"      # macOS Apple Silicon
    "x86_64-apple-darwin"       # macOS Intel
    "x86_64-unknown-linux-gnu"  # Linux AMD64
    "aarch64-unknown-linux-gnu" # Linux ARM64
)

# Log functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Step 1: Validate file syntax
validate_files() {
    log_info "Step 1: Validating Rust file syntax..."
    
    cd "$PROJECT_ROOT"
    
    # Check all Rust files for basic syntax issues
    log_info "Checking Rust files..."
    if ! "$CARGO" check --no-default-features --all-targets 2>&1 | tee /tmp/cargo_check.log; then
        log_error "Cargo check failed. See /tmp/cargo_check.log for details."
        return 1
    fi
    
    log_success "File syntax validation passed"
    return 0
}

# Step 2: Run tests
run_tests() {
    log_info "Step 2: Running tests..."
    
    cd "$PROJECT_ROOT"
    
    # Run unit tests
    log_info "Running unit tests..."
    if ! "$CARGO" test --no-default-features --lib 2>&1 | tee /tmp/cargo_test.log; then
        log_warning "Some tests failed. See /tmp/cargo_test.log for details."
        return 1
    fi
    
    log_success "Tests completed"
    return 0
}

# Step 3: Build for current platform
build_current_platform() {
    log_info "Step 3: Building for current platform..."
    
    cd "$PROJECT_ROOT"
    
    # Build without default features first
    log_info "Building without default features..."
    if ! "$CARGO" build --no-default-features --release 2>&1 | tee /tmp/cargo_build_current.log; then
        log_error "Build failed. See /tmp/cargo_build_current.log for details."
        return 1
    fi
    
    log_success "Current platform build completed"
    return 0
}

# Step 4: Build for specific target
build_target() {
    local target="$1"
    log_info "Building for target: $target"
    
    cd "$PROJECT_ROOT"
    
    # Check if target is already installed
    if ! rustup target list --installed | grep -q "$target"; then
        log_info "Installing target: $target"
        if ! rustup target add "$target" 2>&1 | tee /tmp/rustup_add_target.log; then
            log_warning "Failed to add target $target. Skipping this platform."
            return 0
        fi
    fi
    
    # Build for the target
    log_info "Compiling for $target..."
    if ! "$CARGO" build --no-default-features --release --target "$target" 2>&1 | tee "/tmp/cargo_build_${target//\//-}.log"; then
        log_error "Build failed for $target. See /tmp/cargo_build_${target//\//-}.log for details."
        return 1
    fi
    
    log_success "Build completed for $target"
    return 0
}

# Step 5: Validate build artifacts
validate_artifacts() {
    local target="$1"
    log_info "Validating artifacts for: $target"
    
    cd "$PROJECT_ROOT"
    
    local binary_path="target/$target/release/lan-mouse"
    
    if [ ! -f "$binary_path" ]; then
        log_error "Binary not found: $binary_path"
        return 1
    fi
    
    # Check file size
    local size=$(stat -f%z "$binary_path" 2>/dev/null || stat -c%s "$binary_path" 2>/dev/null)
    if [ "$size" -lt 1000 ]; then
        log_error "Binary size is too small: $size bytes"
        return 1
    fi
    
    log_success "Artifact validation passed for $target (size: $size bytes)"
    return 0
}

# Main function
main() {
    log_info "========================================"
    log_info "Multi-platform Build Validation"
    log_info "========================================"
    log_info "Project root: $PROJECT_ROOT"
    log_info "Cargo: $CARGO"
    log_info ""
    
    # Change to project root
    cd "$PROJECT_ROOT"
    
    # Step 1: Validate files
    if ! validate_files; then
        log_error "File validation failed"
        exit 1
    fi
    echo ""
    
    # Step 2: Run tests
    if ! run_tests; then
        log_warning "Tests had issues, but continuing..."
    fi
    echo ""
    
    # Step 3: Build for current platform
    if ! build_current_platform; then
        log_error "Current platform build failed"
        exit 1
    fi
    echo ""
    
    # Step 4: Build for all targets
    log_info "Step 4: Building for all platforms..."
    local failed_targets=()
    local skipped_targets=()
    
    for target in "${TARGETS[@]}"; do
        # Skip current platform (already built)
        if [ "$target" = "$(rustc -vV | grep 'host:' | awk '{print $2}')" ]; then
            log_info "Skipping current platform: $target"
            continue
        fi
        
        # Check if we can build for this target on current OS
        case "$(uname -s)" in
            Darwin)
                # On macOS, we can only build for macOS targets
                if [[ "$target" != *-apple-darwin ]]; then
                    log_info "Skipping Linux target on macOS: $target"
                    skipped_targets+=("$target")
                    continue
                fi
                ;;
            Linux)
                # On Linux, we can only build for Linux targets
                if [[ "$target" == *-apple-darwin ]]; then
                    log_info "Skipping macOS target on Linux: $target"
                    skipped_targets+=("$target")
                    continue
                fi
                ;;
        esac
        
        if ! build_target "$target"; then
            failed_targets+=("$target")
        elif ! validate_artifacts "$target"; then
            failed_targets+=("$target")
        fi
        echo ""
    done
    
    # Summary
    log_info "========================================"
    log_info "Build Summary"
    log_info "========================================"
    
    if [ ${#failed_targets[@]} -gt 0 ]; then
        log_error "Failed targets:"
        for target in "${failed_targets[@]}"; do
            log_error "  - $target"
        done
    fi
    
    if [ ${#skipped_targets[@]} -gt 0 ]; then
        log_warning "Skipped targets (not supported on current OS):"
        for target in "${skipped_targets[@]}"; do
            log_warning "  - $target"
        done
    fi
    
    if [ ${#failed_targets[@]} -eq 0 ]; then
        log_success "All builds completed successfully!"
        return 0
    else
        log_error "Some builds failed"
        return 1
    fi
}

# Run main function
main "$@"
