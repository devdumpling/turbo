use std::{fs::canonicalize, path::PathBuf};

use once_cell::sync::Lazy;

/// The turbo repo root. Should be used as the root when building with turbopack
/// against fixtures in this crate.
pub static REPO_ROOT: Lazy<String> = Lazy::new(|| {
    let package_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    canonicalize(package_root)
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
        .to_str()
        .unwrap()
        .to_string()
});
