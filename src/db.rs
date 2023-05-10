use std::{collections::HashMap, path::Path, io};

type Uuid = u64;

pub struct DB<T> {
    refs: HashMap<Uuid, Data<T>>,
    next_id: Uuid,
    root: Ref,
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

impl<T> DB<T> {
    pub fn new(root_data: T) -> Self {
        let root_id = Uuid::min_value();
        Self {
            refs: vec![(root_id, Data::introduce_new(root_data))].into_iter().collect(),
            next_id: root_id + 1,
            root: Ref::new(root_id),
        }
    }

    pub fn from_file(file: &Path) -> Self {
        todo!()
    }

    pub fn new_entry(&mut self, data: T) -> Ref {
        let r = Ref::new(self.next_id);
        self.refs.insert(r.id, Data::introduce_new(data));
        self.next_id += 1;
        r
    }

    pub fn root(&self) -> Ref {
        self.root
    }

    pub fn remove_entry(&mut self, r: Ref) {
        assert!(r != self.root(), "There must always be a root (king)");
        todo!()
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

impl<T> Drop for DB<T> {
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
        let mut db = DB::<List>::new(List{data: (), child: None});
        let r = db.new_entry(List{data: (), child: None});
        recur(&mut db, r);
    }

    fn recur(db: &mut DB<List>, r: Ref) {
        let d = db.deref(r).unwrap();
        if let Some(l) = d.child {
            recur(db, l);
        }
    }
}
