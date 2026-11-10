//! A pinned .desktop entry is known by its id, which is what GNOME groups windows by,
//! and by the program it runs, for windows the shell has no app for.

use super::desktop;
use crate::identity::AppIdentity;

pub const CASE_INSENSITIVE: bool = false;

pub fn id_in_text(target: &str) -> &str {
    match desktop::is_desktop_target(target) {
        true => desktop::id_of(target),
        false => target,
    }
}

pub fn enrich(target: &str, identity: &mut AppIdentity) {
    if !desktop::is_desktop_target(target) {
        return;
    }
    let Some(entry) = desktop::find(target) else {
        return;
    };
    if let Some(program) = entry.program.as_deref().and_then(desktop::resolve_program) {
        identity.add_path(program.to_string_lossy().into_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::AppKey;

    #[test]
    fn a_desktop_path_matches_the_id_gnome_reports() {
        let identity =
            AppIdentity::unresolved("/usr/share/applications/org.gnome.Nautilus.desktop");

        assert!(identity.matches(&AppKey::Id("org.gnome.Nautilus.desktop".into()), None));
    }
}
