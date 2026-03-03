#!/bin/bash
# Pre-commit hook to automatically check X11 code compilation
# This ensures X11 code compiles correctly on every commit

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output
print_info() {
    echo -e "${BLUE}[PRE-COMMIT]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[PRE-COMMIT]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[PRE-COMMIT]${NC} $1"
}

print_error() {
    echo -e "${RED}[PRE-COMMIT]${NC} $1"
}

# Check if there are any changes to X11-related files
check_x11_files_changed() {
    # Get staged files
    local staged_files=$(git diff --cached --name-only --diff-filter=ACM)
    
    # Check if any X11-related files are staged
    local x11_files=$(echo "$staged_files" | grep -E "(x11\.rs|input-capture|input-emulation)" || true)
    
    if [[ -n "$x11_files" ]]; then
        return 0
    else
        return 1
    fi
}

# Main pre-commit check
main() {
    print_info "Running X11 compilation check..."
    
    # Check if X11 files were modified
    if check_x11_files_changed; then
        print_info "X11-related files modified, running full X11 compilation check..."
        
    # Run compilation check (using current platform, no cross-compilation needed)
    print_info "Checking X11 code compilation..."
    if cargo check --package input-capture --features x11; then
        print_success "X11 code compilation check passed"
    else
        print_error "X11 code compilation check failed"
        print_error "Please fix the compilation errors before committing"
        exit 1
    fi
    else
        print_info "No X11-related files modified, skipping X11 compilation check"
    fi
    
    # Optional: Run quick check on input-emulation X11 code
    if check_x11_files_changed; then
        print_info "Checking input-emulation X11 code..."
        if cargo check --package input-emulation --features x11; then
            print_success "input-emulation X11 code check passed"
        else
            print_error "input-emulation X11 code check failed"
            print_error "Please fix the compilation errors before committing"
            exit 1
        fi
    fi
    
    print_success "Pre-commit checks completed successfully"
    exit 0
}

# Run main function
main
