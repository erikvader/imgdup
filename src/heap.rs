use std::{collections::HashMap, path::Path, io};

type Uuid = u64;

pub struct Heap<T> {
    refs: HashMap<Uuid, Data<T>>,
    max_size: usize,
    next_id: Uuid,
    root: Option<Ref>,
}

#[derive(Clone, Copy, Debug)]
enum DataState {
    Clean,
    Dirty,
    Remove,
}

struct Data<T> {
    state: DataState,
    data: T,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ref {
    id: Uuid,
}

impl<T> Heap<T> {
    pub fn new() -> Self {
        Self {
            refs: HashMap::new(),
            next_id: Uuid::min_value(),
            root: None,
            max_size: 2048,
        }
    }

    pub fn from_file(file: &Path) -> Self {
        todo!()
    }

    pub fn allocate(&mut self) -> Ref {
        let r = Ref::new(self.next_id);
        self.next_id += 1;
        r
    }

    pub fn root(&self) -> Option<Ref> {
        self.root
    }

    pub fn set_root(&mut self, root: Ref) {
        self.root = Some(root);
    }

    pub fn set(&mut self, r: Ref, data: T) {
        self.refs.insert(r.id, Data::introduce_new(data));
    }

    pub fn remove_entry(&mut self, r: Ref) {
        todo!()
    }

    pub fn has_value(&self, r: Ref) -> bool {
        self.deref(r).is_some()
    }

    pub fn deref(&self, r: Ref) -> Option<&T> {
        todo!()
    }

    pub fn deref_mut(&mut self, r: Ref) -> Option<&mut T> {
        todo!()
    }

    pub fn flush(&mut self) -> io::Result<()> {
        todo!()
    }
}

impl<T> Drop for Heap<T> {
    fn drop(&mut self) {
        self.flush().ok();
    }
}

impl Ref {
    fn new(id: Uuid) -> Self {
        Self {
            id,
        }
    }
}

impl<T> Data<T> {
    fn introduce_new(data: T) -> Self {
        Self {
            data,
            state: DataState::Dirty,
        }
    }

    fn from_file(data: T) -> Self {
        Self {
            data,
            state: DataState::Clean,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    struct List {
        data: (),
        child: Option<Ref>,
    }

    #[test]
    fn test() {
        let mut db = Heap::<List>::new();
        // let r = db.new_entry(List{data: (), child: None});
        // recur(&mut db, r);
    }

    fn recur(db: &mut Heap<List>, r: Ref) {
        let d = db.deref(r).unwrap();
        if let Some(l) = d.child {
            recur(db, l);
        }
    }
}
