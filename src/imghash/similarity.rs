use clap::Args;

use crate::imghash::hamming::{Distance, Hamming};

pub const DEFAULT_SIMILARITY_THRESHOLD: Distance = 23;

#[derive(Args, Debug)]
pub struct SimiCli {
    /// Maximum distance for two images to be considered equal
    #[arg(long, default_value_t = DEFAULT_SIMILARITY_THRESHOLD)]
    similarity_threshold: Distance,
}

impl SimiCli {
    pub fn as_args(&self) -> SimiArgs {
        SimiArgs::default().similarity_threshold(self.similarity_threshold)
    }
}

pub struct SimiArgs {
    similarity_threshold: Distance,
}

impl Default for SimiArgs {
    fn default() -> Self {
        Self {
            similarity_threshold: DEFAULT_SIMILARITY_THRESHOLD,
        }
    }
}

impl SimiArgs {
    pub fn similarity_threshold(mut self, threshold: Distance) -> Self {
        self.similarity_threshold = threshold;
        self
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
