//! The apps pinned to the GNOME dash.

use kile_wheel_core::item::AppCandidate;
use tracing::debug;

use super::desktop;

pub fn app_candidates() -> Vec<AppCandidate> {
    let output = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.shell", "favorite-apps"])
        .output();
    let text = match output {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout).into_owned(),
        Ok(output) => {
            debug!("no dash favourites to offer: gsettings exited with {}", output.status);
            return Vec::new();
        }
        Err(error) => {
            debug!("no dash favourites to offer: {error}");
            return Vec::new();
        }
    };

    let mut candidates: Vec<AppCandidate> = favourite_ids(&text)
        .into_iter()
        .map(|id| AppCandidate {
            name: desktop::find(&id)
                .and_then(|entry| entry.name)
                .unwrap_or_else(|| id.trim_end_matches(".desktop").to_owned()),
            target: id,
        })
        .collect();
    candidates.sort_by(|left, right| left.name.cmp(&right.name));
    candidates
}

/// Reads gsettings' GVariant text, `['a.desktop', 'b.desktop']`.
fn favourite_ids(text: &str) -> Vec<String> {
    text.split('\'')
        .skip(1)
        .step_by(2)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gvariant_list_is_read_in_order() {
        assert_eq!(
            favourite_ids("['firefox_firefox.desktop', 'org.gnome.Nautilus.desktop']\n"),
            ["firefox_firefox.desktop", "org.gnome.Nautilus.desktop"]
        );
        assert!(favourite_ids("@as []\n").is_empty());
    }
}
