use std::{
    ffi::OsString,
    fs::{self, File},
    io::{BufWriter, Write},
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

    pub fn create_file<F>(
        &mut self,
        name: impl AsRef<Path>,
        writer: F,
    ) -> eyre::Result<()>
    where
        F: FnOnce(&mut BufWriter<File>) -> eyre::Result<()>,
    {
        let name = name.as_ref();
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

    /// `target` is relative CWD
    pub fn create_link_relative(
        &mut self,
        link_name: impl AsRef<Path>,
        target: impl AsRef<Path>,
    ) -> eyre::Result<()> {
        let link_name = link_name.as_ref();
        assert!(fsutils::is_basename(link_name));
        let link_name = self.next_path(link_name);
        // TODO: A borrowed SimplePath would be nice here. `link_name` is copied as it
        // works right now.
        let link_name = SimplePath::new(link_name).wrap_err("link name is not simple")?;
        fsutils::symlink_relative(target, &link_name)
            .wrap_err("failed to create link")?;
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

    pub fn get_or_init<F>(&mut self, init: F) -> eyre::Result<&mut Entry>
    where
        F: FnOnce() -> eyre::Result<Entry>,
    {
        if self.inner.is_none() {
            self.inner = Some(init()?);
        }
        Ok(self.inner.as_mut().unwrap())
    }

    pub fn get_or_init2(&mut self, repo: &mut Repo) -> eyre::Result<&mut Entry> {
        self.get_or_init(|| repo.new_entry())
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
