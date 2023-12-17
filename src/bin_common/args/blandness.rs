use image::{GenericImageView, Rgb};

use crate::utils::imgutils::color_variance;

use super::args_helper::args;

args! {
    #[derive(Copy, Clone)]
    Blandness {
        "Images with blandess less than or equal to this are filetered out (negative to disable)"
        blandness_threshold: f64 = -1.0;
    }
}

impl BlandnessArgs {
    pub fn blandness<I>(self, img: &I) -> f64
    where
        I: GenericImageView<Pixel = Rgb<u8>>,
    {
        color_variance(img)
    }

    pub fn is_value_bland(self, blandness: f64) -> bool {
        blandness <= self.blandness_threshold
    }

    pub fn is_bland<I>(self, img: &I) -> bool
    where
        I: GenericImageView<Pixel = Rgb<u8>>,
    {
        self.blandness_threshold >= 0.0 && self.is_value_bland(self.blandness(img))
    }
}
