use image::RgbImage;

use crate::{
    bin_common::args::{
        black_mask::{BlackMaskArgs, BlackMaskCli},
        blandness::{BlandnessArgs, BlandnessCli},
        remove_borders::{RemoveBordersArgs, RemoveBordersCli},
    },
    imghash::{hamming::Hamming, imghash},
    utils::imgutils,
};

use super::args_helper::args;

args! {
    Preproc {
        border_args: RemoveBorders;
        black_mask_args: BlackMask;
        bland_args: Blandness;
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
    /// Preprocesses the image and hashes it, unless it is deemed a bad picture
    pub fn hash_img(&self, img: &RgbImage) -> Result<Hamming, PreprocError> {
        if self.bland_args.is_bland(img) {
            return Err(PreprocError::TooBland);
        }

        let mask = self.border_args.maskify(img);
        if self.black_mask_args.is_too_black(&mask) {
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
