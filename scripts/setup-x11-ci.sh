#!/bin/bash
# Setup script for automated X11 CI on macOS
# This script installs the pre-commit hook and sets up the build environment

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output
print_info() {
    echo -e "${BLUE}[SETUP]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SETUP]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[SETUP]${NC} $1"
}

print_error() {
    echo -e "${RED}[SETUP]${NC} $1"
}

# Check if we're in a git repository
if ! git rev-parse --git-dir > /dev/null 2>&1; then
    print_error "Not in a git repository"
    exit 1
fi

print_info "Setting up automated X11 CI on macOS"
echo ""

# Get the git hooks directory
HOOKS_DIR=$(git rev-parse --git-dir)/hooks

# Make scripts executable
print_info "Making scripts executable..."
chmod +x scripts/build-linux-x11.sh
chmod +x scripts/pre-commit-x11-check.sh
chmod +x scripts/setup-x11-ci.sh
print_success "Scripts made executable"
echo ""

# Install pre-commit hook
print_info "Installing pre-commit hook..."
if [[ -f "$HOOKS_DIR/pre-commit" ]]; then
    print_warning "Pre-commit hook already exists"
    read -p "Do you want to overwrite it? (y/N) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        cp scripts/pre-commit-x11-check.sh "$HOOKS_DIR/pre-commit"
        print_success "Pre-commit hook installed"
    else
        print_warning "Pre-commit hook installation skipped"
    fi
else
    cp scripts/pre-commit-x11-check.sh "$HOOKS_DIR/pre-commit"
    print_success "Pre-commit hook installed"
fi
echo ""

# Install Linux targets
print_info "Installing Linux targets for cross-compilation..."
if rustup target list --installed | grep -q "x86_64-unknown-linux-gnu"; then
    print_success "x86_64-unknown-linux-gnu already installed"
else
    print_info "Installing x86_64-unknown-linux-gnu..."
    rustup target add x86_64-unknown-linux-gnu
    print_success "x86_64-unknown-linux-gnu installed"
fi

if rustup target list --installed | grep -q "aarch64-unknown-linux-gnu"; then
    print_success "aarch64-unknown-linux-gnu already installed"
else
    print_info "Installing aarch64-unknown-linux-gnu..."
    rustup target add aarch64-unknown-linux-gnu
    print_success "aarch64-unknown-linux-gnu installed"
fi
echo ""

# Check for cross-compilation tools
print_info "Checking for cross-compilation tools..."

if command -v x86_64-linux-gnu-gcc &> /dev/null; then
    print_success "x86_64-linux-gnu-gcc found"
else
    print_warning "x86_64-linux-gnu-gcc not found"
    print_info "You can install it with: brew install x86_64-linux-gnu-gcc"
fi

if command -v aarch64-linux-gnu-gcc &> /dev/null; then
    print_success "aarch64-linux-gnu-gcc found"
else
    print_warning "aarch64-linux-gnu-gcc not found"
    print_info "You can install it with: brew install aarch64-linux-gnu-gcc"
fi
echo ""

# Summary
print_info "=========================================="
print_info "Setup Summary"
print_info "=========================================="
echo ""
print_success "Automated X11 CI setup completed!"
echo ""
print_info "Available commands:"
echo "  make build-linux-x11         - Build all Linux X11 binaries"
echo "  make check-x11               - Quick X11 compilation check"
echo "  make check-all-x11           - Check all X11 packages"
echo "  make test-x11                - Run X11 tests"
echo "  make clean                   - Clean build artifacts"
echo ""
print_info "Pre-commit hook:"
echo "  - Automatically checks X11 compilation on commit"
echo "  - Only runs when X11-related files are modified"
echo ""
print_info "For more information, see BUILD.md"
echo ""

exit 0
