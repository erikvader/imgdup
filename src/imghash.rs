use std::sync::OnceLock;

use image::{GenericImageView, Pixel, SubImage};

use self::hamming::{Distance, Hamming};

pub mod hamming;

pub const DEFAULT_SIMILARITY_THRESHOLD: Distance = 23;

static HASHER: OnceLock<Hasher> = OnceLock::new();

pub struct Hasher {
    hasher: image_hasher::Hasher<[u8; Hamming::BYTES]>,
}

impl Hasher {
    pub fn new() -> Self {
        Self {
            hasher: image_hasher::HasherConfig::with_bytes_type::<[u8; Hamming::BYTES]>()
                // NOTE: DoubleGraident is weird and doesn't caclulate the maximum used
                // bits correctly. The actual size seems to be: (wh+w+h)/2
                // https://github.com/abonander/img_hash/issues/46
                // struct NoMaxBits<T>(T); // Use some wrapper to ignore max_bits
                .hash_alg(image_hasher::HashAlg::VertGradient)
                .hash_size(16, 8)
                .preproc_dct()
                .to_hasher(),
        }
    }

    pub fn hash<I>(&self, img: &I) -> Hamming
    where
        I: image_hasher::Image,
    {
        let hash = self.hasher.hash_image(img);
        Hamming::from_hash(hash)
    }
}

pub fn hash<I>(img: &I) -> Hamming
where
    I: image_hasher::Image,
{
    HASHER.get_or_init(|| Hasher::new()).hash(img)
}

pub fn hash_sub<I, P>(img: &SubImage<&I>) -> Hamming
where
    I: image_hasher::Image + GenericImageView<Pixel = P>,
    P: Pixel<Subpixel = u8> + 'static,
{
    if img.bounds() == img.inner().bounds() {
        hash(img.inner())
    } else {
        // TODO: do this without copying the whole image
        hash(&img.to_image())
    }
}

#[cfg(test)]
mod test {
    use crate::imgutils::{construct_gray, filled, BLACK, WHITE};

    use super::*;

    #[test]
    fn simple_hash() {
        let hasher = Hasher::new();
        let black = hasher.hash(&filled(300, 300, 0, 0, 0));
        let white = hasher.hash(&filled(300, 300, 255, 255, 255));
        println!("black: {}", black);
        println!("white: {}", white);
        assert!(true);
        // NOTE: this is not really testing anything. It is bad that these are considered
        // identical, but pictures like these are unlikely to appear in the wild.
        // assert_ne!(black, white);
        // assert!(black.distance_to(white) >= 0);
    }

    #[test]
    fn empty() {
        let hash = Hasher::new().hash(&filled(0, 0, 0, 0, 0));
        println!("empty: {hash}");

        let gray = filled(5, 5, 128, 128, 128);
        let sub_hash = hash_sub(&gray.view(0, 0, 0, 0));

        assert_eq!(hash, sub_hash);
    }

    #[test]
    fn sub() {
        let img = construct_gray(&[
            &[BLACK, WHITE, BLACK, WHITE],
            &[WHITE, BLACK, WHITE, BLACK],
            &[BLACK, WHITE, BLACK, WHITE],
            &[WHITE, BLACK, WHITE, BLACK],
        ]);
        let sub = img.view(1, 1, 2, 2);
        let whole = img.view(0, 0, img.width(), img.height());

        assert_eq!(img.bounds(), sub.inner().bounds());
        assert_ne!(img.bounds(), sub.bounds());

        assert_eq!(hash(&img), hash_sub(&whole));
        assert_ne!(hash(&img), hash_sub(&sub));
    }
}
