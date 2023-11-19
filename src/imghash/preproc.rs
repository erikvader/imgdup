use clap::Args;
use image::RgbImage;

use crate::utils::imgutils::{self, RemoveBordersCli};
use crate::{
    imghash::{
        hamming::{Distance, Hamming},
        imghash,
    },
    utils::imgutils::RemoveBordersArgs,
};

pub const DEFAULT_SIMILARITY_THRESHOLD: Distance = 23;

#[derive(Args, Debug)]
pub struct PreprocCli {
    #[command(flatten)]
    border_args: RemoveBordersCli,
}

impl PreprocCli {
    pub fn to_args(&self) -> PreprocArgs {
        PreprocArgs::default().remove_borders_args(self.border_args.to_args())
    }
}

// NOTE: this is atm just the same as `RemoveBordersArgs`, but having its own args struct
// will make it easier to add additional values in the future, which feels likely.
// Otherwise a `fn hash_img(&RemoveBordersArgs, &RgbImage)` would suffice.
pub struct PreprocArgs {
    border_args: RemoveBordersArgs,
}

impl Default for PreprocArgs {
    fn default() -> Self {
        Self {
            border_args: RemoveBordersArgs::default(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PreprocError {
    #[error("image became empty")]
    Empty,
}

impl PreprocArgs {
    pub fn remove_borders_args(mut self, borders_args: RemoveBordersArgs) -> Self {
        self.border_args = borders_args;
        self
    }

    /// Preprocesses the image and hash it
    pub fn hash_img(&self, img: &RgbImage) -> Result<Hamming, PreprocError> {
        let no_borders = self.border_args.remove_borders(img);

        if imgutils::is_subimg_empty(&no_borders) {
            return Err(PreprocError::Empty);
        }

        Ok(imghash::hash_sub(&no_borders))
    }

    /// Preprocess the image
    // NOTE: this returns a new image to allow for preprocessing other than cropping
    pub fn preprocess(&self, img: &RgbImage) -> RgbImage {
        self.border_args.remove_borders(img).to_image()
    }
}
