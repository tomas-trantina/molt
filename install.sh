#!/usr/bin/env bash
set -e

REPO_URL="https://github.com/tomas-trantina/molt.git"
BINARY_NAME="molt"
INSTALL_DIR="${INSTALL_DIR:-${PREFIX:-$HOME/.local/bin}}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1" >&2
    exit 1
}

# Check for cargo
if ! command -v cargo &> /dev/null; then
    error "Rust / cargo is not installed. Please install Rust first:\n  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
fi

# Detect build directory
TMP_DIR=""
cleanup() {
    if [ -n "$TMP_DIR" ] && [ -d "$TMP_DIR" ]; then
        info "Cleaning up temporary directory..."
        rm -rf "$TMP_DIR"
    fi
}
trap cleanup EXIT

SCRIPT_DIR=""
if [ -n "${BASH_SOURCE[0]}" ] && [ "${BASH_SOURCE[0]}" != "bash" ] && [ "${BASH_SOURCE[0]}" != "-bash" ] && [ -f "${BASH_SOURCE[0]}" ]; then
    SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
fi

if [ -n "$SCRIPT_DIR" ] && [ -f "$SCRIPT_DIR/Cargo.toml" ]; then
    BUILD_DIR="$SCRIPT_DIR"
    info "Building $BINARY_NAME from local repository ($BUILD_DIR)..."
elif [ -f "./Cargo.toml" ]; then
    BUILD_DIR="$(pwd)"
    info "Building $BINARY_NAME from current directory ($BUILD_DIR)..."
else
    if ! command -v git &> /dev/null; then
        error "git is required to download the source code when installing remotely."
    fi
    TMP_DIR="$(mktemp -d)"
    info "Cloning $REPO_URL to temporary directory..."
    git clone --depth 1 "$REPO_URL" "$TMP_DIR"
    BUILD_DIR="$TMP_DIR"
fi

# Build project
info "Compiling $BINARY_NAME (release mode)..."
cd "$BUILD_DIR"
cargo build --release

# Ensure target directory exists
mkdir -p "$INSTALL_DIR"

# Copy binary
info "Installing binary to $INSTALL_DIR/$BINARY_NAME..."
cp "target/release/$BINARY_NAME" "$INSTALL_DIR/$BINARY_NAME"
chmod +x "$INSTALL_DIR/$BINARY_NAME"

success "$BINARY_NAME installed successfully!"

# PATH check
case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        warn "$INSTALL_DIR is not in your PATH."
        echo "Add it to your shell configuration (e.g. ~/.bashrc or ~/.zshrc):"
        echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
        ;;
esac

echo ""
echo "Run '$BINARY_NAME --help' to get started."
