use std::{
    ffi::OsString,
    fs::{self, File},
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use color_eyre::eyre::{self, Context}; // TODO: use custom error type instead
use image::{ImageBuffer, ImageOutputFormat};

use crate::utils::simple_path::SimplePath;

use super::fsutils;

const ENTRY_PADDING: usize = 4;

pub struct Repo {
    path: PathBuf,
    next_entry: u32,
}

pub struct Entry {
    path: PathBuf,
    next_entry: u32,
}

pub struct LazyEntry {
    inner: Option<Entry>,
}

impl Repo {
    pub fn new(path: impl Into<PathBuf>) -> eyre::Result<Self> {
        let path = path.into();
        let next_entry = find_next_entry(&path, |s| s.parse().wrap_err("not a number"))
            .wrap_err("failed to get the next entry")?;
        Ok(Self { path, next_entry })
    }

    pub fn new_entry(&mut self) -> eyre::Result<Entry> {
        let p = ENTRY_PADDING;
        let path = self.path.join(format!("{:0p$}", self.next_entry));
        fs::create_dir(&path).wrap_err("could not create the dir")?;
        self.next_entry += 1;
        Entry::open(path).wrap_err("failed to open dir as an entry")
    }

    pub fn entries(&self) -> eyre::Result<Vec<Entry>> {
        let p = ENTRY_PADDING;
        let mut entries = Vec::new();
        for num in 0..self.next_entry {
            let path = self.path.join(format!("{:0p$}", num));
            if path.is_dir() {
                entries.push(Entry::open(&path).wrap_err_with(|| {
                    format!("failed to open the entry at: {}", path.display())
                })?);
            }
        }
        Ok(entries)
    }
}

impl Entry {
    pub fn open(dir: impl Into<PathBuf>) -> eyre::Result<Self> {
        let dir = dir.into();
        let next_entry = find_next_entry(&dir, |s| {
            if s.len() < ENTRY_PADDING {
                eyre::bail!("path name is too short");
            }
            let num: u32 = s
                .get(..ENTRY_PADDING)
                .ok_or_else(|| {
                    eyre::eyre!("the first few characters don't seem to be numbers")
                })?
                .parse()
                .wrap_err("the parse failed")?;
            Ok(num)
        })
        .wrap_err("failed to get the next entry")?;

        Ok(Self {
            path: dir,
            next_entry,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn next_path(&mut self, name: &Path) -> PathBuf {
        let p = ENTRY_PADDING;
        let mut num: OsString = format!("{:0p$}", self.next_entry).into();
        num.push("_");
        num.push(name);
        self.next_entry += 1;
        self.path.join(num)
    }

    pub fn sub_entry(&mut self, name: impl AsRef<Path>) -> eyre::Result<Self> {
        let name = name.as_ref();
        assert!(fsutils::is_basename(name));
        let sub_path = self.next_path(name);
        fs::create_dir(&sub_path).wrap_err("could not create the dir")?;
        Ok(Self {
            path: sub_path,
            next_entry: 0,
        })
    }

    pub fn create_file<F, E>(
        &mut self,
        name: impl AsRef<Path>,
        writer: F,
    ) -> eyre::Result<()>
    where
        F: FnOnce(&mut BufWriter<File>) -> std::result::Result<(), E>,
        std::result::Result<(), E>: eyre::WrapErr<(), E>,
    {
        let name = name.as_ref();
        // TODO: should probably be an eyre::ensure?
        assert!(fsutils::is_basename(name));
        let file_path = self.next_path(name);
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(file_path)
            .wrap_err("could not create file")?;

        let mut buf = BufWriter::new(file);
        writer(&mut buf).wrap_err("the writer failed")?;
        buf.flush().wrap_err("failed to flush")?;
        Ok(())
    }

    /// Open some file with name `name` and apply the fallible function on it which
    /// should interpret the data to something
    pub fn read_file<R, T, E>(&self, name: impl AsRef<Path>, reader: R) -> eyre::Result<T>
    where
        R: FnOnce(&mut BufReader<File>) -> std::result::Result<T, E>,
        std::result::Result<T, E>: eyre::WrapErr<T, E>,
    {
        let name = name.as_ref();
        assert!(fsutils::is_basename(name));

        // TODO: extract function
        let target_file = {
            // TODO: I don't like that this must be UTF-8, but its not possible, or at least
            // really annoying, to do string operations on `Path` :( Probably use
            // https://doc.rust-lang.org/std/os/unix/ffi/trait.OsStrExt.html#tymethod.as_bytes
            // and do substring searches and stuff on byte slices.
            let name = name.to_str().expect("should be UTF-8");

            let all_files: Vec<_> =
                fsutils::all_files([&self.path]).wrap_err("failed to list myself")?;

            let mut target_file = None;
            for file in all_files {
                let filename = file
                    .file_name()
                    .expect("will contain a filename")
                    .to_str()
                    .ok_or_else(|| eyre::eyre!("path name is not UTF-8: {:?}", file))?;

                if filename.ends_with(name)
                    && filename.len() >= ENTRY_PADDING + 1
                    && filename[..ENTRY_PADDING]
                        .chars()
                        .all(|c| c.is_ascii_digit())
                    && &filename[ENTRY_PADDING..ENTRY_PADDING + 1] == "_"
                {
                    target_file = Some(file);
                    break;
                }
            }
            target_file
        };

        let Some(target_file) = target_file else {
            eyre::bail!(
                "Could not find a file with name {name:?} in entry {:?}",
                self.path
            )
        };

        let mut buf = BufReader::new(
            File::open(target_file)
                .wrap_err_with(|| "failed to open {target_file:?} for reading")?,
        );
        let t = reader(&mut buf)
            .wrap_err_with(|| "failed to read the contents of {target_file:?}")?;
        Ok(t)
    }

    /// `target` is relative CWD, or absolute
    pub fn create_link(
        &mut self,
        link_name: impl AsRef<Path>,
        target: impl AsRef<Path>,
    ) -> eyre::Result<()> {
        let link_name = link_name.as_ref();
        assert!(fsutils::is_basename(link_name));
        let link_name = self.next_path(link_name);
        fsutils::symlink(target, link_name).wrap_err("failed to create link")?;
        Ok(())
    }

    /// `target` and `link` are relative CWD
    pub fn create_link_relative(
        &mut self,
        // TODO: create a `BasenamePath` or something that is a path that must only be a
        // basename, i.e., `fsutils::is_basename`
        link_name: impl AsRef<Path>,
        target: impl AsRef<SimplePath>,
    ) -> eyre::Result<()> {
        let link_name = link_name.as_ref();
        assert!(fsutils::is_basename(link_name));
        let link_name = self.next_path(link_name);
        let link_name =
            SimplePath::new(&link_name).wrap_err("the new link name is not simple")?;
        fsutils::symlink_relative(target, link_name).wrap_err("failed to create link")?;
        Ok(())
    }

    pub fn create_jpg<P, C>(
        &mut self,
        jpg_name: impl AsRef<Path>,
        image: &ImageBuffer<P, C>,
    ) -> eyre::Result<()>
    where
        P: image::Pixel + image::PixelWithColorType,
        [P::Subpixel]: image::EncodableLayout,
        C: std::ops::Deref<Target = [P::Subpixel]>,
    {
        let jpg_name = jpg_name.as_ref();
        assert!(fsutils::is_basename(jpg_name));
        let jpg_name = Path::new(jpg_name).with_extension("jpg");
        self.create_file(jpg_name, |w| {
            image
                .write_to(w, ImageOutputFormat::Jpeg(95))
                .wrap_err("image failed to write")
        })
    }

    pub fn create_text_file(
        &mut self,
        txt_name: impl AsRef<Path>,
        contents: impl AsRef<str>,
    ) -> eyre::Result<()> {
        let txt_name = txt_name.as_ref();
        assert!(fsutils::is_basename(txt_name));
        let txt_name = Path::new(txt_name).with_extension("txt");
        self.create_file(txt_name, |w| {
            w.write_all(contents.as_ref().as_bytes())
                .wrap_err("failed to write string")
        })
    }
}

impl LazyEntry {
    pub fn new() -> Self {
        Self { inner: None }
    }

    // NOTE: fallible version on `OnceCell` is not stable yet, so use a custom
    // implementation https://github.com/rust-lang/rust/issues/109737
    pub fn get_or_try_init<F, E>(&mut self, init: F) -> std::result::Result<&mut Entry, E>
    where
        F: FnOnce() -> std::result::Result<Entry, E>,
    {
        if self.inner.is_none() {
            self.inner = Some(init()?);
        }
        Ok(self.inner.as_mut().unwrap())
    }

    pub fn get_or_init(&mut self, repo: &mut Repo) -> eyre::Result<&mut Entry> {
        self.get_or_try_init(|| repo.new_entry())
    }
}

fn find_next_entry<F>(dir: impl AsRef<Path>, num_extract: F) -> eyre::Result<u32>
where
    F: Fn(&str) -> eyre::Result<u32>,
{
    let all_files: Vec<_> =
        fsutils::all_files([dir]).wrap_err("failed to list the dir")?;
    let next_entry = all_files
        .into_iter()
        .try_fold(None, |maximum, path| -> eyre::Result<Option<u32>> {
            let path = path
                .file_name()
                .expect("will contain a filename")
                .to_str()
                .ok_or_else(|| eyre::eyre!("path name is not UTF-8: {:?}", path))?;
            let num: u32 =
                num_extract(path).wrap_err("failed to parse the path to a number")?;
            Ok(maximum.map(|m| std::cmp::max(m, num)).or(Some(num)))
        })?
        .map(|max| max + 1)
        .unwrap_or(0);
    Ok(next_entry)
}
