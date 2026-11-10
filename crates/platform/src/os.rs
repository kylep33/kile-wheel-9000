//! What differs between desktops in words and features, in one place, so the wheel and
//! the settings window never sprinkle `cfg` checks through their own code.

/// Facts about the desktop this build targets.
#[derive(Debug, Clone, Copy)]
pub struct Desktop {
    /// Where folders open, for labels.
    pub file_manager: &'static str,
    /// How to reach a window that did not fit on the sub-wheel.
    pub window_switcher: &'static str,
    /// Windows Terminal profiles and WSL distros only exist on Windows.
    pub has_terminal_profiles: bool,
    /// What a pinned app's target can be, for the settings window's hint.
    pub app_target_hint: &'static str,
    /// File types the pinned-app picker offers.
    pub app_file_types: &'static [&'static str],
    /// What a start directory means for a terminal item.
    pub terminal_cwd_hint: &'static str,
    /// The name the `Win` modifier goes by here.
    pub super_key: &'static str,
    /// What sets up starting at login, for the settings window.
    pub autostart_note: &'static str,
    /// The path variables `paths::expand` understands, for hints.
    pub variable_hint: &'static str,
}

#[cfg(target_os = "windows")]
pub const DESKTOP: Desktop = Desktop {
    file_manager: "Explorer",
    window_switcher: "Alt-Tab",
    has_terminal_profiles: true,
    app_target_hint: "An exe, a shortcut, or shell:AppsFolder\\<AppUserModelID>.",
    app_file_types: &["exe", "lnk", "bat", "cmd"],
    terminal_cwd_hint: "A Windows path, or a Linux path when a WSL distro is set.",
    super_key: "Win",
    autostart_note: "Managed by install.ps1, which registers the elevated logon task. Run uninstall.ps1 to remove it.",
    variable_hint: "Variables such as %USERPROFILE%",
};

#[cfg(target_os = "linux")]
pub const DESKTOP: Desktop = Desktop {
    file_manager: "Files",
    window_switcher: "Super-Tab",
    has_terminal_profiles: false,
    app_target_hint: "A .desktop id like firefox_firefox.desktop, a .desktop file, or a program.",
    app_file_types: &["desktop", "sh", "AppImage"],
    terminal_cwd_hint: "The folder the terminal starts in.",
    super_key: "Super",
    autostart_note: "Managed by install.sh, which adds an autostart entry. Run uninstall.sh to remove it.",
    variable_hint: "~ and variables such as $HOME",
};

#[cfg(target_os = "macos")]
pub const DESKTOP: Desktop = Desktop {
    file_manager: "Finder",
    window_switcher: "Cmd-Tab",
    has_terminal_profiles: false,
    app_target_hint: "An .app bundle, or a bundle id like com.apple.Safari.",
    app_file_types: &["app", "sh", "command"],
    terminal_cwd_hint: "The folder the terminal starts in.",
    super_key: "Cmd",
    autostart_note: "Managed by install.sh, which adds a login agent. Run uninstall.sh to remove it.",
    variable_hint: "~ and variables such as $HOME",
};
