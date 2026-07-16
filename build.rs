//! Build-time versioning.
//!
//! Produces `TANU_VERSION` = `v<major>.<minor>.<build>` where major/minor come
//! from Cargo.toml and `<build>` is a counter bumped on every build. The count
//! is persisted in `.build_number` (gitignored) so it grows monotonically, e.g.
//! `v1.12.193`.

use std::fs;
use std::path::Path;

fn main() {
    // Force this script to rerun on every build: a rerun-if-changed path that
    // never exists is always considered "changed" by cargo.
    println!("cargo:rerun-if-changed=.tanu-build-always-rerun");

    let major = std::env::var("CARGO_PKG_VERSION_MAJOR").unwrap_or_else(|_| "0".into());
    let minor = std::env::var("CARGO_PKG_VERSION_MINOR").unwrap_or_else(|_| "0".into());

    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    let counter = Path::new(&manifest).join(".build_number");

    let build = fs::read_to_string(&counter)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0)
        + 1;
    let _ = fs::write(&counter, build.to_string());

    println!("cargo:rustc-env=TANU_VERSION=v{major}.{minor}.{build}");
    println!("cargo:rustc-env=TANU_BUILD={build}");
}
