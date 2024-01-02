use crate::imghash::hamming::{Distance, Hamming};

use super::args_helper::args;

args! {
    #[derive(Copy, Clone)]
    Simi {
        "Maximum distance for two images to be considered equal"
        similarity_threshold: Distance = 23;
    }
}

impl SimiArgs {
    pub fn threshold(&self) -> Distance {
        self.similarity_threshold
    }

    pub fn is_within(&self, dist: Distance) -> bool {
        dist <= self.similarity_threshold
    }

    pub fn is_not_within(&self, dist: Distance) -> bool {
        !self.is_within(dist)
    }

    pub fn are_similar(&self, h1: Hamming, h2: Hamming) -> bool {
        self.is_within(h1.distance_to(h2))
    }

    pub fn are_dissimilar(&self, h1: Hamming, h2: Hamming) -> bool {
        !self.are_similar(h1, h2)
    }
}
