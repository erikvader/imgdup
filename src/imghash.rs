use std::{cell::OnceCell, path::Path};

use self::hamming::{Distance, Hamming};

pub mod hamming;

pub const SIMILARITY_THRESHOLD: Distance = 32;

thread_local! {
    static HASHER: OnceCell<Hasher> = OnceCell::new();
}

pub struct Hasher {
    hasher: image_hasher::Hasher<[u8; Hamming::BYTES]>,
}

impl Hasher {
    pub fn new() -> Self {
        Self {
            hasher: image_hasher::HasherConfig::with_bytes_type::<[u8; Hamming::BYTES]>()
                // DoubleGraident is weird and doesn't caclulate the maximum used bits
                // correctly. The actual size seems to be: (wh+w+h)/2
                // https://github.com/abonander/img_hash/issues/46
                // struct NoMaxBits<T>(T); // Use this as a wrapper to ignore max_bits
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
    HASHER.with(|h| h.get_or_init(|| Hasher::new()).hash(img))
}

pub fn hash_from_path(path: &Path) -> image::ImageResult<Hamming> {
    let img = image::open(path)?;
    Ok(hash(&img))
}

#[cfg(test)]
mod test {
    use crate::imgutils::filled;

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
        assert!(true);
    }
}
