//! Which running app a launch target stands for.
//!
//! A pinned slot is configured by what starts it: an exe or shortcut on Windows, a
//! .desktop entry on GNOME, an .app bundle on macOS. Running windows are grouped by the
//! desktop's app id or by executable path. Those only line up for the simplest targets,
//! so a pinned shortcut would never find its own windows and the app would show up twice:
//! once pinned, once in an unpinned slot.
//!
//! Each target is resolved through the OS to the ids and paths it launches. That can
//! read files or ask the shell, so each target is resolved once and remembered, never
//! while the wheel is opening.

use std::collections::HashMap;
use std::path::Path;

use crate::apps::AppKey;
use crate::paths::expand;
use crate::sys;

/// Every exe path and app id a launch target is known by.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppIdentity {
    paths: Vec<String>,
    ids: Vec<String>,
}

impl AppIdentity {
    /// Asks the OS what the target launches, on top of what its text says.
    pub fn resolve(target: &str) -> Self {
        let expanded = expand(target);
        let mut identity = Self::unresolved(&expanded);

        // a missing file or an unknown id is not an error: the text alone still matches
        // a bare exe or id
        sys::identity::enrich(&expanded, &mut identity);
        identity
    }

    /// What the target says about itself without asking the OS. Enough for a bare exe or
    /// a bare app id.
    pub fn unresolved(target: &str) -> Self {
        let mut identity = Self::default();
        identity.add_path(target.to_owned());
        identity.add_id(sys::identity::id_in_text(target).to_owned());
        identity
    }

    /// Whether a running group is this app.
    ///
    /// A group keyed by id is also checked by its exe, because a pinned shortcut usually
    /// names the exe while the running window reports an id, as Chrome does per profile.
    pub fn matches(&self, key: &AppKey, executable: Option<&Path>) -> bool {
        match key {
            AppKey::Executable(path) => self.has_path(path),
            AppKey::Id(id) => {
                self.has_id(id) || executable.is_some_and(|path| self.has_path(path))
            }
        }
    }

    fn has_path(&self, path: &Path) -> bool {
        path.to_str()
            .is_some_and(|path| self.paths.iter().any(|known| same_name(known, path)))
    }

    fn has_id(&self, id: &str) -> bool {
        self.ids.iter().any(|known| same_name(known, id))
    }

    pub(crate) fn add_path(&mut self, path: String) {
        if !path.is_empty() && !self.paths.contains(&path) {
            self.paths.push(path);
        }
    }

    pub(crate) fn add_id(&mut self, id: String) {
        if !id.is_empty() && !self.ids.contains(&id) {
            self.ids.push(id);
        }
    }
}

/// Windows and macOS file systems and app ids ignore case; Linux does not.
fn same_name(left: &str, right: &str) -> bool {
    match sys::identity::CASE_INSENSITIVE {
        true => left.eq_ignore_ascii_case(right),
        false => left == right,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn exe(path: &str) -> AppKey {
        AppKey::Executable(path.into())
    }

    fn id(text: &str) -> AppKey {
        AppKey::Id(text.to_owned())
    }

    #[test]
    fn a_bare_exe_matches_its_running_group() {
        let identity = AppIdentity::unresolved("/apps/code");

        assert!(identity.matches(&exe("/apps/code"), None));
    }

    #[test]
    fn a_bare_exe_does_not_match_a_different_executable() {
        let identity = AppIdentity::unresolved("/apps/code");

        assert!(!identity.matches(&exe("/apps/other"), None));
    }

    #[test]
    fn a_bare_app_id_matches_its_group() {
        let identity = AppIdentity::unresolved("org.example.App");

        assert!(identity.matches(&id("org.example.App"), None));
    }

    /// Chrome reports a per-profile id on its windows while its pin names chrome.exe, so a
    /// group keyed by id has to be claimable through its executable too.
    #[test]
    fn a_group_keyed_by_id_is_matched_through_its_executable() {
        let identity = AppIdentity::unresolved("/opt/chrome/chrome");
        let chrome = Path::new("/opt/chrome/chrome");

        assert!(identity.matches(&id("Chrome.UserData.Profile2"), Some(chrome)));
        assert!(!identity.matches(&id("Chrome.UserData.Profile2"), None));
    }

    /// The case that put Kiro on the wheel twice: a pin that is a shortcut, with the
    /// running window keyed by the exe the shortcut launches.
    #[test]
    fn a_shortcut_matches_the_exe_it_was_resolved_to() {
        let mut identity = AppIdentity::unresolved("/pins/Kiro.lnk");
        assert!(!identity.matches(&exe("/programs/Kiro/Kiro"), None));

        identity.add_path("/programs/Kiro/Kiro".to_owned());
        assert!(identity.matches(&exe("/programs/Kiro/Kiro"), None));
    }

    #[test]
    fn empty_and_repeated_names_are_not_kept() {
        let mut identity = AppIdentity::default();
        identity.add_path(String::new());
        identity.add_path("a".to_owned());
        identity.add_path("a".to_owned());

        assert_eq!(identity.paths, ["a"]);
    }

    /// Resolving runs against the real OS, which must never panic or fail outright,
    /// whatever the target.
    #[test]
    fn resolving_an_unknown_target_falls_back_to_the_text() {
        let missing = AppIdentity::resolve("/definitely/not/here");

        assert!(missing.matches(&exe("/definitely/not/here"), None));
    }

    #[test]
    fn the_cache_resolves_a_target_once() {
        let mut cache = IdentityCache::default();
        cache.identity("/apps/code");
        cache.identity("/apps/code");

        assert_eq!(cache.known.len(), 1);
    }

    #[cfg(windows)]
    mod windows {
        use super::*;

        #[test]
        fn a_bare_exe_matches_whatever_the_case() {
            let identity = AppIdentity::unresolved(r"C:\Apps\Code.exe");

            assert!(identity.matches(&exe(r"c:\apps\code.exe"), None));
        }

        #[test]
        fn an_apps_folder_entry_matches_by_the_id_after_the_prefix() {
            let identity = AppIdentity::unresolved(r"shell:AppsFolder\Some.App_abc!App");

            assert!(identity.matches(&id("Some.App_abc!App"), None));
        }

        #[test]
        fn resolving_an_unknown_apps_folder_entry_falls_back_to_its_id() {
            let unknown = AppIdentity::resolve(r"shell:AppsFolder\Not.A.Real.App!Nope");

            assert!(unknown.matches(&id("Not.A.Real.App!Nope"), None));
        }

        #[test]
        fn a_real_exe_still_matches_itself_after_resolving() {
            let explorer = expand(r"%SystemRoot%\explorer.exe");

            assert!(AppIdentity::resolve(&explorer).matches(&exe(&explorer), None));
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_paths_keep_their_case() {
        let identity = AppIdentity::unresolved("/apps/Code");

        assert!(!identity.matches(&exe("/apps/code"), None));
    }
}
