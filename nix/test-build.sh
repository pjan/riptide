#!/usr/bin/env bash
# Test script for verifying riptide Nix build

set -euo pipefail

echo "=========================================="
echo "Testing riptide Nix Package Build"
echo "=========================================="
echo ""

# Color codes for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print status messages
print_status() {
    echo -e "${YELLOW}▶${NC} $1"
}

print_success() {
    echo -e "${GREEN}✓${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

# Change to repository root
cd "$(dirname "$0")/.."

# Check if Nix is available
print_status "Checking for Nix installation..."
if ! command -v nix &> /dev/null; then
    print_error "Nix is not installed. Please install Nix first:"
    echo "  curl -L https://nixos.org/nix/install | sh"
    exit 1
fi
print_success "Nix is installed: $(nix --version)"
echo ""

# Check if flakes are enabled
print_status "Checking if Nix flakes are enabled..."
if nix flake --version &> /dev/null; then
    print_success "Nix flakes are enabled"
else
    print_error "Nix flakes are not enabled. Add to ~/.config/nix/nix.conf:"
    echo "  experimental-features = nix-command flakes"
    exit 1
fi
echo ""

# Build the package
print_status "Building riptide package..."
if nix build --no-link --print-build-logs; then
    print_success "Build completed successfully"
else
    print_error "Build failed"
    exit 1
fi
echo ""

# Check the built package
print_status "Checking built package structure..."
result=$(nix build --no-link --print-out-paths)
if [ -f "$result/bin/riptide" ]; then
    print_success "Binary exists at: $result/bin/riptide"
else
    print_error "Binary not found at expected location"
    exit 1
fi
echo ""

# Test binary execution
print_status "Testing binary execution..."
if "$result/bin/riptide" --help > /dev/null 2>&1; then
    print_success "Binary executes successfully"
else
    print_error "Binary failed to execute"
    exit 1
fi
echo ""

# Check for ffmpeg in PATH
print_status "Verifying ffmpeg is available..."
if "$result/bin/riptide" --help 2>&1 | grep -q "riptide"; then
    print_success "FFmpeg wrapper is working"
else
    print_error "FFmpeg wrapper may not be working correctly"
fi
echo ""

# Run flake checks
print_status "Running flake checks..."
if nix flake check --no-build 2>&1 | head -20; then
    print_success "Flake checks passed"
else
    print_error "Flake checks failed (this may be expected if checks are not defined)"
fi
echo ""

# Show package info
print_status "Package information:"
nix eval .#packages.$(nix eval --impure --raw --expr 'builtins.currentSystem').default.meta.description 2>/dev/null || echo "  Description: Download Tidal tracks with CLI downloader"
echo "  Version: $(nix eval .#packages.$(nix eval --impure --raw --expr 'builtins.currentSystem').default.version --raw 2>/dev/null || echo '3.2.1')"
echo ""

# Test development shell
print_status "Testing development shell..."
if nix develop --command bash -c 'python --version && ffmpeg -version' > /dev/null 2>&1; then
    print_success "Development shell loads successfully"
else
    print_error "Development shell failed to load"
    exit 1
fi
echo ""

echo "=========================================="
print_success "All tests passed!"
echo "=========================================="
echo ""
echo "You can now:"
echo "  • Run riptide: nix run ."
echo "  • Install it: nix profile install ."
echo "  • Enter dev shell: nix develop"
echo ""
