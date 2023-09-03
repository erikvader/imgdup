use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};

use color_eyre::eyre::{self, Context}; // TODO: use custom error type instead

pub struct Repo {
    path: PathBuf,
    next_entry: u32,
}

pub struct Entry {
    path: PathBuf,
}

impl Repo {
    pub fn new(path: impl Into<PathBuf>) -> eyre::Result<Self> {
        let path = path.into();

        let all_files: Vec<_> =
            crate::fsutils::all_files([&path]).wrap_err("failed to list the dir")?;
        let next_entry = all_files
            .into_iter()
            .try_fold(None, |maximum, path| -> eyre::Result<Option<u32>> {
                let path = path
                    .file_name()
                    .expect("will contain a filename")
                    .to_str()
                    .ok_or_else(|| eyre::eyre!("path name is not UTF-8: {:?}", path))?;
                let num: u32 = path.parse().wrap_err("not a number")?;
                Ok(maximum.map(|m| std::cmp::max(m, num)).or(Some(num)))
            })?
            .map(|max| max + 1)
            .unwrap_or(0);

        Ok(Self { path, next_entry })
    }

    pub fn new_entry(&mut self) -> eyre::Result<Entry> {
        let path = self.path.join(self.next_entry.to_string());
        fs::create_dir(&path).wrap_err("could not create the dir")?;
        self.next_entry += 1;
        Ok(Entry::open(path))
    }
}

impl Entry {
    pub fn open(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        assert!(dir.is_dir());
        Self { path: dir }
    }

    pub fn sub_entry(&self, name: impl AsRef<Path>) -> eyre::Result<Self> {
        let sub_path = self.path.join(name);
        fs::create_dir(&sub_path).wrap_err("could not create the dir")?;
        Ok(Self { path: sub_path })
    }

    pub fn create_file(
        &self,
        name: impl AsRef<Path>,
        contents: &[u8],
    ) -> eyre::Result<()> {
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.path.join(name))
            .wrap_err("could not create file")?;

        let mut buf = io::BufWriter::new(file);
        buf.write_all(contents).wrap_err("failed to write")?;
        buf.flush().wrap_err("failed to flush")?;
        Ok(())
    }

    pub fn create_link(
        &self,
        link_name: impl AsRef<Path>,
        target: impl AsRef<Path>,
    ) -> eyre::Result<()> {
        let link_name = self.path.join(link_name);
        crate::fsutils::symlink(target, link_name).wrap_err("failed to create link")?;
        Ok(())
    }
}
