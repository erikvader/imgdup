use imgdup::heap::{self, Heap, HeapBuilder, Ref, Result};
use rand::{rngs::SmallRng, Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use tempfile::{NamedTempFile, TempPath};

struct List {
    head: Ref,
}

#[derive(Serialize, Deserialize, Debug)]
struct Node {
    data: i32,
    next: Ref,
}

impl Node {
    fn new(data: i32) -> Self {
        Self {
            data,
            next: Ref::null(),
        }
    }
}

impl List {
    fn new() -> Self {
        Self { head: Ref::null() }
    }

    fn from_existing(head: Ref) -> Self {
        Self { head }
    }

    fn head(&self) -> Ref {
        self.head
    }

    fn add(&mut self, db: &mut Heap<Node>, data: i32) -> heap::Result<()> {
        if self.head.is_null() {
            self.head = db.allocate(Node::new(data))?;
            return Ok(());
        }

        let mut next = self.head;
        while let Some(node) = db.deref(next)? {
            if node.next.is_null() {
                break;
            }
            next = node.next;
        }

        let new_node = db.allocate_local(next, Node::new(data))?;
        db.deref_mut(next)?.expect("does exist").next = new_node;
        Ok(())
    }

    fn remove(&mut self, db: &mut Heap<Node>, data: i32) -> heap::Result<()> {
        let mut prev = Ref::null();
        let mut next = self.head;
        while let Some(node) = db.deref(next)? {
            if node.data == data {
                if prev.is_null() {
                    self.head = node.next;
                } else {
                    db.deref_mut(prev)?.expect("does exist").next = node.next;
                }
                db.remove(next)?;
                break;
            }
            prev = next;
            next = node.next;
        }
        Ok(())
    }

    fn to_vec(&self, db: &mut Heap<Node>) -> heap::Result<Vec<i32>> {
        let mut vec = Vec::new();
        let mut next = self.head;
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
    let r1 = db.allocate(Node::new(5))?;
    db.set_root(r1);
    db.close()?;

    let mut db = Heap::<Node>::new_from_file(&tmp_path)?;
    let r2 = db.root();
    assert_eq!(r1, r2);
    assert_eq!(Some(5), db.deref(r2)?.map(|l| l.data));
    db.close()?;

    Ok(())
}

#[test]
fn test_linked_list_add() -> Result<()> {
    let mut db = HeapBuilder::new().maximum_block_size(1).in_memory()?;
    let mut list = List::new();

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
    let mut db = HeapBuilder::new().maximum_block_size(1).in_memory()?;
    let mut list = List::new();

    list.add(&mut db, 1)?;
    list.add(&mut db, 2)?;
    list.remove(&mut db, 1)?;

    assert_eq!(1, db.count_refs()?);
    assert_eq!(vec![2], list.to_vec(&mut db)?);

    Ok(())
}

#[test]
fn test_linked_list_stress_no_blocks() -> Result<()> {
    let tmp_path = tmp_path();
    let mut db: Heap<Node> = HeapBuilder::new()
        .cache_capacity(20)
        .dirtyness_limit(5)
        .maximum_block_size(1)
        .from_file(&tmp_path)?;

    let (list, reference) = list_stress(&mut db)?;

    assert_eq!(reference.len(), db.count_refs()?);
    assert_eq!(reference, list.to_vec(&mut db)?);

    db.set_root(list.head());
    db.close()?;
    let mut db: Heap<Node> = Heap::new_from_file(&tmp_path)?;
    let list = List::from_existing(db.root());
    assert_eq!(reference.len(), db.count_refs()?);
    assert_eq!(reference, list.to_vec(&mut db)?);

    Ok(())
}

#[test]
fn test_linked_list_stress() -> Result<()> {
    let tmp_path = tmp_path();
    let mut db: Heap<Node> = HeapBuilder::new()
        .cache_capacity(20)
        .dirtyness_limit(5)
        .maximum_block_size(10)
        .from_file(&tmp_path)?;

    let (list, reference) = list_stress(&mut db)?;

    let refs_before = db.count_refs()?;
    assert_eq!(reference, list.to_vec(&mut db)?);

    db.set_root(list.head());
    db.close()?;
    let mut db: Heap<Node> = Heap::new_from_file(&tmp_path)?;
    let list = List::from_existing(db.root());
    assert_eq!(refs_before, db.count_refs()?);
    assert_eq!(reference, list.to_vec(&mut db)?);

    Ok(())
}

fn list_stress(db: &mut Heap<Node>) -> Result<(List, Vec<i32>)> {
    let seed: u64 = rand::random();
    println!("Using seed: {}", seed);
    let mut rng = SmallRng::seed_from_u64(seed);

    let mut list = List::new();
    let mut reference = Vec::<i32>::new();

    for i in 0..1_000 {
        if rng.gen_ratio(1, 3) {
            if !reference.is_empty() {
                let i = rng.gen_range(0..reference.len());
                let v = reference[i];

                remove_first(&mut reference, v);
                list.remove(db, v)?;
            }
        } else {
            let to_add: i32 = rng.gen();
            reference.push(to_add);
            list.add(db, to_add)?;
        }

        if i % 10 == 0 {
            db.checkpoint()?;
        }
    }

    Ok((list, reference))
}

#[test]
fn test_linked_list_crash() -> Result<()> {
    let tmp_path = tmp_path();
    let mut db: Heap<Node> = Heap::new_from_file(&tmp_path)?;

    let mut list = List::new();
    list.add(&mut db, 1)?;
    db.set_root(list.head());
    db.flush()?;

    list.add(&mut db, 2)?;
    assert_eq!(vec![1, 2], list.to_vec(&mut db)?);
    drop(db); // a panic or something caused it to drop before calling close

    let mut db: Heap<Node> = Heap::new_from_file(&tmp_path)?;
    let list = List::from_existing(db.root());
    assert_eq!(vec![1], list.to_vec(&mut db)?);

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
