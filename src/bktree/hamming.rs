pub type Distance = u32;

#[derive(
    Clone,
    Copy,
    serde::Serialize,
    serde::Deserialize,
    Debug,
    PartialEq,
    Eq,
    Ord,
    PartialOrd,
)]
pub struct Hamming(pub u64);

impl Hamming {
    pub fn distance_to(self, other: Self) -> Distance {
        (self.0 ^ other.0).count_ones()
    }

    pub fn distance(a: u64, b: u64) -> Distance {
        Hamming(a).distance_to(Hamming(b))
    }
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
