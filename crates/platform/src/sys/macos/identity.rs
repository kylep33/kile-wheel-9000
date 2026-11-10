//! An .app bundle is known by its bundle id, which is what running apps are grouped by.

use objc2_foundation::{NSBundle, NSString};

use crate::identity::AppIdentity;

/// APFS is case-insensitive by default, and bundle ids compare that way too.
pub const CASE_INSENSITIVE: bool = true;

pub fn id_in_text(target: &str) -> &str {
    target
}

pub fn enrich(target: &str, identity: &mut AppIdentity) {
    if !target.ends_with(".app") {
        return;
    }
    let Some(bundle) = NSBundle::bundleWithPath(&NSString::from_str(target)) else {
        return;
    };
    if let Some(id) = bundle.bundleIdentifier() {
        identity.add_id(id.to_string());
    }
}
