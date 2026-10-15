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
