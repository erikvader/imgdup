use std::{
    borrow::Borrow,
    ops::Deref,
    path::{Component, Path, PathBuf},
};

use rkyv::{Archive, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not UTF-8")]
    NotUTF8,
    #[error("not simple")]
    NotSimple,
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Serialize, Archive, Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
#[archive(check_bytes)]
/// A path that: is relative, is UTF-8 and only contains slashes and filenames
pub struct SimplePathBuf {
    inner: String,
}

impl SimplePathBuf {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path
            .into()
            .into_os_string()
            .into_string()
            .map_err(|_| Error::NotUTF8)?;

        if !is_simple(&path) {
            return Err(Error::NotSimple);
        }

        Ok(Self { inner: path })
    }

    pub fn as_path(&self) -> &Path {
        self.inner.as_ref()
    }

    pub fn as_simple_path(&self) -> &SimplePath {
        SimplePath::new_str(self.inner.as_str())
    }
}

impl Deref for SimplePathBuf {
    type Target = SimplePath;

    fn deref(&self) -> &Self::Target {
        self.as_simple_path()
    }
}

impl Borrow<SimplePath> for SimplePathBuf {
    fn borrow(&self) -> &SimplePath {
        self.as_simple_path()
    }
}

impl AsRef<Path> for SimplePathBuf {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl TryFrom<PathBuf> for SimplePathBuf {
    type Error = Error;

    fn try_from(value: PathBuf) -> std::result::Result<Self, Self::Error> {
        SimplePathBuf::new(value)
    }
}

impl ArchivedSimplePathBuf {
    pub fn as_path(&self) -> &Path {
        self.inner.as_str().as_ref()
    }

    pub fn as_simple_path(&self) -> &SimplePath {
        SimplePath::new_str(self.inner.as_str())
    }
}

impl AsRef<Path> for ArchivedSimplePathBuf {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl AsRef<SimplePath> for ArchivedSimplePathBuf {
    fn as_ref(&self) -> &SimplePath {
        self.as_simple_path()
    }
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct SimplePath {
    inner: str,
}

impl SimplePath {
    pub fn new(path: &Path) -> Result<&Self> {
        let path = path.as_os_str().to_str().ok_or(Error::NotUTF8)?;
        if !is_simple(&path) {
            return Err(Error::NotSimple);
        }

        Ok(Self::new_str(path))
    }

    fn new_str(s: &str) -> &Self {
        // SAFETY: ok because the struct is repr(transparent) around an str. This is how
        // Path does it.
        unsafe { &*(s as *const str as *const Self) }
    }

    pub fn as_path(&self) -> &Path {
        self.inner.as_ref()
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
}

impl AsRef<Path> for SimplePath {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl AsRef<SimplePath> for SimplePath {
    fn as_ref(&self) -> &SimplePath {
        self
    }
}

impl<'a> TryFrom<&'a Path> for &'a SimplePath {
    type Error = Error;

    fn try_from(value: &'a Path) -> std::result::Result<Self, Self::Error> {
        SimplePath::new(value)
    }
}

// TODO: is there a trait to impl to make this supported automatically with clap?
pub fn clap_simple_relative_parser(
    s: &str,
) -> std::result::Result<SimplePathBuf, String> {
    SimplePathBuf::new(s).map_err(|_| {
        format!(
            "path is not simple relative, i.e., is relative and only contains \
                     normal components"
        )
    })
}

fn is_simple(s: &str) -> bool {
    let path: &Path = s.as_ref();
    path.components()
        .all(|comp| matches!(comp, Component::Normal(_)))
        && !s.contains("//")
        && !s.contains("/./")
        && !s.ends_with("/.")
        && !s.ends_with("/")
}

#[cfg(test)]
mod test {
    use super::*;

    fn is_simple_relative(path: impl AsRef<Path>) -> bool {
        SimplePath::new(path.as_ref()).is_ok()
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
