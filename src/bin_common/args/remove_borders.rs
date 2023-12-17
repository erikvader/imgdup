use crate::utils::imgutils::{maskify, watermark_getbbox};

use super::args_helper::args;
use image::{imageops::crop_imm, GenericImageView, GrayImage, RgbImage, SubImage};

pub use image::imageops::colorops::grayscale;

args! {
    #[derive(Copy, Clone)]
    RemoveBorders {
        // TODO: extract this to its own args!
        "All gray values below this becomes black"
        maskify_threshold: u8 = 40;

        "A mask line can contain this many percent of white and still be considered black"
        maximum_whites: f64 = 0.1;
    }
}

impl RemoveBordersArgs {
    pub fn remove_borders<'a>(self, img: &'a RgbImage) -> SubImage<&'a RgbImage> {
        let mask = self.maskify(img);
        self.remove_borders_mask(img, &mask)
    }

    pub fn remove_borders_mask<'a>(
        self,
        img: &'a RgbImage,
        mask: &GrayImage,
    ) -> SubImage<&'a RgbImage> {
        let bbox = watermark_getbbox(&mask, self.maximum_whites);
        crop_imm(img, bbox.x, bbox.y, bbox.width, bbox.height)
    }

    pub fn maskify(self, img: &RgbImage) -> GrayImage {
        maskify(img, self.maskify_threshold)
    }
}
