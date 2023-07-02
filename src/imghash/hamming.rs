pub type Distance = u32;
pub type Container = u64;

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
pub struct Hamming(pub Container);

impl Hamming {
    pub const BITS: u32 = Container::BITS;
    pub const BYTES: usize = std::mem::size_of::<Container>();
    pub const MIN_DIST: Distance = 0;
    pub const MAX_DIST: Distance = Hamming::BITS;

    pub fn from_slice(bytes: &[u8]) -> Self {
        assert_eq!(Hamming::BYTES, bytes.len());
        let array: [u8; Hamming::BYTES] = bytes
            .try_into()
            .expect("the slice is of the incorrect length");
        Self(Container::from_ne_bytes(array))
    }

    pub fn to_base64(self) -> String {
        base64::Engine::encode(
            &base64::prelude::BASE64_STANDARD_NO_PAD,
            self.0.to_ne_bytes(),
        )
    }

    pub fn distance_to(self, other: Self) -> Distance {
        (self.0 ^ other.0).count_ones()
    }

    pub fn distance(a: Container, b: Container) -> Distance {
        Hamming(a).distance_to(Hamming(b))
    }
}

impl std::fmt::Display for Hamming {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.to_base64().fmt(f)
    }
}

#[cfg(test)]
mod test {
    use rand::{distributions::Standard, prelude::Distribution, Rng};

    use super::*;

    impl Hamming {
        pub fn random_at_distance<R>(self, rng: &mut R, dist: Distance) -> Self
        where
            R: Rng + ?Sized,
        {
            assert!(dist >= Hamming::MIN_DIST && dist <= Hamming::MAX_DIST);

            let mut new_bits = self.0;
            for i in rand::seq::index::sample(
                rng,
                Hamming::BITS.try_into().unwrap(),
                dist.try_into().unwrap(),
            ) {
                let mask = 1 << i;
                new_bits ^= mask;
            }
            Hamming(new_bits)
        }

        pub fn random_within<R>(self, rng: &mut R, within: Distance) -> Self
        where
            R: Rng + ?Sized,
        {
            let dist = rng.gen_range(Hamming::MIN_DIST..=within);
            self.random_at_distance(rng, dist)
        }

        pub fn random_outside<R>(self, rng: &mut R, within: Distance) -> Self
        where
            R: Rng + ?Sized,
        {
            let dist = rng.gen_range((within + 1)..=Hamming::MAX_DIST);
            self.random_at_distance(rng, dist)
        }
    }

    impl Distribution<Hamming> for Standard {
        fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Hamming {
            Hamming(rng.gen())
        }
    }

    #[test]
    fn random_at_distance() {
        let h1 = Hamming(0b101010);
        let h2 = h1.random_at_distance(&mut rand::thread_rng(), 3);
        assert_eq!(3, h1.distance_to(h2));
    }

    #[test]
    fn hamming_distances() {
        assert_eq!(0, Hamming(0).distance_to(Hamming(0)));
        assert_eq!(
            0,
            Hamming(Container::MAX).distance_to(Hamming(Container::MAX))
        );
        assert_eq!(3, Hamming(0b101).distance_to(Hamming(0b010)));
        assert_eq!(
            Hamming(0b101).distance_to(Hamming(0b010)),
            Hamming(0b010).distance_to(Hamming(0b101))
        );
    }
}
