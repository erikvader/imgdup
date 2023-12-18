use crate::utils::imgutils::{mask_blackness, Mask};

use super::args_helper::args;

args! {
    #[derive(Copy, Clone)]
    BlackMask {
        "Masks that are at least this many percent black are filtered out (negative to disable)"
        black_mask_threshold: f64 = 90.0;
    }
}

impl BlackMaskArgs {
    pub fn blackness(self, mask: &Mask) -> f64 {
        mask_blackness(mask)
    }

    pub fn is_value_too_black(self, blackness: f64) -> bool {
        blackness >= self.black_mask_threshold
    }

    pub fn is_too_black(self, mask: &Mask) -> bool {
        self.black_mask_threshold >= 0.0 && self.is_value_too_black(self.blackness(mask))
    }
}
