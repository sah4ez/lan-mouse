#!/bin/bash
# Local build script for cross-compiling Linux builds on macOS with X11 features
# This script ensures that X11 code compiles correctly on Linux without needing a Linux VM

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output
print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if we're on macOS
if [[ "$OSTYPE" != "darwin"* ]]; then
    print_error "This script is designed to run on macOS for cross-compilation to Linux"
    exit 1
fi

print_info "Starting Linux cross-compilation build on macOS"
echo ""

# Check if Rust is installed
if ! command -v rustc &> /dev/null; then
    print_error "Rust is not installed. Please install Rust from https://rustup.rs/"
    exit 1
fi

# Check if rustup is available
if ! command -v rustup &> /dev/null; then
    print_error "rustup is not installed"
    exit 1
fi

# Install Linux targets
print_info "Installing Linux targets..."

TARGETS=(
    "x86_64-unknown-linux-gnu"
    "aarch64-unknown-linux-gnu"
)

for target in "${TARGETS[@]}"; do
    if rustup target list --installed | grep -q "$target"; then
        print_success "Target $target already installed"
    else
        print_info "Installing target: $target"
        rustup target add "$target" || {
            print_error "Failed to install target: $target"
            exit 1
        }
        print_success "Target $target installed"
    fi
done

echo ""

# Check for cross-compilation tools
print_info "Checking for cross-compilation tools..."

# Check for x86_64 cross-compiler
if ! command -v x86_64-linux-gnu-gcc &> /dev/null; then
    print_warning "x86_64-linux-gnu-gcc not found. Install via: brew install x86_64-linux-gnu-gcc"
    print_warning "You may need to install cross-compilation toolchain manually"
    print_info "Attempting to install via Homebrew..."
    
    if brew list x86_64-linux-gnu-gcc &> /dev/null 2>&1; then
        print_success "x86_64-linux-gnu-gcc already installed via Homebrew"
    else
        print_info "Installing x86_64-linux-gnu-gcc via Homebrew..."
        brew install x86_64-linux-gnu-gcc || {
            print_warning "Failed to install x86_64-linux-gnu-gcc via Homebrew"
            print_warning "You can install it manually or use Docker for cross-compilation"
        }
    fi
fi

# Check for aarch64 cross-compiler
if ! command -v aarch64-linux-gnu-gcc &> /dev/null; then
    print_warning "aarch64-linux-gnu-gcc not found. Install via: brew install aarch64-linux-gnu-gcc"
    print_warning "You may need to install cross-compilation toolchain manually"
    print_info "Attempting to install via Homebrew..."
    
    if brew list aarch64-linux-gnu-gcc &> /dev/null 2>&1; then
        print_success "aarch64-linux-gnu-gcc already installed via Homebrew"
    else
        print_info "Installing aarch64-linux-gnu-gcc via Homebrew..."
        brew install aarch64-linux-gnu-gcc || {
            print_warning "Failed to install aarch64-linux-gnu-gcc via Homebrew"
            print_warning "You can install it manually or use Docker for cross-compilation"
        }
    fi
fi

echo ""

# Build function
build_target() {
    local target=$1
    local features=$2
    local build_name=$3
    
    print_info "Building $build_name for $target with features: $features"
    
    # Set CC for cross-compilation
    local cc=""
    if [[ "$target" == "x86_64-unknown-linux-gnu" ]]; then
        cc="x86_64-linux-gnu-gcc"
    elif [[ "$target" == "aarch64-unknown-linux-gnu" ]]; then
        cc="aarch64-linux-gnu-gcc"
    fi
    
    if [[ -n "$cc" ]] && command -v "$cc" &> /dev/null; then
        export CC="$cc"
        export CXX="${cc/gcc/g++}"
    fi
    
    # Build
    if cargo build --no-default-features --release --target "$target" --features "$features"; then
        print_success "Build $build_name for $target completed successfully"
        
        # Verify binary
        local binary="target/$target/release/lan-mouse"
        if [[ -f "$binary" ]]; then
            print_info "Binary created: $binary"
            file "$binary" || true
            ls -lh "$binary" || true
        else
            print_warning "Binary not found at expected location: $binary"
        fi
    else
        print_error "Build $build_name for $target failed"
        return 1
    fi
    
    echo ""
}

# Build for Linux AMD64
print_info "=========================================="
print_info "Building for Linux AMD64 (x86_64)"
print_info "=========================================="
echo ""

# Build without default features
build_target "x86_64-unknown-linux-gnu" "" "minimal"

# Build with X11 capture and emulation
build_target "x86_64-unknown-linux-gnu" "x11_capture,x11_emulation" "x11-full"

# Build for Linux ARM64
print_info "=========================================="
print_info "Building for Linux ARM64 (aarch64)"
print_info "=========================================="
echo ""

# Build without default features
build_target "aarch64-unknown-linux-gnu" "" "minimal"

# Build with X11 capture and emulation
build_target "aarch64-unknown-linux-gnu" "x11_capture,x11_emulation" "x11-full"

# Summary
print_info "=========================================="
print_info "Build Summary"
print_info "=========================================="
echo ""
print_success "All Linux builds completed successfully!"
echo ""
print_info "Binaries are available in:"
echo "  - target/x86_64-unknown-linux-gnu/release/"
echo "  - target/aarch64-unknown-linux-gnu/release/"
echo ""
print_info "You can transfer these binaries to Linux systems for testing"
echo ""

exit 0
