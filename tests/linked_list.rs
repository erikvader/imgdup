use imgdup::heap::{Heap, Ref, Result};
use serde::{Deserialize, Serialize};
use tempfile::{NamedTempFile, TempPath};

#[derive(Serialize, Deserialize, Debug)]
struct List {
    data: i32,
    next: Option<Ref>,
}

impl List {
    fn new(data: i32) -> Self {
        Self { data, next: None }
    }
}

#[test]
fn test_write_to_file() -> Result<()> {
    let tmp_path = tmp_path();

    let mut db = Heap::<List>::new_from_file(&tmp_path)?;
    let r1 = db.allocate();
    db.set(r1, List::new(5))?;
    db.set_root(r1);
    db.flush()?;
    drop(db);

    let mut db = Heap::<List>::new_from_file(&tmp_path)?;
    let r2 = db.root();
    assert!(r2.is_some());
    let r2 = r2.unwrap();
    assert_eq!(r1, r2);
    assert_eq!(Some(5), db.deref(r2)?.map(|l| l.data));
    drop(db);

    Ok(())
}

fn tmp_path() -> TempPath {
    match option_env!("CARGO_TARGET_TMPDIR") {
        None => NamedTempFile::new(),
        Some(dir) => NamedTempFile::new_in(dir),
    }
    .expect("could not create temporary file")
    .into_temp_path()
}
