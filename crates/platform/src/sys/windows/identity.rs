//! Shortcuts and Apps folder entries, resolved through the shell property store to the
//! exe and AppUserModelID they launch.

use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Storage::EnhancedStorage::{PKEY_AppUserModel_ID, PKEY_Link_TargetParsingPath};
use windows::Win32::UI::Shell::PropertiesSystem::{
    GPS_DEFAULT, IPropertyStore, SHGetPropertyStoreFromParsingName,
};
use windows::core::HSTRING;

use crate::identity::AppIdentity;

pub const CASE_INSENSITIVE: bool = true;

const APPS_FOLDER_PREFIX: &str = r"shell:AppsFolder\";

/// The id in `shell:AppsFolder\<id>`, whatever the case of the prefix, or the text itself.
pub fn id_in_text(target: &str) -> &str {
    let Some(prefix) = target.get(..APPS_FOLDER_PREFIX.len()) else {
        return target;
    };
    match prefix.eq_ignore_ascii_case(APPS_FOLDER_PREFIX) {
        true => &target[APPS_FOLDER_PREFIX.len()..],
        false => target,
    }
}

pub fn enrich(target: &str, identity: &mut AppIdentity) {
    let Some(store) = property_store(target) else {
        return;
    };
    if let Some(path) = read_text(&store, &PKEY_Link_TargetParsingPath) {
        identity.add_path(path);
    }
    if let Some(id) = read_text(&store, &PKEY_AppUserModel_ID) {
        identity.add_id(id);
    }
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

    #[test]
    fn the_apps_folder_prefix_is_matched_whatever_its_case() {
        assert_eq!(id_in_text(r"SHELL:appsfolder\Thing"), "Thing");
        assert_eq!(id_in_text(r"C:\Thing.exe"), r"C:\Thing.exe");
        assert_eq!(id_in_text("short"), "short");
    }
}
