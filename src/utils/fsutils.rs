use std::{
    fs, io, iter,
    path::{Component, Path, PathBuf},
};

use super::simple_path::SimplePath;

/// Checks whether the path is simply a filename, i.e., a normal part of a path.
pub fn is_basename(path: impl AsRef<Path>) -> bool {
    // TODO: handle trailing slashes
    // https://github.com/proot-me/proot-rs/issues/41
    // https://github.com/rust-lang/rust/issues/29008
    let mut components = path.as_ref().components();
    let Some(Component::Normal(_)) = components.next() else {
        return false;
    };
    components.next().is_none()
}

/// Removes all .. at the beginning of the path
pub fn remove_dot_dot(path: impl AsRef<Path>) -> PathBuf {
    path.as_ref()
        .components()
        .skip_while(|comp| matches!(comp, Component::ParentDir))
        .collect()
}

/// Create a symlink with a relative path to `target` at `link_name`.
/// Both arguments must be relative to the current working directory.
/// `link_name` must be a full path to a file to be created, not a directory to create the
/// file in.
pub fn symlink_relative(
    target: impl AsRef<Path>,
    link_name: &SimplePath, // TODO: accept a borrowed variant instead
) -> io::Result<()> {
    let target = target.as_ref();
    assert!(
        target.is_relative(),
        "must be relative: {}",
        target.display()
    );

    let depth = link_name.depth();
    assert!(depth >= 1);
    let target: PathBuf = iter::repeat(Component::ParentDir)
        .take(depth - 1)
        .chain(target.components())
        .collect();

    std::os::unix::fs::symlink(target, link_name)
}

/// Behave more like `ln`. The symlink will always be absolute, relative `target` will be
/// resolved against the current working directory. If the link_name is a directory, then
/// create the link inside that directory, using the same name as `target`.
pub fn symlink(target: impl AsRef<Path>, link_name: impl AsRef<Path>) -> io::Result<()> {
    let target = match target.as_ref() {
        p if p.is_relative() => std::env::current_dir()?.join(p),
        p => p.to_path_buf(),
    };

    let link_name = match link_name.as_ref() {
        f if f.is_dir() => f.join(target.file_name().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "target path does not refer to anything",
            )
        })?),
        f => f.to_path_buf(),
    };

    std::os::unix::fs::symlink(target, link_name)
}

/// Clears the directory at path, or creates it
pub fn clear_dir(dir: impl AsRef<Path>) -> io::Result<()> {
    let dir = dir.as_ref();
    match fs::symlink_metadata(dir) {
        Ok(meta) if meta.is_dir() => {
            fs::remove_dir_all(dir)?;
            fs::create_dir(dir)
        }
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "dir is not a dir",
        )),
        Err(e) if e.kind() == io::ErrorKind::NotFound => fs::create_dir(dir),
        Err(e) => Err(e),
    }
}

/// Escape a filename Emacs style
pub fn path_as_filename(p: impl AsRef<Path>) -> String {
    p.as_ref().to_string_lossy().to_owned().replace('/', "!")
}

/// Collects all files in the given directories, does not walk them recursively.
// TODO: Probably use https://github.com/BurntSushi/walkdir to get better errors, or eyre
pub fn all_files<R>(folders: impl IntoIterator<Item = impl AsRef<Path>>) -> io::Result<R>
where
    R: FromIterator<PathBuf>,
{
    // TODO: try_fold, or something, to avoid creating a vec
    let iters: Result<Vec<_>, _> =
        folders.into_iter().map(|path| fs::read_dir(path)).collect();

    iters?
        .into_iter()
        .flat_map(|x| x)
        .map(|entry| entry.map(|entry| entry.path()))
        .collect()
}

/// Try to read the file, return None if it doesn't exist
pub fn read_optional_file(path: impl AsRef<Path>) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
        Ok(s) => Ok(Some(s)),
    }
}

/// Return true if the path is a directory that is empty
pub fn is_dir_empty(path: impl AsRef<Path>) -> io::Result<bool> {
    let path = path.as_ref();
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => Ok(fs::read_dir(path)?.next().is_none()),
        Ok(_) => Ok(false),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}
