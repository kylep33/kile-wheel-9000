#!/usr/bin/env bash
# Installs or updates kile-wheel-9000 on Linux (GNOME) and macOS. Safe to re-run.
#
# Installs missing prerequisites, builds release, replaces the installed binaries, and
# sets up starting at login. Never touches an existing user config.
#
#   --prereqs-only   install the toolchain and exit; nothing is built or installed
#   --skip-prereqs   assume the toolchain is present; fails fast if it is not
#   --no-start       install but do not launch
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=app-common.sh
. "$SCRIPT_DIR/app-common.sh"

MANIFEST="$SCRIPT_DIR/Cargo.toml"
RUSTUP_URL='https://sh.rustup.rs'

PREREQS_ONLY=0
SKIP_PREREQS=0
NO_START=0
for argument in "$@"; do
    case "$argument" in
        --prereqs-only) PREREQS_ONLY=1 ;;
        --skip-prereqs) SKIP_PREREQS=1 ;;
        --no-start) NO_START=1 ;;
        *) fail "unknown option $argument; see the top of install.sh for the ones it takes" ;;
    esac
done

resolve_cargo() {
    if have cargo; then return 0; fi
    if [ -x "$CARGO_BIN/cargo" ]; then
        export PATH="$CARGO_BIN:$PATH"
        return 0
    fi
    return 1
}

install_compiler() {
    if have cc; then
        detail 'c compiler present'
        return
    fi
    case "$OS" in
        linux)
            step 'installing build-essential (asks for your password)'
            sudo apt-get update
            sudo apt-get install -y build-essential pkg-config
            ;;
        macos)
            fail "no c compiler, which rust needs to link; run 'xcode-select --install', let it finish, then re-run"
            ;;
    esac
}

install_rust() {
    if resolve_cargo; then
        detail "rust present: $(cargo --version)"
        return
    fi
    have curl || fail "curl is needed to install rust; install curl, or install rust from https://rustup.rs, then re-run"

    step 'installing rust via rustup'
    curl --proto '=https' --tlsv1.2 -sSf "$RUSTUP_URL" | sh -s -- -y --profile minimal
    resolve_cargo || fail "cargo not found after installing rustup, expected it in $CARGO_BIN; open a new shell and re-run with --skip-prereqs"
}

assert_prereqs() {
    resolve_cargo || fail 'cargo not found and --skip-prereqs was given; drop --skip-prereqs to install it'
    have cc || fail 'no c compiler and --skip-prereqs was given; drop --skip-prereqs to install one'
}

assert_gnome() {
    [ "$OS" = linux ] || return 0
    have gnome-shell || fail "GNOME Shell not found; on Linux the wheel needs GNOME, which Ubuntu ships by default"
    have gnome-extensions || fail "gnome-extensions not found; install the gnome-shell package, then re-run"
}

wheel_version() {
    cargo metadata --no-deps --format-version 1 --manifest-path "$MANIFEST" \
        | sed -n "s/.*\"name\":\"$WHEEL_PACKAGE\",\"version\":\"\([^\"]*\)\".*/\1/p"
}

build_release() {
    step 'building release'
    cargo build --release --locked --manifest-path "$MANIFEST" \
        || fail 'cargo build failed; fix the build errors above and re-run'

    BUILT_DIR="$SCRIPT_DIR/target/release"
    local name
    for name in "$APP_NAME" "$SETTINGS_BIN"; do
        [ -x "$BUILT_DIR/$name" ] || fail "build reported success but $BUILT_DIR/$name is missing"
    done
}

# the settings window sits beside the wheel, which is where the wheel looks for it
install_binaries() {
    step "installing to $INSTALL_DIR"
    mkdir -p "$INSTALL_DIR"
    local name
    for name in "$APP_NAME" "$SETTINGS_BIN"; do
        install -m 0755 "$BUILT_DIR/$name" "$INSTALL_DIR/$name"
        detail "  $name"
    done
}

# config.toml is written by the app on first run so the schema lives in one place
install_config_dirs() {
    mkdir -p "$CONFIG_DIR/icons" "$CONFIG_DIR/logs"
    detail "config dir: $CONFIG_DIR"
}

# --- linux ---------------------------------------------------------------------------

install_extension() {
    step "installing the GNOME Shell extension $EXTENSION_UUID"
    mkdir -p "$EXTENSION_DIR"
    cp "$SCRIPT_DIR/gnome-extension/$EXTENSION_UUID/"* "$EXTENSION_DIR/"
    # a brand new extension is only picked up by a fresh shell, so this can fail until
    # the next login; the enabled list in gsettings is what makes it stick
    gnome-extensions enable "$EXTENSION_UUID" 2>/dev/null || enable_extension_on_next_login
}

enable_extension_on_next_login() {
    local enabled
    enabled="$(gsettings get org.gnome.shell enabled-extensions)"
    case "$enabled" in
        *"'$EXTENSION_UUID'"*) return ;;
        '@as []') enabled="['$EXTENSION_UUID']" ;;
        *) enabled="${enabled%]}, '$EXTENSION_UUID']" ;;
    esac
    gsettings set org.gnome.shell enabled-extensions "$enabled"
    detail 'the extension switches on at your next login'
}

# evdev needs the input group, and replaying input needs write access to /dev/uinput
install_input_access() {
    local rule='KERNEL=="uinput", SUBSYSTEM=="misc", GROUP="input", MODE="0660", OPTIONS+="static_node=uinput"'
    if [ "$(cat "$UDEV_RULE" 2>/dev/null)" != "$rule" ]; then
        step 'letting the input group use /dev/uinput (asks for your password)'
        echo "$rule" | sudo tee "$UDEV_RULE" >/dev/null
        echo 'uinput' | sudo tee "$MODULES_FILE" >/dev/null
        sudo modprobe uinput
        sudo udevadm control --reload-rules
        sudo udevadm trigger --name-match=uinput
    fi

    if ! id -nG "$USER" | tr ' ' '\n' | grep -qx input; then
        step "adding $USER to the input group (asks for your password)"
        sudo usermod -aG input "$USER"
        NEEDS_RELOGIN=1
    fi
}

install_desktop_entries() {
    mkdir -p "$(dirname "$AUTOSTART_FILE")" "$(dirname "$SETTINGS_ENTRY")"
    cat >"$AUTOSTART_FILE" <<EOF
[Desktop Entry]
Type=Application
Name=$DISPLAY_NAME
Exec=$INSTALL_DIR/$APP_NAME
X-GNOME-Autostart-enabled=true
NoDisplay=true
EOF
    cat >"$SETTINGS_ENTRY" <<EOF
[Desktop Entry]
Type=Application
Name=$DISPLAY_NAME settings
Exec=$INSTALL_DIR/$SETTINGS_BIN
Icon=preferences-desktop
Categories=Settings;
EOF
    detail "autostart: $AUTOSTART_FILE"
}

install_linux() {
    NEEDS_RELOGIN=0
    install_input_access
    install_extension
    install_desktop_entries
}

# --- macos ---------------------------------------------------------------------------

write_info_plist() {
    cat >"$APP_BUNDLE/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
    <key>CFBundleName</key><string>$DISPLAY_NAME</string>
    <key>CFBundleExecutable</key><string>$APP_NAME</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>LSUIElement</key><true/>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
EOF
}

# a stable signature is what lets the Accessibility grant survive a reinstall
sign_bundle() {
    codesign --force --deep --sign - "$APP_BUNDLE" >/dev/null 2>&1 \
        || detail 'codesign failed, so macOS may ask for Accessibility again after each update'
}

install_launch_agent() {
    mkdir -p "$(dirname "$LAUNCH_AGENT")"
    cat >"$LAUNCH_AGENT" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>$BUNDLE_ID</string>
    <key>ProgramArguments</key><array><string>$INSTALL_DIR/$APP_NAME</string></array>
    <key>RunAtLoad</key><true/>
    <key>ProcessType</key><string>Interactive</string>
</dict>
</plist>
EOF
    # replaced rather than loaded twice, so a re-run is harmless
    launchctl bootout "gui/$(id -u)/$BUNDLE_ID" 2>/dev/null || true
    launchctl bootstrap "gui/$(id -u)" "$LAUNCH_AGENT"
    detail "login agent: $LAUNCH_AGENT"
}

install_macos() {
    write_info_plist
    sign_bundle
    if [ "$NO_START" -eq 1 ]; then
        detail 'login agent not loaded because of --no-start; it loads at next login'
        return
    fi
    install_launch_agent
}

# --- main ----------------------------------------------------------------------------

write_summary() {
    echo
    printf '\033[32m%s %s installed\033[0m\n' "$DISPLAY_NAME" "$VERSION"
    detail "binary:   $INSTALL_DIR/$APP_NAME"
    detail "settings: $INSTALL_DIR/$SETTINGS_BIN"
    detail "config:   $CONFIG_DIR"
    detail "hotkey:   $DEFAULT_HOTKEY opens the wheel"
    case "$OS" in
        linux)
            detail "startup:  $AUTOSTART_FILE"
            echo
            echo 'Log out and back in once, so GNOME loads the extension and the input group applies.'
            ;;
        macos)
            detail "startup:  $LAUNCH_AGENT"
            echo
            echo "Grant $DISPLAY_NAME Accessibility access in System Settings, Privacy & Security,"
            echo 'Accessibility (and Input Monitoring if asked), then start it again from Applications.'
            ;;
    esac
}

if [ "$SKIP_PREREQS" -eq 1 ]; then
    assert_prereqs
else
    step 'installing prereqs if not present'
    install_compiler
    install_rust
fi

if [ "$PREREQS_ONLY" -eq 1 ]; then
    echo
    printf '\033[32mprereqs ready\033[0m\n'
    exit 0
fi

assert_gnome
VERSION="$(wheel_version)"

# stop first, so the copy never replaces a binary that is running
stop_app
build_release
install_binaries
install_config_dirs

case "$OS" in
    linux) install_linux ;;
    macos) install_macos ;;
esac

# macOS starts through the login agent just loaded; on Linux a fresh install cannot start
# until the extension and the input group are live after a login
if [ "$NO_START" -eq 0 ] && [ "$OS" = linux ] && [ "$NEEDS_RELOGIN" -eq 0 ]; then
    nohup "$INSTALL_DIR/$APP_NAME" >/dev/null 2>&1 &
fi

write_summary
