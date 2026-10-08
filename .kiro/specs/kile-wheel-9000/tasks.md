# Implementation Plan

Phases match the build order in the source spec. Each phase ends in something usable
and gets its own commit.

## Phase 0 — Scaffold

- [x] 0.1 Create the cargo workspace with `core`, `platform` and `wheel` crates
  - Shared dependency versions and lint config in the workspace manifest
  - `APP_NAME` constant in `core`, consumed by the binary and matched by `app-common.ps1`
  - Pin the toolchain in `rust-toolchain.toml` for reproducible builds
  - _Requirements: 10.1, 10.3_

- [x] 0.2 Write `install.ps1`, `uninstall.ps1` and `app-common.ps1`
  - Idempotent prereq stage that installs rustup and the MSVC C++ build tools only
    when missing
  - Build, stop, copy, config-dir, logon task, start, summary
  - _Requirements: 10.1 - 10.9_

## Phase 1 — Core wheel

Goal: Caps Lock opens an 8-slot wheel of running apps, direction selects, Enter or
click activates, within the 50 ms budget.

- [x] 1.1 Single instance, elevation manifest and DPI awareness
  - Named mutex guard; a second launch signals the first instead of running
  - Embed a manifest requesting `requireAdministrator` and per-monitor DPI v2
  - _Requirements: 8.1, 8.2, 8.3_

- [x] 1.2 Logging to a rotating file in the config folder
  - Resolve `%APPDATA%` through `SHGetKnownFolderPath`, not an env var read
  - _Requirements: 7.1, 8.5_

- [x] 1.3 Binding model and the direction-key state machine in `core`
  - Parse bindings from key, combo and mouse-button forms into a lookup table
  - Selection reads the set of keys physically held, resolving each axis on its own
  - Unit tests for press and release sequences, and for angle-to-slot across all sectors
    plus the dead zone
  - _Requirements: 1.2, 2.2, 2.3, 2.4, 2.5, 2.6, 2.7_

- [x] 1.4 Low-level hook thread
  - `WH_KEYBOARD_LL` and `WH_MOUSE_LL` installed on an owned thread with its own
    message loop
  - Swallow decision is one atomic load plus one uncontended read lock, then return
  - Caps Lock consumed without changing toggle state; Shift+Caps Lock passed through
  - Swallow every key while the wheel is open
  - Semantic events forwarded to main by MPSC push plus `PostMessage` wake
  - _Requirements: 1.1, 1.3, 1.4, 1.5, 1.7, 1.8, 1.10, 9.6_

- [x] 1.5 Overlay window
  - Created hidden at startup, `WS_POPUP` with topmost, noactivate, toolwindow and
    noredirectionbitmap
  - Sized and centered on the monitor under the cursor
  - _Requirements: 3.1, 9.2_

- [x] 1.6 DirectComposition and Direct2D renderer
  - D3D11 device to DComp device, one target and one visual backed by a flip-model
    swapchain with premultiplied alpha
  - Draw 8 slots, the hub, and the hover highlight with DirectWrite text
  - Present before show so the first frame is never stale
  - _Requirements: 3.3, 3.5, 9.1_

- [x] 1.7 Running-window model
  - One `EnumWindows` at startup, then incremental WinEvent updates
  - Taskbar visibility predicate; group by AppUserModelID falling back to exe path
  - Tune the predicate against a real desktop
  - _Requirements: 4.7, 4.8, 4.9, 9.3_

- [ ] 1.8 Icon extraction and in-memory cache
  - Extract once per group key, off the main thread
  - Partly done: the cache is in and extraction happens once per key, but on the main
    thread. The warm-up frame pays for it before the first open, so it is not felt
  - _Requirements: 5.1, 9.4_

- [x] 1.9 Selection and activation
  - Mouse angle selection with the hub dead zone, cursor centering on open
  - Keyboard and mouse mix with last-moved owning the selection
  - Launch when nothing is open, focus when one window, minimize when already focused
  - Foreground must land across monitors and hidden komorebi workspaces
  - Launches run on a worker so input never blocks
  - _Requirements: 2.1, 2.6, 2.8, 2.9, 2.10, 5.3, 5.4, 5.5, 5.8, 9.5_

- [ ] 1.10 Measure and confirm the open budget
  - Instrument hook timestamp to first present and verify under 50 ms
  - Hand off for elevated testing against an elevated foreground window
  - Partly done: instrumented and measuring 6 to 14 ms against the 50 ms budget. The
    elevated foreground test still needs a real desktop
  - _Requirements: 9.1, 1.11_

## Phase 2 — Pinned items and indicators

- [x] 2.1 Config model, first-run default write, and atomic save
  - serde read, `toml_edit` write-back preserving comments, temp file then rename
  - _Requirements: 7.1, 7.2, 7.5, 6.5_

- [x] 2.2 Pinned apps placed from config at their tab and slot
  - _Requirements: 4.2, 5.2_

- [x] 2.3 Slot indicators: running pill, focused pill, multi-window pill, pin glyph
  - _Requirements: 3.4_

- [x] 2.4 Empty-slot quips drawn from the configurable list
  - _Requirements: 3.2_

- [x] 2.5 Sub-wheel for an app with more than one window
  - Labelled by window title, "More..." past 8, Backspace returns to the parent
  - _Requirements: 5.6, 1.8_

- [x] 2.6 Shift+Enter and middle-click launch a new instance
  - _Requirements: 5.7_

## Phase 3 — Tabs

- [x] 3.1 Tabs 1 through 9 with optional name and icon, switched by 1 through 9
  - _Requirements: 4.1, 1.9_

- [ ] 3.2 Overflow filling across tabs with auto-created overflow tabs
  - Dropped: superseded by unpinned app slots. Nothing auto-fills and no tab is created
    behind your back; a blank slot stays blank until you set it
  - _Requirements: 4.3_

- [x] 3.3 Slot stability across open and close churn
  - Group key to position map held while the group lives; freed slots reused on launch
  - _Requirements: 4.5, 4.6_

- [ ] 3.4 "More..." in the last slot of tab 9 when everything is full
  - Dropped: replaced by the hub overflow glyph and its note, which says how many apps
    found no unpinned slot and what to do about it
  - _Requirements: 4.4_

- [x] 3.5 Hub shows tab number, name, icon, date and time, and the cog
  - _Requirements: 3.3_

## Phase 4 — Remaining slot types

- [x] 4.1 Script items with `exec` template, `{path}` substitution, `cwd` and `hidden`
  - _Requirements: 5.9_

- [x] 4.2 URL items with optional browser and browser args
  - _Requirements: 5.10_

- [x] 4.3 Snippet items in clipboard and type modes
  - Close, restore prior focus, deliver, then restore the original clipboard
  - _Requirements: 5.11_

- [x] 4.4 Folder items opening Explorer with environment variables expanded
  - _Requirements: 5.12_

- [x] 4.5 Terminal items with profile, optional WSL distro and starting directory
  - _Requirements: 5.13_

## Phase 5 — Edit mode

- [x] 5.1 Hold for 400 ms on a slot or the cog toggles edit mode
  - _Requirements: 6.1, 6.2_

- [x] 5.2 Edit-mode visual state with slot outlines and the EDIT hub label
  - _Requirements: 6.3_

- [x] 5.3 Drag to move or swap, 1 through 9 while dragging to change tab
  - _Requirements: 6.4_

- [x] 5.4 Pin, unpin, remove, add to an empty slot, edit a filled slot
  - _Requirements: 6.4_

- [x] 5.5 Every edit saves immediately through the atomic writer
  - _Requirements: 6.5_

## Phase 6 — Settings window and the rest of input

- [x] 6.1 `settings` binary on eframe, launched from the cog and the tray
  - _Requirements: 7.6_

- [x] 6.2 General, Hotkeys, Wheel, Tabs, Items and About pages
  - Press-to-record hotkey capture, icon picker, plain-text snippet warning
  - Backspace goes back a page, Esc closes
  - _Requirements: 7.6, 7.8, 7.9_

- [ ] 6.3 Windows Terminal profile and WSL distro enumeration
  - Not started: profile and distro are typed in by hand for now
  - _Requirements: 7.7_

- [x] 6.4 Live reload with last-good-config retention and a tray parse-error notice
  - _Requirements: 7.3, 7.4_

- [x] 6.5 Multiple bindings including mouse buttons, and the open-also-closes toggle
  - _Requirements: 1.2, 1.6_

- [x] 6.6 Fullscreen suppression and the monitor choice setting
  - _Requirements: 1.11, 3.1_

## Phase 7 — Polish

- [x] 7.1 Tray icon with Open Settings, Reload Config and Quit
  - _Requirements: 8.4_

- [x] 7.2 Animations within the 100 ms ceiling, with the speed setting
  - _Requirements: 3.5, 3.6, 3.7_

- [x] 7.3 Theme settings: accent color, wheel size, opacity
  - _Requirements: 3.8_

- [ ] 7.4 Idle CPU and memory check against the targets
  - _Requirements: 9.7_

- [ ] 7.5 Stretch: notification badge overlays, only if reliably obtainable
  - _Requirements: 3.4_
