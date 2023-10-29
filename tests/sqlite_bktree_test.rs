mod common;

use common::tmp_file;
use imgdup::bktree::sqlite::heap;
use imgdup::{bktree::sqlite::bktree::BKTree, imghash::hamming::Hamming};

#[test]
fn bktree_crash() -> heap::Result<()> {
    let tmp_path = tmp_file();

    let mut tree = BKTree::<()>::from_file(&tmp_path)?;
    tree.add(Hamming(69), ())?;
    tree.close()?;

    let res = std::panic::catch_unwind(|| -> heap::Result<()> {
        let mut tree = BKTree::<()>::from_file(&tmp_path)?;
        tree.for_each(|hash, ()| assert_eq!(Hamming(69), hash))?;

        let mut has_given = false;
        tree.add_all(std::iter::from_fn(|| {
            if !has_given {
                has_given = true;
                Some((Hamming(123), ()))
            } else {
                panic!("Oh no! I crashed")
            }
        }))?;

        Ok(())
    });

    assert!(res.is_err());
    let panic_msg = res.unwrap_err().downcast::<&str>();
    assert!(panic_msg.is_ok());
    assert_eq!("Oh no! I crashed", *panic_msg.unwrap());

    let mut tree = BKTree::<()>::from_file(&tmp_path)?;
    tree.for_each(|hash, ()| assert_eq!(Hamming(69), hash))?;
    tree.close()?;

    Ok(())
}
