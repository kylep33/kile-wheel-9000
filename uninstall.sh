#!/usr/bin/env bash
# Removes kile-wheel-9000 from Linux (GNOME) or macOS. Safe to re-run.
#
# Leaves the config folder alone unless --purge is given, so a reinstall picks up where
# you left off. The input group membership is left too, since other tools use it.
#
#   --purge   also delete the config folder, icons and logs
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=app-common.sh
. "$SCRIPT_DIR/app-common.sh"

PURGE=0
for argument in "$@"; do
    case "$argument" in
        --purge) PURGE=1 ;;
        *) fail "unknown option $argument; the only one is --purge" ;;
    esac
done

remove() {
    local path
    for path in "$@"; do
        [ -e "$path" ] || continue
        rm -rf "$path"
        detail "removed $path"
    done
}

uninstall_linux() {
    gnome-extensions disable "$EXTENSION_UUID" 2>/dev/null || true
    remove "$EXTENSION_DIR" "$AUTOSTART_FILE" "$SETTINGS_ENTRY" \
        "$INSTALL_DIR/$APP_NAME" "$INSTALL_DIR/$SETTINGS_BIN"

    if [ -e "$UDEV_RULE" ] || [ -e "$MODULES_FILE" ]; then
        step 'removing the /dev/uinput rule (asks for your password)'
        sudo rm -f "$UDEV_RULE" "$MODULES_FILE"
        sudo udevadm control --reload-rules
    fi
}

uninstall_macos() {
    launchctl bootout "gui/$(id -u)/$BUNDLE_ID" 2>/dev/null || true
    remove "$LAUNCH_AGENT" "$APP_BUNDLE"
    detail 'the Accessibility entry stays in System Settings until you remove it there'
}

stop_app
step "removing $DISPLAY_NAME"
case "$OS" in
    linux) uninstall_linux ;;
    macos) uninstall_macos ;;
esac

if [ "$PURGE" -eq 1 ]; then
    remove "$CONFIG_DIR"
else
    detail "kept your config in $CONFIG_DIR; pass --purge to delete it"
fi

echo
printf '\033[32m%s removed\033[0m\n' "$DISPLAY_NAME"
