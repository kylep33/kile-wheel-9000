#!/usr/bin/env bash
# Shared names, paths and helpers for install.sh and uninstall.sh on Linux and macOS.
# Sourced, not run.

# rename the product here and in core::APP_NAME
APP_NAME='kile-wheel-9000'
DISPLAY_NAME='Kile Wheel 9000'
DEFAULT_HOTKEY='Caps Lock'
SETTINGS_BIN="${APP_NAME}-settings"
WHEEL_PACKAGE="$APP_NAME"
STOP_TIMEOUT_S=10

CARGO_BIN="$HOME/.cargo/bin"

case "$(uname -s)" in
    Linux)
        OS='linux'
        INSTALL_DIR="$HOME/.local/bin"
        CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/$APP_NAME"
        EXTENSION_UUID='kile-wheel-9000@kile.dev'
        EXTENSION_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/gnome-shell/extensions/$EXTENSION_UUID"
        AUTOSTART_FILE="${XDG_CONFIG_HOME:-$HOME/.config}/autostart/$APP_NAME.desktop"
        SETTINGS_ENTRY="${XDG_DATA_HOME:-$HOME/.local/share}/applications/$SETTINGS_BIN.desktop"
        UDEV_RULE='/etc/udev/rules.d/70-kile-wheel-9000.rules'
        MODULES_FILE='/etc/modules-load.d/kile-wheel-9000.conf'
        ;;
    Darwin)
        OS='macos'
        APP_BUNDLE="$HOME/Applications/$DISPLAY_NAME.app"
        INSTALL_DIR="$APP_BUNDLE/Contents/MacOS"
        CONFIG_DIR="$HOME/Library/Application Support/$APP_NAME"
        BUNDLE_ID='dev.kile.wheel9000'
        LAUNCH_AGENT="$HOME/Library/LaunchAgents/$BUNDLE_ID.plist"
        ;;
    *)
        echo "unsupported OS $(uname -s); this script handles Linux and macOS, install.ps1 handles Windows" >&2
        exit 1
        ;;
esac

step() { printf '\033[36m==> %s\033[0m\n' "$*"; }
detail() { printf '\033[90m    %s\033[0m\n' "$*"; }
fail() {
    printf '\033[31merror: %s\033[0m\n' "$*" >&2
    exit 1
}

have() { command -v "$1" >/dev/null 2>&1; }

# force-kill is fine: the overlay has no window to close, and config writes are atomic
stop_app() {
    local name running=0
    for name in "$APP_NAME" "$SETTINGS_BIN"; do
        if pgrep -x "$name" >/dev/null 2>&1; then
            running=1
            pkill -x "$name" || true
        fi
    done
    [ "$running" -eq 1 ] || return 0

    step "stopping running $DISPLAY_NAME"
    local waited=0
    while pgrep -x "$APP_NAME" >/dev/null 2>&1 && [ "$waited" -lt "$STOP_TIMEOUT_S" ]; do
        sleep 1
        waited=$((waited + 1))
    done
}
