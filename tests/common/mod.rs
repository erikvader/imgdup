// NOTE: every test will complain about the functions it doesn't use
#![allow(unused)]

use std::path::PathBuf;

use tempfile::{NamedTempFile, TempPath};

/// Returns a named temporary file inside cargo's tmpdir
pub fn tmp_file() -> TempPath {
    let dir = cargo_tmpdir();
    NamedTempFile::new_in(dir)
        .expect("could not create temporary file")
        .into_temp_path()
}

/// Returns cargo's tmpdir
pub fn cargo_tmpdir() -> PathBuf {
    PathBuf::from(option_env!("CARGO_TARGET_TMPDIR").expect("no cargo tmpdir???"))
}

/// Removes the first instance of `remove`
pub fn remove_first(vec: &mut Vec<i32>, remove: i32) {
    if let Some(i) = vec.iter().position(|i| *i == remove) {
        vec.remove(i);
    }
}
