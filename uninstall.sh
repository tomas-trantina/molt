#!/usr/bin/env bash
set -e

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

PURGE=false
for arg in "$@"; do
    case $arg in
        --purge|--all|-p)
            PURGE=true
            shift
            ;;
    esac
done

REMOVED=false

# Search binary paths
TARGET_PATHS=(
    "$INSTALL_DIR/$BINARY_NAME"
    "$HOME/.local/bin/$BINARY_NAME"
    "/usr/local/bin/$BINARY_NAME"
)

for bin_path in "${TARGET_PATHS[@]}"; do
    if [ -f "$bin_path" ]; then
        info "Removing binary: $bin_path..."
        rm -f "$bin_path"
        REMOVED=true
    fi
done

if [ "$REMOVED" = true ]; then
    success "$BINARY_NAME binary uninstalled successfully."
else
    warn "No $BINARY_NAME binary found in standard locations."
fi

# Optional config purge
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/molt"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/molt"
STATE_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/molt"

if [ "$PURGE" = true ]; then
    info "Purging configuration and data directories..."
    [ -d "$CONFIG_DIR" ] && rm -rf "$CONFIG_DIR" && info "Removed $CONFIG_DIR"
    [ -d "$DATA_DIR" ] && rm -rf "$DATA_DIR" && info "Removed $DATA_DIR"
    [ -d "$STATE_DIR" ] && rm -rf "$STATE_DIR" && info "Removed $STATE_DIR"
    success "All $BINARY_NAME configuration and state files have been purged."
else
    if [ -d "$CONFIG_DIR" ] || [ -d "$DATA_DIR" ] || [ -d "$STATE_DIR" ]; then
        echo ""
        info "Configuration and data files were preserved:"
        [ -d "$CONFIG_DIR" ] && echo "  Config: $CONFIG_DIR"
        [ -d "$DATA_DIR" ] && echo "  Data:   $DATA_DIR"
        [ -d "$STATE_DIR" ] && echo "  State:  $STATE_DIR"
        echo ""
        echo "To completely remove all configuration and data files, run:"
        echo "  ./uninstall.sh --purge"
    fi
fi
