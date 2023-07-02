use std::cell::OnceCell;

use self::hamming::Hamming;

pub mod hamming;

thread_local! {
    static HASHER: OnceCell<Hasher> = OnceCell::new();
}

pub struct Hasher {
    hasher: image_hasher::Hasher,
}

impl Hasher {
    pub fn new() -> Self {
        Self {
            hasher: image_hasher::HasherConfig::new()
                .hash_alg(image_hasher::HashAlg::Blockhash)
                .to_hasher(),
        }
    }

    pub fn hash<I>(&self, img: &I) -> Hamming
    where
        I: image_hasher::Image,
    {
        let hash = self.hasher.hash_image(img);
        Hamming::from_slice(hash.as_bytes())
    }
}

pub fn hash<I>(img: &I) -> Hamming
where
    I: image_hasher::Image,
{
    HASHER.with(|h| h.get_or_init(|| Hasher::new()).hash(img))
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
        assert_ne!(black, white);
        assert!(black.distance_to(white) > 0);
    }

    #[test]
    fn empty() {
        let hash = Hasher::new().hash(&filled(0, 0, 0, 0, 0));
        println!("empty: {hash}");
        assert!(true);
    }
}
