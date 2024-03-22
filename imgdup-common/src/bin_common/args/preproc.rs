use image::{imageops::grayscale, RgbImage, SubImage};

use crate::{
    imghash::{hamming::Hamming, imghash},
    utils::{imgutils, percent::Percent64},
};

use super::{args_helper::args, one_color::OneColor, remove_borders::RemoveBorders};

args! {
    Preproc {
        border_args: RemoveBorders;
        one_color_args: OneColor;

        "If only this many percent of the image is left after preprocessing, then \
         its considered empty."
        emptiness_threshold: Percent64 = 10.0.try_into().unwrap();
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PreprocError {
    #[error("image became empty")]
    Empty,
    #[error("the image consists of too many pixels of the same color")]
    TooOneColor,
}

impl Preproc {
    /// Preprocesses the image and hashes it, unless it is deemed a bad picture
    pub fn hash_img(&self, img: &RgbImage) -> Result<Hamming, PreprocError> {
        let processed = self.check(img)?;
        Ok(imghash::hash_sub(&processed))
    }

    /// Figure out if the image is good and then return it preprocessed
    pub fn check<'a>(
        &self,
        img: &'a RgbImage,
    ) -> Result<SubImage<&'a RgbImage>, PreprocError> {
        // TODO: This whole function is complex and difficult to follow, just to avoid
        // redoing calculations as much as possible. This could maybe be remade in a way
        // that caches results in the background somwhow?
        let gray = grayscale(img);
        let one_color = self.one_color_args.one_color_gray(&gray);
        if self.one_color_args.is_value_too_one_color(one_color) {
            return Err(PreprocError::TooOneColor);
        }

        let mask = self.border_args.maskify(gray);
        let no_borders = self.border_args.remove_borders_mask(img, &mask);
        if imgutils::subimg_coverage(&no_borders) <= self.emptiness_threshold {
            return Err(PreprocError::Empty);
        }

        // TODO: figure out how to avoid converting the image to grayscale twice. One
        // solution could be to transfer the bounds from `no_borders` to the gray image,
        // but it doesn't seem possible to extract the x and y position of the SubImage
        // without deprecated functions... image-0.24.8
        if self.one_color_args.is_too_one_color(&*no_borders) {
            return Err(PreprocError::TooOneColor);
        }

        Ok(no_borders)
    }

    /// Preprocess the image
    pub fn preprocess<'a>(&self, img: &'a RgbImage) -> SubImage<&'a RgbImage> {
        self.border_args.remove_borders(img)
    }
}
