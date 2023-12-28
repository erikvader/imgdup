use crate::utils::imgutils::{maskify, watermark_getbbox, Mask};

use super::args_helper::args;
use image::{imageops::crop_imm, GrayImage, RgbImage, SubImage};

pub use image::imageops::colorops::grayscale;

args! {
    #[derive(Copy, Clone)]
    RemoveBorders {
        "All gray values below this becomes black"
        remove_borders_maskify_threshold: u8 = 40;

        "A mask line can contain this many percent of white and still be considered black"
        remove_borders_maximum_whites: f64 = 0.1;
    }
}

impl RemoveBordersArgs {
    pub fn remove_borders<'a>(self, img: &'a RgbImage) -> SubImage<&'a RgbImage> {
        let gray = grayscale(img);
        let mask = self.maskify(gray);
        self.remove_borders_mask(img, &mask)
    }

    pub fn remove_borders_mask<'a>(
        self,
        img: &'a RgbImage,
        mask: &Mask,
    ) -> SubImage<&'a RgbImage> {
        let bbox = watermark_getbbox(&mask, self.remove_borders_maximum_whites);
        crop_imm(img, bbox.x, bbox.y, bbox.width, bbox.height)
    }

    pub fn maskify(self, img: GrayImage) -> Mask {
        maskify(img, self.remove_borders_maskify_threshold)
    }
}
