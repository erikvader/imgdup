use std::{
    ffi::OsString,
    path::{Component, Path, PathBuf},
};

use rkyv::{Archive, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not UTF-8: {0:?}")]
    NotUTF8(OsString),
    #[error("not simple: {0:?}")]
    NotSimple(OsString),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Serialize, Archive, Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
#[archive(check_bytes)]
/// A path that: is relative, is UTF-8 and only contains slashes and filenames
pub struct SimpleRelative {
    inner: String,
}

impl SimpleRelative {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        if !path
            .components()
            .all(|comp| matches!(comp, Component::Normal(_)))
        {
            return Err(Error::NotSimple(path.into()));
        }

        let path = path
            .into_os_string()
            .into_string()
            .map_err(|original| Error::NotUTF8(original))?;

        if path.contains("//")
            || path.contains("/./")
            || path.ends_with("/.")
            || path.ends_with("/")
        {
            return Err(Error::NotSimple(path.into()));
        }

        Ok(Self { inner: path })
    }

    /// How many components long a simple relative path is
    pub fn depth(&self) -> usize {
        let path: &Path = self.inner.as_ref();
        path.components()
            .filter(|comp| !matches!(comp, Component::CurDir))
            .inspect(|comp| {
                if !matches!(comp, Component::Normal(_)) {
                    panic!("the path must be simple")
                }
            })
            .count()
    }

    pub fn as_path(&self) -> &Path {
        self.inner.as_ref()
    }
}

impl AsRef<Path> for SimpleRelative {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl ArchivedSimpleRelative {
    pub fn as_path(&self) -> &Path {
        self.inner.as_str().as_ref()
    }
}

impl AsRef<Path> for ArchivedSimpleRelative {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

pub fn clap_simple_relative_parser(
    s: &str,
) -> std::result::Result<SimpleRelative, String> {
    SimpleRelative::new(s).map_err(|_| {
        format!(
            "path is not simple relative, i.e., is relative and only contains \
                     normal components"
        )
    })
}

#[cfg(test)]
mod test {
    use super::*;

    fn is_simple_relative(path: impl Into<PathBuf>) -> bool {
        SimpleRelative::new(path).is_ok()
    }

    #[test]
    fn simple_paths() {
        assert!(is_simple_relative("a/b"));
        assert!(is_simple_relative("a"));
        assert!(is_simple_relative(".a"));
        assert!(is_simple_relative("a/.b"));
        assert!(is_simple_relative("a/b."));

        assert!(!is_simple_relative("a//b"));
        assert!(!is_simple_relative("/a/b"));
        assert!(!is_simple_relative("./a/b"));
        assert!(!is_simple_relative("a/b/"));
        assert!(!is_simple_relative("a/b/."));
        assert!(!is_simple_relative("a/./b"));
        assert!(!is_simple_relative("a/../b"));
    }
}
