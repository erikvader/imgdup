use clap::Args;
use image::RgbImage;

use crate::utils::imgutils::{
    self, BlackMaskArgs, BlackMaskCli, BlandnessArgs, BlandnessCli, RemoveBordersCli,
};
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

    #[command(flatten)]
    black_args: BlackMaskCli,

    #[command(flatten)]
    bland_args: BlandnessCli,
}

impl PreprocCli {
    pub fn to_args(&self) -> PreprocArgs {
        PreprocArgs::default().remove_borders_args(self.border_args.to_args())
    }
}

pub struct PreprocArgs {
    border_args: RemoveBordersArgs,
    black_args: BlackMaskArgs,
    bland_args: BlandnessArgs,
}

impl Default for PreprocArgs {
    fn default() -> Self {
        Self {
            border_args: RemoveBordersArgs::default(),
            black_args: BlackMaskArgs::default(),
            bland_args: BlandnessArgs::default(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PreprocError {
    #[error("image became empty")]
    Empty,
    #[error("the image consists of too many black pixels")]
    TooBlack,
    #[error("the image is too bland")]
    TooBland,
}

impl PreprocArgs {
    pub fn remove_borders_args(mut self, args: RemoveBordersArgs) -> Self {
        self.border_args = args;
        self
    }

    pub fn black_mask_args(mut self, args: BlackMaskArgs) -> Self {
        self.black_args = args;
        self
    }

    pub fn bland_args(mut self, args: BlandnessArgs) -> Self {
        self.bland_args = args;
        self
    }

    /// Preprocesses the image and hashes it, unless it is deemed a bad picture
    pub fn hash_img(&self, img: &RgbImage) -> Result<Hamming, PreprocError> {
        if self.bland_args.is_bland(img) {
            return Err(PreprocError::TooBland);
        }

        let mask = self.border_args.maskify(img);
        if self.black_args.is_too_black(&mask) {
            return Err(PreprocError::TooBlack);
        }

        let no_borders = self.border_args.remove_borders_mask(img, &mask);

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
