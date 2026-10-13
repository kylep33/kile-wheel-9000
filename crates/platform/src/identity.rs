//! Which running app a launch target stands for.
//!
//! A pinned slot is configured by what starts it: an exe, a taskbar shortcut, or
//! `shell:AppsFolder\<id>`. Running windows are grouped by AppUserModelID or by exe path.
//! Those only line up for a bare exe, so a pinned shortcut would never find its own
//! windows and the app would show up twice: once pinned, once in an unpinned slot.
//!
//! Shortcuts and Apps folder entries are resolved through the shell to the exe and id
//! they launch. That reads the shell property store, so each target is resolved once and
//! remembered, never while the wheel is opening.

use std::collections::HashMap;
use std::path::Path;

use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Storage::EnhancedStorage::{PKEY_AppUserModel_ID, PKEY_Link_TargetParsingPath};
use windows::Win32::UI::Shell::PropertiesSystem::{
    GPS_DEFAULT, IPropertyStore, SHGetPropertyStoreFromParsingName,
};
use windows::core::HSTRING;

use crate::apps::AppKey;
use crate::paths::expand;

const APPS_FOLDER_PREFIX: &str = r"shell:AppsFolder\";

/// Every exe path and AppUserModelID a launch target is known by.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppIdentity {
    paths: Vec<String>,
    ids: Vec<String>,
}

impl AppIdentity {
    /// Asks the shell what the target launches, on top of what its text says.
    pub fn resolve(target: &str) -> Self {
        let expanded = expand(target);
        let mut identity = Self::unresolved(&expanded);

        // a missing file or an unknown Apps folder id is not an error: the text alone
        // still matches a bare exe or id
        if let Some(store) = property_store(&expanded) {
            if let Some(path) = read_text(&store, &PKEY_Link_TargetParsingPath) {
                identity.add_path(path);
            }
            if let Some(id) = read_text(&store, &PKEY_AppUserModel_ID) {
                identity.add_id(id);
            }
        }

        identity
    }

    /// What the target says about itself without asking the shell. Enough for a bare exe,
    /// a bare AppUserModelID, or `shell:AppsFolder\<id>`.
    pub fn unresolved(target: &str) -> Self {
        let mut identity = Self::default();
        identity.add_path(target.to_owned());
        identity.add_id(apps_folder_id(target).unwrap_or(target).to_owned());
        identity
    }

    /// Whether a running group is this app.
    ///
    /// A group keyed by AppUserModelID is also checked by its exe, because a pinned
    /// shortcut usually names the exe while the running window reports an id, as Chrome
    /// does per profile.
    pub fn matches(&self, key: &AppKey, executable: Option<&Path>) -> bool {
        match key {
            AppKey::Executable(path) => self.has_path(path),
            AppKey::UserModelId(id) => {
                self.has_id(id) || executable.is_some_and(|path| self.has_path(path))
            }
        }
    }

    // Windows paths and app ids are case-insensitive, so comparing them as written would
    // miss
    fn has_path(&self, path: &Path) -> bool {
        path.to_str().is_some_and(|path| {
            self.paths
                .iter()
                .any(|known| known.eq_ignore_ascii_case(path))
        })
    }

    fn has_id(&self, id: &str) -> bool {
        self.ids.iter().any(|known| known.eq_ignore_ascii_case(id))
    }

    fn add_path(&mut self, path: String) {
        if !path.is_empty() && !self.paths.contains(&path) {
            self.paths.push(path);
        }
    }

    fn add_id(&mut self, id: String) {
        if !id.is_empty() && !self.ids.contains(&id) {
            self.ids.push(id);
        }
    }
}

/// Resolved identities, remembered per target as written in the config.
#[derive(Debug, Default)]
pub struct IdentityCache {
    known: HashMap<String, AppIdentity>,
}

impl IdentityCache {
    /// Resolves on first sight only, so later opens are a lookup.
    pub fn identity(&mut self, target: &str) -> &AppIdentity {
        self.known
            .entry(target.to_owned())
            .or_insert_with(|| AppIdentity::resolve(target))
    }

    /// Forgets everything, for a config reload, so a shortcut that was re-pointed is read
    /// again.
    pub fn clear(&mut self) {
        self.known.clear();
    }
}

/// The id in `shell:AppsFolder\<id>`, whatever the case of the prefix.
fn apps_folder_id(target: &str) -> Option<&str> {
    let prefix = target.get(..APPS_FOLDER_PREFIX.len())?;
    prefix
        .eq_ignore_ascii_case(APPS_FOLDER_PREFIX)
        .then(|| &target[APPS_FOLDER_PREFIX.len()..])
}

fn property_store(target: &str) -> Option<IPropertyStore> {
    // SAFETY: the path string outlives the call and the interface is released on drop.
    unsafe { SHGetPropertyStoreFromParsingName(&HSTRING::from(target), None, GPS_DEFAULT) }.ok()
}

fn read_text(store: &IPropertyStore, key: &PROPERTYKEY) -> Option<String> {
    // SAFETY: the property key is a static constant owned by the shell.
    let value = unsafe { store.GetValue(key) }.ok()?;

    let text = value.to_string();
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exe(path: &str) -> AppKey {
        AppKey::Executable(path.into())
    }

    fn id(text: &str) -> AppKey {
        AppKey::UserModelId(text.to_owned())
    }

    #[test]
    fn a_bare_exe_matches_its_running_group_whatever_the_case() {
        let identity = AppIdentity::unresolved(r"C:\Apps\Code.exe");

        assert!(identity.matches(&exe(r"c:\apps\code.exe"), None));
    }

    #[test]
    fn a_bare_exe_does_not_match_a_different_executable() {
        let identity = AppIdentity::unresolved(r"C:\Apps\Code.exe");

        assert!(!identity.matches(&exe(r"C:\Apps\Other.exe"), None));
    }

    #[test]
    fn a_bare_user_model_id_matches_its_group() {
        let identity = AppIdentity::unresolved("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App");

        assert!(identity.matches(&id("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App"), None));
    }

    #[test]
    fn an_apps_folder_entry_matches_by_the_id_after_the_prefix() {
        let identity = AppIdentity::unresolved(r"shell:AppsFolder\Some.App_abc!App");

        assert!(identity.matches(&id("Some.App_abc!App"), None));
    }

    #[test]
    fn the_apps_folder_prefix_is_matched_whatever_its_case() {
        assert_eq!(apps_folder_id(r"SHELL:appsfolder\Thing"), Some("Thing"));
        assert_eq!(apps_folder_id(r"C:\Thing.exe"), None);
        assert_eq!(apps_folder_id("short"), None);
    }

    /// Chrome reports a per-profile id on its windows while its pin names chrome.exe, so a
    /// group keyed by id has to be claimable through its executable too.
    #[test]
    fn a_group_keyed_by_id_is_matched_through_its_executable() {
        let identity = AppIdentity::unresolved(r"C:\Program Files\Google\Chrome\chrome.exe");
        let chrome = Path::new(r"C:\Program Files\Google\Chrome\chrome.exe");

        assert!(identity.matches(&id("Chrome.UserData.Profile2"), Some(chrome)));
        assert!(!identity.matches(&id("Chrome.UserData.Profile2"), None));
    }

    /// The case that put Kiro on the wheel twice: a pin that is a taskbar shortcut, with
    /// the running window keyed by the exe the shortcut launches.
    #[test]
    fn a_shortcut_matches_the_exe_it_was_resolved_to() {
        let mut identity = AppIdentity::unresolved(r"C:\pins\Kiro.lnk");
        assert!(!identity.matches(&exe(r"C:\Programs\Kiro\Kiro.exe"), None));

        identity.add_path(r"C:\Programs\Kiro\Kiro.exe".to_owned());
        assert!(identity.matches(&exe(r"C:\Programs\Kiro\Kiro.exe"), None));
    }

    #[test]
    fn empty_and_repeated_names_are_not_kept() {
        let mut identity = AppIdentity::default();
        identity.add_path(String::new());
        identity.add_path("a.exe".to_owned());
        identity.add_path("a.exe".to_owned());

        assert_eq!(identity.paths, ["a.exe"]);
    }

    /// Resolving runs against the real shell, which must never panic or fail outright,
    /// whatever the target.
    #[test]
    fn resolving_unknown_targets_falls_back_to_the_text() {
        let missing = AppIdentity::resolve(r"C:\definitely\not\here.lnk");
        assert!(missing.matches(&exe(r"C:\definitely\not\here.lnk"), None));

        let unknown = AppIdentity::resolve(r"shell:AppsFolder\Not.A.Real.App!Nope");
        assert!(unknown.matches(&id("Not.A.Real.App!Nope"), None));
    }

    #[test]
    fn a_real_exe_still_matches_itself_after_resolving() {
        let explorer = expand(r"%SystemRoot%\explorer.exe");

        assert!(AppIdentity::resolve(&explorer).matches(&exe(&explorer), None));
    }

    #[test]
    fn the_cache_resolves_a_target_once() {
        let mut cache = IdentityCache::default();
        cache.identity(r"C:\Apps\Code.exe");
        cache.identity(r"C:\Apps\Code.exe");

        assert_eq!(cache.known.len(), 1);
    }
}
