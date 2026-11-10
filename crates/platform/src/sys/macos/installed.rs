//! The apps in the Applications folders, offered by name.

use std::path::{Path, PathBuf};

use kile_wheel_core::item::AppCandidate;

const APP_SUFFIX: &str = "app";
const SYSTEM_FOLDERS: [&str; 2] = ["/Applications", "/System/Applications"];

pub fn app_candidates() -> Vec<AppCandidate> {
    let mut folders: Vec<PathBuf> = SYSTEM_FOLDERS.iter().map(PathBuf::from).collect();
    if let Some(home) = dirs::home_dir() {
        folders.push(home.join("Applications"));
    }

    let mut candidates: Vec<AppCandidate> = folders
        .iter()
        .filter_map(|folder| std::fs::read_dir(folder).ok())
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter_map(|entry| candidate_of(&entry.path()))
        .collect();
    candidates.sort_by(|left, right| left.name.cmp(&right.name));
    candidates.dedup_by(|left, right| left.name == right.name);
    candidates
}

fn candidate_of(path: &Path) -> Option<AppCandidate> {
    if path.extension()?.to_str()? != APP_SUFFIX {
        return None;
    }
    Some(AppCandidate {
        name: path.file_stem()?.to_str()?.to_owned(),
        target: path.to_str()?.to_owned(),
    })
}
