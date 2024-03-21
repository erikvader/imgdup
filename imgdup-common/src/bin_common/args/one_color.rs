use image::{imageops::grayscale, GenericImageView, GrayImage, Pixel, RgbImage};

use crate::utils::{
    imgutils::{most_common_gray, percent_gray},
    percent::Percent64,
};

use super::args_helper::args;

args! {
    #[derive(Copy, Clone)]
    OneColor {
        "Images that are at least this many percent of the same color (in grayscale) are \
         filtered out."
        one_color_threshold: Percent64 = 90.0.try_into().unwrap();

        "Tolerance when determining if two colors (in grayscale) are the same."
        one_color_tolerance: u8 = 20;
    }
}

impl OneColor {
    pub fn one_color(self, img: &RgbImage) -> Percent64 {
        self.one_color_gray(&grayscale(img))
    }

    pub fn one_color_gray(self, img: &GrayImage) -> Percent64 {
        let most_common = most_common_gray(img);
        percent_gray(img, most_common, self.one_color_tolerance)
    }

    pub fn is_value_too_one_color(self, one_color: Percent64) -> bool {
        one_color >= self.one_color_threshold
    }

    pub fn is_too_one_color<I, P>(self, img: &I) -> bool
    where
        I: GenericImageView<Pixel = P>,
        P: Pixel<Subpixel = u8>,
    {
        self.is_too_one_color_gray(&grayscale(img))
    }

    pub fn is_too_one_color_gray(self, img: &GrayImage) -> bool {
        self.is_value_too_one_color(self.one_color_gray(img))
    }
}
