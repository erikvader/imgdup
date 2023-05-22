use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use crate::heap::Ref;

#[derive(Clone, Copy)]
struct Hamming(pub u64);

type Distance = u32;
type Timestamp = u64;

impl Hamming {
    fn distance_to(self, other: Self) -> Distance {
        (self.0 ^ other.0).count_ones()
    }

    fn distance(a: u64, b: u64) -> Distance {
        Hamming(a).distance_to(Hamming(b))
    }
}

struct BKTree {
    head: Ref,
    sources: Ref,
}

enum Node {
    Tree {
        hash: Hamming,
        timestamp: Timestamp,
        source: Ref,
        source_idx: usize,
        children: HashMap<Distance, Ref>,
    },
    Source {
        next: Ref,
        paths: Vec<Option<PathBuf>>,
    },
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn hamming_distances() {
        assert_eq!(0, Hamming(0).distance_to(Hamming(0)));
        assert_eq!(0, Hamming(u64::MAX).distance_to(Hamming(u64::MAX)));
        assert_eq!(3, Hamming(0b101).distance_to(Hamming(0b010)));
    }
}
