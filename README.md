# kile-wheel-9000

A hotkey-triggered radial launcher for Windows. Your taskbar, made circular, with tabs.

## Install

```powershell
.\install.ps1
```

It self-elevates, so approve the UAC prompt. The first run installs the Rust toolchain
and the MSVC C++ build tools if they are missing, which takes a while; later runs skip
straight to building.

Re-run the same command to update.

## Uninstall

```powershell
.\uninstall.ps1
```

Removes the binaries and the logon task. It asks before deleting your config, or pass
`-KeepConfig` / `-RemoveConfig` to answer up front.

## Using it

Press **Caps Lock** to open the wheel.

| | |
|---|---|
| WASD or arrows | select by direction; selection follows the keys you hold |
| Enter | activate the selected slot |
| 1 to 9 | switch tab |
| Backspace | back out one level |
| Esc | close |
| Mouse move | select by angle, no precision needed |
| Click | activate; the cog in the middle opens settings |
| Hold 400 ms | toggle edit mode, then drag slots around |

## Config

Everything lives in `%APPDATA%\kile-wheel-9000\`:

- `config.toml` — settings, tabs and pinned items, written commented on first run
- `icons\` — your own icon images
- `logs\` — rotating log files

Edit it through the settings window or by hand; the wheel reloads it either way. Copying
the folder to another machine carries your whole setup with it.

## Requirements

- Windows 10 or 11, x64
- Runs elevated, so the hotkey still works while an admin window has focus. `install.ps1`
  registers a logon task with highest privileges, which avoids a UAC prompt at every
  sign-in.

## Options

| | |
|---|---|
| `install.ps1 -PrereqsOnly` | install the toolchain and stop |
| `install.ps1 -SkipPrereqs` | assume the toolchain is there |
| `install.ps1 -NoStart` | install without launching |

- now runs on Windows 10/11, Ubuntu 26.04 (GNOME on Wayland) and macOS
- install on Linux or macOS with `./install.sh`, remove with `./uninstall.sh` (`--purge` also deletes the config)
- `install.sh` takes `--prereqs-only`, `--skip-prereqs` and `--no-start`, same as `install.ps1`
- config lives in `~/.config/kile-wheel-9000/` on Linux and `~/Library/Application Support/kile-wheel-9000/` on macOS
- Linux needs the bundled GNOME Shell extension (`gnome-extension/kile-wheel-9000@kile.dev`), which install.sh copies and enables; log out and back in once after the first install
- Linux reads keys and mice through evdev and replays them through uinput, so install.sh adds you to the `input` group and adds a udev rule for `/dev/uinput` (asks for sudo)
- touchpads, tablets and touchscreens are not grabbed on Linux, so their buttons cannot be hotkeys
- the Linux tray icon shows through Ubuntu's AppIndicator extension
- macOS needs Accessibility access (System Settings, Privacy & Security, Accessibility), and Input Monitoring if it asks
- macOS installs as `~/Applications/Kile Wheel 9000.app` with a login agent; it is ad-hoc signed so the Accessibility grant survives updates
- on macOS Caps Lock still toggles caps when it opens the wheel; remap it under Keyboard, Modifier Keys, or bind something else
- `Win` in a binding means Super on Linux and Cmd on macOS
- paths in the config use `%VAR%` on Windows and `~` / `$VAR` on Linux and macOS
- terminal profiles and WSL distros are Windows only; elsewhere a terminal item opens the default terminal in its `cwd`
- typed snippets on Linux only type plain ASCII; clipboard mode handles anything
- the wheel is now drawn on the CPU with the same renderer on every OS (tiny-skia + cosmic-text); slice glyphs are Lucide icons in `crates/wheel/assets/glyphs`
