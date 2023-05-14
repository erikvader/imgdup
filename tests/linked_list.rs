use imgdup::heap::{self, Heap, HeapBuilder, Ref, Result};
use rand::Rng;
use serde::{Deserialize, Serialize};
use tempfile::{NamedTempFile, TempPath};

struct List {
    node: Ref,
}

#[derive(Serialize, Deserialize, Debug)]
struct Node {
    data: i32,
    next: Ref,
}

impl Node {
    fn new(data: i32, next: Ref) -> Self {
        Self { data, next }
    }
}

impl List {
    fn new(node: Ref) -> Self {
        Self { node }
    }

    fn add(&mut self, db: &mut Heap<Node>, data: i32) -> heap::Result<()> {
        let mut next = self.node;
        while let Some(node) = db.deref(next)? {
            next = node.next;
        }

        let node_next = db.allocate();
        db.set(next, Node::new(data, node_next))?;
        Ok(())
    }

    fn remove(&mut self, db: &mut Heap<Node>, data: i32) -> heap::Result<()> {
        let mut prev = Ref::null();
        let mut next = self.node;
        while let Some(node) = db.deref(next)? {
            if node.data == data {
                if prev.is_null() {
                    self.node = node.next;
                } else {
                    db.deref_mut(prev)?.expect("does exist").next = node.next;
                }
                db.remove_entry(next)?;
                break;
            }
            prev = next;
            next = node.next;
        }
        Ok(())
    }

    fn to_vec(&self, db: &mut Heap<Node>) -> heap::Result<Vec<i32>> {
        let mut vec = Vec::new();
        let mut next = self.node;
        while let Some(node) = db.deref(next)? {
            vec.push(node.data);
            next = node.next;
        }
        Ok(vec)
    }
}

#[test]
fn test_write_to_file() -> Result<()> {
    let tmp_path = tmp_path();

    let mut db = Heap::<Node>::new_from_file(&tmp_path)?;
    let r1 = db.allocate();
    let next = db.allocate();
    db.set(r1, Node::new(5, next))?;
    db.set_root(r1);
    db.flush()?;
    drop(db);

    let mut db = Heap::<Node>::new_from_file(&tmp_path)?;
    let r2 = db.root();
    assert_eq!(r1, r2);
    assert_eq!(Some(5), db.deref(r2)?.map(|l| l.data));
    drop(db);

    Ok(())
}

#[test]
fn test_linked_list_add() -> Result<()> {
    let mut db = Heap::<Node>::new_in_memory()?;
    let mut list = List::new(db.allocate());

    let mut reference = Vec::new();
    for i in 0..10 {
        reference.push(i);
        list.add(&mut db, i)?;
    }

    assert_eq!(reference, list.to_vec(&mut db)?);
    assert_eq!(10, db.count_refs()?);

    Ok(())
}

#[test]
fn test_linked_list_remove() -> Result<()> {
    let mut db = Heap::<Node>::new_in_memory()?;
    let mut list = List::new(db.allocate());

    list.add(&mut db, 1)?;
    list.add(&mut db, 2)?;
    list.remove(&mut db, 1)?;

    assert_eq!(1, db.count_refs()?);
    assert_eq!(vec![2], list.to_vec(&mut db)?);

    Ok(())
}

#[test]
fn test_linked_list_stress() -> Result<()> {
    let tmp_path = tmp_path();
    let mut rng = <rand::rngs::SmallRng as rand::SeedableRng>::seed_from_u64(3);
    let mut db: Heap<Node> = HeapBuilder::new().with_max_size(10).from_file(&tmp_path)?;

    let mut list = List::new(db.allocate());
    let mut reference = Vec::<i32>::new();

    for _ in 0..1000 {
        if rng.gen_ratio(1, 3) {
            if !reference.is_empty() {
                let i = rng.gen_range(0..reference.len());
                let v = reference[i];

                remove_first(&mut reference, v);
                list.remove(&mut db, v)?;
            }
        } else {
            let to_add: i32 = rng.gen();
            reference.push(to_add);
            list.add(&mut db, to_add)?;
        }
    }

    assert_eq!(reference.len(), db.count_refs()?);
    assert_eq!(reference, list.to_vec(&mut db)?);

    Ok(())
}

fn remove_first(vec: &mut Vec<i32>, remove: i32) {
    if let Some(i) = vec.iter().position(|i| *i == remove) {
        vec.remove(i);
    }
}

fn tmp_path() -> TempPath {
    match option_env!("CARGO_TARGET_TMPDIR") {
        None => NamedTempFile::new(),
        Some(dir) => NamedTempFile::new_in(dir),
    }
    .expect("could not create temporary file")
    .into_temp_path()
}
