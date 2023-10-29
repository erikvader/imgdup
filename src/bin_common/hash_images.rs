use std::path::Path;

use clap::Args;
use color_eyre::eyre::{self, Context};

use crate::imghash::{
    hamming::{Distance, Hamming},
    imghash,
};
use crate::utils::fsutils::all_files;
use crate::utils::imgutils::{self, RemoveBordersCli, RemoveBordersConf};

pub const DEFAULT_SIMILARITY_THRESHOLD: Distance = 23;

#[derive(Args, Debug)]
pub struct HashCli {
    #[command(flatten)]
    remove_borders_args: RemoveBordersCli,

    /// Maximum distance for two images to be considered equal
    #[arg(long, default_value_t = DEFAULT_SIMILARITY_THRESHOLD)]
    similarity_threshold: Distance,
}

impl HashCli {
    pub fn as_conf(&self) -> HashConf {
        HashConf::default()
            .similarity_threshold(self.similarity_threshold)
            .border_conf(self.remove_borders_args.as_conf())
    }
}

pub struct HashConf {
    pub border_conf: RemoveBordersConf,
    pub similarity_threshold: Distance,
}

impl Default for HashConf {
    fn default() -> Self {
        Self {
            border_conf: RemoveBordersConf::default(),
            similarity_threshold: DEFAULT_SIMILARITY_THRESHOLD,
        }
    }
}

impl HashConf {
    pub fn similarity_threshold(mut self, threshold: Distance) -> Self {
        self.similarity_threshold = threshold;
        self
    }

    pub fn border_conf(mut self, conf: RemoveBordersConf) -> Self {
        self.border_conf = conf;
        self
    }
}

pub fn read_ignored(
    dir: impl AsRef<Path>,
    conf: &HashConf,
) -> eyre::Result<Vec<Hamming>> {
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

            let img = imgutils::remove_borders(&img, &conf.border_conf);
            if imgutils::is_subimg_empty(&img) {
                log::error!(
                    "The ignored file '{}' is empty after border removal (mirror={mirrored})",
                    file.display()
                );
                continue;
            }

            let hash = imghash::hash_sub(&img);
            let the_same: Vec<_> = hashes
                .iter()
                .enumerate()
                .filter(|(_, ignore)| {
                    hash.distance_to(**ignore) <= conf.similarity_threshold
                })
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

    Ok(hashes)
}
