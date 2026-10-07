# Requirements

## Introduction

kile-wheel-9000 is a hotkey-triggered radial menu for Windows 10/11 x64 that mirrors
the Windows taskbar on an 8-slot wheel across up to 9 tabs. It launches and switches
apps fast enough to build muscle memory, and also holds custom slots for scripts,
URLs, text snippets, folders and terminals.

The Windows taskbar is not removed or replaced. The wheel runs alongside it.

Speed is the governing constraint. No feature may add perceptible latency.

### Non-goals

- Removing or replacing the Windows taskbar
- Replicating the system tray or notification area
- Hold-to-open / release-to-select. The wheel is toggle-only.

---

## Requirement 1: Activation

**User story:** As a user, I want one key to summon the wheel from any app, so that
switching never costs me a trip to the mouse.

### Acceptance criteria

1. The system SHALL capture hotkeys with `WH_KEYBOARD_LL` and `WH_MOUSE_LL`, not
   `RegisterHotKey`, so modifier-only keys, Caps Lock and mouse buttons can bind.
2. The system SHALL support any number of bindings, each one key, key combo, or mouse
   button including XButton1/XButton2, and each mapped to exactly one of Open, Close
   or Toggle.
3. The default configuration SHALL bind Caps Lock to Open.
4. WHEN a Caps Lock press is consumed as a hotkey THEN the system SHALL swallow it so
   the Caps Lock toggle state does not change.
5. WHEN Shift+Caps Lock is pressed THEN the system SHALL pass it through so real Caps
   Lock still works.
6. WHEN the open hotkey is pressed while the wheel is open THEN the system SHALL close
   the wheel only if `open_hotkey_closes` is enabled, which defaults to off.
7. WHEN Esc is pressed from the wheel, a sub-wheel, or edit mode THEN the system SHALL
   close the wheel completely.
8. WHEN Backspace is pressed THEN the system SHALL back out exactly one level:
   sub-wheel to parent, edit mode to normal mode, top-level wheel to closed.
9. WHEN 1 through 9 is pressed while the wheel is open THEN the system SHALL switch to
   that tab.
10. WHILE the wheel is open the system SHALL swallow every key press, including keys it
    does not use, so nothing reaches the focused app.
11. WHEN `disable_in_fullscreen` is enabled, which is the default, and the foreground
    window covers its monitor THEN the system SHALL ignore hotkeys. The desktop and
    shell windows SHALL NOT count as fullscreen.

---

## Requirement 2: Selection

**User story:** As a user, I want to pick a slot by direction rather than by precise
aim, so that selection is a reflex instead of a task.

### Acceptance criteria

1. The wheel SHALL always present 8 slots, with slot 0 at north and the remaining
   slots in clockwise order.
2. The system SHALL map W to N, D to E, S to S, A to W, and the perpendicular pairs to
   the four diagonals. Arrow keys SHALL behave the same as WASD.
3. WHEN a single cardinal direction is latched and a perpendicular key is pressed THEN
   the system SHALL latch the diagonal between them, in either press order.
4. WHEN a key is pressed that is already part of the latch, or opposite a key in the
   latch THEN the system SHALL start a fresh latch on that single cardinal direction.
5. The system SHALL NOT require keys to be held together.
6. WHEN Enter is pressed THEN the system SHALL activate the selected slot.
7. The system SHALL select the slot matching the cursor's angle from the wheel center,
   with no hover precision required, and SHALL ignore angle inside a center dead zone
   the size of the hub.
8. WHEN the left mouse button is clicked THEN the system SHALL activate the selected
   slot, except on the hub cog, which SHALL open settings.
9. WHEN `center_cursor` is enabled, which is the default, THEN the system SHALL move the
   cursor to the wheel center on open.
10. The system SHALL let mouse and keyboard mix freely, with the most recent input
    owning the selection.

---

## Requirement 3: Wheel presentation

**User story:** As a user, I want the wheel to read at a glance and never make me wait
on an animation.

### Acceptance criteria

1. The system SHALL center the wheel on the monitor under the cursor by default, or on
   the primary monitor when configured.
2. WHEN a slot is empty THEN the system SHALL show a random quip drawn from a
   configurable list.
3. The hub SHALL show the current tab number, the tab's name and icon when set, the
   settings cog, and the date and time when enabled.
4. Each slot SHALL indicate: a running app with a short pill, the focused app with a
   longer or brighter pill, an app with multiple windows with a stacked or doubled
   pill, and a pinned item with a corner pin glyph.
5. The system SHALL animate hover highlight, select confirmation, and open/close.
6. Every animation SHALL complete within 100 ms and SHALL NOT delay input or
   activation.
7. The system SHALL support an animation speed setting of normal, fast or off.
8. The system SHALL use a dark theme by default and SHALL expose accent color, wheel
   size and opacity settings.

---

## Requirement 4: Tabs and slot assignment

**User story:** As a user, I want an app to stay in the slot I learned it in, so that
muscle memory keeps paying off.

### Acceptance criteria

1. The system SHALL support up to 9 tabs, each with an optional name and icon and
   exactly 8 slots.
2. A pinned item SHALL occupy its configured tab and slot.
3. The system SHALL place unpinned running apps in empty slots in tab then slot order,
   starting at tab 1 slot 0, and SHALL auto-create overflow tabs up to the limit of 9.
4. WHEN every slot in every tab is occupied THEN the last slot of tab 9 SHALL become a
   "More..." item that opens a sub-wheel.
5. WHILE an unpinned app keeps running the system SHALL keep it in the same slot, and
   SHALL NOT reshuffle slots when other apps open or close.
6. WHEN a slot is freed THEN the next app to launch SHALL take it.
7. The system SHALL group windows by AppUserModelID when available and otherwise by
   executable path.
8. The system SHALL include only windows the taskbar would show: visible, unowned, not
   tool windows, and not cloaked by the shell.
9. A window on a hidden komorebi workspace SHALL still count as open.

---

## Requirement 5: Slot types

**User story:** As a user, I want the wheel to hold anything I start my day with, not
just applications.

### Acceptance criteria

1. Every item SHALL have a name and an icon, where the icon is either a file in the
   config's `icons/` folder or extracted automatically from the target.
2. Every item type SHALL be pinnable to a tab and slot.
3. WHEN an app item with no open window is activated THEN the system SHALL launch it.
4. WHEN an app item with one open window is activated THEN the system SHALL focus that
   window, restoring it if minimized.
5. WHEN an app item's single window is already focused THEN the system SHALL minimize
   it, subject to `minimize_if_focused`, which defaults to on.
6. WHEN an app item with more than one window is activated THEN the system SHALL open a
   sub-wheel of those windows labelled by window title, overflowing past 8 into a
   "More..." slot.
7. WHEN Shift+Enter or middle-click activates an app item THEN the system SHALL launch
   a new instance.
8. Activation SHALL reliably take foreground from a background process, including for
   windows on other monitors and on hidden komorebi workspaces.
9. A script item SHALL run `exec` with `{path}` substituted, honoring optional `cwd`
   and `hidden`.
10. A URL item SHALL open in the configured browser with optional `browser_args`, or in
    the system default browser when no browser is set.
11. WHEN a snippet item is activated THEN the system SHALL close the wheel, restore
    focus to the previously focused window, and deliver the text by clipboard with
    save and restore of the prior clipboard, or by synthetic keystrokes in `type` mode.
12. A folder item SHALL open File Explorer at its path, with environment variables
    expanded.
13. A terminal item SHALL open Windows Terminal with the configured profile, optional
    WSL distro, and starting directory.

---

## Requirement 6: Edit mode

**User story:** As a user, I want to rearrange the wheel from the wheel itself.

### Acceptance criteria

1. WHEN a slot or the hub cog is held for at least 400 ms THEN the system SHALL toggle
   edit mode.
2. WHEN Backspace is pressed in edit mode THEN the system SHALL return to normal mode.
3. Edit mode SHALL be visually distinct, with slot outlines and an EDIT label on the hub.
4. In edit mode the system SHALL support drag to move or swap slots, moving a dragged
   item to another tab by pressing 1 through 9, pinning and unpinning a running app,
   removing an item, adding an item of any type to an empty slot, and editing a filled
   slot's name, icon and fields.
5. WHEN an edit is made THEN the system SHALL write `config.toml` immediately using a
   temp file and rename.

---

## Requirement 7: Settings and configuration

**User story:** As a user, I want one portable folder that holds my whole setup.

### Acceptance criteria

1. All user state SHALL live in `%APPDATA%\kile-wheel-9000\`, holding `config.toml`,
   `icons\` and `logs\`.
2. Icon paths in the config SHALL be relative to the config folder, and item paths
   SHALL support environment variables, so the folder ports between machines.
3. The system SHALL watch `config.toml` and apply hand edits without a restart.
4. WHEN `config.toml` fails to parse THEN the system SHALL keep the last good config,
   continue running, and surface the parse error in a tray notification.
5. The system SHALL write the config with `toml_edit` so hand-written comments and
   formatting survive.
6. The settings window SHALL expose General, Hotkeys, Wheel, Tabs, Items and About
   sections, with hotkey capture by key press and an icon picker for items.
7. The settings UI SHALL enumerate Windows Terminal profiles from WT's `settings.json`
   and WSL distros from `wsl.exe -l -q`.
8. The settings UI SHALL warn that snippets are stored as plain text in `config.toml`.
9. WHEN Backspace is pressed in settings THEN the system SHALL go back one page, and
   Esc SHALL close settings.

---

## Requirement 8: Process and lifecycle

**User story:** As a user, I want one instance that starts with Windows and stays out
of the way.

### Acceptance criteria

1. The system SHALL run elevated so UIPI does not block input capture while an
   elevated window is focused.
2. The system SHALL be per-monitor DPI aware v2.
3. The system SHALL enforce a single instance with a named mutex, and a second launch
   SHALL surface the existing wheel or settings rather than start a new process.
4. The system SHALL provide a tray icon with Open Settings, Reload Config and Quit.
5. The system SHALL log to a rotating file in the config folder.

---

## Requirement 9: Performance

**User story:** As a user, I want the wheel to feel instant, because that is the whole
point.

### Acceptance criteria

1. WHEN the open hotkey is pressed THEN the wheel SHALL be visible within 50 ms,
   ideally on the next frame.
2. The overlay window SHALL be created at startup and kept hidden, so opening only
   shows it and updates state.
3. The system SHALL maintain the running-window list incrementally from WinEvent hooks
   and SHALL NOT enumerate all windows on open.
4. The system SHALL extract each icon once and cache it in memory.
5. Launching and activating SHALL NOT block the UI or input threads.
6. Low-level hook callbacks SHALL return without doing heavy work, so Windows does not
   unhook them for exceeding the timeout.
7. The system SHALL use approximately 0% CPU when idle and SHALL target under 60 MB of
   memory.

---

## Requirement 10: Install and update

**User story:** As a user, I want one script that installs the app and updates it later.

### Acceptance criteria

1. `install.ps1` SHALL be idempotent: the first run installs and later runs update.
2. `install.ps1` SHALL self-elevate when not already admin.
3. `install.ps1` SHALL install missing prerequisites, reporting "installing prereqs if
   not present", and SHALL skip any prerequisite already present.
4. `install.ps1` SHALL build in release mode, stop any running instance, and copy the
   binary to `%LOCALAPPDATA%\Programs\kile-wheel-9000\`.
5. `install.ps1` SHALL NOT overwrite an existing `config.toml` or `icons\`.
6. `install.ps1` SHALL register startup through Task Scheduler with highest privileges
   at logon, because a Run key entry cannot start an elevated app without a UAC prompt.
7. `install.ps1` SHALL start the app and print the installed version, paths and hotkey.
8. `uninstall.ps1` SHALL remove the binary and the scheduled task, and SHALL ask before
   deleting the config.
9. Both scripts SHALL run without interactive input apart from UAC and the uninstall
   config prompt, so they work unattended.
