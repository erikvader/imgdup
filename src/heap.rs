use std::path::Path;

use indexmap::IndexMap;
use rand::Rng;
use rusqlite::Connection;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use self::sql::Sql;

mod sql;

type Uuid = i64;
pub type Result<T> = std::result::Result<T, HeapError>;

#[derive(thiserror::Error, Debug)]
pub enum HeapError {
    #[error("SQlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Bincode error: {0}")]
    Bincode(#[from] bincode::Error),
    #[error("IO Error: {0}")]
    IoError(#[from] std::io::Error),
}

pub struct Heap<T>
where
    T: Serialize + DeserializeOwned,
{
    refs: IndexMap<Uuid, Data<T>>,
    max_size: usize,
    next_id: Uuid,
    root: Ref,
    sql: Sql,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DataState {
    Clean,
    Dirty,
    Remove,
}

struct Data<T> {
    state: DataState,
    data: T,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ref {
    id: Uuid,
}

pub struct HeapBuilder {
    max_size: usize,
}

impl HeapBuilder {
    pub fn new() -> Self {
        Self { max_size: 2048 }
    }

    pub fn with_max_size(mut self, max_size: usize) -> Self {
        assert!(max_size >= 1);
        self.max_size = max_size;
        self
    }

    pub fn in_memory<T>(self) -> Result<Heap<T>>
    where
        T: Serialize + DeserializeOwned,
    {
        Heap::new(Sql::new_in_memory()?, self.max_size)
    }

    pub fn from_file<T>(self, file: impl AsRef<Path>) -> Result<Heap<T>>
    where
        T: Serialize + DeserializeOwned,
    {
        Heap::new(Sql::new_from_file(file)?, self.max_size)
    }
}

impl<T> Heap<T>
where
    T: Serialize + DeserializeOwned,
{
    fn new(mut sql: Sql, max_size: usize) -> Result<Self> {
        let trans = sql.transaction()?;
        let next_id = trans
            .get_meta::<Uuid>("next_id")?
            .unwrap_or(Uuid::min_value());
        let root = trans.get_meta::<Ref>("root")?.unwrap_or(Ref::null());
        drop(trans);

        Ok(Self {
            refs: IndexMap::with_capacity(max_size),
            next_id,
            root,
            max_size,
            sql,
        })
    }

    pub fn new_in_memory() -> Result<Self> {
        HeapBuilder::new().in_memory()
    }

    pub fn new_from_file(file: impl AsRef<Path>) -> Result<Self> {
        HeapBuilder::new().from_file(file)
    }

    pub fn allocate(&mut self) -> Ref {
        let r = Ref::new(self.next_id);
        self.next_id += 1;
        r
    }

    pub fn root(&self) -> Ref {
        self.root
    }

    pub fn set_root(&mut self, root: Ref) {
        self.root = root;
    }

    pub fn clear_root(&mut self) {
        self.root = Ref::null();
    }

    pub fn count_refs(&mut self) -> Result<usize> {
        self.flush()?;
        self.sql.transaction()?.count_refs()
    }

    pub fn set(&mut self, r: Ref, data: T) -> Result<()> {
        assert!(!r.is_null());
        self.handle_overflow()?;
        self.refs.insert(r.id, Data::new_dirty(data));
        Ok(())
    }

    pub fn remove_entry(&mut self, r: Ref) -> Result<()> {
        self.load(r)?;
        if let Some(data) = self.refs.get_mut(&r.id) {
            data.state = DataState::Remove;
        }
        Ok(())
    }

    pub fn has_value(&mut self, r: Ref) -> Result<bool> {
        Ok(self.deref(r)?.is_some())
    }

    pub fn deref(&mut self, r: Ref) -> Result<Option<&T>> {
        self.load(r)?;
        Ok(self.refs.get(&r.id).map(|data| &data.data))
    }

    pub fn deref_mut(&mut self, r: Ref) -> Result<Option<&mut T>> {
        self.load(r)?;
        Ok(self.refs.get_mut(&r.id).map(|data| {
            data.state = DataState::Dirty;
            &mut data.data
        }))
    }

    fn load(&mut self, r: Ref) -> Result<()> {
        if !r.is_null() && !self.refs.contains_key(&r.id) {
            let trans = self.sql.transaction()?;
            if let Some(val) = trans.get_refs::<T>(r.id)? {
                drop(trans);
                self.handle_overflow()?;
                self.refs.insert(r.id, Data::new_clean(val));
            }
        }
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        let trans = self.sql.transaction()?;
        trans.put_meta("next_id", self.next_id)?;
        trans.put_meta("root", self.root)?;

        for (r, data) in self.refs.iter() {
            match data.state {
                DataState::Clean => (),
                DataState::Dirty => {
                    trans.put_refs(*r, &data.data)?;
                }
                DataState::Remove => {
                    trans.remove_refs(*r)?;
                }
            }
        }

        trans.commit()?;

        self.refs.retain(|_, data| {
            if data.state == DataState::Dirty {
                data.state = DataState::Clean;
            }
            data.state != DataState::Remove
        });

        Ok(())
    }

    /// Clears space in the cache to make sure that at least one new element can be added
    /// without becoming bigger than the maximum size.
    fn handle_overflow(&mut self) -> Result<()> {
        assert!(self.max_size >= 1);
        while self.refs.len() >= self.max_size {
            let r = 0; // rand::thread_rng().gen_range(0..self.refs.len());

            if self
                .refs
                .get_index(r)
                .expect("the map is not empty")
                .1
                .state
                != DataState::Clean
            {
                self.flush()?;
                continue;
            }

            let (_, data) = self
                .refs
                .swap_remove_index(r)
                .expect("the map is not empty");
            assert!(data.state == DataState::Clean);
        }
        Ok(())
    }
}

impl<T> Drop for Heap<T>
where
    T: Serialize + DeserializeOwned,
{
    fn drop(&mut self) {
        self.flush().ok();
    }
}

impl Ref {
    const fn new(id: Uuid) -> Self {
        Self { id }
    }

    pub const fn null() -> Self {
        Self::new(Uuid::max_value())
    }

    pub fn is_null(&self) -> bool {
        self == &Self::null()
    }
}

impl<T> Data<T> {
    fn new_dirty(data: T) -> Self {
        Self {
            data,
            state: DataState::Dirty,
        }
    }

    fn new_clean(data: T) -> Self {
        Self {
            data,
            state: DataState::Clean,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    impl<T> Heap<T>
    where
        T: Serialize + DeserializeOwned,
    {
        pub fn reset(&mut self) -> Result<()> {
            self.flush()?;
            self.refs.clear();
            Ok(())
        }

        fn state_of(&self, r: Ref) -> Option<DataState> {
            self.refs.get(&r.id).map(|d| d.state)
        }
    }

    #[test]
    fn test_insert() -> Result<()> {
        let mut db = Heap::<i32>::new_in_memory()?;
        let r = db.allocate();
        db.set(r, 5)?;
        assert_eq!(Some(&5), db.deref(r)?);
        assert_eq!(Some(DataState::Dirty), db.state_of(r));

        db.reset()?;
        assert_eq!(None, db.state_of(r));
        assert_eq!(Some(&5), db.deref(r)?);
        assert_eq!(Some(DataState::Clean), db.state_of(r));

        assert_eq!(Some(&mut 5), db.deref_mut(r)?);
        assert_eq!(Some(DataState::Dirty), db.state_of(r));

        assert_eq!(Uuid::min_value() + 1, db.next_id);
        Ok(())
    }
}
