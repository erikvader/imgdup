use std::path::Path;

use color_eyre::eyre::{self, Context};

use crate::imghash::{
    hamming::Hamming,
    preproc::{PreprocArgs, PreprocError},
    similarity::SimiArgs,
};
use crate::utils::fsutils::all_files;
use crate::utils::imgutils;

pub struct Ignored {
    hashes: Vec<Hamming>,
}

impl Ignored {
    pub fn empty() -> Self {
        Self { hashes: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.hashes.len()
    }

    pub fn is_ignored(&self, simi: &SimiArgs, test_subject: Hamming) -> bool {
        self.hashes
            .iter()
            .any(|ign| simi.are_similar(*ign, test_subject))
    }

    pub fn iter(&self) -> impl Iterator<Item = Hamming> + '_ {
        self.hashes.iter().copied()
    }
}

pub fn read_ignored(
    dir: impl AsRef<Path>,
    preproc: &PreprocArgs,
    simi: &SimiArgs,
) -> eyre::Result<Ignored> {
    let all_files: Vec<_> = all_files([dir]).wrap_err("failed to read dir")?;
    let mut hashes = Vec::with_capacity(all_files.len());
    let mut hashes_path = Vec::with_capacity(all_files.len());
    let mut hashes_mirrored: Vec<bool> = Vec::with_capacity(all_files.len());

    for file in all_files.iter() {
        let mut img = image::open(&file)
            .wrap_err_with(|| format!("could not open {} as an image", file.display()))?
            .to_rgb8();

        for mirrored in [false, true] {
            if mirrored {
                img = imgutils::mirror(img);
            }

            let hash = match preproc.hash_img(&img) {
                Ok(hash) => hash,
                Err(PreprocError::Empty) => {
                    log::error!(
                    "The ignored file '{}' is empty after border removal (mirror={mirrored})",
                    file.display()
                );
                    continue;
                }
            };

            let the_same: Vec<_> = hashes
                .iter()
                .enumerate()
                .filter(|(_, ignore)| simi.are_similar(hash, **ignore))
                .filter(|(i, _)| !hashes_mirrored[*i])
                .map(|(i, _)| &hashes_path[i])
                .filter(|coll_path| coll_path != &&file)
                .collect();

            if !the_same.is_empty() {
                log::warn!(
                    "The ignored file '{}' (mirrored={mirrored}) is the same as: {:?}",
                    file.display(),
                    the_same,
                );
                continue;
            }

            hashes.push(hash);
            hashes_path.push(file);
            hashes_mirrored.push(mirrored);
        }
    }

    Ok(Ignored { hashes })
}
