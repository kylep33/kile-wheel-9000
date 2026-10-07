//! Embeds the win32 manifest.
//!
//! Elevation and DPI awareness both have to be settled before the process starts, so
//! they live in a manifest rather than in a call at startup.

use std::path::Path;

const MANIFEST_FILE: &str = "kile-wheel-9000.manifest";

fn main() {
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let manifest = Path::new(&crate_dir).join(MANIFEST_FILE);

    println!("cargo::rerun-if-changed={}", manifest.display());
    println!("cargo::rustc-link-arg-bins=/MANIFEST:EMBED");
    println!(
        "cargo::rustc-link-arg-bins=/MANIFESTINPUT:{}",
        manifest.display()
    );

    // the manifest already states the execution level, so the linker must not add one
    println!("cargo::rustc-link-arg-bins=/MANIFESTUAC:NO");
}
