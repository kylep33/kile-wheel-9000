# Design

## Overview

A single elevated process owns a pre-created hidden overlay window. Low-level input
hooks live on a dedicated thread and make the swallow decision synchronously from
shared atomic state. The running-window model is maintained incrementally by WinEvent
hooks so opening the wheel never enumerates. Rendering goes through DirectComposition
with a Direct2D-drawn flip-model swapchain, which gives per-pixel alpha without a CPU
blit. A second, lightweight binary hosts the settings UI so the wheel binary stays
free of GUI framework weight.

## Crate layout

A cargo workspace. Dependencies point one direction only, which keeps the pure logic
unit-testable on any host and keeps `windows-rs` out of the layer that holds the rules.

```
kile-wheel-9000/
  Cargo.toml              workspace, shared dep versions and lints
  crates/
    core/                 kile-wheel-core      no windows-rs
    platform/             kile-wheel-platform  all windows-rs plumbing
    wheel/                kile-wheel-9000      overlay binary
    settings/             kile-wheel-9000-settings   added in phase 6
  install.ps1
  uninstall.ps1
  app-common.ps1
```

```
core  <-  platform  <-  wheel
  \                        settings
   \_______________________/
```

### kile-wheel-core

Holds `APP_NAME`, the config model, the item model, and the rules that decide what
goes where and what is selected. No OS calls, so it is testable without a desktop.

- `names`: `APP_NAME`, derived config/log/icon subpaths from a caller-supplied base dir
- `config`: serde model for `config.toml`, plus `toml_edit`-based write-back
- `item`: `Item` enum over App, Script, Url, Snippet, Folder, Terminal
- `layout::assign`: slot assignment including the stability rule
- `select::latch`: the WASD latching state machine
- `select::angle`: cursor angle to slot index, with the hub dead zone
- `binding`: parsed hotkey bindings and the lookup the hook path uses

### kile-wheel-platform

OS services, grouped by what they talk to rather than by which header they came from.
The overlay's own window and renderer are not here, because they belong with the wheel
they draw; everything else that touches `windows-rs` is.

- `instance`: named mutex single-instance guard
- `hooks`: `WH_KEYBOARD_LL` / `WH_MOUSE_LL` on an owned thread with its own message loop
- `watcher`: `SetWinEventHook` subscriptions, drained by the main thread
- `apps`: the window model, its taskbar visibility predicate, and AppUserModelID /
  exe-path grouping
- `window_class` / `window_text`: class names, titles and the owning executable
- `activate`: foreground, restore, minimize, launch
- `icons`: extraction and the in-memory cache
- `monitor`: monitor geometry, per-monitor DPI, fullscreen foreground detection
- `clipboard` / `synth_input`: snippet delivery
- `known_folders`: `%APPDATA%` resolution via `SHGetKnownFolderPath`
- `tray`: tray icon and its menu

### kile-wheel-9000 (wheel binary)

A thin `main.rs` over a library, so the parts worth testing are not stuck behind the
manifest's elevation request.

- `lib`: startup order, logging, single-instance handoff
- `app`: the message loop and event dispatch
- `overlay`: the hidden topmost window, its styles and lifecycle
- `render`: D3D11 + DirectComposition + Direct2D + DirectWrite drawing
- `slots`: what each slot shows
- `state`: selection, mode (normal / sub-wheel / edit), animation clocks
- `theme`: sizes and colours, scaled per monitor DPI

## Threading

Two long-lived threads plus short-lived workers. The split exists for one reason: a
slow render must never stall a low-level hook callback, because Windows silently
unhooks hooks that exceed `LowLevelHooksTimeout`.

| Thread | Owns | Must never |
|---|---|---|
| main | overlay window, WinEvent hooks, window model, wheel state, render | block on I/O |
| input | `WH_KEYBOARD_LL`, `WH_MOUSE_LL`, own `GetMessage` loop | allocate or lock on the hook path |
| worker (ad hoc) | `ShellExecuteEx`, `CreateProcess`, icon extraction, config write | touch wheel state directly |

The window model lives on the main thread with a single owner, so the WinEvent
callbacks and the renderer need no locking between them.

### Shared state across the hook boundary

The hook callback needs two things and nothing else:

- `AtomicBool` for "wheel is open", so the swallow-everything rule is a single load
- `RwLock<Arc<BindingTable>>` for the compiled bindings, written only on config reload

`RwLock` is used rather than a lock-free swap because writes happen on config reload
only, so the read side is uncontended. Keeping it in `std` avoids a dependency for no
measured gain.

Semantic events travel hook thread to main thread as an MPSC queue push plus a
`PostMessage` wake, so the hook returns immediately.

## Open path

The 50 ms budget is met by doing nothing on the hot path that could have been done
earlier.

1. Hook callback matches a binding, stores the event, flips `wheel_open`, returns 1 to
   swallow, and posts a wake to main.
2. Main reads the already-maintained window model, runs slot assignment, and draws one
   frame from cached icons and cached text layouts.
3. Main presents, then `ShowWindow(SW_SHOWNOACTIVATE)` and `SetWindowPos(HWND_TOPMOST)`.

Presenting before showing avoids a frame of stale content. No enumeration, no icon
extraction, no file I/O, no process spawn happens between steps 1 and 3.

## Rendering

### Why DirectComposition

A layered window with `UpdateLayeredWindow` needs a CPU-side bitmap blit per frame and
scales badly with wheel size and monitor DPI. DirectComposition composites on the GPU
and supports premultiplied alpha natively, so the overlay gets true per-pixel
transparency at the compositor with no blit.

### Window

`WS_POPUP`, extended with:

| Style | Reason |
|---|---|
| `WS_EX_TOPMOST` | sits above the focused app |
| `WS_EX_NOACTIVATE` | the app underneath keeps focus, which snippet paste depends on |
| `WS_EX_TOOLWINDOW` | keeps the overlay out of Alt+Tab and off the taskbar |
| `WS_EX_NOREDIRECTIONBITMAP` | required for a DComp-composited swapchain |

Mouse input comes from the `WH_MOUSE_LL` hook rather than window messages, because the
wheel has to swallow those clicks anyway. The window itself handles no mouse messages.

### Pipeline

`ID3D11Device` to `IDXGIDevice` to `IDCompositionDevice`, one `IDCompositionTarget` for
the HWND and one `IDCompositionVisual` whose content is an `IDXGISwapChain1` created
with `DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL` and `DXGI_ALPHA_MODE_PREMULTIPLIED`. Direct2D
draws into the swapchain's back buffer; DirectWrite lays out text; WIC decodes user
icon images. The frame-latency waitable object keeps present latency to one frame.

## Running-window model

One `EnumWindows` pass at startup, then incremental updates from out-of-context
WinEvent hooks for create, destroy, show, hide, foreground, name change, cloak and
uncloak. Nothing re-enumerates on open.

Grouping key is the AppUserModelID from `SHGetPropertyStoreForWindow` when present,
falling back to the executable path from `QueryFullProcessImageNameW`. UWP apps need
the AUMID, so the fallback is not sufficient on its own.

### Taskbar visibility

The predicate approximates what the taskbar shows:

- `IsWindowVisible`
- no owner, or `WS_EX_APPWINDOW` set
- `WS_EX_TOOLWINDOW` not set
- non-empty title
- for `ApplicationFrameWindow` and `Windows.UI.Core.CoreWindow`, excluded when
  `DWMWA_CLOAKED` is set, which filters suspended UWP frames
- shell classes excluded by name: `Shell_TrayWnd`, `Progman`, `WorkerW`

`DWM_CLOAKED_SHELL` alone is not an exclusion, because that is also how a window on
another virtual desktop or a hidden komorebi workspace presents, and those still count
as open.

**Risk:** this predicate is a heuristic. It is the most likely thing to need tuning
during phase 1 testing against a real desktop.

## Selection math

### Latching

One rule covers every case in the spec, which avoids a branch per transition:

- if exactly one cardinal is latched and the pressed key is perpendicular to it, latch
  the diagonal of the two
- otherwise latch the pressed key alone as a cardinal

Pressing the same key, the opposite key, or a key while a diagonal is latched all fall
into the second case, which is what the spec asks for.

### Angle

Screen y grows downward, so `atan2(dx, -dy)` yields 0 at north increasing clockwise.
With `SECTOR = TAU / SLOT_COUNT`, the slot is
`floor((angle + SECTOR / 2) / SECTOR) mod SLOT_COUNT`. Inside the hub radius the angle
is ignored and selection is unchanged.

## Slot assignment

Pinned items are placed first at their configured tab and slot. Unpinned running apps
are then placed by walking tabs and slots in order. The stability rule is held by
keeping a map from group key to assigned position for as long as the group has a
window, so a close elsewhere never moves anything. Freed positions return to the pool
for the next launch.

## Configuration

`%APPDATA%\kile-wheel-9000\` holds `config.toml`, `icons\` and `logs\`. The app writes
a default `config.toml` on first run, so the schema exists in exactly one place rather
than being duplicated in `install.ps1`.

Reads go through serde. Writes go through `toml_edit` against the existing document, so
hand-written comments and ordering survive a settings-window edit. Writes are atomic:
temp file in the same directory, then rename.

Live reload watches the file. A parse failure keeps the last good config in memory,
logs the error, and raises a tray notification. It never replaces good config with
nothing.

## Errors and logging

`thiserror` for typed errors in `core` and `platform`, `anyhow` at the binary
boundary where the context chain is what matters. Failure messages name what failed,
why, and the next action, which is the same standard the install script holds.

`tracing` with a rolling file appender in `logs\`. Hook callbacks do not log on the hot
path.

## Testing

`core` carries the unit tests, because that is where the rules live: the latching state
machine's transition table, angle-to-slot across all 8 sectors and the dead zone, slot
assignment including stability across simulated open and close sequences, and config
round-trips that assert comments survive.

`platform` and the renderer need a real desktop and are verified by hand against the
phase checklist, including the elevated-foreground and komorebi cases.

## Open risks

| Risk | Resolution |
|---|---|
| Taskbar visibility heuristic misclassifies windows | tune in phase 1 against a real desktop |
| Foreground activation may not pull a hidden komorebi workspace into view | verify in phase 1; add the optional `komorebic` fallback only if it fails, checking `komorebic --help` for the real command rather than assuming one |
| Notification badge overlays (Teams "9+") may not be readable from another process | stretch goal, explicitly not a blocker |
| Hidden-window Present may be throttled, showing a stale first frame | if observed, pre-render on state change instead of on open |
