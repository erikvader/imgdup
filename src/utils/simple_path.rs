use std::{
    borrow::Borrow,
    fmt, iter,
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
        SimplePath::new_str_unchecked(self.inner.as_str())
    }

    /// Undo the result of `SimplePath::resolve_file_to` and `SimplePath::resolve_dir_to`
    /// to get back the original `target` argument.
    pub fn unresolve(resolved_path: impl AsRef<Path>) -> Result<Self> {
        let restored_path: PathBuf = resolved_path
            .as_ref()
            .components()
            .skip_while(|comp| matches!(comp, Component::ParentDir))
            .collect();
        Self::new(restored_path)
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

impl From<SimplePathBuf> for PathBuf {
    fn from(value: SimplePathBuf) -> Self {
        value.inner.into()
    }
}

impl fmt::Display for SimplePathBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_simple_path().fmt(f)
    }
}

impl ArchivedSimplePathBuf {
    pub fn as_path(&self) -> &Path {
        self.inner.as_str().as_ref()
    }

    pub fn as_simple_path(&self) -> &SimplePath {
        SimplePath::new_str_unchecked(self.inner.as_str())
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
        Self::new_str(path)
    }

    pub fn new_str(path: &str) -> Result<&Self> {
        if !is_simple(&path) {
            return Err(Error::NotSimple);
        }
        Ok(Self::new_str_unchecked(path))
    }

    fn new_str_unchecked(s: &str) -> &Self {
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
        path.components().count()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Return a path that when followed from the directory the file at `self` is in, will
    /// get to `target`. Both `self` and `target` should be relative to the same point.
    /// Self must refer to a file, i.e., it can't be the empty path, `None` is returned in
    /// that case.
    pub fn resolve_file_to(&self, target: impl AsRef<SimplePath>) -> Option<PathBuf> {
        if self.is_empty() {
            return None;
        }

        let res = self.resolve_dir_to(target);
        let mut components = res.components();
        assert_eq!(
            Some(Component::ParentDir),
            components.next(),
            "was expecting to pop a '..'"
        );
        Some(components.collect())
    }

    /// Return a path that when followed from the directory at `self`, will get to
    /// `target`. Both `self` and `target` should be relative to the same point.
    pub fn resolve_dir_to(&self, target: impl AsRef<SimplePath>) -> PathBuf {
        let target = target.as_ref().as_path();
        let depth = self.depth();
        iter::repeat(Component::ParentDir)
            .take(depth)
            .chain(target.components())
            .collect()
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

impl ToOwned for SimplePath {
    type Owned = SimplePathBuf;

    fn to_owned(&self) -> Self::Owned {
        Self::Owned {
            inner: self.inner.to_owned(),
        }
    }
}

impl fmt::Display for SimplePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.inner)
    }
}

impl PartialEq<SimplePathBuf> for &SimplePath {
    fn eq(&self, other: &SimplePathBuf) -> bool {
        *self == other.as_simple_path()
    }
}

impl PartialEq<SimplePathBuf> for SimplePath {
    fn eq(&self, other: &SimplePathBuf) -> bool {
        &self == other
    }
}

impl PartialEq<SimplePath> for SimplePathBuf {
    fn eq(&self, other: &SimplePath) -> bool {
        other == self
    }
}

impl PartialEq<&SimplePath> for SimplePathBuf {
    fn eq(&self, other: &&SimplePath) -> bool {
        other == self
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

    fn is_simple(path: impl AsRef<Path>) -> bool {
        SimplePath::new(path.as_ref()).is_ok()
    }

    fn simple(path: &str) -> &SimplePath {
        SimplePath::new_str(path).unwrap()
    }

    fn resolve_file_to(sself: &str, target: &str) -> Option<PathBuf> {
        simple(sself).resolve_file_to(simple(target))
    }

    fn resolve_dir_to(sself: &str, target: &str) -> PathBuf {
        simple(sself).resolve_dir_to(simple(target))
    }

    #[test]
    fn simple_paths() {
        assert!(is_simple(""));
        assert!(is_simple(" "));
        assert!(is_simple("a/b"));
        assert!(is_simple("a"));
        assert!(is_simple(".a"));
        assert!(is_simple("a/.b"));
        assert!(is_simple("a/b."));

        assert!(!is_simple("."));
        assert!(!is_simple("a//b"));
        assert!(!is_simple("/a/b"));
        assert!(!is_simple("./a/b"));
        assert!(!is_simple("a/b/"));
        assert!(!is_simple("a/b/."));
        assert!(!is_simple("a/./b"));
        assert!(!is_simple("a/../b"));
    }

    #[test]
    fn depths() {
        assert_eq!(0, simple("").depth());
        assert_eq!(1, simple(" ").depth());
        assert_eq!(2, simple(" / ").depth());
        assert_eq!(2, simple("a/b").depth());
        assert_eq!(1, simple("a").depth());
    }

    #[test]
    fn resolve() {
        assert_eq!(
            Some("../hej".into()),
            resolve_file_to("mapp1/fil.txt", "hej")
        );
        assert_eq!(Path::new("../hej"), resolve_dir_to("mapp1", "hej"));

        assert_eq!(None, resolve_file_to("", "hej"));
        assert_eq!(Some("".into()), resolve_file_to("fil.txt", ""));
        assert_eq!(Path::new(".."), resolve_dir_to("mapp", ""));

        assert_eq!(
            Some("../../hej".into()),
            resolve_file_to("mapp1/mapp2/fil.txt", "hej")
        );
    }

    #[test]
    fn unresolve() {
        assert_eq!(
            simple("fil.txt"),
            SimplePathBuf::unresolve("../fil.txt").unwrap()
        );
        assert_eq!(
            simple("fil.txt"),
            SimplePathBuf::unresolve("fil.txt").unwrap()
        );
        assert_eq!(simple(""), SimplePathBuf::unresolve("").unwrap());
    }

    #[test]
    fn equality() {
        let buf = SimplePathBuf::new("hej").unwrap();
        let pat = SimplePath::new_str("hej").unwrap();

        assert_eq!(pat, buf);
        assert_eq!(*pat, buf);
        assert_eq!(buf, pat);
        assert_eq!(buf, *pat);
    }
}
